//! Seam 2 (danielrispler/Falinks#461, Testing Decisions) for the backfill (danielrispler/Falinks#486): the hook's tree of a worktree, and the
//! diff between two of them, against real `git` in a temporary repository.

use std::{fs, path::Path, process::Command};
use studio_core::{Backfill, CardRequest, Trees};
use studio_hook::record_tree;
use studio_understanding::backfilled;

/// Runs `git` in `repo`, away from Daniel's own git config, and returns its output.
fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim_end().into()
}

/// The working tree's files, by path, with their content.
type Files = Vec<(String, Vec<u8>)>;

/// What recording a tree must leave as it was: `git status`, the index and the working tree.
fn state(repo: &Path) -> (String, Vec<u8>, Files) {
    let status = git(repo, &["status", "--porcelain", "--untracked-files=all"]);
    let index = fs::read(repo.join(".git/index")).unwrap();
    let mut files = Vec::new();
    let mut folders = vec![repo.to_owned()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.ends_with(".git") {
                continue;
            }
            if path.is_dir() {
                folders.push(path);
            } else {
                files.push((path.display().to_string(), fs::read(&path).unwrap()));
            }
        }
    }
    files.sort();
    (status, index, files)
}

#[test]
fn recording_a_tree_changes_nothing_and_two_trees_diff_to_the_turns_change() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    fs::write(repo.join("lib.rs"), "fn a() {}\n").unwrap();
    fs::write(repo.join("gone.rs"), "fn g() {}\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "Start"]);
    // A staged edit, so the index differs from `HEAD` and from the working tree.
    fs::write(repo.join("lib.rs"), "fn a() {}\n// staged\n").unwrap();
    git(repo, &["add", "lib.rs"]);
    fs::write(repo.join("lib.rs"), "fn a() {}\n").unwrap();

    let before = state(repo);
    let (root, start) = record_tree(repo).expect("a tree");
    assert_eq!(state(repo), before);

    fs::write(repo.join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();
    fs::write(repo.join("new.rs"), "fn c() {}\n").unwrap();
    fs::remove_file(repo.join("gone.rs")).unwrap();
    fs::create_dir(repo.join("target")).unwrap();
    fs::write(repo.join("target/build.log"), "built\n").unwrap();
    let before = state(repo);
    let (_, end) = record_tree(&repo.join("target")).expect("a tree");
    assert_eq!(state(repo), before);

    let request = backfilled(Backfill {
        request: CardRequest::default(),
        trees: vec![Trees { root, start, end }],
    })
    .expect("a change");
    let change = concat!(
        "--- a/gone.rs\n+++ b/gone.rs\n@@ -1 +0,0 @@\n-fn g() {}\n",
        "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n fn a() {}\n+fn b() {}\n",
        "--- a/new.rs\n+++ b/new.rs\n@@ -0,0 +1 @@\n+fn c() {}\n",
    );
    assert_eq!(request.change.as_deref(), Some(change));
}

#[test]
fn two_equal_trees_give_no_card() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("lib.rs"), "fn a() {}\n").unwrap();
    let (root, start) = record_tree(dir.path()).expect("a tree");
    let end = start.clone();

    let trees = vec![Trees { root, start, end }];
    let request = CardRequest::default();
    assert_eq!(backfilled(Backfill { request, trees }), None);
}
