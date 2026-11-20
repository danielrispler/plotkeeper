//! Turn writing (danielrispler/plotkeeper#250, Testing Decisions), through its interface alone: the requests the core emits go
//! in, and what Daniel would get comes out, in the card store, the offers record and the callback.
//! Behind the seam: the fake writer, a core rebuilt by hook lines, a real card store and offers record in a
//! temporary folder, real temporary git repositories, and a callback that records what it is told.
//! The jobs run in order on the test's thread, through the code a queue's thread runs
//! (`TurnWriting::stepped`); the threaded module has its own tests where the thread is the point.
//! A test that expects a card edits two logic files, so the real offer gives one.

use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use studio_core::{
    Backfill, CardRequest, Kind, NameRequest, Output, Studio, SummaryRequest, Trees,
};
use studio_hook::record_tree;
use studio_understanding::{
    Card, CardStore, CardWriter, Core, Decision, Effects, FakeWriter, Fingerprints, Inputs, Offer,
    OfferKind, Offers, Outcome, StoredSummary, SummarySetup, TurnSummary, TurnWriting, Writer,
    fingerprint_changes, worktree_root,
};

/// What the callback and the writer were told, in order, as short lines.
type Events = Arc<Mutex<Vec<String>>>;

/// The fake writer, telling the test's events what it wrote, and failing its summaries on demand.
struct Recorded {
    events: Events,
    failing: Arc<AtomicBool>,
    summaries: Arc<AtomicUsize>,
    /// Whether a summary has a line of why, which Design so far is written from.
    why: Arc<AtomicBool>,
}

impl CardWriter for Recorded {
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        self.events.lock().unwrap().push("card written".into());
        FakeWriter.reply(request, earlier)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }

    fn goal(&self, prompts: &[&str]) -> Result<String, String> {
        self.events.lock().unwrap().push("goal written".into());
        FakeWriter.goal(prompts)
    }

    fn design(&self, sources: &[(String, String)]) -> Result<String, String> {
        self.events.lock().unwrap().push("design written".into());
        FakeWriter.design(sources)
    }

    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        self.summaries.fetch_add(1, Ordering::SeqCst);
        if self.failing.load(Ordering::SeqCst) {
            return Err("the writer is away".into());
        }
        self.events.lock().unwrap().push("summary written".into());
        let reply = FakeWriter.summarize(setup, request, earlier)?;
        if !self.why.load(Ordering::SeqCst) {
            return Ok(reply);
        }
        let mut value: Value = serde_json::from_str(&reply).unwrap();
        let why = json!({ "kind": "why", "text": "It was done this way, on purpose." });
        value["lines"].as_array_mut().unwrap().push(why);
        Ok(value.to_string())
    }

    fn name(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
    ) -> Result<String, String> {
        FakeWriter.name(setup, prompt, response)
    }
}

/// The shell's four effects, recorded with the number of cards in the store when each was told.
struct Recorder {
    events: Events,
    store: Arc<Mutex<CardStore>>,
    logs: Mutex<Vec<String>>,
}

impl Effects for Recorder {
    fn list(&self) {
        let cards = self.store.lock().unwrap().cards().len();
        self.events
            .lock()
            .unwrap()
            .push(format!("list, {cards} cards"));
    }

    fn new_card(&self, session_id: &str, agent: Option<&str>) {
        let line = format!("notice on {session_id}, agent {agent:?}");
        self.events.lock().unwrap().push(line);
    }

    fn settle_checks(&self) {
        self.events.lock().unwrap().push("settle checks".into());
    }

    fn name(&self, session_id: &str, name: &str) {
        let line = format!("name of {session_id}: {name}");
        self.events.lock().unwrap().push(line);
    }

    fn log(&self, line: String) {
        self.logs.lock().unwrap().push(line);
    }
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// The Unix time now, in milliseconds.
fn now_ms() -> u64 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis() as u64)
}

/// A reply long enough that its turn's summary is wanted for itself.
fn long_reply() -> String {
    "word ".repeat(160)
}

/// One day of session `s1` in one repository, written through Turn writing.
struct Day {
    core: Core,
    fingerprints: HashMap<String, Fingerprints>,
    repo: PathBuf,
    data: PathBuf,
    at: u64,
    /// The turns so far, which name the prompts.
    turns: usize,
    store: Arc<Mutex<CardStore>>,
    events: Events,
    recorder: Arc<Recorder>,
    failing: Arc<AtomicBool>,
    summaries: Arc<AtomicUsize>,
    why: Arc<AtomicBool>,
    writing: TurnWriting,
    _dirs: [tempfile::TempDir; 2],
}

impl Day {
    /// A day on a new repository with one commit, an empty store and no offers on record.
    fn new() -> Day {
        Day::with_writer(|recorded| Arc::new(recorded))
    }

    /// A day as `new` makes it, whose writer is `wrap` around the recording one.
    fn with_writer(wrap: impl FnOnce(Recorded) -> Writer) -> Day {
        let (code, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let repo = code.path().to_owned();
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("lib.rs"), "fn a() {}\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "Start"]);
        let core: Core = Arc::new(Mutex::new(Studio::with_roots(worktree_root)));
        let path = data.path().join("cards.json");
        let store = Arc::new(Mutex::new(CardStore::open(path).unwrap()));
        let events = Events::default();
        let recorder = Arc::new(Recorder {
            events: events.clone(),
            store: store.clone(),
            logs: Mutex::default(),
        });
        let (failing, summaries) = (Arc::new(AtomicBool::new(false)), Arc::default());
        let why = Arc::new(AtomicBool::new(false));
        let writer = wrap(Recorded {
            events: events.clone(),
            failing: failing.clone(),
            summaries: Arc::clone(&summaries),
            why: why.clone(),
        });
        let offers = Offers::beside(&data.path().join("events.jsonl"));
        let writing = TurnWriting::stepped(
            core.clone(),
            store.clone(),
            writer,
            offers,
            recorder.clone(),
        );
        Day {
            core,
            fingerprints: HashMap::new(),
            data: data.path().to_owned(),
            repo,
            at: now_ms(),
            turns: 0,
            store,
            events,
            recorder,
            failing,
            summaries,
            why,
            writing,
            _dirs: [code, data],
        }
    }

    /// Takes event `name`, with `fields` added, and returns what the core asks for.
    fn event(&mut self, name: &str, fields: Value) -> Vec<Output> {
        // The clock follows the real one, which a worktree's creation time is read against.
        self.at = (self.at + 1).max(now_ms());
        let mut event = json!({"session_id": "s1", "hook_event_name": name, "cwd": self.repo,
            "studio_received_ms": self.at});
        event
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let line = event.to_string();
        let looks = self.core.lock().unwrap().fingerprint_folders(&line);
        let changes = fingerprint_changes(looks, &mut self.fingerprints);
        self.core.lock().unwrap().apply_with_changes(&line, changes)
    }

    /// Passes `outputs` to Turn writing as the shell does, then runs its jobs in order.
    fn pass(&self, outputs: Vec<Output>) {
        for output in outputs {
            match output {
                Output::Card(request) => self.writing.card(request, true),
                Output::Brief(request) => self.writing.brief(request),
                Output::Summary(request) => self.writing.summary(request),
                Output::Name(request) => self.writing.name(request),
                _ => {}
            }
        }
        self.writing.run_queued();
    }

    /// Daniel's prompt `text`, the `files` written and committed, each with its number of lines of
    /// code, and the turn's `Stop` with final reply `reply`; returns what the two asked for.
    fn turn_asking(&mut self, text: &str, files: &[(&str, usize)], reply: &str) -> Vec<Output> {
        self.turns += 1;
        let prompt_id = format!("p{}", self.turns);
        let prompt = json!({ "prompt": text, "prompt_id": prompt_id });
        let mut outputs = self.event("UserPromptSubmit", prompt);
        for (path, lines) in files {
            let code: String = (0..*lines).map(|i| format!("let v{i} = {i};\n")).collect();
            fs::write(self.repo.join(path), code).unwrap();
        }
        git(&self.repo, &["add", "."]);
        git(&self.repo, &["commit", "-qm", text]);
        outputs.extend(self.event(
            "Stop",
            json!({ "last_assistant_message": reply, "prompt_id": prompt_id }),
        ));
        outputs
    }

    /// A turn passed to Turn writing, with its jobs run.
    fn turn(&mut self, text: &str, files: &[(&str, usize)], reply: &str) {
        let outputs = self.turn_asking(text, files, reply);
        self.pass(outputs);
    }

    /// The ids of the cards in the store, oldest first.
    fn card_ids(&self) -> Vec<String> {
        let store = self.store.lock().unwrap();
        store.cards().iter().map(|k| k.request.id.clone()).collect()
    }

    /// The lines of the offers record.
    fn offers(&self) -> Vec<Value> {
        let text = fs::read_to_string(self.data.join("offers.jsonl")).unwrap_or_default();
        text.lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }

    fn summary(&self, id: &str) -> Option<StoredSummary> {
        self.store.lock().unwrap().turn_summary(id).cloned()
    }
}

/// The id of the card among `outputs`.
fn card_id(outputs: &[Output]) -> String {
    let mut cards = outputs.iter().filter_map(|o| match o {
        Output::Card(request) => Some(request.id.clone()),
        _ => None,
    });
    cards.next_back().expect("a card request")
}

#[test]
fn a_turns_card_then_its_summary_with_the_summary_taking_the_cards_decision_and_one_line_on_record()
{
    let mut day = Day::new();
    day.turn("Add two files", &[("a.rs", 3), ("b.rs", 3)], &long_reply());

    let ids = day.card_ids();
    assert_eq!(ids.len(), 1);
    let summary = day.summary(&ids[0]).expect("the turn's summary");
    // The card's decision was a card, so the summary gets no lone Quiz.
    assert!(summary.request.no_quiz);
    assert_eq!(summary.summary.quiz, None);
    let lines = day.offers();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["id"], ids[0].as_str());
    assert_eq!(
        (lines[0]["offer"].as_str(), lines[0]["outcome"].as_str()),
        (Some("card"), Some("written"))
    );
}

/// The Quizzes of the store: its cards' and its summaries'.
fn quizzes(day: &Day) -> usize {
    let store = day.store.lock().unwrap();
    let cards = store.cards().iter().filter(|k| k.card.quiz.is_some());
    cards.count()
        + store
            .summaries()
            .iter()
            .filter(|k| k.summary.quiz.is_some())
            .count()
}

/// `outputs` as the core gives them for a turn that ended in Needs you.
fn ended_asking(outputs: Vec<Output>) -> Vec<Output> {
    let asking = |output| match output {
        Output::Card(request) => Output::Card(CardRequest {
            asks: true,
            ..request
        }),
        Output::Summary(request) => Output::Summary(SummaryRequest {
            asks: true,
            ..request
        }),
        other => other,
    };
    outputs.into_iter().map(asking).collect()
}

#[test]
fn a_card_of_a_turn_that_ended_in_needs_you_is_stored_without_a_quiz_and_displaces_no_lone_quiz() {
    let mut day = Day::new();
    for (i, file) in ["x1.rs", "x2.rs", "x3.rs"].iter().enumerate() {
        day.turn(&format!("Lone {i}"), &[(file, 10)], "Done.");
    }
    assert_eq!(quizzes(&day), 3);

    let outputs = day.turn_asking("Add two files", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let id = card_id(&outputs);
    day.pass(ended_asking(outputs));

    let store = day.store.lock().unwrap();
    let card = store.cards().iter().find(|k| k.request.id == id).unwrap();
    assert_eq!(card.card.quiz, None);
    drop(store);
    assert_eq!(
        quizzes(&day),
        3,
        "the count is unchanged and none displaced"
    );
}

#[test]
fn two_sessions_whose_cards_arrive_at_the_caps_edge_offer_one_more_quiz_not_two() {
    let mut day = Day::new();
    // Two card Quizzes, which a later card does not displace.
    for (i, files) in [["a.rs", "b.rs"], ["c.rs", "d.rs"]].iter().enumerate() {
        let files = files.map(|f| (f, 3));
        day.turn(&format!("Card {i}"), &files, "Done.");
    }
    assert_eq!(quizzes(&day), 2);
    // Two more sessions, each with a card-sized change and a turn of its own.
    let mut requests = Vec::new();
    for session in ["s2", "s3"] {
        for name in ["UserPromptSubmit", "Stop"] {
            day.at += 1;
            let line = json!({"session_id": session, "hook_event_name": name, "cwd": day.repo,
                "studio_received_ms": day.at, "prompt": "Go", "prompt_id": "p1",
                "last_assistant_message": "Done."});
            day.core.lock().unwrap().apply(&line.to_string());
        }
        requests.push(CardRequest {
            session_id: session.into(),
            id: format!("{session}@{}", day.at),
            change: Some(diff(&["a.rs", "b.rs"])),
            ..CardRequest::default()
        });
    }
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(Recorded {
            events: day.events.clone(),
            failing: Arc::default(),
            summaries: Arc::default(),
            why: Arc::default(),
        }),
        Offers::beside(&day.data.join("events.jsonl")),
        day.recorder.clone(),
    );
    // Each session's queue has its own thread, so the two cards are written at the same time.
    for request in requests {
        threaded.card(request, true);
    }
    threaded.prune(&Default::default());
    for _ in 0..500 {
        if day.card_ids().len() == 4 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(day.card_ids().len(), 4);
    assert_eq!(quizzes(&day), 3, "no fourth Quiz is offered in the hour");
    let store = day.store.lock().unwrap();
    let kept = |session: &str| {
        let cards = store
            .cards()
            .iter()
            .filter(|k| k.request.session_id == session);
        cards.filter(|k| k.card.quiz.is_some()).count()
    };
    assert_eq!(
        kept("s2") + kept("s3"),
        1,
        "exactly one of the two cards keeps its Quiz"
    );
}

#[test]
fn a_card_of_a_turn_that_ended_at_your_turn_keeps_its_quiz_though_its_session_needs_daniel_when_its_job_runs()
 {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add two files", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let id = card_id(&outputs);
    // Its session asks for a permission after the turn ended, before the job runs.
    day.event(
        "PermissionRequest",
        json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "t1"}),
    );
    assert!(day.core.lock().unwrap().session("s1").unwrap().needs_you());
    day.pass(outputs);

    let store = day.store.lock().unwrap();
    let card = store.cards().iter().find(|k| k.request.id == id).unwrap();
    assert!(card.card.quiz.is_some());
}

#[test]
fn a_summary_of_a_turn_that_ended_in_needs_you_loses_its_quiz_and_its_cards() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add two files", &[("a.rs", 3), ("b.rs", 3)], &long_reply());
    let id = card_id(&outputs);
    day.pass(ended_asking(outputs));

    assert_eq!(day.summary(&id).expect("its summary").summary.quiz, None);
    assert_eq!(quizzes(&day), 0);
}

#[test]
fn a_summary_takes_its_cards_decision_though_the_turns_facts_moved_after_the_card() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], "Done.");
    let id = card_id(&outputs);
    let (cards, summaries): (Vec<_>, Vec<_>) = outputs
        .into_iter()
        .partition(|o| matches!(o, Output::Card(_)));
    // The card's decision is a lone Quiz, which its summary carries.
    day.pass(cards);
    // The turn then reads as a merge turn, which the table alone gives no Quiz.
    day.event(
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_input": {"command": "git merge main"}, "tool_use_id": "t9"}),
    );
    day.pass(summaries);

    let summary = day.summary(&id).expect("the lone Quiz's summary");
    assert!(summary.summary.quiz.is_some());
}

#[test]
fn a_small_change_gets_no_card_and_a_lone_quiz_on_its_summary_with_one_line_on_record() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], "Done.");
    let id = card_id(&outputs);
    day.pass(outputs);

    assert_eq!(day.card_ids(), Vec::<String>::new());
    let summary = day.summary(&id).expect("the lone Quiz's summary");
    assert!(summary.summary.quiz.is_some());
    let lines = day.offers();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        (lines[0]["offer"].as_str(), lines[0]["outcome"].as_str()),
        (Some("quiz"), Some("written"))
    );
}

#[test]
fn the_list_and_the_new_card_notice_go_out_once_the_card_is_stored_and_before_the_goal_is_written()
{
    let mut day = Day::new();
    day.turn("Add two files", &[("a.rs", 3), ("b.rs", 3)], "Done.");

    let events = day.events();
    let card = events.iter().position(|e| e == "card written").unwrap();
    let goal = events.iter().position(|e| e == "goal written").unwrap();
    assert_eq!(
        events[card + 1..goal],
        ["settle checks", "list, 1 cards", "notice on s1, agent None",]
    );
}

#[test]
fn jobs_run_in_the_order_they_were_asked_for_on_the_session_queue() {
    let mut day = Day::new();
    // Two turns' requests are queued before any job runs.
    let first = day.turn_asking("One", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let second = day.turn_asking("Two", &[("c.rs", 3), ("d.rs", 3)], "Done.");
    let (a, b) = (card_id(&first), card_id(&second));
    for output in first.into_iter().chain(second) {
        if let Output::Card(request) = output {
            day.writing.card(request, true);
        }
    }
    assert_eq!(
        day.card_ids(),
        Vec::<String>::new(),
        "nothing runs before the test runs it"
    );
    day.writing.run_queued();

    assert_eq!(day.card_ids(), [a, b]);
}

/// Summaries are written on open, and the turn of a small change waits.
fn on_open(day: &mut Day) -> String {
    day.writing.settings(SummarySetup::default(), true);
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], "Done.");
    let id = card_id(&outputs);
    day.pass(outputs);
    id
}

#[test]
fn a_summary_set_to_write_on_open_waits_and_opening_the_turn_writes_it_once() {
    let mut day = Day::new();
    let id = on_open(&mut day);
    assert_eq!(day.summary(&id), None, "the turn waits");
    assert_eq!(day.summaries.load(Ordering::SeqCst), 0);

    let written = day
        .writing
        .opened(&id)
        .unwrap()
        .expect("the opened turn's summary");
    assert_eq!(written.request.id, id);
    assert_eq!(day.summary(&id), Some(written.clone()));
    // The design follows the summary, which was written on open.
    assert!(day.events().contains(&"summary written".to_string()));

    // A second opening of the same turn writes nothing.
    assert_eq!(day.writing.opened(&id).unwrap(), Some(written));
    assert_eq!(day.summaries.load(Ordering::SeqCst), 1);
    assert_eq!(day.writing.opened("s1@1"), Ok(None));
}

#[test]
fn opening_a_turn_returns_its_summary_before_design_so_far_is_written_on_the_queue() {
    let mut day = Day::new();
    day.why.store(true, Ordering::SeqCst);
    let id = on_open(&mut day);
    let written = day.writing.opened(&id).unwrap();
    assert!(written.is_some(), "the summary comes back");
    assert_eq!(day.summary(&id), written);
    assert!(
        !day.events().contains(&"design written".to_string()),
        "the opening writes no design itself"
    );

    assert_eq!(day.writing.run_queued(), 1);
    let events = day.events();
    let at = |what: &str| events.iter().position(|e| e == what).unwrap();
    assert!(at("summary written") < at("design written"));
    assert!(day.store.lock().unwrap().brief("s1").is_some());
}

#[test]
fn two_designs_of_one_session_leave_the_one_written_from_the_later_sources() {
    let mut day = Day::new();
    day.why.store(true, Ordering::SeqCst);
    let id = on_open(&mut day);
    day.writing.opened(&id).unwrap();
    // The design of the opening waits on the queue while the next turn, whose summary is written
    // at its end, comes in behind it.
    day.writing.settings(SummarySetup::default(), false);
    let outputs = day.turn_asking("Add another", &[("b.rs", 10)], &long_reply());
    day.pass(outputs);

    let designed_from = |day: &Day| day.store.lock().unwrap().brief("s1").unwrap().design_from;
    // The summary of each turn is a source, and the last write saw both.
    assert_eq!(designed_from(&day), 2);
    assert_eq!(day.writing.run_queued(), 0);
}

#[test]
fn two_threads_that_record_the_same_decision_leave_one_line_in_the_offers_record() {
    let data = tempfile::tempdir().unwrap();
    let decision = Decision {
        kind: OfferKind::Turn,
        offer: Offer::Card,
        inputs: Inputs::default(),
        reason: None,
    };
    for round in 0..40 {
        let offers = Arc::new(Offers::beside(&data.path().join("events.jsonl")));
        let id = format!("s1@{round}");
        let threads: Vec<_> = (0..4)
            .map(|_| {
                let (offers, decision, id) = (offers.clone(), decision.clone(), id.clone());
                thread::spawn(move || offers.record("s1", &id, &decision, Some(Outcome::Written)))
            })
            .collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
    }
    let text = fs::read_to_string(data.path().join("offers.jsonl")).unwrap();
    assert_eq!(text.lines().count(), 40);
}

#[test]
fn a_failed_write_on_open_parks_the_turn_again_and_the_next_opening_writes_it() {
    let mut day = Day::new();
    let id = on_open(&mut day);
    day.failing.store(true, Ordering::SeqCst);
    assert_eq!(day.writing.opened(&id), Err(format!("no summary on {id}")));
    assert_eq!(day.summary(&id), None);

    day.failing.store(false, Ordering::SeqCst);
    let written = day.writing.opened(&id).unwrap();
    assert!(written.is_some_and(|s| s.summary.quiz.is_some()));
    // The failed write was tried twice (the writers retry once), then the one that worked.
    assert_eq!(day.summaries.load(Ordering::SeqCst), 3);
    // A silent writer is no final decision: its line stays, and the write that worked adds the last.
    let lines = day.offers();
    let outcomes: Vec<_> = lines
        .iter()
        .map(|l| l["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, ["no reply from the writer", "written"]);
}

#[test]
fn a_session_is_named_on_its_queue_from_its_first_prompt() {
    let mut day = Day::new();
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Fix the build" }));
    assert!(outputs.iter().any(|o| matches!(o, Output::Name(_))));
    day.pass(outputs);

    assert_eq!(day.events(), ["name of s1: Sample session name"]);
}

/// Turn `s1`'s end, replayed as it was while the app was down, with the trees the hook recorded.
fn down_turn(day: &mut Day, files: &[&str]) -> Backfill {
    let line = |name: &str, at: u64| {
        json!({"session_id": "s1", "hook_event_name": name, "cwd": day.repo,
            "studio_received_ms": at, "prompt": "Add two files"})
        .to_string()
    };
    let mut core = day.core.lock().unwrap();
    core.replay(&line("UserPromptSubmit", day.at));
    core.replay(&line("Stop", day.at + 1));
    drop(core);
    let (root, start) = record_tree(&day.repo).expect("a tree");
    for path in files {
        fs::write(
            day.repo.join(path),
            "let v0 = 0;\nlet v1 = 1;\nlet v2 = 2;\n",
        )
        .unwrap();
    }
    let (_, end) = record_tree(&day.repo).expect("a tree");
    let request = CardRequest {
        session_id: "s1".into(),
        id: format!("s1@{}", day.at + 1),
        ..CardRequest::default()
    };
    Backfill {
        request,
        trees: vec![Trees { root, start, end }],
    }
}

#[test]
fn a_backfilled_card_is_written_and_has_no_summary() {
    let mut day = Day::new();
    let turn = down_turn(&mut day, &["a.rs", "b.rs"]);
    let id = turn.request.id.clone();
    day.writing.backfill(vec![turn]);
    day.writing.run_queued();

    assert_eq!(day.card_ids(), std::slice::from_ref(&id));
    assert_eq!(day.summary(&id), None);
    assert_eq!(day.offers().len(), 1);
}

#[test]
fn a_backfilled_card_is_judged_by_how_its_turn_ended_and_not_by_its_sessions_state_now() {
    let mut day = Day::new();
    let mut ended_asking = down_turn(&mut day, &["a.rs", "b.rs"]);
    ended_asking.request.asks = true;
    let (asking, answered) = (
        ended_asking.request.id.clone(),
        format!("s1@{}", day.at + 9),
    );
    let mut ended_at_your_turn = down_turn(&mut day, &["c.rs", "d.rs"]);
    ended_at_your_turn.request.id.clone_from(&answered);
    // The session needs Daniel when the backfill runs.
    day.event(
        "PermissionRequest",
        json!({"tool_name": "Bash", "tool_input": {"command": "ls"}, "tool_use_id": "t1"}),
    );
    day.writing.backfill(vec![ended_asking, ended_at_your_turn]);
    day.writing.run_queued();

    let store = day.store.lock().unwrap();
    let quiz = |id: &str| {
        store
            .cards()
            .iter()
            .find(|k| k.request.id == id)
            .map(|k| k.card.quiz.is_some())
    };
    assert_eq!((quiz(&asking), quiz(&answered)), (Some(false), Some(true)));
}

#[test]
fn a_turn_already_on_record_is_not_asked_again_at_the_backfill() {
    let mut day = Day::new();
    let turn = down_turn(&mut day, &["a.rs", "b.rs"]);
    let decision = Decision {
        kind: OfferKind::Turn,
        offer: Offer::Card,
        inputs: Inputs::default(),
        reason: None,
    };
    let offers = Offers::beside(&day.data.join("events.jsonl"));
    offers.record("s1", &turn.request.id, &decision, Some(Outcome::Written));
    day.writing.backfill(vec![turn]);
    let ran = day.writing.run_queued();

    assert_eq!((ran, day.card_ids()), (0, Vec::<String>::new()));
    assert_eq!(day.offers().len(), 1);
}

/// A card request of session `s1` with `change`, and `id`.
fn request(id: &str, change: &str) -> CardRequest {
    CardRequest {
        session_id: "s1".into(),
        id: id.into(),
        change: Some(change.into()),
        ..CardRequest::default()
    }
}

fn diff(paths: &[&str]) -> String {
    let file =
        |p: &&str| format!("diff --git a/{p} b/{p}\n--- a/{p}\n+++ b/{p}\n@@ -1 +1 @@\n-a\n+b\n");
    paths.iter().map(file).collect()
}

#[test]
fn a_long_checks_card_and_a_workers_card_go_through_the_same_queue_and_only_the_workers_is_judged()
{
    let mut day = Day::new();
    // The session exists, with its own turn's prompt.
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Run the checks" }));
    day.pass(outputs);
    // A check's card on a one-line change, which the table alone would give no card.
    let check = CardRequest {
        check: Some("cargo test".into()),
        ..request("s1@100", &diff(&["a.rs"]))
    };
    // A worker's card on two logic files, which the table gives a card.
    let worker = CardRequest {
        agent: Some("w1".into()),
        ..request("w1@200", &diff(&["a.rs", "b.rs"]))
    };
    day.writing.card(check, true);
    day.writing.card(worker, true);
    day.writing.run_queued();

    assert_eq!(day.card_ids(), ["s1@100", "w1@200"]);
    // The check's card is not judged by the offer: its line is the worker's alone.
    let lines = day.offers();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(
        (lines[0]["id"].as_str(), lines[0]["kind"].as_str()),
        (Some("w1@200"), Some("worker"))
    );
    let notices = day.events().into_iter().filter(|e| e.starts_with("notice"));
    assert_eq!(
        notices.collect::<Vec<_>>(),
        [
            "notice on s1, agent None",
            "notice on s1, agent Some(\"w1\")"
        ]
    );
}

#[test]
fn a_workers_card_and_a_checks_card_of_a_turn_that_asked_are_stored_without_a_quiz() {
    let mut day = Day::new();
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Go" }));
    day.pass(outputs);
    let change = diff(&["a.rs", "b.rs"]);
    let worker = CardRequest {
        agent: Some("w1".into()),
        asks: true,
        ..request("w1@200", &change)
    };
    let check = CardRequest {
        check: Some("cargo test".into()),
        asks: true,
        ..request("s1@100", &change)
    };
    day.writing.card(worker, true);
    day.writing.card(check, true);
    day.writing.run_queued();

    assert_eq!(day.card_ids(), ["w1@200", "s1@100"]);
    assert_eq!(quizzes(&day), 0);
}

/// Runs `call` on another thread and fails unless it returns within a few seconds.
fn returns_in_time<T: Send + 'static>(what: &str, call: impl FnOnce() -> T + Send + 'static) -> T {
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(call());
    });
    finished
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| panic!("{what} did not return"))
}

/// Turn writing over an empty core, which has no such turn and no such session, shared with the thread of a call.
fn on_an_empty_core() -> (Arc<TurnWriting>, tempfile::TempDir) {
    let data = tempfile::tempdir().unwrap();
    let store = Arc::new(Mutex::new(
        CardStore::open(data.path().join("cards.json")).unwrap(),
    ));
    let events = Events::default();
    let recorder = Arc::new(Recorder {
        events: events.clone(),
        store: store.clone(),
        logs: Mutex::default(),
    });
    let writer = Arc::new(Recorded {
        events,
        failing: Arc::default(),
        summaries: Arc::default(),
        why: Arc::default(),
    });
    let offers = Offers::beside(&data.path().join("events.jsonl"));
    let core: Core = Arc::new(Mutex::new(Studio::default()));
    let writing = TurnWriting::stepped(core, store, writer, offers, recorder);
    (Arc::new(writing), data)
}

#[test]
fn every_entry_point_returns_within_a_few_seconds_against_a_core_with_no_such_turn() {
    let (writing, _data) = on_an_empty_core();
    let called = |what: &'static str, call: fn(&TurnWriting)| {
        let writing = writing.clone();
        returns_in_time(what, move || call(&writing));
    };
    called("card", |w| w.card(request("s1@1", &diff(&["a.rs"])), true));
    called("a backfilled card", |w| {
        w.card(request("s1@2", &diff(&["a.rs"])), false)
    });
    called("brief", |w| w.brief(request("s1@3", "")));
    called("backfill", |w| {
        let request = request("s1@4", "");
        w.backfill(vec![Backfill {
            request,
            trees: Vec::new(),
        }]);
    });
    let summary = SummaryRequest {
        session_id: "s1".into(),
        id: "s1@5".into(),
        wanted: true,
        ..SummaryRequest::default()
    };
    called("settings", |w| w.settings(SummarySetup::default(), false));
    returns_in_time("summary", {
        let (writing, summary) = (writing.clone(), summary.clone());
        move || writing.summary(summary)
    });
    called("name", |w| {
        let request = NameRequest {
            session_id: "s1".into(),
            prompt: "Fix it".into(),
            response: None,
        };
        w.name(request);
    });
    called("opened", |w| assert_eq!(w.opened("s1@6"), Ok(None)));
    called("setup", |w| drop(w.setup()));
    called("prune", |w| w.prune(&Default::default()));
    // The jobs they queued run too, a card's read of its session included, and end with nothing written.
    called("the queued jobs", |w| assert_eq!(w.run_queued(), 5));
    // A summary parked on open, opened against a core with no such turn.
    called("settings, on open", |w| {
        w.settings(SummarySetup::default(), true)
    });
    let parked = SummaryRequest {
        id: "s1@7".into(),
        ..summary
    };
    let opened = returns_in_time("a parked summary", {
        let writing = writing.clone();
        move || {
            writing.summary(parked);
            writing.run_queued();
            writing.opened("s1@7")
        }
    });
    assert!(opened.is_ok_and(|s| s.is_some_and(|s| s.request.id == "s1@7")));
}

#[test]
fn the_threaded_module_writes_a_card_on_its_sessions_queue_and_prune_ends_the_queue() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add two files", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(Recorded {
            events: day.events.clone(),
            failing: Arc::default(),
            summaries: Arc::default(),
            why: Arc::default(),
        }),
        Offers::beside(&day.data.join("events.jsonl")),
        day.recorder.clone(),
    );
    for output in outputs {
        if let Output::Card(request) = output {
            threaded.card(request, true);
        }
    }
    // Pruning lets the queued job finish, then ends the thread.
    threaded.prune(&Default::default());
    for _ in 0..500 {
        if !day.card_ids().is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(day.card_ids().len(), 1);
}

#[test]
fn a_turn_opened_in_an_ended_session_gets_its_summary_and_design_and_its_queue_ends() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], "Done.");
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(Recorded {
            events: day.events.clone(),
            failing: Arc::default(),
            summaries: Arc::default(),
            why: Arc::new(AtomicBool::new(true)),
        }),
        Offers::beside(&day.data.join("events.jsonl")),
        day.recorder.clone(),
    );
    threaded.settings(SummarySetup::default(), true);
    let id = card_id(&outputs);
    for output in outputs {
        match output {
            Output::Card(request) => threaded.card(request, true),
            Output::Brief(request) => threaded.brief(request),
            Output::Summary(request) => threaded.summary(request),
            _ => {}
        }
    }
    // The session ends: its queue finishes the parking job, then its thread ends.
    threaded.prune(&Default::default());
    let mut summary = None;
    for _ in 0..500 {
        summary = threaded.opened(&id).unwrap();
        if summary.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(summary.is_some(), "the summary is written");
    for _ in 0..500 {
        if day.store.lock().unwrap().brief("s1").is_some() && threaded.queues() == 0 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        day.store.lock().unwrap().brief("s1").is_some(),
        "the design is written"
    );
    assert_eq!(threaded.queues(), 0, "no queue is left behind");
}

/// A writer whose names take a moment, telling `events` when each starts and ends.
struct Slow(Events);

impl CardWriter for Slow {
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        FakeWriter.reply(request, earlier)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }

    fn goal(&self, prompts: &[&str]) -> Result<String, String> {
        FakeWriter.goal(prompts)
    }

    fn design(&self, sources: &[(String, String)]) -> Result<String, String> {
        FakeWriter.design(sources)
    }

    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        FakeWriter.summarize(setup, request, earlier)
    }

    fn name(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
    ) -> Result<String, String> {
        self.0.lock().unwrap().push(format!("start {prompt}"));
        thread::sleep(Duration::from_millis(40));
        self.0.lock().unwrap().push(format!("end {prompt}"));
        FakeWriter.name(setup, prompt, response)
    }
}

#[test]
fn two_jobs_queued_for_an_ended_session_run_one_at_a_time_in_order() {
    let day = Day::new();
    let events = Events::default();
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(Slow(events.clone())),
        Offers::beside(&day.data.join("events.jsonl")),
        day.recorder.clone(),
    );
    threaded.prune(&Default::default());
    for prompt in ["one", "two"] {
        threaded.name(NameRequest {
            session_id: "s1".into(),
            prompt: prompt.into(),
            response: None,
        });
    }
    for _ in 0..1000 {
        if events.lock().unwrap().len() == 4 && threaded.queues() == 0 {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    let seen = events.lock().unwrap().clone();
    assert_eq!(seen, ["start one", "end one", "start two", "end two"]);
    assert_eq!(threaded.queues(), 0);
}

// The turns of the stand-in's tests (`prompt_cards.rs`, danielrispler/plotkeeper#261), written through the real offer:
// every change that expects a card is two logic files of three lines.

/// The fake writer, keeping what each call read: a card's earlier summaries and each Goal's prompts.
#[derive(Clone, Default)]
struct Reading {
    cards: Arc<Mutex<Vec<Vec<String>>>>,
    goals: Arc<Mutex<Vec<Vec<String>>>>,
}

struct ReadingWriter {
    recorded: Recorded,
    reading: Reading,
}

impl CardWriter for ReadingWriter {
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        self.reading.cards.lock().unwrap().push(earlier.into());
        self.recorded.reply(request, earlier)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        self.recorded.more(request, card)
    }

    fn goal(&self, prompts: &[&str]) -> Result<String, String> {
        let kept = prompts.iter().map(|p| p.to_string()).collect();
        self.reading.goals.lock().unwrap().push(kept);
        self.recorded.goal(prompts)
    }

    fn design(&self, sources: &[(String, String)]) -> Result<String, String> {
        self.recorded.design(sources)
    }

    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        self.recorded.summarize(setup, request, earlier)
    }

    fn name(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
    ) -> Result<String, String> {
        self.recorded.name(setup, prompt, response)
    }
}

/// A day whose writer keeps what it read.
fn reading_day() -> (Day, Reading) {
    let reading = Reading::default();
    let kept = reading.clone();
    let day = Day::with_writer(move |recorded| Arc::new(ReadingWriter { recorded, reading }));
    (day, kept)
}

const MINUTE: u64 = 60 * 1000;

const WAKE_UP: &str =
    "<task-notification><task-id>a1</task-id><status>completed</status></task-notification>";

/// Writes `paths` in `dir`, three lines each.
fn put(dir: &Path, paths: &[&str]) {
    for path in paths {
        let code = "let v0 = 0;\nlet v1 = 1;\nlet v2 = 2;\n";
        fs::write(dir.join(path), code).unwrap();
    }
}

/// The summary the fake writer gives a change to `paths`.
fn summary_of(paths: &[&str]) -> String {
    format!("Sample change to `{}`", paths.join("`, `"))
}

/// The spawn of subagent `id` with `description`, in a worktree of its own when `isolated`.
fn spawn_child(id: &str, description: &str, isolated: bool) -> Value {
    let isolation = if isolated { "worktree" } else { "" };
    json!({"tool_name": "Agent", "tool_response": {"agentId": id},
        "tool_input": {"description": description, "isolation": isolation, "prompt": "…"}})
}

/// A Bash step's result.
fn bash(command: &str) -> Value {
    json!({"tool_name": "Bash", "tool_input": {"command": command}})
}

fn read_step() -> Value {
    json!({"tool_name": "Read", "tool_input": {"file_path": "lib.rs"}})
}

impl Day {
    /// Takes event `name`, with `fields` added, and writes what it asks for.
    fn take(&mut self, name: &str, fields: Value) {
        let outputs = self.event(name, fields);
        self.pass(outputs);
    }

    /// Subagent `agent`'s event `name` in folder `cwd`, with `fields` added, written as `take` does.
    fn child(&mut self, agent: &str, cwd: &Path, name: &str, fields: Value) {
        let mut child = json!({ "agent_id": agent, "cwd": cwd });
        child
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        self.take(name, child);
    }

    /// A new worktree of the repository, outside it, on its own branch `name`.
    fn worktree(&self, name: &str) -> PathBuf {
        let tree = self.data.join(name);
        git(
            &self.repo,
            &["worktree", "add", "-qb", name, tree.to_str().unwrap()],
        );
        tree
    }

    /// The cards in the store, oldest first: whose each is and its summary.
    fn cards(&self) -> Vec<(Option<String>, String)> {
        let store = self.store.lock().unwrap();
        let cards = store.cards().iter();
        cards
            .map(|kept| (kept.request.agent.clone(), kept.card.summary.clone()))
            .collect()
    }

    /// The session's own cards, each as the diff it asked the writer about.
    fn own_changes(&self) -> Vec<String> {
        let store = self.store.lock().unwrap();
        let own = store
            .cards()
            .iter()
            .filter(|kept| kept.request.agent.is_none());
        own.map(|kept| kept.request.diff().to_string()).collect()
    }

    /// What the session's row says at `now`.
    fn line_at(&self, now: u64) -> String {
        let session = self.core.lock().unwrap().session("s1").cloned().unwrap();
        self.store.lock().unwrap().line(&session, now)
    }

    fn line(&self) -> String {
        self.line_at(self.at)
    }

    /// The kind of the session's first subagent.
    fn kind(&self) -> Kind {
        self.core.lock().unwrap().session("s1").unwrap().agents[0].kind
    }
}

/// Worker `w1` in a worktree of its own writes `a.rs` and `b.rs` and ends, with one card.
fn worker_writes_and_ends(day: &mut Day) {
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = day.worktree("w1");
    day.child("w1", &tree, "SubagentStart", json!({}));
    put(&tree, &["a.rs", "b.rs"]);
    day.child("w1", &tree, "SubagentStop", json!({}));
}

#[test]
fn a_worker_gets_a_card_at_each_commit_in_its_worktree_and_at_its_end_with_its_own_goal() {
    let mut day = Day::new();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = day.worktree("w1");
    day.child("w1", &tree, "SubagentStart", json!({}));

    put(&tree, &["a.rs", "b.rs"]);
    day.child("w1", &tree, "PostToolUse", bash("cargo fmt"));
    assert_eq!(day.cards(), []);
    git(&tree, &["add", "."]);
    git(&tree, &["commit", "-qm", "Add a and b"]);
    day.child(
        "w1",
        &tree,
        "PostToolUse",
        bash("git commit -m 'Add a and b'"),
    );
    put(&tree, &["c.rs", "d.rs"]);
    day.child("w1", &tree, "SubagentStop", json!({}));

    let w1 = Some("w1".to_string());
    assert_eq!(
        day.cards(),
        [
            (w1.clone(), summary_of(&["a.rs", "b.rs"])),
            (w1, summary_of(&["c.rs", "d.rs"]))
        ]
    );
    let store = day.store.lock().unwrap();
    let brief = store.brief("w1").expect("the worker's brief");
    assert_eq!(
        brief.goal.as_deref(),
        Some("A sample goal from the fake writer, which read 1 prompts.")
    );
    assert_eq!(store.brief("s1"), None);
}

/// A killed worker has no `SubagentStop`; its end is read from its wake-up, which also opens the session's next turn and
/// takes that turn's base.
#[test]
fn a_killed_worker_gets_its_end_card_at_the_wake_up_that_reports_it() {
    let (mut day, reading) = reading_day();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = day.worktree("w1");
    day.child("w1", &tree, "SubagentStart", json!({}));
    day.take("Stop", json!({}));
    put(&tree, &["a.rs", "b.rs"]);
    // Daniel's own edit before the wake-up is not the woken turn's.
    put(&day.repo, &["x.rs", "y.rs"]);

    let killed =
        "<task-notification><task-id>w1</task-id><status>killed</status></task-notification>";
    day.take("UserPromptSubmit", json!({ "prompt": killed }));
    day.take("Stop", json!({}));

    assert_eq!(
        day.cards(),
        [(Some("w1".into()), summary_of(&["a.rs", "b.rs"]))]
    );
    // The worker's, from its spawn, and the session's, from Daniel's prompt.
    assert_eq!(reading.goals.lock().unwrap().len(), 2);
}

#[test]
fn a_parent_that_changed_nothing_itself_gets_no_card_though_its_workers_wrote_theirs() {
    let (mut day, reading) = reading_day();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    worker_writes_and_ends(&mut day);
    let before = reading.cards.lock().unwrap().len();
    day.take("Stop", json!({}));

    let own = day.cards();
    let [(Some(worker), _)] = &own[..] else {
        panic!("only the worker's card: {own:?}");
    };
    assert_eq!(worker, "w1");
    assert_eq!(reading.cards.lock().unwrap().len(), before);
}

#[test]
fn a_wake_up_turn_that_changed_nothing_itself_writes_no_card_and_its_goal_reads_daniels_prompts() {
    let (mut day, reading) = reading_day();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    day.take("Stop", json!({}));
    worker_writes_and_ends(&mut day);
    let wake_up =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    day.take("UserPromptSubmit", json!({ "prompt": wake_up }));
    day.take("Stop", json!({ "last_assistant_message": "w1 is done." }));

    assert_eq!(
        day.cards(),
        [(Some("w1".into()), summary_of(&["a.rs", "b.rs"]))]
    );
    // The worker's Goal reads its spawn; the session's, Daniel's prompt and never the wake-up.
    assert_eq!(
        *reading.goals.lock().unwrap(),
        [vec!["…"], vec!["Implement #378"]]
    );
    assert_eq!(day.line(), "w1 is done.");
}

/// No timer; a step 20 minutes or more after the last card checks its worktree.
#[test]
fn after_25_minutes_of_uncommitted_change_the_next_step_gets_a_card_and_one_with_no_change_none() {
    let (mut day, reading) = reading_day();
    let repo = day.repo.clone();
    day.take(
        "UserPromptSubmit",
        json!({ "prompt": "Implement #378 and #379" }),
    );
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    day.take("PostToolUse", spawn_child("w2", "Implement #379", true));
    let (one, two) = (day.worktree("w1"), day.worktree("w2"));
    day.child("w1", &one, "SubagentStart", json!({}));
    day.child("w2", &two, "SubagentStart", json!({}));

    put(&one, &["a.rs", "b.rs"]);
    put(&repo, &["lib.rs", "m.rs"]);
    day.child("w1", &one, "PostToolUse", read_step());
    assert_eq!(day.cards(), []);
    day.at += 25 * MINUTE;
    day.child("w1", &one, "PostToolUse", read_step());
    day.child("w2", &two, "PostToolUse", read_step());
    // The session's own prompt too.
    day.take("PostToolUse", read_step());

    let (w1, own) = (Some("w1".to_string()), None);
    assert_eq!(
        day.cards(),
        [
            (w1, summary_of(&["a.rs", "b.rs"])),
            (own, summary_of(&["lib.rs", "m.rs"]))
        ]
    );
    assert_eq!(reading.cards.lock().unwrap().len(), 2);
}

/// A fallback look that finds no change restarts the 20-minute wait, so a quiet worktree gets one look, not one per
/// step, and a later change waits for the next fallback.
#[test]
fn a_quiet_worktrees_fallback_look_restarts_its_wait_so_a_later_change_waits_20_minutes_from_it() {
    let mut day = Day::new();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let one = day.worktree("w1");
    day.child("w1", &one, "SubagentStart", json!({}));

    day.at += 25 * MINUTE;
    day.child("w1", &one, "PostToolUse", read_step());
    put(&one, &["a.rs", "b.rs"]);
    day.at += MINUTE;
    day.child("w1", &one, "PostToolUse", read_step());
    day.at += 18 * MINUTE;
    day.child("w1", &one, "PostToolUse", read_step());
    assert_eq!(day.cards(), []);
    day.at += MINUTE;
    day.child("w1", &one, "PostToolUse", read_step());

    assert_eq!(
        day.cards(),
        [(Some("w1".to_string()), summary_of(&["a.rs", "b.rs"]))]
    );
}

/// The fallback is on a step of a worker or of a prompt; after the session's `Stop` there is no prompt, so Daniel's own
/// edit is no one's card.
#[test]
fn a_background_helpers_step_after_the_stop_writes_no_card_for_the_session_after_25_minutes() {
    let mut day = Day::new();
    let repo = day.repo.clone();
    day.take(
        "UserPromptSubmit",
        json!({ "prompt": "Review in the background" }),
    );
    day.take("PostToolUse", spawn_child("h1", "Review", false));
    day.child("h1", &repo, "SubagentStart", json!({}));
    day.take("Stop", json!({}));

    put(&repo, &["lib.rs", "m.rs"]);
    day.at += 25 * MINUTE;
    day.child("h1", &repo, "PostToolUse", read_step());

    assert_eq!(day.cards(), []);
}

/// The shell fingerprints a helper's Bash step, so a command that changes its worktree makes it a worker live, not only
/// in the core's test.
#[test]
fn a_helper_whose_command_changes_its_worktree_becomes_a_worker() {
    let mut day = Day::new();
    let repo = day.repo.clone();
    day.take("UserPromptSubmit", json!({ "prompt": "Format the code" }));
    day.take("PostToolUse", spawn_child("h1", "Format", false));
    day.child("h1", &repo, "SubagentStart", json!({}));

    day.child("h1", &repo, "PreToolUse", bash("cargo fmt --check"));
    day.child("h1", &repo, "PostToolUse", bash("cargo fmt --check"));
    assert_eq!(day.kind(), Kind::Helper);
    day.child("h1", &repo, "PreToolUse", bash("cargo fmt"));
    fs::write(repo.join("lib.rs"), "fn a() {}\n\n").unwrap();
    day.child("h1", &repo, "PostToolUse", bash("cargo fmt"));

    assert_eq!(day.kind(), Kind::Worker);
}

/// Only a change across the helper's own Bash step counts, not what its parent or Daniel changed in the shared worktree
/// before it.
#[test]
fn a_helper_whose_command_changes_nothing_stays_a_helper_after_others_change_its_worktree() {
    let mut day = Day::new();
    let repo = day.repo.clone();
    day.take("UserPromptSubmit", json!({ "prompt": "Review the change" }));
    day.take("PostToolUse", spawn_child("h1", "Review", false));
    day.child("h1", &repo, "SubagentStart", json!({}));

    fs::write(repo.join("lib.rs"), "fn b() {}\n").unwrap();
    day.child("h1", &repo, "PreToolUse", bash("cargo test"));
    day.child("h1", &repo, "PostToolUse", bash("cargo test"));

    assert_eq!(day.kind(), Kind::Helper);
}

#[test]
fn a_prompt_that_changes_nothing_makes_no_writer_call_and_one_that_commits_gets_its_card() {
    let mut day = Day::new();

    // Daniel's own edit before the prompt is not the prompt's change.
    put(&day.repo, &["x.rs", "y.rs"]);
    day.take("UserPromptSubmit", json!({}));
    day.take("Stop", json!({}));
    assert!(!day.events().contains(&"card written".to_string()));

    day.take("UserPromptSubmit", json!({}));
    put(&day.repo, &["a.rs", "b.rs"]);
    git(&day.repo, &["add", "."]);
    git(&day.repo, &["commit", "-qm", "Add a and b"]);
    day.take("Stop", json!({}));

    let written = day.events().iter().filter(|e| *e == "card written").count();
    assert_eq!(written, 1);
    assert_eq!(day.cards().len(), 1);
}

#[test]
fn a_wake_ups_card_reads_its_prompts_earlier_cards() {
    let (mut day, reading) = reading_day();
    day.take("SessionStart", json!({ "source": "startup" }));
    day.take("PostToolUse", spawn_child("a1", "Implement #377", false));
    day.turn(
        "Split the spec into tickets",
        &[("a.rs", 3), ("b.rs", 3)],
        "Done.",
    );
    day.turn(WAKE_UP, &[("c.rs", 3), ("d.rs", 3)], "Done.");
    day.turn("What did a1 do?", &[("e.rs", 3), ("f.rs", 3)], "Done.");

    // The wake-up's card is on Daniel's prompt before it.
    assert_eq!(
        *reading.cards.lock().unwrap(),
        [vec![], vec![summary_of(&["a.rs", "b.rs"])], vec![]]
    );
}

#[test]
fn the_goal_is_written_again_only_when_a_prompt_of_daniels_came_and_reads_his_prompts_alone() {
    let (mut day, reading) = reading_day();
    day.take("SessionStart", json!({ "source": "startup" }));
    day.take("PostToolUse", spawn_child("a1", "Implement #377", false));
    for (prompt, files) in [
        ("Add a", [("a.rs", 3), ("b.rs", 3)]),
        ("Add b", [("c.rs", 3), ("d.rs", 3)]),
        (WAKE_UP, [("e.rs", 3), ("f.rs", 3)]),
        ("Add d", [("g.rs", 3), ("h.rs", 3)]),
    ] {
        day.turn(prompt, &files, "Done.");
    }

    // No call on the wake-up's card, and never one that rewrites the brief from the cards.
    assert_eq!(
        *reading.goals.lock().unwrap(),
        [
            vec!["Add a"],
            vec!["Add a", "Add b"],
            vec!["Add a", "Add b", "Add d"]
        ]
    );
    let store = day.store.lock().unwrap();
    let brief = store.brief("s1").unwrap();
    assert_eq!((brief.goal_prompts, brief.text.as_str()), (3, ""));
}

#[test]
fn a_stopped_row_shows_the_agents_first_line_then_the_cards_summary_then_the_agents_first_line_again()
 {
    let mut day = Day::new();
    day.take("UserPromptSubmit", json!({ "prompt": "Rename a" }));
    put(&day.repo, &["lib.rs", "m.rs"]);
    assert_eq!(day.line(), "No step yet");

    let message = "\nRenamed `a` to `b`.\n\nThe tests pass.";
    let outputs = day.event("Stop", json!({ "last_assistant_message": message }));
    assert_eq!(day.line(), "Renamed `a` to `b`.");

    day.pass(outputs);
    assert_eq!(day.line(), summary_of(&["lib.rs", "m.rs"]));

    // A turn that changed nothing gets no card: the row shows the agent's first line.
    day.take("UserPromptSubmit", json!({ "prompt": "Anything left?" }));
    day.take("Stop", json!({ "last_assistant_message": "Nothing left." }));
    assert_eq!(day.line(), "Nothing left.");
}

#[test]
fn a_rows_line_is_who_needs_you_then_the_stopped_line_then_who_returned_then_the_last_step() {
    let mut day = Day::new();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    assert_eq!(day.line(), "No step yet");
    day.take(
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "t1",
        "tool_input": {"command": "cargo test"}}),
    );
    assert_eq!(day.line(), "Bash cargo test");

    // The session's own agent asks: its last step and how long it has waited.
    let ask = json!({"tool_name": "AskUserQuestion", "tool_use_id": "t2", "tool_input": {}});
    day.take("PreToolUse", ask.clone());
    let line = day.line_at(day.at + 3 * MINUTE);
    assert!(
        line.starts_with("AskUserQuestion") && line.ends_with(" · 3m"),
        "{line}"
    );
    day.take("PostToolUse", ask);

    // A subagent asks: its name, "asks", and how long it has waited.
    day.take("PostToolUse", spawn_child("w1", "Implement #378", false));
    let ask = json!({"tool_name": "AskUserQuestion", "tool_use_id": "t3", "tool_input": {}});
    let repo = day.repo.clone();
    day.child("w1", &repo, "PreToolUse", ask.clone());
    assert_eq!(day.line_at(day.at + 2 * MINUTE), "Implement #378 asks · 2m");
    day.child("w1", &repo, "PostToolUse", ask);
    day.child("w1", &repo, "SubagentStop", json!({}));

    // A wake-up with no step since: the worker that returned.
    let wake_up =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    day.take("UserPromptSubmit", json!({ "prompt": wake_up }));
    assert_eq!(day.line(), "worker Implement #378 returned");
    day.take(
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "t4",
        "tool_input": {"command": "git log"}}),
    );
    assert_eq!(day.line(), "Bash git log");

    // Once the turn is over: the stopped line.
    day.take("Stop", json!({ "last_assistant_message": "Done." }));
    assert_eq!(day.line(), "Done.");
}

/// A base per worktree at the turn's first touch, and one joined card.
#[test]
fn a_turn_that_starts_in_one_worktree_and_stops_in_another_gets_one_card_with_both_changes() {
    let mut day = Day::new();
    let other = day.worktree("other");
    // Changes before the turn are no one's.
    fs::write(day.repo.join("notes.txt"), "mine\n").unwrap();
    fs::write(other.join("notes.txt"), "mine\n").unwrap();

    day.take("UserPromptSubmit", json!({ "prompt": "Fix both" }));
    put(&day.repo, &["a.rs"]);
    let cd = json!({"tool_name": "Bash", "tool_input": {"command": "cd ../other"}, "cwd": other});
    day.take("PostToolUse", cd);
    put(&other, &["b.rs"]);
    day.take("Stop", json!({ "cwd": other }));

    let [change] = &day.own_changes()[..] else {
        panic!("one card: {:?}", day.own_changes());
    };
    assert!(
        change.contains("+++ b/a.rs") && change.contains("+++ b/b.rs"),
        "{change}"
    );
    assert!(!change.contains("notes.txt"), "{change}");
}

/// An `Edit` path in another worktree touches it, with no `cd`.
#[test]
fn an_edit_into_another_worktree_without_a_cd_puts_its_change_on_the_card() {
    let mut day = Day::new();
    let other = day.worktree("other");
    day.take(
        "UserPromptSubmit",
        json!({ "prompt": "Fix the other tree" }),
    );
    let path = other.join("lib.rs");
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": path}});
    day.take("PreToolUse", edit.clone());
    fs::write(&path, "fn c() {}\nfn d() {}\nfn e() {}\n").unwrap();
    put(&other, &["m.rs"]);
    day.take("PostToolUse", edit);
    day.take("Stop", json!({}));

    let [change] = &day.own_changes()[..] else {
        panic!("one card: {:?}", day.own_changes());
    };
    assert!(change.contains("+fn c() {}"), "{change}");
}

/// A worktree whose worker has its own cards is left out of the session's.
#[test]
fn a_worktree_workers_change_is_on_its_own_cards_only_while_the_session_sits_in_its_worktree() {
    let mut day = Day::new();
    day.take("UserPromptSubmit", json!({ "prompt": "Start #378" }));
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = day.worktree("w1");
    day.child("w1", &tree, "SubagentStart", json!({}));
    day.take("Stop", json!({}));

    day.take(
        "UserPromptSubmit",
        json!({ "prompt": "Watch it", "cwd": tree }),
    );
    put(&tree, &["a.rs", "b.rs"]);
    day.child("w1", &tree, "SubagentStop", json!({}));
    day.take("Stop", json!({ "cwd": tree }));

    let worker = day.cards().into_iter().filter(|(agent, _)| agent.is_some());
    assert_eq!(
        worker.collect::<Vec<_>>(),
        [(Some("w1".to_string()), summary_of(&["a.rs", "b.rs"]))]
    );
    // The session's own card, if any, is built from its worker's card, not from the diff.
    let own = day.own_changes();
    assert!(own.iter().all(|c| !c.contains("+++ b/a.rs")), "{own:?}");
}

/// The tag of helper `h1` on `lib.rs`, which a card's "who changed what" holds.
fn tagged_lib() -> BTreeMap<String, BTreeSet<String>> {
    BTreeMap::from([("h1".to_string(), BTreeSet::from(["lib.rs".to_string()]))])
}

/// Ownership is decided once, by worktree root, at the subagent's first event.
#[test]
fn a_helper_in_the_sessions_folder_keeps_the_session_as_its_owner_after_the_session_cds_into_sub() {
    let mut day = Day::new();
    let repo = day.repo.clone();
    fs::create_dir(repo.join("sub")).unwrap();
    day.take("UserPromptSubmit", json!({ "prompt": "Review and fix" }));
    day.take("PostToolUse", spawn_child("h1", "Review", false));
    day.child("h1", &repo, "SubagentStart", json!({}));
    let sub = repo.join("sub");
    let cd = json!({"tool_name": "Bash", "tool_input": {"command": "cd sub"}, "cwd": sub});
    day.take("PostToolUse", cd);
    fs::write(
        repo.join("lib.rs"),
        "fn a() { b() }\nfn c() {}\nfn d() {}\n",
    )
    .unwrap();
    put(&repo, &["m.rs"]);
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": repo.join("lib.rs")}});
    day.child("h1", &repo, "PostToolUse", edit);
    day.take("Stop", json!({ "cwd": sub }));

    let store = day.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    assert_eq!(card.request.agent, None);
    assert_eq!(card.request.changed_by, tagged_lib());
}

/// "Who changed what" keeps only paths inside the owner's folder.
#[test]
fn a_write_outside_the_owners_folder_is_not_in_who_changed_what() {
    let mut day = Day::new();
    let repo = day.repo.clone();
    let scratch = tempfile::tempdir().unwrap();
    day.take("UserPromptSubmit", json!({ "prompt": "Review and fix" }));
    day.take("PostToolUse", spawn_child("h1", "Review", false));
    day.child("h1", &repo, "SubagentStart", json!({}));
    fs::write(
        repo.join("lib.rs"),
        "fn a() { b() }\nfn c() {}\nfn d() {}\n",
    )
    .unwrap();
    put(&repo, &["m.rs"]);
    for path in [repo.join("lib.rs"), scratch.path().join("note.md")] {
        let write = json!({"tool_name": "Write", "tool_input": {"file_path": path}});
        day.child("h1", &repo, "PostToolUse", write);
    }
    day.take("Stop", json!({}));

    let store = day.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    assert_eq!(card.request.changed_by, tagged_lib());
}

/// A background helper in the session's worktree, spawned in a turn that then stops, edits `lib.rs` and `m.rs` in `dir`.
fn background_helper_edits_after_the_stop(day: &mut Day) {
    let repo = day.repo.clone();
    day.take(
        "UserPromptSubmit",
        json!({ "prompt": "Fix it in the background" }),
    );
    day.take("PostToolUse", spawn_child("h1", "Fix", false));
    day.child("h1", &repo, "SubagentStart", json!({}));
    day.take("Stop", json!({}));
    helper_edits(day, &repo);
}

/// Helper `h1` edits `lib.rs`, and `m.rs` beside it, in `dir`.
fn helper_edits(day: &mut Day, dir: &Path) {
    fs::write(dir.join("lib.rs"), "fn a() { b() }\nfn c() {}\nfn d() {}\n").unwrap();
    put(dir, &["m.rs"]);
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": dir.join("lib.rs")}});
    day.child("h1", dir, "PostToolUse", edit);
}

/// The one card in the store: the session's, on the helper's edit, tagged with it.
fn the_helpers_card_is_the_sessions(day: &Day) {
    let store = day.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    assert_eq!(card.request.agent, None);
    assert!(card.request.changed_by.contains_key("h1"));
    assert!(card.request.diff().contains("+fn a() { b() }"));
}

/// A subagent's end outside a turn writes the session's card, tagged with it.
#[test]
fn a_background_helpers_edits_after_the_stop_get_a_card_tagged_with_it_when_it_ends() {
    let mut day = Day::new();
    background_helper_edits_after_the_stop(&mut day);
    assert_eq!(day.cards(), []);

    let repo = day.repo.clone();
    day.child("h1", &repo, "SubagentStop", json!({}));

    the_helpers_card_is_the_sessions(&day);
}

/// The next turn's start cards what is still uncarded, before its base.
#[test]
fn a_background_helpers_edits_get_a_card_at_the_next_turns_start_if_it_never_ends() {
    let mut day = Day::new();
    background_helper_edits_after_the_stop(&mut day);

    day.take("UserPromptSubmit", json!({ "prompt": "Next" }));
    day.take("Stop", json!({}));

    the_helpers_card_is_the_sessions(&day);
}

/// Such edits in a worktree that the next turn touches only after its start still get their card, so that worktree's base
/// never swallows them.
#[test]
fn a_background_helpers_edits_in_another_worktree_get_a_card_when_the_next_turn_moves_there() {
    let mut day = Day::new();
    let repo = day.repo.clone();
    let tree = day.worktree("b");
    let cd = || json!({"tool_name": "Bash", "tool_input": {"command": "cd b"}, "cwd": tree});
    day.take("UserPromptSubmit", json!({ "prompt": "Fix it over there" }));
    day.take("PostToolUse", cd());
    let mut spawn = spawn_child("h1", "Fix", false);
    spawn["cwd"] = json!(tree);
    day.take("PostToolUse", spawn);
    day.child("h1", &tree, "SubagentStart", json!({}));
    day.take("Stop", json!({ "cwd": tree }));
    helper_edits(&mut day, &tree);

    day.take("UserPromptSubmit", json!({ "prompt": "Next", "cwd": repo }));
    day.take("PostToolUse", cd());
    day.take("Stop", json!({ "cwd": tree }));

    the_helpers_card_is_the_sessions(&day);
}

#[test]
fn a_worker_that_started_while_its_card_waited_is_read_by_the_card_when_it_is_written() {
    let (mut day, reading) = reading_day();
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Split the spec" }));
    day.pass(outputs);
    let worker = CardRequest {
        agent: Some("w1".into()),
        ..request("w1@200", &diff(&["a.rs", "b.rs"]))
    };
    day.writing.card(worker, true);
    // The worker starts while the job waits.
    day.event("PostToolUse", spawn_child("w1", "Implement #378", true));
    day.writing.run_queued();

    // The worker's own prompt, not Daniel's, which a session with no such worker falls back to.
    assert_eq!(*reading.goals.lock().unwrap(), [vec!["…"]]);
}

#[test]
fn a_prompt_sent_while_a_card_waited_is_in_the_goal_that_job_writes() {
    let (mut day, reading) = reading_day();
    let outputs = day.turn_asking("Add a", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let card = outputs.into_iter().find_map(|o| match o {
        Output::Card(request) => Some(request),
        _ => None,
    });
    day.writing.card(card.expect("a card request"), true);
    // Daniel's next prompt comes while the job waits.
    day.event("UserPromptSubmit", json!({ "prompt": "Add b" }));
    day.writing.run_queued();

    assert_eq!(*reading.goals.lock().unwrap(), [vec!["Add a", "Add b"]]);
}

#[test]
fn a_session_deleted_while_its_card_waited_gets_nothing_and_the_queue_goes_on_to_the_next_job() {
    let mut day = Day::new();
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Add a" }));
    day.pass(outputs);
    let other = day.event(
        "UserPromptSubmit",
        json!({ "session_id": "s2", "prompt": "Add b" }),
    );
    day.pass(other);
    let gone = request("s1@100", &diff(&["a.rs", "b.rs"]));
    let next = CardRequest {
        session_id: "s2".into(),
        ..request("s2@200", &diff(&["a.rs", "b.rs"]))
    };
    day.writing.card(gone, true);
    day.writing.card(next, true);
    day.event("StudioDelete", json!({}));

    assert_eq!(day.writing.run_queued(), 2);
    assert_eq!(day.card_ids(), ["s2@200"]);
}
