//! Seam 1 (danielrispler/Falinks#469): replay hook events through the core and check what closing and renaming a session
//! rest on. The studio asks before closing exactly when the session runs or has background tasks
//! (`Stop`'s `background_tasks`), and a session's name is a line of the event log, so a restart,
//! which replays the log, keeps it.

use serde_json::{Value, json};
use studio_core::{State, Studio};

const ID: &str = "5b1e0c7a-2d94-4f63-a8e1-9c3f6d20b417";

fn event(name: &str, at: u64, extra: Value) -> String {
    let mut event = json!({
        "session_id": ID,
        "hook_event_name": name,
        "cwd": "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-469-close-rename",
        "studio_terminal_id": "claude-1",
        "studio_received_ms": at,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
}

fn asks(studio: &Studio) -> bool {
    studio.session(ID).unwrap().asks_before_closing
}

#[test]
fn closing_asks_exactly_while_the_session_runs_or_has_background_tasks() {
    let mut studio = Studio::default();
    studio.replay(&event(
        "SessionStart",
        1_000,
        json!({ "source": "startup" }),
    ));
    assert!(!asks(&studio), "a fresh session waits on Daniel");

    let prompt = json!({ "prompt": "Add a close button.", "prompt_id": "p-1" });
    studio.replay(&event("UserPromptSubmit", 2_000, prompt));
    assert_eq!(studio.session(ID).unwrap().state, State::Running);
    assert!(asks(&studio), "a running turn");

    // A turn ends with a background command still running.
    let tasks = json!([{ "id": "bash-7", "type": "local_bash", "description": "npm test" }]);
    let stop = json!({ "background_tasks": tasks, "last_assistant_message": "Tests run." });
    studio.replay(&event("Stop", 3_000, stop));
    assert_eq!(studio.session(ID).unwrap().state, State::YourTurn);
    assert!(asks(&studio), "a background task");

    // It returns and wakes the session, whose next turn ends with none.
    let wake = json!({ "prompt": "<task-notification><task-id>bash-7</task-id></task-notification>", "prompt_id": "p-2" });
    studio.replay(&event("UserPromptSubmit", 4_000, wake));
    let stop = json!({ "background_tasks": [], "last_assistant_message": "All pass." });
    studio.replay(&event("Stop", 5_000, stop));
    assert!(!asks(&studio), "its turn ended with no background task");

    // A session that ended runs nothing, whatever its last `Stop` listed.
    studio.replay(&event(
        "UserPromptSubmit",
        6_000,
        json!({ "prompt": "Again.", "prompt_id": "p-3" }),
    ));
    let stop = json!({ "background_tasks": tasks, "last_assistant_message": "Started." });
    studio.replay(&event("Stop", 7_000, stop));
    assert!(asks(&studio));
    studio.replay(&event(
        "SessionEnd",
        8_000,
        json!({ "reason": "prompt_input_exit" }),
    ));
    assert!(!asks(&studio), "an ended session");

    let row = serde_json::to_value(studio.session(ID).unwrap()).unwrap();
    assert_eq!(row["asks_before_closing"], false);
}

#[test]
fn a_renamed_session_keeps_its_name_when_the_log_is_replayed() {
    let log = [
        event("SessionStart", 1_000, json!({ "source": "startup" })),
        event("UserPromptSubmit", 2_000, json!({ "prompt": "Go.", "prompt_id": "p-1" })),
        // The line the shell appends when Daniel renames the session.
        json!({ "session_id": ID, "hook_event_name": "StudioRename", "studio_name": "close button", "studio_received_ms": 2_500 }).to_string(),
    ];
    // A restart replays the whole log.
    let studio = Studio::replayed(&log);
    let session = studio.session(ID).unwrap();
    assert_eq!(session.name.as_deref(), Some("close button"));
    // The rename is not the session's event: it keeps its terminal and state.
    assert_eq!(session.terminal.as_deref(), Some("claude-1"));
    assert_eq!(session.state, State::Running);
    let row = serde_json::to_value(session).unwrap();
    assert_eq!(row["name"], "close button");

    // An empty name gives the row its folder's again.
    let mut studio = studio;
    let clear = json!({ "session_id": ID, "hook_event_name": "StudioRename", "studio_name": "", "studio_received_ms": 3_000 });
    studio.replay(&clear.to_string());
    assert_eq!(studio.session(ID).unwrap().name, None);
}

#[test]
fn a_rename_of_an_unknown_session_adds_no_row() {
    let rename = json!({ "session_id": "nobody", "hook_event_name": "StudioRename", "studio_name": "x", "studio_received_ms": 1 });
    let studio = Studio::replayed([rename.to_string()]);
    assert!(studio.sessions().is_empty());
}

// Story 15, decision 8: after Close, the shell appends a `SessionEnd` for each session of the closed
// terminal, as Claude Code may die before its own hook runs.
#[test]
fn the_shells_end_line_after_close_ends_the_row_and_a_late_real_one_is_harmless() {
    let mut studio = Studio::default();
    studio.apply(&event(
        "SessionStart",
        1_000,
        json!({ "source": "startup" }),
    ));
    let prompt = json!({ "prompt": "Go.", "prompt_id": "p-1" });
    studio.apply(&event("UserPromptSubmit", 2_000, prompt));
    let folder = studio.session(ID).unwrap().folder_name.clone();

    let closed = json!({ "session_id": ID, "hook_event_name": "SessionEnd", "reason": "studio_close", "studio_received_ms": 3_000 });
    assert!(studio.apply(&closed.to_string()).is_empty());
    let session = studio.session(ID).unwrap();
    assert_eq!(session.state, State::Ended);
    assert_eq!(session.folder_name, folder, "the line has no cwd");

    let late = event("SessionEnd", 3_100, json!({ "reason": "other" }));
    assert!(studio.apply(&late).is_empty());
    assert_eq!(studio.session(ID).unwrap().state, State::Ended);
    assert_eq!(studio.sessions().len(), 1);
}

// Delete: a `StudioDelete` line hides the row, after a restart too, and drops the selection; a late
// `SessionEnd` leaves it hidden, and a `SessionStart` of it brings it back.
#[test]
fn a_deleted_session_leaves_the_list_until_it_starts_again() {
    let delete =
        json!({ "session_id": ID, "hook_event_name": "StudioDelete", "studio_received_ms": 3_000 });
    let log = [
        event("SessionStart", 1_000, json!({ "source": "startup" })),
        event(
            "SessionEnd",
            2_000,
            json!({ "reason": "prompt_input_exit" }),
        ),
    ];
    let mut studio = Studio::replayed(&log);
    studio.select(ID);
    studio.apply(&delete.to_string());
    assert!(studio.sessions().is_empty());
    assert!(studio.selected().is_none(), "the view no longer shows it");

    let replayed = Studio::replayed(log.iter().cloned().chain([delete.to_string()]));
    assert!(replayed.sessions().is_empty(), "a restart keeps it deleted");

    let mut studio = replayed;
    studio.apply(&event("SessionEnd", 3_100, json!({ "reason": "other" })));
    assert!(studio.sessions().is_empty(), "a late end");
    studio.apply(&event("SessionStart", 4_000, json!({ "source": "resume" })));
    assert_eq!(studio.sessions().len(), 1, "a resume");
}
