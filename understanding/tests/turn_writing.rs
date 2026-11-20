//! Turn writing (danielrispler/plotkeeper#250, Testing Decisions), through its interface alone: the requests the core emits go
//! in, and what Daniel would get comes out, in the card store, the offers record and the callback.
//! Behind the seam: the fake writer, a core rebuilt by hook lines, a real card store and offers record in a
//! temporary folder, real temporary git repositories, and a callback that records what it is told.
//! The jobs run in order on the test's thread, through the code a queue's thread runs
//! (`TurnWriting::stepped`); the threaded module has its own tests where the thread is the point.
//! A test that expects a card edits two logic files, so the real offer gives one.

use serde_json::{Value, json};
use std::{
    collections::HashMap,
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
use studio_core::{Backfill, CardRequest, NameRequest, Output, Studio, SummaryRequest, Trees};
use studio_hook::record_tree;
use studio_understanding::{
    Card, CardStore, CardWriter, Core, Decision, Effects, FakeWriter, Fingerprints, Inputs, Offer,
    OfferKind, Offers, Outcome, StoredSummary, SummarySetup, TurnSummary, TurnWriting,
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
        let writer = Arc::new(Recorded {
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
    // The jobs they queued run too, and end with nothing written.
    called("the queued jobs", |w| assert_eq!(w.run_queued(), 2));
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
