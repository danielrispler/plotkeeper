//! Seam 2 (#328 and #369, Testing Decisions): the worktree fingerprint against real `git` in
//! temporary repositories, as the engine's tests do (`src/lib.rs`,
//! `retention_checks_every_blob_of_a_snapshot`).

use std::{fs, path::Path, process::Command};
use studio_core::{Base, Purpose};
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

/// #369, decision 11: a worker's step looks for a commit. The change it sees stays in the next one
/// until `HEAD` moves, so a step that only edits does not eat the commit's card.
#[test]
fn a_commit_check_keeps_its_record_only_when_head_moved() {
    let (dir, mut prints) = repo();
    let start = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();

    let edited = prints.look(dir.path(), Purpose::Commit).expect("a change");
    assert_eq!(edited.head.as_deref(), Some(start.as_str()));

    git(dir.path(), &["commit", "-qam", "Rename a to b"]);
    let committed = prints.look(dir.path(), Purpose::Commit).expect("a change");

    let renamed = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn b() {}\n";
    assert_eq!(committed.diff, renamed);
    assert_eq!(committed.base.head, Some(start));
    assert_eq!(
        committed.head,
        Some(git(dir.path(), &["rev-parse", "HEAD"]))
    );
    assert_eq!(prints.look(dir.path(), Purpose::Commit), None);
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

/// Commits `file` with `content` on a new branch `branch`, then checks out the branch it left, and
/// fingerprints again so the base is past these moves.
fn branch(dir: &Path, prints: &mut Fingerprints, branch: &str, file: &str, content: &str) {
    git(dir, &["checkout", "-qb", branch]);
    fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-qm", branch]);
    git(dir, &["checkout", "-q", "-"]);
    prints.take(dir);
}

/// #434, decision 12: only the agent's own commits are its change. A move of `HEAD` by any other
/// command moves the base.
#[test]
fn a_fast_forward_pull_gives_no_change_and_moves_the_base() {
    let (dir, mut prints) = repo();
    branch(dir.path(), &mut prints, "upstream", "up.rs", "fn up() {}\n");

    git(dir.path(), &["pull", "-q", "--ff-only", ".", "upstream"]);
    assert_eq!(diff(&mut prints, dir.path()), None);

    // In one look: the pull moves the base, and the edit after it is the change.
    branch(dir.path(), &mut prints, "more", "more.rs", "fn more() {}\n");
    git(dir.path(), &["pull", "-q", "--ff-only", ".", "more"]);
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();
    let change = prints.take(dir.path()).expect("a change");
    let renamed = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn b() {}\n";
    assert_eq!(change.diff, renamed);
    assert_eq!(
        change.base.head,
        Some(git(dir.path(), &["rev-parse", "HEAD"]))
    );
}

#[test]
fn a_rebase_onto_another_branch_gives_no_change() {
    let (dir, mut prints) = repo();
    branch(
        dir.path(),
        &mut prints,
        "other",
        "other.rs",
        "fn other() {}\n",
    );
    fs::write(dir.path().join("mine.rs"), "fn mine() {}\n").unwrap();
    git(dir.path(), &["add", "mine.rs"]);
    git(dir.path(), &["commit", "-qm", "Mine"]);
    assert!(diff(&mut prints, dir.path()).is_some());

    git(dir.path(), &["rebase", "-q", "other"]);

    assert_eq!(diff(&mut prints, dir.path()), None);
}

#[test]
fn a_checkout_gives_no_change() {
    let (dir, mut prints) = repo();
    branch(
        dir.path(),
        &mut prints,
        "other",
        "other.rs",
        "fn other() {}\n",
    );

    git(dir.path(), &["checkout", "-q", "other"]);

    assert_eq!(diff(&mut prints, dir.path()), None);
}

#[test]
fn a_hard_reset_gives_no_change() {
    let (dir, mut prints) = repo();
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();
    git(dir.path(), &["commit", "-qam", "Rename a to b"]);
    assert!(diff(&mut prints, dir.path()).is_some());

    git(dir.path(), &["reset", "-q", "--hard", "HEAD~1"]);

    assert_eq!(diff(&mut prints, dir.path()), None);
}

/// #434, decision 12: a merge moves the base, so upstream work never shows as the agent's.
#[test]
fn a_clean_merge_gives_no_change() {
    let (dir, mut prints) = repo();
    branch(
        dir.path(),
        &mut prints,
        "other",
        "other.rs",
        "fn other() {}\n",
    );
    fs::write(dir.path().join("mine.rs"), "fn mine() {}\n").unwrap();
    git(dir.path(), &["add", "mine.rs"]);
    git(dir.path(), &["commit", "-qm", "Mine"]);
    assert!(diff(&mut prints, dir.path()).is_some());

    git(dir.path(), &["merge", "-q", "--no-edit", "other"]);

    assert_eq!(diff(&mut prints, dir.path()), None);
}

/// #434, decision 13: a conflicted merge's change is its resolution alone, as git's remerge diff
/// shows it.
#[test]
fn a_conflicted_merge_gives_only_its_resolution() {
    let (dir, mut prints) = repo();
    git(dir.path(), &["checkout", "-qb", "other"]);
    fs::write(dir.path().join("lib.rs"), "fn theirs() {}\n").unwrap();
    fs::write(dir.path().join("other.rs"), "fn other() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "Theirs"]);
    git(dir.path(), &["checkout", "-q", "-"]);
    fs::write(dir.path().join("lib.rs"), "fn ours() {}\n").unwrap();
    git(dir.path(), &["commit", "-qam", "Ours"]);
    assert!(diff(&mut prints, dir.path()).is_some());

    let merge = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(["merge", "-q", "other"])
        .current_dir(dir.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(!merge.status.success(), "the merge conflicts");
    fs::write(dir.path().join("lib.rs"), "fn resolved() {}\n").unwrap();
    git(dir.path(), &["add", "lib.rs"]);
    git(dir.path(), &["commit", "-q", "--no-edit"]);

    let change = diff(&mut prints, dir.path()).expect("the resolution");
    assert!(change.contains("+fn resolved() {}"), "{change}");
    assert!(!change.contains("other.rs"), "{change}");
}

/// #434, decision 14: the first commit of a new repository is all its files, also those that were
/// already there, untracked, at the base.
#[test]
fn a_first_commit_gives_every_committed_file() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    fs::write(dir.path().join("lib.rs"), "fn a() {}\n").unwrap();
    let mut prints = Fingerprints::default();
    assert_eq!(diff(&mut prints, dir.path()), None);

    fs::write(dir.path().join("main.rs"), "fn main() {}\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "Start"]);

    let lib = "--- a/lib.rs\n+++ b/lib.rs\n@@ -0,0 +1 @@\n+fn a() {}\n";
    let main = "--- a/main.rs\n+++ b/main.rs\n@@ -0,0 +1 @@\n+fn main() {}\n";
    assert_eq!(diff(&mut prints, dir.path()), Some(format!("{lib}{main}")));
}

#[test]
fn a_folder_that_becomes_a_repository_is_fingerprinted_from_then_on() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("lib.rs"), "fn a() {}\n").unwrap();
    let mut prints = Fingerprints::default();
    assert_eq!(diff(&mut prints, dir.path()), None);

    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "Start"]);
    assert_eq!(diff(&mut prints, dir.path()), None);
    fs::write(dir.path().join("lib.rs"), "fn b() {}\n").unwrap();

    let change = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn b() {}\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(change));
}

/// #568: files the operating system writes are not the agent's work, so a turn that changed only
/// them gets no card.
#[test]
fn changing_only_os_files_is_no_change() {
    let (dir, mut prints) = repo();

    fs::write(dir.path().join(".DS_Store"), [0u8, 1, 2]).unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/._lib.rs"), [0u8, 5]).unwrap();
    fs::write(dir.path().join("src/Thumbs.db"), [0u8, 3]).unwrap();
    fs::create_dir_all(dir.path().join(".Spotlight-V100/Store")).unwrap();
    fs::write(dir.path().join(".Spotlight-V100/Store/index"), "x\n").unwrap();
    fs::create_dir(dir.path().join(".Trashes")).unwrap();
    fs::write(dir.path().join(".Trashes/old"), "x\n").unwrap();

    assert_eq!(diff(&mut prints, dir.path()), None);
}

#[test]
fn a_change_leaves_out_the_os_files_beside_it() {
    let (dir, mut prints) = repo();

    fs::write(dir.path().join(".DS_Store"), [0u8, 1, 2]).unwrap();
    fs::write(dir.path().join("lib.rs"), "fn a() {}\nfn b() {}\n").unwrap();

    let change = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1,2 @@\n fn a() {}\n+fn b() {}\n";
    assert_eq!(diff(&mut prints, dir.path()).as_deref(), Some(change));
}

#[test]
fn committing_only_an_os_file_is_no_change() {
    let (dir, mut prints) = repo();

    fs::write(dir.path().join(".DS_Store"), [0u8, 1, 2]).unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "Oops"]);

    assert_eq!(diff(&mut prints, dir.path()), None);
}
