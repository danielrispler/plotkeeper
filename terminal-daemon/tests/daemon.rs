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

// #555, row 7: only Daniel's account can reach the socket.
#[test]
fn the_daemons_folder_is_private_and_its_socket_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let d = Daemon::start("perms");
    let mode = |p: PathBuf| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(d.dir.clone()), 0o700);
    assert_eq!(mode(d.socket()), 0o600);
}

// #555, row 1: Claude Code and Antigravity report their status only to a terminal that answers the
// OSC 7501 query. The program reads the reply raw and prints it with ESC as `E` and BEL as `B`.
#[test]
fn an_osc_7501_query_gets_the_reply_with_its_own_terminator() {
    let d = Daemon::start("query");
    let mut c = d.client();
    let reply = "E]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";
    for (id, query, end) in [("st", r"\033\\", r"E\"), ("bel", r"\007", "B")] {
        let n = reply.len() + end.len();
        let script = format!(
            "stty raw -echo; printf '\\033]7501;?{query}'; head -c {n} | tr '\\033\\007' EB; echo; sleep 1000"
        );
        spawn(&mut c, id, &["sh", "-c", &script]);
        c.send(&json!({ "op": "attach", "id": id })).unwrap();
        output_until(&mut c, id, &format!("{reply}{end}"));
    }
}

// #555: every program's OSC 7501 reports set its terminal's root status in `list`. `turn` goes up each
// time the root leaves `working`, as herdr-core counts it. Each step waits for Enter, then prints
// its reports and a marker.
#[test]
fn osc_7501_reports_give_list_the_status_and_count_turns() {
    let d = Daemon::start("status");
    let mut c = d.client();
    let long = "A".repeat(6000);
    let steps = [
        // A report split across two reads, with a key the spec may add later.
        r"printf '\033]7501;state=work'; sleep 0.3; printf 'ing:app=claude-code:future=x\033\\'"
            .to_string(),
        // Over the 4096-byte limit: dropped whole.
        format!(r"printf '\033]7501;state=done:msg={long}\033\\'"),
        // The same state again: `status_ms` stays.
        r"printf '\033]7501;state=working:app=claude-code:progress=50\033\\'".into(),
        // A child record: ignored.
        r"printf '\033]7501;state=done:id=a/b\007'".into(),
        r"printf '\033]7501;state=done:app=claude-code\033\\'".into(),
        r"printf '\033]7501;state=blocked:kind=permission:app=claude-code\007'".into(),
        r"printf '\033]7501;state=working\007'".into(),
        r"printf '\033]7501;state=clear\007'".into(),
        r"printf '\033]7501;state=working\007'".into(),
        "exit 0".into(),
    ];
    let script: String = steps
        .iter()
        .enumerate()
        .map(|(n, s)| format!("read x; {s}; echo step-{n}-done\n"))
        .collect();
    spawn(&mut c, "t1", &["sh", "-c", &script]);
    c.send(&json!({ "op": "attach", "id": "t1" })).unwrap();
    let fields = |t: &Value| {
        let f = |k: &str| t[k].as_str().unwrap_or("-").to_string();
        format!(
            "{} {} {} turn {}",
            f("state"),
            f("kind"),
            f("app"),
            t["turn"]
        )
    };
    let never = term(&list(&mut c), "t1").clone();
    assert_eq!(fields(&never), "- - - turn 0");
    assert!(never["status_ms"].is_null());
    let mut step = |n: usize| {
        let enter = studio_terminal_daemon::encode(b"\n");
        c.send(&json!({ "op": "input", "id": "t1", "data": enter }))
            .unwrap();
        output_until(&mut c, "t1", &format!("step-{n}-done"));
        term(&list(&mut c), "t1").clone()
    };
    let working = step(0);
    assert_eq!(fields(&working), "working - claude-code turn 0");
    assert!(working["status_ms"].as_u64().unwrap() > 0);
    assert_eq!(fields(&step(1)), "working - claude-code turn 0");
    let again = step(2);
    assert_eq!(again["status_ms"], working["status_ms"]);
    assert_eq!(fields(&step(3)), "working - claude-code turn 0");
    assert_eq!(fields(&step(4)), "done - claude-code turn 1");
    // Not from `working`, so no new turn.
    assert_eq!(fields(&step(5)), "blocked permission claude-code turn 1");
    assert_eq!(fields(&step(6)), "working - - turn 1");
    assert_eq!(fields(&step(7)), "- - - turn 2");
    assert_eq!(fields(&step(8)), "working - - turn 2");
    c.send(&json!({ "op": "input", "id": "t1", "data": studio_terminal_daemon::encode(b"\n") }))
        .unwrap();
    wait_for("the exit", || {
        fields(term(&list(&mut c), "t1")) == "- - - turn 3"
    });
}

// #555: a crashed app must not leave Remote Control's phone pushes off, so the daemon deletes the
// presence file when the app's connection drops, but not when a CLI attach's does.
#[test]
fn the_presence_file_goes_when_the_app_disconnects_not_a_cli_attach() {
    let d = Daemon::start("presence");
    let present = studio_terminal_daemon::presence(&d.socket());
    assert_eq!(present, d.dir.join("present"));
    std::fs::write(&present, "").unwrap();

    let mut cli = d.client();
    list(&mut cli);
    drop(cli);
    let mut app = d.client();
    app.send(&json!({ "op": "app" })).unwrap();
    list(&mut app);
    // Time for the daemon to see the CLI's drop.
    std::thread::sleep(Duration::from_millis(300));
    assert!(present.exists(), "a CLI attach's disconnect deleted it");

    drop(app);
    wait_for("the presence file to go", || !present.exists());
}

// #555: the daemon drops an app that falls behind while its window may still be focused, so that drop
// leaves the presence file; the app reconnects and marks itself again.
#[test]
fn an_app_dropped_for_lag_keeps_the_presence_file() {
    let d = Daemon::start("presence-lag");
    let present = studio_terminal_daemon::presence(&d.socket());
    std::fs::write(&present, "").unwrap();
    let mut app = d.client();
    app.send(&json!({ "op": "app" })).unwrap();
    spawn(&mut app, "flood", &["yes"]);
    app.send(&json!({ "op": "attach", "id": "flood" })).unwrap();

    wait_for("the lagging app to be dropped", || {
        app.send(&json!({ "op": "list" })).is_err()
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(present.exists(), "a lag drop deleted the presence file");
}
