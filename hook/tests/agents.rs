//! Each hook entry names its agent, and the hook stores the name on the line (#355; #320,
//! Harnesses, "Each hook entry names its agent").
use crate::common::{feed, hook, scratch};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Stdio};

/// The line `plotkeeper-hook` with `args` logs for `payload`.
fn logged(args: &[&str], payload: Value, name: &str) -> Value {
    let home = scratch(&format!("agents-{name}"));
    let mut cmd = hook(&home);
    cmd.args(args).stdout(Stdio::null());
    assert!(feed(cmd, &payload).wait().unwrap().success());
    let text = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    serde_json::from_str(text.lines().next().unwrap()).unwrap()
}

#[test]
fn the_hook_stores_the_agent_its_entry_names() {
    let agy = json!({ "conversationId": "c-1", "hookEventName": "PreToolUse", "cwd": "/" });
    let claude = json!({ "session_id": "s-1", "hook_event_name": "Stop", "cwd": "/" });

    assert_eq!(
        logged(&["--agent", "agy"], agy, "agy")["studio_agent"],
        "agy"
    );
    assert_eq!(
        logged(&["--agent", "claude"], claude.clone(), "claude")["studio_agent"],
        "claude"
    );
    assert_eq!(logged(&[], claude, "none").get("studio_agent"), None);
}

/// The commands of `hooks.json` under `dir` at the checkout's root.
fn commands(dir: &str) -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let text = fs::read_to_string(root.join(dir).join("hooks.json")).unwrap();
    fn walk(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                if let Some(c) = o.get("command") {
                    out.push(c.as_str().unwrap().to_owned());
                }
                o.values().for_each(|v| walk(v, out));
            }
            Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(&serde_json::from_str(&text).unwrap(), &mut found);
    found
}

#[test]
fn every_entry_that_records_names_its_agent() {
    let claude = commands("plugin/hooks");
    let recording: Vec<_> = claude
        .iter()
        .filter(|c| c.contains("plotkeeper-hook"))
        .collect();
    assert!(!recording.is_empty());
    for c in recording {
        assert!(c.ends_with("plotkeeper-hook\" --agent claude"), "{c}");
    }
    let agy = commands("plugin-agy");
    assert_eq!(agy.len(), 6);
    for c in agy {
        assert!(c.contains("\"__PLOTKEEPER_HOOK__\" --agent agy "), "{c}");
    }
    let codex = commands("plugin-codex");
    assert_eq!(codex.len(), 8);
    for c in codex {
        assert_eq!(c, "\"__PLOTKEEPER_BIN__/plotkeeper-hook\" --agent codex");
    }
}

/// The hook records turn trees from Claude Code's, Codex's and Cursor's lines only (#426), so a
/// harness whose capabilities say `turn_trees: false` gets none, though its payload is Claude-shaped
/// (#359; spec #320, "Turn trees and pruning per harness").
#[test]
fn the_hook_keeps_no_turn_of_an_agent_whose_turns_it_does_not_map() {
    let prompt = json!({ "session_id": "s-1", "hook_event_name": "UserPromptSubmit", "cwd": "/", "turn_id": "t-1", "prompt": "go" });
    for (agent, kept) in [("claude", true), ("codex", true), ("qwen", false)] {
        let home = scratch(&format!("agents-turns-{agent}"));
        let mut cmd = hook(&home);
        cmd.args(["--agent", agent]).stdout(Stdio::null());
        assert!(feed(cmd, &prompt).wait().unwrap().success());
        let turn = studio_hook::event_log(&home).with_file_name("turns/s-1.json");
        assert_eq!(turn.exists(), kept, "{agent}");
        let text = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
        let line: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(line["studio_agent"], agent);
    }
}

/// Cursor loads Claude Code's hooks, so one Cursor event can reach plotkeeper's Claude entry and its
/// Cursor entry (#320, "One event, one record, the right agent"): the Claude entry drops it, with
/// one line on stderr, and the Cursor entry records it, once, as Cursor's (#362).
#[test]
fn a_cursor_payload_is_dropped_by_the_claude_entry_and_recorded_once_by_cursors() {
    let payload = json!({
        "conversation_id": "c-1",
        "generation_id": "g-1",
        "hook_event_name": "beforeSubmitPrompt",
        "cursor_version": "2.1.4",
        "workspace_roots": ["/"],
        "prompt": "go",
    });
    let home = scratch("agents-cursor-twice");
    let mut stderrs = Vec::new();
    for agent in ["claude", "cursor"] {
        let mut cmd = hook(&home);
        cmd.args(["--agent", agent])
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let out = feed(cmd, &payload).wait_with_output().unwrap();
        assert!(out.status.success());
        stderrs.push(String::from_utf8(out.stderr).unwrap());
    }
    let text = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert_eq!(lines[0]["studio_agent"], "cursor");
    assert_eq!(lines[0]["conversation_id"], "c-1");
    assert_eq!(stderrs[0].lines().count(), 1, "{}", stderrs[0]);
    assert!(stderrs[0].contains("Cursor"), "{}", stderrs[0]);
    assert_eq!(stderrs[1], "");
}

/// Copilot CLI's own payload, camelCase with `sessionId` (recorded in wave 1's trial, #369), is
/// Copilot's marker: the Claude entry drops it with one line on stderr, and Copilot's entry records
/// it once.
#[test]
fn a_copilot_payload_is_dropped_by_the_claude_entry_and_recorded_once_by_copilots() {
    let payload = json!({
        "cwd": "/",
        "prompt": "go",
        "sessionId": "246c4856-1241-4f20-bb9f-ef981a1a7421",
        "timestamp": 1795512039620u64,
    });
    let home = scratch("agents-copilot-twice");
    let mut stderrs = Vec::new();
    for args in [
        &["--agent", "claude"][..],
        &["--agent", "copilot", "--event", "userPromptSubmitted"],
    ] {
        let mut cmd = hook(&home);
        cmd.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
        let out = feed(cmd, &payload).wait_with_output().unwrap();
        assert!(out.status.success());
        stderrs.push(String::from_utf8(out.stderr).unwrap());
    }
    let text = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 1, "{text}");
    assert_eq!(lines[0]["studio_agent"], "copilot");
    assert!(stderrs[0].contains("Copilot"), "{}", stderrs[0]);
    assert_eq!(stderrs[1], "");
}

/// Every entry of Cursor's hook file names Cursor (#362), on the seven events that wait on no answer.
#[test]
fn every_cursor_entry_names_cursor() {
    let cursor = commands("plugin-cursor");
    assert_eq!(cursor.len(), 7);
    for c in cursor {
        assert_eq!(c, "\"__PLOTKEEPER_BIN__/plotkeeper-hook\" --agent cursor");
    }
}

/// A camelCase Copilot payload names no event, so its entry passes it as `--event`, which the hook
/// stores as `studio_event` (#361).
#[test]
fn the_hook_stores_the_event_its_entry_names() {
    let payload = json!({ "sessionId": "cs-1", "timestamp": 1, "cwd": "/", "prompt": "go" });
    let line = logged(
        &["--agent", "copilot", "--event", "userPromptSubmitted"],
        payload,
        "copilot-event",
    );
    assert_eq!(line["studio_agent"], "copilot");
    assert_eq!(line["studio_event"], "userPromptSubmitted");
}

/// Every entry of Copilot's plugin names Copilot and, since its camelCase payloads name no event,
/// its own event (#361).
#[test]
fn every_copilot_entry_names_copilot_and_its_event() {
    let mut copilot = commands("plugin-copilot");
    copilot.sort();
    let mut wired = [
        "agentStop",
        "postToolUse",
        "postToolUseFailure",
        "preToolUse",
        "sessionEnd",
        "sessionStart",
        "userPromptSubmitted",
    ]
    .map(|e| format!("\"__PLOTKEEPER_BIN__/plotkeeper-hook\" --agent copilot --event {e}"));
    wired.sort();
    assert_eq!(copilot, wired);
}

/// OpenCode 2 runs its plugins in one background service (`opencode serve --service`, reparented to
/// init) that outlives every session it runs (#370), so an OpenCode line records no pid: its sessions
/// take the idle path, as Codex's daemon's do (#369). Another agent's line still records its parent.
#[test]
fn an_opencode_line_records_no_pid() {
    let payload = json!({ "session_id": "ses_1", "hook_event_name": "session.prompt", "cwd": "/" });
    let opencode = logged(&["--agent", "opencode"], payload.clone(), "opencode-pid");
    assert_eq!(opencode["studio_agent"], "opencode");
    assert_eq!(opencode.get("studio_pid"), None);
    let claude = logged(&["--agent", "claude"], payload, "opencode-pid-claude");
    assert!(claude["studio_pid"].is_u64());
}
