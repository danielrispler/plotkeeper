// Seam 3 (#461): the daemon binary against real PTYs, driven over its socket.
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Child, Command},
    time::{Duration, Instant},
};
use studio_terminal_daemon::Client;

struct Daemon {
    child: Child,
    dir: PathBuf,
}

impl Daemon {
    // A daemon started the way a studio launched from a Claude Code session would start it: with that session's
    // variables and a terminal's identity in its environment.
    fn start(name: &str) -> Daemon {
        let dir = std::env::temp_dir().join(format!("std-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_studio-terminal-daemon"))
            .args(["serve", "--socket"])
            .arg(dir.join("d.sock"))
            .env("HOME", &dir)
            .env("SHELL", "/bin/sh")
            .env("CLAUDECODE", "1")
            .env("CLAUDE_CODE_SESSION_ID", "leaked-session")
            .env("TERM_PROGRAM", "ghostty")
            .env("GHOSTTY_RESOURCES_DIR", "/leak")
            .env("TMUX", "/leak")
            .spawn()
            .unwrap();
        let d = Daemon { child, dir };
        let deadline = Instant::now() + Duration::from_secs(5);
        // The socket file appears at `bind`, before `listen`, so wait for a connection, not the file.
        while std::os::unix::net::UnixStream::connect(d.socket()).is_err() {
            assert!(
                Instant::now() < deadline,
                "the daemon did not open its socket"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        d
    }

    fn socket(&self) -> PathBuf {
        self.dir.join("d.sock")
    }

    fn client(&self) -> Client {
        Client::connect(&self.socket(), 1).unwrap()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn spawn(c: &mut Client, id: &str, argv: &[&str]) {
    c.send(&json!({ "op": "spawn", "id": id, "argv": argv, "cwd": "/", "cols": 80, "rows": 24 }))
        .unwrap();
    let ev = c.recv_until(|v| v["id"] == id).unwrap();
    assert_eq!(ev["ev"], "spawned", "{ev}");
}

// Reads events until the terminal's output, decoded, contains `needle`; returns everything seen.
fn output_until(c: &mut Client, id: &str, needle: &str) -> String {
    let mut seen = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !String::from_utf8_lossy(&seen).contains(needle) {
        assert!(
            Instant::now() < deadline,
            "no {needle:?} in {:?}",
            String::from_utf8_lossy(&seen)
        );
        let ev = c.recv().unwrap();
        if ev["id"] == id && (ev["ev"] == "output" || ev["ev"] == "replay") {
            seen.extend(studio_terminal_daemon::decode(&ev["data"]));
        }
    }
    String::from_utf8_lossy(&seen).into_owned()
}

fn list(c: &mut Client) -> Value {
    c.send(&json!({ "op": "list" })).unwrap();
    c.recv_until(|v| v["ev"] == "list").unwrap()
}

fn term<'a>(list: &'a Value, id: &str) -> &'a Value {
    list["terms"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == id)
        .unwrap()
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_second_client_attaches_with_replay_and_types_into_the_terminal() {
    let d = Daemon::start("attach");
    let mut first = d.client();
    spawn(&mut first, "t1", &["cat"]);
    first.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    first.send(&json!({ "op": "input", "id": "t1", "data": studio_terminal_daemon::encode(b"before\n") })).unwrap();
    output_until(&mut first, "t1", "before\r\nbefore");
    drop(first);

    let mut second = d.client();
    second.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    let replay = second.recv_until(|v| v["ev"] == "replay").unwrap();
    assert_eq!(
        String::from_utf8_lossy(&studio_terminal_daemon::decode(&replay["data"])),
        "before\r\nbefore\r\n"
    );
    second.send(&json!({ "op": "input", "id": "t1", "data": studio_terminal_daemon::encode(b"after\n") })).unwrap();
    output_until(&mut second, "t1", "after\r\nafter");
}

// The terminal app running `attach` is itself a daemon terminal here, so the test drives it through a real PTY.
#[test]
fn attach_connects_another_terminal_and_detaches_with_ctrl_bracket() {
    let d = Daemon::start("cli");
    let mut c = d.client();
    spawn(&mut c, "agent", &["cat"]);
    c.send(&json!({ "op": "input", "id": "agent", "data": studio_terminal_daemon::encode(b"earlier\n") })).unwrap();
    let (bin, socket) = (env!("CARGO_BIN_EXE_studio-terminal-daemon"), d.socket());
    spawn(
        &mut c,
        "zed",
        &[bin, "attach", "agent", "--socket", socket.to_str().unwrap()],
    );
    c.send(&json!({ "op": "attach", "id": "zed" })).unwrap();
    output_until(&mut c, "zed", "earlier\r\nearlier");

    c.send(
        &json!({ "op": "input", "id": "zed", "data": studio_terminal_daemon::encode(b"typed\r") }),
    )
    .unwrap();
    output_until(&mut c, "zed", "typed\r\ntyped");
    c.send(&json!({ "op": "input", "id": "zed", "data": studio_terminal_daemon::encode(b"\x1d") }))
        .unwrap();
    output_until(&mut c, "zed", "[detached]");
    wait_for("attach to end", || {
        term(&list(&mut c), "zed")["exited"] == "code 0"
    });
    assert!(term(&list(&mut c), "agent")["exited"].is_null());
}

#[test]
fn resize_reaches_the_program_and_list_shows_size_program_and_exit_code() {
    let d = Daemon::start("resize");
    let mut c = d.client();
    spawn(
        &mut c,
        "t1",
        &["sh", "-c", "read x; stty size; exec sleep 1000"],
    );
    c.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    c.send(&json!({ "op": "resize", "id": "t1", "cols": 101, "rows": 33 }))
        .unwrap();
    c.send(&json!({ "op": "input", "id": "t1", "data": studio_terminal_daemon::encode(b"go\n") }))
        .unwrap();
    output_until(&mut c, "t1", "33 101");
    wait_for("sleep in the foreground", || {
        term(&list(&mut c), "t1")["foreground"] == "sleep"
    });
    let t = term(&list(&mut c), "t1").clone();
    assert_eq!(
        (t["cols"].as_u64(), t["rows"].as_u64(), t["cwd"].as_str()),
        (Some(101), Some(33), Some("/"))
    );
    assert!(t["exited"].is_null());
    assert!(t["rss_kb"].as_u64().unwrap() > 0, "{t}");

    spawn(&mut c, "t2", &["sh", "-c", "exit 7"]);
    wait_for("the exit code", || {
        term(&list(&mut c), "t2")["exited"] == "code 7"
    });
}

#[test]
fn killing_a_terminal_leaves_no_process_of_its_group() {
    let d = Daemon::start("kill");
    let mut c = d.client();
    // A child in its own process group, as job control makes it, and one that ignores the hangup.
    spawn(
        &mut c,
        "t1",
        &[
            "sh",
            "-c",
            "set -m; sleep 1001 & trap '' HUP; sleep 1002 & echo started; wait",
        ],
    );
    c.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    output_until(&mut c, "t1", "started");
    // `echo` can run before a background `sleep` shows in `ps` under load, so wait for all three.
    wait_for("the shell and both sleeps", || {
        term(&list(&mut c), "t1")["processes"]
            .as_array()
            .unwrap()
            .len()
            >= 3
    });

    c.send(&json!({ "op": "kill", "id": "t1" })).unwrap();
    wait_for("every process of the terminal to end", || {
        term(&list(&mut c), "t1")["processes"]
            .as_array()
            .unwrap()
            .is_empty()
    });
    // The killed shell leaves `ps` before the daemon's waiter thread reaps it and records the exit.
    wait_for("the exit", || {
        !term(&list(&mut c), "t1")["exited"].is_null()
    });
    c.send(&json!({ "op": "forget", "id": "t1" })).unwrap();
    wait_for("forget", || {
        list(&mut c)["terms"].as_array().unwrap().is_empty()
    });
}

#[test]
fn processes_left_behind_after_the_shell_exits_are_listed() {
    let d = Daemon::start("leftover");
    let mut c = d.client();
    spawn(
        &mut c,
        "t1",
        &["sh", "-c", "trap '' HUP; sleep 1003 & exit 0"],
    );
    wait_for("the exit", || {
        term(&list(&mut c), "t1")["exited"] == "code 0"
    });
    let t = term(&list(&mut c), "t1").clone();
    assert_eq!(t["processes"][0]["command"], "sleep", "{t}");
    c.send(&json!({ "op": "kill", "id": "t1" })).unwrap();
    wait_for("the leftover to end", || {
        term(&list(&mut c), "t1")["processes"]
            .as_array()
            .unwrap()
            .is_empty()
    });
}

#[test]
fn an_app_whose_minimum_protocol_is_older_works_with_a_newer_daemon() {
    let d = Daemon::start("protocol");
    let mut old_app = Client::connect(&d.socket(), 0).unwrap();
    assert!(old_app.protocol >= 1);
    // Requests and fields a daemon added later are ignored, and the connection keeps serving.
    old_app
        .send(&json!({ "op": "a-request-from-the-future", "id": "t1" }))
        .unwrap();
    old_app
        .send(&json!({ "op": "list", "a_new_field": true }))
        .unwrap();
    assert_eq!(
        old_app.recv_until(|v| v["ev"] == "list").unwrap()["terms"],
        json!([])
    );
    spawn(&mut old_app, "t1", &["true"]);

    let too_new = Client::connect(&d.socket(), studio_terminal_daemon::PROTOCOL + 1);
    assert!(too_new.is_err());
}

#[test]
fn only_one_daemon_serves_a_socket() {
    let d = Daemon::start("single");
    let second = Command::new(env!("CARGO_BIN_EXE_studio-terminal-daemon"))
        .args(["serve", "--socket"])
        .arg(d.socket())
        .output()
        .unwrap();
    assert!(!second.status.success());
    d.client();
}

#[test]
fn a_terminal_starts_without_claude_or_terminal_identity_variables() {
    let d = Daemon::start("env");
    let mut c = d.client();
    spawn(
        &mut c,
        "t1",
        &["sh", "-c", "env; echo END-OF-ENV; sleep 1000"],
    );
    c.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    let env = output_until(&mut c, "t1", "END-OF-ENV");
    for leaked in [
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
        "TERM_PROGRAM",
        "GHOSTTY_RESOURCES_DIR",
        "TMUX=",
    ] {
        assert!(!env.contains(leaked), "{leaked} leaked into:\n{env}");
    }
    assert!(env.contains("STUDIO_TERMINAL_ID=t1"), "{env}");
    assert!(env.contains("TERM=xterm-256color"), "{env}");
}

#[test]
fn scrollback_is_cut_at_a_fixed_size() {
    let d = Daemon::start("scrollback");
    let mut c = d.client();
    spawn(
        &mut c,
        "t1",
        &[
            "sh",
            "-c",
            "head -c 6000000 /dev/zero | tr '\\0' x; sleep 1000",
        ],
    );
    wait_for("the full scrollback", || {
        term(&list(&mut c), "t1")["scrollback_bytes"] == studio_terminal_daemon::daemon::SCROLLBACK
    });
}

// #470: the daemon keeps a record of the terminals it runs, which outlives it, so the app can revive
// them after a crash, a reboot or `stop`.
fn recorded(d: &Daemon) -> Vec<String> {
    let record = studio_terminal_daemon::record(&d.socket());
    let terms: Value = std::fs::read_to_string(record)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(json!([]));
    let line = |t: &Value| format!("{} {} {}", t["id"], t["argv"], t["cwd"]);
    terms.as_array().unwrap().iter().map(line).collect()
}

#[test]
fn stopping_the_daemon_ends_its_terminals_and_leaves_the_record_of_the_running_ones() {
    let d = Daemon::start("stop");
    let mut c = d.client();
    spawn(&mut c, "t1", &["cat"]);
    spawn(&mut c, "t2", &["sh", "-c", "exit 0"]);
    wait_for("t2's exit", || {
        !term(&list(&mut c), "t2")["exited"].is_null()
    });
    spawn(&mut c, "t3", &["cat"]);
    c.send(&json!({ "op": "forget", "id": "t3" })).unwrap();
    let running = [r#""t1" ["cat"] "/""#];
    wait_for("the record", || recorded(&d) == running);
    let pid = term(&list(&mut c), "t1")["pid"].as_i64().unwrap() as i32;

    studio_terminal_daemon::stop(&d.socket()).unwrap();
    assert!(Client::connect(&d.socket(), 0).is_err());
    wait_for("t1's process to end", || unsafe { libc::kill(pid, 0) } != 0);
    assert_eq!(recorded(&d), running);
}

// #470: a shell comes back in the folder it moved to, not the one it started in.
#[test]
fn list_and_the_record_give_the_folder_a_shell_moved_to() {
    let d = Daemon::start("cwd");
    let mut c = d.client();
    spawn(
        &mut c,
        "t1",
        &["sh", "-c", "cd /usr/bin && echo moved && sleep 1000"],
    );
    c.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    output_until(&mut c, "t1", "moved");
    assert_eq!(term(&list(&mut c), "t1")["cwd"], "/usr/bin");
    assert_eq!(
        recorded(&d)[0],
        r#""t1" ["sh","-c","cd /usr/bin && echo moved && sleep 1000"] "/usr/bin""#
    );
}

// A client that stops reading must not stall the daemon: it is dropped once too far behind, and every
// other terminal keeps flowing.
#[test]
fn a_client_that_stops_reading_leaves_the_other_terminals_flowing() {
    let d = Daemon::start("stall");
    let mut stalled = d.client();
    spawn(&mut stalled, "flood", &["yes"]);
    stalled
        .send(&json!({ "op": "attach", "id": "flood" }))
        .unwrap();
    std::thread::sleep(Duration::from_millis(500));

    let socket = d.socket();
    let (done, finished) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut other = Client::connect(&socket, 1).unwrap();
        spawn(&mut other, "t2", &["cat"]);
        other.send(&json!({ "op": "attach", "id": "t2" })).unwrap();
        let data = studio_terminal_daemon::encode(b"still\n");
        other
            .send(&json!({ "op": "input", "id": "t2", "data": data }))
            .unwrap();
        output_until(&mut other, "t2", "still\r\nstill");
        let _ = done.send(());
    });
    finished
        .recv_timeout(Duration::from_secs(10))
        .expect("the other terminal stalled behind a client that stopped reading");
    // The stalled client is dropped once its queue fills, and the daemon shuts its socket, so a write fails. Reading
    // first would let it catch up, so it would never be dropped and the read would follow `yes` forever.
    wait_for("the stalled client to be dropped", || {
        stalled.send(&json!({ "op": "list" })).is_err()
    });
}
