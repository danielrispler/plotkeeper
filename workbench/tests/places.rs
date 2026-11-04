//! Seam 2 (#328 and #461, Testing Decisions): the picker's places from real `git` in temporary
//! repositories, as the engine's tests do (`src/lib.rs`, `retention_checks_every_blob_of_a_snapshot`).

use std::{path::Path, process::Command};
use studio_workbench::{Place, claude, places};

/// Runs `git` in `repo`, away from Daniel's own git config.
fn git(repo: &Path, args: &[&str]) {
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
}

/// A repository with one commit on `main`, at its real path, as `git` spells it.
fn repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().canonicalize().unwrap().display().to_string();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "Start"],
    );
    (dir, main)
}

fn place(path: &str, branch: Option<&str>) -> Place {
    Place {
        path: path.into(),
        branch: branch.map(Into::into),
    }
}

#[test]
fn worktrees_under_claude_worktrees_are_listed_and_agent_ones_hidden_and_counted() {
    let (dir, main) = repo();
    let add = |args: &[&str]| git(dir.path(), &[&["worktree", "add", "-q"], args].concat());
    add(&["-b", "issue-1", ".claude/worktrees/issue-1"]);
    add(&["-b", "agent-x", ".claude/worktrees/agent-a1b2c3"]);
    add(&["-b", "agent-y", ".claude/worktrees/agent-d4e5f6"]);
    add(&["--detach", ".claude/worktrees/look"]);
    // A worktree outside `.claude/worktrees/` is not a task.
    let elsewhere = tempfile::tempdir().unwrap();
    add(&[
        "-b",
        "elsewhere",
        elsewhere.path().join("w").to_str().unwrap(),
    ]);

    let found = places(dir.path()).unwrap();

    assert_eq!(found.main, place(&main, Some("main")));
    let tasks = format!("{main}/.claude/worktrees");
    assert_eq!(
        found.tasks,
        [
            place(&format!("{tasks}/issue-1"), Some("issue-1")),
            // A detached HEAD has no branch; the picker shows its folder.
            place(&format!("{tasks}/look"), None),
        ]
    );
    assert_eq!(found.hidden, 2);
}

#[test]
fn the_listing_from_inside_a_task_still_names_the_main_checkout() {
    let (dir, main) = repo();
    git(
        dir.path(),
        &["worktree", "add", "-q", "-b", "t", ".claude/worktrees/t"],
    );

    let found = places(&dir.path().join(".claude/worktrees/t")).unwrap();

    assert_eq!(found.main, place(&main, Some("main")));
    assert_eq!(found.tasks.len(), 1);
}

#[test]
fn a_folder_outside_git_is_an_error() {
    let dir = tempfile::tempdir().unwrap();

    assert!(places(dir.path()).is_err());
}

/// `claude` runs with the studio plugin alone, set the way the README loads it, so a profile that
/// also sets `CLAUDE_CODE_PLUGIN_DIRS` never loads its hooks twice.
#[test]
fn claude_starts_with_the_studio_plugin_and_resumes_by_session_id() {
    let plugin = Path::new("/r/apps/studio/plugin");

    assert_eq!(
        claude(plugin, None),
        [
            "env",
            "CLAUDE_CODE_PLUGIN_DIRS=/r/apps/studio/plugin",
            "claude"
        ]
    );
    assert_eq!(
        claude(plugin, Some("9e0b5f13")),
        [
            "env",
            "CLAUDE_CODE_PLUGIN_DIRS=/r/apps/studio/plugin",
            "claude",
            "--resume",
            "9e0b5f13"
        ]
    );
}
