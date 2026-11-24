//! The Codex adapter (#359; spec #320, Harnesses): Codex's hook payloads replayed into sessions,
//! turns and tools.
//!
//! `fixtures/codex/run.jsonl` is built from Codex's hook docs (https://learn.chatgpt.com/docs/hooks,
//! read 2026-11-23), not from a real run: the common fields, each event's own fields and the tool
//! names (`Bash`, `apply_patch`, `mcp__…`) are the docs'; the ids, the `tool_response` values and the
//! rollout path are made up, since the docs give no example of them. Two sessions run at once.

use crate::common;
use serde_json::{Value, json};
use studio_core::{CODEX_CAPABILITIES, State, Studio};

const RUN: &str = include_str!("fixtures/codex/run.jsonl");
const TRIAL: &str = include_str!("fixtures/codex/trial.jsonl");
const FIRST: &str = "thr_7f3c2a";
const SECOND: &str = "thr_9b41d0";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

/// The tool and turn of each call of session `id`, in order.
fn calls(studio: &Studio, id: &str) -> Vec<(String, String, Option<usize>)> {
    let session = studio.session(id).unwrap();
    let rows = session.ledger.rows().iter();
    rows.map(|r| (r.call.clone(), r.tool.clone(), r.turn))
        .collect()
}

#[test]
fn the_documented_run_replays_into_its_sessions_turns_and_tools() {
    let studio = Studio::replayed(fixture());

    let first = studio.session(FIRST).expect("the first Codex session");
    assert_eq!(first.capabilities, CODEX_CAPABILITIES);
    assert_eq!(first.capabilities.agent, "codex");
    assert_eq!(first.model.as_deref(), Some("gpt-5.5-codex"));
    assert_eq!(first.permission_mode.as_deref(), Some("default"));
    assert_eq!(first.pid, Some(4242));
    let prompts: Vec<_> = first.turns.iter().map(|t| t.prompt.as_str()).collect();
    assert_eq!(
        prompts,
        ["Add a greeting to hello.txt", "Now run the tests"]
    );
    assert_eq!(
        first.turns[0].message.as_deref(),
        Some("hello.txt now says hello, world.")
    );
    assert!(first.turns.iter().all(|t| t.ended_at.is_some()));
    assert_eq!(first.state, State::Ended);
    assert_eq!(
        calls(&studio, FIRST),
        [
            ("call_1".into(), "Bash".into(), Some(0)),
            ("call_2".into(), "apply_patch".into(), Some(0)),
            ("call_3".into(), "Bash".into(), Some(1)),
        ]
    );
    let check = first.ledger.row(None, "call_1").unwrap();
    assert_eq!(check.command.as_deref(), Some("cat hello.txt"));

    let second = studio.session(SECOND).expect("the second Codex session");
    assert_eq!(second.capabilities, CODEX_CAPABILITIES);
    assert_eq!(second.turns.len(), 1);
    assert_eq!(second.turns[0].prompt, "What does this repo do?");
    assert_eq!(second.state, State::YourTurn);
    assert_eq!(
        calls(&studio, SECOND),
        [(
            "call_b1".into(),
            "mcp__filesystem__read_file".into(),
            Some(0)
        )]
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_permission_request_needs_you_until_its_call_ends() {
    let lines = fixture();
    let state_after = |n: usize| {
        let studio = Studio::replayed(&lines[..n]);
        studio.session(FIRST).unwrap().state
    };
    // The `PermissionRequest` of `apply_patch`, then its result.
    assert_eq!(state_after(7), State::NeedsYou);
    assert_eq!(state_after(9), State::Running);
}

#[test]
fn an_interrupt_ends_its_turn() {
    let lines = fixture();
    let at = lines
        .iter()
        .position(|l| l.contains("\"Interrupt\""))
        .unwrap();

    let before = Studio::replayed(&lines[..at]);
    assert_eq!(before.session(FIRST).unwrap().state, State::Running);
    let after = Studio::replayed(&lines[..=at]);
    let session = after.session(FIRST).unwrap();
    assert_ne!(session.state, State::Running);
    assert!(session.turns[1].ended_at.is_some());
}

/// Wave 1's trial (#369): a Codex TUI turn that ran `calc.py`, patched it and ran it again, as Codex
/// sent its lines (`fixtures/codex/trial.jsonl`, the trial's folder written `/w/trial`). Its
/// summary said nothing was recorded, as Codex recorded no turn trees (#426).
#[test]
fn a_codex_turn_that_patches_a_file_lists_it_and_its_summarys_facts_carry_it() {
    let (listed, carried) = common::trial_files(TRIAL, "UserPromptSubmit", "Stop");
    assert_eq!(listed, ["calc.py"]);
    assert_eq!(carried, ["calc.py"]);
}

/// A line the server writes itself for session `id`, shaped as Claude Code's lines are.
fn server_line(id: &str, name: &str, at: u64, fields: Value) -> String {
    let mut line = json!({ "session_id": id, "hook_event_name": name, "studio_received_ms": at });
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
}

#[test]
fn a_codex_session_ended_by_the_process_check_or_interrupted_stays_codex() {
    // The run up to its second turn's tool, before Codex's own interrupt and end.
    let lines: Vec<String> = fixture()[..15].iter().map(|l| l.to_string()).collect();
    let synthetic = [
        server_line(
            FIRST,
            "StudioInterrupted",
            1_794_700_011_000,
            json!({ "prompt_id": "turn_2" }),
        ),
        server_line(
            FIRST,
            "SessionEnd",
            1_794_700_012_000,
            json!({ "reason": "process_gone" }),
        ),
    ];

    let mut live = Studio::replayed(&lines);
    for line in &synthetic {
        live.apply(line);
    }
    let mut all = lines.clone();
    all.extend(synthetic);
    let restarted = Studio::replayed(&all);
    for studio in [&live, &restarted] {
        let session = studio.session(FIRST).unwrap();
        assert_eq!(session.state, State::Ended);
        assert_eq!(session.capabilities, CODEX_CAPABILITIES);
    }
}
