//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for the backfill (danielrispler/Falinks#486): the hook records each worktree's git
//! tree at a turn's start and end, so a turn that ended while the app was down gets its card request
//! after the replay. The shell turns the two trees into the card's diff.

use serde_json::json;
use studio_core::{Backfill, CardRequest, Studio, Trees};

const SESSION: &str = "4c2e8a1f-7b3d-4e9a-a6c5-0f1d2b3e4a57";
const ROOT: &str = "/w/repo";
const OTHER: &str = "/w/other";
const DAY: u64 = 24 * 60 * 60 * 1000;
/// When the app starts after the log.
const NOW: u64 = 1_800_000_000_000;

/// One event of the session's own agent in turn `prompt`, received at `at`, with `extra` fields.
fn line(name: &str, prompt: &str, at: u64, extra: serde_json::Value) -> String {
    let mut event = json!({
        "session_id": SESSION,
        "hook_event_name": name,
        "cwd": ROOT,
        "prompt_id": prompt,
        "studio_received_ms": at,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
}

/// A turn whose prompt records `start` and an edit of another worktree records `other`, and whose
/// `Stop` at `at` records `end` for both.
fn turn(prompt: &str, at: u64, start: &str, end: &str) -> Vec<String> {
    vec![
        line(
            "UserPromptSubmit",
            prompt,
            at - 3000,
            json!({"prompt": "go", "studio_turn_start": {ROOT: start}}),
        ),
        line(
            "PreToolUse",
            prompt,
            at - 2000,
            json!({"tool_name": "Edit", "tool_input": {"file_path": "/w/other/a.rs"}, "studio_turn_start": {OTHER: "o1"}}),
        ),
        line(
            "Stop",
            prompt,
            at,
            json!({"studio_turn_end": {ROOT: end, OTHER: "o2"}}),
        ),
    ]
}

/// The backfill after replaying `lines`, with `cards` in the store.
fn backfill(lines: &[String], cards: &[CardRequest]) -> Vec<Backfill> {
    let mut studio = Studio::replayed(lines);
    studio.backfill(NOW, cards)
}

#[test]
fn a_turn_that_ended_with_no_card_and_two_trees_gets_a_card_request_at_start() {
    let at = NOW - 1000;
    let found = backfill(&turn("p1", at, "t1", "t2"), &[]);

    assert_eq!(
        found,
        [Backfill {
            request: CardRequest {
                session_id: SESSION.into(),
                id: format!("{SESSION}@{at}"),
                prompt: Some(at - 3000),
                ..CardRequest::default()
            },
            trees: vec![
                Trees {
                    root: OTHER.into(),
                    start: "o1".into(),
                    end: "o2".into()
                },
                Trees {
                    root: ROOT.into(),
                    start: "t1".into(),
                    end: "t2".into()
                },
            ],
        }]
    );
}

#[test]
fn a_turn_with_a_live_card_gets_none() {
    let at = NOW - 1000;
    let live = CardRequest {
        session_id: SESSION.into(),
        id: format!("{SESSION}@{at}"),
        ..CardRequest::default()
    };

    assert_eq!(backfill(&turn("p1", at, "t1", "t2"), &[live]), []);
}

#[test]
fn a_turn_older_than_24_hours_gets_none() {
    assert_eq!(backfill(&turn("p1", NOW - DAY - 1, "t1", "t2"), &[]), []);
}

#[test]
fn a_turn_whose_two_trees_are_equal_gets_none() {
    let lines = vec![
        line(
            "UserPromptSubmit",
            "p1",
            NOW - 3000,
            json!({"prompt": "go", "studio_turn_start": {ROOT: "t1"}}),
        ),
        line(
            "Stop",
            "p1",
            NOW - 1000,
            json!({"studio_turn_end": {ROOT: "t1"}}),
        ),
    ];

    assert_eq!(backfill(&lines, &[]), []);
}

/// Esc fires no `Stop`: the next turn's first event ends it, and the hook records its end there.
#[test]
fn a_turn_ended_by_the_next_one_gets_its_trees_from_that_event() {
    let at = NOW - 1000;
    let lines = vec![
        line(
            "UserPromptSubmit",
            "p1",
            at - 3000,
            json!({"prompt": "go", "studio_turn_start": {ROOT: "t1"}}),
        ),
        line(
            "UserPromptSubmit",
            "p2",
            at,
            json!({"prompt": "again", "studio_turn_end": {ROOT: "t2"}, "studio_turn_start": {ROOT: "t2"}}),
        ),
    ];

    let found = backfill(&lines, &[]);

    let ids: Vec<&str> = found.iter().map(|b| b.request.id.as_str()).collect();
    assert_eq!(ids, [format!("{SESSION}@{at}")]);
    assert_eq!(
        found[0].trees,
        [Trees {
            root: ROOT.into(),
            start: "t1".into(),
            end: "t2".into()
        }]
    );
}

/// The live path is unchanged: a live turn with trees requests no backfill.
#[test]
fn a_live_turn_leaves_nothing_to_backfill() {
    let mut studio = Studio::default();
    for line in turn("p1", NOW - 1000, "t1", "t2") {
        studio.apply(&line);
    }

    assert_eq!(studio.backfill(NOW, &[]), []);
}
