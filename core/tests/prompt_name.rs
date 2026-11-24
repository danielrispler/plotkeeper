//! Seam 1 (danielrispler/plotkeeper#53, "Session name"): replay hook events through the core and
//! check when a session asks for its name from Daniel's first prompt, and that the written name,
//! a `StudioPromptName` line of the log, shows, yields to his rename and survives a replay.

use crate::common;
use serde_json::{Value, json};
use studio_core::{NameRequest, Output, Studio};

const ID: &str = "8c2f4b6d-1a3e-4d57-9b80-7e5a2c1f3d96";
/// A written name of 7 words, which the 6-word cut ended mid-phrase before #444.
const SEVEN: &str = "Merge the hook event types into one";
const CWD: &str = "/Users/danielrispler/work/plotkeeper/.claude/worktrees/issue-37-one-event";

fn event(name: &str, at: u64, extra: Value) -> String {
    common::event(ID, name, CWD, at, extra)
}

fn prompt(at: u64, text: &str) -> String {
    event("UserPromptSubmit", at, json!({ "prompt": text }))
}

fn stop(at: u64, reply: &str) -> String {
    event("Stop", at, json!({ "last_assistant_message": reply }))
}

fn named(outputs: Vec<Output>) -> Vec<NameRequest> {
    outputs
        .into_iter()
        .filter_map(|o| match o {
            Output::Name(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn written(name: &str) -> String {
    json!({ "session_id": ID, "hook_event_name": "StudioPromptName", "studio_name": name, "studio_received_ms": 9_000 }).to_string()
}

#[test]
fn the_first_prompt_asks_for_a_name_once() {
    let mut studio = Studio::default();
    let first = named(studio.apply(&prompt(1_000, "Merge the event types into one")));
    assert_eq!(
        first,
        [NameRequest {
            session_id: ID.into(),
            prompt: "Merge the event types into one".into(),
            response: None,
            cwd: CWD.into(),
        }]
    );
    studio.apply(&stop(2_000, "Done."));
    assert!(named(studio.apply(&prompt(3_000, "Now the tests"))).is_empty());
}

#[test]
fn a_wake_up_never_asks() {
    let mut studio = Studio::default();
    studio.apply(&prompt(1_000, "Go"));
    studio.apply(&event(
        "PostToolUse",
        1_500,
        json!({ "tool_name": "Bash", "tool_input": { "command": "sleep 9", "run_in_background": true }, "tool_response": { "backgroundTaskId": "bash-7" }, "tool_use_id": "t1" }),
    ));
    studio.apply(&stop(2_000, "Started."));
    let wake = common::task_notification("bash-7", "completed");
    assert!(named(studio.apply(&prompt(3_000, &wake))).is_empty());
}

#[test]
fn a_bare_slash_command_asks_at_its_turns_end_with_the_response() {
    let mut studio = Studio::default();
    assert!(named(studio.apply(&prompt(1_000, "/wayfinder"))).is_empty());
    let asked = named(studio.apply(&stop(2_000, "Claimed #37, One event internal.")));
    assert_eq!(
        asked,
        [NameRequest {
            session_id: ID.into(),
            prompt: "/wayfinder".into(),
            response: Some("Claimed #37, One event internal.".into()),
            cwd: CWD.into(),
        }]
    );
    // Only once.
    studio.apply(&prompt(3_000, "Go on"));
    assert!(named(studio.apply(&stop(4_000, "Done."))).is_empty());

    // A slash command with text after it is named at once.
    let mut text = Studio::default();
    assert_eq!(
        named(text.apply(&prompt(1_000, "/review the parser"))).len(),
        1
    );
}

#[test]
fn a_replayed_prompt_asks_for_nothing() {
    let mut studio = Studio::default();
    studio.replay(&prompt(1_000, "Fix the bug"));
    studio.replay(&prompt(2_000, "/wayfinder"));
    assert!(named(studio.apply(&stop(3_000, "Done."))).is_empty());
    assert!(named(studio.apply(&prompt(4_000, "More"))).is_empty());
}

#[test]
fn the_written_name_shows_loses_to_a_rename_and_survives_a_replay() {
    // A written name of 7 words is kept whole: the writer's reply is no longer cut at 6 (#444).
    let log = [prompt(1_000, "Merge the event types"), written(SEVEN)];
    let mut studio = Studio::replayed(&log);
    assert_eq!(
        studio.session(ID).unwrap().prompt_name.as_deref(),
        Some(SEVEN)
    );
    let row = serde_json::to_value(studio.session(ID).unwrap()).unwrap();
    assert_eq!(row["prompt_name"], SEVEN);
    assert_eq!(row["name"], Value::Null);

    // His rename is kept beside it; an empty one gives the prompt name back.
    let rename = |name: &str| {
        json!({ "session_id": ID, "hook_event_name": "StudioRename", "studio_name": name, "studio_received_ms": 9_500 }).to_string()
    };
    studio.replay(&rename("my name"));
    let session = studio.session(ID).unwrap();
    assert_eq!(session.name.as_deref(), Some("my name"));
    assert_eq!(session.prompt_name.as_deref(), Some(SEVEN));
    studio.replay(&rename(""));
    assert_eq!(studio.session(ID).unwrap().name, None);
    assert_eq!(
        studio.session(ID).unwrap().prompt_name.as_deref(),
        Some(SEVEN)
    );

    // A name for an unknown session adds no row.
    let nobody = written("x").replace(ID, "nobody");
    assert!(Studio::replayed([nobody]).sessions().is_empty());
}

#[test]
fn the_issue_prefix_follows_the_identity() {
    let mut studio = Studio::replayed([prompt(1_000, "Merge"), written("One event type")]);
    let row = |studio: &Studio| serde_json::to_value(studio.session(ID).unwrap()).unwrap();
    // The folder's branch is not known until the transcript says so; the claim names the issue.
    assert_eq!(row(&studio)["identity"]["issue"], Value::Null);
    studio.replay(&event(
        "PostToolUse",
        2_000,
        json!({ "tool_name": "Bash", "tool_input": { "command": "cargo run --bin wayfinder-startup -- --claim 37" }, "tool_response": {}, "tool_use_id": "t2" }),
    ));
    assert_eq!(row(&studio)["identity"]["issue"], 37);
    assert_eq!(row(&studio)["prompt_name"], "One event type");
}
