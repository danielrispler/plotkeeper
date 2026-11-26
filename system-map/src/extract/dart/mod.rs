//! The `dart-imports` extractor: the imports, exports and parts of Dart files, read as text with no `dart` run (ADR
//! 0002), and paired with the tracked files they name (spec #621). `package:` URIs give no pair yet (#628).

mod reader;

use super::{Extractor, Output, Pair, Repo};
use reader::{Directive, Kind};

pub(crate) struct DartImports;

impl Extractor for DartImports {
    fn name(&self) -> &'static str {
        "dart-imports"
    }

    fn reads(&self, file: &str) -> bool {
        file.ends_with(".dart")
    }

    /// A file under a folder named `test`, `integration_test` or `test_driver` below its package's root, or one whose
    /// header cannot be lexed.
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        directives(repo, file).is_none()
    }

    fn extract(&self, repo: &Repo) -> Output {
        let mut pairs = Vec::new();
        for file in repo.files() {
            if !self.reads(file) {
                continue;
            }
            for directive in directives(repo, file).unwrap_or_default() {
                for uri in &directive.uris {
                    let Some(to) = relative(file, uri) else {
                        continue;
                    };
                    let mut pair = Pair::at(file, &to, directive.line);
                    pair.declaration = directive.kind == Kind::Part;
                    pairs.push(pair);
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
            support: vec![("pubspec.yaml", "name: kit\n".into())],
            source: |name| format!("lib/{name}.dart"),
            importing: |from, to| {
                if from.starts_with("lib/") {
                    format!("import '{}';\n", to.trim_start_matches("lib/"))
                } else {
                    format!("import '../{to}';\n")
                }
            },
            plain: |_| "// Imports nothing.\n".into(),
            missing_config: MissingConfig::Fallback {
                file: "pubspec.yaml",
            },
            malformed_config: None,
            test_file: Some(|name| format!("test/{name}_test.dart")),
            skip: vec![(
                Case::MalformedConfig,
                "a pubspec with no name is read by the `package:` ticket (#628)",
            )],
        }
    }
}

/// The folder holding `file`, as a prefix: empty for the root, else ending in `/`.
fn folder_of(file: &str) -> &str {
    file.rfind('/').map_or("", |at| &file[..=at])
}

/// The folder of `file`'s package: the nearest tracked `pubspec.yaml` at or above it, any pubspec, else the root.
fn package_root<'f>(repo: &Repo, file: &'f str) -> &'f str {
    let mut folder = folder_of(file);
    loop {
        if folder.is_empty()
            || repo
                .files()
                .contains(format!("{folder}pubspec.yaml").as_str())
        {
            return folder;
        }
        folder = folder_of(&folder[..folder.len() - 1]);
    }
}

/// Whether a folder of `file` below its package's root is one Dart keeps its tests in.
fn in_test_folder(repo: &Repo, file: &str) -> bool {
    let below = &file[package_root(repo, file).len()..];
    let mut folders: Vec<&str> = below.split('/').collect();
    folders.pop();
    folders
        .iter()
        .any(|f| matches!(*f, "test" | "integration_test" | "test_driver"))
}

/// The directives of `file`'s header, or none for a file the extractor skips: one in a test folder, one the repository
/// cannot read, or one whose header cannot be lexed.
fn directives(repo: &Repo, file: &str) -> Option<Vec<Directive>> {
    if in_test_folder(repo, file) {
        return None;
    }
    reader::read(&repo.read(file)?)
}

/// The tracked path a relative `uri` names from `file`'s folder, with `.` and `..` removed; none for a URI with a
/// scheme, an absolute path, an empty one, or a path that climbs above the root.
fn relative(file: &str, uri: &str) -> Option<String> {
    let scheme = uri.find(':').is_some_and(|at| {
        let name = &uri[..at];
        name.starts_with(|c: char| c.is_ascii_alphabetic())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if scheme || uri.is_empty() || uri.starts_with('/') {
        return None;
    }
    let mut segments: Vec<&str> = folder_of(file)
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    for segment in uri.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(segment),
        }
    }
    Some(segments.join("/"))
}
