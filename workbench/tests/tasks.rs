//! Seam 2 (danielrispler/Falinks#328 and danielrispler/Falinks#461, Testing Decisions): creating and removing tasks (danielrispler/Falinks#468) with real `git`
//! in temporary repositories that have a bare `origin`.

use std::{ops::Deref, path::Path};
use studio_test_support::{Origin, git, origin};
use studio_workbench::{
    Branch, GitState, Losses, Workspace, branches, create_named_task, create_task,
    delete_branch_merged_at, delete_merged_branch, git_state, losses, remove_task, switch_branch,
    workspaces,
};

/// The shared `origin` with a second commit pushed after the clone, so the clone's `trunk` is one
/// commit behind.
struct Repos {
    base: Origin,
    second: String,
}

impl Deref for Repos {
    type Target = Origin;

    fn deref(&self) -> &Origin {
        &self.base
    }
}

fn repos() -> Repos {
    let base = origin();
    git(
        &base.seed,
        &["commit", "-q", "--allow-empty", "-m", "Second"],
    );
    git(&base.seed, &["push", "-q", "origin", "trunk"]);
    let second = git(&base.seed, &["rev-parse", "HEAD"]);
    Repos { base, second }
}

fn task_path(repos: &Repos, name: &str) -> std::path::PathBuf {
    repos.main.join(".claude/worktrees").join(name)
}

/// A `gh` that is not there, as when the lookup finds none.
const NO_GH: &str = "/nonexistent/gh";

/// A stub `gh` beside the repos that prints `output` when run with exactly `args` and fails
/// otherwise, so no test reaches GitHub.
fn stub_gh(repos: &Repos, args: &str, output: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let gh = repos.origin.with_file_name("gh");
    let script = format!("#!/bin/sh\n[ \"$*\" = '{args}' ] || exit 1\necho '{output}'\n");
    std::fs::write(&gh, script).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    gh.display().to_string()
}

#[test]
fn a_task_branches_from_origins_default_branch_after_a_fetch_even_when_local_is_behind() {
    let repos = repos();

    let task = create_task(&repos.main, "issue-1").unwrap();

    let path = task_path(&repos, "issue-1");
    assert_eq!(
        task,
        Workspace {
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

    let branch = |request: &str| {
        let task = create_named_task(main, request, NO_GH).unwrap();
        task.branch.unwrap()
    };

    // A prompt: its first words; a taken name gets the next free number.
    assert_eq!(
        branch("Fix the picker's outside click, then open a PR with every fix"),
        "fix-the-pickers-outside-click-2"
    );
    // A spec: its file's name.
    assert_eq!(branch("docs/Picker Spec.md"), "picker-spec");
    // An issue: its number, then its title, which no `gh` reads here.
    assert_eq!(branch("#123"), "issue-123");
    assert_eq!(branch("!!!"), "session");
}

/// A task started from `#N` is named from the title the `gh` it is given reads.
#[test]
fn a_task_started_from_an_issue_is_named_from_its_title() {
    let repos = repos();
    let args = "issue view 123 --json title --jq .title";
    let gh = stub_gh(&repos, args, "Fix the outside click");

    let task = create_named_task(&repos.main, "#123", &gh).unwrap();

    assert_eq!(task.branch.unwrap(), "123-fix-the-outside-click");
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
fn losses_are_empty_for_a_clean_pushed_task_and_list_new_work() {
    let repos = repos();
    let task = create_task(&repos.main, "shipped").unwrap();
    let path = Path::new(&task.path);
    git(path, &["commit", "-q", "--allow-empty", "-m", "Work"]);
    git(path, &["push", "-q", "origin", "shipped"]);
    let nothing = Losses {
        uncommitted: vec![],
        unpushed: vec![],
    };
    assert_eq!(losses(path), Ok(nothing));

    // New work after the push shows at once: a commit, then a file.
    git(path, &["commit", "-q", "--allow-empty", "-m", "More"]);
    assert_eq!(losses(path).unwrap().unpushed.len(), 1);
    git(path, &["push", "-q", "origin", "shipped"]);
    std::fs::write(path.join("new.txt"), "x").unwrap();
    let found = losses(path).unwrap();
    assert_eq!(
        (found.uncommitted, found.unpushed),
        (vec!["?? new.txt".into()], vec![])
    );
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

    assert_eq!(delete_merged_branch(&repos.main, "merged", NO_GH), Ok(()));
    let branches = git(&repos.main, &["branch", "--list", "merged"]);
    assert!(branches.is_empty(), "{branches}");

    let refused = delete_merged_branch(&repos.main, "kept", NO_GH).unwrap_err();
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

/// A squash-merged branch goes when the `gh` it is given lists its tip as a merged pull request's
/// head, and stays with git's refusal when no `gh` is found.
#[test]
fn a_squash_merged_branch_is_deleted_through_the_merged_pull_requests_gh_lists() {
    let repos = repos();
    let task = create_task(&repos.main, "squashed").unwrap();
    git(
        Path::new(&task.path),
        &["commit", "-q", "--allow-empty", "-m", "Merged by squash"],
    );
    let tip = git(&repos.main, &["rev-parse", "squashed"]);
    remove_task(&repos.main, &task.path, true).unwrap();

    let refused = delete_merged_branch(&repos.main, "squashed", NO_GH).unwrap_err();
    assert!(refused.contains("not fully merged"), "{refused}");
    assert_eq!(git(&repos.main, &["rev-parse", "squashed"]), tip);

    let args = "pr list --head squashed --state merged --json headRefOid --jq .[].headRefOid";
    let gh = stub_gh(&repos, args, &tip);
    assert_eq!(delete_merged_branch(&repos.main, "squashed", &gh), Ok(()));
    let branches = git(&repos.main, &["branch", "--list", "squashed"]);
    assert!(branches.is_empty(), "{branches}");
}

/// A task's branch has no upstream until pushed; the main checkout's is `origin/trunk`; both count
/// the files `git status` lists (#29, after danielrispler/Falinks#576).
#[test]
fn a_workspace_reads_its_upstream_distance_and_its_changed_files() {
    let repos = repos();
    let busy = create_task(&repos.main, "busy").unwrap();
    let path = Path::new(&busy.path);
    git(path, &["commit", "-q", "--allow-empty", "-m", "One"]);
    for file in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(path.join(file), "draft").unwrap();
    }
    let seed = repos.origin.with_file_name("seed");
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "Third"]);
    git(&seed, &["push", "-q", "origin", "trunk"]);
    git(&repos.main, &["fetch", "-q", "origin"]);
    // The tasks' folder is not a change of the main checkout's.
    std::fs::write(repos.main.join(".git/info/exclude"), ".claude/\n").unwrap();
    let main = workspaces(&repos.main).unwrap().main;
    let state = |branch: &str, upstream, changed, main_behind, on_default| {
        Ok(GitState {
            branch: Some(branch.into()),
            upstream,
            changed,
            operation: None,
            main_behind,
            on_default,
            would_conflict: false,
            would_overwrite: false,
        })
    };

    // `busy` branched before the third commit reached `origin/trunk`, so it is one behind it.
    assert_eq!(
        git_state(&repos.main, &busy.path),
        state("busy", None, 3, Some(1), false)
    );
    // The main checkout's `trunk` is two behind `origin/trunk`, its upstream.
    assert_eq!(
        git_state(&repos.main, &main.path),
        state("trunk", Some((0, 2)), 0, Some(2), true)
    );
    git(path, &["push", "-q", "-u", "origin", "busy"]);
    assert_eq!(
        git_state(&repos.main, &busy.path),
        state("busy", Some((0, 0)), 3, Some(1), false)
    );
}

/// No code path forces git (danielrispler/Falinks#461, decision 9): the studio's own sources, the workbench, the shell
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

#[test]
fn the_branch_list_has_local_and_remote_only_branches_with_the_ones_a_task_holds_marked() {
    let repos = repos();
    let main = &repos.main;
    let seed = repos.origin.with_file_name("seed");
    git(&seed, &["push", "-q", "origin", "HEAD:teammate"]);
    git(&seed, &["push", "-q", "origin", "HEAD:both"]);
    git(main, &["branch", "both", "HEAD"]);
    git(main, &["branch", "spare", "HEAD"]);
    create_task(main, "issue-1").unwrap();
    git(main, &["fetch", "-q", "origin"]);

    let list = branches(main).unwrap();

    let row = |name: &str, remote, task: Option<&str>, current| Branch {
        name: name.into(),
        remote,
        task: task.map(String::from),
        current,
    };
    assert_eq!(
        list,
        [
            row("both", false, None, false),
            row("issue-1", false, Some("issue-1"), false),
            row("spare", false, None, false),
            row("trunk", false, None, true),
            // `both` is at origin too, but a local branch of its name is in the list; `HEAD` is no branch.
            row("teammate", true, None, false),
        ]
    );
}

#[test]
fn a_local_branch_is_switched_to_and_a_remote_only_one_is_tracked() {
    let repos = repos();
    let main = &repos.main;
    let seed = repos.origin.with_file_name("seed");
    git(&seed, &["push", "-q", "origin", "HEAD:teammate"]);
    git(main, &["fetch", "-q", "origin"]);
    git(main, &["branch", "spare", "HEAD"]);

    switch_branch(main, "spare", false).unwrap();
    assert_eq!(git(main, &["branch", "--show-current"]), "spare");

    switch_branch(main, "teammate", true).unwrap();
    assert_eq!(git(main, &["branch", "--show-current"]), "teammate");
    assert_eq!(
        git(main, &["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/teammate"
    );
    // Asked from a task, it still switches the main checkout and leaves the task alone.
    let task = create_task(main, "issue-1").unwrap();
    switch_branch(Path::new(&task.path), "trunk", false).unwrap();
    assert_eq!(git(main, &["branch", "--show-current"]), "trunk");
    assert_eq!(
        git(Path::new(&task.path), &["branch", "--show-current"]),
        "issue-1"
    );
}

#[test]
fn a_switch_git_refuses_shows_its_message_and_leaves_the_branch_and_the_file_alone() {
    let repos = repos();
    let main = &repos.main;
    std::fs::write(main.join("f.txt"), "on trunk\n").unwrap();
    git(main, &["add", "f.txt"]);
    git(main, &["commit", "-q", "-m", "f on trunk"]);
    git(main, &["switch", "-q", "-c", "other", "HEAD~1"]);
    std::fs::write(main.join("f.txt"), "on other\n").unwrap();
    git(main, &["add", "f.txt"]);
    git(main, &["commit", "-q", "-m", "f on other"]);
    std::fs::write(main.join("f.txt"), "dirty\n").unwrap();

    let refused = switch_branch(main, "trunk", false).unwrap_err();

    assert!(refused.contains("f.txt"), "{refused}");
    assert!(refused.contains("overwritten"), "{refused}");
    assert_eq!(git(main, &["branch", "--show-current"]), "other");
    assert_eq!(
        std::fs::read_to_string(main.join("f.txt")).unwrap(),
        "dirty\n"
    );
    assert_eq!(git(main, &["stash", "list"]), "");
    // A name that is no branch, or a flag, is refused too.
    assert!(switch_branch(main, "--detach", false).is_err());
    assert!(switch_branch(main, "nope", false).is_err());
}
