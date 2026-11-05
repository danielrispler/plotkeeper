//! Seam 1 for the card while a check runs (#461, decision 15): at a check's `PreToolUse`, a check
//! whose typical duration is over 2 minutes requests one card on the change so far, which names the
//! check; a shorter or unknown one requests none. The check's result settles the guess on it.

use serde_json::Value;
use std::collections::BTreeMap;
use studio_core::{Base, CardRequest, Change, Output, Purpose, Studio};

const CHECKS: &str = include_str!("fixtures/checks.jsonl");
const SESSION: &str = "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18";
const COMMAND: &str = "cd apps/studio && cargo test";
const DIFF: &str = "--- a/ui/style.css\n+++ b/ui/style.css\n@@ -1 +1 @@\n-a\n+b\n";

/// `lines` with every line from index `from` on received `by` milliseconds later.
fn later(lines: &mut [String], from: usize, by: u64) {
    for line in &mut lines[from..] {
        let mut event: Value = serde_json::from_str(line).unwrap();
        let at = event["studio_received_ms"].as_u64().unwrap();
        event["studio_received_ms"] = (at + by).into();
        *line = event.to_string();
    }
}

/// `checks.jsonl` with its first check's result 200 seconds later, so it took 3 m 25 s. The second
/// wait edits a file, then runs the same check, which now usually takes over 2 minutes and passes in
/// 2 s; the third runs it again, usually 2 s by then.
fn log() -> Vec<String> {
    let mut lines: Vec<String> = CHECKS.lines().map(Into::into).collect();
    later(&mut lines, 3, 200_000);
    lines
}

/// Runs `log` live as the shell does, each worktree changed at every look that is not a base, and
/// returns the card requests that name a check, with the number of the line that made each.
fn check_cards(log: &[String]) -> (Studio, Vec<(usize, CardRequest)>) {
    let mut studio = Studio::default();
    let mut cards = Vec::new();
    for (index, line) in log.iter().enumerate() {
        let changes: BTreeMap<String, Change> = studio
            .fingerprint_folders(line)
            .into_iter()
            .filter(|look| look.purpose != Purpose::Base)
            .map(|look| {
                let change = Change {
                    base: Base::default(),
                    diff: DIFF.into(),
                    head: None,
                };
                (look.key, change)
            })
            .collect();
        for output in studio.apply_with_changes(line, changes) {
            if let Output::Card(request) = output
                && request.check.is_some()
            {
                cards.push((index + 1, request));
            }
        }
    }
    (studio, cards)
}

#[test]
fn a_check_over_2_minutes_requests_one_card_at_its_start_on_the_change_so_far_and_a_shorter_none() {
    let (_, cards) = check_cards(&log());

    let [(line, request)] = &cards[..] else {
        panic!("one check card: {cards:?}");
    };
    // The second wait's check, at its `PreToolUse`. The first had no typical time yet, and the
    // third's is 2 s.
    assert_eq!(*line, 9);
    assert_eq!(request.check.as_deref(), Some(COMMAND));
    assert_eq!(request.diff(), DIFF);
    assert_eq!(request.session_id, SESSION);
}

#[test]
fn the_checks_result_settles_the_guess_on_its_card() {
    let (studio, cards) = check_cards(&log());
    let [(_, request)] = &cards[..] else {
        panic!("one check card: {cards:?}");
    };

    let checks = &studio.session(SESSION).unwrap().checks;
    let carded: Vec<_> = checks.iter().filter(|l| l.card.is_some()).collect();
    let [line] = &carded[..] else {
        panic!("one check line with a card");
    };
    assert_eq!(line.card.as_ref(), Some(&request.id));
    // It passed, and its exit code counts, so the guess on its card is settled.
    assert_eq!(line.passed, Some(true));
    assert!(line.ended_at.is_some());
}

#[test]
fn a_long_check_that_fails_settles_its_guess_as_failed() {
    let mut log = log();
    // The second check's result also lands 200 s late, so the third check usually takes over 2
    // minutes too; the third wait edits the file again before it.
    later(&mut log, 9, 200_000);
    let prompt: Value = serde_json::from_str(&log[11]).unwrap();
    let edit = |line: &String, after: u64| {
        let mut event: Value = serde_json::from_str(line).unwrap();
        event["prompt_id"] = prompt["prompt_id"].clone();
        let at = prompt["studio_received_ms"].as_u64().unwrap();
        event["studio_received_ms"] = (at + after).into();
        event.to_string()
    };
    let (pre, post) = (edit(&log[6], 1000), edit(&log[7], 1100));
    log.splice(12..12, [pre, post]);

    let (studio, cards) = check_cards(&log);

    let lines: Vec<usize> = cards.iter().map(|(line, _)| *line).collect();
    assert_eq!(lines, [9, 15]);
    let checks = &studio.session(SESSION).unwrap().checks;
    let settled: Vec<Option<bool>> = checks
        .iter()
        .filter(|l| l.card.is_some())
        .map(|l| l.passed)
        .collect();
    assert_eq!(settled, [Some(true), Some(false)]);
}
