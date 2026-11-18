//! The self-test's report (danielrispler/plotkeeper#130): per arm, the turns, the retests answered and right, the turns whose
//! hunk was opened, and the median time from a retest's showing to its pick, from a card store built here.

use studio_understanding::{CardStore, Choice, Quiz, Tap, self_test};

const START: u64 = 1_800_000_000_000;
const WEEK: u64 = 7 * 24 * 60 * 60 * 1000;

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

/// Retests turn `id`, shown at `shown` and, when `pick` is given, picked then.
fn retest(store: &mut CardStore, id: &str, shown: u64, pick: Option<(usize, u64)>) {
    let ask = store
        .due_retests(u64::MAX)
        .into_iter()
        .find(|a| a.of == id)
        .unwrap();
    let option = |text: &str| Choice {
        text: text.into(),
        why: "w".into(),
    };
    let quiz = Quiz {
        text: "Q".into(),
        options: vec![option("A"), option("B")],
        key: 0,
    };
    store.add_retest(&ask, Some(quiz), START + WEEK).unwrap();
    let name = format!("retest {id}");
    store.quiz_tap(&name, Tap::Shown, shown).unwrap();
    if let Some((option, at)) = pick {
        store.quiz_tap(&name, Tap::Pick { option }, at).unwrap();
    }
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
    retest(&mut store, "q1", 1_000, Some((0, 5_000)));
    retest(&mut store, "q2", 1_000, Some((1, 9_000)));
    retest(&mut store, "q3", 1_000, None);
    retest(&mut store, "b1", 1_000, Some((0, 3_000)));

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
