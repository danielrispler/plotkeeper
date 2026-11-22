//! The self-test's report (danielrispler/plotkeeper#130): per arm, the turns, the retests answered and right, the turns whose
//! hunk was opened, and the median time from a retest's showing to its pick, from a card store built here.

use studio_understanding::{CardStore, Tap, self_test};

const START: u64 = 1_800_000_000_000;

fn turn(store: &mut CardStore, id: &str, at: u64, body_only: bool) {
    let request = studio_core::SummaryRequest {
        id: id.into(),
        session_id: "s1".into(),
        facts: studio_core::Facts {
            files: vec![studio_core::FactFile {
                file: studio_core::TurnFile {
                    root: "/w".into(),
                    path: "a.rs".into(),
                    from: None,
                    added: Some(1),
                    removed: None,
                    agent: None,
                },
                hunk: "+x\n".into(),
            }],
            ..Default::default()
        },
        ..Default::default()
    };
    let summary: studio_understanding::TurnSummary = serde_json::from_value(serde_json::json!({
        "summary": "Did a thing.",
        "quiz": { "text": "Why?", "key": 0, "options": [
            { "text": "A", "why": "Because." }, { "text": "B", "why": "No." }] },
    }))
    .unwrap();
    store
        .add_summary_with(request, summary, at, body_only)
        .unwrap();
}

/// A retest of turn `id` as an earlier build stored it, shown at `shown` and, when `pick` is given, picked then. No build writes
/// retests now (danielrispler/plotkeeper#347), so the store reads them from `retests.json`.
fn retest(id: &str, shown: u64, pick: Option<(usize, u64)>) -> serde_json::Value {
    let mut taps = vec![serde_json::json!({ "kind": "shown", "at": shown })];
    if let Some((option, at)) = pick {
        taps.push(serde_json::json!({ "kind": "pick", "option": option, "at": at }));
    }
    serde_json::json!({
        "id": format!("retest {id}"), "of": id, "session_id": "s1", "added_at": START,
        "quiz": { "text": "Q", "options": [{ "text": "A", "why": "w" }, { "text": "B", "why": "w" }], "key": 0 },
        "taps": taps,
    })
}

#[test]
fn the_report_counts_each_arm_from_the_store() {
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    store.set_self_test(Some(START));
    // Quiz-first: three turns, two retests answered (one right), a hunk opened on one.
    for id in ["q1", "q2", "q3"] {
        turn(&mut store, id, START + 1, false);
    }
    // Body-only: two turns, one retest answered, right.
    for id in ["b1", "b2"] {
        turn(&mut store, id, START + 2, true);
    }
    // A turn from outside the test is in neither arm.
    store.set_self_test(None);
    turn(&mut store, "out", START + 3, true);
    store.quiz_tap("q1", Tap::Hunk, START + 10).unwrap();
    store.quiz_tap("q1", Tap::Hunk, START + 11).unwrap();
    store.quiz_tap("b2", Tap::Hunk, START + 12).unwrap();
    let retests = [
        retest("q1", 1_000, Some((0, 5_000))),
        retest("q2", 1_000, Some((1, 9_000))),
        retest("q3", 1_000, None),
        retest("b1", 1_000, Some((0, 3_000))),
    ];
    let text = serde_json::to_string(&retests).unwrap();
    std::fs::write(data.path().join("retests.json"), text).unwrap();
    let store = CardStore::open(data.path().join("cards.json")).unwrap();

    let report = self_test(store.summaries(), store.retests());
    let [quiz_first, body_only] = report.arms;
    assert_eq!(
        (quiz_first.turns, quiz_first.answered, quiz_first.right),
        (3, 2, 1)
    );
    assert_eq!(
        (quiz_first.hunks, quiz_first.median_pick_ms),
        (1, Some(6_000))
    );
    assert_eq!(
        (body_only.turns, body_only.answered, body_only.right),
        (2, 1, 1)
    );
    assert_eq!(
        (body_only.hunks, body_only.median_pick_ms),
        (1, Some(2_000))
    );
    let text = report.to_string();
    assert!(
        text.contains("quiz-first")
            && text.contains("body-only")
            && text.contains("About 20 turns")
    );
}

#[test]
fn an_arm_with_nothing_to_count_prints_none_not_a_division() {
    let report = self_test(&[], &[]);
    assert_eq!(report.arms[0].median_pick_ms, None);
    assert!(report.to_string().contains("n/a"));
}
