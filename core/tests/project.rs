//! A session's project (spec #659, "A session's project"; ticket #671): the root of the first folder it gets, the git top
//! level the shell's lookup finds, else that folder, with a task's worktree suffix stripped. Later events never change it,
//! while its folder, folder name and worktree name follow them.

use crate::common::base;
use studio_core::Studio;

const T0: u64 = 1_793_270_000_000;
const REPO: &str = "/w/plotkeeper";
const TASK: &str = "/w/plotkeeper/.claude/worktrees/issue-1";

/// A studio whose root lookup finds git's top level as `git rev-parse --show-toplevel` would: the task's worktree for a
/// folder in it, the repository for any other folder in it, and none outside both.
fn studio() -> Studio {
    Studio::with_roots(|folder: &str| {
        let inside = |root: &str| folder == root || folder.starts_with(&format!("{root}/"));
        [TASK, REPO]
            .into_iter()
            .find(|r| inside(r))
            .map(String::from)
    })
}

fn events(studio: &mut Studio, session: &str, folders: &[&str]) {
    for (at, cwd) in (T0..).zip(folders) {
        let name = if at == T0 {
            "SessionStart"
        } else {
            "UserPromptSubmit"
        };
        studio.replay(&base(session, name, cwd, at).to_string());
    }
}

fn project(studio: &Studio, session: &str) -> String {
    studio.session(session).unwrap().project.clone()
}

#[test]
fn a_session_started_in_a_repository_s_subfolder_keeps_its_root_after_a_cd_elsewhere() {
    let mut studio = studio();
    events(
        &mut studio,
        "s1",
        &[&format!("{REPO}/ui"), "/w/elsewhere/server"],
    );

    let session = studio.session("s1").unwrap();
    assert_eq!(session.project, REPO);
    // Its folder still follows its events.
    assert_eq!(session.cwd, "/w/elsewhere/server");
    assert_eq!(session.folder_name, "server");
}

#[test]
fn a_session_started_in_a_task_s_worktree_belongs_to_its_main_checkout() {
    let mut studio = studio();
    events(&mut studio, "s1", &[&format!("{TASK}/ui"), REPO]);

    assert_eq!(project(&studio, "s1"), REPO);
    assert_eq!(
        studio.session("s1").unwrap().identity.worktree,
        "plotkeeper"
    );
}

#[test]
fn a_session_started_outside_git_keeps_its_starting_folder() {
    let mut studio = studio();
    events(
        &mut studio,
        "s1",
        &["/w/notes/drafts", &format!("{REPO}/ui")],
    );

    assert_eq!(project(&studio, "s1"), "/w/notes/drafts");
}

#[test]
fn a_session_whose_first_events_have_no_folder_takes_the_first_folder_it_gets() {
    let mut studio = studio();
    events(
        &mut studio,
        "s1",
        &["", &format!("{REPO}/server"), "/w/notes"],
    );

    assert_eq!(project(&studio, "s1"), REPO);
}
