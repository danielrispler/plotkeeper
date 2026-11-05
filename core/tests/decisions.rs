//! Seam 1 (#461, Testing Decisions) for the decision history (#479, decision 17): an answered
//! question, plan or permission leaves the open list and enters its session's history, with what it
//! asked, the answer, when, and whether the studio card or the terminal answered it.

use serde_json::{Value, json};
use studio_core::{Answer, Decision, Source, Studio};

const ASKER: &str = "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95";
const PROBE: &str = "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31";

/// The answer hook's `StudioAnswer` line for `line`'s call: Daniel's `answer` from the studio card
/// (#478), 1 ms after the line.
fn card_answer(line: &str, answer: Value) -> String {
    let mut event: Value = serde_json::from_str(line).unwrap();
    event["studio_asked_by"] = event["hook_event_name"].clone();
    event["hook_event_name"] = "StudioAnswer".into();
    event["studio_answer"] = answer;
    event["studio_received_ms"] = (event["studio_received_ms"].as_u64().unwrap() + 1).into();
    event.to_string()
}

fn replayed(lines: &[&str]) -> Studio {
    Studio::replayed(lines)
}

/// The history of `session`, as (tool, answer, time, source).
fn history(studio: &Studio, session: &str) -> Vec<(String, Value, u64, Source)> {
    let decisions: &[Decision] = &studio.session(session).unwrap().decisions;
    decisions
        .iter()
        .map(|d| (d.tool.clone(), d.answer.clone(), d.at, d.source))
        .collect()
}

#[test]
fn a_question_and_a_plan_answered_in_the_terminal_leave_the_open_list_for_the_history() {
    let lines: Vec<&str> = include_str!("fixtures/ask-then-plan.jsonl")
        .lines()
        .collect();

    // Asked, and not answered yet: open, and no history.
    let studio = replayed(&lines[..4]);
    let session = studio.session(ASKER).unwrap();
    assert_eq!(session.questions[0].answer, Answer::Open);
    assert!(session.decisions.is_empty());

    let studio = replayed(&lines);
    let session = studio.session(ASKER).unwrap();
    assert!(session.questions.iter().all(|q| q.answer != Answer::Open));
    assert_eq!(
        history(&studio, ASKER),
        [
            (
                "AskUserQuestion".into(),
                json!({"Which README section should I fix?": "Install"}),
                1_793_262_013_905,
                Source::Terminal
            ),
            (
                "ExitPlanMode".into(),
                json!("allow"),
                1_793_262_074_933,
                Source::Terminal
            ),
        ]
    );
    // It keeps what was asked: the question and its options, the plan.
    let decisions = &session.decisions;
    assert_eq!(
        decisions[0].input["questions"][0]["options"][1]["label"],
        "Usage"
    );
    assert!(
        decisions[1].input["plan"]
            .as_str()
            .unwrap()
            .starts_with("# Fix the README")
    );
}

#[test]
fn a_question_and_a_plan_answered_on_the_card_are_the_cards() {
    let lines: Vec<&str> = include_str!("fixtures/ask-then-plan.jsonl")
        .lines()
        .collect();
    let answer = card_answer(
        lines[1],
        json!({"answers": {"Which README section should I fix?": "Install"}}),
    );
    let rejected = card_answer(lines[11], json!({"decision": "deny"}));
    let mut log = vec![lines[0], lines[1], &answer, lines[4]];
    log.extend(&lines[5..12]);
    log.push(&rejected);
    let studio = replayed(&log);

    assert_eq!(
        history(&studio, ASKER),
        [
            (
                "AskUserQuestion".into(),
                json!({"Which README section should I fix?": "Install"}),
                1_793_262_013_905,
                Source::Card
            ),
            // A rejected plan may get no result, so the card's answer is the record.
            (
                "ExitPlanMode".into(),
                json!("deny"),
                1_793_262_052_411,
                Source::Card
            ),
        ]
    );
}

#[test]
fn a_permission_is_the_cards_when_decided_there_and_the_terminals_otherwise() {
    let lines: Vec<&str> = include_str!("fixtures/permission-write.jsonl")
        .lines()
        .collect();
    let allowed = card_answer(lines[2], json!({"decision": "allow"}));
    let studio = replayed(&[lines[0], lines[1], lines[2], lines[3], &allowed, lines[4]]);
    assert_eq!(
        history(&studio, PROBE),
        [(
            "Write".into(),
            json!("allow"),
            1_793_260_023_456,
            Source::Card
        )]
    );
    assert_eq!(
        studio.session(PROBE).unwrap().decisions[0].input["file_path"],
        "/Users/danielrispler/work/Falinks/probe.txt"
    );

    let studio = replayed(
        &include_str!("fixtures/permission-write.jsonl")
            .lines()
            .collect::<Vec<_>>(),
    );
    assert_eq!(
        history(&studio, PROBE),
        [(
            "Write".into(),
            json!("allow"),
            1_793_260_034_880,
            Source::Terminal
        )]
    );

    let denied: Vec<&str> = include_str!("fixtures/permission-denied.jsonl")
        .lines()
        .collect();
    let studio = replayed(&denied);
    assert_eq!(
        history(&studio, PROBE),
        [(
            "Bash".into(),
            json!("deny"),
            1_793_260_051_877,
            Source::Terminal
        )]
    );
}

#[test]
fn a_call_with_no_permission_asked_leaves_no_history() {
    let lines: Vec<&str> = include_str!("fixtures/bash-steps.jsonl").lines().collect();
    let studio = replayed(&lines);
    assert!(studio.sessions().iter().all(|s| s.decisions.is_empty()));
}
