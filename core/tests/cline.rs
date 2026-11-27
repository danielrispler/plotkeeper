//! The Cline adapter (#371; spec #320, Harnesses): the lines plotkeeper's Cline plugin
//! (`plugin-cline/plotkeeper.js`) appends, replayed into sessions, turns and tools.
//!
//! `fixtures/cline/run.jsonl` is a real run, recorded on 2026-11-27 with Cline CLI 3.0.76 and
//! `claude-haiku-5-5`: the plugin, installed into a scratch project's `.cline/plugins/` with
//! `cline plugin install --cwd`, ran this checkout's `plotkeeper-hook` with a scratch home. One
//! headless `cline --json` run read a file, edited it and ran two commands in one call, the second
//! failing, then called Cline's `submit_and_exit`. The scratch folder's path is replaced by
//! `/Users/me/work`.

use serde_json::{Value, json};
use studio_core::{CLINE_CAPABILITIES, Outcome, State, Studio};

const RUN: &str = include_str!("fixtures/cline/run.jsonl");
const HELLO: &str = "1795785548678_7h0oj";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

#[test]
fn the_recorded_run_replays_into_its_session_turn_and_tools() {
    let studio = Studio::replayed(fixture());

    let hello = studio.session(HELLO).expect("the hello session");
    assert_eq!(hello.capabilities, CLINE_CAPABILITIES);
    assert_eq!(hello.capabilities.agent, "cline");
    assert_eq!(hello.pid, Some(78277));
    assert_eq!(hello.model, None);
    let prompts: Vec<_> = hello.turns.iter().map(|t| t.prompt.as_str()).collect();
    // The prompt without the `<user_input>` tag Cline wraps it in.
    assert_eq!(
        prompts,
        [
            "Read hello.txt, change its contents to 'hello, world' with your file editing tool, run the shell command 'cat hello.txt', then run 'ls /nonexistent-dir-xyz' (it will fail). Reply with one short sentence."
        ]
    );
    let message = hello.turns[0].message.as_deref().unwrap();
    assert!(
        message.starts_with("Submission recorded (verified): Read hello.txt"),
        "{message}"
    );
    assert!(hello.turns[0].ended_at.is_some());
    assert_eq!(hello.state, State::YourTurn);
    let calls: Vec<_> = (hello.ledger.rows().iter())
        .map(|r| (r.tool.as_str(), r.turn))
        .collect();
    assert_eq!(
        calls,
        [
            ("read_files", Some(0)),
            ("editor", Some(0)),
            ("run_commands", Some(0)),
            ("submit_and_exit", Some(0)),
        ]
    );
    let edit = hello
        .ledger
        .row(None, "toolu_01PsEjJH4cF9XRBzdz3dMa2P")
        .unwrap();
    assert_eq!(edit.file.as_deref(), Some("/Users/me/work/hello/hello.txt"));
    assert_eq!(edit.outcome, Some(Outcome::Succeeded));
    // One call ran both commands; the second failed, which Cline's result tells only in that command's entry.
    let commands = hello
        .ledger
        .row(None, "toolu_01Dpr4ANYJh7Ud9SZxKUVqaa")
        .unwrap();
    assert_eq!(
        commands.command.as_deref(),
        Some("cd /Users/me/work/hello && cat hello.txt\nls /nonexistent-dir-xyz")
    );
    assert_eq!(
        commands.outcome,
        Some(Outcome::Failed { exit_code: Some(1) })
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_cline_session_records_no_turn_trees() {
    let studio = Studio::replayed(fixture());
    let session = studio.session(HELLO).unwrap();
    assert!(!session.capabilities.turn_trees);
    assert!(session.turns.iter().all(|t| t.files.is_none()));
}

/// Line `n` of the fixture with `fields` set.
fn changed(n: usize, fields: Value) -> String {
    let mut line: Value = serde_json::from_str(fixture()[n]).unwrap();
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
}

/// A subagent's runs have hooks of their own; neither its start nor its end is the session's turn's
/// (unverified: no real run of Cline's subagents was recorded).
#[test]
fn a_subagents_run_neither_starts_nor_ends_the_sessions_turn() {
    let sub = json!({ "agent_id": "agent_sub", "parent_agent_id": "agent_26ef" });
    let mut lines: Vec<String> = fixture()[..3].iter().map(ToString::to_string).collect();
    // Its run's start and end, in the session's turn.
    lines.push(changed(0, sub.clone()));
    lines.push(changed(9, sub));

    let studio = Studio::replayed(&lines);
    let session = studio.session(HELLO).unwrap();
    assert_eq!(session.turns.len(), 1);
    assert_eq!(session.state, State::Running);
}

#[test]
fn a_line_the_adapter_does_not_map_is_no_drift() {
    let mut lines: Vec<String> = fixture().iter().map(ToString::to_string).collect();
    lines.insert(2, changed(1, json!({ "hook_event_name": "beforeModel" })));
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.session(HELLO).unwrap().state, State::YourTurn);
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}
