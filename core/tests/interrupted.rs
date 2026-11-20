//! Seam 1: replay hook events and scripted daemon `list` status through the core. A turn Daniel
//! interrupts fires no `Stop`, and no `idle_prompt` either, so Claude Code's own OSC 7501 `idle`
//! ends it: past `MISMATCH_AFTER_MS` the core asks the shell for a `StudioInterrupted` line, and
//! that line, as a restart replays it, ends the turn.

use serde_json::{Value, json};
use studio_core::{Output, State, Studio, Terminal};

const ID: &str = "a1f0cafd-0e34-46d1-816e-357ce0fccbb2";
const TERMINAL: &str = "claude-ae926462";
const T0: u64 = 1_795_168_437_000;

fn event(name: &str, at: u64, extra: Value) -> String {
    let mut event = json!({
        "session_id": ID,
        "hook_event_name": name,
        "cwd": "/Users/danielrispler/work/plotkeeper",
        "studio_received_ms": at,
        "studio_terminal_id": TERMINAL,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
}

fn prompt(at: u64, text: &str, prompt_id: &str) -> String {
    event(
        "UserPromptSubmit",
        at,
        json!({ "prompt": text, "prompt_id": prompt_id }),
    )
}

/// The line the shell appends for an `Output::Interrupted`.
fn interrupted_line(at: u64, prompt_id: &str) -> String {
    event("StudioInterrupted", at, json!({ "prompt_id": prompt_id }))
}

/// Daniel's terminal in the daemon's `list`, reporting `state` through OSC 7501.
fn term(state: &str, at: u64) -> Terminal {
    serde_json::from_value(json!({
        "id": TERMINAL,
        "argv": ["claude"],
        "cwd": "/Users/danielrispler/work/plotkeeper",
        "pid": 100,
        "exited": null,
        "foreground": "claude",
        "processes": [],
        "state": state,
        "kind": null,
        "app": "claude-code",
        "status_ms": at,
        "turn": 0,
    }))
    .unwrap()
}

fn listed(studio: &mut Studio, state: &str, now: u64) -> Vec<Output> {
    studio.tick(now);
    studio.terminals(&[term(state, now)], now)
}

fn interrupts(outputs: &[Output]) -> Vec<(&str, &str)> {
    outputs
        .iter()
        .filter_map(|o| match o {
            Output::Interrupted {
                session_id,
                prompt_id,
                ..
            } => Some((session_id.as_str(), prompt_id.as_str())),
            _ => None,
        })
        .collect()
}

fn state(studio: &Studio) -> State {
    studio.session(ID).unwrap().state
}

/// A session whose first prompt, `claude`, was cancelled before any reply (the 2026-11-20 run).
fn cancelled_first_prompt() -> Studio {
    let mut studio = Studio::default();
    studio.apply(&event("SessionStart", T0, json!({ "source": "startup" })));
    studio.apply(&prompt(T0 + 2_000, "claude", "p-1"));
    assert_eq!(state(&studio), State::Running);
    studio
}

#[test]
fn a_turn_whose_terminal_stays_idle_asks_for_one_interrupted_line() {
    let mut studio = cancelled_first_prompt();

    let mut out = Vec::new();
    for s in 0..20 {
        out.extend(listed(&mut studio, "idle", T0 + 3_000 + s * 1_000));
    }

    assert_eq!(interrupts(&out), [(ID, "p-1")]);
}

#[test]
fn a_turn_end_reported_as_done_asks_for_none() {
    // At a normal turn's end 7501 says `done` before the `Stop` hook lands, and may linger.
    let mut studio = cancelled_first_prompt();
    let mut out = Vec::new();
    for s in 0..20 {
        out.extend(listed(&mut studio, "done", T0 + 3_000 + s * 1_000));
    }

    assert!(interrupts(&out).is_empty(), "{out:?}");
    assert_eq!(state(&studio), State::Running);
}

#[test]
fn a_brief_idle_asks_for_none() {
    let mut studio = cancelled_first_prompt();
    let mut out = listed(&mut studio, "idle", T0 + 3_000);
    out.extend(listed(&mut studio, "idle", T0 + 6_000));
    out.extend(listed(&mut studio, "working", T0 + 7_000));
    out.extend(listed(&mut studio, "idle", T0 + 10_000));

    assert!(interrupts(&out).is_empty(), "{out:?}");
}

#[test]
fn an_idle_after_a_lasting_done_still_asks() {
    // A new 7501 state starts a new disagreement, with its own line.
    let mut studio = cancelled_first_prompt();
    let mut out = Vec::new();
    for s in 0..10 {
        out.extend(listed(&mut studio, "done", T0 + 3_000 + s * 1_000));
    }
    for s in 0..10 {
        out.extend(listed(&mut studio, "idle", T0 + 13_000 + s * 1_000));
    }

    assert_eq!(interrupts(&out), [(ID, "p-1")]);
}

#[test]
fn the_interrupted_line_ends_its_turn_and_a_replay_does_the_same() {
    let mut studio = cancelled_first_prompt();
    let out = studio.apply(&interrupted_line(T0 + 8_000, "p-1"));

    assert_eq!(state(&studio), State::YourTurn);
    assert!(out.contains(&Output::YourTurn {
        session_id: ID.into()
    }));

    let lines = [
        event("SessionStart", T0, json!({ "source": "startup" })),
        prompt(T0 + 2_000, "claude", "p-1"),
        interrupted_line(T0 + 8_000, "p-1"),
    ];
    let replayed = Studio::replayed(lines.iter().map(String::as_str));
    assert_eq!(replayed.session(ID).unwrap().state, State::YourTurn);
}

#[test]
fn a_late_interrupted_line_leaves_the_next_turn_running() {
    // Daniel sent the next prompt before the shell's line landed.
    let mut studio = cancelled_first_prompt();
    studio.apply(&prompt(T0 + 7_000, "Diagnose the stuck row", "p-2"));
    studio.apply(&interrupted_line(T0 + 8_000, "p-1"));

    assert_eq!(state(&studio), State::Running);
}

#[test]
fn a_late_hook_event_of_the_interrupted_turn_does_not_reopen_it() {
    // The cancelled call's `PostToolUse` may land after the line.
    let mut studio = cancelled_first_prompt();
    studio.apply(&event(
        "PreToolUse",
        T0 + 3_000,
        json!({ "prompt_id": "p-1", "tool_name": "Bash", "tool_input": { "command": "sleep 60" }, "tool_use_id": "t1" }),
    ));
    studio.apply(&interrupted_line(T0 + 9_000, "p-1"));
    studio.apply(&event(
        "PostToolUseFailure",
        T0 + 9_500,
        json!({ "prompt_id": "p-1", "tool_name": "Bash", "tool_input": { "command": "sleep 60" }, "tool_use_id": "t1", "error": "Interrupted", "is_interrupt": true }),
    ));

    assert_eq!(state(&studio), State::YourTurn);
}

fn names(outputs: &[Output]) -> Vec<&str> {
    outputs
        .iter()
        .filter_map(|o| match o {
            Output::Name(request) => Some(request.prompt.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_first_turn_interrupted_before_any_step_is_named_again_from_the_next_prompt() {
    let mut studio = Studio::default();
    studio.apply(&event("SessionStart", T0, json!({ "source": "startup" })));
    let first = studio.apply(&prompt(T0 + 2_000, "claude", "p-1"));
    assert_eq!(names(&first), ["claude"]);
    studio.apply(&interrupted_line(T0 + 8_000, "p-1"));

    let next = studio.apply(&prompt(T0 + 9_000, "Diagnose the stuck row", "p-2"));

    assert_eq!(names(&next), ["Diagnose the stuck row"]);
}

#[test]
fn a_first_turn_ended_by_the_next_prompt_before_any_step_is_named_from_it() {
    // Outside a studio terminal there is no 7501: the next prompt ends the turn.
    let mut studio = Studio::default();
    studio.apply(&prompt(T0 + 2_000, "claude", "p-1"));

    let next = studio.apply(&prompt(T0 + 9_000, "Diagnose the stuck row", "p-2"));

    assert_eq!(names(&next), ["Diagnose the stuck row"]);
}

#[test]
fn a_first_turn_interrupted_after_a_step_keeps_its_name() {
    let mut studio = Studio::default();
    studio.apply(&prompt(T0 + 2_000, "Fix the rail", "p-1"));
    studio.apply(&event(
        "PreToolUse",
        T0 + 3_000,
        json!({ "prompt_id": "p-1", "tool_name": "Read", "tool_input": { "file_path": "/a" }, "tool_use_id": "t1" }),
    ));
    studio.apply(&interrupted_line(T0 + 8_000, "p-1"));

    let next = studio.apply(&prompt(T0 + 9_000, "Only the header", "p-2"));

    assert!(names(&next).is_empty());
}

#[test]
fn a_late_stop_after_the_interrupted_line_tells_nothing_again() {
    // A hook line can land minutes late: the turn's `Stop` after the line that ended it.
    let mut studio = cancelled_first_prompt();
    studio.apply(&interrupted_line(T0 + 8_000, "p-1"));
    let late = studio.apply(&event(
        "Stop",
        T0 + 9_000,
        json!({ "prompt_id": "p-1", "last_assistant_message": "Done.", "background_tasks": [] }),
    ));

    assert_eq!(state(&studio), State::YourTurn);
    assert!(
        !late.iter().any(|o| matches!(o, Output::YourTurn { .. })),
        "{late:?}"
    );
}

#[test]
fn a_permission_wait_asks_for_none() {
    // Needs you comes from the hooks only (danielrispler/Falinks#540); 7501 never changes it.
    let mut studio = cancelled_first_prompt();
    studio.apply(&event(
        "PermissionRequest",
        T0 + 3_000,
        json!({ "prompt_id": "p-1", "tool_name": "Bash", "tool_input": { "command": "rm -rf target" } }),
    ));
    assert_eq!(state(&studio), State::NeedsYou);

    let mut out = Vec::new();
    for state in ["blocked", "idle"] {
        for s in 0..10 {
            out.extend(listed(&mut studio, state, T0 + 4_000 + s * 1_000));
        }
    }

    assert!(interrupts(&out).is_empty(), "{out:?}");
    assert_eq!(state(&studio), State::NeedsYou);
}

#[test]
fn a_long_call_or_a_queued_prompt_reporting_working_asks_for_none() {
    let mut studio = cancelled_first_prompt();
    studio.apply(&event(
        "PreToolUse",
        T0 + 3_000,
        json!({ "prompt_id": "p-1", "tool_name": "Bash", "tool_input": { "command": "npm run check" }, "tool_use_id": "t1" }),
    ));
    // Typed while it runs: queued with the running turn's id.
    studio.apply(&prompt(T0 + 4_000, "and push it", "p-1"));

    let mut out = Vec::new();
    for s in 0..120 {
        out.extend(listed(&mut studio, "working", T0 + 5_000 + s * 1_000));
    }

    assert!(interrupts(&out).is_empty(), "{out:?}");
    assert_eq!(state(&studio), State::Running);
}
