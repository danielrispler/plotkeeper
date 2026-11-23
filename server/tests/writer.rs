//! A recorded log through the server with a fake writer (danielrispler/plotkeeper#328): a replay changes no stored summary, and a
//! writer failure reaches the page with what to do. Cards are retired (danielrispler/plotkeeper#348): a start writes none.

mod isolated_home;
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
    fn replayed(&self) {}
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn app_with(log: &Path, writer: studio_understanding::Writer) -> (Arc<App>, Arc<Heard>) {
    isolated_home::set();
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

// @law: replay-never-rewrites
#[test]
fn replaying_a_log_twice_over_a_store_changes_no_summary_and_writes_no_card() {
    let dir = tempfile::tempdir().unwrap();
    let ended = now() - 60_000;
    let log = log_with_turn(dir.path(), ended);
    let id = format!("s1@{ended}");
    // The first start reads the log; the turn has a summary.
    let (app, _) = app_with(&log, Arc::new(FakeWriter));
    Follower::start(&app, &log).pass();
    {
        let mut store = app.cards.lock().unwrap();
        let request = studio_core::SummaryRequest {
            session_id: "s1".into(),
            id: id.clone(),
            ..Default::default()
        };
        let summary = serde_json::from_value(json!({"summary": "Added two files"})).unwrap();
        store.add_summary_at(request, summary, ended + 10).unwrap();
    }
    drop(app);
    let summaries = || fs::read(dir.path().join("summaries.json")).unwrap();
    let before = summaries();
    assert!(String::from_utf8_lossy(&before).contains("Added two files"));

    for _ in 0..2 {
        let (app, _) = app_with(&log, Arc::new(FakeWriter));
        Follower::start(&app, &log).pass();
        std::thread::sleep(Duration::from_secs(1));
        assert_eq!(app.cards.lock().unwrap().summaries().len(), 1);
        drop(app);
    }

    assert_eq!(summaries(), before);
    assert!(
        !dir.path().join("cards-v2.json").exists(),
        "a start writes no card"
    );
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

    tx.send(Some("Summaries are off: `claude` is not logged in.".into()))
        .unwrap();
    while heard.0.lock().unwrap().is_empty() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        ask().unwrap(),
        json!("Summaries are off: `claude` is not logged in.")
    );

    tx.send(None).unwrap();
    while heard.0.lock().unwrap().len() < 2 {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(ask().unwrap(), serde_json::Value::Null);
}

// Picking and testing the writer on the page (spec #320, "Writer settings on the page" and "The Test gate").

#[test]
fn a_passed_test_saves_the_choice_and_turns_summaries_on_and_a_failed_one_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let agy = dir.path().join("agy");
    let script = |body: &str| {
        fs::write(&agy, format!("#!/bin/sh\n{body}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&agy, fs::Permissions::from_mode(0o755)).unwrap();
    };
    let writer = ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into())
        .with_agy(agy.clone(), dir.path().into());
    let (app, _) = app_with(&log, Arc::new(writer));
    studio_server::set_summaries_on(&app, false);
    let settings = studio_server::settings::paths().unwrap()[0].clone();
    let test = |expected: Option<&str>| {
        studio_server::api::call(
            &app,
            &NoPage,
            "test_writer",
            json!({ "writer": "agy", "model": "gemini-4-neon-high", "effort": "", "expected": expected }),
        )
        .unwrap()
    };

    script(r#"echo '{"status":"ERROR","error":"please log in"}'; exit 1"#);
    let failed = test(None);
    assert_eq!(failed["outcome"], "not_logged_in", "{failed}");
    assert!(
        failed["line"].as_str().unwrap().contains("Log in to `agy`"),
        "{failed}"
    );
    assert_eq!(failed["settings"], serde_json::Value::Null);
    assert!(!settings.exists(), "a failed Test saves nothing");
    let off = studio_server::api::call(&app, &NoPage, "writers", serde_json::Value::Null).unwrap();
    assert_eq!(off["summaries_on"], false, "{off}");

    script(r#"echo '{"status":"SUCCESS","response":"ok"}'"#);
    let passed = test(None);
    assert_eq!(passed["outcome"], "passed", "{passed}");
    let saved = passed["settings"].as_str().unwrap();
    assert_eq!(fs::read_to_string(&settings).unwrap(), saved);
    let saved: serde_json::Value = serde_json::from_str(saved).unwrap();
    assert_eq!(
        saved["summaries"],
        json!({ "writer": "agy", "model": "gemini-4-neon-high" })
    );
    let on = studio_server::api::call(&app, &NoPage, "writers", serde_json::Value::Null).unwrap();
    assert_eq!(on["summaries_on"], true, "{on}");
    assert_eq!(on["writers"][1]["kind"], "agy", "{on}");
    // The next start reads the gate open.
    let data = settings.parent().unwrap();
    fs::remove_file(&settings).unwrap();
    assert!(studio_server::settings::summaries_open(data));
}
