//! A turn's changed files (danielrispler/plotkeeper#111, #207): the diff between the git trees the
//! hook recorded at its start and end. Every file that differs is the turn's, but one that another
//! session's or turn's call names (`names`). The core runs no `git`: it names the trees to diff
//! (`TurnDiff`), and the shell hands back git's numstat (`RootDiff`).

use crate::Roots;
use crate::ledger::Row;
use serde::{Deserialize, Serialize};

/// The tools whose call changes the file its input names.
const EDITS: [&str; 4] = ["Edit", "MultiEdit", "Write", "NotebookEdit"];

/// Whether a call of `tool` edits the file it names.
pub(crate) fn edits(tool: &str) -> bool {
    EDITS.contains(&tool)
}

/// A worktree's two trees of a turn, which the shell diffs. A running turn has no `end` yet: the
/// shell takes the tree of the worktree as it is now.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnTrees {
    pub root: String,
    pub start: String,
    pub end: Option<String>,
}

/// The diff the shell is asked for: the worktrees of one turn of one session.
#[derive(Clone, Debug, PartialEq)]
pub struct TurnDiff {
    pub session_id: String,
    /// The turn's index in `Session::turns`.
    pub turn: usize,
    /// Whether its trees are final and worth anchoring: the turn has ended, and no worker it
    /// spawned runs on in one of its worktrees.
    pub ended: bool,
    pub trees: Vec<TurnTrees>,
    /// The span of time its trees run in, in milliseconds (`u64::MAX` while it runs): the moves of
    /// `HEAD` in it are the shell's to find.
    pub window: (u64, u64),
}

/// One line of `git diff --numstat -M` between two trees.
#[derive(Clone, Debug, PartialEq)]
pub struct Numstat {
    /// The path from its worktree's root; a rename's new path.
    pub path: String,
    /// A rename's old path.
    pub from: Option<String>,
    /// Lines added and removed; `None` for a binary file.
    pub added: Option<u64>,
    pub removed: Option<u64>,
}

/// The numstat of one worktree, as the shell read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RootDiff {
    pub root: String,
    pub files: Vec<Numstat>,
    /// The paths a move of `HEAD` during the turn changed, other than the agent's own commits: a
    /// merge, a pull, a rebase, a checkout or a reset brought them in.
    pub moved: Vec<String>,
}

/// A file a turn changed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnFile {
    /// Its worktree's root, and its path from there; a rename's new path.
    pub root: String,
    pub path: String,
    /// A rename's old path: one row, not a deletion and an addition.
    pub from: Option<String>,
    /// Lines added and removed; `None` for a binary file.
    pub added: Option<u64>,
    pub removed: Option<u64>,
    /// The name of the agent that changed it: the one whose call names it, else the worker whose
    /// worktree it is in. `None` when neither says, as for Daniel's own edit.
    #[serde(default)]
    pub agent: Option<String>,
}

/// What a turn changed, as far as the trees say.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnFiles {
    pub files: Vec<TurnFile>,
    /// How many files that differ were brought in by a move of `HEAD` and no call of the turn
    /// names: merged work, not the turn's change.
    #[serde(default)]
    pub merged: usize,
}

/// Whether the call of `row` names the file at `path` from worktree `root`: an edit of its
/// absolute path, or a command whose text holds its absolute path, or its path from the root when
/// the command touched that root. A bare file name never matches, and no shell is parsed. An edit's
/// path is the tool's own spelling, so `roots` finds it inside `root` (git's spelling) through a
/// symlink; a command's text is taken as written.
pub(crate) fn names(row: &Row, roots: &Roots, root: &str, path: &str) -> bool {
    let absolute = format!("{}/{path}", root.trim_end_matches('/'));
    if edits(&row.tool) {
        return row.file.as_deref().and_then(|f| roots.inside(root, f)) == Some(path);
    }
    let Some(command) = row.command.as_deref() else {
        return false;
    };
    // A root-level file's path from the root is its bare name, which a command names only in full.
    let relative = path.contains('/') && row.roots.iter().any(|r| r == root);
    mentions(command, &absolute) || (relative && mentions(command, path))
}

/// Whether `command` holds `path` as a whole path: not as the tail of a longer one (`mysrc/a.rs`,
/// `x/core/a.rs`, though `./src/a.rs` is `src/a.rs`) and not as the start of a longer name
/// (`a.rs.bak`, `a.rs2`).
fn mentions(command: &str, path: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || "_-".contains(c);
    command.match_indices(path).any(|(at, _)| {
        let before = &command[..at];
        let after = command[at + path.len()..].chars();
        let start = match before.strip_suffix("./") {
            Some(before) => before,
            None => before,
        };
        let opens = start
            .chars()
            .next_back()
            .is_none_or(|c| !word(c) && c != '/' && c != '.')
            || path.starts_with('/') && before.chars().next_back().is_none_or(|c| !word(c));
        let mut rest = after;
        let closes = match rest.next() {
            None => true,
            Some('.') => rest.next().is_none_or(|c| !word(c)),
            Some(c) => !word(c) && c != '/',
        };
        opens && closes
    })
}

impl TurnFiles {
    /// The files of `diffs` but those `others` name and no call of `ours` does, each with the agent that `agent` finds for it
    /// (its calls, else its worktree's worker). A file a move of `HEAD` brought in, which no call
    /// of `ours` names, is counted in `merged` instead.
    pub(crate) fn of(
        diffs: Vec<RootDiff>,
        roots: &Roots,
        others: &[&Row],
        ours: &[&Row],
        agent: impl Fn(&RootDiff, &Numstat) -> Option<String>,
    ) -> TurnFiles {
        let mut found = TurnFiles::default();
        for diff in diffs {
            for file in &diff.files {
                // A rename counts by its old path too.
                let by =
                    |f: &dyn Fn(&str) -> bool| f(&file.path) || file.from.as_deref().is_some_and(f);
                let named = |path: &str| others.iter().any(|row| names(row, roots, &diff.root, path));
                let called = |path: &str| ours.iter().any(|row| names(row, roots, &diff.root, path));
                if by(&named) && !by(&called) {
                    continue;
                }
                let moved = |path: &str| diff.moved.iter().any(|m| m == path);
                if by(&moved) && !by(&called) {
                    found.merged += 1;
                    continue;
                }
                found.files.push(TurnFile {
                    root: diff.root.clone(),
                    path: file.path.clone(),
                    from: file.from.clone(),
                    added: file.added,
                    removed: file.removed,
                    agent: agent(&diff, file),
                });
            }
        }
        found
    }
}

/// How long a running turn's files stay as they are before the shell is asked again.
pub(crate) const RUNNING_REFRESH_MS: u64 = 5_000;

/// Turns that ended longer ago than this get no files: their trees are likely gone.
pub(crate) const SINCE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
