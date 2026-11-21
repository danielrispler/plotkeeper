// Seam 4 (#43, decision 9): `studio-hook` writes an agy event only inside a studio terminal. agy's
// hooks fire for every agy on the machine; Claude's rows without a terminal stay as they were.
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
fn an_agy_event_outside_a_studio_terminal_writes_nothing() {
    let (written, _) = run(None, agy(), "unset");
    assert!(!written);
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
