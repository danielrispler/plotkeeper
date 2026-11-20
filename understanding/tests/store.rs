//! The card store (danielrispler/Falinks#328, module 6; danielrispler/Falinks#369, decisions 15 and 17): cards, their base, Daniel's guesses,
//! his flags, his ratings and the longer explanations he asked for survive a restart.

use std::{cell::Cell, sync::Mutex};
use studio_core::{Base, CardRequest, Change, LiveFacts, Output, Studio};
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

/// A store at `path` holding a lone Quiz on turn `t1` whose request carries the file's whole hunk.
fn quiz_with_hunk(path: &std::path::Path) -> CardStore {
    use studio_core::{FactFile, Facts, SummaryRequest, TurnFile};
    let mut store = CardStore::open(path.to_owned()).unwrap();
    let request = SummaryRequest {
        id: "t1".into(),
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
    store.add_summary(request, summary).unwrap();
    store
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
        .quiz_tap(id, Tap::Confidence { sure: false }, 1)
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
        .quiz_tap(id, Tap::Confidence { sure: true }, 4)
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
                tap: Tap::Confidence { sure: false },
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
        .quiz_tap("t1", Tap::Confidence { sure: true }, 1)
        .unwrap();
    store
        .quiz_tap("t1", Tap::Why { text: "x".into() }, 2)
        .unwrap();
    assert_eq!(
        store.turn_summary("t1").unwrap().taps,
        [
            QuizTap {
                tap: Tap::Confidence { sure: true },
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
        [("w1@9".to_string(), Some(0)), ("w1@11".to_string(), Some(2))]
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
