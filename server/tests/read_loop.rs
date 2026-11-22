//! The read loop over a recorded log into a fake sink (danielrispler/plotkeeper#324): what it sends, when it sends it again,
//! and where its focus input comes from.

use std::{
    io::Write,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use studio_core::Sound;
use studio_server::{App, Follower, Sink};
use studio_understanding::{CardStore, Effects, FakeWriter, RiskNotes};

const LOG: &str = include_str!("../../core/tests/fixtures/two-sessions.jsonl");
const OTHER: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";
const FIRST: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";

#[derive(Default)]
struct FakeSink {
    events: Mutex<Vec<(String, serde_json::Value)>>,
    badges: Mutex<Vec<Option<usize>>>,
    focused: AtomicBool,
    replayed: AtomicBool,
}

impl Sink for FakeSink {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.events.lock().unwrap().push((name.into(), payload));
    }
    fn focused(&self) -> bool {
        self.focused.load(Ordering::Acquire)
    }
    fn badge(&self, count: Option<usize>) {
        self.badges.lock().unwrap().push(count);
    }
    fn notify(&self, _: &str, _: &str) {}
    fn play(&self, _: Sound) {}
    fn replayed(&self) {
        self.replayed.store(true, Ordering::Release);
    }
}

impl FakeSink {
    /// The payloads of event `name` sent so far, then forgotten.
    fn take(&self, name: &str) -> Vec<serde_json::Value> {
        let mut events = self.events.lock().unwrap();
        let (taken, kept) = std::mem::take(&mut *events)
            .into_iter()
            .partition::<Vec<_>, _>(|(n, _)| n == name);
        *events = kept;
        taken.into_iter().map(|(_, p)| p).collect()
    }
}

struct NoEffects;

impl Effects for NoEffects {
    fn list(&self) {}
    fn new_card(&self, _: &str, _: Option<&str>) {}
    fn settle_checks(&self) {}
    fn name(&self, _: &str, _: &str) {}
    fn log(&self, _: String) {}
}

fn app(log: &Path) -> (Arc<App>, Arc<FakeSink>) {
    let cards = Arc::new(Mutex::new(CardStore::open_beside(log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(log).unwrap()));
    let app = App::open_with(log, cards, notes, Arc::new(FakeWriter), Arc::new(NoEffects)).unwrap();
    let sink = Arc::new(FakeSink::default());
    app.events.set(sink.clone());
    (Arc::new(app), sink)
}

fn ids(list: &serde_json::Value) -> Vec<&str> {
    let mut ids: Vec<_> = (list.as_array().unwrap().iter())
        .map(|row| row["id"].as_str().unwrap())
        .collect();
    ids.sort();
    ids
}

#[test]
fn the_replay_sends_the_list_once_and_a_live_line_sends_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(&log, LOG).unwrap();
    let (app, sink) = app(&log);
    let mut follower = Follower::start(&app, &log);

    follower.pass();
    let lists = sink.take("sessions");
    // The replay's list, and the one that marks the fixture's missing transcripts.
    assert_eq!(lists.len(), 2);
    assert_eq!(ids(&lists[0]), [OTHER, FIRST]);
    assert!(sink.replayed.load(Ordering::Acquire));
    assert_eq!(*sink.badges.lock().unwrap(), [None]);

    follower.pass();
    assert!(
        sink.take("sessions").is_empty(),
        "an unchanged list is not sent again"
    );

    let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    writeln!(
        file,
        r#"{{"session_id":"third","prompt_id":"p","cwd":"/nowhere","hook_event_name":"UserPromptSubmit","prompt":"Hello","studio_received_ms":1793261100000}}"#
    )
    .unwrap();
    follower.pass();
    let lists = sink.take("sessions");
    assert_eq!(lists.len(), 1);
    assert!(ids(&lists[0]).contains(&"third"));
}

#[test]
fn a_finished_turn_is_seen_only_while_the_sink_is_focused() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(&log, LOG).unwrap();
    let (app, sink) = app(&log);
    let mut follower = Follower::start(&app, &log);
    follower.pass();
    studio_server::select(&app, Some(FIRST.into()));
    let mut file = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
    writeln!(
        file,
        r#"{{"session_id":"{FIRST}","prompt_id":"e2a7c9d1-3f5b-4a8e-9c6d-0b1a2f3e4d5c","cwd":"/nowhere","hook_event_name":"Stop","last_assistant_message":"Done.","studio_received_ms":1793261100000}}"#
    )
    .unwrap();
    let unseen = |sink: &FakeSink| {
        let lists = sink.take("sessions");
        let last = lists.last().cloned().expect("the list was sent");
        let row = (last.as_array().unwrap().iter())
            .find(|row| row["id"] == FIRST)
            .cloned()
            .unwrap();
        row["unseen"].as_bool().unwrap()
    };
    follower.pass();
    assert!(
        unseen(&sink),
        "the window is not focused: the turn stays unseen"
    );
    sink.focused.store(true, Ordering::Release);
    follower.pass();
    assert!(
        !unseen(&sink),
        "the sink says it is focused: the turn is seen"
    );
}
