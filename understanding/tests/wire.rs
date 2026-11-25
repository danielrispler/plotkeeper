//! The wire's agreement check (#246) for the understanding layer's types: the session list's rows, a session's brief and decisions, a turn's summary, written into `scripts/wire/understanding.ts`
//! (`scripts/wire/samples.rs`). The store's values are built whole, every optional field filled once and left empty
//! once, so the samples hold every shape; still, an optional field no sample fills is not checked.

use crate::samples::{Sample, json, write};
use studio_core::{FactFile, Facts, Studio, SummaryRequest, TurnFile};
use studio_understanding::{
    Answered, Arm, CardStore, Choice, DesignLine, Line, LineKind, OpenItem, OpenKind, Quiz,
    QuizTap, RiskLevel, RiskView, Row, SessionCards, StoredSummary, Surface, Tap, TurnSummary,
    hunk_view,
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

// @law: wire-types-agree
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one sample per reply, written out in one place"
)]
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
    samples.push(("SessionCards", json(store.session_cards(&session.agent.id))));
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
        decisions,
        // A card's note on its way, one that arrived and one that is not coming.
        risk_notes: vec![
            RiskView::Reading {
                call: "toolu_1".into(),
            },
            RiskView::Arrived {
                call: "toolu_2".into(),
                level: RiskLevel::VeryHigh,
                line: "Deletes the remote branch feature-x on GitHub".into(),
                at: AT,
            },
            RiskView::Failed {
                call: "toolu_3".into(),
            },
        ],
        // The note of a Bash decision, by the decision's call.
        decision_notes: vec![RiskView::Arrived {
            call: "toolu_2".into(),
            level: RiskLevel::High,
            line: "Pushes a branch to GitHub".into(),
            at: AT,
        }],
    };
    let shown = json(shown);
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

    // Why it didn't touch a name, as `ask_why_not` sends it: with the writer's reasons, and with nothing recorded.
    let facts = studio_core::AskFacts {
        sections: vec![studio_core::AskSection {
            label: "Reads".into(),
            facts: vec![studio_core::AskFact {
                at: Some(AT),
                text: "Read src/lib.rs".into(),
            }],
        }],
    };
    samples.push((
        "WhyNot",
        json(studio_understanding::WhyNot {
            facts,
            reasons: Some("It read the file but the plan did not name it.".into()),
        }),
    ));
    samples.push((
        "WhyNot",
        json(studio_understanding::WhyNot {
            facts: studio_core::AskFacts::default(),
            reasons: None,
        }),
    ));

    let types = [
        "Session",
        "SessionCards",
        "OpenItem",
        "TurnSummary",
        "Hunk",
        "WhyNot",
    ];
    write(
        env!("CARGO_MANIFEST_DIR"),
        "understanding",
        &types,
        &samples,
    );
}
