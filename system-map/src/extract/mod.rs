//! Extractors: readers of one language's code, or of one kind of build manifest, that tell the map which tracked file
//! depends on which (spec #529, "The contract"). Each is one module behind [`Extractor`], registered in
//! [`registry::EXTRACTORS`]; the core here runs the ones a map names and holds their output to the contract.

mod cargo;
#[cfg(test)]
mod contract;
mod dart;
mod dotnet;
mod folders;
mod go;
mod java;
mod names;
mod php;
mod python;
pub(crate) mod registry;
mod rust_modules;
mod typescript;

use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, btree_map::Entry},
    path::Path,
};

use serde::Serialize;

use crate::PartFiles;
use folders::folder_of;

/// A reader of one language's code, or of one kind of build manifest.
pub(crate) trait Extractor {
    /// The name a map gives it in `extractors`.
    fn name(&self) -> &'static str;

    /// Whether `file` is one of its language's files, or of its manifest kind.
    fn reads(&self, file: &str) -> bool;

    /// Whether `file` is a file its language's build leaves out: a test file, or one the build never compiles. A skipped
    /// file makes no pairs and never makes its part read.
    fn skips(&self, _repo: &Repo, _file: &str) -> bool {
        false
    }

    /// Whether the repository holds a file it would read: a tracked, non-ignored file it reads and does not skip.
    fn detect(&self, repo: &Repo) -> bool {
        repo.files()
            .iter()
            .any(|f| !repo.is_ignored(f) && self.reads(f) && !self.skips(repo, f))
    }

    /// Its pairs, and its problems when it cannot run, or not fully.
    fn extract(&self, repo: &Repo) -> Output;

    /// The files the contract cases build their repositories from.
    #[cfg(test)]
    fn fixture(&self) -> contract::Fixture;
}

/// The core's view of the repository: its root, the files git tracks (ignored ones too) and which are ignored, read from
/// the working tree or, in commit mode, from one commit.
pub(crate) struct Repo<'a> {
    root: &'a Path,
    files: BTreeSet<&'a str>,
    ignored: HashSet<&'a str>,
    /// In commit mode, the commit's texts (`crate::commit`): the scans read these and never the working tree.
    texts: Option<&'a BTreeMap<String, String>>,
    /// In commit mode, a folder holding the commit's manifests and Rust files for `cargo metadata`, written once.
    cargo_dir: OnceCell<Result<tempfile::TempDir, String>>,
    /// In commit mode, whether the commit's toolchain file goes in that folder: only for a commit plotkeeper trusts.
    trusted: bool,
    /// `php-imports`' index of the tracked `composer.json` files, built on first use.
    autoload: OnceCell<php::Autoload>,
}

impl<'a> Repo<'a> {
    pub(crate) fn new(
        root: &'a Path,
        files: &'a [String],
        is_ignored: impl Fn(&str) -> bool,
    ) -> Repo<'a> {
        Repo {
            root,
            files: files.iter().map(String::as_str).collect(),
            ignored: files
                .iter()
                .map(String::as_str)
                .filter(|f| is_ignored(f))
                .collect(),
            texts: None,
            cargo_dir: OnceCell::new(),
            trusted: true,
            autoload: OnceCell::new(),
        }
    }

    /// The repository in commit mode: `texts` are the commit's. An untrusted commit's toolchain file is left out of the
    /// folder `cargo metadata` runs in, so the commit cannot choose the toolchain rustup resolves for `cargo` (#641).
    pub(crate) fn at_commit(self, texts: &'a BTreeMap<String, String>, trusted: bool) -> Repo<'a> {
        Repo {
            texts: Some(texts),
            trusted,
            ..self
        }
    }

    /// Whether `file` is a regular file: on disk, tracked or not, or in the commit.
    pub(crate) fn has(&self, file: &str) -> bool {
        match self.texts {
            Some(texts) => texts.contains_key(file),
            None => self.root.join(file).is_file(),
        }
    }

    /// The folder to run `cargo metadata` in: the root, or in commit mode a temporary folder holding the commit's
    /// `Cargo.toml` files, Rust files and, for a trusted commit, toolchain file at their paths, which are all `cargo metadata
    /// --no-deps` reads.
    pub(crate) fn cargo_dir(&self) -> Result<&Path, String> {
        let Some(texts) = self.texts else {
            return Ok(self.root);
        };
        let dir = self.cargo_dir.get_or_init(|| {
            let dir = tempfile::tempdir().map_err(|e| format!("cannot make a folder: {e}"))?;
            for (file, text) in texts {
                let name = file.rsplit('/').next().unwrap_or(file);
                let toolchain = self.trusted
                    && matches!(file.as_str(), "rust-toolchain" | "rust-toolchain.toml");
                if name == "Cargo.toml" || name.ends_with(".rs") || toolchain {
                    let path = dir.path().join(file);
                    std::fs::create_dir_all(path.parent().unwrap_or(dir.path()))
                        .and_then(|_| std::fs::write(&path, text))
                        .map_err(|e| format!("cannot write {file} to a folder: {e}"))?;
                }
            }
            Ok(dir)
        });
        dir.as_ref()
            .map(tempfile::TempDir::path)
            .map_err(Clone::clone)
    }

    /// What the tracked `composer.json` files map, read from the same texts as everything else, so in commit mode from
    /// the commit's; built on first use and kept, since `skips` asks for it once per file.
    pub(crate) fn autoload(&self) -> &php::Autoload {
        self.autoload.get_or_init(|| php::Autoload::build(self))
    }

    /// Every tracked file, sorted.
    pub(crate) fn files(&self) -> &BTreeSet<&'a str> {
        &self.files
    }

    pub(crate) fn is_ignored(&self, file: &str) -> bool {
        self.ignored.contains(file)
    }

    /// The text of a tracked file, its bytes that are not UTF-8 read lossily; none for an untracked file or one the
    /// scans do not read (`crate::read_tracked`).
    pub(crate) fn read(&self, file: &str) -> Option<String> {
        if !self.files.contains(file) {
            return None;
        }
        match self.texts {
            Some(texts) => texts.get(file).cloned(),
            None => crate::read_tracked(self.root, file),
        }
    }
}

/// A resolved dependency of the tracked file `from` on the tracked file `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pair {
    pub(crate) from: String,
    pub(crate) to: String,
    /// The lines of `from` that make the pair, each an import site (spec #477, "What an import site is"): a `use` line
    /// once however many names it brings in, an inline path once per line. Empty when the extractor finds no line.
    pub(crate) lines: BTreeSet<u32>,
    /// A crate root's `mod` line or a Dart library's `part` line: kept for the check, left out of the imports report.
    pub(crate) declaration: bool,
    /// The one target an import named that covers several files (a Go package's folder, say): the imports report
    /// counts the pairs of one import of it as one unit per part. None for a pair that is its own unit.
    pub(crate) group: Option<String>,
    /// The group `from` itself belongs to (its Go package's folder, say): the imports report counts a grouped pair's
    /// importer as that group's slice, so a file nothing imports still counts as its package. None for a pair that
    /// counts its importer as the file.
    pub(crate) from_group: Option<String>,
}

impl Pair {
    pub(crate) fn new(from: &str, to: &str) -> Pair {
        Pair {
            from: from.to_string(),
            to: to.to_string(),
            lines: BTreeSet::new(),
            declaration: false,
            group: None,
            from_group: None,
        }
    }

    /// The pair `from` → `to` made at `line` of `from`.
    pub(crate) fn at(from: &str, to: &str, line: u32) -> Pair {
        Pair {
            lines: BTreeSet::from([line]),
            ..Pair::new(from, to)
        }
    }
}

/// A tracked file an extractor reads but cannot resolve with certainty: an import in it could name more than one target
/// (spec #569, "Unsure").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unsure {
    /// The importing file.
    pub file: String,
    /// The name in doubt, such as the import path `example.com/a`.
    pub name: String,
    /// Why it is in doubt, such as `declared twice`.
    pub reason: String,
    /// The tracked files the name could mean, sorted; never empty.
    pub candidates: Vec<String>,
}

/// What an extractor found: the problems that say it could not run, or not fully, its pairs and its Unsure files,
/// together when they apply.
#[derive(Debug, Default)]
pub(crate) struct Output {
    pub(crate) problems: Vec<String>,
    pub(crate) pairs: Vec<Pair>,
    pub(crate) unsure: Vec<Unsure>,
    /// What it did not read and says so, one line each: `check` prints them as notes and never fails over them.
    pub(crate) notes: Vec<String>,
    /// Each file it puts in a group (its Go package's folder, say), with that group, whether or not the file makes or
    /// ends a pair: the imports report names a group's slice by its part when the group's files sit in more than one.
    pub(crate) members: BTreeSet<(String, String)>,
}

/// Runs the extractors `names` names from `registry`, in that order, and holds their output to the contract: a pair
/// whose ends are not both tracked, from a file to itself, or from a file the extractor reads and skips is dropped,
/// and a pair two extractors (or one, twice) find is kept once, sorted, with the lines of every finding, as a
/// declaration only if every finding of it is one, and with the lowest of its findings' groups and of their `from`
/// groups, none counting as lowest.
/// An Unsure finding with no candidates is a problem, one in an untracked, ignored or skipped file is dropped, and
/// findings of one file, name and reason are kept once, sorted, with the union of their candidates. A name the registry
/// lacks is a problem. Every extractor's notes are kept, in the order of `names`. A group's member that is untracked or
/// skipped is dropped.
pub(crate) fn run(registry: &[&dyn Extractor], names: &[String], repo: &Repo) -> Output {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    // Each Unsure finding's file, name and reason, with the candidates of every finding of them.
    let mut unsure: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
    // Each pair by its ends, merged from its findings.
    let mut pairs: BTreeMap<(String, String), Pair> = BTreeMap::new();
    let mut members = BTreeSet::new();
    for name in names {
        let Some(extractor) = registry.iter().find(|e| e.name() == name) else {
            problems.push(format!(
                "unknown extractor {name:?} in extractors; use one of {}",
                registry
                    .iter()
                    .map(|e| e.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            continue;
        };
        let found = extractor.extract(repo);
        problems.extend(found.problems);
        notes.extend(found.notes);
        // Whether each file it reads is skipped, asked once per file, since `skips` may read the file.
        let mut skipped: HashMap<String, bool> = HashMap::new();
        let mut skips = |file: &str| {
            extractor.reads(file)
                && *skipped
                    .entry(file.to_string())
                    .or_insert_with(|| extractor.skips(repo, file))
        };
        let tracked = |f: &str| repo.files.contains(f);
        for finding in found.unsure {
            if finding.candidates.is_empty() {
                problems.push(format!(
                    "extractor {name:?} found {} unsure of {:?} with no candidates; an Unsure finding names at least one \
                     candidate file",
                    finding.file, finding.name
                ));
                continue;
            }
            if !tracked(&finding.file) || repo.is_ignored(&finding.file) || skips(&finding.file) {
                continue;
            }
            unsure
                .entry((finding.file, finding.name, finding.reason))
                .or_default()
                .extend(finding.candidates);
        }
        for (file, group) in found.members {
            if tracked(&file) && !skips(&file) {
                members.insert((file, group));
            }
        }
        for pair in found.pairs {
            if pair.from == pair.to || !tracked(&pair.from) || !tracked(&pair.to) {
                continue;
            }
            if skips(&pair.from) {
                continue;
            }
            match pairs.entry((pair.from.clone(), pair.to.clone())) {
                Entry::Vacant(entry) => {
                    entry.insert(pair);
                }
                Entry::Occupied(mut entry) => {
                    let kept = entry.get_mut();
                    kept.lines.extend(&pair.lines);
                    kept.declaration &= pair.declaration;
                    // `Option`'s order puts none first, so none is the lowest group.
                    kept.group = kept.group.take().min(pair.group);
                    kept.from_group = kept.from_group.take().min(pair.from_group);
                }
            }
        }
    }
    Output {
        problems,
        notes,
        pairs: pairs.into_values().collect(),
        unsure: unsure
            .into_iter()
            .map(|((file, name, reason), candidates)| Unsure {
                file,
                name,
                reason,
                candidates: candidates.into_iter().collect(),
            })
            .collect(),
        members,
    }
}

/// The parts one of the named extractors reads: those that own a file it reads and does not skip.
pub(crate) fn read_parts<'p>(
    registry: &[&dyn Extractor],
    names: &[String],
    repo: &Repo,
    parts: &'p [PartFiles],
) -> HashSet<&'p str> {
    let named: Vec<&&dyn Extractor> = registry
        .iter()
        .filter(|e| names.iter().any(|n| n == e.name()))
        .collect();
    parts
        .iter()
        .filter(|part| {
            part.files
                .iter()
                .any(|file| named.iter().any(|e| e.reads(file) && !e.skips(repo, file)))
        })
        .map(|part| part.id.as_str())
        .collect()
}

/// The name a folder's files go by as a pair's group: the folder, or `./` for the root.
fn group(folder: &str) -> String {
    if folder.is_empty() { "./" } else { folder }.to_string()
}

#[cfg(test)]
mod tests;
