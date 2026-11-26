//! The `dart-imports` extractor: the imports, exports and parts of Dart files, read as text with no `dart` run (ADR
//! 0002), and paired with the tracked files they name (spec #621). A `package:` URI names `lib/` of the tracked
//! `pubspec.yaml` that declares the package (the package index, `Packages`).

mod reader;

use std::collections::BTreeMap;

use super::{
    Extractor, Output, Pair, Repo, Unsure,
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

    /// A file under a folder named `test`, `integration_test` or `test_driver` below its package's root, or one whose
    /// header cannot be lexed.
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

/// The folder holding `file`, as a prefix: empty for the root, else ending in `/`.
fn folder_of(file: &str) -> &str {
    file.rfind('/').map_or("", |at| &file[..=at])
}

/// The folder above `folder`, or none for the root.
fn parent(folder: &str) -> Option<&str> {
    if folder.is_empty() {
        return None;
    }
    Some(folder_of(&folder[..folder.len() - 1]))
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

/// The folder of `file`'s package: the nearest tracked `pubspec.yaml` at or above it, any pubspec, else the root.
fn package_root<'f>(repo: &Repo, file: &'f str) -> &'f str {
    nearest_package(repo, folder_of(file)).unwrap_or("")
}

/// Whether `name` is a folder name Dart keeps its tests in.
fn is_test_folder(name: &str) -> bool {
    matches!(name, "test" | "integration_test" | "test_driver")
}

/// Whether a folder of `file` below its package's root is one Dart keeps its tests in.
fn in_test_folder(repo: &Repo, file: &str) -> bool {
    let below = &file[package_root(repo, file).len()..];
    let mut folders: Vec<&str> = below.split('/').collect();
    folders.pop();
    folders.into_iter().any(is_test_folder)
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

/// The packages `package:` URIs can name: each indexed `pubspec.yaml`'s package (spec #621, "The package index").
struct Packages {
    /// Each indexed package's name, with its folder.
    index: Names<String>,
    /// Each indexed package's folder, with its name.
    names: BTreeMap<String, String>,
}

impl Packages {
    fn read(repo: &Repo) -> (Packages, Vec<String>) {
        let mut packages = Packages {
            index: Names::new(),
            names: BTreeMap::new(),
        };
        let mut problems = Vec::new();
        for &file in repo.files() {
            if file.rsplit('/').next() != Some("pubspec.yaml") {
                continue;
            }
            let folder = folder_of(file);
            // A pubspec in a test folder is a fixture, not a package, and never a problem.
            if pubspec_in_test_folder(repo, folder) {
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
                    packages.index.register(&name, folder.to_string());
                    packages.names.insert(folder.to_string(), name);
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

/// Whether `folder`, a folder holding a `pubspec.yaml`, is a test folder or lies in one below an enclosing package's
/// root: the nearest tracked pubspec above it. With no enclosing package, a folder named `test` is the package itself.
fn pubspec_in_test_folder(repo: &Repo, folder: &str) -> bool {
    let Some(enclosing) = parent(folder).and_then(|above| nearest_package(repo, above)) else {
        return false;
    };
    folder[enclosing.len()..].split('/').any(is_test_folder)
}

/// The package a `pubspec.yaml` declares: the value of its first line that starts at column 0 with `name:`, bare or in
/// matching quotes, a trailing ` #` comment and blanks cut. No YAML library reads it: pub requires a plain identifier.
fn package_name(text: &str) -> Option<String> {
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
    let lib = |folder: &str| format!("{folder}lib/{path}");
    // Own package first: a file's own package is its nearest pubspec's, when that one is indexed.
    let own = nearest_package(repo, folder_of(file))
        .and_then(|folder| packages.names.get(folder).map(|name| (folder, name)));
    if let Some((folder, own_name)) = own
        && own_name == name
    {
        return Resolved::File(lib(folder));
    }
    match packages.index.lookup(name) {
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
