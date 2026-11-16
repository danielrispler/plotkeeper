//! A card's code, pulled from git (danielrispler/plotkeeper#121): the writer names a path and a line range, never the
//! lines, and the studio reads them from the turn's end tree.

use crate::fingerprint::git;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

/// The most lines a card shows of one range.
const SHOWN: usize = 12;

/// Lines `from` to `to`, counted from 1 and both included, of the file `path` in the card's end tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodeRange {
    pub path: String,
    pub from: usize,
    pub to: usize,
}

/// The code of `ranges`, each as its `path:from-to` line and then its lines, as the end trees `ends`, a tree by worktree
/// root, hold them. A range whose path is in none of the trees, or whose lines are not all in the file, is dropped, and
/// the others stay; a range is cut to its first `SHOWN` lines.
pub fn pull_code(ranges: &[CodeRange], ends: &BTreeMap<String, String>) -> String {
    let blocks = ranges.iter().filter_map(|range| {
        let CodeRange { path, from, to } = range;
        if *from == 0 || to < from {
            return None;
        }
        let file = ends.iter().find_map(|(root, tree)| {
            let blob = git(Path::new(root), &["show", &format!("{tree}:{path}")])?;
            String::from_utf8(blob).ok()
        })?;
        let lines: Vec<&str> = file.lines().collect();
        let shown = &lines.get(from - 1..*to)?[..(to - from + 1).min(SHOWN)];
        Some(format!("{path}:{from}-{to}\n{}", shown.join("\n")))
    });
    blocks.collect::<Vec<_>>().join("\n\n")
}
