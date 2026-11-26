//! The `php-imports` extractor: the class `use` items and the `require` and `include` of PHP files, read as text with no
//! `php` or `composer` run (ADR 0002), paired with the tracked files they name (spec #647). A class resolves through
//! Composer's autoload rules, read from the tracked `composer.json` files (`composer.rs`).

mod composer;
mod reader;

pub(crate) use composer::Autoload;

use reader::Reading;

use super::{Extractor, Output, Pair, Repo, Unsure};

pub(crate) struct PhpImports;

impl Extractor for PhpImports {
    fn name(&self) -> &'static str {
        "php-imports"
    }

    fn reads(&self, file: &str) -> bool {
        file.ends_with(".php") && !file.ends_with(".blade.php")
    }

    /// A file under a folder named `tests` or `test`, one only a `classmap` reaches, or one the reader cannot lex. The
    /// paths are judged first, so a file they skip is never read.
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        in_test_folder(file)
            || repo.autoload().classmap_only(file)
            || repo
                .read(file)
                .is_none_or(|text| Reading::read(file, &text).is_none())
    }

    fn extract(&self, repo: &Repo) -> Output {
        let mut found = Output {
            problems: repo.autoload().problems.clone(),
            ..Output::default()
        };
        for &file in repo.files() {
            if !self.reads(file) || repo.is_ignored(file) || self.skips(repo, file) {
                continue;
            }
            let Some(reading) = repo.read(file).and_then(|text| Reading::read(file, &text)) else {
                continue;
            };
            for class in reading.uses {
                match repo.autoload().resolve(repo, &class.name).as_slice() {
                    [] => {}
                    [to] => found.pairs.push(Pair::at(file, to, class.line)),
                    candidates => found.unsure.push(Unsure {
                        file: file.to_string(),
                        name: class.name,
                        reason: "two composer files map it".into(),
                        candidates: candidates.to_vec(),
                    }),
                }
            }
            for require in reading.requires {
                found
                    .pairs
                    .push(Pair::at(file, &require.path, require.line));
            }
        }
        found
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Fixture, MissingConfig};
        Fixture {
            support: vec![(
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#.to_string(),
            )],
            source: |name| format!("src/{name}.php"),
            importing: |_, to| {
                let class = to.strip_prefix("src/").unwrap_or(to);
                let class = class.strip_suffix(".php").unwrap_or(class);
                format!(
                    "<?php\n\nnamespace App;\n\nuse App\\{};\n",
                    class.replace('/', "\\")
                )
            },
            plain: |_| "<?php\n".to_string(),
            missing_config: MissingConfig::Fallback {
                file: "composer.json",
            },
            malformed_config: Some("{ not json"),
            test_file: Some(|name| format!("tests/{name}Test.php")),
            skip: vec![],
        }
    }
}

/// Whether a folder in the path of `file` is named `tests` or `test`, in any case and at any depth: Symfony keeps its
/// tests in `src/**/Tests/`. A file named like a test, `CreatesMatchingTest.php`, is production.
fn in_test_folder(file: &str) -> bool {
    let (folders, _) = file.rsplit_once('/').unwrap_or(("", file));
    folders
        .split('/')
        .any(|folder| folder.eq_ignore_ascii_case("tests") || folder.eq_ignore_ascii_case("test"))
}
