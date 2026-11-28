//! The `c-includes` extractor: the `#include` directives of C and C++ files, read as text with no compiler, `cmake` or
//! `make` run (ADR 0002), each resolved to a tracked file and paired with it, or left Unsure (spec #806, "Resolution,
//! per include"; ADR 0004).

mod lexer;
mod resolve;
mod system_headers;

use std::collections::BTreeMap;

use super::{Extractor, Output, Pair, Repo, Unsure, folders};

use lexer::includes;
use resolve::Settled;

pub(crate) struct CIncludes;

/// The extensions the reader reads, in any case: the thirteen the research scanned (spec #806, "What it reads").
const EXTENSIONS: [&str; 13] = [
    "c", "h", "cc", "cpp", "cxx", "c++", "hpp", "hh", "hxx", "h++", "inl", "ipp", "tcc",
];

impl Extractor for CIncludes {
    fn name(&self) -> &'static str {
        "c-includes"
    }

    fn reads(&self, file: &str) -> bool {
        let name = file.rsplit('/').next().unwrap_or(file);
        name.rsplit_once('.')
            .is_some_and(|(_, ext)| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
    }

    fn skips(&self, _repo: &Repo, file: &str) -> bool {
        is_test_file(file)
    }

    fn extract(&self, repo: &Repo) -> Output {
        // Every include is settled as far as the path-ending match first, so that the folder evidence is counted over
        // the whole repository before any tie is settled.
        let index = resolve::Index::new(repo);
        let mut evidence = BTreeMap::new();
        let mut settled = Vec::new();
        for &file in repo.files() {
            if repo.is_ignored(file) || !self.reads(file) || is_test_file(file) {
                continue;
            }
            let Some(text) = repo.read(file) else {
                continue;
            };
            for include in includes(&text) {
                let Some(path) = resolve::clean(&include.path) else {
                    continue;
                };
                let found = resolve::settle(repo, &index, file, &include, &path, &mut evidence);
                settled.push((file, include, path, found));
            }
        }
        let mut output = Output::default();
        for (file, include, path, found) in settled {
            let (reason, candidates) = match found {
                Settled::Nothing => continue,
                Settled::File(to) => {
                    output.pairs.push(Pair::at(file, &to, include.line));
                    continue;
                }
                Settled::Tie(candidates) => {
                    if let Some(to) = resolve::tie_break(&candidates, &path, &evidence) {
                        output.pairs.push(Pair::at(file, &to, include.line));
                        continue;
                    }
                    ("several files match it", candidates)
                }
                Settled::IgnoredCopy(candidates) => ("an ignored copy matches it", candidates),
            };
            output.unsure.push(Unsure {
                file: file.to_string(),
                name: include.path,
                reason: reason.to_string(),
                candidates: candidates.iter().map(ToString::to_string).collect(),
            });
        }
        output
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Case, Fixture, MissingConfig};
        Fixture {
            support: Vec::new(),
            source: |name| format!("src/{name}.c"),
            importing: |_, to| format!("#include \"{}\"\n", to.rsplit('/').next().unwrap_or(to)),
            plain: |_| "int x;\n".to_string(),
            missing_config: MissingConfig::None,
            malformed_config: None,
            test_file: Some(|name| format!("tests/{name}_test.c")),
            skip: vec![(Case::MalformedConfig, "c-includes has no config file")],
        }
    }
}

/// Whether `file` is a test file (spec #806, "Skipped files"): in a folder named `test`, `tests`, `testing`, `unittest`
/// or `fuzz` in any case, or named `*_test.*`, `*_tests.*` or `test_*.*`. The scan's own `is_test` is another rule.
fn is_test_file(file: &str) -> bool {
    let (folders, name) = file.rsplit_once('/').unwrap_or(("", file));
    folders.split('/').any(|folder| {
        ["test", "tests", "testing", "unittest", "fuzz"]
            .iter()
            .any(|t| folder.eq_ignore_ascii_case(t))
    }) || name.contains("_test.")
        || name.contains("_tests.")
        || name
            .strip_prefix("test_")
            .is_some_and(|rest| rest.contains('.'))
}
