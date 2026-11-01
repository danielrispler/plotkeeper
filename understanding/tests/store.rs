//! The card store (#328, module 6; #369, decisions 15 and 17): cards, their base, Daniel's guesses,
//! his flags, his ratings and the longer explanations he asked for survive a restart.

use std::{cell::Cell, sync::Mutex};
use studio_core::{Base, CardRequest, Change, Output, Studio};
use studio_understanding::{
    Card, CardStore, CardWriter, FakeWriter, Guess, Level, Rating, Reason, StoredCard,
    explain_more, write_card,
};

/// The card on an edit of `path`, named `id`, from a base with `README.md` already edited.
fn edit(id: &str, path: &str) -> CardRequest {
    CardRequest {
        session_id: "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092".into(),
        id: id.into(),
        base: Base {
            head: Some("5f0c1e2d9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d".into()),
            files: vec!["README.md".into()],
        },
        change: format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n"),
    }
}

/// The fake writer's card on `request`, whose quiz's key is its first option of three.
fn card(request: &CardRequest) -> Card {
    write_card(&FakeWriter, request, |line| panic!("{line}")).unwrap()
}

/// A store at `path` with a card on `src/lib.rs` and one on `README.md`.
fn two_cards(path: &std::path::Path) -> CardStore {
    let mut store = CardStore::open(path.into()).unwrap();
    for request in [
        edit("toolu_01A", "src/lib.rs"),
        edit("toolu_01B", "README.md"),
    ] {
        let card = card(&request);
        store.add(request, card, 1_761_743_990_000).unwrap();
    }
    store
}

#[test]
fn a_tap_records_the_guess_and_its_verdict_and_only_the_first_counts() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    store.guess("toolu_01A", 2, 1_761_744_000_000).unwrap();
    // A second tap, once the reasons show, changes nothing.
    store.guess("toolu_01A", 0, 1_761_744_009_000).unwrap();
    store.guess("toolu_01B", 0, 1_761_744_010_000).unwrap();

    let reopened = CardStore::open(path).unwrap();

    let guesses: Vec<Option<Guess>> = reopened.cards().iter().map(|kept| kept.guess).collect();
    assert_eq!(
        guesses,
        [
            Some(Guess {
                option: 2,
                right: false,
                at: 1_761_744_000_000
            }),
            Some(Guess {
                option: 0,
                right: true,
                at: 1_761_744_010_000
            }),
        ]
    );
    assert_eq!(reopened.cards(), store.cards());
}

#[test]
fn a_tap_on_no_option_of_the_quiz_records_nothing() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = CardStore::open(path.clone()).unwrap();
    let request = edit("toolu_01A", "src/lib.rs");
    let no_quiz = Card {
        quiz: None,
        ..card(&request)
    };
    store.add(request, no_quiz, 1_761_743_990_000).unwrap();
    let request = edit("toolu_01B", "README.md");
    let card = card(&request);
    store.add(request, card, 1_761_743_990_000).unwrap();

    store.guess("toolu_01A", 0, 1_761_744_000_000).unwrap();
    store.guess("toolu_01B", 3, 1_761_744_000_000).unwrap();

    let reopened = CardStore::open(path).unwrap();
    assert!(reopened.cards().iter().all(|kept| kept.guess.is_none()));
}

#[test]
fn the_key_is_wrong_is_recorded_on_the_card() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    store.guess("toolu_01B", 1, 1_761_744_000_000).unwrap();

    store.key_wrong("toolu_01B").unwrap();

    let reopened = CardStore::open(path).unwrap();
    let flags: Vec<bool> = reopened.cards().iter().map(|kept| kept.key_wrong).collect();
    assert_eq!(flags, [false, true]);
}

#[test]
fn a_rating_with_its_reasons_survives_a_restart_beside_the_one_key_is_wrong_flag() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    // As the pane sends it.
    let poor = r#"{"level": "poor", "reasons": ["unclear", "too_shallow"]}"#;
    store
        .rate("toolu_01A", serde_json::from_str(poor).unwrap())
        .unwrap();
    // "The key is wrong" in the rating is the quiz's flag.
    store.key_wrong("toolu_01A").unwrap();
    // Another level replaces the first.
    let good = Rating {
        level: Level::Good,
        reasons: vec![],
    };
    store.rate("toolu_01B", good).unwrap();
    let fair = Rating {
        level: Level::Fair,
        reasons: vec![Reason::TooEasy],
    };
    store.rate("toolu_01B", fair.clone()).unwrap();

    let reopened = CardStore::open(path).unwrap();

    let records: Vec<_> = reopened
        .cards()
        .iter()
        .map(|kept| (kept.rating.clone(), kept.key_wrong))
        .collect();
    let poor = Rating {
        level: Level::Poor,
        reasons: vec![Reason::Unclear, Reason::TooShallow],
    };
    assert_eq!(records, [(Some(poor), true), (Some(fair), false)]);
}

/// The fake writer, counting its calls for "explain more".
#[derive(Default)]
struct Counting(Cell<usize>);

impl CardWriter for Counting {
    fn reply(&self, request: &CardRequest) -> Result<String, String> {
        FakeWriter.reply(request)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        self.0.set(self.0.get() + 1);
        FakeWriter.more(request, card)
    }
}

#[test]
fn explain_more_calls_the_writer_once_and_a_second_tap_shows_the_kept_text() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let store = Mutex::new(two_cards(&path));
    let writer = Counting::default();
    let request = edit("toolu_01A", "src/lib.rs");
    let expected = FakeWriter.more(&request, &card(&request)).unwrap();

    let first = explain_more(&store, &writer, "toolu_01A", |line| panic!("{line}"));
    let second = explain_more(&store, &writer, "toolu_01A", |line| panic!("{line}"));

    assert_eq!(writer.0.get(), 1);
    assert_eq!(first.as_ref(), Some(&expected));
    assert_eq!(second.as_ref(), Some(&expected));
    let reopened = CardStore::open(path).unwrap();
    let more: Vec<_> = reopened
        .cards()
        .iter()
        .map(|kept| kept.more.clone())
        .collect();
    assert_eq!(more, [Some(expected), None]);
}

/// A fake writer whose "explain more" replies with no text.
struct Blank;

impl CardWriter for Blank {
    fn reply(&self, request: &CardRequest) -> Result<String, String> {
        FakeWriter.reply(request)
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Ok("\n".into())
    }
}

#[test]
fn explain_more_with_no_text_logs_one_line_and_keeps_nothing() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let store = Mutex::new(two_cards(&path));
    let mut lines = Vec::new();

    let more = explain_more(&store, &Blank, "toolu_01A", |line| lines.push(line));

    assert_eq!(more, None);
    let [line] = &lines[..] else {
        panic!("one log line: {lines:?}");
    };
    assert!(line.contains("toolu_01A"), "{line}");
    let reopened = CardStore::open(path).unwrap();
    assert!(reopened.cards().iter().all(|kept| kept.more.is_none()));
}

#[test]
fn each_card_records_when_it_was_added_when_its_quiz_showed_and_when_it_was_guessed() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    store.show("toolu_01A", 1_761_744_000_000).unwrap();
    // The pane draws it again after a restart; the first time stays.
    store.show("toolu_01A", 1_761_744_050_000).unwrap();
    store.guess("toolu_01A", 0, 1_761_744_004_000).unwrap();
    // A card guessed before its quiz time was kept gets none, so a guess never comes
    // before its quiz.
    store.guess("toolu_01B", 0, 1_761_744_060_000).unwrap();
    store.show("toolu_01B", 1_761_744_070_000).unwrap();

    let reopened = CardStore::open(path).unwrap();

    let records: Vec<_> = reopened
        .cards()
        .iter()
        .map(|kept| (kept.added_at, kept.shown_at, kept.guess.map(|a| a.at)))
        .collect();
    assert_eq!(
        records,
        [
            (
                Some(1_761_743_990_000),
                Some(1_761_744_000_000),
                Some(1_761_744_004_000)
            ),
            (Some(1_761_743_990_000), None, Some(1_761_744_060_000)),
        ]
    );
}

/// A store written by #374's build, before cards had a quiz.
const STORE_374: &str = r#"[{"request": {"session_id": "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092",
    "id": "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092@1761743990000",
    "base": {"head": null, "files": []}, "change": "-old\n+new\n", "check": null},
    "card": {"question": "למה?", "answer": "כי.", "paths": ["src/lib.rs"], "code": "+new",
    "why_it_matters": "", "not_checked": "", "passed": null},
    "added_at": 1761743990000, "shown_at": null, "revealed_at": 1761744000000,
    "passed": null, "outcome": null}]"#;

#[test]
fn a_store_from_before_quizzes_opens_with_cards_that_ask_nothing() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards-v2.json");
    std::fs::write(&path, STORE_374).unwrap();

    let store = CardStore::open(path).unwrap();

    let [kept] = store.cards() else {
        panic!("one card");
    };
    assert_eq!(
        (kept.card.explanation.as_str(), &kept.card.quiz),
        ("כי.", &None)
    );
    assert_eq!(kept.guess, None);
}

/// A store from before #374, with a card on one step.
const STORE_V1: &str = r#"[{"request": {"session_id": "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092",
    "tool_use_id": "toolu_01A", "step": {"tool": "Edit", "detail": "src/lib.rs"},
    "change": "-old\n+new\n"},
    "card": {"question": "למה?", "answer": "כי.", "paths": ["src/lib.rs"], "code": "+new",
    "why_it_matters": "", "not_checked": ""}, "revealed_at": 1761744000000}]"#;

#[test]
fn the_old_store_is_moved_aside_on_first_start_and_the_new_one_starts_empty() {
    let data = tempfile::tempdir().unwrap();
    let log = data.path().join("events.jsonl");
    std::fs::write(data.path().join("cards.json"), STORE_V1).unwrap();

    let mut store = CardStore::open_beside(&log).unwrap();

    assert_eq!(store.cards(), []);
    let v1 = data.path().join("cards-v1.json");
    assert_eq!(std::fs::read_to_string(&v1).unwrap(), STORE_V1);
    assert!(!data.path().join("cards.json").exists());
    // The next start keeps the new store, and the old one stays aside.
    let request = edit("toolu_01A", "src/lib.rs");
    let card = card(&request);
    store.add(request, card, 1_761_743_990_000).unwrap();
    assert_eq!(CardStore::open_beside(&log).unwrap().cards().len(), 1);
    assert_eq!(std::fs::read_to_string(&v1).unwrap(), STORE_V1);
}

#[test]
fn a_store_set_aside_before_is_never_overwritten() {
    let data = tempfile::tempdir().unwrap();
    let log = data.path().join("events.jsonl");
    std::fs::write(data.path().join("cards-v1.json"), STORE_V1).unwrap();
    // An older build of the app, run since, wrote a store of its own.
    std::fs::write(data.path().join("cards.json"), "[]").unwrap();

    CardStore::open_beside(&log).unwrap();

    let v1 = data.path().join("cards-v1.json");
    assert_eq!(std::fs::read_to_string(&v1).unwrap(), STORE_V1);
}

#[test]
fn a_missing_store_starts_empty() {
    let data = tempfile::tempdir().unwrap();

    let store = CardStore::open(data.path().join("cards.json")).unwrap();

    assert_eq!(store.cards(), []);
}

const MINUTE: u64 = 60 * 1000;

fn ids(cards: &[StoredCard]) -> Vec<&str> {
    cards.iter().map(|kept| kept.request.id.as_str()).collect()
}

#[test]
fn a_sessions_cards_split_at_30_minutes_before_the_tick() {
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    let start = 1_761_743_990_000;
    for (i, request) in [
        edit("toolu_01A", "src/lib.rs"),
        edit("toolu_01B", "README.md"),
    ]
    .into_iter()
    .enumerate()
    {
        let card = card(&request);
        store
            .add(request, card, start + i as u64 * 10 * MINUTE)
            .unwrap();
    }
    let session = "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092";

    let before = store.session_cards(session, start + 30 * MINUTE - 1);
    assert_eq!(ids(&before.recent), ["toolu_01A", "toolu_01B"]);
    assert_eq!(ids(&before.older), [] as [&str; 0]);
    let at = store.session_cards(session, start + 30 * MINUTE);
    assert_eq!(ids(&at.recent), ["toolu_01B"]);
    assert_eq!(ids(&at.older), ["toolu_01A"]);
}

#[test]
fn a_card_from_before_its_time_was_kept_is_older() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let request = edit("toolu_01A", "src/lib.rs");
    let card = card(&request);
    let kept = serde_json::json!([{ "request": request, "card": card }]);
    std::fs::write(&path, kept.to_string()).unwrap();

    let store = CardStore::open(path).unwrap();

    let shown = store.session_cards("5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092", 1_761_743_990_000);
    assert_eq!(ids(&shown.older), ["toolu_01A"]);
}

/// #347's checkpoint day (#370's fixture), live, with a change at every fingerprint: each card
/// request gets a card at its event's time. Before #371 the pane showed only the cards of the
/// session prompted last, so the other session's cards seemed deleted (#347 note, "Cards were not
/// deleted").
#[test]
fn every_session_of_the_day_shows_all_of_its_cards_whichever_was_prompted_last() {
    let day = include_str!("../../core/tests/fixtures/checkpoint-day.jsonl");
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    let mut studio = Studio::default();
    let changed = Change {
        base: Base::default(),
        diff: "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-old\n+new\n".into(),
    };
    for line in day.lines() {
        let at = serde_json::from_str::<serde_json::Value>(line).unwrap()["studio_received_ms"]
            .as_u64()
            .unwrap();
        studio.tick(at);
        let change = studio.fingerprint_folder(line).and(Some(changed.clone()));
        for output in studio.apply_with_change(line, change) {
            if let Output::Card(request) = output {
                let card = card(&request);
                store.add(request, card, at).unwrap();
            }
        }
    }
    let sessions: Vec<String> = studio.sessions().iter().map(|s| s.id.clone()).collect();

    let shown: Vec<(&str, usize)> = sessions
        .iter()
        .map(|id| {
            studio.select(id);
            let selected = studio.selected().unwrap();
            let cards = store.session_cards(&selected.id, studio.now());
            (&id[..8], cards.recent.len() + cards.older.len())
        })
        .collect();

    // In list order, one card per `Stop`; the two sessions with no prompt have none.
    assert_eq!(
        shown,
        [
            ("0222b62d", 4),
            ("596c28a1", 7),
            ("2afc85f5", 14),
            ("5bb84a4a", 4),
            ("57e9c1ab", 0),
            ("6641e809", 0),
            ("1ac172be", 7),
            ("5d7f69ab", 14),
        ]
    );
    assert_eq!(store.cards().len(), 50);
}
