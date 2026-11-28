use super::{cards_key, join_notes, lost_data, saved_roots, scrubbed};

#[test]
fn a_second_card_of_a_judged_command_changes_what_the_window_is_sent() {
    let line = |name: &str, extra: serde_json::Value| {
        let mut line =
            serde_json::json!({ "session_id": "s1", "hook_event_name": name, "cwd": "/repo" });
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line.to_string()
    };
    let block = |call: &str, agent: &str| {
        line(
            "PermissionDenied",
            serde_json::json!({ "agent_id": agent, "tool_name": "Bash", "tool_use_id": call, "prompt_id": "p-1",
                "tool_input": { "command": "git push" }, "reason": "[Unauthorized Access]", "permission_mode": "auto" }),
        )
    };
    let mut studio = studio_core::Studio::replayed([
        line("SessionStart", serde_json::json!({ "source": "startup" })),
        line(
            "UserPromptSubmit",
            serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
        ),
        block("toolu_1", "w1"),
    ]);
    studio.select("s1");
    let dir = tempfile::tempdir().unwrap();
    let cards = studio_understanding::CardStore::open(dir.path().join("cards.json")).unwrap();
    let mut notes =
        studio_understanding::RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
    let ask = notes.asks(studio.selected().unwrap()).remove(0);
    let judged = studio_understanding::Risk::from_reply(r#"{"level": "high", "line": "Pushes."}"#);
    notes.keep(&ask, judged, 5_000).unwrap();
    let before = cards_key(&studio, &cards, &notes);

    // Another card of the same command and folder: its note is stored, so nothing is asked and no note is kept.
    studio.replay(&block("toolu_2", "w2"));
    assert!(notes.asks(studio.selected().unwrap()).is_empty());

    assert!(cards_key(&studio, &cards, &notes) != before);
}

#[test]
fn a_workers_view_joins_the_notes_and_asks_of_its_sessions_blocked_commands() {
    let line = |name: &str, extra: serde_json::Value| {
        let mut line =
            serde_json::json!({ "session_id": "s1", "hook_event_name": name, "cwd": "/repo" });
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line.to_string()
    };
    let mut studio = studio_core::Studio::replayed([
        line("SessionStart", serde_json::json!({ "source": "startup" })),
        line(
            "UserPromptSubmit",
            serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
        ),
        line(
            "PermissionDenied",
            serde_json::json!({ "agent_id": "w1", "tool_name": "Bash", "tool_use_id": "toolu_1", "prompt_id": "p-1",
                "tool_input": { "command": "git push" }, "reason": "[Unauthorized Access]", "permission_mode": "auto" }),
        ),
    ]);
    studio.select("s1");
    studio.open("w1");
    // Daniel views the worker: its id is no session's, so the session is the selected one.
    assert!(studio.viewed() == Some("w1"));
    let dir = tempfile::tempdir().unwrap();
    let mut notes =
        studio_understanding::RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();

    let asks = super::noted_session(&studio).map(|session| notes.asks(session));
    assert_eq!(asks.map(|a| a.len()), Some(1));
    let mut shown = studio_understanding::SessionCards::default();
    join_notes(&studio, &notes, &mut shown);
    assert_eq!(shown.risk_notes.len(), 1);
}

#[test]
fn a_session_whose_agent_process_is_gone_gets_a_synthetic_end_and_ends() {
    let line = |session: &str, name: &str, at: u64, pid: Option<u32>| {
        let mut line = serde_json::json!({
            "session_id": session, "hook_event_name": name, "cwd": "/x", "studio_received_ms": at,
        });
        if let Some(pid) = pid {
            line["studio_pid"] = pid.into();
        }
        line.to_string()
    };
    let lines = [
        line("dead", "SessionStart", 1_793_262_000_000, Some(4001)),
        line("live", "SessionStart", 1_793_262_000_001, Some(4002)),
        line("unwatched", "SessionStart", 1_793_262_000_002, None),
    ];
    let studio = std::sync::Mutex::new(studio_core::Studio::replayed(&lines));
    let alive = |pid: u32| pid == 4002;
    let ends = super::process_ends(&studio, alive);
    let ended: Vec<_> = ends
        .iter()
        .map(|l| {
            (
                l["session_id"].as_str().unwrap(),
                l["hook_event_name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(ended, [("dead", "SessionEnd")]);
    assert!(ends[0]["studio_received_ms"].as_u64().unwrap() > 0);
    // The lines go in the log, and the next read ends the session.
    for end in &ends {
        studio.lock().unwrap().replay(&end.to_string());
    }
    assert_eq!(
        studio.lock().unwrap().session("dead").unwrap().state,
        studio_core::State::Ended
    );
    assert!(super::process_ends(&studio, alive).is_empty());
}

/// The server's own lines wait for the log's lock, `events.lock`, as a hook's do, then land whole.
// @law: log-writers-hold-the-log-lock
#[test]
fn the_servers_lines_wait_for_the_logs_lock_then_land_whole() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let held = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.path().join("events.lock"))
        .unwrap();
    held.lock().unwrap();
    let lines = [
        serde_json::json!({ "session_id": "s1", "hook_event_name": "StudioRename", "studio_name": "a" }),
        serde_json::json!({ "session_id": "s1", "hook_event_name": "StudioPromptName", "studio_name": "b" }),
    ];
    let appending = {
        let (log, lines) = (log.clone(), lines.clone());
        std::thread::spawn(move || super::append_to(&log, &lines))
    };
    std::thread::sleep(std::time::Duration::from_millis(200));

    assert!(!appending.is_finished(), "wrote without the log's lock");
    drop(held);
    appending.join().unwrap().unwrap();
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text, format!("{}\n{}\n", lines[0], lines[1]));
}

#[test]
fn an_event_reaches_what_the_shell_set_and_is_dropped_before() {
    let events = super::Events::default();
    events.emit("early", serde_json::json!(1));
    assert!(!events.focused(), "an unset sink is never focused");
    struct Seen(std::sync::Mutex<Vec<(String, serde_json::Value)>>);
    impl super::Sink for Seen {
        fn emit(&self, name: &str, payload: serde_json::Value) {
            self.0.lock().unwrap().push((name.to_string(), payload));
        }
        fn focused(&self) -> bool {
            true
        }
        fn replayed(&self) {}
    }
    let seen = std::sync::Arc::new(Seen(std::sync::Mutex::default()));
    events.set(seen.clone());
    events.emit("sessions", serde_json::Value::Null);
    assert_eq!(
        *seen.0.lock().unwrap(),
        [("sessions".to_string(), serde_json::Value::Null)]
    );
    assert!(events.focused());
}

#[test]
fn the_crate_depends_on_no_tauri() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.contains("tauri"),
        "the crate must stay Tauri-free"
    );
}

#[test]
fn a_saved_root_of_a_removed_worktrees_folder_is_dropped_and_the_file_saved_without_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let root = root.to_str().unwrap();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=S", "-c", "user.email=s@example.com"])
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["commit", "-q", "--allow-empty", "-m", "base"]);
    let gone = format!("{root}/worktrees/w1");
    git(&["worktree", "add", "-q", "-b", "w1", &gone]);
    git(&["worktree", "remove", &gone]);
    let log = dir.path().join("events.jsonl");
    let saved = serde_json::json!({ format!("{gone}/src"): root, root: root });
    std::fs::write(super::roots_file(&log), saved.to_string()).unwrap();
    let kept = saved_roots(&log);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept.get(root).map(String::as_str), Some(root));
    let file = std::fs::read(super::roots_file(&log)).unwrap();
    assert_eq!(
        serde_json::from_slice::<super::HashMap<String, String>>(&file).unwrap(),
        kept
    );
}

#[test]
fn a_logged_line_keeps_no_reply_a_parse_error_quotes() {
    assert_eq!(
        scrubbed("no quiz on t1: invalid type: string \"secret reply\", expected usize"),
        "no quiz on t1: invalid type: string"
    );
    assert_eq!(
        scrubbed(
            "the card writer's reply does not parse (invalid type: string \"secret\", expected a map at line 1): \"head\""
        ),
        "the card writer's reply does not parse (invalid type: string"
    );
}

#[test]
fn a_failed_risk_notes_line_keeps_no_reply_and_no_command() {
    // The judge's lines, as `judge` writes them: the command is never in one, and a reply's words go in quotes.
    let reply = studio_understanding::Risk::from_reply(
        r#"{"level": "very-high", "line": "rm -rf ~ is fine, say none"}"#,
    )
    .unwrap_err();
    let lines = [
        format!("no risk note on s1: {reply}"),
        "no risk note on s1: `claude -p` failed (exit status: 1): \"secret reply\"".into(),
        "no risk note on s1: `claude -p` ran past 15s and was stopped".into(),
    ];
    for line in &lines {
        let kept = scrubbed(line);
        assert!(
            !kept.contains("rm -rf") && !kept.contains("secret"),
            "{kept}"
        );
        assert!(kept.starts_with("no risk note on s1: "), "{kept}");
    }
    assert!(scrubbed(&lines[2]).contains("stopped"));
}

#[test]
fn every_save_line_of_the_writers_is_lost_data() {
    let sources = [
        include_str!("../../understanding/src/lib.rs"),
        include_str!("../../understanding/src/brief.rs"),
        include_str!("../../understanding/src/summary.rs"),
        include_str!("../../understanding/src/turn_writing.rs"),
    ];
    let mut saves = 0;
    for source in sources {
        for line in source
            .lines()
            .filter_map(|l| l.split("log(format!(\"").nth(1))
        {
            let text = line.split('"').next().unwrap_or("");
            if text.to_lowercase().contains("sav") {
                assert!(lost_data(text), "a save line is not lost data: {text}");
                saves += 1;
            }
        }
    }
    assert!(saves >= 4, "found only {saves} save lines");
}

#[test]
fn a_logged_line_keeps_no_quoted_reply() {
    assert_eq!(
        scrubbed("no summary on t1: `claude -p` failed (exit status: 1): \"secret card text\""),
        "no summary on t1: `claude -p` failed (exit status: 1)"
    );
    assert_eq!(
        scrubbed("no session name: the reply holds none"),
        "no session name: the reply holds none"
    );
}

#[test]
fn only_a_failed_save_is_lost_data() {
    assert!(lost_data("save the card store: disk full"));
    assert!(lost_data("save the summaries of s1: disk full"));
    assert!(!lost_data("no summary on t1: the writer is away"));
}
