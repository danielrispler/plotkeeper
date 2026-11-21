//! Seam 2 (#164, Testing Decisions): Pull main with real `git` in temporary repositories that have a bare
//! `origin`, after `tasks.rs`.

use std::path::Path;
use studio_workbench::{create_task, pull};

/// `origin`'s `trunk` gets one more commit, from the second clone; returns its sha.
fn merged_upstream(repos: &Origin, file: Option<&str>) -> String {
    if let Some(file) = file {
        std::fs::write(repos.seed.join(file), "theirs\n").unwrap();
        git(&repos.seed, &["add", file]);
        git(&repos.seed, &["commit", "-q", "-m", "Theirs"]);
    } else {
        git(
            &repos.seed,
            &["commit", "-q", "--allow-empty", "-m", "Theirs"],
        );
    }
    git(&repos.seed, &["push", "-q", "origin", "trunk"]);
    git(&repos.seed, &["rev-parse", "HEAD"])
}

#[test]
fn a_pull_fast_forwards_the_default_branch_checked_out_in_the_main_checkout() {
    let repos = origin();
    let tip = merged_upstream(&repos, None);

    pull(&repos.main).unwrap();

    assert_eq!(git(&repos.main, &["rev-parse", "trunk"]), tip);
    assert_eq!(git(&repos.main, &["rev-parse", "HEAD"]), tip);
}

#[test]
fn a_pull_updates_the_default_branch_while_another_branch_is_checked_out() {
    let repos = origin();
    git(&repos.main, &["switch", "-q", "-c", "work"]);
    git(
        &repos.main,
        &["commit", "-q", "--allow-empty", "-m", "Mine"],
    );
    let mine = git(&repos.main, &["rev-parse", "HEAD"]);
    let tip = merged_upstream(&repos, None);

    pull(&repos.main).unwrap();

    assert_eq!(git(&repos.main, &["rev-parse", "trunk"]), tip);
    assert_eq!(git(&repos.main, &["branch", "--show-current"]), "work");
    assert_eq!(git(&repos.main, &["rev-parse", "work"]), mine);
}

#[test]
fn a_diverged_default_branch_is_refused_and_left_as_it_was() {
    let repos = origin();
    git(
        &repos.main,
        &["commit", "-q", "--allow-empty", "-m", "Mine"],
    );
    let mine = git(&repos.main, &["rev-parse", "HEAD"]);
    merged_upstream(&repos, None);

    let error = pull(&repos.main).unwrap_err();
    assert!(!error.is_empty());
    assert_eq!(git(&repos.main, &["rev-parse", "trunk"]), mine);

    // The same when another branch is checked out.
    git(&repos.main, &["switch", "-q", "-c", "work"]);
    assert!(pull(&repos.main).is_err());
    assert_eq!(git(&repos.main, &["rev-parse", "trunk"]), mine);
}

#[test]
fn uncommitted_changes_in_the_way_refuse_the_pull_and_stay() {
    let repos = origin();
    let before = git(&repos.main, &["rev-parse", "trunk"]);
    merged_upstream(&repos, Some("a.txt"));
    std::fs::write(repos.main.join("a.txt"), "mine\n").unwrap();

    let error = pull(&repos.main).unwrap_err();

    assert!(!error.is_empty());
    assert_eq!(git(&repos.main, &["rev-parse", "trunk"]), before);
    assert_eq!(
        std::fs::read_to_string(repos.main.join("a.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn a_task_branch_is_left_untouched() {
    let repos = origin();
    let task = create_task(&repos.main, "t").unwrap();
    let before = git(Path::new(&task.path), &["rev-parse", "HEAD"]);
    merged_upstream(&repos, None);

    pull(&repos.main).unwrap();

    assert_eq!(git(Path::new(&task.path), &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&repos.main, &["rev-parse", "t"]), before);
}

use studio_test_support::{Origin, git, origin};
use studio_workbench::git_state;

#[test]
fn only_changes_the_incoming_commits_touch_would_be_overwritten() {
    let repos = origin();
    let main = repos.main.display().to_string();
    // The state Daniel's checkout is always in: an untracked folder, and an edit elsewhere.
    std::fs::create_dir_all(repos.main.join(".claude/worktrees")).unwrap();
    std::fs::write(repos.main.join(".claude/worktrees/x"), "x\n").unwrap();
    std::fs::write(repos.main.join("other.txt"), "mine\n").unwrap();
    merged_upstream(&repos, Some("a.txt"));
    git(&repos.main, &["fetch", "-q", "origin"]);

    let state = git_state(&repos.main, &main).unwrap();
    assert!(state.changed > 0 && !state.would_overwrite);
    pull(&repos.main).unwrap();

    // An untracked file, and a tracked edit, that the incoming commit also adds or edits.
    merged_upstream(&repos, Some("b.txt"));
    git(&repos.main, &["fetch", "-q", "origin"]);
    std::fs::write(repos.main.join("b.txt"), "mine\n").unwrap();
    assert!(git_state(&repos.main, &main).unwrap().would_overwrite);
}

#[test]
fn a_tracked_edit_the_incoming_commits_touch_would_be_overwritten() {
    let repos = origin();
    let main = repos.main.display().to_string();
    merged_upstream(&repos, Some("a.txt"));
    git(&repos.seed, &["pull", "-q", "origin", "trunk"]);
    git(&repos.main, &["pull", "-q", "--ff-only"]);
    std::fs::write(repos.seed.join("a.txt"), "theirs 2\n").unwrap();
    git(&repos.seed, &["commit", "-q", "-am", "Again"]);
    git(&repos.seed, &["push", "-q", "origin", "trunk"]);
    git(&repos.main, &["fetch", "-q", "origin"]);
    std::fs::write(repos.main.join("a.txt"), "mine\n").unwrap();

    assert!(git_state(&repos.main, &main).unwrap().would_overwrite);
}

#[test]
fn a_non_ascii_path_the_incoming_commits_touch_would_be_overwritten() {
    let repos = origin();
    let main = repos.main.display().to_string();
    merged_upstream(&repos, Some("é.txt"));
    git(&repos.main, &["fetch", "-q", "origin"]);
    std::fs::write(repos.main.join("é.txt"), "mine\n").unwrap();

    assert!(git_state(&repos.main, &main).unwrap().would_overwrite);
}

#[test]
fn a_rename_away_from_a_path_the_incoming_commits_touch_would_be_overwritten() {
    let repos = origin();
    let main = repos.main.display().to_string();
    merged_upstream(&repos, Some("a.txt"));
    git(&repos.seed, &["pull", "-q", "origin", "trunk"]);
    git(&repos.main, &["pull", "-q", "--ff-only"]);
    std::fs::write(repos.seed.join("a.txt"), "theirs 2\n").unwrap();
    git(&repos.seed, &["commit", "-q", "-am", "Again"]);
    git(&repos.seed, &["push", "-q", "origin", "trunk"]);
    git(&repos.main, &["fetch", "-q", "origin"]);
    git(&repos.main, &["mv", "a.txt", "renamed.txt"]);

    assert!(git_state(&repos.main, &main).unwrap().would_overwrite);
}
