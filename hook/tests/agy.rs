// `plotkeeper-hook` records an agy event from any terminal (#323, which reverses #43's decision 9), and
// no event from a process with `PLOTKEEPER_INTERNAL=1`, plotkeeper's own model runs.
use crate::common::{feed, hook, scratch};
use std::{fs, process::Stdio};

fn run(payload: serde_json::Value, name: &str) -> (bool, String) {
    let home = scratch(&format!("agy-{name}"));
    let mut cmd = hook(&home);
    cmd.stdout(Stdio::piped());
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
fn an_agy_event_from_any_terminal_is_logged() {
    let (written, log) = run(agy(), "unset");
    assert!(written);
    assert!(log.contains("c-1"), "{log}");
}

#[test]
fn an_event_from_a_plotkeeper_run_writes_nothing() {
    for (name, payload) in [
        ("agy", agy()),
        (
            "claude",
            serde_json::json!({ "session_id": "s-1", "hook_event_name": "Stop", "cwd": "/" }),
        ),
    ] {
        let home = scratch(&format!("internal-{name}"));
        let mut cmd = hook(&home);
        cmd.env("PLOTKEEPER_INTERNAL", "1").stdout(Stdio::piped());
        let out = feed(cmd, &payload).wait_with_output().unwrap();
        assert!(out.status.success() && out.stdout.is_empty());
        assert!(!studio_hook::event_log(&home).exists(), "{name}");
    }
}

#[test]
fn a_claude_event_from_any_terminal_is_logged() {
    let claude = serde_json::json!({ "session_id": "s-1", "hook_event_name": "Stop", "cwd": "/" });
    let (_, log) = run(claude, "claude");
    assert!(log.contains("s-1"));
}
