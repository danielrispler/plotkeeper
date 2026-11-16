//! #101: which terminal a session's row links to. Every hook event of a studio-run `claude`
//! carries its terminal, but an event may lack one (a hook outside the studio's terminal), and a
//! conversation may run in two terminals at once. The row keeps its terminal through the first and
//! stays on one through the second, so the pane never shows "Open here" for a terminal that runs.

use serde_json::{Value, json};
use studio_core::{Studio, Terminal};

const ID: &str = "5b1e0c7a-2d94-4f63-a8e1-9c3f6d20b417";

fn event(name: &str, at: u64, terminal: Option<&str>, extra: Value) -> String {
    let mut event = json!({
        "session_id": ID,
        "hook_event_name": name,
        "cwd": "/tmp",
        "studio_received_ms": at,
    });
    let fields = event.as_object_mut().unwrap();
    if let Some(terminal) = terminal {
        fields.insert("studio_terminal_id".into(), terminal.into());
    }
    fields.extend(extra.as_object().unwrap().clone());
    event.to_string()
}

fn tool(at: u64, terminal: Option<&str>) -> String {
    let input = json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } });
    event("PostToolUse", at, terminal, input)
}

fn terminals(ids: &[&str]) -> Vec<Terminal> {
    let rows = ids
        .iter()
        .map(|id| json!({ "id": id, "argv": ["claude"], "cwd": "/tmp" }));
    serde_json::from_value(Value::Array(rows.collect())).unwrap()
}

fn terminal(studio: &Studio) -> Option<String> {
    studio.session(ID).unwrap().terminal.clone()
}

fn started(terminal: &str) -> Studio {
    let mut studio = Studio::default();
    let start = json!({ "source": "startup" });
    studio.replay(&event("SessionStart", 1_000, Some(terminal), start));
    studio
}

#[test]
fn an_event_with_no_terminal_keeps_the_sessions_terminal() {
    let mut studio = started("claude-a");
    studio.replay(&tool(2_000, None));
    assert_eq!(terminal(&studio).as_deref(), Some("claude-a"));
}

#[test]
fn a_start_with_no_terminal_moves_the_session_out_of_the_studio() {
    let mut studio = started("claude-a");
    let resume = json!({ "source": "resume" });
    studio.replay(&event("SessionStart", 2_000, None, resume));
    assert_eq!(
        terminal(&studio),
        None,
        "resumed in a terminal the studio does not run"
    );
}

#[test]
fn a_session_run_by_two_live_terminals_stays_on_one() {
    let mut studio = started("claude-a");
    studio.terminals(&terminals(&["claude-a", "claude-b"]), 1_500);
    for (i, who) in ["claude-b", "claude-a", "claude-b", "claude-a"]
        .into_iter()
        .enumerate()
    {
        studio.apply(&tool(2_000 + i as u64, Some(who)));
        assert_eq!(
            terminal(&studio).as_deref(),
            Some("claude-a"),
            "after event {i}"
        );
    }
}

#[test]
fn a_session_follows_its_conversation_when_the_old_terminal_is_gone() {
    let mut studio = started("claude-a");
    studio.terminals(&terminals(&["claude-b"]), 1_500);
    studio.apply(&tool(2_000, Some("claude-b")));
    assert_eq!(terminal(&studio).as_deref(), Some("claude-b"));
}

#[test]
fn a_start_in_another_live_terminal_moves_the_session() {
    let mut studio = started("claude-a");
    studio.terminals(&terminals(&["claude-a", "claude-b"]), 1_500);
    let resume = json!({ "source": "resume" });
    studio.apply(&event("SessionStart", 2_000, Some("claude-b"), resume));
    assert_eq!(terminal(&studio).as_deref(), Some("claude-b"));
}

#[test]
fn before_the_first_terminal_list_a_second_terminals_event_does_not_move_the_session() {
    let mut studio = started("claude-a");
    for (i, who) in ["claude-b", "claude-a", "claude-b"].into_iter().enumerate() {
        studio.replay(&tool(2_000 + i as u64, Some(who)));
        assert_eq!(
            terminal(&studio).as_deref(),
            Some("claude-a"),
            "after event {i}"
        );
    }
}
