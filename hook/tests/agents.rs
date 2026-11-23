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

/// The hook records turn trees from Claude Code's event names only, so a harness whose capabilities
/// say `turn_trees: false` gets none, though its payload is Claude-shaped (#359; spec #320, "Turn
/// trees and pruning per harness").
#[test]
fn the_hook_keeps_no_turn_of_an_agent_other_than_claude() {
    let prompt = json!({ "session_id": "s-1", "hook_event_name": "UserPromptSubmit", "cwd": "/", "turn_id": "t-1", "prompt": "go" });
    for (agent, kept) in [("claude", true), ("codex", false), ("qwen", false)] {
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
