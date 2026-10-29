//! The card store (#328, module 6): cards and their reveal state survive a restart.

use studio_core::{CardRequest, Check, Step};
use studio_understanding::{CardStore, CardWriter, FakeWriter, Outcome};

fn edit(tool_use_id: &str, path: &str) -> CardRequest {
    CardRequest {
        session_id: "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092".into(),
        tool_use_id: tool_use_id.into(),
        step: Step {
            tool: "Edit".into(),
            detail: Some(path.into()),
        },
        change: "@@ -1,1 +1,1 @@\n-old\n+new\n".into(),
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
        .map(|kept| (kept.request.tool_use_id.as_str(), kept.revealed_at))
        .collect();
    assert_eq!(
        revealed,
        [("toolu_01A", Some(1_761_744_000_000)), ("toolu_01B", None)]
    );
    assert_eq!(reopened.cards(), store.cards());
}

fn check(guess: Option<bool>, exit_passed: Option<bool>) -> CardRequest {
    CardRequest {
        session_id: "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18".into(),
        tool_use_id: "toolu_01Uy4Ka9Lo3Pv6Bs1Dc8Fh5R".into(),
        step: Step {
            tool: "Bash".into(),
            detail: Some("cargo test 2>&1 | tail -5".into()),
        },
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

#[test]
fn a_store_written_before_checks_existed_still_opens() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let before = r#"[{"request": {"session_id": "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092",
        "tool_use_id": "toolu_01A", "step": {"tool": "Edit", "detail": "src/lib.rs"},
        "change": "-old\n+new\n"},
        "card": {"question": "למה?", "answer": "כי.", "paths": ["src/lib.rs"], "code": "+new",
        "why_it_matters": "", "not_checked": ""}, "revealed_at": 1761744000000}]"#;
    std::fs::write(&path, before).unwrap();

    let store = CardStore::open(path).unwrap();

    let kept = &store.cards()[0];
    assert_eq!(kept.request.check, None);
    assert_eq!(
        (kept.card.passed, kept.passed, kept.outcome),
        (None, None, None)
    );
    assert_eq!(kept.revealed_at, Some(1_761_744_000_000));
    assert_eq!((kept.added_at, kept.shown_at), (None, None));
}

#[test]
fn a_missing_store_starts_empty() {
    let data = tempfile::tempdir().unwrap();

    let store = CardStore::open(data.path().join("cards.json")).unwrap();

    assert_eq!(store.cards(), []);
}
