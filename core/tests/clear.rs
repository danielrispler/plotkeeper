//! Seam 1 (#474, #461 decision 18): `/clear` keeps the session's row. Claude Code ends the session
//! with reason `clear` and starts a new one, under a new id, with source `clear`. The new one continues
//! the row on the same studio terminal, or in the same folder outside studio terminals.

use serde_json::{Value, json};
use studio_core::{State, Studio};

const FOLDER: &str = "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-474-clear";
const OLD: &str = "5b1e0c7a-2d94-4f63-a8e1-9c3f6d20b417";
const NEW: &str = "9a7d3e21-0c4b-4e8f-b6a2-71d5c0e9f384";
const OTHER: &str = "c3f0a8d2-6e17-4b59-9d4c-2a8e5f1b7c06";

fn event(id: &str, name: &str, at: u64, terminal: Option<&str>, extra: Value) -> String {
    let mut event = json!({
        "session_id": id,
        "hook_event_name": name,
        "cwd": FOLDER,
        "studio_terminal_id": terminal,
        "studio_received_ms": at,
    });
    let fields = event.as_object_mut().unwrap();
    fields.extend(extra.as_object().unwrap().clone());
    event.to_string()
}

/// A session that asked a question Daniel answered, renamed, then cleared, in `terminal`.
fn cleared(id: &str, new: &str, terminal: Option<&str>) -> Vec<String> {
    let ask = json!({
        "tool_name": "AskUserQuestion",
        "tool_use_id": "toolu_ask",
        "tool_input": { "questions": [{ "question": "Which base?", "options": [] }] },
        "prompt_id": "p-1",
    });
    let mut answered = ask.clone();
    answered["tool_response"] = json!({ "answers": { "Which base?": "origin/main" } });
    vec![
        event(id, "SessionStart", 1_000, terminal, json!({ "source": "startup" })),
        event(id, "UserPromptSubmit", 2_000, terminal, json!({ "prompt": "Plan it.", "prompt_id": "p-1" })),
        event(id, "PreToolUse", 3_000, terminal, ask),
        event(id, "PostToolUse", 4_000, terminal, answered),
        event(id, "Stop", 5_000, terminal, json!({ "last_assistant_message": "Done." })),
        json!({ "session_id": id, "hook_event_name": "StudioRename", "studio_name": "Clear keeps the row", "studio_received_ms": 6_000 }).to_string(),
        event(id, "SessionEnd", 7_000, terminal, json!({ "reason": "clear" })),
        event(new, "SessionStart", 7_100, terminal, json!({ "source": "clear" })),
    ]
}

#[test]
fn a_clear_on_a_studio_terminal_continues_the_row_with_a_cleared_marker() {
    let mut studio = Studio::replayed(cleared(OLD, NEW, Some("claude-1")));

    assert_eq!(studio.sessions().len(), 1, "one row, not a new one");
    let row = studio.sessions()[0];
    assert_eq!(row.name.as_deref(), Some("Clear keeps the row"));
    assert_eq!(row.questions.len(), 1, "its decisions stay");
    assert_eq!(row.prompts.len(), 1, "its history stays");
    assert_eq!(row.cleared, vec![7_100]);
    assert_eq!(row.state, State::YourTurn);
    assert_eq!(
        row.resume_id, NEW,
        "`claude --resume` takes the new conversation"
    );

    // The new conversation's events, and a rename the row's view sends, land on the same row.
    studio.replay(&event(
        NEW,
        "UserPromptSubmit",
        8_000,
        Some("claude-1"),
        json!({ "prompt": "Next.", "prompt_id": "p-2" }),
    ));
    let rename = json!({ "session_id": NEW, "hook_event_name": "StudioRename", "studio_name": "Renamed", "studio_received_ms": 9_000 });
    studio.replay(&rename.to_string());
    assert_eq!(studio.sessions().len(), 1);
    let row = studio.sessions()[0];
    assert_eq!(row.state, State::Running);
    assert_eq!(row.prompts.len(), 2);
    assert_eq!(row.name.as_deref(), Some("Renamed"));
}

#[test]
fn a_clear_outside_studio_terminals_continues_the_row_by_folder() {
    let studio = Studio::replayed(cleared(OLD, NEW, None));

    assert_eq!(studio.sessions().len(), 1);
    let row = studio.sessions()[0];
    assert_eq!(row.cleared, vec![7_100]);
    assert_eq!(row.name.as_deref(), Some("Clear keeps the row"));
    assert_eq!(row.resume_id, NEW);
}

#[test]
fn of_two_sessions_in_one_folder_only_the_cleared_one_continues() {
    let mut lines = vec![
        event(
            OTHER,
            "SessionStart",
            500,
            None,
            json!({ "source": "startup" }),
        ),
        event(
            OTHER,
            "UserPromptSubmit",
            600,
            None,
            json!({ "prompt": "Other work.", "prompt_id": "o-1" }),
        ),
    ];
    lines.extend(cleared(OLD, NEW, None));
    let studio = Studio::replayed(lines);

    assert_eq!(studio.sessions().len(), 2);
    let other = studio.session(OTHER).unwrap();
    assert!(other.cleared.is_empty(), "the other one is untouched");
    assert_eq!(other.state, State::Running);
    assert_eq!(other.resume_id, OTHER);
    let row = studio.session(OLD).unwrap();
    assert_eq!(row.cleared, vec![7_100]);
    assert_eq!(row.resume_id, NEW);
}

#[test]
fn a_session_end_with_another_reason_still_ends_the_row() {
    let mut lines = cleared(OLD, NEW, Some("claude-1"));
    lines.truncate(6);
    lines.push(event(
        OLD,
        "SessionEnd",
        7_000,
        Some("claude-1"),
        json!({ "reason": "prompt_input_exit" }),
    ));
    // A later fresh session on the same terminal is a new row.
    lines.push(event(
        NEW,
        "SessionStart",
        8_000,
        Some("claude-1"),
        json!({ "source": "startup" }),
    ));
    let studio = Studio::replayed(lines);

    assert_eq!(studio.sessions().len(), 2);
    let old = studio.session(OLD).unwrap();
    assert_eq!(old.state, State::Ended);
    assert!(old.cleared.is_empty());
    assert_eq!(studio.session(NEW).unwrap().name, None);
}

/// The checkpoint day's log (#370) holds two real clears, each a `SessionEnd` then a `SessionStart`
/// about 60 ms later, in the main checkout and outside studio terminals.
#[test]
fn the_checkpoint_days_clears_each_keep_their_row() {
    let studio = Studio::replayed(include_str!("fixtures/checkpoint-day.jsonl").lines());

    for (old, new) in [
        (
            "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b",
            "6641e809-ea82-462a-8082-497560f88bae",
        ),
        (
            "5bb84a4a-6f08-46b4-8329-b0b88df06518",
            "0222b62d-188d-42fa-8baf-8353313fc719",
        ),
    ] {
        let row = studio.session(new).expect("the row");
        assert_eq!(row.agent.id, old, "{new} continues {old}'s row");
        assert_eq!(row.cleared.len(), 1);
    }
}
