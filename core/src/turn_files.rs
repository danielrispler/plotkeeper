//! A turn's changed files (danielrispler/plotkeeper#111, #207): the diff between the git trees the
//! hook recorded at its start and end. Every file that differs is the turn's, but one that another
//! session's or turn's call names (`names`). The core runs no `git`: it names the trees to diff
//! (`TurnDiff`), and the shell hands back git's numstat (`RootDiff`).

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
#[derive(Clone, Debug, PartialEq)]
pub struct RootDiff {
    pub root: String,
    pub files: Vec<Numstat>,
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
}

/// Whether the call of `row` names the file at `path` from worktree `root`: an edit of its
/// absolute path, or a command whose text holds its absolute path, or its path from the root when
/// the command touched that root. A bare file name never matches, and no shell is parsed.
pub(crate) fn names(row: &Row, root: &str, path: &str) -> bool {
    let absolute = format!("{}/{path}", root.trim_end_matches('/'));
    if edits(&row.tool) {
        return row.file.as_deref() == Some(absolute.as_str());
    }
    let Some(command) = row.command.as_deref() else {
        return false;
    };
    command.contains(&absolute) || (row.roots.iter().any(|r| r == root) && command.contains(path))
}

impl TurnFiles {
    /// The files of `diffs` but those `others` name, each with the agent that `agent` finds for it
    /// (its calls, else its worktree's worker).
    pub(crate) fn of(
        diffs: Vec<RootDiff>,
        others: &[&Row],
        agent: impl Fn(&RootDiff, &Numstat) -> Option<String>,
    ) -> TurnFiles {
        let mut found = TurnFiles::default();
        for diff in diffs {
            for file in &diff.files {
                let named = |path: &str| others.iter().any(|row| names(row, &diff.root, path));
                let taken = named(&file.path) || file.from.as_deref().is_some_and(named);
                if !taken {
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
        }
        found
    }
}

/// How long a running turn's files stay as they are before the shell is asked again.
pub(crate) const RUNNING_REFRESH_MS: u64 = 5_000;

/// Turns that ended longer ago than this get no files: their trees are likely gone.
pub(crate) const SINCE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
