//! Commit mode (spec #477, "Reading a commit without checking it out"): a commit's tracked files from `git ls-tree`, and
//! the text of each of its regular files from one `git cat-file --batch`, with nothing checked out.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::Duration,
};
use studio_workbench::run::{RunError, git as git_in, run_until, run_until_with_input};

/// The longest one read of a commit may take.
const GIT_LIMIT: Duration = Duration::from_secs(30);

/// What a commit holds: the files it tracks, sorted, and the text of each regular file among them, its bytes that are not
/// UTF-8 read lossily. A symlink or a submodule is tracked but has no text, as on disk the scans read neither.
pub(crate) struct Snapshot {
    pub(crate) files: Vec<String>,
    pub(crate) texts: BTreeMap<String, String>,
}

/// Reads `commit` of the repository at `root`.
pub(crate) fn read(root: &Path, commit: &str) -> Result<Snapshot, String> {
    let id = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{commit}^{{commit}}"),
        ],
    )
    .map_err(|error| {
        format!(
            "{commit:?} is not a commit in {}; name one ({error})",
            root.display()
        )
    })?;
    let id = String::from_utf8_lossy(&id).trim().to_string();
    let listing = git(root, &["ls-tree", "-r", "-z", "--full-tree", &id])?;
    let mut files = BTreeSet::new();
    // Each regular file's blob and path, in the order `cat-file` answers.
    let mut blobs: Vec<(String, String)> = Vec::new();
    for entry in listing.split(|&b| b == 0).filter(|e| !e.is_empty()) {
        let entry = String::from_utf8_lossy(entry);
        let Some((head, path)) = entry.split_once('\t') else {
            continue;
        };
        let mut fields = head.split(' ');
        let (mode, oid) = (fields.next(), fields.nth(1));
        if let (Some("100644" | "100755"), Some(oid)) = (mode, oid) {
            blobs.push((oid.to_string(), path.to_string()));
        }
        files.insert(path.to_string());
    }
    let texts = cat_file(root, &blobs)?;
    Ok(Snapshot {
        files: files.into_iter().collect(),
        texts,
    })
}

/// The text of each blob in `blobs`, by its path, read through one `git cat-file --batch`.
fn cat_file(root: &Path, blobs: &[(String, String)]) -> Result<BTreeMap<String, String>, String> {
    let ids: String = blobs.iter().map(|(oid, _)| format!("{oid}\n")).collect();
    let output = run_until_with_input(
        git_in(root).args(["cat-file", "--batch"]),
        ids.as_bytes(),
        GIT_LIMIT,
    )
    .map_err(|error| failed(root, "cat-file", &error))?;
    let mut texts = BTreeMap::new();
    let mut rest = &output.stdout[..];
    for (_, path) in blobs {
        // `<oid> <type> <size>\n<bytes>\n`, or `<oid> missing\n`.
        let end = rest
            .iter()
            .position(|&b| b == b'\n')
            .ok_or("git cat-file ended early")?;
        let header = String::from_utf8_lossy(&rest[..end]).into_owned();
        rest = &rest[end + 1..];
        let Some(size) = header
            .split(' ')
            .nth(2)
            .and_then(|size| size.parse::<usize>().ok())
        else {
            continue;
        };
        let bytes = rest.get(..size).ok_or("git cat-file ended early")?;
        texts.insert(path.clone(), String::from_utf8_lossy(bytes).into_owned());
        rest = rest.get(size + 1..).unwrap_or_default();
    }
    Ok(texts)
}

/// The standard output of `git args` in `root`, or why it failed.
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = run_until(git_in(root).args(args), GIT_LIMIT)
        .map_err(|error| failed(root, args.first().copied().unwrap_or_default(), &error))?;
    Ok(output.stdout)
}

/// Why `git command` in `root` gave nothing to use.
fn failed(root: &Path, command: &str, error: &RunError) -> String {
    match error {
        RunError::Start(error) => format!("cannot run git in {}: {error}", root.display()),
        error => format!("git {command} failed in {}: {error}", root.display()),
    }
}
