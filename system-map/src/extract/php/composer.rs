//! The autoload index of `php-imports`: what the tracked `composer.json` files map, and Composer's walk from a class
//! name to the file it loads (spec #647, "The autoload index" and "Class resolution").

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::super::Repo;

/// What one `composer.json` maps.
struct Composer {
    /// Each non-empty `psr-4` prefix, with its trailing `\`, and its folders in order, from the repository root.
    prefixes: BTreeMap<String, Vec<String>>,
    /// The folders of the `""` prefix, tried last.
    fallback: Vec<String>,
    /// The folders and files of `classmap`, from the repository root, `""` for the root.
    classmap: Vec<String>,
}

/// Every read `composer.json` of a repository, built once per [`Repo`].
pub(crate) struct Autoload {
    composers: Vec<Composer>,
    /// What stopped a `composer.json` from being read, one line each, naming it.
    pub(super) problems: Vec<String>,
}

impl Autoload {
    /// The index of the tracked `composer.json` files of `repo`, read from its texts.
    pub(in crate::extract) fn build(repo: &Repo) -> Autoload {
        let mut index = Autoload {
            composers: Vec::new(),
            problems: Vec::new(),
        };
        for &file in repo.files() {
            let Some(folder) = composer_folder(file) else {
                continue;
            };
            // A file the scans do not read, or one that is no JSON object, maps nothing and is no problem.
            let Some(text) = repo.read(file) else {
                continue;
            };
            match serde_json::from_str::<Value>(&text) {
                Err(e) => index.problems.push(format!("{file}: not valid JSON: {e}")),
                Ok(Value::Object(root)) => match Composer::read(folder, &root) {
                    Ok(composer) => index.composers.push(composer),
                    Err(problems) => index
                        .problems
                        .extend(problems.into_iter().map(|p| format!("{file}: {p}"))),
                },
                Ok(_) => {}
            }
        }
        index
    }

    /// Whether `file` lies under a `classmap` path of a read `composer.json` and under no PSR-4 folder of any: only a
    /// classmap loads it, and that needs a scan of the classes the files declare, which the text does not give.
    pub(super) fn classmap_only(&self, file: &str) -> bool {
        let under = |path: &String| {
            path.is_empty()
                || file == path
                || file
                    .strip_prefix(path.as_str())
                    .is_some_and(|r| r.starts_with('/'))
        };
        self.composers.iter().any(|c| c.classmap.iter().any(under))
            && !self
                .composers
                .iter()
                .any(|c| c.prefixes.values().flatten().chain(&c.fallback).any(under))
    }

    /// The tracked files Composer's walk gives `class`, one per `composer.json` that has an answer, each once.
    pub(super) fn resolve(&self, repo: &Repo, class: &str) -> Vec<String> {
        let mut answers = Vec::new();
        for composer in &self.composers {
            if let Some(file) = composer.walk(repo, class)
                && !answers.contains(&file)
            {
                answers.push(file);
            }
        }
        answers.sort();
        answers
    }
}

impl Composer {
    /// What the `psr-4` tables of `autoload` and `autoload-dev` in `root`, the `composer.json` of `folder`, map; the
    /// problems of the prefixes and values Composer refuses when there are any.
    fn read(folder: &str, root: &Map<String, Value>) -> Result<Composer, Vec<String>> {
        let mut composer = Composer {
            prefixes: BTreeMap::new(),
            fallback: Vec::new(),
            classmap: Vec::new(),
        };
        let mut problems = Vec::new();
        for section in ["autoload", "autoload-dev"] {
            if let Some(Value::Array(paths)) = root.get(section).and_then(|s| s.get("classmap")) {
                composer.classmap.extend(
                    paths
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|p| join(folder, p)),
                );
            }
            let Some(Value::Object(psr4)) = root.get(section).and_then(|s| s.get("psr-4")) else {
                continue;
            };
            for (prefix, value) in psr4 {
                let prefix = prefix.strip_prefix('\\').unwrap_or(prefix);
                if !prefix.is_empty() && !prefix.ends_with('\\') {
                    problems.push(format!(
                        "the psr-4 prefix \"{prefix}\" does not end in a backslash, which Composer refuses"
                    ));
                    continue;
                }
                let paths = match value {
                    Value::String(path) => Some(vec![path.as_str()]),
                    Value::Array(paths) => paths.iter().map(Value::as_str).collect(),
                    _ => None,
                };
                let Some(paths) = paths else {
                    problems.push(format!(
                        "the psr-4 prefix \"{prefix}\" maps to neither a folder nor a list of folders"
                    ));
                    continue;
                };
                let folders = paths.into_iter().map(|path| join(folder, path));
                if prefix.is_empty() {
                    composer.fallback.extend(folders);
                } else {
                    composer
                        .prefixes
                        .entry(prefix.to_string())
                        .or_default()
                        .extend(folders);
                }
            }
        }
        if problems.is_empty() {
            Ok(composer)
        } else {
            Err(problems)
        }
    }

    /// The first tracked file Composer would load for `class`: each prefix of the name the table holds, from the longest
    /// down, each of its folders in order.
    fn walk(&self, repo: &Repo, class: &str) -> Option<String> {
        let rest = class.replace('\\', "/");
        let mut sub = class;
        let mut tries: Vec<(&String, &str)> = Vec::new();
        while let Some(at) = sub.rfind('\\') {
            sub = &sub[..at];
            for folder in self.prefixes.get(&format!("{sub}\\")).into_iter().flatten() {
                tries.push((folder, &rest[at + 1..]));
            }
        }
        tries.extend(self.fallback.iter().map(|folder| (folder, rest.as_str())));
        tries
            .into_iter()
            .find_map(|(folder, rest)| {
                let path = if folder.is_empty() {
                    format!("{rest}.php")
                } else {
                    format!("{folder}/{rest}.php")
                };
                repo.files().contains(path.as_str()).then_some(path)
            })
            // An ignored first hit is no answer, not a reason to go on: Composer would have loaded it.
            .filter(|path| !repo.is_ignored(path))
    }
}

/// The folder of a tracked `composer.json` that no test or `vendor` folder holds, `""` for the root.
fn composer_folder(file: &str) -> Option<&str> {
    let folder = match file.rsplit_once('/') {
        Some((folder, "composer.json")) => folder,
        None if file == "composer.json" => "",
        _ => return None,
    };
    let barred = folder.split('/').any(|name| {
        name.eq_ignore_ascii_case("tests") || name.eq_ignore_ascii_case("test") || name == "vendor"
    });
    (!barred).then_some(folder)
}

/// `path` joined to `folder` and cleaned: no `.`, `..` or empty segment, `""` for the root.
fn join(folder: &str, path: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    for segment in folder.split('/').chain(path.split('/')) {
        match segment {
            "" | "." => {}
            ".." => {
                kept.pop();
            }
            s => kept.push(s),
        }
    }
    kept.join("/")
}
