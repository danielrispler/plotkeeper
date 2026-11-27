//! A turn's changed files (danielrispler/plotkeeper#111, #207): the diff between the git trees the
//! hook recorded at its start and end. Every file that differs is the turn's, but one that another
//! session's or turn's call names (`names`). The core runs no `git`: it names the trees to diff
//! (`TurnDiff`), and the shell hands back git's numstat (`RootDiff`).

use crate::ledger::Row;
use crate::studio::Roots;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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

/// A command row of a snapshot: its text, and the worktree roots it touched.
#[derive(Debug)]
struct Command {
    /// The row's place among the turn's own calls, which the first call to name a file is found by.
    seq: usize,
    text: String,
    roots: Vec<String>,
}

/// What the matching of a turn's files reads, copied out of the core in one short read so that the
/// matching itself runs with the core unlocked. Only the rows that can match are in it: the calls of
/// edit tools, indexed by the path each names inside a diffed worktree, and the commands.
#[derive(Debug, Default)]
pub struct TurnFilesSnapshot {
    /// Other calls' edits, by worktree root: the paths from the root they name.
    others_edits: HashMap<String, HashSet<String>>,
    others_commands: Vec<Command>,
    /// The turn's own edits, by worktree root: each path's first call, by its `seq`.
    ours_edits: HashMap<String, HashMap<String, usize>>,
    ours_commands: Vec<Command>,
    /// The name of the agent of each of the turn's own calls, by `seq`.
    ours_agents: Vec<String>,
    /// The name of the worker whose worktree each diffed root is, if a worker spawned by the turn
    /// has one.
    workers: HashMap<String, String>,
}

impl TurnFilesSnapshot {
    /// Reads the rows `others` and `ours` for the worktree `roots` of a diff. `agent` names a row's
    /// agent, and `worker` a root's worker.
    pub(crate) fn of<'a>(
        roots_of: &Roots,
        diff_roots: impl IntoIterator<Item = &'a str>,
        others: &[&Row],
        ours: &[&Row],
        agent: impl Fn(&Row) -> String,
        worker: impl Fn(&str) -> Option<String>,
    ) -> TurnFilesSnapshot {
        let diff_roots: Vec<&str> = diff_roots.into_iter().collect();
        let mut snapshot = TurnFilesSnapshot::default();
        for root in &diff_roots {
            if let Some(name) = worker(root) {
                snapshot.workers.insert(root.to_string(), name);
            }
        }
        // The path an edit names inside each root, found once per edit row.
        let inside = |row: &Row, root: &str| {
            let file = row.file.as_deref()?;
            roots_of.inside(root, file).map(str::to_string)
        };
        for row in others.iter().filter(|r| r.command.is_some() || r.edits()) {
            if row.edits() {
                for root in &diff_roots {
                    if let Some(path) = inside(row, root) {
                        (snapshot.others_edits.entry(root.to_string()).or_default()).insert(path);
                    }
                }
            } else if let Some(text) = &row.command {
                let (text, roots) = (text.clone(), row.roots.clone());
                snapshot.others_commands.push(Command {
                    seq: 0,
                    text,
                    roots,
                });
            }
        }
        for row in ours.iter().filter(|r| r.command.is_some() || r.edits()) {
            let seq = snapshot.ours_agents.len();
            snapshot.ours_agents.push(agent(row));
            if row.edits() {
                for root in &diff_roots {
                    if let Some(path) = inside(row, root) {
                        let paths = snapshot.ours_edits.entry(root.to_string()).or_default();
                        paths.entry(path).or_insert(seq);
                    }
                }
            } else if let Some(text) = &row.command {
                let (text, roots) = (text.clone(), row.roots.clone());
                snapshot.ours_commands.push(Command { seq, text, roots });
            }
        }
        snapshot
    }
}

/// The commands that can name a file of worktree `root`, once per root: those holding `root/`, which
/// can name it by its absolute path, and those that touched `root`, which can by its path from it.
struct Partition<'a> {
    absolute: Vec<&'a Command>,
    relative: Vec<&'a Command>,
}

impl<'a> Partition<'a> {
    fn of(commands: &'a [Command], root: &str, prefix: &str) -> Partition<'a> {
        Partition {
            absolute: (commands.iter())
                .filter(|c| c.text.contains(prefix))
                .collect(),
            relative: (commands.iter())
                .filter(|c| c.roots.iter().any(|r| r == root))
                .collect(),
        }
    }

    /// The first command to name `path` (`absolute` is its absolute path), by its `seq`. A root-level
    /// file's path from the root is its bare name, which a command names only in full.
    fn naming(&self, absolute: &str, path: &str) -> Option<usize> {
        let by_absolute = (self.absolute.iter()).find(|c| mentions(&c.text, absolute));
        let by_relative = if path.contains('/') {
            (self.relative.iter()).find(|c| mentions(&c.text, path))
        } else {
            None
        };
        [by_absolute, by_relative]
            .into_iter()
            .flatten()
            .map(|c| c.seq)
            .min()
    }
}

/// The files of `diffs` but those the other calls of `snapshot` name and no call of the turn does,
/// each with the agent of the call that names it, else its worktree's worker (`None` when neither
/// says). A file a move of `HEAD` brought in, which no call of the turn names, is counted in
/// `merged` instead. A file's rename counts by its old path too. An edit's path is the tool's own
/// spelling, which `Roots::inside` found inside the root through a symlink at the snapshot; a
/// command's text is taken as written, by `mentions`.
pub fn compute_turn_files(snapshot: &TurnFilesSnapshot, diffs: Vec<RootDiff>) -> TurnFiles {
    let mut found = TurnFiles::default();
    for diff in diffs {
        let root = diff.root.trim_end_matches('/');
        let prefix = format!("{root}/");
        let others = Partition::of(&snapshot.others_commands, &diff.root, &prefix);
        let ours = Partition::of(&snapshot.ours_commands, &diff.root, &prefix);
        let other_edits = snapshot.others_edits.get(&diff.root);
        let our_edits = snapshot.ours_edits.get(&diff.root);
        let moved: HashSet<&str> = diff.moved.iter().map(String::as_str).collect();
        for file in &diff.files {
            // A rename counts by its old path too.
            let paths = [Some(file.path.as_str()), file.from.as_deref()]
                .into_iter()
                .flatten();
            let paths: Vec<(&str, String)> = paths.map(|p| (p, format!("{prefix}{p}"))).collect();
            let named = paths.iter().any(|(p, abs)| {
                other_edits.is_some_and(|e| e.contains(*p)) || others.naming(abs, p).is_some()
            });
            let moved = paths.iter().any(|(p, _)| moved.contains(p));
            // The first of the turn's calls to name the file, by its path and then its old path.
            let calling = |(p, abs): &(&str, String)| {
                let edit = our_edits.and_then(|e| e.get(*p)).copied();
                [edit, ours.naming(abs, p)].into_iter().flatten().min()
            };
            let call = paths.iter().find_map(calling);
            if named && call.is_none() {
                continue;
            }
            if moved && call.is_none() {
                found.merged += 1;
                continue;
            }
            found.files.push(TurnFile {
                root: diff.root.clone(),
                path: file.path.clone(),
                from: file.from.clone(),
                added: file.added,
                removed: file.removed,
                agent: match call {
                    Some(seq) => Some(snapshot.ours_agents[seq].clone()),
                    None => snapshot.workers.get(&diff.root).cloned(),
                },
            });
        }
    }
    found
}

/// How long a running turn's files stay as they are before the shell is asked again.
pub(crate) const RUNNING_REFRESH_MS: u64 = 5_000;

/// Turns that ended longer ago than this get no files: their trees are likely gone.
pub(crate) const SINCE_MS: u64 = 7 * 24 * 60 * 60 * 1000;
