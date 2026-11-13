//! Seam 1 (danielrispler/Falinks#562): one conversation is one row. "herdr grilling" showed as 4 rows. Claude Code
//! forked it mid-turn into a new id (`SessionStart` from `fork`), after a throwaway `startup` id on
//! the same terminal, and the old id went on sending late events. A `/resume` from a fresh `claude`
//! leaves a throwaway `startup` id too, which ends with reason `resume`.

use serde_json::{Value, json};
use studio_core::{State, Studio};

const FOLDER: &str = "/Users/danielrispler/work/Falinks";
const TERMINAL: &str = "claude-4cde64b0";
const OLD: &str = "d43167d0-78fa-4ce2-9fc0-8268a653fbfe";
const THROWAWAY: &str = "8bfb6191-cf3e-4c02-b28e-165efd3c5585";
const FORK: &str = "e0842765-2d76-4242-844e-e30a23f75973";

fn event(id: &str, name: &str, at: u64, extra: Value) -> String {
    let mut event = json!({
        "session_id": id,
        "hook_event_name": name,
        "cwd": FOLDER,
        "studio_terminal_id": TERMINAL,
        "transcript_path": format!("/h/.claude/projects/x/{id}.jsonl"),
        "studio_received_ms": at,
    });
    let fields = event.as_object_mut().unwrap();
    fields.extend(extra.as_object().unwrap().clone());
    event.to_string()
}

/// A session that asked a question Daniel answered, then forked mid-turn, as on 2026-11-09.
fn forked() -> Vec<String> {
    let ask = json!({
        "tool_name": "AskUserQuestion",
        "tool_use_id": "toolu_ask",
        "tool_input": { "questions": [{ "question": "Which base?", "options": [] }] },
        "prompt_id": "p-1",
    });
    let mut answered = ask.clone();
    answered["tool_response"] = json!({ "answers": { "Which base?": "origin/main" } });
    let read = json!({ "tool_name": "Read", "tool_use_id": "toolu_read", "tool_input": {}, "prompt_id": "p-1" });
    let edit = json!({ "tool_name": "Edit", "tool_use_id": "toolu_edit", "tool_input": {}, "prompt_id": "p-1" });
    vec![
        event(OLD, "SessionStart", 1_000, json!({ "source": "startup" })),
        event(OLD, "UserPromptSubmit", 2_000, json!({ "prompt": "Grill me.", "prompt_id": "p-1" })),
        event(OLD, "PreToolUse", 3_000, ask),
        event(OLD, "PostToolUse", 3_500, answered),
        json!({ "session_id": OLD, "hook_event_name": "StudioRename", "studio_name": "herdr grilling", "studio_received_ms": 3_600 }).to_string(),
        event(OLD, "PreToolUse", 3_700, read.clone()),
        event(OLD, "PostToolUse", 3_800, read),
        event(THROWAWAY, "SessionStart", 4_000, json!({ "source": "startup", "agent_type": "claude" })),
        event(FORK, "SessionStart", 4_250, json!({ "source": "fork", "session_title": "herdr grilling" })),
        event(FORK, "PreToolUse", 5_000, edit.clone()),
        event(FORK, "PostToolUse", 5_100, edit),
        event(FORK, "Stop", 6_000, json!({ "last_assistant_message": "Pick 1 or 2." })),
        // The old id's late event still lands on the row.
        event(OLD, "Notification", 6_100, json!({ "notification_type": "agent_completed", "message": "done" })),
    ]
}

#[test]
fn a_fork_mid_turn_continues_the_row_with_its_history() {
    let studio = Studio::replayed(forked());

    assert_eq!(studio.sessions().len(), 1, "one row, not 3");
    let row = studio.sessions()[0];
    assert_eq!(row.agent.id, OLD);
    assert_eq!(row.name.as_deref(), Some("herdr grilling"));
    assert_eq!(row.prompts.len(), 1, "its history stays");
    assert_eq!(row.questions.len(), 1, "its decisions stay");
    assert!(row.cleared.is_empty(), "a fork is no clear");
    assert_eq!(row.state, State::YourTurn, "the fork's Stop ends the turn");
    assert_eq!(
        row.resume_id, FORK,
        "`claude --resume` takes the fork, not the old id"
    );
    assert_eq!(studio.session(FORK).unwrap().agent.id, OLD);
}

#[test]
fn a_resume_from_a_fresh_claude_shows_only_the_resumed_row() {
    let mut lines = forked();
    lines.truncate(5);
    lines.push(event(
        OLD,
        "SessionEnd",
        4_000,
        json!({ "reason": "other" }),
    ));
    // Daniel starts `claude` in a terminal and picks the session with `/resume`, twice.
    for (throwaway, at) in [(THROWAWAY, 5_000), (FORK, 7_000)] {
        lines.extend([
            event(
                throwaway,
                "SessionStart",
                at,
                json!({ "source": "startup" }),
            ),
            event(
                throwaway,
                "SessionEnd",
                at + 500,
                json!({ "reason": "resume" }),
            ),
            event(OLD, "SessionStart", at + 600, json!({ "source": "resume" })),
        ]);
    }
    let studio = Studio::replayed(lines);

    assert_eq!(studio.sessions().len(), 1, "no row for the throwaway ids");
    let row = studio.sessions()[0];
    assert_eq!(row.agent.id, OLD);
    assert_eq!(row.prompts.len(), 1);
    assert_eq!(row.state, State::YourTurn);
}

/// `74034bf9` on 2026-11-09: a second `claude` from the fork's terminal loaded an old session in the
/// background, likely from the `/resume` picker backed out of, and never got a prompt.
#[test]
fn a_prompt_less_resume_beside_a_live_session_is_no_row_until_its_first_prompt() {
    const LOADED: &str = "74034bf9-6b2e-4a8e-9d51-0c3f2e7a1b44";
    let mut lines = forked();
    lines.push(event(
        LOADED,
        "SessionStart",
        7_000,
        json!({ "source": "resume" }),
    ));
    let studio = Studio::replayed(lines.clone());

    assert_eq!(studio.sessions().len(), 1, "no row for the loaded session");
    assert_eq!(studio.sessions()[0].agent.id, OLD);

    lines.push(event(
        LOADED,
        "UserPromptSubmit",
        8_000,
        json!({ "prompt": "Go on.", "prompt_id": "p-2" }),
    ));
    let studio = Studio::replayed(lines);

    assert_eq!(
        studio.sessions().len(),
        2,
        "its first prompt makes it a row"
    );
    assert!(studio.sessions().iter().any(|s| s.agent.id == LOADED));
}
