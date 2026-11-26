//! The `php-imports` extractor: the `require` and `include` of PHP files, read as text with no `php` or `composer` run
//! (ADR 0002), paired with the tracked files they name (spec #647). Class `use` items are read but not yet resolved; the
//! next ticket adds Composer's autoload walk.

mod reader;

use reader::Reading;

use super::{Extractor, Output, Pair, Repo};

pub(crate) struct PhpImports;

impl Extractor for PhpImports {
    fn name(&self) -> &'static str {
        "php-imports"
    }

    fn reads(&self, file: &str) -> bool {
        file.ends_with(".php") && !file.ends_with(".blade.php")
    }

    /// A file under a folder named `tests` or `test`, or one the reader cannot lex. The path is judged first, so a file
    /// in a test folder is never read.
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        in_test_folder(file)
            || repo
                .read(file)
                .is_none_or(|text| Reading::read(file, &text).is_none())
    }

    fn extract(&self, repo: &Repo) -> Output {
        let mut found = Output::default();
        for &file in repo.files() {
            if !self.reads(file) || repo.is_ignored(file) || self.skips(repo, file) {
                continue;
            }
            let Some(reading) = repo.read(file).and_then(|text| Reading::read(file, &text)) else {
                continue;
            };
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
        use super::contract::{Case, Fixture, MissingConfig};
        Fixture {
            support: vec![("composer.json", "{}".to_string())],
            source: |name| format!("src/{name}.php"),
            importing: |from, to| format!("<?php\n\nrequire {};\n", relative(from, to)),
            plain: |_| "<?php\n".to_string(),
            missing_config: MissingConfig::Fallback {
                file: "composer.json",
            },
            malformed_config: None,
            test_file: Some(|name| format!("tests/{name}Test.php")),
            skip: vec![(
                Case::MalformedConfig,
                "php-imports reads no composer.json until the next ticket (#653)",
            )],
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

/// The static path expression that names `to` from the file `from`: `__DIR__ . '/to.php'` beside it, and one that climbs
/// out of its folder otherwise.
#[cfg(test)]
fn relative(from: &str, to: &str) -> String {
    let folder = from.rsplit_once('/').map_or("", |(folder, _)| folder);
    let (to_folder, to_name) = to.rsplit_once('/').unwrap_or(("", to));
    if folder == to_folder {
        return format!("__DIR__ . '/{to_name}'");
    }
    let up = "../".repeat(folder.split('/').filter(|s| !s.is_empty()).count());
    format!("__DIR__ . '/{up}{to}'")
}
