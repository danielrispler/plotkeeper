//! Session ending (danielrispler/plotkeeper#330, spec #319): a session whose leader pid is gone
//! ends; one with no pid shows idle after 2 hours without events and comes back on its next event.
//! The process table is a fake: a closure over the pids it lists, so no real process is signalled.

use crate::common;
use serde_json::{Value, json};
use studio_core::{State, Studio};

const SESSION: &str = "7a1d3c5e-2b4f-4a6c-8d0e-9f1a2b3c4d5e";
const OTHER: &str = "8b2e4d6f-3c5a-4b7d-9e1f-0a2b3c4d5e6f";
const CWD: &str = "/Users/danielrispler/work/plotkeeper";
const START: u64 = 1_793_262_000_000;
const HOUR: u64 = 60 * 60 * 1000;

fn event(session: &str, name: &str, at: u64, extra: Value) -> String {
    common::event(session, name, CWD, at, extra)
}

fn prompt(session: &str, at: u64, pid: Option<u32>) -> String {
    let mut extra = json!({ "prompt": "Go.", "prompt_id": "p-1" });
    if let Some(pid) = pid {
        extra["studio_pid"] = json!(pid);
    }
    event(session, "UserPromptSubmit", at, extra)
}

fn started(session: &str, pid: Option<u32>) -> String {
    let mut extra = json!({ "source": "startup" });
    if let Some(pid) = pid {
        extra["studio_pid"] = json!(pid);
    }
    event(session, "SessionStart", START, extra)
}

/// A process table that lists `pids` as alive.
fn table(pids: &'static [u32]) -> impl Fn(u32) -> bool {
    move |pid| pids.contains(&pid)
}

#[test]
fn a_session_whose_pid_is_gone_is_due_to_end_and_a_live_one_is_not() {
    let mut lines = vec![
        started(SESSION, Some(4001)),
        prompt(SESSION, START + 1, Some(4001)),
    ];
    lines.push(started(OTHER, Some(4002)));
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.gone(table(&[4002])), [SESSION]);
    assert!(studio.gone(table(&[4001, 4002])).is_empty());
    assert_eq!(studio.gone(table(&[])), [SESSION, OTHER]);
}

#[test]
fn a_session_with_no_pid_is_never_due_to_end() {
    let studio = Studio::replayed(&[started(SESSION, None), prompt(SESSION, START + 1, None)]);
    assert!(studio.gone(table(&[])).is_empty());
}

#[test]
fn an_ended_session_is_not_due_again_and_a_synthetic_end_ends_it() {
    let mut lines = vec![started(SESSION, Some(4001))];
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.gone(table(&[])), [SESSION]);
    lines.push(event(
        SESSION,
        "SessionEnd",
        START + 2,
        json!({ "reason": "process_gone" }),
    ));
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.session(SESSION).unwrap().state, State::Ended);
    assert!(studio.gone(table(&[])).is_empty());
}

#[test]
fn the_latest_pid_wins() {
    // A resume in a new process reports its own pid.
    let lines = vec![
        started(SESSION, Some(4001)),
        prompt(SESSION, START + 1, Some(4005)),
    ];
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.gone(table(&[4005])), Vec::<String>::new());
    assert_eq!(studio.gone(table(&[4001])), [SESSION]);
}

#[test]
fn a_running_session_with_no_pid_goes_idle_after_two_hours_and_a_new_event_revives_it() {
    let mut studio = Studio::replayed(&[started(SESSION, None), prompt(SESSION, START + 1, None)]);
    assert_eq!(studio.session(SESSION).unwrap().state, State::Running);
    assert!(!studio.tick(START + 2 * HOUR));
    assert_eq!(studio.session(SESSION).unwrap().state, State::Running);
    assert!(studio.tick(START + 1 + 2 * HOUR));
    assert_eq!(studio.session(SESSION).unwrap().state, State::Idle);
    // Never ended.
    assert!(studio.gone(table(&[])).is_empty());
    let tool = event(
        SESSION,
        "PostToolUse",
        START + 3 * HOUR,
        json!({
            "tool_name": "Bash", "tool_use_id": "t1", "tool_input": { "command": "ls" }, "prompt_id": "p-1",
        }),
    );
    studio.replay(&tool);
    assert_eq!(studio.session(SESSION).unwrap().state, State::Running);
}

#[test]
fn a_running_session_with_a_pid_stays_running_however_quiet() {
    let mut studio = Studio::replayed(&[
        started(SESSION, Some(4001)),
        prompt(SESSION, START + 1, Some(4001)),
    ]);
    studio.tick(START + 5 * HOUR);
    assert_eq!(studio.session(SESSION).unwrap().state, State::Running);
}
