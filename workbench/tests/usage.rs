//! danielrispler/Falinks#481 (danielrispler/Falinks#461, decision 19): the status line writes one snapshot of its stdin per session into the
//! studio's `usage` folder; the studio shows the plan's 5-hour and weekly usage from the newest one.

use serde_json::json;
use std::{fs, thread, time::Duration};
use studio_workbench::{
    AgentUsage, claude_usage, cli_usages, context_window, find_in_path, login_shell_path,
    parse_agy, parse_codex, read_agy, read_codex,
};

const LIMITS: &str = r#"{"five_hour":{"used_percentage":23.5,"resets_at":1738425600},"seven_day":{"used_percentage":41.2,"resets_at":1738857600}}"#;

fn snapshot(limits: &str) -> String {
    format!(r#"{{"session_id":"s","model":{{"id":"x"}},"rate_limits":{limits}}}"#)
}

/// Whether Claude's usage in `dir` is no usage: no windows, and a reason why.
fn no_usage(dir: &std::path::Path) -> bool {
    let usage = claude_usage(dir);
    usage.groups.is_empty() && usage.reason.is_some()
}

#[test]
fn the_newest_snapshot_gives_the_rate_limits() {
    let dir = tempfile::tempdir().unwrap();
    let old = r#"{"five_hour":{"used_percentage":1,"resets_at":1}}"#;
    fs::write(dir.path().join("old.json"), snapshot(old)).unwrap();
    thread::sleep(Duration::from_millis(20));
    fs::write(dir.path().join("new.json"), snapshot(LIMITS)).unwrap();

    let usage = claude_usage(dir.path());

    let windows = &usage.groups[0].windows;
    assert_eq!(
        (windows[0].used_percentage, windows[0].resets_at),
        (23.5, 1738425600)
    );
    assert_eq!(windows[1].used_percentage, 41.2);
}

#[test]
fn a_missing_folder_or_a_snapshot_without_limits_is_no_usage() {
    let dir = tempfile::tempdir().unwrap();
    assert!(no_usage(&dir.path().join("usage")));
    assert!(no_usage(dir.path()));
    // Before the session's first API response, or on a plan without limits, stdin has none.
    fs::write(dir.path().join("s.json"), r#"{"session_id":"s"}"#).unwrap();
    assert!(no_usage(dir.path()));
}

#[test]
fn a_half_written_snapshot_is_no_usage() {
    let dir = tempfile::tempdir().unwrap();
    let whole = snapshot(LIMITS);
    fs::write(dir.path().join("s.json"), &whole[..whole.len() / 2]).unwrap();
    assert!(no_usage(dir.path()));
}

#[test]
fn the_status_lines_temp_file_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("s.json"), snapshot(LIMITS)).unwrap();
    thread::sleep(Duration::from_millis(20));
    let partial = snapshot(r#"{"five_hour":{"used_percentage":99,"resets_at":1}}"#);
    fs::write(dir.path().join("s.json.tmp"), partial).unwrap();
    assert_eq!(
        claude_usage(dir.path()).groups[0].windows[0].used_percentage,
        23.5
    );
}

/// danielrispler/Falinks#517, decision 1; danielrispler/Falinks#519: a session's context is its own snapshot's `context_window`.
#[test]
fn a_sessions_snapshot_gives_its_context_window() {
    let dir = tempfile::tempdir().unwrap();
    let window = |size: &str| {
        format!(
            r#"{{"session_id":"a-1","context_window":{{"context_window_size":{size},"used_percentage":13}}}}"#
        )
    };
    fs::write(dir.path().join("a-1.json"), window("1000000")).unwrap();
    fs::write(dir.path().join("b-2.json"), window("200000")).unwrap();

    let a = json!({"context_window_size": 1_000_000, "used_percentage": 13});
    assert_eq!(context_window(dir.path(), "a-1"), Some(a));
    let b = context_window(dir.path(), "b-2").unwrap();
    assert_eq!(b["context_window_size"], json!(200_000));
    // No snapshot, one without the field, or an id that is no file name: none.
    assert_eq!(context_window(dir.path(), "c-3"), None);
    fs::write(dir.path().join("d-4.json"), r#"{"session_id":"d-4"}"#).unwrap();
    assert_eq!(context_window(dir.path(), "d-4"), None);
    let name = dir.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(context_window(dir.path(), &format!("../{name}/a-1")), None);
}

/// danielrispler/plotkeeper#163: the usage seam gives Claude's windows, when the snapshot was saved and where it came from.
#[test]
fn claude_usage_gives_the_windows_the_snapshots_time_and_its_source() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("s.json"), snapshot(LIMITS)).unwrap();
    let saved = fs::metadata(dir.path().join("s.json"))
        .unwrap()
        .modified()
        .unwrap();
    let saved = saved
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let usage = serde_json::to_value(claude_usage(dir.path())).unwrap();

    assert_eq!(usage["agent"], json!("claude"));
    assert_eq!(
        usage["groups"],
        json!([{"name": "", "windows": [
            {"label": "5h", "used_percentage": 23.5, "resets_at": 1738425600},
            {"label": "week", "used_percentage": 41.2, "resets_at": 1738857600},
        ]}])
    );
    assert_eq!(usage["read_at"], json!(saved));
    assert_eq!(usage["source"], json!("last Claude session"));
    assert_eq!(usage["reason"], json!(null));
}

#[test]
fn claude_usage_without_a_snapshot_says_why_and_gives_no_number() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("s.json"), r#"{"session_id":"s"}"#).unwrap();
    for folder in [dir.path().join("none"), dir.path().to_path_buf()] {
        let usage = serde_json::to_value(claude_usage(&folder)).unwrap();
        assert_eq!(usage["agent"], json!("claude"));
        assert_eq!(usage["groups"], json!([]));
        assert_eq!(usage["read_at"], json!(null));
        assert!(usage["reason"].as_str().unwrap().contains("Claude"));
    }
}

// danielrispler/plotkeeper#165: Codex's and agy's readers, from the replies #161's spike recorded.

fn recorded(name: &str) -> serde_json::Value {
    let path = format!("{}/tests/fixtures/usage/{name}", env!("CARGO_MANIFEST_DIR"));
    let wrapped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    wrapped["reply"].clone()
}

/// A stand-in for a CLI: a script that runs `body`.
fn fake_cli(dir: &std::path::Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path.to_str().unwrap().into()
}

fn reason(usage: &AgentUsage) -> &str {
    assert!(usage.groups.is_empty(), "a failed read gives no number");
    usage.reason.as_deref().unwrap()
}

#[test]
fn codexs_free_plan_reply_gives_one_30_day_window() {
    let usage = parse_codex(&recorded("codex-rate-limits.json"), 1000);
    assert_eq!(usage.agent, "codex");
    assert_eq!(usage.reason, None);
    assert_eq!(usage.read_at, Some(1000));
    assert_eq!(usage.source.as_deref(), Some("codex app-server"));
    assert_eq!(
        serde_json::to_value(&usage.groups).unwrap(),
        json!([{"name": "", "windows": [{"label": "30d", "used_percentage": 0.0, "resets_at": 1797525063}]}])
    );
}

#[test]
fn codexs_windows_are_named_by_their_length_not_their_place() {
    let reply = json!({"id": 2, "result": {"rateLimits": {
        "primary": {"usedPercent": 10, "windowDurationMins": 10080, "resetsAt": 5},
        "secondary": {"usedPercent": 20, "windowDurationMins": 300, "resetsAt": 6},
    }}});
    let windows = &parse_codex(&reply, 0).groups[0].windows;
    let labels: Vec<_> = windows.iter().map(|w| w.label.as_str()).collect();
    assert_eq!(labels, ["5h", "week"]);
    assert_eq!(windows[0].used_percentage, 20.0);
}

#[test]
fn agys_reply_gives_a_group_per_model_group_with_used_shares_and_reset_times() {
    let usage = parse_agy(&recorded("agy-usage.json"), 1000);
    assert_eq!(usage.reason, None);
    assert_eq!(usage.source.as_deref(), Some("agy /usage"));
    let names: Vec<_> = usage.groups.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(
        names,
        ["Gemini Models", "Claude and GPT models", "Gemini 4 Argon"]
    );
    let gemini = &usage.groups[0].windows;
    assert_eq!(gemini[0].label, "week");
    assert!((gemini[0].used_percentage - 0.9726941585).abs() < 1e-6);
    // 2026-11-21T14:09:17Z
    assert_eq!(gemini[0].resets_at, 1_795_270_157);
    assert_eq!(gemini[1].label, "5h");
    let argon = &usage.groups[2].windows;
    assert_eq!(argon.len(), 1);
    assert!((argon[0].used_percentage - 78.2952398).abs() < 1e-5);
}

#[test]
fn an_unreadable_reply_gives_a_reason_for_each_agent() {
    assert!(
        reason(&parse_codex(
            &json!({"id": 2, "error": {"message": "not logged in"}}),
            0
        ))
        .contains("not logged in")
    );
    assert!(reason(&parse_codex(&json!({"id": 2, "result": {}}), 0)).contains("Codex"));
    let none = json!({"id": 2, "result": {"rateLimits": {"primary": null, "secondary": null}}});
    assert!(reason(&parse_codex(&none, 0)).contains("Codex"));
    assert!(reason(&parse_agy(&json!({"status": "ERROR"}), 0)).contains("agy"));
    assert!(
        reason(&parse_agy(
            &json!({"status": "SUCCESS", "command": {"data": {"groups": []}}}),
            0
        ))
        .contains("agy")
    );
}

#[test]
fn a_command_that_fails_gives_a_reason_line() {
    let dir = tempfile::tempdir().unwrap();
    let broken = fake_cli(dir.path(), "broken", "exit 3");
    assert!(reason(&read_codex(&broken, None, Duration::from_secs(10), 0)).contains("Codex"));
    assert!(reason(&read_agy(&broken, None, Duration::from_secs(10), 0)).contains("agy"));
    let missing = dir.path().join("none");
    assert!(
        reason(&read_agy(
            missing.to_str().unwrap(),
            None,
            Duration::from_secs(10),
            0
        ))
        .contains("did not start")
    );
}

#[test]
fn a_reply_that_is_not_the_answer_gives_a_reason_line() {
    let dir = tempfile::tempdir().unwrap();
    let junk = fake_cli(
        dir.path(),
        "junk",
        "echo 'not json'; echo '{\"id\":1,\"result\":{}}'",
    );
    assert!(reason(&read_codex(&junk, None, Duration::from_secs(10), 0)).contains("no answer"));
    assert!(reason(&read_agy(&junk, None, Duration::from_secs(10), 0)).contains("not JSON"));
}

#[test]
fn a_command_that_times_out_is_killed_and_gives_a_reason_line() {
    let dir = tempfile::tempdir().unwrap();
    let slow = fake_cli(dir.path(), "slow", "exec sleep 30");
    let start = std::time::Instant::now();
    assert!(reason(&read_codex(&slow, None, Duration::from_millis(200), 0)).contains("in time"));
    assert!(reason(&read_agy(&slow, None, Duration::from_millis(200), 0)).contains("in time"));
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn the_readers_run_the_cli_and_parse_what_it_prints() {
    let dir = tempfile::tempdir().unwrap();
    let codex = fake_cli(
        dir.path(),
        "codex",
        &format!(
            "cat >/dev/null; echo '{{\"method\":\"account/updated\"}}'; echo '{}'",
            recorded("codex-rate-limits.json")
        ),
    );
    assert_eq!(
        read_codex(&codex, None, Duration::from_secs(10), 1).groups[0].windows[0].label,
        "30d"
    );
    let reply = dir.path().join("agy.json");
    fs::write(&reply, recorded("agy-usage.json").to_string()).unwrap();
    let agy = fake_cli(dir.path(), "agy", &format!("cat '{}'", reply.display()));
    assert_eq!(
        read_agy(&agy, None, Duration::from_secs(10), 1)
            .groups
            .len(),
        3
    );
}

#[test]
fn an_agent_whose_cli_is_not_found_has_no_entry() {
    let dir = tempfile::tempdir().unwrap();
    let agy = fake_cli(dir.path(), "agy", "exit 1");
    let find = |agent: &str| (agent == "agy").then(|| agy.clone());
    let read = cli_usages(&["codex", "agy"], &find, None, 0);
    assert_eq!(
        read.iter().map(|u| u.agent.as_str()).collect::<Vec<_>>(),
        ["agy"]
    );
    assert!(cli_usages(&["codex", "agy"], &|_: &str| None, None, 0).is_empty());
}

/// The shapes the UI's tests read: the recorded replies as the seam gives them, with `read_at` 1000.
#[test]
fn the_recorded_replies_give_the_shapes_the_ui_tests_read() {
    for (reply, parsed, usage) in [
        (
            "codex-rate-limits.json",
            "codex-parsed.json",
            parse_codex as fn(&serde_json::Value, i64) -> AgentUsage,
        ),
        ("agy-usage.json", "agy-parsed.json", parse_agy),
    ] {
        let got = serde_json::to_value(usage(&recorded(reply), 1000)).unwrap();
        let path = format!(
            "{}/tests/fixtures/usage/{parsed}",
            env!("CARGO_MANIFEST_DIR")
        );
        assert_eq!(
            got,
            serde_json::from_str::<serde_json::Value>(&fs::read_to_string(&path).unwrap()).unwrap()
        );
    }
}

#[test]
fn a_read_is_bounded_even_when_a_grandchild_holds_the_pipe_open() {
    let dir = tempfile::tempdir().unwrap();
    // The CLI exits at once; a helper it started keeps its stdout open.
    let leaky = fake_cli(dir.path(), "leaky", "(sleep 30) &\nexit 0");
    let start = std::time::Instant::now();
    let usage = read_agy(&leaky, None, Duration::from_millis(500), 0);
    assert!(reason(&usage).contains("in time"), "{usage:?}");
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_time_with_fractional_seconds_or_a_zero_offset_is_read() {
    for time in [
        "2026-11-17T18:11:50.123456Z",
        "2026-11-17T18:11:50+00:00",
        "2026-11-17T18:11:50.5+00:00",
    ] {
        let mut reply = recorded("agy-usage.json");
        reply["command"]["data"]["groups"][0]["buckets"][0]["reset_time"] = json!(time);
        let usage = parse_agy(&reply, 0);
        assert_eq!(
            usage.groups[0].windows[0].resets_at, 1_794_939_110,
            "{time}"
        );
    }
}

#[test]
fn a_program_is_found_in_the_directories_of_a_given_path() {
    let one = tempfile::tempdir().unwrap();
    let two = tempfile::tempdir().unwrap();
    let agy = fake_cli(two.path(), "agy", "exit 0");
    fs::write(one.path().join("codex"), "not executable").unwrap();
    let path = format!("{}:{}", one.path().display(), two.path().display());
    assert_eq!(find_in_path("agy", &path), Some(agy));
    assert_eq!(find_in_path("codex", &path), None);
    assert_eq!(find_in_path("agy", ""), None);
}

#[test]
fn a_child_sees_the_path_it_was_given() {
    let dir = tempfile::tempdir().unwrap();
    let agy = fake_cli(
        dir.path(),
        "agy",
        r#"printf '{"status":"SUCCESS","command":{"data":{"groups":[{"name":"%s","buckets":[{"window":"5h","remaining_fraction":0.5,"reset_time":"2026-11-17T18:11:50Z"}]}]}}}' "$PATH""#,
    );
    let usage = read_agy(&agy, Some("/given/bin:/other"), Duration::from_secs(10), 0);
    assert_eq!(usage.groups[0].name, "/given/bin:/other");
}

#[test]
fn the_login_shells_path_is_its_last_line_and_a_hung_shell_gives_none() {
    let dir = tempfile::tempdir().unwrap();
    let shell = fake_cli(dir.path(), "shell", "echo 'rc noise'; echo /a/bin:/b/bin");
    assert_eq!(
        login_shell_path(&shell, Duration::from_secs(10)),
        Some("/a/bin:/b/bin".into())
    );
    let hung = fake_cli(dir.path(), "hung", "exec sleep 30");
    let start = std::time::Instant::now();
    assert_eq!(login_shell_path(&hung, Duration::from_millis(300)), None);
    assert!(start.elapsed() < Duration::from_secs(10));
}
