//! The Codex adapter (#359; spec #320, Harnesses): Codex's hook payloads replayed into sessions,
//! turns and tools.
//!
//! `fixtures/codex/run.jsonl` is built from Codex's hook docs (https://learn.chatgpt.com/docs/hooks,
//! read 2026-11-23), not from a real run: the common fields, each event's own fields and the tool
//! names (`Bash`, `apply_patch`, `mcp__…`) are the docs'; the ids, the `tool_response` values and the
//! rollout path are made up, since the docs give no example of them. Two sessions run at once.

use crate::common;
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
    assert_eq!(
        first.turns[0].message.as_deref(),
        Some("hello.txt now says hello, world.")
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

/// Wave 1's trial (#369): a Codex TUI turn that ran `calc.py`, patched it and ran it again, as Codex
/// sent its lines (`fixtures/codex/trial.jsonl`, the trial's folder written `/w/trial`). Its
/// summary said nothing was recorded, as Codex recorded no turn trees (#426).
#[test]
fn a_codex_turn_that_patches_a_file_lists_it_and_its_summarys_facts_carry_it() {
    let (listed, carried) =
        common::trial_files(TRIAL, ("UserPromptSubmit", "Stop"), ("/w/trial", "calc.py"));
    assert_eq!(listed, ["calc.py"]);
    assert_eq!(carried, Some(vec!["calc.py".to_owned()]));
}
