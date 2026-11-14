//! Seam 2 (danielrispler/Falinks#328 and danielrispler/Falinks#461, Testing Decisions): the picker's places from real `git` in temporary
//! repositories, as the engine's tests do (`src/lib.rs`, `retention_checks_every_blob_of_a_snapshot`).

use std::{path::Path, process::Command};
use studio_workbench::{GitState, Place, claude, fetch, git_state, merge_tree_runs, places};

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

/// With no upstream a branch is not pushed: no distance, but its branch is known (#29).
#[test]
fn a_branch_with_no_upstream_has_no_distance() {
    let (dir, main) = repo();
    git(
        dir.path(),
        &["worktree", "add", "-q", "-b", "t", ".claude/worktrees/t"],
    );
    // The tasks' folder is not a change of the main checkout's.
    std::fs::write(dir.path().join(".git/info/exclude"), ".claude/\n").unwrap();
    let found = places(dir.path()).unwrap();
    let state = |branch: &str| {
        Ok(GitState {
            branch: Some(branch.into()),
            upstream: None,
            changed: 0,
            operation: None,
            main_behind: None,
            on_default: false,
            would_conflict: false,
        })
    };

    assert_eq!(git_state(dir.path(), &main), state("main"));
    assert_eq!(git_state(dir.path(), &found.tasks[0].path), state("t"));
}

/// A repository whose `main` follows `origin/main`, a bare repository in the temp folder.
fn pushed() -> (tempfile::TempDir, String, std::path::PathBuf) {
    let (dir, main) = repo();
    let origin = dir.path().join(".git/origin.git");
    git(
        dir.path(),
        &["clone", "-q", "--bare", ".", origin.to_str().unwrap()],
    );
    git(
        dir.path(),
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(dir.path(), &["fetch", "-q", "origin"]);
    git(dir.path(), &["branch", "-q", "-u", "origin/main"]);
    (dir, main, origin)
}

/// The upstream distance is against the branch's own `origin` copy (#29).
#[test]
fn upstream_distance_counts_unpushed_and_unpulled_commits() {
    let (dir, main, origin) = pushed();
    // Another clone pushes one commit, which this one fetches but has not pulled.
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", origin.to_str().unwrap(), "c"],
    );
    let clone = other.path().join("c");
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "Theirs"]);
    git(&clone, &["push", "-q", "origin", "main"]);
    git(dir.path(), &["fetch", "-q", "origin"]);
    for message in ["One", "Two"] {
        git(
            dir.path(),
            &["commit", "-q", "--allow-empty", "-m", message],
        );
    }
    std::fs::write(dir.path().join("a.txt"), "draft").unwrap();

    assert_eq!(
        git_state(dir.path(), &main),
        Ok(GitState {
            branch: Some("main".into()),
            upstream: Some((2, 1)),
            changed: 1,
            operation: None,
            main_behind: Some(1),
            on_default: true,
            would_conflict: false,
        })
    );
}

/// A detached `HEAD` has no branch, and so no upstream either (#29).
#[test]
fn a_detached_head_has_no_branch() {
    let (dir, main, _) = pushed();
    git(dir.path(), &["checkout", "-q", "--detach"]);

    assert_eq!(
        git_state(dir.path(), &main),
        Ok(GitState {
            branch: None,
            upstream: None,
            changed: 0,
            operation: None,
            main_behind: None,
            on_default: false,
            would_conflict: false,
        })
    );
}

/// Reading a place's state never rewrites the index, which would take `index.lock` against an agent's commit (#27).
#[test]
fn git_state_leaves_the_index_untouched_even_when_its_stat_info_is_stale() {
    let (dir, main) = repo();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "a").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "Add a"]);
    // Same bytes, new mtime: the index's stat info is stale and a plain `git status` refreshes it.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(&file, "a").unwrap();
    let index = dir.path().join(".git/index");
    let before = (
        std::fs::read(&index).unwrap(),
        std::fs::metadata(&index).unwrap().modified().unwrap(),
    );

    git_state(dir.path(), &main).unwrap();

    let after = (
        std::fs::read(&index).unwrap(),
        std::fs::metadata(&index).unwrap().modified().unwrap(),
    );
    assert_eq!(before, after);
}

/// The git state is read only in a place of the repository (danielrispler/Falinks#576).
#[test]
fn git_state_refuses_a_folder_that_is_not_a_place() {
    let (dir, _) = repo();
    let other = tempfile::tempdir().unwrap();

    assert!(git_state(dir.path(), &other.path().display().to_string()).is_err());
    assert!(git_state(dir.path(), "/").is_err());
}

#[test]
fn a_folder_outside_git_is_an_error() {
    let dir = tempfile::tempdir().unwrap();

    assert!(places(dir.path()).is_err());
}

/// The studio's status line wrapper beside the plugin (danielrispler/Falinks#521), quoted for the shell that runs it.
const STATUS_LINE: &str =
    r#"{"statusLine":{"command":"'/r/apps/studio/workbench/statusline.sh'","type":"command"}}"#;

/// `claude` runs with the studio plugin alone, set the way the README loads it, so a profile that
/// also sets `CLAUDE_CODE_PLUGIN_DIRS` never loads its hooks twice. It names the presence file, so
/// Remote Control skips a phone push while the studio is focused (danielrispler/Falinks#522).
#[test]
fn claude_starts_with_the_studio_plugin_and_resumes_by_session_id() {
    let plugin = Path::new("/r/apps/studio/plugin");
    let presence = Path::new("/d/present");

    assert_eq!(
        claude(plugin, presence, None, false),
        [
            "env",
            "CLAUDE_CODE_PLUGIN_DIRS=/r/apps/studio/plugin",
            "CLAUDE_CLIENT_PRESENCE_FILE=/d/present",
            "claude",
            "--settings",
            STATUS_LINE,
        ]
    );
    assert_eq!(
        claude(plugin, presence, Some("9e0b5f13"), false),
        [
            "env",
            "CLAUDE_CODE_PLUGIN_DIRS=/r/apps/studio/plugin",
            "CLAUDE_CLIENT_PRESENCE_FILE=/d/present",
            "claude",
            "--settings",
            STATUS_LINE,
            "--resume",
            "9e0b5f13"
        ]
    );
}

/// Auto mode (danielrispler/Falinks#541) is Claude Code's `--permission-mode auto`, added only when asked, for a new
/// session and for a resumed one, which keeps the mode it was started with. Never bypass.
#[test]
fn claude_runs_in_auto_mode_only_when_asked() {
    let plugin = Path::new("/r/apps/studio/plugin");
    let presence = Path::new("/d/present");
    let tail = |argv: Vec<String>| argv[6..].to_vec();

    assert_eq!(
        tail(claude(plugin, presence, None, true)),
        ["--permission-mode", "auto"]
    );
    assert_eq!(
        tail(claude(plugin, presence, Some("9e0b5f13"), true)),
        ["--permission-mode", "auto", "--resume", "9e0b5f13"]
    );
    assert!(tail(claude(plugin, presence, None, false)).is_empty());
}

/// An operation left half-done shows until it is over (#31): a rebase stopped on a conflict
/// reports a rebase, and none after `git rebase --abort`.
#[test]
fn a_rebase_stopped_on_a_conflict_is_in_progress_until_aborted() {
    let (dir, main) = repo();
    let write = |text: &str| std::fs::write(dir.path().join("f"), text).unwrap();
    write("base\n");
    git(dir.path(), &["add", "f"]);
    git(dir.path(), &["commit", "-q", "-m", "Base"]);
    git(dir.path(), &["checkout", "-q", "-b", "t"]);
    write("theirs\n");
    git(dir.path(), &["commit", "-q", "-am", "Theirs"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    write("ours\n");
    git(dir.path(), &["commit", "-q", "-am", "Ours"]);
    git(dir.path(), &["checkout", "-q", "t"]);
    assert_eq!(git_state(dir.path(), &main).unwrap().operation, None);

    // The rebase stops on the conflict, so `git` exits non-zero.
    let stopped = Command::new("git")
        .args(["rebase", "main"])
        .current_dir(dir.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_COMMITTER_NAME", "Studio")
        .env("GIT_COMMITTER_EMAIL", "studio@example.com")
        .output()
        .unwrap();
    assert!(!stopped.status.success());
    assert_eq!(
        git_state(dir.path(), &main).unwrap().operation.as_deref(),
        Some("rebase")
    );

    git(dir.path(), &["rebase", "--abort"]);
    assert_eq!(git_state(dir.path(), &main).unwrap().operation, None);
}

/// Main distance is against `origin`'s default branch: a task behind it reports how far, one level
/// with it reports none, and the default branch itself is flagged so the window shows one distance (#30).
#[test]
fn main_distance_counts_commits_origins_default_branch_has_that_the_branch_lacks() {
    let (dir, main, origin) = pushed();
    git(dir.path(), &["remote", "set-head", "origin", "main"]);
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "t",
            ".claude/worktrees/t",
            "origin/main",
        ],
    );
    let task = places(dir.path()).unwrap().tasks[0].path.clone();
    let level = git_state(dir.path(), &task).unwrap();
    assert_eq!((level.main_behind, level.on_default), (None, false));

    // Another clone pushes two commits to main; this one fetches them.
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", origin.to_str().unwrap(), "c"],
    );
    let clone = other.path().join("c");
    for message in ["Theirs", "More"] {
        git(&clone, &["commit", "-q", "--allow-empty", "-m", message]);
    }
    git(&clone, &["push", "-q", "origin", "main"]);
    git(dir.path(), &["fetch", "-q", "origin"]);

    let behind = git_state(dir.path(), &task).unwrap();
    assert_eq!((behind.main_behind, behind.on_default), (Some(2), false));
    let on_main = git_state(dir.path(), &main).unwrap();
    assert!(on_main.on_default);
    assert_eq!(on_main.upstream, Some((0, 2)));
}

/// A fetch brings `origin`'s new commits, and the place's distance follows (#33).
#[test]
fn a_fetch_moves_origins_default_branch_and_the_distances() {
    let (dir, main, origin) = pushed();
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", origin.to_str().unwrap(), "c"],
    );
    let clone = other.path().join("c");
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "Theirs"]);
    git(&clone, &["push", "-q", "origin", "main"]);
    let behind = |n| assert_eq!(git_state(dir.path(), &main).unwrap().upstream, Some((0, n)));
    behind(0);

    fetch(dir.path()).unwrap();

    behind(1);
}

/// A failed fetch says why and leaves the refs as they were (#33).
#[test]
fn a_fetch_from_a_missing_remote_fails_with_gits_message_and_keeps_the_refs() {
    let (dir, _main, origin) = pushed();
    let refs = || {
        let out = Command::new("git")
            .args(["for-each-ref"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let before = refs();
    std::fs::rename(&origin, dir.path().join(".git/gone.git")).unwrap();

    let error = fetch(dir.path()).unwrap_err();

    assert!(error.contains("origin.git"), "{error}");
    assert_eq!(refs(), before);
}

/// A pushed repository with a task `t` and a second clone for pushing to `origin`'s default
/// branch: `(repo, clone's folder, task path, clone)`.
fn task_and_a_clone() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    String,
    std::path::PathBuf,
) {
    let (dir, _main, origin) = pushed();
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "x\n").unwrap();
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "Files"]);
    git(dir.path(), &["push", "-q", "origin", "main"]);
    git(dir.path(), &["remote", "set-head", "origin", "main"]);
    git(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "t",
            ".claude/worktrees/t",
            "origin/main",
        ],
    );
    let task = places(dir.path()).unwrap().tasks[0].path.clone();
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", origin.to_str().unwrap(), "c"],
    );
    let clone = other.path().join("c");
    (dir, other, task, clone)
}

/// Commits `content` to `file` in `repo`.
fn commit_file(repo: &Path, file: &str, content: &str) {
    std::fs::write(repo.join(file), content).unwrap();
    git(repo, &["commit", "-q", "-am", "Edit"]);
}

/// A branch that edits the line `origin`'s default branch edited would conflict in a rebase onto
/// it; one that edits another file would not (#32).
#[test]
fn a_branch_editing_the_same_line_as_origins_default_branch_would_conflict() {
    let (dir, _other, task, clone) = task_and_a_clone();
    commit_file(Path::new(&task), "a.txt", "one\nmine\n");
    commit_file(&clone, "a.txt", "one\ntheirs\n");
    git(&clone, &["push", "-q", "origin", "main"]);
    git(dir.path(), &["fetch", "-q", "origin"]);
    let read = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&task)
            .output()
            .unwrap()
            .stdout
    };
    let before = (read(&["for-each-ref"]), read(&["ls-files", "-s"]));
    let state = git_state(dir.path(), &task).unwrap();
    assert_eq!(state.main_behind, Some(1));
    assert!(state.would_conflict);

    // The check changed no ref, no index entry and no file.
    assert_eq!((read(&["for-each-ref"]), read(&["ls-files", "-s"])), before);
    assert_eq!(read(&["status", "--porcelain"]), b"");
    assert_eq!(
        std::fs::read_to_string(Path::new(&task).join("a.txt")).unwrap(),
        "one\nmine\n"
    );
}

#[test]
fn a_branch_editing_another_file_would_not_conflict() {
    let (dir, _other, task, clone) = task_and_a_clone();
    commit_file(Path::new(&task), "a.txt", "one\nmine\n");
    commit_file(&clone, "b.txt", "y\n");
    git(&clone, &["push", "-q", "origin", "main"]);
    git(dir.path(), &["fetch", "-q", "origin"]);
    let state = git_state(dir.path(), &task).unwrap();
    assert_eq!(state.main_behind, Some(1));
    assert!(!state.would_conflict);
}

/// The check runs once per pair of (`HEAD`, default branch) tips: not while only the files change,
/// once more when either tip moves (#32).
#[test]
fn would_conflict_is_recomputed_only_when_head_or_the_default_branch_moves() {
    let (dir, _other, task, clone) = task_and_a_clone();
    commit_file(&clone, "b.txt", "y\n");
    git(&clone, &["push", "-q", "origin", "main"]);
    git(dir.path(), &["fetch", "-q", "origin"]);
    assert_eq!(merge_tree_runs(&task), 0);
    git_state(dir.path(), &task).unwrap();
    assert_eq!(merge_tree_runs(&task), 1);

    std::fs::write(Path::new(&task).join("a.txt"), "edited\n").unwrap();
    git_state(dir.path(), &task).unwrap();
    git_state(dir.path(), &task).unwrap();
    assert_eq!(merge_tree_runs(&task), 1);

    commit_file(Path::new(&task), "a.txt", "committed\n");
    git_state(dir.path(), &task).unwrap();
    assert_eq!(merge_tree_runs(&task), 2);

    commit_file(&clone, "b.txt", "z\n");
    git(&clone, &["push", "-q", "origin", "main"]);
    git(dir.path(), &["fetch", "-q", "origin"]);
    git_state(dir.path(), &task).unwrap();
    assert_eq!(merge_tree_runs(&task), 3);
}

/// A branch level with the default branch, or only ahead of it, is not behind it, so the check
/// does not run (#32).
#[test]
fn would_conflict_does_not_run_when_the_branch_is_not_behind_the_default_branch() {
    let (dir, _other, task, _clone) = task_and_a_clone();
    git_state(dir.path(), &task).unwrap();
    assert_eq!(merge_tree_runs(&task), 0);

    commit_file(Path::new(&task), "a.txt", "ahead\n");
    let state = git_state(dir.path(), &task).unwrap();
    assert!(!state.would_conflict);
    assert_eq!(merge_tree_runs(&task), 0);
}
