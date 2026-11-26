//! The `dart-imports` extractor: the imports, exports and parts of Dart files, read as text with no `dart` run (ADR
//! 0002), and paired with the tracked files they name (spec #621). A `package:` URI names `lib/` of the tracked
//! `pubspec.yaml` that declares the package (the package index, `Packages`).

mod reader;

use std::collections::BTreeMap;

use super::{
    Extractor, Output, Pair, Repo, Unsure,
    folders::{folder_of, parent},
    names::{Lookup, Names},
};
use reader::{Directive, Kind};

pub(crate) struct DartImports;

impl Extractor for DartImports {
    fn name(&self) -> &'static str {
        "dart-imports"
    }

    fn reads(&self, file: &str) -> bool {
        file.ends_with(".dart")
    }

    /// A test file (`is_test_file`: one of a fixture's package, or under a folder named `test`, `integration_test` or
    /// `test_driver` below its package's root), one that cannot be read, or one whose header cannot be lexed.
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        directives(repo, file).is_none()
    }

    fn extract(&self, repo: &Repo) -> Output {
        let (packages, problems) = Packages::read(repo);
        let mut found = Output {
            problems,
            ..Output::default()
        };
        for file in repo.files() {
            if !self.reads(file) {
                continue;
            }
            for directive in directives(repo, file).unwrap_or_default() {
                for uri in &directive.uris {
                    let to = match resolve(repo, &packages, file, uri) {
                        Resolved::Nothing => continue,
                        Resolved::File(to) => to,
                        Resolved::Unsure { name, candidates } => {
                            found.unsure.push(Unsure {
                                file: file.to_string(),
                                name,
                                reason: "declared twice".into(),
                                candidates,
                            });
                            continue;
                        }
                    };
                    let mut pair = Pair::at(file, &to, directive.line);
                    pair.declaration = directive.kind == Kind::Part;
                    found.pairs.push(pair);
                }
            }
        }
        found
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Fixture, MissingConfig};
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
            malformed_config: Some("version: 1.0.0\n"),
            test_file: Some(|name| format!("test/{name}_test.dart")),
            skip: Vec::new(),
        }
    }
}

/// The folder of the nearest tracked `pubspec.yaml` at or above `folder`, any pubspec; none when there is none.
fn nearest_package<'f>(repo: &Repo, mut folder: &'f str) -> Option<&'f str> {
    loop {
        if repo
            .files()
            .contains(format!("{folder}pubspec.yaml").as_str())
        {
            return Some(folder);
        }
        folder = parent(folder)?;
    }
}

/// Whether `name` is a folder name Dart keeps its tests in.
fn is_test_folder(name: &str) -> bool {
    matches!(name, "test" | "integration_test" | "test_driver")
}

/// Whether `file` is a test file: its own package, the nearest tracked `pubspec.yaml` at or above it, is a fixture, or
/// a folder of its path below that package's root (else the repository's root) is a test folder.
fn is_test_file(repo: &Repo, file: &str) -> bool {
    let root = nearest_package(repo, folder_of(file));
    if root.is_some_and(|folder| is_fixture(repo, folder)) {
        return true;
    }
    let mut folders: Vec<&str> = file[root.unwrap_or("").len()..].split('/').collect();
    folders.pop();
    folders.into_iter().any(is_test_folder)
}

/// The directives of `file`'s header, or none for a file the extractor skips: a test file, one the repository
/// cannot read, or one whose header cannot be lexed.
fn directives(repo: &Repo, file: &str) -> Option<Vec<Directive>> {
    if is_test_file(repo, file) {
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
    joined(folder_of(file), uri)
}

/// `path` joined to `folder`, with `.` and `..` removed; none when it climbs above the root.
fn joined(folder: &str, path: &str) -> Option<String> {
    let mut segments: Vec<&str> = folder.split('/').filter(|s| !s.is_empty()).collect();
    for segment in path.split('/') {
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

/// The packages `package:` URIs can name: each indexed `pubspec.yaml`'s package (spec #621, "The package index").
struct Packages {
    /// Each indexed package's name, with its folder.
    names: Names<String>,
    /// Each indexed package's folder, with its name.
    by_folder: BTreeMap<String, String>,
}

impl Packages {
    fn read(repo: &Repo) -> (Packages, Vec<String>) {
        let mut packages = Packages {
            names: Names::new(),
            by_folder: BTreeMap::new(),
        };
        let mut problems = Vec::new();
        for &file in repo.files() {
            if file.rsplit('/').next() != Some("pubspec.yaml") {
                continue;
            }
            let folder = folder_of(file);
            // A fixture is not a package, and never a problem.
            if is_fixture(repo, folder) {
                continue;
            }
            // The package's `lib/` files, and whether the map leaves one unignored. A package is ignored by these, never
            // by its `pubspec.yaml` alone: the map may ignore a config file, and the package's imports still name it.
            let lib = format!("{folder}lib/");
            let (mut tracked, mut live) = (false, false);
            for lib_file in repo
                .files()
                .range(lib.as_str()..)
                .take_while(|f| f.starts_with(&lib))
            {
                tracked = true;
                live |= !repo.is_ignored(lib_file);
            }
            match repo.read(file).as_deref().and_then(package_name) {
                Some(name) if live => {
                    packages.names.register(&name, folder.to_string());
                    packages.by_folder.insert(folder.to_string(), name);
                }
                Some(_) => {}
                None if repo.is_ignored(file) || (tracked && !live) => {}
                None => problems.push(format!(
                    "{file}: no readable name; a pubspec.yaml names its package with a line `name: <package>` that \
                     starts at the beginning of the line"
                )),
            }
        }
        (packages, problems)
    }
}

/// Whether the `pubspec.yaml` in `folder` is a fixture, not a package: a folder strictly between its enclosing package's
/// root (the nearest tracked pubspec above it, else the repository's root) and its own folder is a test folder. Its own
/// folder's name never counts, so `pkgs/test/pubspec.yaml` is the `test` package. A pubspec whose enclosing package is a
/// fixture is one too.
fn is_fixture(repo: &Repo, folder: &str) -> bool {
    let Some(above) = parent(folder) else {
        return false;
    };
    let enclosing = nearest_package(repo, above);
    above[enclosing.unwrap_or("").len()..]
        .split('/')
        .any(is_test_folder)
        || enclosing.is_some_and(|enclosing| is_fixture(repo, enclosing))
}

/// The package a `pubspec.yaml` declares: the value of its first line that starts at column 0 with `name:`, bare or in
/// matching quotes, a trailing ` #` comment and blanks cut, after a byte-order mark. No YAML library reads it: pub
/// requires a plain identifier.
fn package_name(text: &str) -> Option<String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value = text.lines().find_map(|line| line.strip_prefix("name:"))?;
    let comment = value
        .char_indices()
        .find(|&(at, c)| {
            c == '#'
                && value[..at]
                    .chars()
                    .next_back()
                    .is_none_or(char::is_whitespace)
        })
        .map_or(value.len(), |(at, _)| at);
    let value = value[..comment].trim();
    let value = ['\'', '"']
        .iter()
        .find_map(|&q| value.strip_prefix(q)?.strip_suffix(q))
        .unwrap_or(value);
    (!value.is_empty()).then(|| value.to_string())
}

/// What a URI of a directive names.
enum Resolved {
    /// No file the repository can hold: a scheme, a package nothing declares, a path above the root.
    Nothing,
    File(String),
    /// A `package:` name two indexed pubspecs declare, neither the importer's own.
    Unsure {
        name: String,
        candidates: Vec<String>,
    },
}

/// What `uri` names from `file`.
fn resolve(repo: &Repo, packages: &Packages, file: &str, uri: &str) -> Resolved {
    let Some(package) = uri.strip_prefix("package:") else {
        return relative(file, uri).map_or(Resolved::Nothing, Resolved::File);
    };
    let Some((name, path)) = package.split_once('/') else {
        return Resolved::Nothing;
    };
    // The path within `lib/`, with `.` and `..` removed as in a relative URI; one climbing above `lib/` names nothing.
    let Some(path) = joined("", path) else {
        return Resolved::Nothing;
    };
    let lib = |folder: &str| format!("{folder}lib/{path}");
    // Own package first: a file's own package is its nearest pubspec's, when that one is indexed.
    let own = nearest_package(repo, folder_of(file))
        .and_then(|folder| packages.by_folder.get(folder).map(|name| (folder, name)));
    if let Some((folder, own_name)) = own
        && own_name == name
    {
        return Resolved::File(lib(folder));
    }
    match packages.names.lookup(name) {
        Lookup::Unknown => Resolved::Nothing,
        Lookup::One(folder) => Resolved::File(lib(folder)),
        Lookup::Ambiguous(folders) => {
            let candidates: Vec<String> = folders
                .into_iter()
                .map(|folder| lib(folder))
                .filter(|target| repo.files().contains(target.as_str()))
                .collect();
            if candidates.is_empty() {
                Resolved::Nothing
            } else {
                Resolved::Unsure {
                    name: format!("package:{name}"),
                    candidates,
                }
            }
        }
    }
}
