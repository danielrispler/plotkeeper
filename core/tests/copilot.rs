//! The Copilot adapter (#361; spec #320, Harnesses): Copilot's hook payloads, in both casings,
//! replayed into sessions, turns and tools.
//!
//! `fixtures/copilot/run.jsonl` is built from Copilot's hooks reference
//! (https://docs.github.com/en/copilot/reference/hooks-configuration, read 2026-11-23), not from a
//! real run. Session `cs-7a1f` is camelCase, as Copilot sends it to the camelCase entries install
//! wires: `sessionId`, `toolName`, `toolArgs` as a JSON string (the "use hooks" page's example), and no
//! event name, which the entry passes as `--event` and the hook stores as `studio_event`. Session
//! `vs-3c9e` is snake_case, as Copilot sends it to PascalCase entries, with `hook_event_name` and
//! Claude-style tool names, and no `studio_pid`, as for a VS Code-hosted session whose pid the hook
//! told as the IDE's. The tool names `edit` and `Read`, the edit's arguments, the ids and the
//! transcript paths are made up: the docs give no example of them.

use serde_json::json;
use studio_core::{COPILOT_CAPABILITIES, State, Studio};

const RUN: &str = include_str!("fixtures/copilot/run.jsonl");
const CAMEL: &str = "cs-7a1f";
const SNAKE: &str = "vs-3c9e";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

/// The tool, turn and file or command of each call of session `id`, in order.
fn calls(studio: &Studio, id: &str) -> Vec<(String, Option<usize>, Option<String>)> {
    let session = studio.session(id).unwrap();
    let rows = session.ledger.rows().iter();
    rows.map(|r| (r.tool.clone(), r.turn, r.file.clone().or(r.command.clone())))
        .collect()
}

#[test]
fn the_documented_run_replays_into_its_sessions_turns_and_tools_in_both_casings() {
    let studio = Studio::replayed(fixture());

    let camel = studio
        .session(CAMEL)
        .expect("the camelCase Copilot session");
    assert_eq!(camel.capabilities, COPILOT_CAPABILITIES);
    assert_eq!(camel.capabilities.agent, "copilot");
    assert_eq!(camel.cwd, "/Users/me/work/hello");
    assert_eq!(camel.pid, Some(5151));
    let prompts: Vec<_> = camel.turns.iter().map(|t| t.prompt.as_str()).collect();
    assert_eq!(
        prompts,
        ["Add a greeting to hello.txt", "Now run the tests"]
    );
    assert!(camel.turns.iter().all(|t| t.ended_at.is_some()));
    assert_eq!(camel.state, State::Ended);
    assert_eq!(
        calls(&studio, CAMEL),
        [
            ("bash".into(), Some(0), Some("cat hello.txt".into())),
            (
                "edit".into(),
                Some(0),
                Some("/Users/me/work/hello/hello.txt".into())
            ),
            ("bash".into(), Some(1), Some("npm test".into())),
        ]
    );
    let rows = camel.ledger.rows();
    assert!(rows.iter().all(|r| r.end.is_some()), "every call ended");

    let snake = studio
        .session(SNAKE)
        .expect("the snake_case Copilot session");
    assert_eq!(snake.capabilities, COPILOT_CAPABILITIES);
    assert_eq!(snake.pid, None);
    assert_eq!(snake.turns.len(), 1);
    assert_eq!(snake.turns[0].prompt, "What does this repo do?");
    assert!(snake.turns[0].ended_at.is_some());
    assert_eq!(snake.state, State::YourTurn);
    assert_eq!(
        calls(&studio, SNAKE),
        [(
            "Read".into(),
            Some(0),
            Some("/Users/me/work/other/README.md".into())
        )]
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_running_tool_keeps_its_turn_running_until_its_call_ends() {
    let lines = fixture();
    let state_after = |n: usize| Studio::replayed(&lines[..n]).session(CAMEL).unwrap().state;
    // The first `preToolUse`, then the camelCase session's first `agentStop`.
    assert_eq!(state_after(3), State::Running);
    assert_eq!(state_after(10), State::YourTurn);
}

#[test]
fn a_copilot_session_records_no_turn_trees() {
    let studio = Studio::replayed(fixture());
    let session = studio.session(CAMEL).unwrap();
    assert!(!session.capabilities.turn_trees);
    assert!(session.turns.iter().all(|t| t.files.is_none()));
}

/// A session whose reported pid was the IDE's has none: the process check never ends it, and the
/// quiet period marks it "idle?" (#320, "IDE-hosted pids").
#[test]
fn a_session_with_the_ides_pid_goes_idle_and_is_never_ended_by_the_process_check() {
    let lines = fixture();
    // The snake_case session up to its tool's start: in a turn.
    let at = lines
        .iter()
        .position(|l| l.contains("\"PreToolUse\""))
        .unwrap();
    let mut studio = Studio::replayed(&lines[..=at]);
    assert_eq!(studio.session(SNAKE).unwrap().state, State::Running);
    assert!(!studio.gone(|_| false).contains(&SNAKE.to_owned()));
    studio.tick(1_794_700_005_500 + 2 * 60 * 60 * 1000);
    assert_eq!(studio.session(SNAKE).unwrap().state, State::Idle);
    assert!(!studio.gone(|_| false).contains(&SNAKE.to_owned()));
}

#[test]
fn a_copilot_session_ended_by_the_process_check_stays_copilot() {
    // The camelCase session up to its second turn's tool, before its own stop and end.
    let mut lines: Vec<String> = fixture()[..14].iter().map(|l| l.to_string()).collect();
    let end = json!({ "session_id": CAMEL, "hook_event_name": "SessionEnd", "reason": "process_gone", "studio_received_ms": 1_794_700_012_000_u64 });
    let mut live = Studio::replayed(&lines);
    live.apply(&end.to_string());
    lines.push(end.to_string());
    let restarted = Studio::replayed(&lines);
    for studio in [&live, &restarted] {
        let session = studio.session(CAMEL).unwrap();
        assert_eq!(session.state, State::Ended);
        assert_eq!(session.capabilities, COPILOT_CAPABILITIES);
    }
}
