//! Seam 1 (#434, Testing Decisions) for what survives a restart (#434, decisions 37 to 39): lines
//! taken live, then the same lines replayed into a new core with the card store's live-only facts and
//! cards applied, as the shell does at start, then the rest live.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use studio_core::{Base, CardRequest, Change, Kind, LiveFacts, Output, Purpose, Studio};

/// The scripted run of `record-subagent-run.sh` (#370): its worktree worker commits (line 15) and
/// ends (line 18); its helper runs a command (lines 16 and 17) and is reported killed (line 25).
const RUN: &str = include_str!("fixtures/subagent-run.jsonl");
const RUN_SESSION: &str = "40e8e6ef-9b2f-4467-9b55-87c2d053b616";
const RUN_WORKER: &str = "add0c7bbaced40ed0";
const RUN_HELPER: &str = "a551b1ece0a4430ce";

fn change() -> Change {
    Change {
        base: Base::default(),
        diff: "--- a/hello.txt\n+++ b/hello.txt\n@@ -0,0 +1 @@\n+hello\n".into(),
        head: Some("c0ffee".into()),
    }
}

/// Takes `line` live, as the shell does when the fingerprints of `changed` keys find a change.
fn live(studio: &mut Studio, line: &str, changed: &[&str]) -> Vec<Output> {
    let looks = studio.fingerprint_folders(line).into_iter();
    let changes = looks.filter(|look| changed.contains(&look.key.as_str()));
    let changes: BTreeMap<String, Change> = changes.map(|look| (look.key, change())).collect();
    studio.apply_with_changes(line, changes)
}

/// The app restarted after `lines`: a new core with them replayed, then `facts` and `cards` applied.
fn restarted(lines: &[impl AsRef<str>], facts: &LiveFacts, cards: &[CardRequest]) -> Studio {
    let mut studio = Studio::replayed(lines);
    studio.restore(facts, cards);
    studio
}

#[test]
fn a_helper_promoted_live_is_still_a_worker_after_a_restart_and_its_kill_is_a_worker_failed() {
    let lines: Vec<&str> = RUN.lines().collect();
    let mut facts = LiveFacts::default();
    let mut studio = Studio::default();
    // The helper's command changes the session's worktree (line 17).
    for line in &lines[..17] {
        for output in live(&mut studio, line, &[RUN_HELPER]) {
            if let Output::Promoted { agent } = output {
                facts.promoted.insert(agent);
            }
        }
    }
    assert_eq!(facts.promoted, [RUN_HELPER.to_string()].into());

    let mut studio = restarted(&lines[..17], &facts, &[]);
    let kind = |studio: &Studio| {
        studio
            .session(RUN_SESSION)
            .unwrap()
            .subagent(RUN_HELPER)
            .unwrap()
            .kind
    };
    assert_eq!(kind(&studio), Kind::Worker);
    let failed: Vec<Output> = lines[17..]
        .iter()
        .flat_map(|line| live(&mut studio, line, &[]))
        .filter(|output| matches!(output, Output::Failed { .. }))
        .collect();

    assert_eq!(
        failed,
        [Output::Failed {
            session_id: RUN_SESSION.into(),
            agent: RUN_HELPER.into()
        }]
    );
}

#[test]
fn a_failed_line_daniel_opened_stays_faded_after_a_restart() {
    let mut facts = LiveFacts::default();
    facts.opened.insert(RUN_HELPER.into());

    let studio = restarted(&RUN.lines().collect::<Vec<_>>(), &facts, &[]);

    let helper = studio
        .session(RUN_SESSION)
        .unwrap()
        .subagent(RUN_HELPER)
        .unwrap();
    assert!(helper.opened && helper.faded());
}

/// The card requests among `outputs`.
fn requests(outputs: Vec<Output>) -> Vec<CardRequest> {
    let requests = outputs.into_iter().filter_map(|output| match output {
        Output::Card(request) | Output::Brief(request) => Some(request),
        _ => None,
    });
    requests.collect()
}

#[test]
fn after_a_restart_mid_run_the_parents_stop_still_writes_its_card_from_both_worker_cards() {
    let lines: Vec<&str> = RUN.lines().collect();
    let mut studio = Studio::default();
    // Up to the worker's end (line 18); its worktree changed at its commit and at its end.
    let stored: Vec<CardRequest> = lines[..18]
        .iter()
        .flat_map(|line| requests(live(&mut studio, line, &[RUN_WORKER])))
        .collect();
    assert_eq!(stored.len(), 2);

    let mut studio = restarted(&lines[..18], &LiveFacts::default(), &stored);
    let after: Vec<CardRequest> = lines[18..]
        .iter()
        .flat_map(|line| requests(live(&mut studio, line, &[])))
        .collect();

    let [stop] = &after[..] else {
        panic!("one card at the stop: {after:?}");
    };
    let both = [
        format!("{RUN_WORKER}@1793471824831"),
        format!("{RUN_WORKER}@1793471826289"),
    ];
    assert_eq!(
        (stop.agent.as_deref(), &stop.workers[..]),
        (None, &both[..])
    );
}

/// Session `s1`'s event `name` at `at` in `/w`, with `fields` added.
fn event(name: &str, at: u64, fields: Value) -> String {
    let mut event =
        json!({"session_id": "s1", "hook_event_name": name, "cwd": "/w", "studio_received_ms": at});
    let fields = fields.as_object().unwrap().clone();
    event.as_object_mut().unwrap().extend(fields);
    event.to_string()
}

const MINUTE: u64 = 60 * 1000;

/// The spawn of worker `w1` in a worktree of its own.
fn spawn() -> Value {
    json!({"tool_name": "Agent", "tool_response": {"agentId": "w1"},
        "tool_input": {"description": "Implement #378", "isolation": "worktree"}})
}

/// Worker `w1`'s event `name` at `at`, in its worktree `/t`, with `fields` added.
fn child(name: &str, at: u64, fields: Value) -> String {
    let mut fields = fields.as_object().unwrap().clone();
    fields.insert("agent_id".into(), json!("w1"));
    event(name, at, fields.into()).replace("\"/w\"", "\"/t\"")
}

#[test]
fn after_a_restart_a_workers_fallback_waits_20_minutes_from_its_newest_stored_card() {
    let lines = [
        event("UserPromptSubmit", 0, json!({"prompt": "Implement #378"})),
        event("PostToolUse", 1, spawn()),
        child("SubagentStart", 2, json!({})),
        // Its commit, at 10 minutes, gets a card.
        child("PostToolUse", 10 * MINUTE, json!({"tool_name": "Bash"})),
    ];
    let mut studio = Studio::default();
    let stored: Vec<CardRequest> = lines
        .iter()
        .flat_map(|line| requests(live(&mut studio, line, &["w1"])))
        .collect();
    assert_eq!(stored.len(), 1, "the commit's card");

    let studio = restarted(&lines, &LiveFacts::default(), &stored);
    let looks = |at| {
        let step = child("PostToolUse", at, json!({"tool_name": "Read"}));
        let looks = studio.fingerprint_folders(&step).into_iter();
        looks.map(|look| look.purpose).collect::<Vec<_>>()
    };

    assert_eq!(looks(29 * MINUTE), []);
    assert_eq!(looks(30 * MINUTE), [Purpose::Fallback]);
}

#[test]
fn worker_cards_a_woken_turns_brief_took_are_not_collected_again_after_a_restart() {
    let woken =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    let lines = [
        event("UserPromptSubmit", 100, json!({"prompt": "Implement #378"})),
        event("PostToolUse", 110, spawn()),
        event("Stop", 120, json!({})),
        child("SubagentStart", 130, json!({})),
        child("SubagentStop", 140, json!({})),
        event("UserPromptSubmit", 150, json!({ "prompt": woken })),
        event("Stop", 160, json!({})),
    ];
    let mut studio = Studio::default();
    let requested: Vec<CardRequest> = lines
        .iter()
        .flat_map(|line| requests(live(&mut studio, line, &["w1"])))
        .collect();
    // The worker's card, and the brief that took it.
    assert_eq!(requested.len(), 2);
    let stored = &requested[..1];

    let mut studio = restarted(&lines, &LiveFacts::default(), stored);
    let next = [
        event("UserPromptSubmit", 200, json!({"prompt": "Anything left?"})),
        event("Stop", 210, json!({})),
    ];
    let after: Vec<CardRequest> = next
        .iter()
        .flat_map(|line| requests(live(&mut studio, line, &[])))
        .collect();

    assert_eq!(after, []);
}
