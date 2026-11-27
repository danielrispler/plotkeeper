//! Seam 1 (danielrispler/Falinks#469): replay hook events through the core and check what closing and renaming a session
//! rest on. A session's name is a line of the event log, so a restart, which replays the log,
//! keeps it.

use crate::common;
use serde_json::{Value, json};
use studio_core::{State, Studio};

const ID: &str = "5b1e0c7a-2d94-4f63-a8e1-9c3f6d20b417";

fn event(name: &str, at: u64, extra: Value) -> String {
    let cwd = "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-469-close-rename";
    let base = common::base(ID, name, cwd, at);
    common::with(base, extra)
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
    // The rename is not the session's event: it keeps its state.
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

// A deleted session stays out of the list while its tool and prompt events keep arriving; only a
// resume's `SessionStart` brings it back.
#[test]
fn a_deleted_session_stays_deleted_while_its_events_keep_arriving() {
    let mut studio = Studio::default();
    studio.apply(&event(
        "SessionStart",
        1_000,
        json!({ "source": "startup" }),
    ));
    assert_eq!(studio.sessions().len(), 1);
    studio.apply(
        &json!({ "session_id": ID, "hook_event_name": "StudioDelete", "studio_received_ms": 2_000 })
            .to_string(),
    );
    assert!(studio.sessions().is_empty());

    studio.apply(&event(
        "UserPromptSubmit",
        3_000,
        json!({ "prompt": "More.", "prompt_id": "p-1" }),
    ));
    studio.apply(&event(
        "PreToolUse",
        3_100,
        json!({ "tool_name": "Bash", "tool_input": { "command": "ls" }, "tool_use_id": "t-1" }),
    ));
    studio.apply(&event("Stop", 3_200, json!({})));
    assert!(
        studio.sessions().is_empty(),
        "its events do not bring it back"
    );

    studio.apply(&event("SessionStart", 4_000, json!({ "source": "resume" })));
    assert_eq!(studio.sessions().len(), 1, "a resume does");
}
