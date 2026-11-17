//! A turn's changed files (danielrispler/plotkeeper#111): the diff between the git trees the hook
//! recorded at its start and end, kept for the files the step ledger says its agent or its workers
//! touched. The rest of the diff is counted as external. The core runs no `git`: it names the trees
//! to diff (`TurnDiff`), and the shell hands back git's numstat (`RootDiff`).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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

/// A file the agent or its workers changed in a turn.
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
}

/// What a turn changed, as far as the trees say.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TurnFiles {
    pub files: Vec<TurnFile>,
    /// How many other files the trees differ in, which someone else changed or a shell command did.
    pub external: u64,
}

impl TurnFiles {
    /// The files of `diffs` whose absolute path is in `ours`, and the count of the others, but
    /// those in `theirs`, which another turn changed.
    pub(crate) fn of(
        diffs: Vec<RootDiff>,
        ours: &BTreeSet<String>,
        theirs: &BTreeSet<String>,
    ) -> TurnFiles {
        let mut found = TurnFiles::default();
        for diff in diffs {
            let at = |path: &str| format!("{}/{path}", diff.root.trim_end_matches('/'));
            for file in diff.files {
                let listed = |paths: &BTreeSet<String>| {
                    paths.contains(&at(&file.path))
                        || (file.from.as_deref()).is_some_and(|from| paths.contains(&at(from)))
                };
                if listed(ours) {
                    found.files.push(TurnFile {
                        root: diff.root.clone(),
                        path: file.path,
                        from: file.from,
                        added: file.added,
                        removed: file.removed,
                    });
                } else if !listed(theirs) {
                    found.external += 1;
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
