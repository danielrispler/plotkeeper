//! The card store (danielrispler/Falinks#328, module 6; danielrispler/Falinks#369, decisions 15 and 17): cards, their base, Daniel's guesses,
//! his flags, his ratings and the longer explanations he asked for survive a restart.

use crate::common::{Answer, Scripted};
use std::sync::Mutex;
use studio_core::{Base, BlockedStatus, CardRequest, Change, LiveFacts, Output, State, Studio};
use studio_understanding::{
    Card, CardStore, CardWriter, FakeWriter, Guess, Level, Rating, Reason, Risk, RiskLevel,
    RiskNotes, RiskRequest, RiskView, StoredCard, Surface, explain_more, judge, write_card,
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
        change: Some(format!(
            "--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-old\n+new\n"
        )),
        prompt: None,
        ..CardRequest::default()
    }
}

/// The fake writer's card on `request`, whose quiz's key is its first option of three.
fn card(request: &CardRequest) -> Card {
    let mut card = write_card(&FakeWriter, request, &[], |line| panic!("{line}")).unwrap();
    // The app shuffled the options; put the key back first.
    let quiz = card.quiz.as_mut().unwrap();
    quiz.options.swap(0, quiz.key);
    quiz.key = 0;
    card
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
                right: Some(false),
                at: 1_761_744_000_000
            }),
            Some(Guess {
                option: 0,
                right: Some(true),
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

#[test]
fn explain_more_calls_the_writer_once_and_a_second_tap_shows_the_kept_text() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let store = Mutex::new(two_cards(&path));
    let writer = Scripted::fake();
    let request = edit("toolu_01A", "src/lib.rs");
    let expected = FakeWriter.more(&request, &card(&request)).unwrap();

    let first = explain_more(&store, &writer, "toolu_01A", |line| panic!("{line}"));
    let second = explain_more(&store, &writer, "toolu_01A", |line| panic!("{line}"));

    assert_eq!(writer.calls("more"), 1);
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

#[test]
fn an_older_card_whose_change_is_its_workers_summaries_loads_shows_and_explains_more() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let request = CardRequest {
        change: Some("No diff: the agent changed nothing itself since its last card. Its workers, each with its status and the summaries of its cards since then:\n- `Implement #378` (completed): Sample change.\n".into()),
        workers: vec!["w1@1".into()],
        ..edit("s1@2", "a.rs")
    };
    let mut store = CardStore::open(path.clone()).unwrap();
    store
        .add(request.clone(), card(&request), 1_761_743_990_000)
        .unwrap();
    drop(store);

    let store = Mutex::new(CardStore::open(path).unwrap());
    let shown: Vec<_> = store
        .lock()
        .unwrap()
        .cards()
        .iter()
        .map(|kept| kept.request.id.clone())
        .collect();
    let more = explain_more(&store, &FakeWriter, "s1@2", |line| panic!("{line}"));

    assert_eq!(shown, ["s1@2"]);
    assert_eq!(
        more,
        Some(FakeWriter.more(&request, &card(&request)).unwrap())
    );
}

#[test]
fn explain_more_with_no_text_logs_one_line_and_keeps_nothing() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let store = Mutex::new(two_cards(&path));
    let mut lines = Vec::new();

    let more = explain_more(
        &store,
        &Scripted {
            more: Answer::ok("\n"),
            ..Scripted::fake()
        },
        "toolu_01A",
        |line| lines.push(line),
    );

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
    store.show(&["toolu_01A"], 1_761_744_000_000).unwrap();
    store.guess("toolu_01A", 0, 1_761_744_004_000).unwrap();
    // A card guessed before its quiz time was kept gets none, so a guess never comes
    // before its quiz.
    store.guess("toolu_01B", 0, 1_761_744_060_000).unwrap();
    // The pane draws both again in one call, after a restart; the first time stays.
    store
        .show(&["toolu_01A", "toolu_01B"], 1_761_744_070_000)
        .unwrap();

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

/// A store written by danielrispler/Falinks#374's build, before cards had a quiz.
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
    assert_eq!(kept.request.diff(), "-old\n+new\n");
    assert_eq!(kept.guess, None);
}

/// A store from before danielrispler/Falinks#374, with a card on one step.
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

/// danielrispler/Falinks#347's checkpoint day (danielrispler/Falinks#370's fixture), live, with a change at every `Stop`'s fingerprint: each
/// card request gets a card at its event's time. Before danielrispler/Falinks#371 the pane showed only the cards of the
/// session prompted last, so the other session's cards seemed deleted (danielrispler/Falinks#347 note, "Cards were not
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
        head: None,
    };
    for line in day.lines() {
        let event = serde_json::from_str::<serde_json::Value>(line).unwrap();
        let at = event["studio_received_ms"].as_u64().unwrap();
        studio.tick(at);
        let stop = event["hook_event_name"] == "Stop";
        let looks = studio
            .fingerprint_folders(line)
            .into_iter()
            .filter(|_| stop);
        let changes = looks.map(|look| (look.key, changed.clone())).collect();
        for output in studio.apply_with_changes(line, changes) {
            if let Output::Card(request) = output {
                let card = card(&request);
                store.add(request, card, at).unwrap();
            }
        }
    }
    let sessions: Vec<String> = studio
        .sessions()
        .iter()
        .map(|s| s.agent.id.clone())
        .collect();

    let shown: Vec<(&str, usize)> = sessions
        .iter()
        .map(|id| {
            studio.select(id);
            let selected = studio.selected().unwrap();
            let cards = store.session_cards(&selected.agent.id, studio.now());
            (&id[..8], cards.recent.len() + cards.older.len())
        })
        .collect();

    // In list order, one card per `Stop`; the session with no prompt has none.
    assert_eq!(
        shown,
        [
            ("596c28a1", 7),
            // With `0222b62d`'s 4, whose `/clear` continued its row (danielrispler/Falinks#474), as `6641e809`'s none
            // join `5d7f69ab`'s.
            ("5bb84a4a", 8),
            ("2afc85f5", 14),
            ("57e9c1ab", 0),
            ("1ac172be", 7),
            ("5d7f69ab", 14),
        ]
    );
    assert_eq!(store.cards().len(), 50);
}

#[test]
fn a_goal_survives_a_restart_and_a_brief_stored_as_prose_shows_until_it_comes() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let session = "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092";
    // The briefs file of before the slots: prose and an update count.
    let prose = "הסשן התחיל.\n`README.md` עודכן.";
    let old = serde_json::json!({ session: { "text": prose, "updates": 7 } });
    std::fs::write(data.path().join("briefs.json"), old.to_string()).unwrap();
    let mut store = CardStore::open(path.clone()).unwrap();
    let shown = store.session_cards(session, 0);
    assert_eq!((shown.brief.as_deref(), shown.goal), (Some(prose), None));

    store
        .set_goal(session, "לשלוח את הכרטיס.".into(), 2)
        .unwrap();
    store.set_goal("another", "סשן אחר.".into(), 1).unwrap();
    let reopened = CardStore::open(path).unwrap();

    let kept = reopened.brief(session).unwrap();
    assert_eq!(
        (kept.goal.as_deref(), kept.goal_prompts),
        (Some("לשלוח את הכרטיס."), 2)
    );
    let shown = reopened.session_cards(session, 0);
    assert_eq!(shown.goal.as_deref(), Some("לשלוח את הכרטיס."));
    assert_eq!(reopened.session_cards("none", 0).goal, None);
}

#[test]
fn every_change_bumps_the_revision_and_a_call_that_changes_nothing_leaves_it() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = CardStore::open(path.clone()).unwrap();
    let mut last = store.revision();
    let mut bumped = |store: &CardStore, changed: bool| {
        assert_eq!(store.revision() > last, changed);
        last = store.revision();
    };
    let request = edit("toolu_01A", "src/lib.rs");
    let first = card(&request);
    store.add(request, first, 1_761_743_990_000).unwrap();
    bumped(&store, true);
    store.show(&["toolu_01A"], 1_761_743_995_000).unwrap();
    bumped(&store, true);
    store.show(&["toolu_01A"], 1_761_743_996_000).unwrap();
    bumped(&store, false);
    store.guess("toolu_01A", 1, 1_761_744_000_000).unwrap();
    bumped(&store, true);
    store.guess("toolu_01A", 0, 1_761_744_001_000).unwrap();
    bumped(&store, false);
    store.key_wrong("toolu_01A").unwrap();
    bumped(&store, true);
    store.key_wrong("toolu_01A").unwrap();
    bumped(&store, false);
    let good = Rating {
        level: Level::Good,
        reasons: vec![],
    };
    store.rate("toolu_01A", good).unwrap();
    bumped(&store, true);
    store
        .rate(
            "toolu_01Z",
            Rating {
                level: Level::Poor,
                reasons: vec![],
            },
        )
        .unwrap();
    bumped(&store, false);
    store
        .set_goal("5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092", "Goal.".into(), 1)
        .unwrap();
    bumped(&store, true);
    let store = Mutex::new(store);
    explain_more(&store, &FakeWriter, "toolu_01A", |line| panic!("{line}"));
    bumped(&store.lock().unwrap(), true);
    explain_more(&store, &FakeWriter, "toolu_01A", |line| panic!("{line}"));
    bumped(&store.lock().unwrap(), false);
}

#[test]
fn quizzes_that_showed_at_once_are_saved_in_one_call() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    store
        .show(&["toolu_01A", "toolu_01B"], 1_761_744_000_000)
        .unwrap();

    let reopened = CardStore::open(path).unwrap();

    let shown: Vec<_> = reopened.cards().iter().map(|kept| kept.shown_at).collect();
    assert_eq!(shown, [Some(1_761_744_000_000), Some(1_761_744_000_000)]);
}

/// The pane never reads a card's diff or base, so the payload of a session's cards goes without them.
#[test]
fn the_payload_without_diffs() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let request = edit("toolu_01A", "src/lib.rs");
    let card = card(&request);
    let kept = serde_json::json!([{ "request": request, "card": card }]);
    std::fs::write(&path, kept.to_string()).unwrap();
    let store = CardStore::open(path).unwrap();

    let shown = store.session_cards("5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092", 1_761_743_990_000);

    let payload = serde_json::to_value(&shown).unwrap();
    let sent = payload["older"][0]["request"].as_object().unwrap();
    let mut keys: Vec<&str> = sent.keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "agent",
            "changed_by",
            "check",
            "id",
            "prompt",
            "session_id",
            "workers"
        ]
    );
    assert_eq!(
        payload["older"][0]["card"],
        serde_json::to_value(&card).unwrap()
    );
}

/// danielrispler/Falinks#434, decision 37: the helpers promoted live, the failed lines Daniel opened and the turns he
/// saw (danielrispler/Falinks#556) survive a restart, and a store from before they were kept opens with none.
#[test]
fn the_live_only_facts_survive_a_restart_and_an_older_store_opens_with_none() {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    assert_eq!(store.facts(), &LiveFacts::default());

    store.promoted("a551b").unwrap();
    store.opened("a551b").unwrap();
    store.opened("add0c").unwrap();
    store.seen("c41d8", 2).unwrap();

    let reopened = CardStore::open(path).unwrap();
    let facts = LiveFacts {
        promoted: ["a551b".to_string()].into(),
        opened: ["a551b".to_string(), "add0c".to_string()].into(),
        seen: [("c41d8".to_string(), 2)].into(),
    };
    assert_eq!(reopened.facts(), &facts);
    assert_eq!(reopened.cards(), store.cards());
}

/// A store at `path` holding a lone Quiz on turn `t1`: the summary's own, with no card.
fn lone_quiz(path: &std::path::Path) -> CardStore {
    let mut store = CardStore::open(path.to_owned()).unwrap();
    let request = studio_core::SummaryRequest {
        id: "t1".into(),
        ..Default::default()
    };
    let summary: studio_understanding::TurnSummary = serde_json::from_value(serde_json::json!({
        "summary": "Did a thing.",
        "quiz": { "text": "Why?", "key": 0, "options": [
            { "text": "A", "why": "Because." }, { "text": "B", "why": "No." }] },
    }))
    .unwrap();
    store.add_summary(request, summary).unwrap();
    store
}

#[test]
fn a_skip_alone_is_stored_and_a_pick_after_it_is_not() {
    use studio_understanding::{QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let mut store = lone_quiz(&data.path().join("cards.json"));
    store.quiz_tap("t1", Tap::Skip, 5).unwrap();
    store.quiz_tap("t1", Tap::Pick { option: 0 }, 6).unwrap();
    assert_eq!(
        store.turn_summary("t1").unwrap().taps,
        [QuizTap {
            tap: Tap::Skip,
            at: 5
        }]
    );
}

#[test]
fn a_cards_skip_and_chips_are_stored_beside_its_guess_and_the_key_flag_is_the_one_record() {
    use studio_understanding::{QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = two_cards(&path);
    store.quiz_tap("toolu_01A", Tap::Skip, 7).unwrap();
    store
        .quiz_tap("toolu_01A", Tap::Pick { option: 0 }, 8)
        .unwrap();
    store.quiz_tap("toolu_01A", Tap::KeyWrong, 9).unwrap();
    store
        .quiz_tap("toolu_01B", Tap::Pick { option: 2 }, 10)
        .unwrap();
    store.quiz_tap("toolu_01B", Tap::Skip, 11).unwrap();

    let reopened = CardStore::open(path).unwrap();
    let [a, b] = reopened.cards() else { panic!() };
    assert_eq!(a.guess, None);
    assert!(a.key_wrong);
    assert_eq!(
        a.taps,
        [
            QuizTap {
                tap: Tap::Skip,
                at: 7
            },
            QuizTap {
                tap: Tap::KeyWrong,
                at: 9
            }
        ]
    );
    assert_eq!(b.guess.map(|g| (g.option, g.at)), Some((2, 10)));
    assert_eq!(b.taps, []);
}

const DAY: u64 = 24 * 60 * 60 * 1000;
const START: u64 = 1_800_000_000_000;

fn quiz_turn(store: &mut CardStore, id: &str, at: u64, coin: bool) {
    let request = studio_core::SummaryRequest {
        id: id.into(),
        session_id: "s1".into(),
        ..Default::default()
    };
    let summary: studio_understanding::TurnSummary = serde_json::from_value(serde_json::json!({
        "summary": "Did a thing.",
        "quiz": { "text": "Why?", "key": 0, "options": [
            { "text": "A", "why": "Because." }, { "text": "B", "why": "No." }] },
    }))
    .unwrap();
    store.add_summary_with(request, summary, at, coin).unwrap();
}

#[test]
fn inside_the_test_each_quiz_turn_gets_an_arm_stored_with_it_and_outside_none_is_body_only() {
    use studio_understanding::Arm;
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = CardStore::open(path.clone()).unwrap();
    // Before the test starts, and with no test set, a turn has no arm.
    quiz_turn(&mut store, "none", START, true);
    store.set_self_test(Some(START + DAY));
    quiz_turn(&mut store, "before", START, true);
    quiz_turn(&mut store, "first", START + DAY, true);
    quiz_turn(&mut store, "second", START + DAY, false);
    quiz_turn(&mut store, "last", START + 22 * DAY - 1, true);
    quiz_turn(&mut store, "after", START + 22 * DAY, true);
    let arm = |s: &CardStore, id: &str| s.turn_summary(id).unwrap().arm;
    assert_eq!(arm(&store, "none"), None);
    assert_eq!(arm(&store, "before"), None);
    assert_eq!(arm(&store, "first"), Some(Arm::BodyOnly));
    assert_eq!(arm(&store, "second"), Some(Arm::QuizFirst));
    assert_eq!(arm(&store, "last"), Some(Arm::BodyOnly));
    assert_eq!(arm(&store, "after"), None);
    assert_eq!(
        arm(&CardStore::open(path).unwrap(), "first"),
        Some(Arm::BodyOnly)
    );
}

#[test]
fn the_coin_gives_both_arms_and_a_turn_with_no_quiz_gets_none() {
    use studio_understanding::Arm;
    let data = tempfile::tempdir().unwrap();
    let mut store = lone_quiz(&data.path().join("cards.json"));
    store.set_self_test(Some(START));
    for i in 0..40 {
        let request = studio_core::SummaryRequest {
            id: format!("r{i}"),
            ..Default::default()
        };
        let summary: studio_understanding::TurnSummary =
            serde_json::from_value(serde_json::json!({
                "summary": "Did a thing.",
                "quiz": { "text": "Why?", "key": 0, "options": [
                    { "text": "A", "why": "Because." }, { "text": "B", "why": "No." }] },
            }))
            .unwrap();
        store.add_summary_at(request, summary, START + i).unwrap();
    }
    let arms: Vec<_> = store.summaries().iter().filter_map(|s| s.arm).collect();
    assert!(arms.contains(&Arm::BodyOnly) && arms.contains(&Arm::QuizFirst));
    let plain: studio_understanding::TurnSummary =
        serde_json::from_value(serde_json::json!({ "summary": "Nothing." })).unwrap();
    store
        .add_summary_with(Default::default(), plain, START, true)
        .unwrap();
    assert_eq!(store.summaries().last().unwrap().arm, None);
}

/// danielrispler/plotkeeper#212: session `s1`, whose first turn spawns worker `w1` and whose wake-up for it opens its third.
fn session_with_a_worker() -> Studio {
    let event = |name: &str, at: u64, prompt: &str, fields: serde_json::Value| {
        let mut event = serde_json::json!({"session_id": "s1", "hook_event_name": name,
            "cwd": "/w", "prompt_id": prompt, "studio_received_ms": at});
        event
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        event.to_string()
    };
    let mut studio = Studio::default();
    studio.apply(&event(
        "UserPromptSubmit",
        1,
        "p1",
        serde_json::json!({"prompt": "Do it"}),
    ));
    studio.apply(&event("PostToolUse", 2, "p1", serde_json::json!({"tool_name": "Agent",
        "tool_use_id": "t1", "tool_input": {}, "tool_response": {"agentId": "w1", "status": "async_launched"}})));
    studio.apply(&event(
        "SubagentStop",
        10,
        "p1",
        serde_json::json!({"agent_id": "w1"}),
    ));
    studio.apply(&event(
        "Stop",
        11,
        "p1",
        serde_json::json!({"background_tasks": []}),
    ));
    studio.apply(&event(
        "UserPromptSubmit",
        12,
        "p2",
        serde_json::json!({"prompt": "Meanwhile"}),
    ));
    studio.apply(&event(
        "Stop",
        14,
        "p2",
        serde_json::json!({"background_tasks": []}),
    ));
    let woken =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    studio.apply(&event(
        "UserPromptSubmit",
        20,
        "p3",
        serde_json::json!({"prompt": woken}),
    ));
    studio
}

#[test]
fn a_sessions_rail_cards_include_its_workers_each_with_its_turn_and_the_workers_view_is_unchanged()
{
    let studio = session_with_a_worker();
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    let request = |session: &str, agent: Option<&str>, at: u64| CardRequest {
        session_id: session.into(),
        id: format!("{}@{at}", agent.unwrap_or(session)),
        agent: agent.map(Into::into),
        change: Some("--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n".into()),
        ..CardRequest::default()
    };
    for r in [
        request("s1", None, 5),
        request("s1", Some("w1"), 9),
        request("s1", Some("w1"), 11),
        request("other", Some("w9"), 12),
    ] {
        let card = card(&r);
        store.add(r, card, 1_761_743_990_000).unwrap();
    }

    let rail = store.rail_cards(studio.session("s1").unwrap(), 1_761_743_990_000);
    assert_eq!(
        store.placements(studio.session("s1").unwrap()),
        [
            ("w1@9".to_string(), Some(0)),
            ("w1@11".to_string(), Some(2))
        ]
    );

    let placed: Vec<(&str, Option<usize>)> = rail
        .workers
        .iter()
        .map(|w| (w.card.request.id.as_str(), w.turn))
        .collect();
    assert_eq!(placed, [("w1@9", Some(0)), ("w1@11", Some(2))]);
    assert_eq!(ids(&rail.recent), ["s1@5"]);
    let own = store.session_cards("w1", 1_761_743_990_000);
    assert_eq!(ids(&own.recent), ["w1@9", "w1@11"]);
    assert!(own.workers.is_empty());
}

/// The previous build's `Facts`, which requires `external`.
#[derive(serde::Deserialize)]
#[allow(dead_code)]
struct MainFacts {
    files: Vec<serde_json::Value>,
    checks: Vec<serde_json::Value>,
    external: u64,
}

#[test]
fn a_summary_file_can_still_be_read_by_the_previous_builds_facts() {
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    let file = studio_core::TurnFile {
        root: "/w".into(),
        path: "src/gate.rs".into(),
        from: None,
        added: Some(1),
        removed: Some(1),
        agent: None,
    };
    let request = studio_core::SummaryRequest {
        id: "t1".into(),
        session_id: "s1".into(),
        facts: studio_core::Facts {
            files: vec![studio_core::FactFile {
                file,
                hunk: "@@ -1 +1 @@\n-open\n+frozen\n".into(),
            }],
            ..studio_core::Facts::default()
        },
        ..Default::default()
    };
    let summary: studio_understanding::TurnSummary =
        serde_json::from_value(serde_json::json!({ "summary": "Froze the gate." })).unwrap();
    store.add_summary(request, summary).unwrap();
    let text = std::fs::read_to_string(data.path().join("summaries.json")).unwrap();
    let saved: serde_json::Value = serde_json::from_str(&text).unwrap();
    let facts = saved
        .as_array()
        .and_then(|all| all.first())
        .and_then(|first| first.pointer("/request/facts"))
        .unwrap_or_else(|| panic!("no facts in {text}"));
    serde_json::from_value::<MainFacts>(facts.clone()).unwrap();
}

#[test]
fn a_summary_that_becomes_waiting_raises_the_revision_so_the_window_asks_again() {
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    let before = store.revision();
    store.waiting_summary();
    assert!(store.revision() > before);
}

#[test]
fn a_confidence_tap_keeps_its_surface_and_one_without_reads_as_the_rails() {
    use studio_understanding::{QuizTap, Tap};
    // A tap stored before the field existed, and one the rail sends, are the rail's.
    let old: QuizTap = serde_json::from_str(r#"{"kind":"confidence","sure":true,"at":2}"#).unwrap();
    assert_eq!(
        old.tap,
        Tap::Confidence {
            sure: true,
            surface: Surface::Rail
        }
    );
    let home: Tap =
        serde_json::from_str(r#"{"kind":"confidence","sure":true,"surface":"home"}"#).unwrap();
    assert_eq!(
        home,
        Tap::Confidence {
            sure: true,
            surface: Surface::Home
        }
    );
}

#[test]
fn a_tap_of_a_kind_this_build_does_not_know_reads_without_error() {
    use studio_understanding::{QuizTap, Tap};
    let tap: QuizTap = serde_json::from_str(r#"{"kind":"from_the_future","at":7,"x":1}"#).unwrap();
    assert_eq!((tap.tap, tap.at), (Tap::Unknown, 7));
}

#[test]
fn a_summary_stored_before_the_checks_grew_fields_still_opens() {
    let data = tempfile::tempdir().unwrap();
    let log = data.path().join("events.jsonl");
    let mut store = CardStore::open(data.path().join("cards-v2.json")).unwrap();
    let check = serde_json::json!({
        "suite": "cargo test", "worktree": "/w", "foreign": false, "status": "passed", "retries": 0,
        "runs": [{ "command": "cargo test", "statement": "cargo test", "status": "passed",
            "exit_code": 0, "call": "t1", "agent": null, "started_at": 1, "ended_at": 2 }],
    });
    let request = studio_core::SummaryRequest {
        id: "t1".into(),
        facts: studio_core::Facts {
            checks: vec![serde_json::from_value(check).unwrap()],
            ..Default::default()
        },
        ..Default::default()
    };
    let summary: studio_understanding::TurnSummary =
        serde_json::from_value(serde_json::json!({ "summary": "Did a thing." })).unwrap();
    store.add_summary(request, summary).unwrap();
    // What the build before the checks module wrote: no statement, times, worktree or foreign.
    let path = data.path().join("summaries.json");
    let mut stored: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let check = &mut stored[0]["request"]["facts"]["checks"][0];
    check.as_object_mut().unwrap().remove("worktree");
    check.as_object_mut().unwrap().remove("foreign");
    for field in ["statement", "started_at", "ended_at"] {
        check["runs"][0].as_object_mut().unwrap().remove(field);
    }
    std::fs::write(&path, stored.to_string()).unwrap();

    let reopened = CardStore::open_beside(&log).unwrap();

    let kept = &reopened.summaries()[0].request.facts.checks[0];
    assert_eq!(kept.runs[0].statement, "");
    assert_eq!(kept.worktree, "");
}

// The risk notes (danielrispler/plotkeeper#298): a store of their own, keyed by the session and a hash of the command and its folder.

const RISK_SESSION: &str = "5c1e7a42-0b9d-4f36-8a15-3d2e9c6b7f08";
const PUSH: &str = "git push origin feature-x";
const REPO: &str = "/work/repo";

/// A hook line of the risk session.
fn risk_line(session: &str, name: &str, at: u64, extra: serde_json::Value) -> String {
    let mut line = serde_json::json!({
        "session_id": session, "hook_event_name": name, "cwd": REPO, "studio_received_ms": at,
    });
    line.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    line.to_string()
}

/// The lines in which auto mode blocks `command` in `cwd` as call `id`.
fn blocks(session: &str, id: &str, command: &str, cwd: &str) -> String {
    let extra = serde_json::json!({ "tool_name": "Bash", "tool_use_id": id, "prompt_id": "p-1",
        "tool_input": { "command": command }, "reason": "[Unauthorized Access]", "permission_mode": "auto" });
    let line = risk_line(session, "PermissionDenied", 3_000, extra);
    let mut line: serde_json::Value = serde_json::from_str(&line).unwrap();
    line["cwd"] = cwd.into();
    line.to_string()
}

/// Daniel's Allow of call `id` from the card.
fn allows(session: &str, id: &str, command: &str) -> String {
    let extra = serde_json::json!({ "studio_asked_by": "PermissionDenied", "tool_name": "Bash", "tool_use_id": id,
        "tool_input": { "command": command }, "studio_answer": { "decision": "allow" } });
    risk_line(session, "StudioAnswer", 4_000, extra)
}

/// The session `lines` make, after its start and a prompt.
fn risk_session(lines: &[String]) -> studio_core::Session {
    let mut all = vec![
        risk_line(
            RISK_SESSION,
            "SessionStart",
            1_000,
            serde_json::json!({ "source": "startup" }),
        ),
        risk_line(
            RISK_SESSION,
            "UserPromptSubmit",
            2_000,
            serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
        ),
    ];
    all.extend(lines.iter().cloned());
    let studio = Studio::replayed(&all);
    studio.session(RISK_SESSION).unwrap().clone()
}

fn push_blocked() -> studio_core::Session {
    risk_session(&[blocks(RISK_SESSION, "toolu_1", PUSH, REPO)])
}

fn judged(level: RiskLevel, line: &str) -> Result<Risk, String> {
    Ok(Risk {
        level,
        line: line.into(),
    })
}

fn the_ask() -> RiskRequest {
    RiskRequest {
        session: RISK_SESSION.into(),
        command: PUSH.into(),
        cwd: REPO.into(),
    }
}

fn arrived(call: &str, level: RiskLevel, line: &str, at: u64) -> RiskView {
    RiskView::Arrived {
        call: call.into(),
        level,
        line: line.into(),
        at,
    }
}

#[test]
fn a_note_is_kept_across_a_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("risk-notes.json");
    let session = push_blocked();
    let mut notes = RiskNotes::open(path.clone()).unwrap();
    assert_eq!(notes.asks(&session), [the_ask()]);
    notes
        .keep(
            &the_ask(),
            judged(RiskLevel::Medium, "Pushes a branch."),
            7_000,
        )
        .unwrap();

    let mut reopened = RiskNotes::open(path).unwrap();

    assert_eq!(
        reopened.views(&session),
        [arrived(
            "toolu_1",
            RiskLevel::Medium,
            "Pushes a branch.",
            7_000
        )]
    );
    // Judged before: not asked for again after the restart.
    assert_eq!(reopened.asks(&session), []);
}

#[test]
fn a_card_whose_call_id_changed_still_has_its_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut notes = RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
    notes
        .keep(&the_ask(), judged(RiskLevel::High, "Pushes."), 7_000)
        .unwrap();
    // Allowed, then blocked again: the card's call id is the second block's.
    let again = risk_session(&[
        blocks(RISK_SESSION, "toolu_1", PUSH, REPO),
        allows(RISK_SESSION, "toolu_1", PUSH),
        blocks(RISK_SESSION, "toolu_2", PUSH, REPO),
    ]);
    let [card] = &again.blocked_commands[..] else {
        panic!("one card: {:?}", again.blocked_commands);
    };
    assert_eq!(card.call, "toolu_2");

    assert_eq!(
        notes.views(&again),
        [arrived("toolu_2", RiskLevel::High, "Pushes.", 7_000)]
    );
    assert_eq!(notes.asks(&again), []);
}

#[test]
fn two_cards_of_one_command_and_folder_in_a_session_share_one_note_and_one_ask() {
    let dir = tempfile::tempdir().unwrap();
    let mut notes = RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
    let session = risk_session(&[
        blocks(RISK_SESSION, "toolu_1", PUSH, REPO),
        blocks(RISK_SESSION, "toolu_2", PUSH, REPO),
        blocks(RISK_SESSION, "toolu_3", PUSH, "/work/other"),
    ]);
    assert_eq!(session.blocked_commands.len(), 3);

    let asks = notes.asks(&session);
    assert_eq!(
        asks.iter().map(|a| a.cwd.as_str()).collect::<Vec<_>>(),
        [REPO, "/work/other"]
    );
    notes
        .keep(&asks[0], judged(RiskLevel::Low, "Pushes."), 7_000)
        .unwrap();

    assert_eq!(
        notes.views(&session),
        [
            arrived("toolu_1", RiskLevel::Low, "Pushes.", 7_000),
            arrived("toolu_2", RiskLevel::Low, "Pushes.", 7_000),
            RiskView::Reading {
                call: "toolu_3".into()
            },
        ]
    );
    // Another session's card of the same command and folder has no note of this one's.
    let other = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";
    let elsewhere = {
        let lines = [
            risk_line(
                other,
                "SessionStart",
                1_000,
                serde_json::json!({ "source": "startup" }),
            ),
            risk_line(
                other,
                "UserPromptSubmit",
                2_000,
                serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
            ),
            blocks(other, "toolu_9", PUSH, REPO),
        ];
        Studio::replayed(&lines).session(other).unwrap().clone()
    };
    assert_eq!(notes.views(&elsewhere), []);
    assert_eq!(notes.asks(&elsewhere).len(), 1);
}

#[test]
fn a_failure_is_not_kept_it_reads_failed_for_the_run_and_the_next_run_asks_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("risk-notes.json");
    let session = push_blocked();
    let mut notes = RiskNotes::open(path.clone()).unwrap();
    notes.asks(&session);
    notes
        .keep(
            &the_ask(),
            Err("`claude -p` ran past 15s and was stopped".into()),
            7_000,
        )
        .unwrap();

    assert_eq!(
        notes.views(&session),
        [RiskView::Failed {
            call: "toolu_1".into()
        }]
    );
    // Not asked for again in this run.
    assert_eq!(notes.asks(&session), []);
    // Nothing was written, so a restart has no note and asks again.
    let mut reopened = RiskNotes::open(path).unwrap();
    assert_eq!(reopened.views(&session), []);
    assert_eq!(reopened.asks(&session), [the_ask()]);
}

#[test]
fn a_note_is_asked_for_once_per_run_only_for_a_card_with_buttons_and_never_for_an_ended_session() {
    let dir = tempfile::tempdir().unwrap();
    let mut notes = RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
    let session = push_blocked();

    assert_eq!(notes.asks(&session), [the_ask()]);
    assert_eq!(notes.asks(&session), [], "once per command per run");
    // The card shows before the ask is answered.
    assert_eq!(
        notes.views(&session),
        [RiskView::Reading {
            call: "toolu_1".into()
        }]
    );

    // An Allowed card has no button, and a declined one is closed.
    let mut notes = RiskNotes::open(dir.path().join("other.json")).unwrap();
    let allowed = risk_session(&[
        blocks(RISK_SESSION, "toolu_1", PUSH, REPO),
        allows(RISK_SESSION, "toolu_1", PUSH),
    ]);
    assert_eq!(allowed.blocked_commands[0].status, BlockedStatus::Allowed);
    assert_eq!(notes.asks(&allowed), []);
    assert_eq!(notes.views(&allowed), []);

    // A session that ended asks for none.
    let ended = risk_session(&[
        blocks(RISK_SESSION, "toolu_1", PUSH, REPO),
        risk_line(RISK_SESSION, "SessionEnd", 5_000, serde_json::json!({})),
    ]);
    assert_eq!(ended.state, State::Ended);
    assert_eq!(notes.asks(&ended), []);
}

#[test]
fn judging_keeps_the_note_with_the_time_without_holding_the_store_and_a_failure_logs_one_line_without_the_command()
 {
    let dir = tempfile::tempdir().unwrap();
    let notes = std::sync::Arc::new(Mutex::new(
        RiskNotes::open(dir.path().join("risk-notes.json")).unwrap(),
    ));
    let session = push_blocked();
    let asks = notes.lock().unwrap().asks(&session);
    let free = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let writer = {
        let (notes, free) = (notes.clone(), free.clone());
        let mut writer = Scripted::fake();
        writer.risk = Answer::run(move |_| {
            // The store answers other calls while the judge runs.
            free.store(
                notes.try_lock().is_ok(),
                std::sync::atomic::Ordering::SeqCst,
            );
            Some(Ok(
                r#"{"level": "medium", "line": "Pushes a branch."}"#.into()
            ))
        });
        writer
    };

    judge(&notes, &writer, &asks[0], || 7_000, |line| panic!("{line}"));

    assert_eq!(writer.judged(), [(PUSH.to_string(), REPO.to_string())]);
    assert!(free.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        notes.lock().unwrap().views(&session),
        [arrived(
            "toolu_1",
            RiskLevel::Medium,
            "Pushes a branch.",
            7_000
        )]
    );

    // A writer that answers nothing readable, and one that writes no risk notes, each log one line.
    let mut bare = Scripted::bare();
    let unreadable = [
        Answer::ok("Shall I read the file?"),
        Answer::err("`claude -p` failed"),
    ];
    for (n, answer) in unreadable.into_iter().enumerate() {
        let other = format!("{PUSH} {n}");
        let session = risk_session(&[blocks(RISK_SESSION, "toolu_9", &other, REPO)]);
        let ask = notes.lock().unwrap().asks(&session).remove(0);
        bare.risk = answer;
        let mut lines = Vec::new();

        judge(&notes, &bare, &ask, || 8_000, |line| lines.push(line));

        let [line] = &lines[..] else {
            panic!("one line: {lines:?}");
        };
        assert_eq!(line.lines().count(), 1, "{line}");
        assert!(!line.contains(&other), "{line}");
        assert_eq!(
            notes.lock().unwrap().views(&session),
            [RiskView::Failed {
                call: "toolu_9".into()
            }]
        );
    }
}

#[test]
fn a_note_is_kept_with_the_time_the_judge_answered_not_the_time_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let notes = Mutex::new(RiskNotes::open(dir.path().join("risk-notes.json")).unwrap());
    let session = push_blocked();
    let ask = notes.lock().unwrap().asks(&session).remove(0);
    // The clock reads 1_000 when the judge starts and 5_000 when the writer returns.
    let clock = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1_000));
    let mut writer = Scripted::fake();
    writer.risk = {
        let clock = clock.clone();
        Answer::run(move |_| {
            clock.store(5_000, std::sync::atomic::Ordering::SeqCst);
            Some(Ok(r#"{"level": "high", "line": "Pushes."}"#.into()))
        })
    };

    judge(
        &notes,
        &writer,
        &ask,
        || clock.load(std::sync::atomic::Ordering::SeqCst),
        |line| panic!("{line}"),
    );

    assert_eq!(
        notes.lock().unwrap().views(&session),
        [arrived("toolu_1", RiskLevel::High, "Pushes.", 5_000)]
    );
}

#[test]
fn a_bash_decision_is_joined_to_its_note_by_command_and_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut notes = RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
    let allowed = risk_session(&[
        blocks(RISK_SESSION, "toolu_1", PUSH, REPO),
        allows(RISK_SESSION, "toolu_1", PUSH),
    ]);
    // No note yet: an entry that says so.
    assert_eq!(
        notes.decision_views(&allowed),
        [RiskView::Failed {
            call: "toolu_1".into()
        }]
    );

    notes
        .keep(
            &the_ask(),
            judged(RiskLevel::High, "Pushes a branch."),
            7_000,
        )
        .unwrap();
    assert_eq!(
        notes.decision_views(&allowed),
        [arrived(
            "toolu_1",
            RiskLevel::High,
            "Pushes a branch.",
            7_000
        )]
    );

    // The same command in another folder is another note.
    let elsewhere = risk_session(&[
        blocks(RISK_SESSION, "toolu_2", PUSH, "/work/other"),
        allows(RISK_SESSION, "toolu_2", PUSH),
    ]);
    assert_eq!(
        notes.decision_views(&elsewhere),
        [RiskView::Failed {
            call: "toolu_2".into()
        }]
    );
}

#[test]
fn a_blocked_commands_decision_has_an_entry_with_or_without_a_note_and_an_ordinary_bash_permission_has_none()
 {
    let dir = tempfile::tempdir().unwrap();
    let mut notes = RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
    let ask = |name: &str, at: u64| {
        let extra = serde_json::json!({ "tool_name": "Bash", "tool_use_id": "toolu_7",
            "tool_input": { "command": "ls" } });
        risk_line(RISK_SESSION, name, at, extra)
    };
    let session = risk_session(&[
        blocks(RISK_SESSION, "toolu_1", PUSH, REPO),
        allows(RISK_SESSION, "toolu_1", PUSH),
        ask("PreToolUse", 5_000),
        ask("PermissionRequest", 5_001),
        ask("PostToolUse", 5_002),
    ]);
    let calls: Vec<_> = session.decisions.iter().map(|d| d.call.as_str()).collect();
    assert_eq!(calls, ["toolu_1", "toolu_7"]);

    // No note yet: the blocked command's decision has an entry that says so, the ordinary permission has none.
    assert_eq!(
        notes.decision_views(&session),
        [RiskView::Failed {
            call: "toolu_1".into()
        }]
    );

    notes
        .keep(&the_ask(), judged(RiskLevel::High, "Pushes."), 3_500)
        .unwrap();
    assert_eq!(
        notes.decision_views(&session),
        [arrived("toolu_1", RiskLevel::High, "Pushes.", 3_500)]
    );
}

#[test]
fn a_corrupt_risk_notes_file_opens_empty_and_never_stops_the_start() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("risk-notes.json");
    std::fs::write(&path, "{ not json").unwrap();
    let session = push_blocked();

    let mut notes = RiskNotes::open(path.clone()).unwrap();

    // The notes are a cache: the card is judged again, and the file is whole after it.
    assert_eq!(notes.asks(&session), [the_ask()]);
    notes
        .keep(&the_ask(), judged(RiskLevel::Low, "Pushes."), 7_000)
        .unwrap();
    assert_eq!(
        RiskNotes::open(path).unwrap().views(&session),
        [arrived("toolu_1", RiskLevel::Low, "Pushes.", 7_000)]
    );
}
