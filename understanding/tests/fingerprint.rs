//! Seam 2 (#328 and #369, Testing Decisions): the worktree fingerprint against real `git` in
//! temporary repositories, as the engine's tests do (`src/lib.rs`,
//! `retention_checks_every_blob_of_a_snapshot`).

use std::{fs, path::Path, process::Command};
use studio_core::Base;
use studio_understanding::Fingerprints;

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

/// The diff of the worktree that holds `folder` since its previous fingerprint.
fn diff(prints: &mut Fingerprints, folder: &Path) -> Option<String> {
    prints.take(folder).map(|change| change.diff)
}

/// A repository with `lib.rs` committed and `target/` ignored, fingerprinted once. The first record
/// has nothing to compare with, so it reports no change.
fn repo() -> (tempfile::TempDir, Fingerprints) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
    fs::write(dir.path().join("lib.rs"), "fn a() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "Start"]);
    let mut prints = Fingerprints::default();
    assert_eq!(diff(&mut prints, dir.path()), None);
    (dir, prints)
}

#[test]
fn editing_a_tracked_file_is_a_change_shown_as_its_diff() {
    let (dir, mut prints) = repo();

    fs::write(dir.path().join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();

    let change = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n fn a() {}\n+fn b() {}\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(change));
    assert_eq!(diff(&mut prints, dir.path()), None);
}

#[test]
fn adding_an_untracked_file_is_a_change_shown_as_its_content() {
    let (dir, mut prints) = repo();

    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/new.rs"), "fn c() {}\n").unwrap();

    let change = "--- a/src/new.rs\n+++ b/src/new.rs\n@@ -0,0 +1 @@\n+fn c() {}\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(change));
}

#[test]
fn changing_an_ignored_file_is_no_change() {
    let (dir, mut prints) = repo();

    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target/build.log"), "built\n").unwrap();

    assert_eq!(diff(&mut prints, dir.path()), None);
}

#[test]
fn committing_a_changed_file_is_no_change() {
    let (dir, mut prints) = repo();
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();
    assert!(diff(&mut prints, dir.path()).is_some());

    git(dir.path(), &["commit", "-qam", "Rename a to b"]);

    assert_eq!(diff(&mut prints, dir.path()), None);
}

/// #369, decision 10: a commit within the prompt counts, though `git status` lists the file in
/// neither fingerprint.
#[test]
fn an_edit_then_a_commit_within_one_prompt_is_the_change() {
    let (dir, mut prints) = repo();
    let start = git(dir.path(), &["rev-parse", "HEAD"]);

    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();
    git(dir.path(), &["commit", "-qam", "Rename a to b"]);

    let change = prints.take(dir.path()).expect("a change");
    let renamed = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn b() {}\n";
    assert_eq!(change.diff, renamed);
    assert_eq!(
        change.base,
        Base {
            head: Some(start),
            files: vec![]
        }
    );
}

#[test]
fn a_second_change_covers_only_what_changed_since_the_first() {
    let (dir, mut prints) = repo();
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();
    assert!(diff(&mut prints, dir.path()).is_some());
    let first = git(dir.path(), &["rev-parse", "HEAD"]);

    git(dir.path(), &["commit", "-qam", "Rename a to b"]);
    fs::write(dir.path().join("new.rs"), "fn c() {}\n").unwrap();

    let change = prints.take(dir.path()).expect("a change");
    let added = "--- a/new.rs\n+++ b/new.rs\n@@ -0,0 +1 @@\n+fn c() {}\n";
    assert_eq!(change.diff, added);
    // The base is the first change's end: `lib.rs` edited and not yet committed.
    assert_eq!(
        change.base,
        Base {
            head: Some(first),
            files: vec!["lib.rs".into()]
        }
    );
}

#[test]
fn two_worktrees_each_give_their_own_change() {
    let (dir, mut prints) = repo();
    let outside = tempfile::tempdir().unwrap();
    let worker = outside.path().join("worker");
    git(
        dir.path(),
        &["worktree", "add", "-q", worker.to_str().unwrap()],
    );
    assert_eq!(diff(&mut prints, &worker), None);
    assert_eq!(diff(&mut prints, dir.path()), None);

    fs::write(dir.path().join("lib.rs"), "fn main_side() {}\n").unwrap();
    fs::write(worker.join("lib.rs"), "fn worker_side() {}\n").unwrap();
    git(&worker, &["commit", "-qam", "Worker side"]);

    let main = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn main_side() {}\n";
    let side = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn worker_side() {}\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(main));
    assert_eq!(diff(&mut prints, &worker).as_deref(), Some(side));
}

#[test]
fn reverting_a_changed_file_is_a_change_back() {
    let (dir, mut prints) = repo();
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();
    assert!(diff(&mut prints, dir.path()).is_some());

    git(dir.path(), &["checkout", "-q", "--", "lib.rs"]);

    let change = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn b() {}\n+fn a() {}\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(change));
}

#[test]
fn a_session_in_a_subfolder_shares_its_worktree_record() {
    let (dir, mut prints) = repo();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/new.rs"), "fn c() {}\n").unwrap();

    let change = "--- a/src/new.rs\n+++ b/src/new.rs\n@@ -0,0 +1 @@\n+fn c() {}\n";
    assert_eq!(
        diff(&mut prints, &dir.path().join("src")).as_deref(),
        Some(change)
    );
    assert_eq!(diff(&mut prints, dir.path()), None);
}

#[test]
fn a_file_over_1_mib_is_compared_by_size_and_time_and_named_without_its_diff() {
    let (dir, mut prints) = repo();
    let dump = dir.path().join("dump.txt");
    fs::write(&dump, "row\n".repeat(300_000)).unwrap();

    let named = "Large file dump.txt changed\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(named));
    assert_eq!(diff(&mut prints, dir.path()), None);

    fs::write(&dump, "row\n".repeat(300_001)).unwrap();

    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(named));
}

#[test]
fn a_folder_outside_git_has_no_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    let mut prints = Fingerprints::default();
    assert_eq!(diff(&mut prints, dir.path()), None);

    fs::write(dir.path().join("notes.txt"), "a\n").unwrap();

    assert_eq!(diff(&mut prints, dir.path()), None);
}
