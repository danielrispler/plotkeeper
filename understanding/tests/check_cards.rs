//! The card while a check runs (danielrispler/Falinks#461, decision 15): the request a long check's start makes reaches
//! the writer with the change so far and the check's command, the card's guess is whether the check
//! passes, and the check's result settles it.

use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, sync::Mutex};
use studio_core::{Base, CardRequest, Change, Output, Purpose, Studio};
use studio_understanding::{
    Card, CardStore, CardWriter, ClaudeWriter, FakeWriter, Rate, week, write_card, write_stop,
};

const CHECKS: &str = include_str!("../../core/tests/fixtures/checks.jsonl");
const SESSION: &str = "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18";
const COMMAND: &str = "cd apps/studio && cargo test";
const DIFF: &str = "--- a/ui/style.css\n+++ b/ui/style.css\n@@ -1 +1 @@\n-a\n+b\n";

/// The fake writer, keeping each request it was asked to write a card on.
#[derive(Default)]
struct Recording(RefCell<Vec<CardRequest>>);

impl CardWriter for Recording {
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        self.0.borrow_mut().push(request.clone());
        FakeWriter.reply(request, earlier, brief)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }

    fn rewrite(&self, p: &[&str], s: &[String], d: &str) -> Result<String, String> {
        FakeWriter.rewrite(p, s, d)
    }

    fn update(&self, brief: &str, workers: &str) -> Result<String, String> {
        FakeWriter.update(brief, workers)
    }
}

/// `checks.jsonl` with its first check's result 200 s late, so the second, which passes, usually
/// takes over 2 minutes.
fn long_check_log() -> Vec<String> {
    let lines = CHECKS.lines().enumerate().map(|(index, line)| {
        let mut event: Value = serde_json::from_str(line).unwrap();
        let at = event["studio_received_ms"].as_u64().unwrap();
        event["studio_received_ms"] = (at + if index >= 3 { 200_000 } else { 0 }).into();
        event.to_string()
    });
    lines.collect()
}

/// Every worktree `line` looks at changed by `DIFF`, but at a base.
fn changed(studio: &Studio, line: &str) -> BTreeMap<String, Change> {
    let looks = studio.fingerprint_folders(line).into_iter();
    looks
        .filter(|look| look.purpose != Purpose::Base)
        .map(|look| {
            let change = Change {
                base: Base::default(),
                diff: DIFF.into(),
                head: None,
            };
            (look.key, change)
        })
        .collect()
}

#[test]
fn a_long_checks_card_request_reaches_the_writer_with_the_change_so_far_and_the_command() {
    let data = tempfile::tempdir().unwrap();
    let store = Mutex::new(CardStore::open(data.path().join("cards.json")).unwrap());
    let writer = Recording::default();
    let mut studio = Studio::default();
    for line in long_check_log() {
        let changes = changed(&studio, &line);
        for output in studio.apply_with_changes(&line, changes) {
            if let Output::Card(request) = output
                && request.check.is_some()
            {
                let session = studio.session(SESSION).unwrap().clone();
                let log = |line| panic!("{line}");
                assert!(write_stop(&store, &writer, request, &session, || {}, log));
            }
        }
    }

    let asked = writer.0.borrow();
    let [request] = &asked[..] else {
        panic!("one check card: {asked:?}");
    };
    assert_eq!(request.check.as_deref(), Some(COMMAND));
    assert_eq!(request.diff(), DIFF);
    let store = store.lock().unwrap();
    let quiz = store.cards()[0].card.quiz.clone().unwrap();
    assert_eq!(quiz.text, format!("Will `{COMMAND}` pass?"));
}

/// A check card on `DIFF`, named `id`.
fn check_request(id: &str) -> CardRequest {
    CardRequest {
        session_id: SESSION.into(),
        id: id.into(),
        change: Some(DIFF.into()),
        check: Some(COMMAND.into()),
        ..CardRequest::default()
    }
}

#[test]
fn the_writers_prompt_holds_the_checks_command_and_the_change() {
    let folder = tempfile::tempdir().unwrap();
    let writer = ClaudeWriter::new("/opt/homebrew/bin/claude".into(), folder.path().into());

    let (_, prompt) = writer.command(&check_request("a@1"), &[], "");
    let (_, plain) = writer.command(&edit_request(), &[], "");

    assert!(
        prompt.contains(&format!("The check's command:\n{COMMAND}\n")),
        "{prompt}"
    );
    assert!(prompt.contains(DIFF), "{prompt}");
    assert!(prompt.contains("if_it_passes"), "{prompt}");
    assert!(!plain.contains("The check's command"), "{plain}");
}

/// A card on `DIFF` that no check requested.
fn edit_request() -> CardRequest {
    CardRequest {
        check: None,
        ..check_request("a@2")
    }
}

/// A store with two check cards, `a@1` and `a@2`, and one card on an edit, `a@3`.
fn store(path: &std::path::Path) -> CardStore {
    let mut store = CardStore::open(path.into()).unwrap();
    let edit = CardRequest {
        id: "a@3".into(),
        ..edit_request()
    };
    for request in [check_request("a@1"), check_request("a@2"), edit] {
        let card = write_card(&FakeWriter, &request, &[], "", |l| panic!("{l}"))
            .unwrap()
            .0;
        store.add(request, card, 1_793_264_000_000).unwrap();
    }
    store
}

#[test]
fn the_checks_result_settles_a_guess_made_while_it_ran() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = store(&path);
    // "It fails" on the first check, "It passes" on the second, while they run.
    store.guess("a@1", 1, 1_793_264_010_000).unwrap();
    store.guess("a@2", 0, 1_793_264_011_000).unwrap();
    // A running check settles nothing.
    assert_eq!(store.cards()[0].check_result, None);
    assert_eq!(store.cards()[0].guess.unwrap().right, None);

    store.settle("a@1", Some(false)).unwrap();
    store.settle("a@2", Some(false)).unwrap();
    // The edit's card was no check's, so its guess is the writer's key's to grade.
    store.settle("a@3", Some(true)).unwrap();

    let reopened = CardStore::open(path).unwrap();
    let settled: Vec<_> = reopened
        .cards()
        .iter()
        .map(|kept| (kept.check_result, kept.guess.map(|g| g.right)))
        .collect();
    use studio_understanding::CheckResult::Failed;
    assert_eq!(
        settled,
        [
            (Some(Failed), Some(Some(true))),
            (Some(Failed), Some(Some(false))),
            (None, None)
        ]
    );
    // The fails option is the key now, so the card shows why each option is right or wrong.
    assert_eq!(reopened.cards()[0].card.quiz.as_ref().unwrap().key, 1);
    let week = week(reopened.cards(), 1_793_264_100_000);
    assert_eq!(week.surprise, Rate { count: 1, of: 2 });
}

#[test]
fn a_tap_after_the_result_or_on_a_check_whose_exit_code_does_not_count_is_no_guess() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = store(&path);
    store.guess("a@2", 0, 1_793_264_011_000).unwrap();
    store.settle("a@1", Some(true)).unwrap();
    store.settle("a@2", None).unwrap();
    // A tap after the result counts for nothing.
    store.guess("a@1", 0, 1_793_264_020_000).unwrap();

    let reopened = CardStore::open(path).unwrap();
    use studio_understanding::CheckResult::{NotCounted, Passed};
    let cards = reopened.cards();
    assert_eq!(
        (cards[0].check_result, cards[0].guess),
        (Some(Passed), None)
    );
    assert_eq!(cards[1].check_result, Some(NotCounted));
    // Its tap stays, but the weekly rates leave it out.
    assert!(cards[1].guess.is_some());
    let week = week(cards, 1_793_264_100_000);
    assert_eq!(week.surprise, Rate { count: 0, of: 0 });
}

#[test]
fn a_card_stored_after_its_check_ended_settles_at_once_so_a_later_tap_is_no_guess() {
    let mut studio = Studio::default();
    let mut requests = Vec::new();
    for line in long_check_log() {
        let changes = changed(&studio, &line);
        for output in studio.apply_with_changes(&line, changes) {
            if let Output::Card(request) = output
                && request.check.is_some()
            {
                requests.push(request);
            }
        }
    }
    // The writer is slow: the check ended before its card is stored.
    let [request] = &requests[..] else {
        panic!("one check card: {requests:?}");
    };
    let data = tempfile::tempdir().unwrap();
    let store = Mutex::new(CardStore::open(data.path().join("cards.json")).unwrap());
    let session = studio.session(SESSION).unwrap();
    let log = |line| panic!("{line}");
    assert!(write_stop(
        &store,
        &FakeWriter,
        request.clone(),
        session,
        || {},
        log
    ));
    let mut store = store.lock().unwrap();

    store.settle_checks(session).unwrap();
    store.guess(&request.id, 0, 1_793_264_500_000).unwrap();

    use studio_understanding::CheckResult::Passed;
    let kept = &store.cards()[0];
    assert_eq!((kept.check_result, kept.guess), (Some(Passed), None));
}
