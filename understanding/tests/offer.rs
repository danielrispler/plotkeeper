//! What a turn gets for a Quiz and the cap per hour (danielrispler/plotkeeper#119), with canned writer replies.

use serde_json::json;
use std::sync::Mutex;
use studio_core::{
    Base, CardRequest, Check, CheckRun, CheckStatus, FactFile, Facts, SummaryRequest, TurnFile,
};
use studio_understanding::{
    Card, CardStore, CardWriter, FakeWriter, Offer, Place, Shape, SummarySetup, Tap, TurnSummary,
    write_stop, write_summary,
};

/// Replies to the summary with a Headline, one more line of `kind`, and a Quiz.
struct Canned(&'static str);

impl CardWriter for Canned {
    fn reply(&self, r: &CardRequest, e: &[String]) -> Result<String, String> {
        FakeWriter.reply(r, e)
    }
    fn more(&self, r: &CardRequest, c: &Card) -> Result<String, String> {
        FakeWriter.more(r, c)
    }
    fn summarize(
        &self,
        _: &SummarySetup,
        _: &SummaryRequest,
        _: &[TurnSummary],
    ) -> Result<String, String> {
        let option = |text: &str, why: &str, right: bool| json!({ "text": text, "why": why, "right": right });
        Ok(json!({
            "headline": "Rewrote the parser.",
            "lines": [{ "kind": self.0, "text": "A line." }],
            "quiz": { "text": "Why a stack?", "key": 0, "options": [
                option("A stack.", "It nests.", true),
                option("A queue of tokens.", "It does not nest.", false),
                option("A recursive descent only.", "It overflows.", false),
            ]},
        })
        .to_string())
    }
}

fn quiz() -> studio_understanding::Quiz {
    let option = |text: &str| studio_understanding::Choice {
        text: text.into(),
        why: "r".into(),
    };
    studio_understanding::Quiz {
        text: "Why?".into(),
        options: vec![option("a"), option("b"), option("c")],
        key: 0,
    }
}

fn store() -> (tempfile::TempDir, Mutex<CardStore>) {
    let dir = tempfile::tempdir().unwrap();
    let store = Mutex::new(CardStore::open(dir.path().join("cards.json")).unwrap());
    (dir, store)
}

fn turn(n: u64) -> SummaryRequest {
    SummaryRequest {
        session_id: "s".into(),
        id: format!("s@{n}"),
        prompt: "Explain the parser".into(),
        responses: vec!["Rewrote the parser.".into()],
        intentions: vec!["Rewrote".into()],
        ..SummaryRequest::default()
    }
}

fn lone(
    store: &Mutex<CardStore>,
    n: u64,
    at: u64,
    asks: bool,
    kind: &'static str,
) -> Option<TurnSummary> {
    let request = SummaryRequest { asks, ..turn(n) };
    let _ = at;
    write_summary(
        store,
        &Canned(kind),
        &SummarySetup::default(),
        request,
        |l| panic!("{l}"),
    )
}

fn stop(store: &Mutex<CardStore>, n: u64) {
    let request = CardRequest {
        session_id: "s".into(),
        id: format!("s@{n}"),
        base: Base::default(),
        change: Some("--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n".into()),
        ..CardRequest::default()
    };
    let session = studio_core::Session::default();
    assert!(write_stop(
        store,
        &FakeWriter,
        request,
        &session,
        || {},
        |l| panic!("{l}")
    ));
}

fn quiz_of_card(store: &Mutex<CardStore>, n: u64) -> bool {
    let store = store.lock().unwrap();
    let card = store
        .cards()
        .iter()
        .find(|k| k.request.id == format!("s@{n}"))
        .unwrap();
    card.card.quiz.is_some()
}

#[test]
fn the_fourth_quiz_within_an_hour_is_not_offered() {
    let (_dir, store) = store();
    for n in 1..=3 {
        assert!(
            lone(&store, n, 0, false, "why").unwrap().quiz.is_some(),
            "{n}"
        );
    }
    assert_eq!(lone(&store, 4, 0, false, "why").unwrap().quiz, None);
    // The summary itself is kept.
    assert!(store.lock().unwrap().turn_summary("s@4").is_some());
}

#[test]
fn a_cards_quiz_past_the_cap_replaces_the_oldest_unanswered_lone_quiz() {
    let (_dir, store) = store();
    for n in 1..=3 {
        lone(&store, n, 0, false, "why");
    }
    // The first Quiz was answered: the oldest unanswered one is the second.
    store
        .lock()
        .unwrap()
        .quiz_tap("s@1", Tap::Pick { option: 0 }, 1)
        .unwrap();
    stop(&store, 10);
    assert!(quiz_of_card(&store, 10));
    let kept = |id: &str| {
        store
            .lock()
            .unwrap()
            .turn_summary(id)
            .unwrap()
            .summary
            .quiz
            .is_some()
    };
    assert!(kept("s@1") && !kept("s@2") && kept("s@3"));
}

#[test]
fn a_cards_quiz_past_the_cap_with_every_lone_quiz_answered_is_not_offered() {
    let (_dir, store) = store();
    for n in 1..=3 {
        lone(&store, n, 0, false, "why");
        store
            .lock()
            .unwrap()
            .quiz_tap(&format!("s@{n}"), Tap::Pick { option: 0 }, 1)
            .unwrap();
    }
    stop(&store, 10);
    assert!(!quiz_of_card(&store, 10));
    assert_eq!(
        store
            .lock()
            .unwrap()
            .place_quiz(true, studio_understanding::millis()),
        Place::Withhold
    );
}

#[test]
fn a_quiz_older_than_an_hour_does_not_count() {
    let (_dir, store) = store();
    for n in 1..=3 {
        let at = studio_understanding::millis() - 61 * 60 * 1000;
        let mut kept = store.lock().unwrap();
        let summary = TurnSummary {
            summary: String::new(),
            headline: None,
            lines: vec![],
            quiz: Some(quiz()),
        };
        kept.add_summary_at(turn(n), summary, at).unwrap();
    }
    assert_eq!(
        store
            .lock()
            .unwrap()
            .place_quiz(false, studio_understanding::millis()),
        Place::Offer
    );
}

#[test]
fn a_turn_the_table_gives_no_quiz_keeps_its_summary_without_one() {
    let (_dir, store) = store();
    let request = SummaryRequest {
        no_quiz: true,
        ..turn(1)
    };
    let summary = write_summary(
        &store,
        &Canned("why"),
        &SummarySetup::default(),
        request,
        |l| panic!("{l}"),
    )
    .unwrap();
    assert_eq!(summary.quiz, None);
    assert_eq!(summary.headline.as_deref(), Some("Rewrote the parser."));
}

#[test]
fn a_turn_that_asks_has_no_quiz_beside_the_decision() {
    let (_dir, store) = store();
    assert_eq!(lone(&store, 1, 0, true, "why").unwrap().quiz, None);
    assert_eq!(lone(&store, 2, 0, false, "ask").unwrap().quiz, None);
    assert!(lone(&store, 3, 0, false, "open").unwrap().quiz.is_some());
    // The card's Quiz goes with it.
    stop(&store, 5);
    let request = SummaryRequest {
        card: Some("s@5".into()),
        ..turn(5)
    };
    write_summary(
        &store,
        &Canned("ask"),
        &SummarySetup::default(),
        request,
        |l| panic!("{l}"),
    );
    assert!(!quiz_of_card(&store, 5));
}

// The table's edges, from a turn's recorded facts.

fn facts(files: &[(&str, u64)], check: Option<CheckStatus>) -> Facts {
    let file = |(path, lines): &(&str, u64)| FactFile {
        file: TurnFile {
            root: "/r".into(),
            path: (*path).into(),
            from: None,
            added: Some(*lines),
            removed: Some(0),
        },
        hunk: format!(
            "--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n{}",
            "+let x = 1;\n".repeat(*lines as usize)
        ),
    };
    let check = check.map(|status| Check {
        suite: "cargo test".into(),
        status,
        retries: 0,
        runs: Vec::<CheckRun>::new(),
    });
    Facts {
        files: files.iter().map(file).collect(),
        external: 0,
        checks: check.into_iter().collect(),
    }
}

fn offer(files: &[(&str, u64)], check: Option<CheckStatus>, merge: bool) -> Offer {
    Shape::of(&facts(files, check), "", merge).offer()
}

#[test]
fn the_table_holds_at_each_edge() {
    assert_eq!(offer(&[("a.rs", 24)], None, false), Offer::Quiz);
    assert_eq!(offer(&[("a.rs", 25)], None, false), Offer::Card);
    assert_eq!(offer(&[("a.rs", 9)], None, false), Offer::Quiz);
    assert_eq!(offer(&[("a.rs", 9), ("b.rs", 1)], None, false), Offer::Card);
    assert_eq!(offer(&[("a.rs", 7)], None, false), Offer::Nothing);
    assert_eq!(offer(&[("a.rs", 8)], None, false), Offer::Quiz);
    assert_eq!(
        offer(&[("a.rs", 8)], Some(CheckStatus::Failed), false),
        Offer::Nothing
    );
    assert_eq!(
        offer(&[("a.rs", 8)], Some(CheckStatus::Passed), false),
        Offer::Quiz
    );
    assert_eq!(offer(&[("README.md", 40)], None, false), Offer::Nothing);
    assert_eq!(offer(&[("a.rs", 12)], None, true), Offer::Nothing);
    assert_eq!(offer(&[("tests/a.rs", 12)], None, false), Offer::Nothing);
}

#[test]
fn an_agent_with_no_trees_reads_its_cards_change() {
    let diff = format!(
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1,30 @@\n{}",
        "+let x = 1;\n".repeat(30)
    );
    assert_eq!(
        Shape::of(&Facts::default(), &diff, false).offer(),
        Offer::Card
    );
    assert_eq!(
        Shape::of(&Facts::default(), "", false).offer(),
        Offer::Nothing
    );
}
