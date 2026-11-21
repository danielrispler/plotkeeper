//! Seam 1 (danielrispler/plotkeeper#54, Testing Decisions) for a subagent's log: hook events through the core,
//! with a made-up subagent log at the path the core works out from the parent's transcript path. The fixture
//! is shaped on Claude Code 2.1.311's lines; a format change shows in the version notice, not here.

use serde_json::json;
use std::{fs, path::PathBuf};
use studio_core::{LogEntry, Studio};

const SESSION: &str = "s1";
const AGENT: &str = "a7e2c4f6b8d0a1c3e";

/// A session whose transcript is `<dir>/s1.jsonl`, with subagent `AGENT` seen at a step. Returns the studio
/// and the folder where its log belongs, created.
fn setup(transcript_name: &str) -> (Studio, PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join(SESSION).join("subagents");
    fs::create_dir_all(&logs).unwrap();
    let transcript = dir.path().join(transcript_name);
    let event = json!({
        "session_id": SESSION, "hook_event_name": "PreToolUse", "cwd": "/w", "agent_id": AGENT,
        "tool_name": "Bash", "tool_use_id": "t0", "tool_input": {"command": "ls"},
        "transcript_path": transcript, "studio_received_ms": 1,
    });
    // Only the session's own agent's events carry its transcript path.
    let own = json!({
        "session_id": SESSION, "hook_event_name": "UserPromptSubmit", "cwd": "/w", "prompt": "Go.",
        "transcript_path": transcript, "studio_received_ms": 0,
    });
    (
        Studio::replayed([own.to_string(), event.to_string()]),
        logs,
        dir,
    )
}

fn write(logs: &std::path::Path, lines: &[serde_json::Value]) {
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    fs::write(logs.join(format!("agent-{AGENT}.jsonl")), text).unwrap();
}

fn line(kind: &str, agent: &str, message: serde_json::Value) -> serde_json::Value {
    json!({"type": kind, "agentId": agent, "sessionId": SESSION, "isSidechain": true, "message": message})
}
fn user(agent: &str, content: serde_json::Value) -> serde_json::Value {
    line("user", agent, json!({"role": "user", "content": content}))
}
fn assistant(agent: &str, content: serde_json::Value) -> serde_json::Value {
    line(
        "assistant",
        agent,
        json!({"role": "assistant", "content": content}),
    )
}

fn conversation() -> Vec<serde_json::Value> {
    vec![
        user(AGENT, json!("Find the probe.")),
        assistant(
            AGENT,
            json!([
                {"type": "thinking", "thinking": "hmm"},
                {"type": "text", "text": "Looking."},
                {"type": "tool_use", "id": "c1", "name": "Read", "input": {"file_path": "/w/src/probe.rs"}},
            ]),
        ),
        user(
            AGENT,
            json!([{"type": "tool_result", "tool_use_id": "c1", "content": "fn probe() {}"}]),
        ),
        json!({"type": "attachment", "agentId": AGENT, "sessionId": SESSION}),
        json!({"type": "user", "agentId": AGENT, "isMeta": true, "message": {"role": "user", "content": "caveat"}}),
        user("other", json!("Not mine.")),
        assistant(
            AGENT,
            json!([
                {"type": "tool_use", "id": "c2", "name": "Bash", "input": {"command": "cat x", "description": "Show x"}},
            ]),
        ),
        user(
            AGENT,
            json!([{"type": "tool_result", "tool_use_id": "c2",
            "content": [{"type": "text", "text": "no such file"}], "is_error": true}]),
        ),
        assistant(AGENT, json!([{"type": "text", "text": "Found it."}])),
    ]
}

#[test]
fn entries_come_in_order_without_thinking_meta_attachments_or_other_agents() {
    let (studio, logs, _dir) = setup("s1.jsonl");
    write(&logs, &conversation());

    let log = studio.subagent_log(SESSION, AGENT, 0).expect("a log");
    assert_eq!(
        log.entries,
        vec![
            LogEntry::Prompt {
                text: "Find the probe.".into()
            },
            LogEntry::Reply {
                text: "Looking.".into()
            },
            LogEntry::Call {
                id: "c1".into(),
                tool: "Read".into(),
                detail: Some("src/probe.rs".into())
            },
            LogEntry::Result {
                id: "c1".into(),
                text: "fn probe() {}".into(),
                failed: false,
                cut: false
            },
            LogEntry::Call {
                id: "c2".into(),
                tool: "Bash".into(),
                detail: Some("Show x".into())
            },
            LogEntry::Result {
                id: "c2".into(),
                text: "no such file".into(),
                failed: true,
                cut: false
            },
            LogEntry::Reply {
                text: "Found it.".into()
            },
        ]
    );
}

#[test]
fn a_long_result_is_capped_and_marked_cut() {
    let (studio, logs, _dir) = setup("s1.jsonl");
    let long = "x".repeat(10_000);
    write(
        &logs,
        &[user(
            AGENT,
            json!([{"type": "tool_result", "tool_use_id": "c1", "content": long}]),
        )],
    );

    let log = studio.subagent_log(SESSION, AGENT, 0).unwrap();
    let [LogEntry::Result { text, cut, .. }] = &log.entries[..] else {
        panic!("one result")
    };
    assert!(*cut);
    assert!(text.chars().count() < 10_000 && text.chars().count() >= 1_000);
}

#[test]
fn reading_on_from_the_offset_returns_only_new_complete_lines() {
    let (studio, logs, _dir) = setup("s1.jsonl");
    let path = logs.join(format!("agent-{AGENT}.jsonl"));
    write(&logs, &conversation()[..1]);
    let first = studio.subagent_log(SESSION, AGENT, 0).unwrap();
    assert_eq!(first.entries.len(), 1);

    // A reply, then a line Claude Code is still writing and a line that does not parse.
    let more = format!(
        "{}\nnot json\n{}",
        assistant(AGENT, json!([{"type": "text", "text": "Next."}])),
        r#"{"type":"assistant","agentId":"#
    );
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str(&more);
    fs::write(&path, text).unwrap();

    let second = studio.subagent_log(SESSION, AGENT, first.offset).unwrap();
    assert_eq!(
        second.entries,
        vec![LogEntry::Reply {
            text: "Next.".into()
        }]
    );
    // The unfinished line waits: reading on from here finds it once finished.
    let third = studio.subagent_log(SESSION, AGENT, second.offset).unwrap();
    assert!(third.entries.is_empty());
    assert_eq!(third.offset, second.offset);
}

#[test]
fn a_missing_log_an_unknown_subagent_and_a_non_jsonl_path_read_as_no_log() {
    let (studio, _logs, _dir) = setup("s1.jsonl");
    assert_eq!(studio.subagent_log(SESSION, AGENT, 0), None);

    let (studio, logs, _dir) = setup("s1.jsonl");
    write(&logs, &conversation());
    assert_eq!(studio.subagent_log(SESSION, "nobody", 0), None);
    assert_eq!(studio.subagent_log("nosession", AGENT, 0), None);

    // The transcript path has no `.jsonl`, so the folder cannot be worked out.
    let (studio, logs, _dir) = setup("s1.txt");
    write(&logs, &conversation());
    assert_eq!(studio.subagent_log(SESSION, AGENT, 0), None);
}
