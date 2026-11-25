//! Extractors: readers of one language's code, or of one kind of build manifest, that tell the map which tracked file
//! depends on which (spec #529, "The contract"). Each is one module behind [`Extractor`], registered in
//! [`registry::EXTRACTORS`]; the core here runs the ones a map names and holds their output to the contract.

mod cargo;
#[cfg(test)]
mod contract;
pub(crate) mod registry;
mod rust_modules;
mod typescript;

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
};

use crate::PartFiles;

/// A reader of one language's code, or of one kind of build manifest.
pub(crate) trait Extractor {
    /// The name a map gives it in `extractors`.
    fn name(&self) -> &'static str;

    /// Whether `file` is one of its language's files, or of its manifest kind.
    fn reads(&self, file: &str) -> bool;

    /// Whether `file` is one of its language's test files, which make no pairs and never make their part read.
    fn is_test(&self, _repo: &Repo, _file: &str) -> bool {
        false
    }

    /// Whether the repository holds a file it would read: a tracked, non-ignored file it reads that is no test file.
    #[allow(dead_code, reason = "`check`'s note calls it (#534)")]
    fn detect(&self, repo: &Repo) -> bool {
        repo.files()
            .iter()
            .any(|f| !repo.is_ignored(f) && self.reads(f) && !self.is_test(repo, f))
    }

    /// Its pairs, and its problems when it cannot run, or not fully.
    fn extract(&self, repo: &Repo) -> Output;

    /// The files the contract cases build their repositories from.
    #[cfg(test)]
    fn fixture(&self) -> contract::Fixture;
}

/// The core's view of the repository: its root, the files git tracks (ignored ones too) and which are ignored.
pub(crate) struct Repo<'a> {
    root: &'a Path,
    files: BTreeSet<&'a str>,
    ignored: HashSet<&'a str>,
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
        }
    }

    pub(crate) fn root(&self) -> &Path {
        self.root
    }

    /// Every tracked file, sorted.
    pub(crate) fn files(&self) -> &BTreeSet<&'a str> {
        &self.files
    }

    #[allow(dead_code, reason = "`detect` calls it (#534)")]
    pub(crate) fn is_ignored(&self, file: &str) -> bool {
        self.ignored.contains(file)
    }

    /// The text of a tracked file, its bytes that are not UTF-8 read lossily; none for an untracked file or one the
    /// scans do not read (`crate::read_tracked`).
    pub(crate) fn read(&self, file: &str) -> Option<String> {
        self.files
            .contains(file)
            .then(|| crate::read_tracked(self.root, file))
            .flatten()
    }
}

/// A resolved dependency of the tracked file `from` on the tracked file `to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pair {
    pub(crate) from: String,
    pub(crate) to: String,
    /// The import site's line; none until #498 fills it.
    pub(crate) line: Option<u32>,
    /// A crate root's `mod` line: kept for the check, left out of the imports report.
    pub(crate) declaration: bool,
}

impl Pair {
    pub(crate) fn new(from: &str, to: &str) -> Pair {
        Pair {
            from: from.to_string(),
            to: to.to_string(),
            line: None,
            declaration: false,
        }
    }
}

/// What an extractor found: the problems that say it could not run, or not fully, and its pairs, together when both
/// apply.
#[derive(Debug, Default)]
pub(crate) struct Output {
    pub(crate) problems: Vec<String>,
    pub(crate) pairs: Vec<Pair>,
}

/// Runs the extractors `names` names from `registry`, in that order, and holds their output to the contract: a pair
/// whose ends are not both tracked, from a file to itself, or from a file the extractor names a test file is dropped,
/// and a pair two extractors (or one, twice) find is kept once, sorted, with its lowest line, and as a declaration only
/// if every finding of it is one. A name the registry lacks is a problem.
pub(crate) fn run(registry: &[&dyn Extractor], names: &[String], repo: &Repo) -> Output {
    let mut problems = Vec::new();
    // Each pair's ends, with its lowest line and whether every finding of it is a declaration.
    let mut pairs: BTreeMap<(String, String), (Option<u32>, bool)> = BTreeMap::new();
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
        for pair in found.pairs {
            let tracked = |f: &str| repo.files.contains(f);
            if pair.from == pair.to
                || !tracked(&pair.from)
                || !tracked(&pair.to)
                || extractor.is_test(repo, &pair.from)
            {
                continue;
            }
            pairs
                .entry((pair.from, pair.to))
                .and_modify(|(line, declaration)| {
                    *line = [*line, pair.line].into_iter().flatten().min();
                    *declaration &= pair.declaration;
                })
                .or_insert((pair.line, pair.declaration));
        }
    }
    Output {
        problems,
        pairs: pairs
            .into_iter()
            .map(|((from, to), (line, declaration))| Pair {
                from,
                to,
                line,
                declaration,
            })
            .collect(),
    }
}

/// The parts one of the named extractors reads: those that own a file it reads that is no test file.
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
            part.files.iter().any(|file| {
                named
                    .iter()
                    .any(|e| e.reads(file) && !e.is_test(repo, file))
            })
        })
        .map(|part| part.id.as_str())
        .collect()
}

#[cfg(test)]
mod tests;
