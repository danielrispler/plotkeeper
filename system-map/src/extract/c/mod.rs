//! The `c-includes` extractor: the `#include` directives of C and C++ files, read as text with no compiler, `cmake` or
//! `make` run (ADR 0002), each resolved to a tracked file and paired with it (spec #806). This first slice resolves a
//! quoted include of a file next to the including file; the later steps (spec #806, "Resolution, per include") go beside
//! it.

mod lexer;

use super::{Extractor, Output, Pair, Repo, folders};

use lexer::{Include, includes};

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
        let mut pairs = Vec::new();
        for &file in repo.files() {
            if repo.is_ignored(file) || !self.reads(file) || is_test_file(file) {
                continue;
            }
            let Some(text) = repo.read(file) else {
                continue;
            };
            for include in includes(&text) {
                if let Some(to) = resolve(repo, file, &include) {
                    pairs.push(Pair::at(file, &to, include.line));
                }
            }
        }
        Output {
            pairs,
            ..Output::default()
        }
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

/// The include's path as written, less its `.` segments; none for an empty path, an absolute one or one with a
/// backslash (spec #806, "Paths").
fn clean(path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains('\\') {
        return None;
    }
    let kept: Vec<&str> = path.split('/').filter(|s| *s != ".").collect();
    let cleaned = kept.join("/");
    (!cleaned.is_empty()).then_some(cleaned)
}

/// The tracked file `include`, in `from`, names, when it is a file a pair may end in.
fn resolve(repo: &Repo, from: &str, include: &Include) -> Option<String> {
    let path = clean(&include.path)?;
    if let Some(answer) = next_to_the_file(repo, from, include, &path) {
        return answer;
    }
    // The later steps (system names, the path-ending match, the folder tie-break) go here, in modules beside this one.
    None
}

/// Step 1 (spec #806, "Resolution, per include"): a quoted include of a tracked file next to the including file is that
/// file, as the compiler's first search step has it. The outer none means no such file, so the search goes on; the inner
/// one means the file is ignored or skipped, so the search stops with no pair.
fn next_to_the_file(
    repo: &Repo,
    from: &str,
    include: &Include,
    path: &str,
) -> Option<Option<String>> {
    if !include.quoted {
        return None;
    }
    let near = folders::join(folders::folder_of(from), path)?;
    repo.files()
        .contains(near.as_str())
        .then(|| (!repo.is_ignored(&near) && !is_test_file(&near)).then_some(near))
}
