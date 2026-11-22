// `studio-hook` records an agy event from any terminal (#323, which reverses #43's decision 9), and
// no event from a process with `PLOTKEEPER_INTERNAL=1`, plotkeeper's own model runs.
use crate::common::{feed, hook, scratch};
use std::{fs, process::Stdio};

fn run(terminal: Option<&str>, payload: serde_json::Value, name: &str) -> (bool, String) {
    let home = scratch(&format!("agy-{name}"));
    let mut cmd = hook(&home);
    cmd.stdout(Stdio::piped());
    if let Some(t) = terminal {
        cmd.env("STUDIO_TERMINAL_ID", t);
    }
    let out = feed(cmd, &payload).wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    let log = studio_hook::event_log(&home);
    (log.exists(), fs::read_to_string(log).unwrap_or_default())
}

fn agy() -> serde_json::Value {
    serde_json::json!({ "conversationId": "c-1", "hookEventName": "PreToolUse", "cwd": "/" })
}

#[test]
fn an_agy_event_outside_a_studio_terminal_is_logged_without_one() {
    let (written, log) = run(None, agy(), "unset");
    assert!(written);
    assert!(log.contains("c-1") && !log.contains("studio_terminal_id"), "{log}");
}

#[test]
fn an_event_from_a_plotkeeper_run_writes_nothing() {
    for (name, payload) in [("agy", agy()), ("claude", serde_json::json!({ "session_id": "s-1", "hook_event_name": "Stop", "cwd": "/" }))] {
        let home = scratch(&format!("internal-{name}"));
        let mut cmd = hook(&home);
        cmd.env("PLOTKEEPER_INTERNAL", "1").stdout(Stdio::piped());
        let out = feed(cmd, &payload).wait_with_output().unwrap();
        assert!(out.status.success() && out.stdout.is_empty());
        assert!(!studio_hook::event_log(&home).exists(), "{name}");
    }
}

#[test]
fn an_agy_event_in_a_studio_terminal_is_logged_with_it() {
    let (_, log) = run(Some("t-9"), agy(), "set");
    assert!(log.contains("\"studio_terminal_id\":\"t-9\""), "{log}");
    assert!(log.contains("c-1"));
}

#[test]
fn a_claude_event_outside_a_studio_terminal_is_still_logged() {
    let claude = serde_json::json!({ "session_id": "s-1", "hook_event_name": "Stop", "cwd": "/" });
    let (_, log) = run(None, claude, "claude");
    assert!(log.contains("s-1"));
}
