//! The git calls on a turn's trees (danielrispler/plotkeeper#111): the hook records a tree of each
//! worktree a turn touches, at its start and end, and these read them.
//!
//! - `numstat`: the line counts of the files that differ between two trees. A rename is one row; a
//!   binary file has no counts.
//! - `hunk`: one file's diff between two trees, which file rows, cards and the diff pane reuse.
//! - `anchor`: a ref under `refs/plotkeeper/turns/` on each tree of an ended turn. The hook's trees
//!   are loose, unreferenced objects, which `git gc` prunes after two weeks (at once with
//!   `--prune=now`); a ref keeps the tree and every blob it holds, so a turn's hunks stay readable.

use crate::fingerprint::{git, is_own, names, reflog};
use std::sync::Mutex;
use std::{fs, path::Path};
use studio_core::{
    Facts, Numstat, RootDiff, Studio, TurnDiff, TurnFiles, TurnFilesSnapshot, compute_turn_files,
};

/// The refs' namespace.
const REFS: &str = "refs/plotkeeper/turns";

/// The most files one worktree's diff of a turn may hold; a diff of more is skipped, so a leftover
/// build directory cannot cost minutes of rename detection or fill the session list.
pub const MAX_TURN_FILES: usize = 2_000;

/// The files that differ between trees `start` and `end` of the worktree at `root`, with their
/// line counts; `None` when `git` fails, such as for a tree that was pruned.
pub fn numstat(root: &Path, start: &str, end: &str) -> Option<Vec<Numstat>> {
    let args = ["diff", "--numstat", "-z", "-M", "--no-ext-diff", start, end];
    let out = String::from_utf8(git(root, &args)?).ok()?;
    Some(parse(&out))
}

/// How many files differ between trees `start` and `end`, counted with no rename detection, which
/// is what costs on a large diff; `None` when `git` fails.
fn changed(root: &Path, start: &str, end: &str) -> Option<usize> {
    let args = [
        "diff",
        "--name-only",
        "-z",
        "--no-renames",
        "--no-ext-diff",
        start,
        end,
    ];
    Some(git(root, &args)?.iter().filter(|b| **b == 0).count())
}

/// Parses `--numstat -z`: `added TAB removed TAB path NUL`, and for a rename an empty path, then
/// the old path and the new one, each ending in NUL.
fn parse(out: &str) -> Vec<Numstat> {
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut files = Vec::new();
    while let Some(record) = fields.next() {
        let mut parts = record.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (added, removed) = (added.parse().ok(), removed.parse().ok());
        let (path, from) = match path {
            "" => match (fields.next(), fields.next()) {
                (Some(from), Some(to)) => (to.to_owned(), Some(from.to_owned())),
                _ => continue,
            },
            path => (path.to_owned(), None),
        };
        files.push(Numstat {
            path,
            from,
            added,
            removed,
        });
    }
    files
}

/// `path`'s diff between trees `start` and `end`, as `git diff` shows it; `from` is a renamed
/// file's old path. `None` when `git` fails; empty when the file is the same in both.
pub fn hunk(root: &Path, start: &str, end: &str, path: &str, from: Option<&str>) -> Option<String> {
    let mut args = vec![
        "diff",
        "-M",
        "--no-ext-diff",
        "--no-color",
        start,
        end,
        "--",
    ];
    args.extend(from);
    args.push(path);
    String::from_utf8(git(root, &args)?).ok()
}

/// How many lines of a hunk the rail shows.
const SHOWN: usize = 40;

/// A file's hunk as the rail shows it: the first `SHOWN` lines of `git diff`, how many it left out, and where the
/// first change is in the file's new text.
#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Hunk {
    pub text: String,
    pub left_out: usize,
    /// A binary file has a line saying so and no lines to show.
    pub binary: bool,
    /// The line of the first changed line in the new file; 1 when there is none.
    pub first_line: u32,
}

/// Shapes a file's `git diff` for the rail; `all` (the diff pane) keeps every line.
pub fn hunk_view(diff: &str, all: bool) -> Hunk {
    let shown = if all { usize::MAX } else { SHOWN };
    let lines: Vec<&str> = diff.lines().collect();
    let binary = lines.iter().any(|l| l.starts_with("Binary files "));
    if binary {
        return Hunk {
            text: String::new(),
            left_out: 0,
            binary,
            first_line: 1,
        };
    }
    Hunk {
        text: lines
            .iter()
            .take(shown)
            .fold(String::new(), |t, l| t + l + "\n"),
        left_out: lines.len().saturating_sub(shown),
        binary,
        first_line: first_changed(&lines),
    }
}

/// The new file's line of the first `+` or `-` line after the first `@@ -a,b +c,d @@`.
fn first_changed(lines: &[&str]) -> u32 {
    let Some(at) = lines.iter().position(|l| l.starts_with("@@")) else {
        return 1;
    };
    let start = lines[at]
        .split_whitespace()
        .find_map(|f| f.strip_prefix('+'))
        .and_then(|f| f.split(',').next())
        .and_then(|n| n.parse::<u32>().ok())
        .unwrap_or(1);
    let context = lines[at + 1..]
        .iter()
        .take_while(|l| !l.starts_with(['+', '-']))
        .count();
    start.max(1) + context as u32
}

/// The hunk of `path` between a turn's trees, whole when `all`: `end` is `None` while the turn runs, then the worktree's tree now.
pub fn turn_hunk(
    root: &Path,
    start: &str,
    end: Option<&str>,
    path: &str,
    from: Option<&str>,
    all: bool,
) -> Option<Hunk> {
    let now;
    let end = match end {
        Some(end) => end,
        None => {
            now = studio_hook::record_tree(root)?.1;
            &now
        }
    };
    hunk(root, start, end, path, from).map(|diff| hunk_view(&diff, all))
}

/// Keeps `tree` of the worktree at `root` from `git gc`, under `session`'s turn `turn`; `side` is
/// `start` or `end`.
pub fn anchor(root: &Path, session: &str, turn: usize, side: &str, tree: &str) -> Option<()> {
    let name = format!("{REFS}/{session}/{turn}/{:08x}-{side}", hash(root));
    git(root, &["update-ref", &name, tree]).map(drop)
}

/// FNV-1a of a worktree's root, so that worktrees of one repository, which share its refs, each
/// get their own.
fn hash(root: &Path) -> u32 {
    let bytes = root.as_os_str().as_encoded_bytes();
    bytes.iter().fold(0x811c_9dc5, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(0x0100_0193)
    })
}

/// The numstat of each worktree `ask` names, for the core's `set_turn_files`: `None` when none
/// could be diffed, or every one held more than `MAX_TURN_FILES` files, which `warn` hears of (with
/// no file's path). A running turn's end is the worktree's tree now. An ended turn's trees are
/// anchored first, so they outlive `git gc`.
pub fn turn_diffs(ask: &TurnDiff, warn: &mut dyn FnMut(String)) -> Option<Vec<RootDiff>> {
    let safe = studio_hook::answer::is_file_safe(&ask.session_id);
    let diffs: Vec<RootDiff> = ask
        .trees
        .iter()
        .filter_map(|trees| {
            let root = Path::new(&trees.root);
            let end = match &trees.end {
                Some(end) => end.clone(),
                None => studio_hook::record_tree(root)?.1,
            };
            if ask.ended && safe {
                for (side, tree) in [("start", &trees.start), ("end", &end)] {
                    anchor(root, &ask.session_id, ask.turn, side, tree);
                }
            }
            let count = changed(root, &trees.start, &end)?;
            if count > MAX_TURN_FILES {
                warn(format!(
                    "turn {} of session {}: {count} changed files, over the cap of {MAX_TURN_FILES}; its files are skipped",
                    ask.turn, ask.session_id
                ));
                return None;
            }
            let files = numstat(root, &trees.start, &end)?;
            let moved = moved(root, ask.window);
            Some(RootDiff {
                root: trees.root.clone(),
                files,
                moved,
            })
        })
        .collect();
    (!diffs.is_empty()).then_some(diffs)
}

/// The paths that moves of `HEAD` in the worktree at `root` changed within `window`, in
/// milliseconds: a merge, a pull, a rebase, a checkout or a reset brought them in, and the agent's
/// own commits did not. The reflog dates each move (danielrispler/plotkeeper#208).
pub fn moved(root: &Path, window: (u64, u64)) -> Vec<String> {
    let log = reflog(root).and_then(|log| fs::read(log).ok());
    let log = String::from_utf8_lossy(log.as_deref().unwrap_or_default()).into_owned();
    let (since, until) = (window.0 / 1000, window.1 / 1000);
    let mut paths = Vec::new();
    for line in log.lines() {
        // `old new Name <email> seconds zone TAB subject`
        let Some((who, subject)) = line.split_once('\t') else {
            continue;
        };
        let mut parts = who.rsplit(' ');
        let at = parts.nth(1).and_then(|secs| secs.parse::<u64>().ok());
        let mut shas = who.split(' ');
        let (Some(old), Some(new), Some(at)) = (shas.next(), shas.next(), at) else {
            continue;
        };
        let unborn = old.bytes().all(|byte| byte == b'0');
        if is_own(subject) || unborn || !(since..=until).contains(&at) {
            continue;
        }
        for path in names(root, Some(old), new) {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

/// Fills the hunk of each file in `facts` from the trees `ask` names, for the turn writer.
pub fn add_hunks(ask: &TurnDiff, facts: &mut Facts) {
    for fact in &mut facts.files {
        let file = &fact.file;
        let trees = ask.trees.iter().find(|t| t.root == file.root);
        let Some((trees, end)) = trees.and_then(|t| Some((t, t.end.as_deref()?))) else {
            continue;
        };
        let root = Path::new(&file.root);
        let hunk = hunk(root, &trees.start, end, &file.path, file.from.as_deref());
        fact.hunk = hunk.unwrap_or_default();
    }
}

/// `ask`'s worktrees diffed by `git` with the core unlocked. A turn skipped for holding too many
/// files is warned of through `warn` and settled, so no pass asks for it again.
pub fn diff_turn(
    state: &Mutex<Studio>,
    ask: &TurnDiff,
    warn: &mut dyn FnMut(String),
) -> Option<Vec<RootDiff>> {
    let mut skipped = false;
    let diffs = turn_diffs(ask, &mut |warning| {
        warn(warning);
        skipped = true;
    });
    if skipped && diffs.is_none() {
        state.lock().unwrap().settle_turn_files(ask);
    }
    diffs
}

/// The files of `ask` among `diffs`, found by `compute` with the core unlocked: the core is locked
/// only to copy out what the matching reads. `None` when git could not diff the turn.
pub fn found_files(
    state: &Mutex<Studio>,
    ask: &TurnDiff,
    diffs: Option<Vec<RootDiff>>,
    compute: impl FnOnce(&TurnFilesSnapshot, Vec<RootDiff>) -> TurnFiles,
) -> Option<TurnFiles> {
    let diffs = diffs?;
    let snapshot = state.lock().unwrap().turn_files_snapshot(ask, &diffs);
    Some(compute(&snapshot, diffs))
}

/// What the hooks and git recorded of the turn named `id`, for its summary writer
/// (danielrispler/plotkeeper#115): its files with their counts and hunks, and its checks. Empty for
/// a turn with no trees.
pub fn turn_facts(state: &Mutex<Studio>, id: &str, warn: &mut dyn FnMut(String)) -> Facts {
    let Some(ask) = state.lock().unwrap().summary_ask(id) else {
        return Facts::default();
    };
    let diffs = diff_turn(state, &ask, warn);
    let found = found_files(state, &ask, diffs, compute_turn_files);
    let mut facts = state.lock().unwrap().facts_of(&ask, found);
    add_hunks(&ask, &mut facts);
    facts
}

#[cfg(test)]
mod tests {
    use super::{hunk_view, parse};

    #[test]
    fn a_rename_is_one_row_and_a_binary_file_has_no_counts() {
        let out = "3\t1\tsrc/a.rs\0-\t-\timg.png\0\
                   0\t0\t\0old.rs\0new.rs\0";
        let files = parse(out);
        assert_eq!(files.len(), 3);
        assert_eq!((files[0].added, files[0].removed), (Some(3), Some(1)));
        assert_eq!((files[1].added, files[1].removed), (None, None));
        assert_eq!(files[2].path, "new.rs");
        assert_eq!(files[2].from.as_deref(), Some("old.rs"));
    }

    #[test]
    fn a_hunk_is_capped_and_knows_its_first_changed_line() {
        let mut diff =
            String::from("diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -9,3 +9,60 @@\n ctx\n ctx\n");
        diff.push_str(&"+new\n".repeat(55));
        let h = hunk_view(&diff, false);
        assert_eq!(h.text.lines().count(), 40);
        assert_eq!(h.left_out, 61 - 40);
        assert_eq!(h.first_line, 11);
        assert!(hunk_view("Binary files a/x and b/x differ\n", false).binary);
    }
}
