//! The Anthropic API key through the server (spec #320, "API-key writers"): saved, replaced and removed through the key store
//! (memory here: no test touches the real Keychain), and a planted key never reaches what the page is sent or the log, even
//! when the API's error body and headers quote it.

mod isolated_home;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    thread,
};
use studio_server::App;
use studio_understanding::{CardStore, ClaudeWriter, Effects, RiskNotes};

const PLANTED: &str = "sk-ant-planted-key-7f3a9c";
const REPLACED: &str = "sk-ant-replaced-key-41b2";

/// The log as the server keeps it (`studio_server::logged`), asked for at every level; and how many lines it dropped.
struct Logs(Mutex<Vec<String>>, Mutex<usize>);

impl log::Log for Logs {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        match studio_server::logged(record.metadata()) {
            true => (self.0.lock().unwrap()).push(format!("{} {}", record.target(), record.args())),
            false => *self.1.lock().unwrap() += 1,
        }
    }
    fn flush(&self) {}
}

static LOGS: Logs = Logs(Mutex::new(Vec::new()), Mutex::new(0));

struct NoEffects;

impl Effects for NoEffects {
    fn list(&self) {}
    fn name(&self, _: &str, _: &str) {}
    fn log(&self, line: String) {
        studio_server::report(line);
    }
}

struct NoPage;

impl studio_server::api::Page for NoPage {
    fn focus(&self, _: bool) {}
}

/// A fake Messages API that answers each request with the next of `answers` (status and body, the body and a header
/// quoting the key) and keeps the `x-api-key` each one sent.
fn fake(answers: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    let keys = Arc::new(Mutex::new(Vec::new()));
    let sent = keys.clone();
    thread::spawn(move || {
        for (status, body) in answers {
            let mut request = server.recv().unwrap();
            let mut text = String::new();
            request.as_reader().read_to_string(&mut text).unwrap();
            let key = (request.headers().iter())
                .find(|h| h.field.equiv("x-api-key"))
                .map(|h| h.value.to_string())
                .unwrap_or_default();
            sent.lock().unwrap().push(key.clone());
            let echo = tiny_http::Header::from_bytes("x-echo-key", key.as_bytes()).unwrap();
            let _ = request.respond(
                tiny_http::Response::from_string(body.replace("{key}", &key))
                    .with_status_code(status)
                    .with_header(echo),
            );
        }
    });
    (base, keys)
}

fn ok() -> String {
    json!({ "type": "message", "stop_reason": "end_turn", "content": [{ "type": "text", "text": "ok" }],
        "usage": { "input_tokens": 5, "output_tokens": 1 } })
    .to_string()
}

#[test]
fn a_planted_key_is_saved_replaced_and_removed_and_never_reaches_the_page_or_the_log() {
    log::set_logger(&LOGS).unwrap();
    log::set_max_level(log::LevelFilter::Trace);
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let refused = json!({ "type": "error", "error": { "type": "authentication_error", "message": "invalid x-api-key {key}" } });
    let (base, sent) = fake(vec![
        (401, refused.to_string()),
        (200, ok()),
        (500, refused.to_string()),
        (200, ok()),
    ]);
    let writer =
        ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into()).with_api_url(base);
    let cards = Arc::new(Mutex::new(CardStore::open_beside(&log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(&log).unwrap()));
    let app = App::open_with(&log, cards, notes, Arc::new(writer), Arc::new(NoEffects))
        .unwrap()
        .with_keys(Arc::new(studio_server::settings::MemoryKeys::default()));
    let app = Arc::new(app);
    let mut page = Vec::<Value>::new();
    let mut call = |name: &str, args: Value| {
        let reply = studio_server::api::call(&app, &NoPage, name, args).unwrap();
        page.push(reply.clone());
        reply
    };
    let test = json!({ "writer": "anthropic", "model": "claude-haiku-5-5", "effort": "", "expected": null });

    let unset = call("writers", Value::Null);
    assert_eq!(
        unset["writers"][0]["kind"], "anthropic",
        "an API key comes first: {unset}"
    );
    assert_eq!(unset["writers"][0]["found"], false, "{unset}");

    call("set_api_key", json!({ "key": PLANTED }));
    let set = call("writers", Value::Null);
    assert_eq!(set["writers"][0]["found"], true, "{set}");

    // A refused key: its kind and fix, and no word of the body or header that quoted it.
    let failed = call("test_writer", test.clone());
    assert_eq!(failed["outcome"], "not_logged_in", "{failed}");
    assert!(
        failed["line"].as_str().unwrap().contains("API key"),
        "{failed}"
    );
    let passed = call("test_writer", test.clone());
    assert_eq!(passed["outcome"], "passed", "{passed}");
    let saved = passed["settings"].as_str().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(saved).unwrap()["summaries"]["writer"],
        "anthropic"
    );

    // A summary call that fails goes to the log, as the turn writing logs it, and to the notice the page reads.
    app.writer
        .use_writer(studio_understanding::WriterKind::Anthropic);
    let line = app.writer.goal(&["hello"]).unwrap_err();
    studio_server::report(line);
    call("writer_notice", Value::Null);

    // Replaced: the next call sends the new key.
    call("set_api_key", json!({ "key": REPLACED }));
    assert!(app.writer.goal(&["hello"]).is_ok());
    assert_eq!(*sent.lock().unwrap(), [PLANTED, PLANTED, PLANTED, REPLACED]);

    call("remove_api_key", Value::Null);
    let removed = call("writers", Value::Null);
    assert_eq!(removed["writers"][0]["found"], false, "{removed}");
    assert_eq!(app.keys.get(), None);
    let error = app.writer.goal(&["hello"]).unwrap_err();
    assert!(error.contains("no API key"), "{error}");
    call("settings_files", Value::Null);

    for key in [PLANTED, REPLACED] {
        for reply in &page {
            assert!(
                !reply.to_string().contains(key),
                "the page was sent the key: {reply}"
            );
        }
        for line in LOGS.0.lock().unwrap().iter() {
            assert!(!line.contains(key), "the log holds the key: {line}");
        }
    }
    assert!(
        LOGS.0
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("HTTP 500")),
        "the failure was logged, with its status: {:?}",
        LOGS.0.lock().unwrap()
    );
}
