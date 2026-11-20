//! The card store (danielrispler/Falinks#328, module 6; danielrispler/Falinks#369, decisions 15 and 17): cards, their base, Daniel's guesses,
//! his flags, his ratings and the longer explanations he asked for survive a restart.

use std::{cell::Cell, sync::Mutex};
use studio_core::{Base, CardRequest, Change, LiveFacts, Output, Studio};
use studio_understanding::{
    Card, CardStore, CardWriter, FakeWriter, Guess, Level, Rating, Reason, StoredCard, Surface,
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

/// The fake writer, counting its calls for "explain more".
#[derive(Default)]
struct Counting(Cell<usize>);

impl CardWriter for Counting {
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        FakeWriter.reply(request, earlier)
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
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        FakeWriter.reply(request, earlier)
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
fn a_lone_quizzes_pick_skip_and_chips_are_stored_with_their_times_and_survive_a_restart() {
    use studio_understanding::{QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = lone_quiz(&path);
    store.quiz_tap("t1", Tap::Pick { option: 1 }, 10).unwrap();
    // The first answer counts: a later pick and a skip change nothing; a chip is kept once.
    store.quiz_tap("t1", Tap::Pick { option: 0 }, 11).unwrap();
    store.quiz_tap("t1", Tap::Skip, 12).unwrap();
    store.quiz_tap("t1", Tap::ExplainMore, 13).unwrap();
    store.quiz_tap("t1", Tap::ExplainMore, 14).unwrap();
    store.quiz_tap("t1", Tap::Understood, 15).unwrap();
    // No such option, and no such turn, record nothing.
    store.quiz_tap("t1", Tap::Pick { option: 5 }, 16).unwrap();
    store.quiz_tap("nobody", Tap::Skip, 17).unwrap();

    let reopened = CardStore::open(path).unwrap();
    let taps = &reopened.turn_summary("t1").unwrap().taps;
    assert_eq!(
        taps,
        &[
            QuizTap {
                tap: Tap::Pick { option: 1 },
                at: 10
            },
            QuizTap {
                tap: Tap::ExplainMore,
                at: 13
            },
            QuizTap {
                tap: Tap::Understood,
                at: 15
            },
        ]
    );
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

#[test]
fn a_shown_tap_is_kept_once_on_a_card_and_on_a_turns_quiz_and_is_not_an_answer() {
    use studio_understanding::{QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let mut cards = two_cards(&data.path().join("cards.json"));
    cards.quiz_tap("toolu_01A", Tap::Shown, 3).unwrap();
    cards.quiz_tap("toolu_01A", Tap::Shown, 4).unwrap();
    cards
        .quiz_tap("toolu_01A", Tap::Pick { option: 0 }, 5)
        .unwrap();
    let a = &cards.cards()[0];
    assert_eq!(
        a.taps,
        [QuizTap {
            tap: Tap::Shown,
            at: 3
        }]
    );
    assert_eq!(a.guess.as_ref().map(|g| g.option), Some(0));
    let mut turns = lone_quiz(&data.path().join("turns.json"));
    turns.quiz_tap("t1", Tap::Shown, 6).unwrap();
    turns.quiz_tap("t1", Tap::Shown, 7).unwrap();
    turns.quiz_tap("t1", Tap::Pick { option: 1 }, 8).unwrap();
    let taps = &turns.turn_summary("t1").unwrap().taps;
    assert_eq!(taps.len(), 2);
    assert_eq!(taps[0].tap, Tap::Shown);
}

/// A store at `path` holding a lone Quiz on turn `t1` whose request carries the file's whole hunk.
fn quiz_with_hunk(path: &std::path::Path) -> CardStore {
    let mut store = CardStore::open(path.to_owned()).unwrap();
    add_quiz(&mut store, "t1", studio_understanding::millis());
    store
}

/// Keeps a lone Quiz on turn `id`, offered at `at`, whose request carries the file's whole hunk.
fn add_quiz(store: &mut CardStore, id: &str, at: u64) {
    use studio_core::{FactFile, Facts, SummaryRequest, TurnFile};
    let request = SummaryRequest {
        id: id.into(),
        session_id: "s1".into(),
        facts: Facts {
            files: vec![FactFile {
                file: TurnFile {
                    root: "/w".into(),
                    path: "src/gate.rs".into(),
                    from: None,
                    added: Some(1),
                    removed: Some(1),
                    agent: None,
                },
                hunk: "@@ -1 +1 @@\n-open\n+frozen\n".into(),
            }],
            ..Facts::default()
        },
        ..Default::default()
    };
    let summary: studio_understanding::TurnSummary = serde_json::from_value(serde_json::json!({
        "summary": "Froze the gate.",
        "quiz": { "text": "Why?", "key": 0, "options": [
            { "text": "A", "why": "Because the pairs must not move." }, { "text": "B", "why": "No." }] },
    }))
    .unwrap();
    store.add_summary_at(request, summary, at).unwrap();
}

const WEEK: u64 = 7 * 24 * 60 * 60 * 1000;

#[test]
fn a_quiz_offered_seven_days_ago_gets_one_retest_and_one_offered_six_days_ago_gets_none() {
    let data = tempfile::tempdir().unwrap();
    let mut store = quiz_with_hunk(&data.path().join("cards.json"));
    let offered = store.turn_summary("t1").unwrap().added_at;

    assert!(store.due_retests(offered + WEEK - 1).is_empty());
    let [due] = &store.due_retests(offered + WEEK)[..] else {
        panic!("one retest is due")
    };
    // The writer's input: the turn's stored hunk, and the first Quiz with the key's reason.
    assert_eq!(due.of, "t1");
    assert!(due.hunk.contains("src/gate.rs") && due.hunk.contains("+frozen"));
    assert_eq!(
        due.first.options[due.first.key].why,
        "Because the pairs must not move."
    );

    // Once written, or rejected, it is not due again.
    let ask = due.clone();
    store.add_retest(&ask, None, offered + WEEK).unwrap();
    assert!(store.due_retests(offered + WEEK + WEEK).is_empty());
}

#[test]
fn a_retests_confidence_why_and_pick_are_stored_with_their_times_and_survive_a_restart() {
    use studio_understanding::{Choice, Quiz, QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = quiz_with_hunk(&path);
    let offered = store.turn_summary("t1").unwrap().added_at;
    let ask = store.due_retests(offered + WEEK).remove(0);
    let quiz = Quiz {
        text: "What stays true?".into(),
        options: vec![
            Choice {
                text: "A".into(),
                why: "Yes.".into(),
            },
            Choice {
                text: "B".into(),
                why: "No.".into(),
            },
        ],
        key: 0,
    };
    store.add_retest(&ask, Some(quiz), offered + WEEK).unwrap();
    let id = "retest t1";
    store
        .quiz_tap(
            id,
            Tap::Confidence {
                sure: false,
                surface: Surface::Rail,
            },
            1,
        )
        .unwrap();
    store.quiz_tap(id, Tap::Pick { option: 1 }, 2).unwrap();
    store
        .quiz_tap(
            id,
            Tap::Why {
                text: "it freezes".into(),
            },
            3,
        )
        .unwrap();
    // Only the first of each counts, and no such option records nothing.
    store
        .quiz_tap(
            id,
            Tap::Confidence {
                sure: true,
                surface: Surface::Rail,
            },
            4,
        )
        .unwrap();
    store.quiz_tap(id, Tap::Pick { option: 0 }, 5).unwrap();

    let reopened = CardStore::open(path).unwrap();
    let [retest] = reopened.retests() else {
        panic!()
    };
    assert_eq!(
        retest.taps,
        [
            QuizTap {
                tap: Tap::Confidence {
                    sure: false,
                    surface: Surface::Rail
                },
                at: 1
            },
            QuizTap {
                tap: Tap::Pick { option: 1 },
                at: 2
            },
            QuizTap {
                tap: Tap::Why {
                    text: "it freezes".into()
                },
                at: 3
            },
        ]
    );
    // The first Quiz's own record is untouched.
    assert!(reopened.turn_summary("t1").unwrap().taps.is_empty());
}

#[test]
fn a_confidence_tap_and_a_why_line_on_a_lone_quiz_are_stored_too() {
    use studio_understanding::{QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let mut store = lone_quiz(&data.path().join("cards.json"));
    store
        .quiz_tap(
            "t1",
            Tap::Confidence {
                sure: true,
                surface: Surface::Rail,
            },
            1,
        )
        .unwrap();
    store
        .quiz_tap("t1", Tap::Why { text: "x".into() }, 2)
        .unwrap();
    assert_eq!(
        store.turn_summary("t1").unwrap().taps,
        [
            QuizTap {
                tap: Tap::Confidence {
                    sure: true,
                    surface: Surface::Rail
                },
                at: 1
            },
            QuizTap {
                tap: Tap::Why { text: "x".into() },
                at: 2
            },
        ]
    );
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

#[test]
fn a_card_past_the_cap_never_replaces_a_body_only_turns_quiz() {
    use studio_understanding::Place;
    let data = tempfile::tempdir().unwrap();
    let mut store = CardStore::open(data.path().join("cards.json")).unwrap();
    store.set_self_test(Some(START));
    // The body-only turn is the oldest and does not count toward the cap; three offered ones do.
    quiz_turn(&mut store, "hidden", START + 1, true);
    for (i, id) in ["a", "b", "c"].into_iter().enumerate() {
        quiz_turn(&mut store, id, START + 2 + i as u64, false);
    }
    assert_eq!(
        store.place_quiz(true, START + 10),
        Place::Replace("a".into())
    );
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
    let _store = quiz_with_hunk(&data.path().join("cards.json"));
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

// Home's learning slot (danielrispler/plotkeeper#240): the store chooses the item.
mod learning {
    use super::*;
    use studio_understanding::{
        Choice, Learning, Quiz, RETEST_AFTER_MS, RETEST_REACH_MS, RetestAsk, Tap,
    };

    const DAY: u64 = 24 * 60 * 60 * 1000;
    /// The start of "today" in these tests, and the clock: noon of that day.
    const START: u64 = 400 * DAY;
    const NOW: u64 = START + DAY / 2;

    fn quiz() -> Quiz {
        Quiz {
            text: "Why?".into(),
            options: vec![
                Choice {
                    text: "A".into(),
                    why: "Yes.".into(),
                },
                Choice {
                    text: "B".into(),
                    why: "No.".into(),
                },
            ],
            key: 0,
        }
    }

    /// Facts of one file whose hunk is `+the hunk`.
    fn hunk_facts() -> studio_core::Facts {
        studio_core::Facts {
            files: vec![studio_core::FactFile {
                file: studio_core::TurnFile {
                    root: "/r".into(),
                    path: "a.rs".into(),
                    from: None,
                    added: Some(1),
                    removed: Some(0),
                    agent: None,
                },
                hunk: "+the hunk".into(),
            }],
            ..Default::default()
        }
    }

    /// Turn `t` with a Quiz, offered so that its retest came due `due_ago` before NOW, and that Retest, `retest t`.
    fn seed(store: &mut CardStore, t: &str, due_ago: u64) {
        seed_offered(store, t, NOW - due_ago - RETEST_AFTER_MS);
    }

    /// Turn `t` with a Quiz offered at `offered`, and its Retest.
    fn seed_offered(store: &mut CardStore, t: &str, offered: u64) {
        let request = studio_core::SummaryRequest {
            id: t.into(),
            session_id: "s1".into(),
            facts: hunk_facts(),
            ..Default::default()
        };
        let summary: studio_understanding::TurnSummary =
            serde_json::from_value(serde_json::json!({
                "summary": "Did a thing.",
                "quiz": { "text": "Why?", "key": 0, "options": [
                    { "text": "A", "why": "Yes." }, { "text": "B", "why": "No." }] },
            }))
            .unwrap();
        store.add_summary_at(request, summary, offered).unwrap();
        let ask = RetestAsk {
            of: t.into(),
            session_id: "s1".into(),
            hunk: String::new(),
            first: quiz(),
        };
        store
            .add_retest(&ask, Some(quiz()), offered + RETEST_AFTER_MS)
            .unwrap();
    }

    fn empty() -> (tempfile::TempDir, CardStore) {
        let data = tempfile::tempdir().unwrap();
        let store = CardStore::open(data.path().join("cards.json")).unwrap();
        (data, store)
    }

    /// What the slot serves at `now`: the turn the Retest is of, "done", or the fresh Quiz's id.
    fn served_at(store: &CardStore, now: u64, start: u64) -> Option<String> {
        match store.learning(now, start) {
            Some(Learning::Retest(r)) => Some(r.of),
            Some(Learning::Done) => Some("done".into()),
            Some(Learning::Fresh { id, .. }) => Some(id),
            None => None,
        }
    }

    fn served(store: &CardStore) -> Option<String> {
        served_at(store, NOW, START)
    }

    #[test]
    fn the_latest_due_retest_is_served_and_nothing_is_served_when_none_is_eligible() {
        let (_data, mut store) = empty();
        assert_eq!(served(&store), None);
        seed(&mut store, "old", 3 * DAY);
        seed(&mut store, "new", DAY);
        assert_eq!(served(&store).as_deref(), Some("new"));
    }

    #[test]
    fn a_served_retest_carries_the_date_its_first_quiz_was_offered() {
        let (_data, mut store) = empty();
        seed(&mut store, "a", DAY);
        let Some(Learning::Retest(r)) = store.learning(NOW, START) else {
            panic!()
        };
        assert_eq!(r.of_at, Some(NOW - DAY - RETEST_AFTER_MS));
    }

    fn pick(store: &mut CardStore, id: &str, at: u64) {
        store.quiz_tap(id, Tap::Pick { option: 0 }, at).unwrap();
    }

    #[test]
    fn a_retest_of_an_answered_quiz_beats_a_later_due_one_of_an_unanswered_quiz() {
        let (_data, mut store) = empty();
        seed(&mut store, "answered", 5 * DAY);
        seed(&mut store, "answered too", 4 * DAY);
        seed(&mut store, "unanswered", DAY);
        pick(&mut store, "answered", START - 6 * DAY);
        pick(&mut store, "answered too", START - 6 * DAY);
        // Within the group the latest due comes first.
        assert_eq!(served(&store).as_deref(), Some("answered too"));
        store
            .quiz_tap("retest answered too", Tap::NotNow, NOW)
            .unwrap();
        assert_eq!(served(&store).as_deref(), Some("answered"));
        store.quiz_tap("retest answered", Tap::NotNow, NOW).unwrap();
        assert_eq!(served(&store).as_deref(), Some("unanswered"));
    }

    #[test]
    fn a_retest_due_more_than_14_days_ago_is_not_served() {
        let (_data, mut store) = empty();
        seed(&mut store, "late", RETEST_REACH_MS + 1);
        assert_eq!(served(&store), None);
        seed(&mut store, "edge", RETEST_REACH_MS);
        assert_eq!(served(&store).as_deref(), Some("edge"));
    }

    #[test]
    fn a_retest_not_yet_due_is_not_served() {
        let (_data, mut store) = empty();
        // Offered six days ago: due tomorrow.
        seed_offered(&mut store, "early", NOW - 6 * DAY);
        assert_eq!(served(&store), None);
    }

    #[test]
    fn a_retest_whose_first_quiz_is_flagged_key_wrong_is_not_served() {
        let (_data, mut store) = empty();
        seed(&mut store, "wrong", DAY);
        store.quiz_tap("wrong", Tap::KeyWrong, START - DAY).unwrap();
        assert_eq!(served(&store), None);
    }

    #[test]
    fn a_picked_retest_is_not_served() {
        let (_data, mut store) = empty();
        seed(&mut store, "a", DAY);
        pick(&mut store, "retest a", NOW);
        assert_eq!(served(&store), None);
    }

    #[test]
    fn after_a_not_now_the_item_waits_for_the_next_day_and_the_tap_does_not_count_toward_the_cap() {
        let (_data, mut store) = empty();
        for (i, t) in ["a", "b", "c", "d"].iter().enumerate() {
            seed(&mut store, t, (i as u64 + 1) * DAY / 4);
        }
        for t in ["a", "b", "c"] {
            assert_eq!(served(&store).as_deref(), Some(t));
            store
                .quiz_tap(&format!("retest {t}"), Tap::NotNow, NOW)
                .unwrap();
        }
        // Three taps of "Not now" are not three answers: the fourth item is served, not "done for today".
        assert_eq!(served(&store).as_deref(), Some("d"));
        pick(&mut store, "retest d", NOW);
        assert_eq!(served(&store), None);
        // The next day the others are served again.
        assert_eq!(
            served_at(&store, NOW + DAY, START + DAY).as_deref(),
            Some("a")
        );
        // And can be set aside again that day.
        store.quiz_tap("retest a", Tap::NotNow, NOW + DAY).unwrap();
        assert_eq!(
            served_at(&store, NOW + DAY, START + DAY).as_deref(),
            Some("b")
        );
    }

    #[test]
    fn after_three_picked_retests_it_is_done_for_today_and_the_count_resets_at_the_next_day() {
        let (_data, mut store) = empty();
        for (i, t) in ["a", "b", "c", "d"].iter().enumerate() {
            seed(&mut store, t, (i as u64 + 1) * DAY / 4);
        }
        for t in ["a", "b"] {
            pick(&mut store, &format!("retest {t}"), START + 1);
        }
        assert_eq!(served(&store).as_deref(), Some("c"));
        pick(&mut store, "retest c", START + 2);
        assert_eq!(served(&store).as_deref(), Some("done"));
        assert_eq!(
            served_at(&store, NOW + DAY, START + DAY).as_deref(),
            Some("d")
        );
    }

    #[test]
    fn a_pick_after_a_not_now_still_counts_on_a_card_a_summary_and_a_retest() {
        let data = tempfile::tempdir().unwrap();
        let mut store = two_cards(&data.path().join("cards.json"));
        store.quiz_tap("toolu_01A", Tap::NotNow, 1).unwrap();
        store
            .quiz_tap("toolu_01A", Tap::Pick { option: 1 }, 2)
            .unwrap();
        assert_eq!(
            store
                .cards()
                .iter()
                .find(|c| c.request.id == "toolu_01A")
                .unwrap()
                .guess
                .as_ref()
                .map(|g| g.option),
            Some(1)
        );

        let (_data, mut store) = empty();
        seed(&mut store, "t", DAY);
        for id in ["t", "retest t"] {
            store.quiz_tap(id, Tap::NotNow, 1).unwrap();
            store.quiz_tap(id, Tap::NotNow, 2).unwrap();
            pick(&mut store, id, 3);
        }
        let kinds = |taps: &[studio_understanding::QuizTap]| {
            taps.iter().map(|t| t.tap.clone()).collect::<Vec<_>>()
        };
        let expected = [Tap::NotNow, Tap::NotNow, Tap::Pick { option: 0 }];
        assert_eq!(kinds(&store.turn_summary("t").unwrap().taps), expected);
        assert_eq!(kinds(&store.retests()[0].taps), expected);
    }

    const MIN: u64 = 60 * 1000;

    /// A turn `t` with a Quiz whose turn ended `ago` before NOW.
    fn fresh_summary(store: &mut CardStore, t: &str, ago: u64) {
        let request = studio_core::SummaryRequest {
            id: t.into(),
            session_id: "s1".into(),
            ..Default::default()
        };
        let summary: studio_understanding::TurnSummary =
            serde_json::from_value(serde_json::json!({
                "summary": "Did a thing.",
                "quiz": { "text": "Why?", "key": 0, "options": [
                    { "text": "A", "why": "Yes." }, { "text": "B", "why": "No." }] },
            }))
            .unwrap();
        store.add_summary_at(request, summary, NOW - ago).unwrap();
    }

    /// A card `id` of session `session` (a worker's, say) whose Quiz was added `ago` before NOW.
    fn fresh_card(store: &mut CardStore, id: &str, session: &str, ago: u64) {
        let mut request = super::edit(id, "src/lib.rs");
        request.session_id = session.into();
        let card = super::card(&request);
        store.add(request, card, NOW - ago).unwrap();
    }

    #[test]
    fn with_no_retest_picked_today_an_eligible_retest_beats_a_fresh_quiz_and_after_one_the_fresh_quiz_comes_first()
     {
        let (_data, mut store) = empty();
        seed(&mut store, "a", DAY);
        seed(&mut store, "b", 2 * DAY);
        fresh_summary(&mut store, "now", 5 * MIN);
        assert_eq!(served(&store).as_deref(), Some("a"));
        pick(&mut store, "retest a", NOW);
        assert_eq!(served(&store).as_deref(), Some("now"));
        // Answered, it gives way to the next Retest.
        pick(&mut store, "now", NOW);
        assert_eq!(served(&store).as_deref(), Some("b"));
    }

    #[test]
    fn a_fresh_quiz_is_served_with_no_retest_eligible_and_even_after_the_cap_and_newest_first() {
        let (_data, mut store) = empty();
        fresh_summary(&mut store, "older", 20 * MIN);
        fresh_summary(&mut store, "newer", 2 * MIN);
        assert_eq!(served(&store).as_deref(), Some("newer"));
        for t in ["a", "b", "c"] {
            seed(&mut store, t, DAY);
            pick(&mut store, &format!("retest {t}"), START + 1);
        }
        assert_eq!(served(&store).as_deref(), Some("newer"));
        pick(&mut store, "newer", NOW);
        pick(&mut store, "older", NOW);
        assert_eq!(served(&store).as_deref(), Some("done"));
    }

    #[test]
    fn a_worker_cards_fresh_quiz_counts_and_a_turn_ended_31_minutes_ago_is_not_fresh() {
        let (_data, mut store) = empty();
        fresh_card(&mut store, "worker card", "worker 1", 10 * MIN);
        fresh_summary(&mut store, "late", 31 * MIN);
        assert_eq!(served(&store).as_deref(), Some("worker card"));
        pick(&mut store, "worker card", NOW);
        assert_eq!(served(&store), None);
        fresh_summary(&mut store, "edge", 30 * MIN);
        assert_eq!(served(&store).as_deref(), Some("edge"));
    }

    #[test]
    fn a_retest_skipped_by_an_older_build_counts_as_answered_and_is_never_served() {
        let (_data, mut store) = empty();
        seed(&mut store, "skipped", DAY);
        seed(&mut store, "next", 2 * DAY);
        store.quiz_tap("retest skipped", Tap::Skip, NOW).unwrap();
        assert_eq!(served(&store).as_deref(), Some("next"));
    }

    #[test]
    fn a_fresh_quiz_is_fresh_by_when_its_turn_ended_not_when_it_was_written() {
        let (_data, mut store) = empty();
        // Written now, but the turn (named by the Stop's time after the `@`) ended two days ago.
        let old = NOW - 2 * DAY;
        let request = studio_core::SummaryRequest {
            id: format!("s1@{old}"),
            session_id: "s1".into(),
            ..Default::default()
        };
        let summary: studio_understanding::TurnSummary = serde_json::from_value(serde_json::json!({
            "summary": "Did a thing.",
            "quiz": { "text": "Why?", "key": 0, "options": [
                { "text": "A", "why": "Yes." }, { "text": "B", "why": "No." }] },
        }))
        .unwrap();
        store.add_summary_at(request, summary.clone(), NOW).unwrap();
        assert_eq!(served(&store), None);
        let request = studio_core::SummaryRequest {
            id: format!("s1@{}", NOW - 5 * MIN),
            session_id: "s1".into(),
            ..Default::default()
        };
        store.add_summary_at(request, summary, NOW).unwrap();
        assert_eq!(served(&store), Some(format!("s1@{}", NOW - 5 * MIN)));
    }

    #[test]
    fn a_fresh_quiz_carries_the_time_its_turn_ended_and_its_stored_hunk() {
        let (_data, mut store) = empty();
        let at = NOW - 5 * MIN;
        let request = studio_core::SummaryRequest {
            id: format!("s1@{at}"),
            session_id: "s1".into(),
            facts: hunk_facts(),
            ..Default::default()
        };
        let summary: studio_understanding::TurnSummary = serde_json::from_value(serde_json::json!({
            "summary": "Did a thing.",
            "quiz": { "text": "Why?", "key": 0, "options": [
                { "text": "A", "why": "Yes." }, { "text": "B", "why": "No." }] },
        }))
        .unwrap();
        store.add_summary_at(request, summary, NOW).unwrap();
        let Some(Learning::Fresh { at: got, hunk, .. }) = store.learning(NOW, START) else {
            panic!()
        };
        assert_eq!((got, hunk.as_str()), (at, "--- a.rs\n+the hunk"));
    }

    #[test]
    fn a_served_retest_carries_the_stored_hunk_of_its_first_quiz() {
        let (_data, mut store) = empty();
        seed(&mut store, "a", DAY);
        let Some(Learning::Retest(r)) = store.learning(NOW, START) else {
            panic!()
        };
        assert_eq!(r.hunk, "--- a.rs\n+the hunk");
    }

    #[test]
    fn a_picked_skipped_or_set_aside_fresh_quiz_is_not_served() {
        let (_data, mut store) = empty();
        fresh_summary(&mut store, "picked", MIN);
        fresh_summary(&mut store, "skipped", 2 * MIN);
        fresh_summary(&mut store, "aside", 3 * MIN);
        fresh_card(&mut store, "card aside", "s1", 4 * MIN);
        fresh_summary(&mut store, "open", 5 * MIN);
        pick(&mut store, "picked", NOW);
        store.quiz_tap("skipped", Tap::Skip, NOW).unwrap();
        store.quiz_tap("aside", Tap::NotNow, NOW).unwrap();
        store.quiz_tap("card aside", Tap::NotNow, NOW).unwrap();
        assert_eq!(served(&store).as_deref(), Some("open"));
    }
}

#[test]
fn a_confidence_tap_keeps_its_surface_and_one_without_reads_as_the_rails() {
    use studio_understanding::{QuizTap, Tap};
    let data = tempfile::tempdir().unwrap();
    let path = data.path().join("cards.json");
    let mut store = lone_quiz(&path);
    store
        .quiz_tap(
            "t1",
            Tap::Confidence {
                sure: false,
                surface: Surface::Home,
            },
            1,
        )
        .unwrap();
    let reopened = CardStore::open(path).unwrap();
    assert_eq!(
        reopened.turn_summary("t1").unwrap().taps,
        [QuizTap {
            tap: Tap::Confidence {
                sure: false,
                surface: Surface::Home
            },
            at: 1
        }]
    );
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

/// A retest's Quiz of two options.
fn retest_quiz() -> studio_understanding::Quiz {
    use studio_understanding::{Choice, Quiz};
    let option = |text: &str| Choice {
        text: text.into(),
        why: "w".into(),
    };
    Quiz {
        text: "What stays true?".into(),
        options: vec![option("A"), option("B")],
        key: 0,
    }
}

const NOW: u64 = 1_790_000_000_000;

/// A store at `path` with Quizzes due before `NOW`: `fresh` 1 day, `edge` 14 days and `old` 15 days before; and, 1 day
/// before, `flagged`, a turn's own, and `card`, a card's, both flagged Key wrong.
fn retest_cases(path: &std::path::Path) -> CardStore {
    use studio_understanding::Tap;
    let mut store = CardStore::open(path.to_owned()).unwrap();
    for (id, due) in [
        ("fresh", NOW - DAY),
        ("edge", NOW - 14 * DAY),
        ("old", NOW - 15 * DAY),
        ("flagged", NOW - DAY),
    ] {
        add_quiz(&mut store, id, due - WEEK);
    }
    store.quiz_tap("flagged", Tap::KeyWrong, NOW - DAY).unwrap();
    let request = edit("card", "src/lib.rs");
    let card = card(&request);
    store.add(request, card, NOW - DAY - WEEK).unwrap();
    store.key_wrong("card").unwrap();
    store
}

#[test]
fn a_retest_waits_unanswered_while_home_could_serve_it() {
    use studio_understanding::Tap;
    let data = tempfile::tempdir().unwrap();
    let mut store = retest_cases(&data.path().join("cards.json"));
    for ask in store.due_retests(NOW) {
        store.add_retest(&ask, Some(retest_quiz()), NOW).unwrap();
    }
    // Of the five, only `fresh` and `edge` wait: `old` came due 15 days ago, and the others' keys are flagged wrong.
    assert_eq!(store.waiting_retests(NOW), 2);

    // A shown retest still waits; a picked one does not.
    store.quiz_tap("retest fresh", Tap::Shown, NOW).unwrap();
    assert_eq!(store.waiting_retests(NOW), 2);
    store
        .quiz_tap("retest fresh", Tap::Pick { option: 1 }, NOW)
        .unwrap();
    assert_eq!(store.waiting_retests(NOW), 1);
    // Nor does a skipped one, which can no longer be picked.
    store.quiz_tap("retest edge", Tap::Skip, NOW).unwrap();
    assert_eq!(store.waiting_retests(NOW), 0);

    // A retest kept with no Quiz, after the writer broke the guards twice, does not wait.
    add_quiz(&mut store, "rejected", NOW - DAY - WEEK);
    let ask = store.due_retests(NOW).remove(0);
    store.add_retest(&ask, None, NOW).unwrap();
    assert_eq!(store.waiting_retests(NOW), 0);
}

#[test]
fn no_retest_is_written_for_a_quiz_due_15_days_ago_or_flagged_key_wrong() {
    let data = tempfile::tempdir().unwrap();
    let store = retest_cases(&data.path().join("cards.json"));

    let asks = store.retests_to_write(NOW);
    let ofs: Vec<&str> = asks.iter().map(|ask| ask.of.as_str()).collect();
    assert_eq!(ofs, ["fresh", "edge"]);
}

#[test]
fn a_tap_of_a_kind_this_build_does_not_know_reads_without_error() {
    use studio_understanding::{QuizTap, Tap};
    let tap: QuizTap = serde_json::from_str(r#"{"kind":"from_the_future","at":7,"x":1}"#).unwrap();
    assert_eq!((tap.tap, tap.at), (Tap::Unknown, 7));
}

#[test]
fn a_shown_tap_on_a_card_marks_its_quiz_shown_as_the_rails_show_does_and_never_after_a_guess() {
    use studio_understanding::Tap;
    let data = tempfile::tempdir().unwrap();
    let mut store = two_cards(&data.path().join("cards.json"));
    let shown = |store: &CardStore, id: &str| {
        store.cards().iter().find(|c| c.request.id == id).unwrap().shown_at
    };
    store.quiz_tap("toolu_01A", Tap::Shown, 5).unwrap();
    assert_eq!(shown(&store, "toolu_01A"), Some(5));
    store.quiz_tap("toolu_01A", Tap::Shown, 9).unwrap();
    assert_eq!(shown(&store, "toolu_01A"), Some(5));
    store.quiz_tap("toolu_01B", Tap::Pick { option: 0 }, 6).unwrap();
    store.quiz_tap("toolu_01B", Tap::Shown, 7).unwrap();
    assert_eq!(shown(&store, "toolu_01B"), None);
}
