//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for the backfill (danielrispler/Falinks#486): the hook records each worktree's git
//! tree at a turn's start and end, so a turn that ended while the app was down gets its card request
//! after the replay. The shell turns the two trees into the card's diff.

use crate::common;
use serde_json::json;
use studio_core::{Backfill, CardRequest, CatchUp, Studio, Trees};

const SESSION: &str = "4c2e8a1f-7b3d-4e9a-a6c5-0f1d2b3e4a57";
const ROOT: &str = "/w/repo";
const OTHER: &str = "/w/other";
const HOUR: u64 = 60 * 60 * 1000;
/// When the app starts after the log.
const NOW: u64 = 1_800_000_000_000;

/// One event of the session's own agent in turn `prompt`, received at `at`, with `extra` fields.
fn line(name: &str, prompt: &str, at: u64, extra: serde_json::Value) -> String {
    let mut base = common::base(SESSION, name, ROOT, at);
    base["prompt_id"] = json!(prompt);
    common::with(base, extra)
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
    studio.backfill(NOW, cards, CatchUp::default())
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
fn a_turn_older_than_the_default_window_gets_none() {
    assert_eq!(
        backfill(&turn("p1", NOW - 2 * HOUR - 1, "t1", "t2"), &[]),
        []
    );
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

    assert_eq!(studio.backfill(NOW, &[], CatchUp::default()), []);
}

/// One event of an agy session, whose terminal is `fixture-terminal`, received at `at`.
fn agy_line(name: &str, at: u64, extra: serde_json::Value) -> String {
    let mut event = json!({
        "conversationId": SESSION,
        "hookEventName": name,
        "cwd": ROOT,
        "studio_received_ms": at,
        "studio_terminal_id": "fixture-terminal",
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
}

/// A backfilled card judges "asks" from what the replay saw, not from the terminal's state, which
/// a resumed state carries from before the app went down: a turn that ended at your turn asks
/// nothing.
#[test]
fn a_backfilled_card_does_not_ask_because_the_resumed_terminal_state_was_blocked() {
    let at = NOW - 1000;
    let before = [
        agy_line("SessionStart", at - 3000, json!({})),
        agy_line(
            "PreInvocation",
            at - 2000,
            json!({"invocationNum": 0, "initialNumSteps": 1, "lastUserInput": "go",
                "studio_turn_start": {ROOT: "t1"}}),
        ),
    ];
    let mut was = Studio::replayed(&before);
    let terminal: studio_core::Terminal = serde_json::from_value(json!({
        "id": "fixture-terminal", "argv": [], "cwd": "/tmp", "pid": 1, "exited": null,
        "foreground": "agy", "processes": [], "state": "blocked", "app": "agy",
        "status_ms": 1, "turn": 0,
    }))
    .unwrap();
    was.terminals(&[terminal], 10);
    let saved = was.save();

    let mut studio = Studio::default();
    studio.resume(&saved, u64::MAX);
    let stop = agy_line(
        "Stop",
        at,
        json!({"fullyIdle": true, "studio_turn_end": {ROOT: "t2"}}),
    );
    studio.replay(&stop);
    studio.replay(&agy_line("PostInvocation", at + 1, json!({})));
    let found = studio.backfill(NOW, &[], CatchUp::default());

    assert_eq!(found.len(), 1, "{found:?}");
    assert!(!found[0].request.asks);
}

fn ids(found: &[Backfill]) -> Vec<String> {
    found.iter().map(|b| b.request.id.clone()).collect()
}

#[test]
fn a_turn_older_than_the_window_gets_no_card_at_start_and_a_wider_window_takes_it() {
    let at = NOW - 3 * HOUR;
    let lines = turn("p1", at, "t1", "t2");

    assert_eq!(backfill(&lines, &[]), []);
    let wide = CatchUp {
        window_ms: 4 * HOUR,
        max: 10,
    };
    let found = Studio::replayed(&lines).backfill(NOW, &[], wide);
    assert_eq!(ids(&found), [format!("{SESSION}@{at}")]);
}

#[test]
fn only_the_newest_turns_up_to_the_cap_get_a_card_at_start_newest_first() {
    let lines: Vec<String> = (1..=12u64)
        .flat_map(|n| {
            turn(
                &format!("p{n}"),
                NOW - 10_000 + n * 100,
                "t1",
                &format!("e{n}"),
            )
        })
        .collect();

    let found = backfill(&lines, &[]);

    let expected: Vec<String> = (3..=12u64)
        .rev()
        .map(|n| format!("{SESSION}@{}", NOW - 10_000 + n * 100))
        .collect();
    assert_eq!(ids(&found), expected);
    let two = CatchUp {
        window_ms: 2 * HOUR,
        max: 2,
    };
    assert_eq!(Studio::replayed(&lines).backfill(NOW, &[], two).len(), 2);
}

/// A card written on open can fail (logged out, quota): the turn keeps waiting, so opening it again asks again.
#[test]
fn a_turn_opened_without_a_card_keeps_waiting_for_the_next_opening() {
    let at = NOW - 3 * HOUR;
    let id = format!("{SESSION}@{at}");
    let mut studio = Studio::replayed(&turn("p1", at, "t1", "t2"));
    assert_eq!(studio.backfill(NOW, &[], CatchUp::default()), []);

    let first = studio.backfill_on_open(&id);
    let second = studio.backfill_on_open(&id);

    assert_eq!(first.map(|b| b.request.id), Some(id.clone()));
    assert_eq!(second.map(|b| b.request.id), Some(id));
}
