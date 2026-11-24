//! Turn writing (danielrispler/plotkeeper#250, Testing Decisions), through its interface alone: the requests the core emits go
//! in, and what Daniel would get comes out, in the card store and the callback.
//! Behind the seam: the fake writer, a core rebuilt by hook lines, a real card store in a
//! temporary folder, real temporary git repositories, and a callback that records what it is told.
//! The jobs run in order on the test's thread, through the code a queue's thread runs
//! (`TurnWriting::stepped`); the threaded module has its own tests where the thread is the point.
//! Cards are retired (danielrispler/plotkeeper#348): a turn's end writes its summary, its Goal and Design so far.

use crate::common::{Answer, Scripted};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use studio_core::{CardRequest, Kind, NameRequest, Output, Studio, SummaryRequest};
use studio_test_support::git;
use studio_understanding::{
    CardStore, CardWriter, Core, Effects, FakeWriter, Fingerprints, StoredSummary, SummarySetup,
    TurnSummary, TurnWriting, Writer, fingerprint_changes, worktree_root,
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
        folder: &str,
    ) -> Result<String, String> {
        FakeWriter.name(setup, prompt, response, folder)
    }
}

/// The shell's effects, recorded with the number of cards in the store when each was told.
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

    fn name(&self, session_id: &str, name: &str) {
        let line = format!("name of {session_id}: {name}");
        self.events.lock().unwrap().push(line);
    }

    fn log(&self, line: String) {
        self.logs.lock().unwrap().push(line);
    }
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
    /// A day on a new repository with one commit and an empty store.
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
        let writing = TurnWriting::stepped(core.clone(), store.clone(), writer, recorder.clone());
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
                Output::Card(request) => self.writing.brief(request),
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
fn a_code_only_turn_with_a_short_reply_gets_no_summary_and_its_row_shows_the_replys_first_line() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add two files", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let id = card_id(&outputs);
    day.pass(outputs);

    // A summary nobody wanted is not written (danielrispler/plotkeeper#347), and no card stands in for it.
    assert_eq!(day.summary(&id), None);
    assert_eq!(day.card_ids(), Vec::<String>::new());
    assert_eq!(day.line(), "Done.");
}

// @law: session-pieces-in-order
#[test]
fn jobs_run_in_the_order_they_were_asked_for_on_the_session_queue() {
    let mut day = Day::new();
    // Two turns' requests are queued before any job runs.
    let first = day.turn_asking("One", &[("a.rs", 3), ("b.rs", 3)], &long_reply());
    let second = day.turn_asking("Two", &[("c.rs", 3), ("d.rs", 3)], &long_reply());
    let (a, b) = (card_id(&first), card_id(&second));
    for output in first.into_iter().chain(second) {
        match output {
            Output::Card(request) => day.writing.brief(request),
            Output::Summary(request) => day.writing.summary(request),
            _ => {}
        }
    }
    assert_eq!(
        day.events(),
        Vec::<String>::new(),
        "nothing runs before the test runs it"
    );
    day.writing.run_queued();

    // The Goal reads the prompts when its job runs, so the first job's covers both turns; then the two summaries, in order.
    let written: Vec<_> = (day.events().into_iter())
        .filter(|e| e.ends_with("written"))
        .collect();
    assert_eq!(
        written,
        ["goal written", "summary written", "summary written"]
    );
    assert!(day.summary(&a).is_some() && day.summary(&b).is_some());
}

/// Summaries are written on open, and the turn of a small change waits.
fn on_open(day: &mut Day) -> String {
    day.writing.settings(SummarySetup::default(), true);
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], &long_reply());
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
fn a_failed_write_on_open_parks_the_turn_again_and_the_next_opening_writes_it() {
    let mut day = Day::new();
    let id = on_open(&mut day);
    day.failing.store(true, Ordering::SeqCst);
    assert_eq!(day.writing.opened(&id), Err(format!("no summary on {id}")));
    assert_eq!(day.summary(&id), None);

    day.failing.store(false, Ordering::SeqCst);
    let written = day.writing.opened(&id).unwrap();
    assert!(written.is_some());
    // The failed write was tried twice (the writers retry once), then the one that worked.
    assert_eq!(day.summaries.load(Ordering::SeqCst), 3);
}

#[test]
fn a_session_is_named_on_its_queue_from_its_first_prompt() {
    let mut day = Day::new();
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Fix the build" }));
    assert!(outputs.iter().any(|o| matches!(o, Output::Name(_))));
    day.pass(outputs);

    assert_eq!(day.events(), ["name of s1: Sample session name"]);
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
    let core: Core = Arc::new(Mutex::new(Studio::default()));
    let writing = TurnWriting::stepped(core, store, writer, recorder);
    (Arc::new(writing), data)
}

#[test]
fn every_entry_point_returns_within_a_few_seconds_against_a_core_with_no_such_turn() {
    let (writing, _data) = on_an_empty_core();
    let called = |what: &'static str, call: fn(&TurnWriting)| {
        let writing = writing.clone();
        returns_in_time(what, move || call(&writing));
    };
    called("brief", |w| w.brief(request("s1@1", &diff(&["a.rs"]))));
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
            cwd: String::new(),
        };
        w.name(request);
    });
    called("opened", |w| assert_eq!(w.opened("s1@6"), Ok(None)));
    called("setup", |w| drop(w.setup()));
    // The jobs they queued run too, a brief's read of its session included, and end with nothing written.
    called("the queued jobs", |w| assert_eq!(w.run_queued(), 3));
    // A summary of a session that is not there parks nothing, so opening its turn finds none.
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
    assert_eq!(opened, Ok(None));
}

#[test]
fn the_threaded_module_writes_the_goal_on_its_sessions_queue_and_its_queue_ends() {
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
        day.recorder.clone(),
    );
    for output in outputs {
        if let Output::Card(request) = output {
            threaded.brief(request);
        }
    }
    for _ in 0..500 {
        if day.store.lock().unwrap().brief("s1").is_some() && threaded.queues() == 0 {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        day.store
            .lock()
            .unwrap()
            .brief("s1")
            .is_some_and(|b| b.goal.is_some())
    );
    assert_eq!(threaded.queues(), 0, "no queue is left behind");
}

#[test]
fn a_turn_opened_in_an_ended_session_gets_its_summary_and_design_and_its_queue_ends() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], &long_reply());
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(Recorded {
            events: day.events.clone(),
            failing: Arc::default(),
            summaries: Arc::default(),
            why: Arc::new(AtomicBool::new(true)),
        }),
        day.recorder.clone(),
    );
    threaded.settings(SummarySetup::default(), true);
    let id = card_id(&outputs);
    for output in outputs {
        match output {
            Output::Card(request) | Output::Brief(request) => threaded.brief(request),
            Output::Summary(request) => threaded.summary(request),
            _ => {}
        }
    }
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
fn slow(events: &Events) -> Scripted {
    let events = events.clone();
    Scripted {
        name: Answer::run(move |prompt| {
            events.lock().unwrap().push(format!("start {prompt}"));
            thread::sleep(Duration::from_millis(40));
            events.lock().unwrap().push(format!("end {prompt}"));
            None
        }),
        ..Scripted::fake()
    }
}

#[test]
fn two_jobs_queued_for_an_ended_session_run_one_at_a_time_in_order() {
    let mut day = Day::new();
    day.event("UserPromptSubmit", json!({ "prompt": "Fix the build" }));
    let events = Events::default();
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(slow(&events)),
        day.recorder.clone(),
    );
    for prompt in ["one", "two"] {
        threaded.name(NameRequest {
            session_id: "s1".into(),
            prompt: prompt.into(),
            response: None,
            cwd: String::new(),
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

/// A writer whose name panics for the prompt "boom".
fn panicking(events: &Events) -> Scripted {
    let events = events.clone();
    Scripted {
        name: Answer::run(move |prompt| {
            if prompt == "boom" {
                // Long enough for the next job to be queued behind this one.
                thread::sleep(Duration::from_millis(100));
            }
            assert_ne!(prompt, "boom", "the writer panics");
            events.lock().unwrap().push(format!("named {prompt}"));
            None
        }),
        ..Scripted::fake()
    }
}

#[test]
fn a_job_that_panics_does_not_stop_the_later_jobs_of_its_session() {
    let mut day = Day::new();
    day.event("UserPromptSubmit", json!({ "prompt": "Fix the build" }));
    let events = Events::default();
    let threaded = TurnWriting::new(
        day.core.clone(),
        day.store.clone(),
        Arc::new(panicking(&events)),
        day.recorder.clone(),
    );
    let name = |prompt: &str| {
        threaded.name(NameRequest {
            session_id: "s1".into(),
            prompt: prompt.into(),
            response: None,
            cwd: String::new(),
        });
    };
    name("boom");
    name("after");
    for _ in 0..1000 {
        if !events.lock().unwrap().is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(*events.lock().unwrap(), ["named after"]);
    let logs = day.recorder.logs.lock().unwrap();
    assert_eq!(logs.len(), 1, "{logs:?}");
}

// The turns of the stand-in's tests (`prompt_cards.rs`, danielrispler/plotkeeper#261).

/// The fake writer, keeping what each Goal read: its prompts.
#[derive(Clone, Default)]
struct Reading {
    goals: Arc<Mutex<Vec<Vec<String>>>>,
}

struct ReadingWriter {
    recorded: Recorded,
    reading: Reading,
}

impl CardWriter for ReadingWriter {
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
        folder: &str,
    ) -> Result<String, String> {
        self.recorded.name(setup, prompt, response, folder)
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

/// Worker `w1` in a worktree of its own writes `a.rs` and `b.rs` and ends.
fn worker_writes_and_ends(day: &mut Day) {
    day.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = day.worktree("w1");
    day.child("w1", &tree, "SubagentStart", json!({}));
    put(&tree, &["a.rs", "b.rs"]);
    day.child("w1", &tree, "SubagentStop", json!({}));
}

/// A killed worker has no `SubagentStop`; its end is read from its wake-up, which also opens the session's next turn and
/// takes that turn's base.
#[test]
fn a_killed_workers_goal_is_written_at_the_wake_up_that_reports_it() {
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

    // The worker's, from its spawn, and the session's, from Daniel's prompt.
    assert_eq!(reading.goals.lock().unwrap().len(), 2);
}

#[test]
fn a_wake_up_turn_that_changed_nothing_itself_writes_its_goal_from_daniels_prompts() {
    let (mut day, reading) = reading_day();
    day.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    day.take("Stop", json!({}));
    worker_writes_and_ends(&mut day);
    let wake_up =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    day.take("UserPromptSubmit", json!({ "prompt": wake_up }));
    day.take("Stop", json!({ "last_assistant_message": "w1 is done." }));

    // The worker's Goal reads its spawn; the session's, Daniel's prompt and never the wake-up.
    assert_eq!(
        *reading.goals.lock().unwrap(),
        [vec!["…"], vec!["Implement #378"]]
    );
    assert_eq!(day.line(), "w1 is done.");
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
fn a_prompt_that_changes_nothing_makes_no_writer_call_and_one_that_commits_gets_its_goal() {
    let mut day = Day::new();

    // Daniel's own edit before the prompt is not the prompt's change.
    put(&day.repo, &["x.rs", "y.rs"]);
    day.take("UserPromptSubmit", json!({}));
    day.take("Stop", json!({}));
    let writes = |day: &Day| day.events().into_iter().filter(|e| e.ends_with("written"));
    assert_eq!(writes(&day).count(), 0);

    day.take("UserPromptSubmit", json!({}));
    put(&day.repo, &["a.rs", "b.rs"]);
    git(&day.repo, &["add", "."]);
    git(&day.repo, &["commit", "-qm", "Add a and b"]);
    day.take("Stop", json!({}));

    assert_eq!(writes(&day).collect::<Vec<_>>(), ["goal written"]);
    assert_eq!(day.card_ids(), Vec::<String>::new());
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
fn a_stopped_row_shows_the_agents_first_line_and_the_turns_writing_leaves_it_so() {
    let mut day = Day::new();
    day.take("UserPromptSubmit", json!({ "prompt": "Rename a" }));
    put(&day.repo, &["lib.rs", "m.rs"]);
    assert_eq!(day.line(), "No step yet");

    let message = "\nRenamed `a` to `b`.\n\nThe tests pass.";
    let outputs = day.event("Stop", json!({ "last_assistant_message": message }));
    assert_eq!(day.line(), "Renamed `a` to `b`.");

    // No card comes to replace it (danielrispler/plotkeeper#348).
    day.pass(outputs);
    assert_eq!(day.line(), "Renamed `a` to `b`.");

    // A turn that changed nothing: the row shows the agent's first line.
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

#[test]
fn a_prompt_sent_while_a_brief_job_waited_is_in_the_goal_that_job_writes() {
    let (mut day, reading) = reading_day();
    let outputs = day.turn_asking("Add a", &[("a.rs", 3), ("b.rs", 3)], "Done.");
    let card = outputs.into_iter().find_map(|o| match o {
        Output::Card(request) => Some(request),
        _ => None,
    });
    day.writing.brief(card.expect("a card request"));
    // Daniel's next prompt comes while the job waits.
    day.event("UserPromptSubmit", json!({ "prompt": "Add b" }));
    day.writing.run_queued();

    assert_eq!(*reading.goals.lock().unwrap(), [vec!["Add a", "Add b"]]);
}

#[test]
fn a_session_deleted_while_its_brief_waited_gets_nothing_and_the_queue_goes_on_to_the_next_job() {
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
    day.writing.brief(gone);
    day.writing.brief(next);
    day.event("StudioDelete", json!({}));

    assert_eq!(day.writing.run_queued(), 2);
    let store = day.store.lock().unwrap();
    assert!(store.brief("s1").is_none() && store.brief("s2").is_some());
}

#[test]
fn a_session_deleted_while_its_summary_waited_gets_no_summary_or_writer_call() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add one file", &[("a.rs", 10)], &long_reply());
    let id = card_id(&outputs);
    for output in outputs {
        if let Output::Summary(request) = output {
            day.writing.summary(request);
        }
    }
    day.event("StudioDelete", json!({}));
    day.writing.run_queued();

    assert_eq!(day.summary(&id), None);
    assert_eq!(day.summaries.load(Ordering::SeqCst), 0);
}

#[test]
fn a_session_deleted_while_its_name_waited_is_not_named() {
    let mut day = Day::new();
    let outputs = day.event("UserPromptSubmit", json!({ "prompt": "Fix the build" }));
    for output in outputs {
        if let Output::Name(request) = output {
            day.writing.name(request);
        }
    }
    day.event("StudioDelete", json!({}));
    day.writing.run_queued();

    assert_eq!(day.events(), Vec::<String>::new());
}

// Cards are retired (spec #334, ticket #348): a turn's end asks the writer for its summary, its Goal and Design so far, and for no card.
#[test]
fn a_turns_end_writes_only_its_summary_and_the_briefs_pieces_and_no_card() {
    let mut day = Day::new();
    let outputs = day.turn_asking("Add two files", &[("a.rs", 3), ("b.rs", 3)], &long_reply());
    let id = card_id(&outputs);
    day.pass(outputs);

    assert!(day.summary(&id).is_some(), "the turn's summary is written");
    assert_eq!(day.card_ids(), Vec::<String>::new());
    // The writer was asked for the Goal and the summary, and for nothing else: no card.
    let written: Vec<_> = day
        .events()
        .into_iter()
        .filter(|e| e.ends_with("written"))
        .collect();
    assert_eq!(written, ["goal written", "summary written"]);
}
