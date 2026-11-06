//! Seam 2 (#328 and #461, Testing Decisions): creating and removing tasks (#468) with real `git`
//! in temporary repositories that have a bare `origin`.

use std::{path::Path, process::Command};
use studio_workbench::{
    Losses, Place, create_named_task, create_task, delete_branch_merged_at, delete_merged_branch,
    remove_task,
};

/// Runs `git` in `dir`, away from Daniel's own git config, and returns its trimmed output.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).trim().into()
}

/// A bare `origin` whose default branch is `trunk`, with two commits, and a clone of it whose
/// local `trunk` is one commit behind. Returns the folders, the clone's real path, and the two
/// commits.
struct Repos {
    _dir: tempfile::TempDir,
    origin: std::path::PathBuf,
    main: std::path::PathBuf,
    first: String,
    second: String,
}

fn repos() -> Repos {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let origin = root.join("origin.git");
    let seed = root.join("seed");
    let main = root.join("main");
    git(
        &root,
        &["init", "-q", "--bare", "-b", "trunk", "origin.git"],
    );
    git(&root, &["clone", "-q", "origin.git", "seed"]);
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "First"]);
    git(&seed, &["push", "-q", "origin", "trunk"]);
    git(&root, &["clone", "-q", "origin.git", "main"]);
    let first = git(&main, &["rev-parse", "HEAD"]);
    // `origin` moves on after the clone, so the clone's `trunk` is behind.
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "Second"]);
    git(&seed, &["push", "-q", "origin", "trunk"]);
    let second = git(&seed, &["rev-parse", "HEAD"]);
    Repos {
        _dir: dir,
        origin,
        main,
        first,
        second,
    }
}

fn task_path(repos: &Repos, name: &str) -> std::path::PathBuf {
    repos.main.join(".claude/worktrees").join(name)
}

#[test]
fn a_task_branches_from_origins_default_branch_after_a_fetch_even_when_local_is_behind() {
    let repos = repos();

    let task = create_task(&repos.main, "issue-1").unwrap();

    let path = task_path(&repos, "issue-1");
    assert_eq!(
        task,
        Place {
            path: path.display().to_string(),
            branch: Some("issue-1".into()),
        }
    );
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), repos.second);
    assert_eq!(git(&path, &["branch", "--show-current"]), "issue-1");
    // The local default branch is left where it was.
    assert_eq!(git(&repos.main, &["rev-parse", "trunk"]), repos.first);
}

#[test]
fn a_taken_name_is_refused_and_nothing_is_created() {
    let repos = repos();
    let main = &repos.main;
    git(main, &["branch", "here", "HEAD"]);
    let seed = repos.origin.with_file_name("seed");
    git(&seed, &["push", "-q", "origin", "HEAD:there"]);
    std::fs::create_dir_all(task_path(&repos, "folder")).unwrap();
    let worktrees = git(main, &["worktree", "list", "--porcelain"]);
    let branches = git(main, &["branch", "--list"]);

    for name in ["here", "there", "folder", "../escape"] {
        assert!(create_task(main, name).is_err(), "{name} was not refused");
    }

    assert_eq!(git(main, &["worktree", "list", "--porcelain"]), worktrees);
    assert_eq!(git(main, &["branch", "--list"]), branches);
    assert!(!task_path(&repos, "here").exists());
    assert!(!task_path(&repos, "there").exists());
    assert!(!main.join(".claude/escape").exists());
}

#[test]
fn a_named_task_takes_its_branch_from_the_prompt_the_spec_or_the_issue() {
    let repos = repos();
    let main = &repos.main;
    std::fs::create_dir_all(main.join("docs")).unwrap();
    std::fs::write(main.join("docs/Picker Spec.md"), "spec").unwrap();
    git(main, &["branch", "fix-the-pickers-outside-click", "HEAD"]);

    let branch = |request: &str| create_named_task(main, request).unwrap().branch.unwrap();

    // A prompt: its first words; a taken name gets the next free number.
    assert_eq!(
        branch("Fix the picker's outside click, then open a PR with every fix"),
        "fix-the-pickers-outside-click-2"
    );
    // A spec: its file's name.
    assert_eq!(branch("docs/Picker Spec.md"), "picker-spec");
    // An issue: its number, then its title, which `gh` cannot read here.
    assert_eq!(branch("#123"), "issue-123");
    assert_eq!(branch("!!!"), "session");
}

#[test]
fn removing_a_clean_pushed_task_removes_its_worktree_and_keeps_its_branch() {
    let repos = repos();
    let task = create_task(&repos.main, "done").unwrap();
    let path = Path::new(&task.path);
    git(path, &["commit", "-q", "--allow-empty", "-m", "Work"]);
    git(path, &["push", "-q", "origin", "done"]);
    let work = git(path, &["rev-parse", "HEAD"]);

    assert_eq!(remove_task(&repos.main, &task.path, false), Ok(None));

    assert!(!path.exists());
    assert_eq!(git(&repos.main, &["rev-parse", "done"]), work);
}

#[test]
fn uncommitted_changes_and_unpushed_commits_are_listed_and_kept_until_confirmed() {
    let repos = repos();
    let task = create_task(&repos.main, "busy").unwrap();
    let path = Path::new(&task.path);
    git(
        path,
        &["commit", "-q", "--allow-empty", "-m", "Unpushed work"],
    );
    let work = git(path, &["rev-parse", "--short", "HEAD"]);
    std::fs::write(path.join("notes.txt"), "draft").unwrap();

    let asked = remove_task(&repos.main, &task.path, false).unwrap();

    assert_eq!(
        asked,
        Some(Losses {
            uncommitted: vec!["?? notes.txt".into()],
            unpushed: vec![format!("{work} Unpushed work")],
        })
    );
    assert!(path.join("notes.txt").exists());

    // Confirmed, git still refuses to drop the uncommitted file, and says why.
    let refused = remove_task(&repos.main, &task.path, true).unwrap_err();
    assert!(refused.contains("untracked"), "{refused}");
    assert!(path.join("notes.txt").exists());

    // With the file gone, the confirmed removal goes ahead; the unpushed commit stays on the branch.
    std::fs::remove_file(path.join("notes.txt")).unwrap();
    assert_eq!(remove_task(&repos.main, &task.path, true), Ok(None));
    assert!(!path.exists());
    assert_eq!(git(&repos.main, &["rev-parse", "--short", "busy"]), work);
}

#[test]
fn a_merged_branch_is_deleted_and_an_unmerged_one_kept_with_gits_reason() {
    let repos = repos();
    git(&repos.main, &["pull", "-q", "origin", "trunk"]);
    let merged = create_task(&repos.main, "merged").unwrap();
    let kept = create_task(&repos.main, "kept").unwrap();
    git(
        Path::new(&kept.path),
        &["commit", "-q", "--allow-empty", "-m", "Not merged"],
    );
    let work = git(&repos.main, &["rev-parse", "kept"]);
    remove_task(&repos.main, &merged.path, true).unwrap();
    remove_task(&repos.main, &kept.path, true).unwrap();

    assert_eq!(delete_merged_branch(&repos.main, "merged"), Ok(()));
    let branches = git(&repos.main, &["branch", "--list", "merged"]);
    assert!(branches.is_empty(), "{branches}");

    let refused = delete_merged_branch(&repos.main, "kept").unwrap_err();
    assert!(refused.contains("not fully merged"), "{refused}");
    assert_eq!(git(&repos.main, &["rev-parse", "kept"]), work);
}

/// A branch merged by squash, which git refuses, goes when a merged pull request's head is its tip,
/// and stays when it moved on since.
#[test]
fn a_squash_merged_branch_goes_only_at_its_pull_requests_head() {
    let repos = repos();
    let task = create_task(&repos.main, "squashed").unwrap();
    git(
        Path::new(&task.path),
        &["commit", "-q", "--allow-empty", "-m", "Merged by squash"],
    );
    let head = git(&repos.main, &["rev-parse", "squashed"]);
    git(
        Path::new(&task.path),
        &["commit", "-q", "--allow-empty", "-m", "After the merge"],
    );
    remove_task(&repos.main, &task.path, true).unwrap();

    let refused = delete_branch_merged_at(&repos.main, "squashed", &[head]).unwrap_err();
    assert!(refused.contains("not fully merged"), "{refused}");

    let tip = git(&repos.main, &["rev-parse", "squashed"]);
    assert_eq!(
        delete_branch_merged_at(&repos.main, "squashed", &[tip]),
        Ok(())
    );
    let branches = git(&repos.main, &["branch", "--list", "squashed"]);
    assert!(branches.is_empty(), "{branches}");
}

/// No code path forces git (#461, decision 9): the studio's own sources, the workbench, the shell
/// and the UI, never spell git's force option.
#[test]
fn no_code_path_passes_force() {
    let studio = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut checked = 0;
    for dir in ["workbench/src", "src-tauri/src", "ui"] {
        for entry in std::fs::read_dir(studio.join(dir)).unwrap() {
            let file = entry.unwrap().path();
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("rs" | "ts")) {
                continue;
            }
            let source = std::fs::read_to_string(&file).unwrap();
            for force in ["--force", "\"-f\"", "'-f'"] {
                assert!(!source.contains(force), "{} has {force}", file.display());
            }
            checked += 1;
        }
    }
    assert!(checked > 5, "only {checked} files checked");
}
