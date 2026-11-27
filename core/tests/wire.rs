//! The wire's agreement check (#246) for the core's types: sessions replayed from the fixtures, their checks, the
//! selection and a Claude subagent's log, written into `scripts/wire/core.ts` (`scripts/wire/samples.rs`). It sees only the
//! shapes these samples produce: an optional field no sample fills is not checked.

use crate::samples::{Sample, json, write};
use std::path::PathBuf;
use studio_core::{AskFact, AskFacts, AskSection, CheckOutput, Studio};

/// The fixtures whose sessions are the samples: every log in `tests/fixtures/` but the two day-long ones, which would
/// make the file megabytes long.
fn logs() -> Vec<String> {
    const DAY_LONG: [&str; 2] = ["checkpoint-day.jsonl", "ordinary-tickets.jsonl"];
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut paths: Vec<PathBuf> = (std::fs::read_dir(dir).unwrap())
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter(|p| !DAY_LONG.iter().any(|day| p.ends_with(day)))
        .collect();
    paths.sort();
    let logs: Vec<String> = paths
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    assert!(logs.len() > 10, "the fixtures");
    logs
}

/// The recorded agy run, read from its brain folder.
fn agy() -> Studio {
    let brain = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    let run =
        include_str!("fixtures/agy/run.jsonl").replace("@BRAIN@", &brain.display().to_string());
    Studio::replayed(run.lines())
}

/// A Claude session in `dir` with a subagent whose log, beside the session's transcript, holds a
/// prompt, a call, its result and a reply.
fn claude_subagent(dir: &std::path::Path) -> Studio {
    const AGENT: &str = "a7e2c4f6b8d0a1c3e";
    let logs = dir.join("s1/subagents");
    std::fs::create_dir_all(&logs).unwrap();
    let line = |kind: &str, content: serde_json::Value| {
        let message = serde_json::json!({ "role": kind, "content": content });
        serde_json::json!({ "type": kind, "agentId": AGENT, "sessionId": "s1", "isSidechain": true, "message": message })
            .to_string()
    };
    let log = [
        line("user", serde_json::json!("Find the probe.")),
        line(
            "assistant",
            serde_json::json!([{ "type": "tool_use", "id": "c1", "name": "Read", "input": { "file_path": "/w/probe.rs" } }]),
        ),
        line(
            "user",
            serde_json::json!([{ "type": "tool_result", "tool_use_id": "c1", "content": "fn probe() {}" }]),
        ),
        line(
            "assistant",
            serde_json::json!([{ "type": "text", "text": "Found it." }]),
        ),
    ];
    std::fs::write(
        logs.join(format!("agent-{AGENT}.jsonl")),
        log.join("\n") + "\n",
    )
    .unwrap();
    let transcript = dir.join("s1.jsonl");
    let own = serde_json::json!({
        "session_id": "s1", "hook_event_name": "UserPromptSubmit", "cwd": "/w", "prompt": "Go.",
        "transcript_path": transcript, "studio_received_ms": 0,
    });
    let step = serde_json::json!({
        "session_id": "s1", "hook_event_name": "PreToolUse", "cwd": "/w", "agent_id": AGENT,
        "tool_name": "Read", "tool_use_id": "t0", "tool_input": { "file_path": "/w/probe.rs" },
        "transcript_path": transcript, "studio_received_ms": 1,
    });
    Studio::replayed([own.to_string(), step.to_string()])
}

// @law: wire-types-agree
#[test]
fn the_wire_samples_of_the_core_are_written_for_tsc() {
    let mut studios: Vec<Studio> = logs()
        .iter()
        .map(|log| Studio::replayed(log.lines()))
        .collect();
    studios.push(agy());
    let dir = tempfile::tempdir().unwrap();
    studios.push(claude_subagent(dir.path()));
    let mut samples: Vec<Sample> = Vec::new();
    for studio in &mut studios {
        let ids: Vec<String> = studio
            .sessions()
            .iter()
            .map(|s| s.agent.id.clone())
            .collect();
        for id in &ids {
            let session = studio.session(id).unwrap();
            samples.push(("Session", json(session)));
            for check in session.turns.iter().flat_map(|t| &t.checks) {
                samples.push(("Check", json(check)));
            }
            // The selection with no subagent opened, then with each opened in turn.
            studio.select(id);
            samples.push(("Selection", json(studio.selection())));
            let agents: Vec<String> = studio
                .session(id)
                .unwrap()
                .agents
                .iter()
                .map(|a| a.agent.id.clone())
                .collect();
            for agent in &agents {
                studio.open(agent);
                samples.push(("Selection", json(studio.selection())));
                if let Some(log) = studio.subagent_log(id, agent, 0) {
                    samples.push(("SubagentLog", json(log)));
                }
            }
        }
    }
    // A check's output and Ask's facts, as `check_output` and `ask_facts` send them.
    for exit_code in [Some(1), None] {
        let tail = "error: 1 test failed".into();
        samples.push(("CheckOutput", json(CheckOutput { exit_code, tail })));
    }
    let facts = AskFacts {
        sections: vec![AskSection {
            label: "Edits".into(),
            facts: vec![
                AskFact {
                    at: Some(1_761_743_990_000),
                    text: "Edited src/lib.rs".into(),
                },
                AskFact {
                    at: None,
                    text: "3 lines added".into(),
                },
            ],
        }],
    };
    samples.push(("Facts", json(facts)));
    let types = [
        "Session",
        "Selection",
        "Check",
        "SubagentLog",
        "CheckOutput",
        "Facts",
    ];
    write(env!("CARGO_MANIFEST_DIR"), "core", &types, &samples);
}
