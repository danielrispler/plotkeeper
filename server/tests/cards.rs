//! A recorded log through the server with a fake writer (danielrispler/plotkeeper#328): a turn that ended while plotkeeper was
//! stopped gets its card inside the catch-up window and none outside it, and a writer failure reaches the page with what to do.

use serde_json::json;
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use studio_server::{App, Follower, Sink};
use studio_test_support::git;
use studio_understanding::{CardStore, ClaudeWriter, Effects, FakeWriter, RiskNotes};

struct NoEffects;

impl Effects for NoEffects {
    fn list(&self) {}
    fn new_card(&self, _: &str, _: Option<&str>) {}
    fn settle_checks(&self) {}
    fn name(&self, _: &str, _: &str) {}
    fn log(&self, _: String) {}
}

#[derive(Default)]
struct Heard(Mutex<Vec<(String, serde_json::Value)>>);

impl Sink for Heard {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((name.into(), payload));
    }
    fn focused(&self) -> bool {
        false
    }
    fn badge(&self, _: Option<usize>) {}
    fn notify(&self, _: &str, _: &str) {}
    fn play(&self, _: studio_core::Sound) {}
    fn replayed(&self) {}
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn app_with(log: &Path, writer: studio_understanding::Writer) -> (Arc<App>, Arc<Heard>) {
    let cards = Arc::new(Mutex::new(CardStore::open_beside(log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(log).unwrap()));
    let app = App::open_with(log, cards, notes, writer, Arc::new(NoEffects)).unwrap();
    let heard = Arc::new(Heard::default());
    app.events.set(heard.clone());
    (Arc::new(app), heard)
}

/// A repository whose one turn, with the trees the hook records, adds two logic files; its log has the turn ending at `ended`.
fn log_with_turn(dir: &Path, ended: u64) -> std::path::PathBuf {
    let repo = dir.join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    fs::write(repo.join("lib.rs"), "fn a() {}\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "Start"]);
    let (root, start) = studio_hook::record_tree(&repo).expect("a tree");
    for path in ["a.rs", "b.rs"] {
        fs::write(repo.join(path), "let v0 = 0;\nlet v1 = 1;\nlet v2 = 2;\n").unwrap();
    }
    let (_, end) = studio_hook::record_tree(&repo).expect("a tree");
    let line = |name: &str, at: u64, extra: serde_json::Value| {
        let mut line = json!({"session_id": "s1", "prompt_id": "p1", "hook_event_name": name,
            "cwd": repo, "studio_received_ms": at, "prompt": "Add two files"});
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line.to_string()
    };
    let text = [
        line(
            "UserPromptSubmit",
            ended - 2,
            json!({"studio_turn_start": {&root: start}}),
        ),
        line("Stop", ended, json!({"studio_turn_end": {&root: end}})),
    ];
    let log = dir.join("events.jsonl");
    fs::write(&log, text.join("\n") + "\n").unwrap();
    log
}

fn card_ids(app: &App) -> Vec<String> {
    let store = app.cards.lock().unwrap();
    store.cards().iter().map(|c| c.request.id.clone()).collect()
}

fn wait_for_cards(app: &App) -> Vec<String> {
    for _ in 0..400 {
        let ids = card_ids(app);
        if !ids.is_empty() {
            return ids;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Vec::new()
}

#[test]
fn a_turn_that_ended_while_stopped_gets_its_card_from_the_writer_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let ended = now() - 60_000;
    let log = log_with_turn(dir.path(), ended);
    let (app, _) = app_with(&log, Arc::new(FakeWriter));

    Follower::start(&app, &log).pass();

    assert_eq!(wait_for_cards(&app), [format!("s1@{ended}")]);
    let store = app.cards.lock().unwrap();
    let card = &store.cards()[0].card;
    assert!(
        card.summary.starts_with("Sample change to"),
        "{}",
        card.summary
    );
}

#[test]
fn a_turn_older_than_the_window_gets_no_card_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let log = log_with_turn(dir.path(), now() - 3 * 60 * 60 * 1000);
    let (app, _) = app_with(&log, Arc::new(FakeWriter));

    Follower::start(&app, &log).pass();

    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(card_ids(&app), Vec::<String>::new());
}

#[test]
fn a_writer_that_is_not_logged_in_shows_what_to_do_on_the_page_and_clears_when_it_works() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let program = dir.path().join("claude");
    let write = |body: &str| {
        fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    };
    write("echo 'Not logged in. Please run /login'; exit 1");
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let writer = ClaudeWriter::new(program.clone(), dir.path().into())
        .on_availability(move |notice| tx.lock().unwrap().send(notice).unwrap());
    let (app, heard) = app_with(&log, Arc::new(writer));
    studio_server::forward_writer_notices(&app, rx);

    assert!(app.writer.goal(&["hello"]).is_err());
    write("echo ok");
    assert!(app.writer.goal(&["hello"]).is_ok());

    let wait = || {
        for _ in 0..100 {
            let events = heard.0.lock().unwrap();
            if events.len() >= 2 {
                return events.clone();
            }
            drop(events);
            std::thread::sleep(Duration::from_millis(50));
        }
        heard.0.lock().unwrap().clone()
    };
    let events = wait();
    assert_eq!(events.len(), 2, "{events:?}");
    let shown = events[0].1.as_str().unwrap();
    assert!(
        events[0].0 == "writer-notice"
            && shown.contains("not logged in")
            && shown.contains("/login"),
        "{shown}"
    );
    assert_eq!(
        events[1],
        ("writer-notice".to_string(), serde_json::Value::Null)
    );
}

struct NoPage;

impl studio_server::api::Page for NoPage {
    fn focus(&self, _: bool) {}
}

#[test]
fn a_page_opened_after_the_notice_asks_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let (tx, rx) = mpsc::channel();
    let (app, heard) = app_with(&log, Arc::new(FakeWriter));
    studio_server::forward_writer_notices(&app, rx);
    let ask = || studio_server::api::call(&app, &NoPage, "writer_notice", serde_json::Value::Null);
    assert_eq!(ask().unwrap(), serde_json::Value::Null);

    tx.send(Some(
        "Cards and briefs are off: `claude` is not logged in.".into(),
    ))
    .unwrap();
    while heard.0.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        ask().unwrap(),
        json!("Cards and briefs are off: `claude` is not logged in.")
    );

    tx.send(None).unwrap();
    while heard.0.lock().unwrap().len() < 2 {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ask().unwrap(), serde_json::Value::Null);
}
