//! The page's transport (danielrispler/plotkeeper#325, spec #319 "Security and lifecycle"): the token and its cookie, the Host
//! check, a command's answer against the function behind it, an event reaching a subscriber, and one plotkeeper per data folder.

mod isolated_home;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::Duration,
};
use studio_server::{
    App, Follower,
    lock::{Instance, acquire},
    web::{Web, new_token},
};
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
    _dir: tempfile::TempDir,
}

fn fixture() -> Fixture {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(&log, LOG).unwrap();
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
    Follower::start(&app, &log).pass();
    let web = Web::start(app.clone(), ASSETS, TOKEN.into(), 47900).unwrap();
    app.events.set(web.sink());
    Fixture {
        app,
        web,
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
        text += &format!("{line}\r\n");
    }
    text += &format!("Content-Length: {}\r\n\r\n{body}", body.len());
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
    assert!(
        !home.join("Library/Application Support/plotkeeper").exists(),
        "{home:?}"
    );
}
