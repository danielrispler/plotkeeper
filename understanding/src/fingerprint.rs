//! The worktree fingerprint (#328, module 5; #369, decision 10): `HEAD`, and the files `git status
//! --porcelain` lists in a worktree, with their content. The worktree changed when a file listed
//! before or now, or one a commit since changed, has other content, or now exists or is gone.
//! Ignored files are never listed, and a commit alone leaves every file's content as it was. A file
//! over 1 MiB is compared by size and modification time, and its diff is one line that names it, so
//! neither the record nor the card writer gets it.

use similar::TextDiff;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};
use studio_core::{Base, Change};

/// A worktree's `HEAD`, and the files `git status` listed, by path from the worktree's root, with
/// their content: `None` for a file that is gone.
struct Record {
    head: Option<String>,
    files: BTreeMap<String, Option<Content>>,
}

/// A file's content, or a large file's size and modification time.
#[derive(Clone, PartialEq)]
enum Content {
    Bytes(Vec<u8>),
    Large(u64, Option<SystemTime>),
}

/// The size above which a file counts as large: 1 MiB.
const LARGE: usize = 1 << 20;

/// The file at `path`, or `None` when it is gone.
fn read(path: &Path) -> Option<Content> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > LARGE as u64 {
        return Some(Content::Large(meta.len(), meta.modified().ok()));
    }
    fs::read(path).ok().map(Content::Bytes)
}

/// Each worktree's latest record, by the worktree's root.
#[derive(Default)]
pub struct Fingerprints {
    records: HashMap<PathBuf, Record>,
}

impl Fingerprints {
    /// Records the worktree that holds `folder` and returns its change since its previous record,
    /// which is the change's base. `None` when nothing changed, when this is the worktree's first
    /// record, or when `folder` is in no git worktree.
    pub fn take(&mut self, folder: &Path) -> Option<Change> {
        let root = git(folder, &["rev-parse", "--show-toplevel"])?;
        let root = PathBuf::from(String::from_utf8(root).ok()?.trim_end());
        let head = git(&root, &["rev-parse", "HEAD"]);
        let head = head.and_then(|sha| Some(String::from_utf8(sha).ok()?.trim_end().to_owned()));
        // Untracked files one by one rather than their folders; a rename as a deletion and an addition.
        let status = [
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ];
        // Each entry is `XY path`, with the path from the root.
        let files = split_paths(&git(&root, &status)?, 3)
            .map(|path| {
                let content = read(&root.join(&path));
                (path, content)
            })
            .collect();
        let record = Record { head, files };
        let previous = self.records.insert(root.clone(), record)?;
        let record = &self.records[&root];
        // A file edited and committed since lists in neither record.
        let committed = match (&previous.head, &record.head) {
            (Some(before), Some(now)) if before != now => {
                let names = ["diff", "--name-only", "-z", "--no-renames", before, now];
                git(&root, &names).map_or_else(Vec::new, |out| split_paths(&out, 0).collect())
            }
            _ => Vec::new(),
        };
        let listed = previous.files.keys().chain(record.files.keys());
        let paths: BTreeSet<&String> = listed.chain(&committed).collect();
        let diff: String = paths
            .into_iter()
            .filter_map(|path| {
                // A file the previous record does not list was clean then: as its `HEAD` had it.
                let before = previous.files.get(path).cloned().unwrap_or_else(|| {
                    let head = previous.head.as_deref()?;
                    git(&root, &["show", &format!("{head}:{path}")]).map(Content::Bytes)
                });
                let after = record.files.get(path).cloned();
                let after = after.unwrap_or_else(|| read(&root.join(path)));
                (before != after).then(|| unified(path, before, after))
            })
            .collect();
        let base = Base {
            head: previous.head,
            files: previous.files.into_keys().collect(),
        };
        (!diff.is_empty()).then_some(Change { base, diff })
    }
}

/// The paths in `git`'s `-z` output, each after `skip` bytes.
fn split_paths(out: &[u8], skip: usize) -> impl Iterator<Item = String> {
    let entries = out.split(|&byte| byte == 0);
    let entries = entries.filter_map(move |entry| entry.get(skip..).filter(|p| !p.is_empty()));
    entries.map(|path| String::from_utf8_lossy(path).into_owned())
}

/// `git`'s output in `folder`, or `None` when it fails. It takes no optional locks, so it never
/// holds up the agent's own `git` commands in the same worktree.
fn git(folder: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(folder)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

/// One file's change as a unified diff, as `git diff` shows it.
fn unified(path: &str, before: Option<Content>, after: Option<Content>) -> String {
    // A large committed file reaches here as its bytes, from `git show`.
    let bytes = |content: Option<Content>| match content {
        None => Some(Vec::new()),
        Some(Content::Bytes(bytes)) if bytes.len() <= LARGE => Some(bytes),
        Some(_) => None,
    };
    let (Some(before), Some(after)) = (bytes(before), bytes(after)) else {
        return format!("Large file {path} changed\n");
    };
    let (Ok(before), Ok(after)) = (str::from_utf8(&before), str::from_utf8(&after)) else {
        return format!("Binary file {path} changed\n");
    };
    TextDiff::from_lines(before, after)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}
