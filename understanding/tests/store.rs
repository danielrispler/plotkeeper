//! The card store (#328, module 6; #369, decision 17): cards, their base and their reveal state
//! survive a restart.

use studio_core::{Base, CardRequest, Change, Check, Output, Studio};
use studio_understanding::{CardStore, CardWriter, FakeWriter, Outcome, StoredCard};

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
        check: None,
    }
}

#[test]
fn a_reopened_store_keeps_the_cards_and_which_ones_daniel_revealed() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = CardStore::open(path.clone()).unwrap();
    for request in [
        edit("toolu_01A", "src/lib.rs"),
        edit("toolu_01B", "README.md"),
    ] {
        let card = FakeWriter.write(&request).unwrap();
        store.add(request, card, 1_761_743_990_000).unwrap();
    }
    store.reveal("toolu_01A", 1_761_744_000_000).unwrap();
    // Opening it again later keeps the first reveal.
    store.reveal("toolu_01A", 1_761_744_009_000).unwrap();

    let reopened = CardStore::open(path).unwrap();

    let revealed: Vec<(&str, Option<u64>)> = reopened
        .cards()
        .iter()
        .map(|kept| (kept.request.id.as_str(), kept.revealed_at))
        .collect();
    assert_eq!(
        revealed,
        [("toolu_01A", Some(1_761_744_000_000)), ("toolu_01B", None)]
    );
    assert_eq!(reopened.cards(), store.cards());
}

/// A check's card, as stores kept them before #374.
fn check(guess: Option<bool>, exit_passed: Option<bool>) -> CardRequest {
    CardRequest {
        session_id: "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18".into(),
        id: "toolu_01Uy4Ka9Lo3Pv6Bs1Dc8Fh5R".into(),
        base: Base::default(),
        change: "test result: FAILED. 11 passed; 1 failed; 0 ignored".into(),
        check: Some(Check { guess, exit_passed }),
    }
}

/// The stored card's verdict and outcome once the card writer says whether the check `passed`.
fn settled(request: CardRequest, passed: Option<bool>) -> (Option<bool>, Option<Outcome>) {
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    let mut card = FakeWriter.write(&request).unwrap();
    card.passed = passed;
    store.add(request, card, 1_761_743_990_000).unwrap();
    let reopened = CardStore::open(data.path().join("cards.json")).unwrap();
    (reopened.cards()[0].passed, reopened.cards()[0].outcome)
}

#[test]
fn the_writers_passed_settles_a_yes_on_a_piped_check_as_right_wrong_or_not_counted() {
    let yes = || check(Some(true), None);

    assert_eq!(
        settled(yes(), Some(true)),
        (Some(true), Some(Outcome::Right))
    );
    assert_eq!(
        settled(yes(), Some(false)),
        (Some(false), Some(Outcome::Wrong))
    );
    assert_eq!(settled(yes(), None), (None, Some(Outcome::NotCounted)));
}

#[test]
fn an_exit_code_that_counts_settles_the_guess_and_a_missing_tap_is_not_counted() {
    let yes_but_exited_1 = check(Some(true), Some(false));
    assert_eq!(
        settled(yes_but_exited_1, Some(true)),
        (Some(false), Some(Outcome::Wrong))
    );
    let untapped = check(None, Some(true));
    assert_eq!(
        settled(untapped, Some(true)),
        (Some(true), Some(Outcome::NotCounted))
    );
    assert_eq!(settled(edit("toolu_01A", "src/lib.rs"), None), (None, None));
}

#[test]
fn each_card_records_when_it_was_added_when_its_question_showed_and_when_it_was_revealed() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = CardStore::open(path.clone()).unwrap();
    for request in [
        edit("toolu_01A", "src/lib.rs"),
        edit("toolu_01B", "README.md"),
    ] {
        let card = FakeWriter.write(&request).unwrap();
        store.add(request, card, 1_761_743_990_000).unwrap();
    }
    store.show("toolu_01A", 1_761_744_000_000).unwrap();
    // The pane draws it again after a restart; the first time stays.
    store.show("toolu_01A", 1_761_744_050_000).unwrap();
    store.reveal("toolu_01A", 1_761_744_004_000).unwrap();
    // A card revealed before its question time was kept, as in a store from before this record,
    // gets none, so a reveal never comes before its question.
    store.reveal("toolu_01B", 1_761_744_060_000).unwrap();
    store.show("toolu_01B", 1_761_744_070_000).unwrap();

    let reopened = CardStore::open(path).unwrap();

    let records: Vec<_> = reopened
        .cards()
        .iter()
        .map(|kept| (kept.added_at, kept.shown_at, kept.revealed_at))
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
    let card = FakeWriter.write(&request).unwrap();
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
        let card = FakeWriter.write(&request).unwrap();
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
    let card = FakeWriter.write(&request).unwrap();
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
                let card = FakeWriter.write(&request).unwrap();
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
