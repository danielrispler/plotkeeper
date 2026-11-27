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
        skips_path(repo, file) || reading(repo, file).is_none()
    }

    fn extract(&self, repo: &Repo) -> Output {
        let mut found = Output {
            problems: repo.autoload().problems.clone(),
            ..Output::default()
        };
        for &file in repo.files() {
            if !self.reads(file) || repo.is_ignored(file) || skips_path(repo, file) {
                continue;
            }
            // A file the reader cannot lex is skipped.
            let Some(reading) = reading(repo, file) else {
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

/// Whether `file` is skipped for its path alone: under a test folder, or reached only by a `classmap`.
fn skips_path(repo: &Repo, file: &str) -> bool {
    in_test_folder(file) || repo.autoload().classmap_only(file)
}

/// What the reader takes from `file`; none when it cannot be read or lexed.
fn reading(repo: &Repo, file: &str) -> Option<Reading> {
    repo.read(file).and_then(|text| Reading::read(file, &text))
}

/// Whether a folder in the path of `file` is named `tests` or `test`, in any case and at any depth: Symfony keeps its
/// tests in `src/**/Tests/`. A file named like a test, `CreatesMatchingTest.php`, is production.
fn in_test_folder(file: &str) -> bool {
    let (folders, _) = file.rsplit_once('/').unwrap_or(("", file));
    folders
        .split('/')
        .any(|folder| folder.eq_ignore_ascii_case("tests") || folder.eq_ignore_ascii_case("test"))
}

/// `text`, a path under the repository root that starts with `/` (or is empty), as a path from the root with no `.`,
/// `..` or empty segment. None when it does not start there, or climbs above the root.
fn clean(text: &str) -> Option<String> {
    let rest = text
        .strip_prefix('/')
        .or_else(|| text.is_empty().then_some(text))?;
    crate::extract::folders::join("", rest)
}
