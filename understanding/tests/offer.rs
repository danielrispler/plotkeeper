//! What a turn gets (danielrispler/plotkeeper#119): a card or nothing, with canned writer replies. The lone Quiz and the hourly cap
//! are retired (danielrispler/plotkeeper#347).

use crate::common::{Answer, Scripted};
use serde_json::json;
use std::sync::Mutex;
use studio_core::{
    CardRequest, Check, CheckRun, CheckStatus, FactFile, Facts, SummaryRequest, TurnFile,
};
use studio_understanding::{
    CardStore, CardWriter, FakeWriter, Offer, OfferKind, Offers, Shape, TurnFacts, changed_in,
    decide, offer_stop, settle, turns_card,
};

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

// The table's edges, from a turn's recorded facts.

fn facts(files: &[(&str, u64)], check: Option<CheckStatus>) -> Facts {
    let file = |(path, lines): &(&str, u64)| FactFile {
        file: TurnFile {
            root: "/r".into(),
            path: (*path).into(),
            from: None,
            added: Some(*lines),
            removed: Some(0),
            agent: None,
        },
        hunk: format!(
            "--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n{}",
            "+let x = 1;\n".repeat(*lines as usize)
        ),
    };
    let check = check.map(|status| Check {
        suite: "cargo test".into(),
        worktree: String::new(),
        foreign: false,
        status,
        retries: 0,
        runs: Vec::<CheckRun>::new(),
    });
    Facts {
        files: files.iter().map(file).collect(),
        checks: check.into_iter().collect(),
        ..Facts::default()
    }
}

fn offer(files: &[(&str, u64)], check: Option<CheckStatus>, merge: bool) -> Offer {
    Shape::of(&facts(files, check), "", merge).offer()
}

#[test]
fn the_table_holds_at_each_edge() {
    // Below the card row a turn gets nothing: no lone Quiz at 8 or more non-trivial lines.
    assert_eq!(offer(&[("a.rs", 24)], None, false), Offer::Nothing);
    assert_eq!(offer(&[("a.rs", 25)], None, false), Offer::Card);
    assert_eq!(offer(&[("a.rs", 9)], None, false), Offer::Nothing);
    assert_eq!(offer(&[("a.rs", 9), ("b.rs", 1)], None, false), Offer::Card);
    assert_eq!(offer(&[("a.rs", 7)], None, false), Offer::Nothing);
    assert_eq!(offer(&[("a.rs", 8)], None, false), Offer::Nothing);
    assert_eq!(
        offer(&[("a.rs", 30)], Some(CheckStatus::Failed), false),
        Offer::Card
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

// One offer function, and its record (danielrispler/plotkeeper#209).

fn turn_facts(files: &[(&str, u64)], check: Option<CheckStatus>) -> TurnFacts {
    TurnFacts {
        facts: facts(files, check),
        merge: false,
        workers: Vec::new(),
        traced: true,
    }
}

/// The turn of `turn_facts`, with its files in `/w` the home of a worker.
fn with_worker(files: &[(&str, u64)], own: &[(&str, u64)]) -> TurnFacts {
    let mut turn = turn_facts(files, None);
    let mut worker = facts(own, None).files;
    worker.iter_mut().for_each(|f| f.file.root = "/w".into());
    turn.facts.files.extend(worker);
    turn.workers = vec!["/w".into()];
    turn
}

#[test]
fn a_turn_is_offered_what_changed_in_its_own_worktrees_and_not_its_workers() {
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(1)
    };
    let only = with_worker(&[], &[("w.rs", 200)]);
    let settled = settle(&mut request, &only, &[], None);
    assert_eq!(settled.decision.offer, Offer::Nothing);
    assert_eq!(settled.decision.inputs.logic_lines, 0);
    // The evidence line keeps the worker's file.
    assert_eq!(only.facts.files.len(), 1);

    let both = with_worker(&[("a.rs", 30)], &[("w.rs", 200)]);
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(2)
    };
    let settled = settle(&mut request, &both, &[], None);
    assert_eq!(settled.decision.offer, Offer::Card);
    assert_eq!(settled.decision.inputs.logic_lines, 30);
    assert_eq!(both.facts.files.len(), 2);
}

fn diff_of(lines: usize) -> String {
    format!(
        "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1,{lines} @@\n{}",
        "+let x = 1;\n".repeat(lines)
    )
}

fn card_request(id: &str, agent: Option<&str>, diff: &str) -> CardRequest {
    CardRequest {
        session_id: "s".into(),
        id: id.into(),
        agent: agent.map(Into::into),
        change: Some(diff.into()),
        ..CardRequest::default()
    }
}

fn lines_of(offers: &Offers) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(offers.path()).unwrap_or_default();
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn offers() -> (tempfile::TempDir, Offers) {
    let dir = tempfile::tempdir().unwrap();
    let offers = Offers::beside(&dir.path().join("events.jsonl"));
    (dir, offers)
}

fn stop_of(
    store: &Mutex<CardStore>,
    offers: &Offers,
    request: CardRequest,
    turn: &TurnFacts,
) -> bool {
    let session = studio_core::Session::default();
    offer_stop(
        store,
        &FakeWriter,
        request,
        &session,
        turn,
        offers,
        || {},
        |_| {},
    )
    .0
}

#[test]
fn the_card_path_and_the_summary_path_agree_on_the_same_facts() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let turn = turn_facts(&[("a.rs", 30)], None);
    // The card's own diff is empty: it is never what the turn is judged on.
    assert!(stop_of(
        &store,
        &offers,
        card_request("s@1", None, ""),
        &turn
    ));
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(1)
    };
    let settled = settle(&mut request, &turn, &[], None);
    assert_eq!(settled.decision.offer, Offer::Card);
    assert!(!settled.write);
}

fn turn_request(n: u64) -> SummaryRequest {
    turn(n)
}

#[test]
fn an_agent_with_no_trees_is_judged_on_its_cards_diff_by_both_paths() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let none = TurnFacts::default();
    let diff = diff_of(30);
    assert!(stop_of(
        &store,
        &offers,
        card_request("s@1", None, &diff),
        &none
    ));
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(2)
    };
    assert_eq!(
        settle(&mut request, &none, &changed_in(&diff), None)
            .decision
            .offer,
        Offer::Card
    );
    let small = diff_of(10);
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(3)
    };
    let settled = settle(&mut request, &none, &changed_in(&small), None);
    assert_eq!(settled.decision.offer, Offer::Nothing);
    assert!(!settled.write);
}

#[test]
fn a_workers_card_passes_the_first_row_or_is_not_written() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let none = TurnFacts::default();
    let big = card_request("w@1", Some("w"), &diff_of(30));
    assert!(stop_of(&store, &offers, big, &none));
    let small = card_request("w@2", Some("w"), &diff_of(10));
    assert!(!stop_of(&store, &offers, small, &none));
    let kept: Vec<_> = store
        .lock()
        .unwrap()
        .cards()
        .iter()
        .map(|k| k.request.id.clone())
        .collect();
    assert_eq!(kept, ["w@1"]);
    let lines = lines_of(&offers);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["kind"], "worker");
    assert_eq!(lines[0]["offer"], "card");
    assert_eq!(lines[0]["outcome"], "written");
    assert_eq!(lines[1]["offer"], "nothing");
    assert_eq!(lines[1]["reason"], "below the card row");
    assert_eq!(lines[1]["inputs"]["logic_lines"], 10);
}

#[test]
fn a_check_card_is_written_and_leaves_no_line() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let request = CardRequest {
        check: Some("cargo test".into()),
        ..card_request("s@1", None, "")
    };
    assert!(stop_of(&store, &offers, request, &TurnFacts::default()));
    assert!(lines_of(&offers).is_empty());
}

#[test]
fn each_decision_is_a_line_with_its_inputs_and_its_outcome_and_no_text() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let turn = turn_facts(&[("a.rs", 30), ("README.md", 4)], Some(CheckStatus::Failed));
    assert!(stop_of(
        &store,
        &offers,
        card_request("s@1", None, ""),
        &turn
    ));
    let refused = turn_facts(&[("README.md", 40)], None);
    assert!(!stop_of(
        &store,
        &offers,
        card_request("s@2", None, ""),
        &refused
    ));
    let lines = lines_of(&offers);
    assert_eq!(lines.len(), 2);
    let first = &lines[0];
    assert_eq!(first["session"], "s");
    assert_eq!(first["id"], "s@1");
    assert_eq!(first["kind"], "turn");
    assert!(first["at"].as_u64().unwrap() > 0);
    let inputs = &first["inputs"];
    assert_eq!(inputs["logic_lines"], 30);
    assert_eq!(inputs["logic_files"], 1);
    assert_eq!(inputs["non_trivial"], 30);
    assert_eq!(inputs["failed"], true);
    assert_eq!(inputs["merge"], false);
    assert_eq!(inputs["test_run"], false);
    assert_eq!(inputs["docs"], false);
    assert_eq!(lines[1]["offer"], "nothing");
    assert_eq!(lines[1]["reason"], "no logic file");
    assert_eq!(lines[1]["inputs"]["docs"], true);
    let text = std::fs::read_to_string(offers.path()).unwrap();
    assert!(!text.contains("let x") && !text.contains("diff --git"));
}

/// A writer that gives no reply.
fn silent() -> Scripted {
    Scripted {
        reply: Answer::err("no reply"),
        more: Answer::Fake,
        ..Scripted::bare()
    }
}

#[test]
fn a_subagents_late_change_below_the_card_row_has_a_line_of_its_own_kind() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let mut request = card_request("s@9", None, &diff_of(10));
    request.changed_by = [("w".to_string(), ["a.rs".to_string()].into())].into();
    assert!(!stop_of(&store, &offers, request, &TurnFacts::default()));
    let lines = lines_of(&offers);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["kind"], "late_change");
}

#[test]
fn a_writer_that_gave_no_reply_is_asked_again_and_the_line_stays_one_per_outcome() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let turn = turn_facts(&[("a.rs", 30)], None);
    let session = studio_core::Session::default();
    let ask = |writer: &dyn CardWriter| {
        offer_stop(
            &store,
            writer,
            card_request("s@1", None, ""),
            &session,
            &turn,
            &offers,
            || {},
            |_| {},
        )
        .0
    };
    assert!(!ask(&silent()));
    assert_eq!(lines_of(&offers)[0]["outcome"], "no reply from the writer");
    assert!(!offers.settled("s", "s@1"));
    assert!(ask(&FakeWriter));
    assert!(offers.settled("s", "s@1"));
}

#[test]
fn the_summary_takes_the_decision_the_card_made_not_a_new_one() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    // The card path saw a small change and decided nothing.
    let at_card = turn_facts(&[("a.rs", 2)], None);
    let (_, decided) = {
        let session = studio_core::Session::default();
        offer_stop(
            &store,
            &FakeWriter,
            card_request("s@1", None, ""),
            &session,
            &at_card,
            &offers,
            || {},
            |_| {},
        )
    };
    let decided = decided.unwrap();
    assert_eq!(decided.offer, Offer::Nothing);
    // 20 seconds later the facts differ: a fresh decision would be a card.
    let later = turn_facts(&[("a.rs", 30)], None);
    let mut request = turn_request(1);
    let settled = settle(&mut request, &later, &[], Some(&decided));
    assert_eq!(settled.decision.offer, Offer::Nothing);
}

#[test]
fn a_workers_200_lines_are_offered_once_on_the_workers_card_and_not_again_on_the_turn() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    // The turn changed only what its worker changed, in the worker's worktree `/w`.
    let turn = with_worker(&[], &[("a.rs", 200)]);
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(1)
    };
    let settled = settle(&mut request, &turn, &[], None);
    assert_eq!(settled.decision.offer, Offer::Nothing);
    assert!(!settled.write);

    // The worker's own card is the one that passes the first row, on its diff: its id names no turn, so
    // the shell gives it no turn facts.
    let (written, decided) = {
        let session = studio_core::Session::default();
        offer_stop(
            &store,
            &FakeWriter,
            card_request("s@1", Some("w"), &diff_of(200)),
            &session,
            &TurnFacts::default(),
            &offers,
            || {},
            |_| {},
        )
    };
    assert!(written);
    assert_eq!(decided.unwrap().offer, Offer::Card);
    let lines = lines_of(&offers);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["kind"], "worker");
}

#[test]
fn a_worktree_the_session_edits_itself_counts_for_its_turn_when_no_worker_is_in_it() {
    let mut turn = turn_facts(&[], None);
    let mut own = facts(&[("a.rs", 30)], None).files;
    own.iter_mut().for_each(|f| f.file.root = "/w".into());
    turn.facts.files = own;
    // No worker has `/w` as its home, so its files are the turn's own.
    let mut request = SummaryRequest {
        wanted: false,
        ..turn_request(1)
    };
    assert_eq!(
        settle(&mut request, &turn, &[], None).decision.offer,
        Offer::Card
    );
}

// The diff a fingerprint really produces (danielrispler/plotkeeper#202).

#[test]
fn the_files_of_a_fingerprints_diff_are_read() {
    use std::process::Command;
    use studio_understanding::Fingerprints;
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=Studio",
                "-c",
                "user.email=studio@example.com",
            ])
            .args(args)
            .current_dir(dir.path())
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "Start"]);
    let mut prints = Fingerprints::default();
    assert!(prints.take(dir.path()).is_none());

    std::fs::write(dir.path().join("a.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    std::fs::write(dir.path().join("new.rs"), "fn d() {}\n").unwrap();
    let change = prints.take(dir.path()).unwrap();

    let files = changed_in(&change.diff);
    let found: Vec<(&str, u64)> = files.iter().map(|f| (f.path.as_str(), f.lines)).collect();
    assert_eq!(found, [("a.rs", 2), ("new.rs", 1)]);
}

#[test]
fn a_turn_that_changed_nothing_is_not_offered_the_change_of_a_card_in_a_shared_checkout() {
    let traced = TurnFacts {
        traced: true,
        ..TurnFacts::default()
    };
    let other = changed_in(&diff_of(30));
    assert_eq!(
        decide(OfferKind::Turn, &traced, &other).offer,
        Offer::Nothing
    );
    let untraced = TurnFacts::default();
    assert_eq!(
        decide(OfferKind::Turn, &untraced, &other).offer,
        Offer::Card
    );
}

#[test]
fn a_turns_own_card_with_a_helpers_edit_is_a_turn_decision() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let mut studio = studio_core::Studio::default();
    for (name, ms) in [("SessionStart", 1), ("UserPromptSubmit", 2), ("Stop", 9)] {
        let line = json!({ "session_id": "s", "hook_event_name": name, "prompt": "p", "studio_received_ms": ms });
        studio.replay(&line.to_string());
    }
    let session = studio.session("s").cloned().unwrap();
    assert_eq!(session.turns[0].id.as_deref(), Some("s@9"));
    let mut request = card_request("s@9", None, &diff_of(10));
    request.changed_by = [("w".to_string(), ["a.rs".to_string()].into())].into();
    let (_, decided) = offer_stop(
        &store,
        &FakeWriter,
        request,
        &session,
        &TurnFacts::default(),
        &offers,
        || {},
        |_| {},
    );
    let decided = decided.unwrap();
    assert_eq!(
        (decided.kind, decided.offer),
        (OfferKind::Turn, Offer::Nothing)
    );
}

/// A session whose turn `s@9` stopped, as the shell reads it.
fn stopped_session() -> studio_core::Session {
    let mut studio = studio_core::Studio::default();
    for (name, ms) in [("SessionStart", 1), ("UserPromptSubmit", 2), ("Stop", 9)] {
        let line = json!({ "session_id": "s", "hook_event_name": name, "prompt": "p", "studio_received_ms": ms });
        studio.replay(&line.to_string());
    }
    studio.session("s").cloned().unwrap()
}

#[test]
fn a_decision_the_summary_path_makes_alone_is_on_the_record_once() {
    let (_o, offers) = offers();
    for (n, files, offer) in [(1, 2, Offer::Nothing), (2, 30, Offer::Card)] {
        let turn = turn_facts(&[("a.rs", files)], None);
        let mut request = turn_request(n);
        let settled = settle(&mut request, &turn, &[], None);
        assert_eq!(settled.decision.offer, offer);
        for _ in 0..2 {
            settled.record(&offers, &request);
        }
        assert!(offers.settled("s", &format!("s@{n}")));
    }
    assert_eq!(lines_of(&offers).len(), 2);
    // The card path's decision is put on record by its card, so a summary that takes it adds none.
    let turn = turn_facts(&[("a.rs", 9)], None);
    let decided = decide(OfferKind::Turn, &turn, &[]);
    let mut request = turn_request(4);
    settle(&mut request, &turn, &[], Some(&decided)).record(&offers, &request);
    assert_eq!(lines_of(&offers).len(), 2);
}

#[test]
fn a_decision_is_on_the_record_once_across_a_restart() {
    let (_dir, store) = store();
    let (_o, offers) = offers();
    let turn = turn_facts(&[("a.rs", 2)], None);
    for _ in 0..2 {
        offer_stop(
            &store,
            &FakeWriter,
            card_request("s@1", None, ""),
            &studio_core::Session::default(),
            &turn,
            &offers,
            || {},
            |_| {},
        );
    }
    assert_eq!(lines_of(&offers).len(), 1);
    assert!(offers.settled("s", "s@1"));
    assert!(!offers.settled("s", "s@2"));
}

#[test]
fn only_a_card_with_its_turns_id_is_the_turns_card() {
    let session = stopped_session();
    assert!(turns_card(&session, &card_request("s@9", None, "")));
    assert!(!turns_card(&session, &card_request("s@20", None, "")));
    assert!(!turns_card(&session, &card_request("s@9", Some("w"), "")));
}
