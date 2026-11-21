//! The wire's agreement check (#246) for the core's types: sessions replayed from the fixtures, their checks, the
//! selection and a subagent's log, written into `scripts/wire/core.ts` (`scripts/wire/samples.rs`). It sees only the
//! shapes these samples produce: an optional field no sample fills is not checked.

#[path = "../../scripts/wire/samples.rs"]
mod samples;

use samples::{Sample, json, write};
use std::path::PathBuf;
use studio_core::{AskFact, AskFacts, AskSection, CheckOutput, Studio};

/// The fixtures whose sessions are the samples: every hand-written and recorded log but the two day-long ones, which
/// would make the file megabytes long.
const LOGS: [&str; 19] = [
    include_str!("fixtures/ask-then-plan.jsonl"),
    include_str!("fixtures/background-agent.jsonl"),
    include_str!("fixtures/bash-steps.jsonl"),
    include_str!("fixtures/blocked-run.jsonl"),
    include_str!("fixtures/checks.jsonl"),
    include_str!("fixtures/child-question.jsonl"),
    include_str!("fixtures/closed-at-plan.jsonl"),
    include_str!("fixtures/elicitation.jsonl"),
    include_str!("fixtures/failing-bash.jsonl"),
    include_str!("fixtures/interrupt-by-esc.jsonl"),
    include_str!("fixtures/interrupt-then-prompt.jsonl"),
    include_str!("fixtures/permission-denied.jsonl"),
    include_str!("fixtures/permission-write.jsonl"),
    include_str!("fixtures/queued-prompt.jsonl"),
    include_str!("fixtures/sandbox-network.jsonl"),
    include_str!("fixtures/subagent-run.jsonl"),
    include_str!("fixtures/subagent-tree.jsonl"),
    include_str!("fixtures/times.jsonl"),
    include_str!("fixtures/two-sessions.jsonl"),
];

/// The recorded agy run, whose subagent has a log the core reads from its brain folder.
fn agy() -> Studio {
    let brain = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    let run =
        include_str!("fixtures/agy/run.jsonl").replace("@BRAIN@", &brain.display().to_string());
    Studio::replayed(run.lines())
}

#[test]
fn the_wire_samples_of_the_core_are_written_for_tsc() {
    let mut studios: Vec<Studio> = LOGS
        .iter()
        .map(|log| Studio::replayed(log.lines()))
        .collect();
    studios.push(agy());
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
    assert!(
        samples.iter().any(|(t, _)| *t == "Check"),
        "a sample of each type"
    );
    assert!(
        samples.iter().any(|(t, _)| *t == "SubagentLog"),
        "a sample of each type"
    );
    write(env!("CARGO_MANIFEST_DIR"), "core", &samples);
}
