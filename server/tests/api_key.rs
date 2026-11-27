//! The API keys through the server (spec #320, "API-key writers"): the Anthropic key, and the OpenAI-compatible API's key
//! and base URL (#367), saved, replaced and removed through the key store (memory here: no test touches the real Keychain),
//! and a planted key never reaches what the page is sent or the log, even when the API's error body and headers quote it.

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
const OPENAI_PLANTED: &str = "sk-proj-planted-key-93d1";

/// The log as the server keeps it (`studio_server::logged`), asked for at every level; and how many lines it dropped.
struct Logs(Mutex<Vec<String>>, Mutex<usize>);

impl log::Log for Logs {
    fn enabled(&self, _: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        if studio_server::logged(record.metadata()) {
            (self.0.lock().unwrap()).push(format!("{} {}", record.target(), record.args()));
        } else {
            *self.1.lock().unwrap() += 1;
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

/// A fake API that answers each request with the next of `answers` (status and body, the body and a header quoting the
/// key) and keeps the key each one sent, as `x-api-key` or a bearer `authorization`; none sent is kept as "".
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
            let header = |name: &'static str| {
                (request.headers().iter())
                    .find(|h| h.field.equiv(name))
                    .map(|h| h.value.to_string())
            };
            let bearer =
                header("authorization").map(|b| b.trim_start_matches("Bearer ").to_string());
            let key = header("x-api-key").or(bearer).unwrap_or_default();
            sent.lock().unwrap().push(key.clone());
            let echo = tiny_http::Header::from_bytes("x-echo-key", key.as_bytes()).unwrap();
            // The client may have hung up; there is no one left to answer.
            request
                .respond(
                    tiny_http::Response::from_string(body.replace("{key}", &key))
                        .with_status_code(status)
                        .with_header(echo),
                )
                .ok();
        }
    });
    (base, keys)
}

fn chat_ok() -> String {
    json!({ "choices": [{ "finish_reason": "stop", "message": { "role": "assistant", "content": "ok" } }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 1 } })
    .to_string()
}

fn ok() -> String {
    json!({ "type": "message", "stop_reason": "end_turn", "content": [{ "type": "text", "text": "ok" }],
        "usage": { "input_tokens": 5, "output_tokens": 1 } })
    .to_string()
}

#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one test scenario, read top to bottom"
)]
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

    call(
        "set_api_key",
        json!({ "writer": "anthropic", "key": PLANTED, "base_url": null, "expected": null }),
    );
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
    call(
        "set_api_key",
        json!({ "writer": "anthropic", "key": REPLACED, "base_url": null, "expected": null }),
    );
    assert!(app.writer.goal(&["hello"]).is_ok());
    assert_eq!(*sent.lock().unwrap(), [PLANTED, PLANTED, PLANTED, REPLACED]);

    call(
        "remove_api_key",
        json!({ "writer": "anthropic", "expected": null }),
    );
    let removed = call("writers", Value::Null);
    assert_eq!(removed["writers"][0]["found"], false, "{removed}");
    assert_eq!(
        app.keys.get(studio_understanding::WriterKind::Anthropic),
        None
    );
    let error = app.writer.goal(&["hello"]).unwrap_err();
    assert!(error.contains("no API key"), "{error}");

    // The OpenAI-compatible API (#367): its base URL and key saved together, its key sent as a bearer, a refused one read
    // as having no working key; removed, both are gone; a base URL alone, as for a local Ollama, sends no key.
    let (openai_base, openai_sent) = fake(vec![
        (401, refused.to_string()),
        (200, chat_ok()),
        (200, chat_ok()),
    ]);
    // The page sends the settings text it last read, as `expected`.
    let read = |call: &mut dyn FnMut(&str, Value) -> Value| {
        call("settings_files", Value::Null)["settings"].clone()
    };
    let openai_test = |expected: Value| json!({ "writer": "openai", "model": "llama3.2", "effort": "", "expected": expected });
    let unset = call("writers", Value::Null);
    assert_eq!(unset["writers"][1]["kind"], "openai", "{unset}");
    assert_eq!(unset["writers"][1]["found"], false, "{unset}");
    let expected = read(&mut call);
    let saved = call(
        "set_api_key",
        json!({ "writer": "openai", "key": OPENAI_PLANTED, "base_url": openai_base, "expected": expected }),
    );
    assert!(
        saved
            .as_str()
            .unwrap()
            .contains(&format!("\"base_url\": \"{openai_base}\"")),
        "settings.json keeps the base URL: {saved}"
    );
    let set = call("writers", Value::Null);
    assert_eq!(set["writers"][1]["found"], true, "{set}");
    assert_eq!(set["keys"], json!(["openai"]), "{set}");
    assert_eq!(set["base_url"], openai_base, "{set}");
    let expected = read(&mut call);
    let failed = call("test_writer", openai_test(expected));
    assert_eq!(failed["outcome"], "not_logged_in", "{failed}");
    assert!(
        failed["line"].as_str().unwrap().contains("API key"),
        "{failed}"
    );
    let expected = read(&mut call);
    let passed = call("test_writer", openai_test(expected));
    assert_eq!(passed["outcome"], "passed", "{passed}");

    let expected = read(&mut call);
    call(
        "remove_api_key",
        json!({ "writer": "openai", "expected": expected }),
    );
    let removed = call("writers", Value::Null);
    assert_eq!(removed["writers"][1]["found"], false, "{removed}");
    assert_eq!(
        (&removed["keys"], &removed["base_url"]),
        (&json!([]), &Value::Null),
        "{removed}"
    );
    assert_eq!(app.keys.get(studio_understanding::WriterKind::OpenAI), None);

    let expected = read(&mut call);
    call(
        "set_api_key",
        json!({ "writer": "openai", "key": null, "base_url": openai_base, "expected": expected }),
    );
    let keyless = call("writers", Value::Null);
    assert_eq!(keyless["writers"][1]["found"], true, "{keyless}");
    assert_eq!(keyless["keys"], json!([]), "{keyless}");
    let expected = read(&mut call);
    let passed = call("test_writer", openai_test(expected));
    assert_eq!(passed["outcome"], "passed", "{passed}");
    assert_eq!(
        *openai_sent.lock().unwrap(),
        [OPENAI_PLANTED, OPENAI_PLANTED, ""]
    );
    // A base URL that is no URL is refused, and nothing is saved.
    let expected = read(&mut call);
    let bad = studio_server::api::call(
        &app,
        &NoPage,
        "set_api_key",
        json!({ "writer": "openai", "key": OPENAI_PLANTED, "base_url": "localhost:11434", "expected": expected }),
    );
    assert!(bad.is_err());
    assert_eq!(app.keys.get(studio_understanding::WriterKind::OpenAI), None);
    call("settings_files", Value::Null);

    for key in [PLANTED, REPLACED, OPENAI_PLANTED] {
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

#[test]
fn a_key_the_store_refuses_leaves_the_base_url_unsaved() {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let writer = ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into());
    let cards = Arc::new(Mutex::new(CardStore::open_beside(&log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(&log).unwrap()));
    let app = App::open_with(&log, cards, notes, Arc::new(writer), Arc::new(NoEffects))
        .unwrap()
        .with_keys(Arc::new(studio_server::settings::MemoryKeys::default()));
    let before = studio_server::settings::settings_files()["settings"].clone();

    let refused = studio_server::set_api_key(
        &app,
        studio_understanding::WriterKind::OpenAI,
        Some("not a key".into()),
        Some("http://localhost:11434/v1".into()),
        before.as_str().map(str::to_owned),
    );
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        studio_server::settings::settings_files()["settings"],
        before
    );
}

#[test]
fn a_pass_whose_choice_was_not_saved_leaves_summaries_off() {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let (base, _) = fake(vec![(200, ok())]);
    let writer =
        ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into()).with_api_url(base);
    let cards = Arc::new(Mutex::new(CardStore::open_beside(&log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(&log).unwrap()));
    let app = App::open_with(&log, cards, notes, Arc::new(writer), Arc::new(NoEffects))
        .unwrap()
        .with_keys(Arc::new(studio_server::settings::MemoryKeys::default()));
    studio_server::set_summaries_on(&app, false);
    studio_server::set_api_key(
        &app,
        studio_understanding::WriterKind::Anthropic,
        Some(PLANTED.into()),
        None,
        None,
    )
    .unwrap();

    // The page read a settings text that is no longer the file's, so the save is refused.
    let test = studio_server::test_writer(
        &app,
        studio_understanding::WriterKind::Anthropic,
        "claude-haiku-5-5".into(),
        String::new(),
        Some("{ \"stale\": true }".into()),
    );
    assert_eq!(test.outcome, "passed", "{}", test.line);
    assert_eq!(test.settings, None, "{}", test.line);
    assert!(!studio_server::writers(&app).summaries_on, "{}", test.line);
}

/// What the server says when the machine has no Secret Service to keep a key in (#510, "API keys").
const NO_STORE: &str =
    "No secret store on this machine; install a keyring and libsecret-tools, or pick a CLI writer.";

/// A fake `secret-tool` in `dir/bin`, which keeps each entry in `dir/store/<account>` and logs its arguments to `dir/log`.
/// While `dir/no-store` exists, every call prints that file's text as `secret-tool`'s error and fails, as when no Secret
/// Service runs; while `dir/hang` exists, every call waits 10 s first, as for a locked keyring's unlock prompt that no
/// one sees. The real Secret Service is never touched.
fn fake_secret_tool(dir: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(dir.join("store")).unwrap();
    let program = bin.join("secret-tool");
    std::fs::write(
        &program,
        format!(
            r#"#!/bin/sh
PATH=/usr/bin:/bin
dir='{dir}'
echo "ARGS $*" >> "$dir/log"
if [ -f "$dir/hang" ]; then sleep 10; fi
if [ -f "$dir/no-store" ]; then cat "$dir/no-store" >&2; exit 1; fi
for word; do account=$word; done
case "$1" in
  store) cat > "$dir/store/$account";;
  lookup) [ -f "$dir/store/$account" ] && cat "$dir/store/$account" || exit 1;;
  clear) rm -f "$dir/store/$account";;
  *) exit 2;;
esac
"#,
            dir = dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn app_with(dir: &std::path::Path, keys: Arc<dyn studio_server::settings::ApiKeys>) -> Arc<App> {
    let log = dir.join("events.jsonl");
    let writer = ClaudeWriter::new(dir.join("no-claude"), dir.into());
    let cards = Arc::new(Mutex::new(CardStore::open_beside(&log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(&log).unwrap()));
    let app = App::open_with(&log, cards, notes, Arc::new(writer), Arc::new(NoEffects))
        .unwrap()
        .with_keys(keys);
    Arc::new(app)
}

/// A command's refusal, as the page shows it.
fn refusal(app: &Arc<App>, name: &str, args: Value) -> String {
    match studio_server::api::call(app, &NoPage, name, args) {
        Err(studio_server::api::Failure::Failed(text)) => text,
        other => panic!("{name} was not refused: {other:?}"),
    }
}

/// On Linux the keys are kept in the Secret Service through `secret-tool` (#510, "API keys"), here a fake on the PATH: a
/// key saved before is read at start, one saved goes in on standard input and one removed is gone, the page is told the
/// store's name, and a machine with no Secret Service running is named as such. No key is ever in `secret-tool`'s arguments.
#[test]
fn the_secret_service_keeps_the_keys_through_secret_tool_on_the_path() {
    use studio_understanding::WriterKind::{Anthropic, OpenAI};
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_secret_tool(dir.path());
    // SAFETY: nextest runs each test in a process of its own, and no thread has started yet.
    unsafe { std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display())) };
    let stored =
        |account: &str| std::fs::read_to_string(dir.path().join("store").join(account)).ok();
    // A key an earlier run saved.
    std::fs::write(dir.path().join("store/anthropic-api-key"), PLANTED).unwrap();

    // The server is given the Secret Service on Linux and the Keychain on macOS, which this does not ask.
    let os = if cfg!(target_os = "macos") {
        "the macOS Keychain"
    } else {
        "the Secret Service"
    };
    assert_eq!(studio_server::settings::os_keys().store(), os);

    // Read at start.
    let app = app_with(
        dir.path(),
        Arc::new(studio_server::settings::SecretTool::default()),
    );
    let start = studio_server::api::call(&app, &NoPage, "writers", Value::Null).unwrap();
    assert_eq!(start["key_store"], "the Secret Service", "{start}");
    assert_eq!(start["keys"], json!(["anthropic"]), "{start}");
    assert_eq!(app.keys.get(Anthropic).as_deref(), Some(PLANTED));

    // Saved and replaced, each exactly as pasted.
    let save = |writer: &str, key: &str| {
        studio_server::api::call(
            &app,
            &NoPage,
            "set_api_key",
            json!({ "writer": writer, "key": key, "base_url": null, "expected": null }),
        )
    };
    save("openai", OPENAI_PLANTED).unwrap();
    assert_eq!(stored("openai-api-key").as_deref(), Some(OPENAI_PLANTED));
    save("anthropic", REPLACED).unwrap();
    assert_eq!(stored("anthropic-api-key").as_deref(), Some(REPLACED));
    assert_eq!(app.keys.get(Anthropic).as_deref(), Some(REPLACED));

    // Removed.
    studio_server::api::call(
        &app,
        &NoPage,
        "remove_api_key",
        json!({ "writer": "anthropic", "expected": null }),
    )
    .unwrap();
    assert_eq!(stored("anthropic-api-key"), None);
    let removed = studio_server::api::call(&app, &NoPage, "writers", Value::Null).unwrap();
    assert_eq!(removed["keys"], json!(["openai"]), "{removed}");

    // No Secret Service: no session bus, or one with no store on it, each as `secret-tool` says it; any other failure is
    // the store's own.
    for (error, said) in [
        (
            "secret-tool: The name org.freedesktop.secrets was not provided by any .service files",
            NO_STORE,
        ),
        (
            "secret-tool: Cannot autolaunch D-Bus without X11 $DISPLAY",
            NO_STORE,
        ),
        (
            "secret-tool: Could not connect: No such file or directory",
            NO_STORE,
        ),
        (
            "secret-tool: Cannot create an item in a locked collection",
            "The Secret Service did not save the key; check that your keyring is unlocked, then save again.",
        ),
    ] {
        std::fs::write(dir.path().join("no-store"), format!("{error}\n")).unwrap();
        let args =
            json!({ "writer": "anthropic", "key": PLANTED, "base_url": null, "expected": null });
        assert_eq!(refusal(&app, "set_api_key", args), said, "{error}");
    }
    let args = json!({ "writer": "openai", "expected": null });
    std::fs::write(
        dir.path().join("no-store"),
        "secret-tool: Cannot autolaunch D-Bus without X11 $DISPLAY\n",
    )
    .unwrap();
    assert_eq!(refusal(&app, "remove_api_key", args), NO_STORE);
    assert_eq!(
        stored("openai-api-key").as_deref(),
        Some(OPENAI_PLANTED),
        "a refused removal keeps the key"
    );
    assert_eq!(app.keys.get(OpenAI), None, "no store, no key read");

    let log = std::fs::read_to_string(dir.path().join("log")).unwrap();
    for line in log.lines() {
        for key in [PLANTED, REPLACED, OPENAI_PLANTED] {
            assert!(!line.contains(key), "a key in the arguments: {line}");
        }
        assert!(
            line.ends_with(" service plotkeeper account anthropic-api-key")
                || line.ends_with(" service plotkeeper account openai-api-key"),
            "{line}"
        );
    }
}

/// Without `secret-tool` on the PATH, start reads no key, and a save is refused with the package to install.
#[test]
fn without_secret_tool_a_save_names_the_package_to_install() {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    // SAFETY: nextest runs each test in a process of its own, and no thread has started yet.
    unsafe { std::env::set_var("PATH", &empty) };
    let app = app_with(
        dir.path(),
        Arc::new(studio_server::settings::SecretTool::default()),
    );
    assert_eq!(
        app.keys.get(studio_understanding::WriterKind::Anthropic),
        None
    );
    let args = json!({ "writer": "anthropic", "key": PLANTED, "base_url": null, "expected": null });
    assert_eq!(
        refusal(&app, "set_api_key", args),
        "secret-tool is not installed; install libsecret-tools."
    );
}

/// A `secret-tool` that never answers, as with a locked keyring waiting on an unlock prompt that has no screen (over SSH,
/// in WSL2), is cut off at its deadline (#510, "API keys"): the server still starts, with no key read, and a save says the
/// keyring did not answer.
#[test]
fn a_keyring_that_never_answers_holds_up_no_start() {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let bin = fake_secret_tool(dir.path());
    // SAFETY: nextest runs each test in a process of its own, and no thread has started yet.
    unsafe { std::env::set_var("PATH", format!("{}:/usr/bin:/bin", bin.display())) };
    std::fs::write(dir.path().join("store/anthropic-api-key"), PLANTED).unwrap();
    std::fs::write(dir.path().join("hang"), "").unwrap();

    let started = std::time::Instant::now();
    let keys =
        studio_server::settings::SecretTool::with_deadline(std::time::Duration::from_millis(300));
    let app = app_with(dir.path(), Arc::new(keys));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "the start waited {:?} on the keyring",
        started.elapsed()
    );
    let start = studio_server::api::call(&app, &NoPage, "writers", Value::Null).unwrap();
    assert_eq!(start["keys"], json!([]), "{start}");

    let args =
        json!({ "writer": "openai", "key": OPENAI_PLANTED, "base_url": null, "expected": null });
    assert_eq!(
        refusal(&app, "set_api_key", args),
        "The keyring did not answer; it may be locked and waiting to be unlocked. Unlock it, then save again."
    );
}
