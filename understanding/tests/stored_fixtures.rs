//! NEVER EDIT A FROZEN SAMPLE in `tests/fixtures/stored/`. Each one is a stored file as some build of Studio wrote it,
//! and today's code must keep loading it: a sample that fails is fixed in the reading code (a `#[serde(default)]`, an
//! `Option`, a tolerant enum), or, for a deliberate format bump, by testing the move-aside instead. When a stored type
//! changes, add a new sample beside the old ones (see the folder's `README.md`).
//!
//! The files are the ones `CardStore::open` reads at Studio's start, where a parse failure stops the start
//! (`.expect("read the card store")`). The samples are hand-made, never copied from a real data folder.

use std::fs;
use std::path::Path;
use studio_core::CheckStatus as Status;
use studio_core::Source;
use studio_understanding::{CardStore, Level, Reason};
use studio_understanding::{CheckResult, RiskLevel, RiskNotes, RiskView, Surface, Tap};

const FILES: [&str; 7] = [
    "cards-v2.json",
    "briefs.json",
    "live-facts.json",
    "decisions.json",
    "summaries.json",
    "agent-summaries.json",
    "retests.json",
];

/// The store opened from a temporary copy of the frozen sample folder `sample`, so that opening it can write nothing
/// into the repository. The folder is returned to keep it alive.
fn open(sample: &str) -> (tempfile::TempDir, CardStore) {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stored")
        .join(sample);
    let data = tempfile::tempdir().unwrap();
    for file in FILES {
        fs::copy(from.join(file), data.path().join(file))
            .unwrap_or_else(|e| panic!("sample `{sample}/{file}`: {e}"));
    }
    let store = CardStore::open(data.path().join("cards-v2.json"))
        .unwrap_or_else(|e| panic!("the `{sample}` sample does not load: {e}"));
    (data, store)
}

// @law: old-stored-files-load
#[test]
fn the_full_samples_of_every_stored_file_load_with_their_values() {
    let (_data, store) = open("full");

    let [card] = store.cards() else {
        panic!("one card")
    };
    assert_eq!(card.request.id, "worker-1@1761743900000");
    assert_eq!(
        card.request.change.as_deref(),
        Some("--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\n")
    );
    assert_eq!(card.card.explanation, "Sample explanation.");
    assert_eq!(card.card.quiz.as_ref().unwrap().options.len(), 2);
    assert_eq!(card.added_at, Some(1_761_743_990_000));
    assert_eq!(card.guess.unwrap().option, 1);
    assert!(card.key_wrong);
    assert_eq!(card.more.as_deref(), Some("A longer explanation."));
    let rating = card.rating.as_ref().unwrap();
    assert_eq!(
        (rating.level, &rating.reasons[..]),
        (Level::Fair, &[Reason::Unclear, Reason::TooEasy][..])
    );
    assert_eq!(card.check_result, Some(CheckResult::Passed));
    assert_eq!(card.taps.len(), 2);
    assert!(matches!(card.taps[0].tap, Tap::Pick { option: 1 }));

    let brief = store.brief("sess-1").unwrap();
    assert_eq!(brief.goal.as_deref(), Some("Sample goal."));
    assert_eq!(brief.design[0].cite, "sess-1@1761743800000");

    assert!(store.facts().promoted.contains("helper-1"));
    assert!(store.facts().opened.contains("line-1"));
    assert_eq!(store.facts().seen["sess-1"], 3);

    let decisions = store.session_cards("sess-1", 1_761_744_000_000).decisions;
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].call, "toolu_1");
    assert_eq!(decisions[0].source, Source::Card);

    let [summary] = store.summaries() else {
        panic!("one summary")
    };
    assert_eq!(
        summary.summary.headline.as_deref(),
        Some("Sample headline.")
    );
    assert_eq!(summary.hunk, "@@ -1 +1 @@\n-old\n+new\n");
    let [check] = &summary.request.facts.checks[..] else {
        panic!("one check")
    };
    assert_eq!(check.runs.len(), 2);
    assert_eq!(check.runs[1].statement, "cargo test");
    assert_eq!(check.runs[1].status, Status::Passed);

    assert_eq!(
        store.agent_summary("agent-1"),
        Some("Sample agent summary.")
    );

    let [retest] = store.retests() else {
        panic!("one retest")
    };
    assert_eq!(
        (retest.id.as_str(), retest.of.as_str()),
        ("retest sess-1@1761743900000", "sess-1@1761743900000")
    );
    assert_eq!(retest.quiz.as_ref().unwrap().key, 0);
    assert_eq!(retest.taps.len(), 1);
}

// @law: old-stored-files-load
#[test]
fn the_minimal_samples_of_every_stored_file_load_with_their_values() {
    let (_data, store) = open("minimal");

    let [card] = store.cards() else {
        panic!("one card")
    };
    assert_eq!(card.request.id, "sess-1@1761743900000");
    assert_eq!(card.card.explanation, "Sample explanation.");
    assert_eq!(card.added_at, None);
    assert!(card.card.quiz.is_none());
    let [tap] = &card.taps[..] else {
        panic!("one tap")
    };
    assert_eq!(
        tap.tap,
        Tap::Confidence {
            sure: true,
            surface: Surface::Rail
        }
    );

    assert_eq!(store.brief("sess-1").unwrap().goal, None);
    assert!(store.facts().promoted.is_empty());

    let decisions = store.session_cards("sess-1", 1_761_744_000_000).decisions;
    assert_eq!(decisions[0].call, "toolu_1");

    let [summary] = store.summaries() else {
        panic!("one summary")
    };
    assert_eq!(summary.summary.summary, "Sample summary.");
    assert!(summary.request.wanted);
    let [check] = &summary.request.facts.checks[..] else {
        panic!("one check")
    };
    assert_eq!(check.runs[0].command, "cargo test");
    assert_eq!(check.runs[0].statement, "");

    assert_eq!(
        store.agent_summary("agent-1"),
        Some("Sample agent summary.")
    );

    let [retest] = store.retests() else {
        panic!("one retest")
    };
    assert_eq!(retest.of, "sess-1@1761743900000");
    assert!(retest.quiz.is_none());
}

/// The risk notes (#298) of the frozen sample folder `sample`, read for session `sess-1` with a card for each of `commands`,
/// which also freezes the hash that keys a note: the command, a NUL and its folder `/work/repo`, hashed with FNV-1a.
fn risk_views(sample: &str, commands: &[&str]) -> Vec<RiskView> {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stored")
        .join(sample)
        .join("risk-notes.json");
    let data = tempfile::tempdir().unwrap();
    fs::copy(&from, data.path().join("risk-notes.json"))
        .unwrap_or_else(|e| panic!("sample `{sample}/risk-notes.json`: {e}"));
    let notes = RiskNotes::open(data.path().join("risk-notes.json"))
        .unwrap_or_else(|e| panic!("the `{sample}` risk notes do not load: {e}"));
    let line = |name: &str, at: u64, extra: serde_json::Value| {
        let mut line = serde_json::json!({
            "session_id": "sess-1", "hook_event_name": name, "cwd": "/work/repo", "studio_received_ms": at,
        });
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line.to_string()
    };
    let mut lines = vec![
        line(
            "SessionStart",
            1_000,
            serde_json::json!({ "source": "startup" }),
        ),
        line(
            "UserPromptSubmit",
            2_000,
            serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
        ),
    ];
    for (n, command) in commands.iter().enumerate() {
        lines.push(line(
            "PermissionDenied",
            3_000 + n as u64,
            serde_json::json!({ "tool_name": "Bash", "tool_use_id": format!("toolu_{n}"), "prompt_id": "p-1",
                "tool_input": { "command": command }, "reason": "[Unauthorized Access]", "permission_mode": "auto" }),
        ));
    }
    let studio = studio_core::Studio::replayed(&lines);
    notes.views(studio.session("sess-1").unwrap())
}

#[test]
fn the_full_sample_of_the_risk_notes_loads_with_its_values() {
    let views = risk_views("full-2", &["git push origin feature-x", "rm -rf build"]);

    assert_eq!(
        views,
        [
            RiskView::Arrived {
                call: "toolu_0".into(),
                level: RiskLevel::Medium,
                line: "Pushes the branch issue-298 to origin.".into(),
                at: 1_761_743_950_000
            },
            RiskView::Arrived {
                call: "toolu_1".into(),
                level: RiskLevel::VeryHigh,
                line: "Deletes the build folder and everything in it.".into(),
                at: 1_761_743_960_000
            },
        ]
    );
}

#[test]
fn the_minimal_sample_of_the_risk_notes_loads_with_its_values() {
    let views = risk_views("minimal-2", &["git push origin feature-x"]);

    assert_eq!(
        views,
        [RiskView::Arrived {
            call: "toolu_0".into(),
            level: RiskLevel::None,
            line: "Only reads.".into(),
            at: 1
        }]
    );
}
