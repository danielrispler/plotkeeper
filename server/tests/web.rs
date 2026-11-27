//! The page's transport (danielrispler/plotkeeper#325, spec #319 "Security and lifecycle"): the token and its cookie, the Host
//! check, a command's answer against the function behind it, an event reaching a subscriber, and one plotkeeper per data folder.

mod isolated_home;
use std::fmt::Write as _;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
use studio_server::{
    App, Follower,
    lock::{Instance, acquire},
    web::{Web, new_token},
};
use studio_test_support::{git, repo_in};
use studio_understanding::{CardStore, Effects, FakeWriter, RiskNotes};

const LOG: &str = include_str!("../../core/tests/fixtures/two-sessions.jsonl");
const FIRST: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
const TOKEN: &str = "t0k3n";
static ASSETS: &[(&str, &[u8])] = &[("index.html", b"<html>page</html>")];

struct NoEffects;
impl Effects for NoEffects {
    fn list(&self) {}
    fn name(&self, _: &str, _: &str) {}
    fn log(&self, _: String) {}
}

struct Fixture {
    app: Arc<App>,
    web: Web,
    follower: Follower,
    _dir: tempfile::TempDir,
}

fn fixture() -> Fixture {
    fixture_of(|_| LOG.into())
}

/// A fixture over the event log `events` writes, given the fixture's folder.
fn fixture_of(events: impl FnOnce(&std::path::Path) -> String) -> Fixture {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(&log, events(dir.path())).unwrap();
    let cards = Arc::new(Mutex::new(CardStore::open_beside(&log).unwrap()));
    let notes = Arc::new(Mutex::new(RiskNotes::open_beside(&log).unwrap()));
    let app = App::open_with(
        &log,
        cards,
        notes,
        Arc::new(FakeWriter),
        Arc::new(NoEffects),
    )
    .unwrap();
    let app = Arc::new(app);
    let mut follower = Follower::start(&app, &log);
    follower.pass();
    let web = Web::start(app.clone(), ASSETS, TOKEN.into(), 47900).unwrap();
    app.events.set(web.sink());
    Fixture {
        app,
        web,
        follower,
        _dir: dir,
    }
}

struct Reply {
    status: u16,
    head: String,
    body: String,
}

/// One request, read to the end of the connection. `host` is the Host header, `extra` more header lines.
fn request(port: u16, method: &str, path: &str, host: &str, extra: &[&str], body: &str) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut text = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n");
    for line in extra {
        // Writing to a `String` cannot fail.
        write!(text, "{line}\r\n").ok();
    }
    write!(text, "Content-Length: {}\r\n\r\n{body}", body.len()).ok();
    stream.write_all(text.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Reply {
        status,
        head: head.into(),
        body: body.into(),
    }
}

fn cookie() -> String {
    format!("Cookie: plotkeeper={TOKEN}")
}

fn get(f: &Fixture, path: &str, extra: &[&str]) -> Reply {
    let port = f.web.port();
    request(port, "GET", path, &format!("127.0.0.1:{port}"), extra, "")
}

fn post(f: &Fixture, command: &str, body: &str) -> Reply {
    let port = f.web.port();
    let cookie = cookie();
    request(
        port,
        "POST",
        &format!("/api/{command}"),
        &format!("127.0.0.1:{port}"),
        &[&cookie],
        body,
    )
}

#[test]
fn a_request_without_the_cookie_is_refused() {
    let f = fixture();
    assert_eq!(get(&f, "/", &[]).status, 403);
    assert_eq!(post_without_cookie(&f).status, 403);
    assert_eq!(get(&f, "/", &["Cookie: plotkeeper=wrong"]).status, 403);
}

fn post_without_cookie(f: &Fixture) -> Reply {
    let port = f.web.port();
    request(
        port,
        "POST",
        "/api/sessions",
        &format!("127.0.0.1:{port}"),
        &[],
        "{}",
    )
}

#[test]
fn the_token_in_the_url_gives_the_cookie_and_leaves_the_url() {
    let f = fixture();
    assert_eq!(get(&f, "/?token=wrong", &[]).status, 403);
    let reply = get(&f, &format!("/?token={TOKEN}"), &[]);
    assert_eq!(reply.status, 302);
    assert!(
        reply.head.contains(&format!("plotkeeper={TOKEN}")),
        "{}",
        reply.head
    );
    assert!(reply.head.contains("HttpOnly"));
    let location = reply
        .head
        .lines()
        .find(|l| l.to_lowercase().starts_with("location:"))
        .unwrap();
    assert!(!location.contains("token"), "{location}");
    let page = get(&f, "/", &[&cookie()]);
    assert_eq!(page.status, 200);
    assert_eq!(page.body, "<html>page</html>");
}

#[test]
fn a_host_other_than_the_servers_own_is_refused() {
    let f = fixture();
    let port = f.web.port();
    let cookie = cookie();
    for host in [
        "evil.test",
        "127.0.0.1:1",
        "localhost",
        &format!("evil.test:{port}"),
    ] {
        assert_eq!(
            request(port, "GET", "/", host, &[&cookie], "").status,
            403,
            "{host}"
        );
    }
    assert_eq!(
        request(
            port,
            "GET",
            "/",
            &format!("localhost:{port}"),
            &[&cookie],
            ""
        )
        .status,
        200
    );
}

#[test]
fn a_post_from_another_origin_is_refused() {
    let f = fixture();
    let cookie = cookie();
    let port = f.web.port();
    let reply = request(
        port,
        "POST",
        "/api/sessions",
        &format!("127.0.0.1:{port}"),
        &[&cookie, "Origin: http://evil.test"],
        "{}",
    );
    assert_eq!(reply.status, 403);
}

#[test]
fn a_command_answers_as_the_function_behind_it_does() {
    let f = fixture();
    let sessions = post(&f, "sessions", "{}");
    assert_eq!(sessions.status, 200);
    let expected = serde_json::to_value(studio_server::sessions(&f.app)).unwrap();
    assert!(!expected.as_array().unwrap().is_empty());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&sessions.body).unwrap(),
        expected
    );

    // Arguments arrive as the page names them, in camel case.
    let selected = post(&f, "select", &format!(r#"{{"id":"{FIRST}"}}"#));
    assert_eq!(selected.status, 200);
    let expected = studio_server::selected(&f.app);
    assert_eq!(expected["session"]["id"], FIRST);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&selected.body).unwrap(),
        expected
    );
    let summary = post(&f, "agent_summary", r#"{"agentId":"nobody"}"#);
    assert_eq!((summary.status, summary.body.trim()), (200, "null"));

    assert_eq!(post(&f, "no_such_command", "{}").status, 404);
    assert_eq!(post(&f, "terminal_send", "{}").status, 404);
}

#[test]
fn no_workbench_command_is_served() {
    let f = fixture();
    // The terminals, the project registry and the settings page left the page (#326): each answers as an unknown command. The
    // saved project list's `projects` (#381) is another command, which reads the list and adds nothing.
    for command in [
        "select_terminal",
        "closed",
        "split_shown",
        "sounds",
        "clearing",
        "write_setting",
        "terminal_send",
        "branches",
        "skills",
        "usage",
    ] {
        assert_eq!(post(&f, command, "{}").status, 404, "{command}");
    }
}

#[test]
fn a_failed_command_answers_500_with_its_reason() {
    let f = fixture();
    // A folder that is no repository: the command fails reading it and writes nothing, in the real home or elsewhere.
    let project = f._dir.path().join("none");
    let body = serde_json::json!({"project": project, "commit": "abc"}).to_string();
    let reply = post(&f, "map_at", &body);
    assert_eq!(reply.status, 500);
    assert!(
        serde_json::from_str::<String>(&reply.body).is_ok(),
        "{}",
        reply.body
    );
}

#[test]
fn an_event_reaches_a_subscriber_and_the_page_reports_its_focus() {
    let f = fixture();
    let port = f.web.port();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{}\r\n\r\n",
        cookie()
    )
    .unwrap();
    let mut lines = BufReader::new(stream).lines();
    let status = lines.next().unwrap().unwrap();
    assert!(status.contains("200"), "{status}");
    // The headers end at the blank line; the stream is open once the first comment arrives.
    loop {
        if lines.next().unwrap().unwrap().starts_with(':') {
            break;
        }
    }
    assert!(!f.app.events.focused());
    assert_eq!(post(&f, "page_focus", r#"{"focused":true}"#).status, 200);
    assert!(f.app.events.focused());
    f.app.events.emit("ping", serde_json::json!({"n": 1}));
    let mut seen = vec![];
    for line in lines.by_ref().take(6) {
        let line = line.unwrap();
        seen.push(line.clone());
        if line.starts_with("data:") {
            break;
        }
    }
    assert!(seen.contains(&"event: ping".to_string()), "{seen:?}");
    assert!(seen.contains(&r#"data: {"n":1}"#.to_string()), "{seen:?}");
}

#[test]
fn a_taken_port_gives_the_next_free_one() {
    let f = fixture();
    let second = Web::start(f.app.clone(), ASSETS, new_token(), f.web.port()).unwrap();
    assert!(second.port() > f.web.port());
    assert_eq!(new_token().len(), 64);
    assert_ne!(new_token(), new_token());
}

#[test]
fn a_second_plotkeeper_for_the_same_folder_finds_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let Instance::First(held) = acquire(dir.path()).unwrap() else {
        panic!("the first is first")
    };
    held.publish(47911, "abc").unwrap();
    let Instance::Second(running) = acquire(dir.path()).unwrap() else {
        panic!("the second is second")
    };
    assert_eq!((running.port, running.token.as_str()), (47911, "abc"));
    drop(held);
    assert!(
        matches!(acquire(dir.path()).unwrap(), Instance::First(_)),
        "a stopped one frees the folder"
    );
}

#[test]
fn another_command_answers_while_a_subagents_log_is_read() {
    // A session with a subagent, whose log is beside the session's transcript in the fixture's folder.
    let (session, agent) = ("s1", "a7e2c4f6b8d0a1c3e");
    let f = fixture_of(|dir| {
        let transcript = dir.join("s1.jsonl");
        let logs = dir.join(session).join("subagents");
        std::fs::create_dir_all(&logs).unwrap();
        let line = serde_json::json!({
            "type": "user", "agentId": agent, "sessionId": session, "isSidechain": true,
            "message": {"role": "user", "content": "Find the probe."},
        });
        std::fs::write(
            logs.join(format!("agent-{agent}.jsonl")),
            format!("{line}\n"),
        )
        .unwrap();
        let own = serde_json::json!({
            "session_id": session, "hook_event_name": "UserPromptSubmit", "cwd": "/w", "prompt": "Go.",
            "transcript_path": transcript, "studio_received_ms": 1793261000100u64,
        });
        let step = serde_json::json!({
            "session_id": session, "hook_event_name": "PreToolUse", "cwd": "/w", "agent_id": agent,
            "tool_name": "Bash", "tool_use_id": "t0", "tool_input": {"command": "ls"},
            "transcript_path": transcript, "studio_received_ms": 1793261000200u64,
        });
        format!("{own}\n{step}\n")
    });

    // The read waits, timing-free, until the other command has answered: were the studio's lock held over the read,
    // that command would never answer.
    let (reading, started) = mpsc::channel();
    let (answered, released) = mpsc::channel::<()>();
    let app = f.app.clone();
    let reader = std::thread::spawn(move || {
        studio_server::subagent_log_through(&app, session.into(), agent.into(), 0, |file| {
            reading.send(()).unwrap();
            // A closed channel or a timeout both mean carry on.
            released.recv().ok();
            file.read()
        })
    });
    started.recv().unwrap();
    let sessions = post(&f, "sessions", "{}");
    assert_eq!(sessions.status, 200);
    assert!(sessions.body.contains(session), "{}", sessions.body);
    answered.send(()).unwrap();

    let log = reader.join().unwrap().expect("a log");
    assert_eq!(
        log.entries,
        [studio_core::LogEntry::Prompt {
            text: "Find the probe.".into()
        }]
    );
}

/// Opens the event stream and reads up to its first comment.
fn open_stream(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "GET /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{}\r\n\r\n",
        cookie()
    )
    .unwrap();
    let mut lines = BufReader::new(stream.try_clone().unwrap()).lines();
    while !lines.next().unwrap().unwrap().starts_with(':') {}
    stream
}

#[test]
fn the_focus_a_closed_tab_reported_is_not_kept_while_another_tab_listens() {
    let f = fixture();
    let port = f.web.port();
    let (closing, _other) = (open_stream(port), open_stream(port));
    assert_eq!(post(&f, "page_focus", r#"{"focused":true}"#).status, 200);
    assert!(f.app.events.focused());

    drop(closing);
    // The server finds the closed tab when it writes to it.
    let mut waited = 0;
    while f.app.events.focused() && waited < 100 {
        f.app.events.emit("ping", serde_json::json!({}));
        std::thread::sleep(Duration::from_millis(50));
        waited += 1;
    }

    assert!(!f.app.events.focused());
}

#[test]
fn the_token_in_the_url_leaves_it_even_when_the_cookie_is_already_set() {
    let f = fixture();
    let reply = get(&f, &format!("/?token={TOKEN}"), &[&cookie()]);
    assert_eq!(reply.status, 302);
    let location = reply
        .head
        .lines()
        .find(|l| l.to_lowercase().starts_with("location:"))
        .unwrap();
    assert!(!location.contains("token"), "{location}");
}

#[test]
fn the_tests_run_in_an_empty_home_not_daniels() {
    let _f = fixture();
    let home = std::env::home_dir().unwrap();
    assert!(
        std::fs::read_dir(&home).unwrap().next().is_none(),
        "{home:?}"
    );
    assert!(!studio_hook::data_dir(&home).exists(), "{home:?}");
}

#[test]
fn the_page_is_served_with_a_policy_that_lets_no_frame_load_a_url() {
    // A micro-world's frame is a `srcdoc` page, which the policy allows; a frame that navigates itself to a network URL,
    // carrying what it read in the address, is refused (checked by hand in headless Chrome, 2026-11-24).
    let f = fixture();
    let cookie = cookie();
    let page = get(&f, "/", &[&cookie]);
    assert_eq!(page.status, 200);
    let policy = page
        .head
        .lines()
        .find_map(|line| line.strip_prefix("Content-Security-Policy: "))
        .unwrap_or_else(|| panic!("no policy: {}", page.head));
    assert_eq!(policy, "frame-src 'none'");
}

/// The events the stream carries up to a `ping` the test sends after what it checks, as `(name, data)`.
fn events_to_ping(f: &Fixture, stream: TcpStream) -> Vec<(String, serde_json::Value)> {
    f.app.events.emit("ping", serde_json::json!(null));
    let mut events = vec![];
    let mut name = String::new();
    for line in BufReader::new(stream).lines() {
        let line = line.unwrap();
        if let Some(event) = line.strip_prefix("event: ") {
            name = event.to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            if name == "ping" {
                break;
            }
            events.push((name.clone(), serde_json::from_str(data).unwrap()));
        }
    }
    events
}

#[test]
fn a_hand_edit_of_settings_json_reaches_the_page_and_its_next_save_succeeds() {
    let mut f = fixture();
    let stream = open_stream(f.web.port());
    let [settings, _] = studio_server::settings::paths().unwrap();
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    let by_hand = "{\n  // mine\n  \"explanations\": false\n}\n";
    std::fs::write(&settings, by_hand).unwrap();

    f.follower.pass();

    let sent: Vec<_> = events_to_ping(&f, stream)
        .into_iter()
        .filter(|(name, _)| name == "settings-files")
        .map(|(_, data)| data)
        .collect();
    assert_eq!(
        sent,
        [serde_json::json!({ "settings": by_hand, "keymap": null })]
    );
    // The page saves with the text the event gave it.
    let body = serde_json::json!({ "expected": sent[0]["settings"], "on": true }).to_string();
    let saved = post(&f, "set_explanations", &body);
    assert_eq!(saved.status, 200, "{}", saved.body);
    assert_eq!(
        std::fs::read_to_string(&settings).unwrap(),
        "{\n  // mine\n  \"explanations\": true\n}\n"
    );
}

#[test]
fn only_an_error_of_the_servers_own_reaches_the_page_as_server_error() {
    let f = fixture();
    // Another test of this binary may have set it already.
    log::set_logger(&studio_server::LOGGER).ok();
    log::set_max_level(log::LevelFilter::Trace);
    studio_server::LOGGER.send_errors_to(f.web.sink());
    let stream = open_stream(f.web.port());

    log::error!("save the cards: disk full");
    log::warn!("prune the event log: busy");
    log::error!(target: "ureq::unit", "x-api-key: secret");
    // The page shows its own errors itself, so they are not sent back to it.
    let own = r#"{"level":"error","line":"delete_session: gone"}"#;
    assert_eq!(post(&f, "log", own).status, 200);

    let sent: Vec<_> = events_to_ping(&f, stream)
        .into_iter()
        .filter(|(name, _)| name == "server-error")
        .map(|(_, data)| data)
        .collect();
    assert_eq!(sent, [serde_json::json!("save the cards: disk full")]);
}

/// Start-up registration (spec #753, "Projects"): the folder plotkeeper starts in adds its git root's project, in process for a first
/// start and through the API for a second. The folders are under `/var/tmp`: the system's temporary folders are never saved, and a
/// folder under the build's own (`CARGO_TARGET_TMPDIR`) is, in a task's worktree, that worktree's main checkout's.
#[test]
fn a_start_folder_adds_its_repositorys_project() {
    let base = tempfile::tempdir_in("/var/tmp").unwrap();
    let base_path = base.path().canonicalize().unwrap();
    let main = repo_in(&base_path, "main").display().to_string();
    git(
        std::path::Path::new(&main),
        &[
            "worktree",
            "add",
            "-q",
            ".claude/worktrees/task",
            "-b",
            "task",
        ],
    );
    git(
        std::path::Path::new(&main),
        &["worktree", "add", "-q", "../elsewhere", "-b", "elsewhere"],
    );
    std::fs::create_dir_all(format!("{main}/src/deep")).unwrap();
    let elsewhere = format!("{}/elsewhere", base_path.display());
    let not_git = base_path.join("plain");
    std::fs::create_dir(&not_git).unwrap();
    let rows: [(&str, String, Option<&str>); 5] = [
        ("a git root", main.clone(), Some(&main)),
        (
            "a folder inside it",
            format!("{main}/src/deep"),
            Some(&main),
        ),
        (
            "a task's worktree is its main checkout's",
            format!("{main}/.claude/worktrees/task"),
            Some(&main),
        ),
        (
            "any other worktree is its own",
            elsewhere.clone(),
            Some(&elsewhere),
        ),
        (
            "a folder with no git root adds nothing",
            not_git.display().to_string(),
            None,
        ),
    ];
    let f = fixture();
    let saved = |f: &Fixture| -> Vec<String> {
        let reply = post(f, "projects", "{}");
        let list: Vec<serde_json::Value> = serde_json::from_str(&reply.body).unwrap();
        list.iter()
            .map(|p| p["path"].as_str().unwrap().to_string())
            .collect()
    };
    for (case, folder, expected) in rows {
        let added = Vec::from_iter(expected.map(String::from));
        // In process, as a first start does.
        let data = tempfile::tempdir().unwrap();
        let projects = studio_server::projects::Projects::at(data.path().join("projects.json"));
        assert_eq!(
            projects.register(&folder).unwrap().as_deref(),
            expected,
            "{case}"
        );
        let listed: Vec<String> = projects.list().into_iter().map(|p| p.path).collect();
        assert_eq!(listed, added, "{case}");

        // Through the API, as a second start sends its folder to the running server.
        let body = serde_json::json!({ "folder": folder }).to_string();
        let before = saved(&f);
        let reply = post(&f, "register_folder", &body);
        assert_eq!(reply.status, 200, "{case}: {}", reply.body);
        let reply: Option<String> = serde_json::from_str(&reply.body).unwrap();
        assert_eq!(reply.as_deref(), expected, "{case}");
        let now = saved(&f);
        match expected {
            Some(project) => assert!(now.iter().any(|p| p == project), "{case}: {now:?}"),
            None => assert_eq!(now, before, "{case}"),
        }
    }

    // A removed project comes back when plotkeeper starts in it again.
    let data = tempfile::tempdir().unwrap();
    let projects = studio_server::projects::Projects::at(data.path().join("projects.json"));
    projects.register(&main).unwrap();
    assert_eq!(projects.remove(&main, 1).unwrap(), []);
    assert_eq!(
        projects.register(&main).unwrap().as_deref(),
        Some(main.as_str())
    );
    assert_eq!(projects.list().len(), 1);

    // Only a caller holding the server's token adds a folder.
    let other = repo_in(&base_path, "other").display().to_string();
    let body = serde_json::json!({ "folder": other }).to_string();
    let port = f.web.port();
    let refused = request(
        port,
        "POST",
        "/api/register_folder",
        &format!("127.0.0.1:{port}"),
        &["Cookie: plotkeeper=wrong"],
        &body,
    );
    assert_eq!(refused.status, 403);
    assert!(!saved(&f).contains(&other));
    assert_eq!(post(&f, "register_folder", &body).status, 200);
    assert!(saved(&f).contains(&other));
}
