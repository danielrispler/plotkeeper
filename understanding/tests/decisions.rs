//! The decision history in the card store (danielrispler/Falinks#461, decision 17): it survives a restart, and one that
//! does not parse stops the app as a card store that does not parse does (danielrispler/Falinks#345).

use studio_core::{Decision, Studio};
use studio_understanding::CardStore;

const PROBE: &str = "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31";

/// The decisions of the denied-permission session, replayed through the core.
fn denied() -> Vec<Decision> {
    let lines = include_str!("../../core/tests/fixtures/permission-denied.jsonl").lines();
    let studio = Studio::replayed(lines);
    studio.session(PROBE).unwrap().decisions.clone()
}

#[test]
fn the_history_survives_a_restart_and_keeping_it_again_adds_nothing() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards-v2.json");
    let decisions = denied();
    assert_eq!(decisions.len(), 1);
    let mut store = CardStore::open(path.clone()).unwrap();
    store.keep_decisions(PROBE, &decisions).unwrap();
    let revision = store.revision();
    // The shell keeps every session's decisions after each read of the log.
    store.keep_decisions(PROBE, &decisions).unwrap();
    assert_eq!(store.revision(), revision);

    let reopened = CardStore::open(path).unwrap();
    assert_eq!(reopened.session_cards(PROBE, 0).decisions, decisions);
    assert!(reopened.session_cards("another", 0).decisions.is_empty());
}

#[test]
fn a_history_that_does_not_parse_stops_the_store_and_is_never_overwritten() {
    let data = tempfile::tempdir().unwrap();
    let kept = data.path().join("decisions.json");
    std::fs::write(&kept, "{ half a file").unwrap();

    assert!(CardStore::open(data.path().join("cards-v2.json")).is_err());
    assert_eq!(std::fs::read_to_string(kept).unwrap(), "{ half a file");
}
