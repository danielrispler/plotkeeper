//! Resolving an include to a tracked file (spec #806, "Resolution, per include"; ADR 0004).
//!
//! Steps 1 and 2 need only the include and the file; steps 3 and 4 need the whole repository. So the reader first
//! settles each include as far as the path-ending match, counts the evidence for the folder tie-break over every
//! include, and only then settles the ties, which is why the result doesn't depend on file order.

use std::collections::{BTreeMap, HashMap};

use super::lexer::Include;
use super::system_headers::is_system;
use super::{Repo, folders, is_test_file};

/// How many includes, each with one candidate, a folder needs for it to settle a tie (research, round 4, item 2).
const EVIDENCE: usize = 5;

/// The tracked files, keyed by their last path segment, built once per run.
pub(super) struct Index<'a> {
    by_name: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Index<'a> {
    pub(super) fn new(repo: &Repo<'a>) -> Self {
        let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
        for &file in repo.files() {
            by_name
                .entry(file.rsplit('/').next().unwrap_or(file))
                .or_default()
                .push(file);
        }
        Index { by_name }
    }

    /// The tracked files equal to `path` or ending in `/` and `path`, less skipped files, sorted; ignored files stay.
    fn candidates(&self, path: &str) -> Vec<&'a str> {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.by_name
            .get(name)
            .into_iter()
            .flatten()
            .copied()
            .filter(|file| ends_with_path(file, path) && !is_test_file(file))
            .collect()
    }
}

/// Whether `file` is `path` or ends in `/` followed by it.
fn ends_with_path(file: &str, path: &str) -> bool {
    file.strip_suffix(path)
        .is_some_and(|rest| rest.is_empty() || rest.ends_with('/'))
}

/// The folder `file` implies for `path`: `file` less `/` and `path`, or `.` when they are equal.
fn implied_folder<'a>(file: &'a str, path: &str) -> &'a str {
    file.len()
        .checked_sub(path.len() + 1)
        .map_or(".", |end| &file[..end])
}

/// What an include came to before the folder tie-break.
pub(super) enum Settled<'a> {
    Nothing,
    File(String),
    /// Several candidates, none ignored, for the tie-break.
    Tie(Vec<&'a str>),
    /// Some but not all candidates are ignored.
    IgnoredCopy(Vec<&'a str>),
}

/// The path of `include` as written, less its `.` segments; none for an empty path, an absolute one or one with a
/// backslash (spec #806, "Paths").
pub(super) fn clean(path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains('\\') {
        return None;
    }
    let kept: Vec<&str> = path.split('/').filter(|s| *s != ".").collect();
    let cleaned = kept.join("/");
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Steps 1 to 3 for one include of `from`, and `path` its cleaned path. A folder's evidence is added to `evidence` for
/// an include with exactly one candidate.
pub(super) fn settle<'a>(
    repo: &Repo,
    index: &Index<'a>,
    from: &str,
    include: &Include,
    path: &str,
    evidence: &mut BTreeMap<String, usize>,
) -> Settled<'a> {
    // Step 1: next to the file.
    if include.quoted
        && let Some(near) = folders::join(folders::folder_of(from), path)
        && repo.files().contains(near.as_str())
    {
        return if repo.is_ignored(&near) || is_test_file(&near) {
            Settled::Nothing
        } else {
            Settled::File(near)
        };
    }
    // Step 2: system names.
    if !include.quoted && is_system(path) {
        return Settled::Nothing;
    }
    // A `..` path is resolved only next to the file.
    if path.split('/').any(|s| s == "..") {
        return Settled::Nothing;
    }
    // Step 3: the path-ending match.
    let candidates = index.candidates(path);
    if let [only] = candidates[..] {
        *evidence
            .entry(implied_folder(only, path).to_string())
            .or_default() += 1;
    }
    let ignored = candidates.iter().filter(|c| repo.is_ignored(c)).count();
    match (candidates.len(), ignored) {
        (0, _) => Settled::Nothing,
        (n, i) if i == n => Settled::Nothing,
        (1, _) => Settled::File(candidates[0].to_string()),
        (_, 0) => Settled::Tie(candidates),
        _ => Settled::IgnoredCopy(candidates),
    }
}

/// Step 4: the one candidate whose implied folder has evidence of 5 or more, when exactly one has.
pub(super) fn tie_break(
    candidates: &[&str],
    path: &str,
    evidence: &BTreeMap<String, usize>,
) -> Option<String> {
    let mut backed = candidates.iter().filter(|c| {
        evidence
            .get(implied_folder(c, path))
            .is_some_and(|n| *n >= EVIDENCE)
    });
    match (backed.next(), backed.next()) {
        (Some(one), None) => Some(one.to_string()),
        _ => None,
    }
}
