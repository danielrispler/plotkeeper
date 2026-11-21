//! The wire's agreement check (#246) for the understanding layer's types: the session list's rows, a session's cards
//! with its brief, a turn's summary and Home's learning slot, written into `scripts/wire/understanding.ts`
//! (`scripts/wire/samples.rs`). The store's values are built whole, every optional field filled once and left empty
//! once, so the samples hold every shape; still, an optional field no sample fills is not checked.

use crate::samples::{Sample, json, write};
use std::collections::{BTreeMap, BTreeSet};
use studio_core::{Base, CardRequest, FactFile, Facts, Studio, SummaryRequest, TurnFile};
use studio_understanding::{
    Answered, Arm, Card, CardStore, CheckResult, Choice, CodeRange, DesignLine, Guess, Learning,
    Level, Line, LineKind, OpenItem, OpenKind, PlacedCard, Quiz, QuizTap, Rating, Reason, Retest,
    Row, SessionCards, StoredCard, StoredSummary, Surface, Tap, TurnSummary, hunk_view,
};

const SESSION: &str = "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092";
const AT: u64 = 1_761_743_990_000;

fn quiz() -> Quiz {
    let choice = |text: &str| Choice {
        text: text.into(),
        why: format!("Why {text}"),
    };
    Quiz {
        text: "Which file holds the rule?".into(),
        options: vec![choice("a"), choice("b"), choice("c")],
        key: 1,
    }
}

/// Every kind of tap, a Confidence on each surface.
fn taps() -> Vec<QuizTap> {
    let taps = [
        Tap::Pick { option: 1 },
        Tap::Confidence {
            sure: true,
            surface: Surface::Home,
        },
        Tap::Confidence {
            sure: false,
            surface: Surface::Rail,
        },
        Tap::Why {
            text: "It reads the log".into(),
        },
        Tap::Shown,
        Tap::Hunk,
        Tap::Skip,
        Tap::Understood,
        Tap::ExplainMore,
        Tap::KeyWrong,
        Tap::NotNow,
    ];
    taps.into_iter()
        .map(|tap| QuizTap { tap, at: AT })
        .collect()
}

/// A card with every field filled, and one with every optional field empty.
fn cards() -> [StoredCard; 2] {
    let request = CardRequest {
        session_id: SESSION.into(),
        id: "toolu_01A".into(),
        base: Base {
            head: Some("5f0c1e2d9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d".into()),
            files: vec!["README.md".into()],
        },
        change: Some("--- a/src/lib.rs\n+++ b/src/lib.rs\n".into()),
        prompt: Some(AT),
        agent: Some("a1b2c3".into()),
        changed_by: BTreeMap::from([("d4e5f6".into(), BTreeSet::from(["src/lib.rs".into()]))]),
        workers: vec!["a1b2c3".into()],
        check: Some("cargo test".into()),
        ends: BTreeMap::from([("/repo".into(), "4b825dc".into())]),
        asks: true,
    };
    let card = Card {
        explanation: "It reads the log once.".into(),
        summary: "Reads the log once".into(),
        paths: vec!["src/lib.rs".into()],
        code: "fn read() {}".into(),
        ranges: vec![CodeRange {
            path: "src/lib.rs".into(),
            from: 1,
            to: 3,
        }],
        why_it_matters: "A second read doubles the rows.".into(),
        not_checked: "A log over 1 GB.".into(),
        architecture: Some("The core owns the log.".into()),
        quiz: Some(quiz()),
    };
    let full = StoredCard {
        request: request.clone(),
        card: card.clone(),
        added_at: Some(AT),
        shown_at: Some(AT),
        guess: Some(Guess {
            option: 1,
            right: Some(true),
            at: AT,
        }),
        key_wrong: true,
        more: Some("More on it.".into()),
        rating: Some(Rating {
            level: Level::Fair,
            reasons: vec![
                Reason::Unclear,
                Reason::Wrong,
                Reason::TooShallow,
                Reason::TooEasy,
            ],
        }),
        check_result: Some(CheckResult::NotCounted),
        taps: taps(),
    };
    let bare = StoredCard {
        request: CardRequest {
            session_id: SESSION.into(),
            id: "toolu_01B".into(),
            ..CardRequest::default()
        },
        card: Card {
            ranges: Vec::new(),
            architecture: None,
            quiz: None,
            ..card
        },
        added_at: None,
        shown_at: None,
        guess: None,
        key_wrong: false,
        more: None,
        rating: None,
        check_result: None,
        taps: Vec::new(),
    };
    [full, bare]
}

/// A turn's summary with its request, facts and Quiz, and one stored before the Headline and lines.
fn summaries(checks: &Studio) -> [StoredSummary; 2] {
    let check = (checks.sessions().iter())
        .flat_map(|s| &s.turns)
        .flat_map(|t| &t.checks)
        .next()
        .expect("a check in the fixture")
        .clone();
    let file = TurnFile {
        root: "/repo".into(),
        path: "src/lib.rs".into(),
        from: Some("src/old.rs".into()),
        added: Some(3),
        removed: None,
        agent: Some("a1b2c3".into()),
    };
    let request = SummaryRequest {
        session_id: SESSION.into(),
        id: format!("{SESSION}@{AT}"),
        prompt: "Fix the reader".into(),
        responses: vec!["Done.".into()],
        intentions: vec!["Fixing it".into()],
        card: Some("toolu_01A".into()),
        facts: Facts {
            files: vec![FactFile {
                file,
                hunk: "@@ -1 +1 @@".into(),
            }],
            checks: vec![check],
            external: 0,
        },
        wanted: true,
        asks: true,
        no_quiz: true,
    };
    let kinds = [
        LineKind::Why,
        LineKind::Reported,
        LineKind::Open,
        LineKind::Ask,
    ];
    let full = StoredSummary {
        request: request.clone(),
        summary: TurnSummary {
            summary: "Fixed the reader".into(),
            headline: Some("Fixed the reader".into()),
            lines: (kinds.into_iter())
                .map(|kind| Line {
                    kind,
                    text: "A line".into(),
                })
                .collect(),
            quiz: Some(quiz()),
        },
        added_at: AT,
        taps: taps(),
        hunk: "--- src/lib.rs\n@@ -1 +1 @@".into(),
        arm: Some(Arm::BodyOnly),
    };
    let old = StoredSummary {
        request: SummaryRequest::default(),
        summary: TurnSummary {
            summary: "Fixed the reader".into(),
            headline: None,
            lines: Vec::new(),
            quiz: None,
        },
        added_at: AT,
        taps: Vec::new(),
        hunk: String::new(),
        arm: None,
    };
    [full, old]
}

/// Each of Home's learning items: a Retest filled in for serving and one as stored, a fresh Quiz, and done.
fn learning() -> [Learning; 4] {
    let retest = Retest {
        id: format!("retest {SESSION}@{AT}"),
        of: format!("{SESSION}@{AT}"),
        session_id: SESSION.into(),
        added_at: AT,
        of_at: Some(AT),
        hunk: "--- src/lib.rs".into(),
        quiz: Some(quiz()),
        taps: taps(),
    };
    let stored = Retest {
        of_at: None,
        hunk: String::new(),
        quiz: None,
        taps: Vec::new(),
        ..retest.clone()
    };
    [
        Learning::Retest(retest),
        Learning::Retest(stored),
        Learning::Fresh {
            id: format!("{SESSION}@{AT}"),
            session_id: SESSION.into(),
            quiz: quiz(),
            at: AT,
            hunk: "--- src/lib.rs".into(),
        },
        Learning::Done,
    ]
}

#[test]
fn the_wire_samples_of_the_understanding_layer_are_written_for_tsc() {
    let data = tempfile::tempdir().unwrap();
    let store = CardStore::open(data.path().join("cards.json")).unwrap();
    let studio =
        Studio::replayed(include_str!("../../core/tests/fixtures/ask-then-plan.jsonl").lines());
    let checks = Studio::replayed(include_str!("../../core/tests/fixtures/checks.jsonl").lines());
    let mut samples: Vec<Sample> = Vec::new();

    // The session list's rows, as `sessions` sends them.
    for session in studio.sessions().into_iter().chain(checks.sessions()) {
        let row = Row {
            line: store.line(session, AT),
            session: session.clone(),
        };
        samples.push(("Session", json(row)));
    }

    // A session's cards: the store's own, and one with every slot filled.
    let session = studio.sessions()[0];
    samples.push(("SessionCards", json(store.rail_cards(session, AT))));
    let [full, bare] = cards();
    let kinds = [
        OpenKind::Question,
        OpenKind::Check,
        OpenKind::Unchecked,
        OpenKind::Open,
        OpenKind::Ask,
        OpenKind::Unpushed,
        OpenKind::Pr,
    ];
    let open: Vec<OpenItem> = (kinds.into_iter())
        .map(|kind| OpenItem {
            kind,
            text: "An open line".into(),
        })
        .collect();
    let decisions = session.decisions.clone();
    assert!(!decisions.is_empty(), "the fixture's answers");
    let shown = SessionCards {
        brief: Some("A brief stored as prose".into()),
        goal: Some("Fix the reader".into()),
        answers: vec![Answered {
            question: "Which reader?".into(),
            reply: "The log's".into(),
        }],
        open: open.clone(),
        design: vec![DesignLine {
            text: "One read per start".into(),
            cite: "toolu_01A".into(),
        }],
        recent: vec![full.clone(), bare.clone()],
        older: vec![bare.clone()],
        workers: vec![
            PlacedCard {
                card: full,
                turn: Some(0),
            },
            PlacedCard {
                card: bare,
                turn: None,
            },
        ],
        decisions,
    };
    let shown = json(shown);
    for card in shown["recent"].as_array().unwrap() {
        samples.push(("StoredCard", card.clone()));
    }
    for item in &open {
        samples.push(("OpenItem", json(item)));
    }
    samples.push(("SessionCards", shown));

    // A turn's summary, as `turn_summary` sends it.
    for summary in summaries(&checks) {
        samples.push(("TurnSummary", json(summary)));
    }

    // A file's hunk, as `turn_file_hunk` sends it, and a binary one's.
    let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3 +3 @@\n-old\n+new\n";
    samples.push(("Hunk", json(hunk_view(diff, false))));
    samples.push((
        "Hunk",
        json(hunk_view(
            "Binary files a/logo.png and b/logo.png differ\n",
            false,
        )),
    ));

    // Home's learning slot.
    for item in learning() {
        samples.push(("Learning", json(item)));
    }

    let types = [
        "Session",
        "SessionCards",
        "StoredCard",
        "OpenItem",
        "TurnSummary",
        "Hunk",
        "Learning",
    ];
    write(
        env!("CARGO_MANIFEST_DIR"),
        "understanding",
        &types,
        &samples,
    );
}
