//! The Qwen Code adapter (#360; spec #320, Harnesses): Qwen Code's hook payloads replayed into
//! sessions, turns and tools.
//!
//! `fixtures/qwen/run.jsonl` is built from Qwen Code's hook docs
//! (https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/, read 2026-11-23), not from a
//! real run: the common fields (`session_id`, `transcript_path`, `cwd`, `hook_event_name`,
//! `timestamp`, `permission_mode`, `prompt_id`), each event's own fields and the runtime tool ids
//! (`run_shell_command`, `write_file`, `edit`) are the docs'; the ids, the `tool_response` values, the
//! model name and the transcript path are made up, since the docs give no example of them. Two
//! sessions run at once.

use serde_json::json;
use studio_core::{Outcome, QWEN_CAPABILITIES, State, Studio};

const RUN: &str = include_str!("fixtures/qwen/run.jsonl");
const FIRST: &str = "5c0e1f7a-2b3d-4e8f-9a10-b2c3d4e5f601";
const SECOND: &str = "9d8c7b6a-5f4e-4d3c-8b2a-1f0e9d8c7b02";

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

    let first = studio.session(FIRST).expect("the first Qwen session");
    assert_eq!(first.capabilities, QWEN_CAPABILITIES);
    assert_eq!(first.capabilities.agent, "qwen");
    assert_eq!(first.model.as_deref(), Some("qwen3-coder-plus"));
    assert_eq!(first.permission_mode.as_deref(), Some("default"));
    assert_eq!(first.pid, Some(5151));
    assert_eq!(
        first.turns[0].message.as_deref(),
        Some("hello.txt now says hello, world.")
    );
    let check = first.ledger.row(None, "toolu_1").unwrap();
    assert_eq!(check.command.as_deref(), Some("cat hello.txt"));
    assert_eq!(check.duration_ms, Some(42));
    assert_eq!(check.outcome, Some(Outcome::Succeeded));
    let write = first.ledger.row(None, "toolu_2").unwrap();
    assert_eq!(
        write.file.as_deref(),
        Some("/Users/me/work/hello/hello.txt")
    );

    let second = studio.session(SECOND).expect("the second Qwen session");
    assert_eq!(second.capabilities, QWEN_CAPABILITIES);
    assert_eq!(second.turns.len(), 1);
    assert_eq!(second.turns[0].prompt, "Fix the typo in README.md");
    assert_eq!(second.state, State::YourTurn);
    assert_eq!(
        calls(&studio, SECOND),
        [("toolu_b1".into(), "edit".into(), Some(0))]
    );
    let edit = second.ledger.row(None, "toolu_b1").unwrap();
    assert_eq!(edit.file.as_deref(), Some("/Users/me/work/other/README.md"));
    assert_eq!(edit.outcome, Some(Outcome::Failed { exit_code: None }));
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn an_event_the_adapter_does_not_map_is_no_drift() {
    let mut lines: Vec<String> = fixture().iter().map(ToString::to_string).collect();
    lines.insert(
        2,
        json!({
            "session_id": FIRST, "cwd": "/Users/me/work/hello", "hook_event_name": "SubagentStart",
            "agent_id": "sub_1", "agent_type": "Explorer", "studio_agent": "qwen",
            "studio_received_ms": 1_794_700_001_500u64,
        })
        .to_string(),
    );
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.session(FIRST).unwrap().state, State::Ended);
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}
