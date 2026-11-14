//! Seam 1 for **Landed** (#59, spec #53): hook events through the core, with the facts the shell
//! looks up (files, PR, issue) fed in as `set_task_facts` reports them.

use studio_core::{State, Studio, TaskFacts};

const TASK: &str = "/work/plotkeeper/.claude/worktrees/issue-59-landed";
const MAIN: &str = "/work/plotkeeper";

fn event(session: &str, name: &str, cwd: &str, extra: &str) -> String {
    format!(
        r#"{{"session_id":"{session}","hook_event_name":"{name}","cwd":"{cwd}","studio_received_ms":1793300000000{extra}}}"#
    )
}

/// A session in `cwd` whose turn ended.
fn at_your_turn(studio: &mut Studio, session: &str, cwd: &str) {
    studio.replay(&event(
        session,
        "UserPromptSubmit",
        cwd,
        r#","prompt":"go""#,
    ));
    studio.replay(&event(session, "Stop", cwd, ""));
}

/// A session in `cwd` that claims `issue`, so its task has one.
fn with_issue(studio: &mut Studio, session: &str, cwd: &str, issue: u64) {
    let command = format!("cargo run --bin wayfinder-startup -- --claim {issue}");
    let step = format!(
        r#","tool_name":"Bash","tool_input":{{"command":"{command}"}},"tool_use_id":"toolu_1""#
    );
    studio.replay(&event(
        session,
        "UserPromptSubmit",
        cwd,
        r#","prompt":"go""#,
    ));
    studio.replay(&event(session, "PreToolUse", cwd, &step));
    studio.replay(&event(session, "Stop", cwd, ""));
}

const SHIPPED: TaskFacts = TaskFacts {
    safe: true,
    merged: true,
    issue_closed: true,
};

fn landed(studio: &Studio, session: &str) -> bool {
    studio.landed(studio.session(session).unwrap())
}

#[test]
fn a_task_lands_only_when_every_condition_holds() {
    let mut studio = Studio::default();
    with_issue(&mut studio, "a", TASK, 59);
    assert!(!landed(&studio, "a"), "nothing looked up yet");

    studio.set_task_facts(TASK, Some(SHIPPED));
    assert!(landed(&studio, "a"));

    for facts in [
        TaskFacts {
            safe: false,
            ..SHIPPED
        },
        TaskFacts {
            merged: false,
            ..SHIPPED
        },
        TaskFacts {
            issue_closed: false,
            ..SHIPPED
        },
    ] {
        studio.set_task_facts(TASK, Some(facts));
        assert!(!landed(&studio, "a"), "{facts:?}");
    }
}

#[test]
fn a_failed_lookup_is_not_landed() {
    let mut studio = Studio::default();
    at_your_turn(&mut studio, "a", TASK);
    studio.set_task_facts(TASK, Some(SHIPPED));
    assert!(landed(&studio, "a"));
    assert!(studio.set_task_facts(TASK, None));
    assert!(!landed(&studio, "a"));
}

#[test]
fn a_task_with_no_issue_needs_no_closed_issue() {
    let mut studio = Studio::default();
    at_your_turn(&mut studio, "a", TASK);
    let open = TaskFacts {
        issue_closed: false,
        ..SHIPPED
    };
    studio.set_task_facts(TASK, Some(open));
    assert!(landed(&studio, "a"));
}

#[test]
fn the_main_checkout_never_lands() {
    let mut studio = Studio::default();
    at_your_turn(&mut studio, "a", MAIN);
    studio.set_task_facts(MAIN, Some(SHIPPED));
    assert!(!landed(&studio, "a"));
    assert!(studio.task_queries().is_empty());
}

#[test]
fn a_running_or_waiting_session_in_the_worktree_keeps_it_unlanded() {
    let mut studio = Studio::default();
    at_your_turn(&mut studio, "reviewer", TASK);
    studio.set_task_facts(TASK, Some(SHIPPED));
    assert!(landed(&studio, "reviewer"));

    let prompt = r#","prompt":"go""#;
    studio.replay(&event("worker", "UserPromptSubmit", TASK, prompt));
    assert_eq!(studio.session("worker").unwrap().state, State::Running);
    assert!(!landed(&studio, "reviewer"));
    assert!(!landed(&studio, "worker"));

    let ask = r#","tool_name":"Bash","tool_input":{"command":"ls"},"tool_use_id":"toolu_2""#;
    studio.replay(&event("worker", "PermissionRequest", TASK, ask));
    assert_eq!(studio.session("worker").unwrap().state, State::NeedsYou);
    assert!(!landed(&studio, "reviewer"));

    studio.replay(&event("worker", "Stop", TASK, ""));
    assert!(landed(&studio, "reviewer"));
    assert!(landed(&studio, "worker"));
}

#[test]
fn the_shell_looks_up_each_task_once_with_its_issue() {
    let mut studio = Studio::default();
    with_issue(&mut studio, "a", TASK, 59);
    at_your_turn(&mut studio, "b", TASK);
    at_your_turn(&mut studio, "c", MAIN);
    let queries = studio.task_queries();
    assert_eq!(queries.len(), 1);
    assert_eq!(
        (queries[0].path.as_str(), queries[0].issue),
        (TASK, Some(59))
    );
}
