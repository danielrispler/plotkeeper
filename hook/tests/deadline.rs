// Seam 4 (#43): the hook's deadline. The binary runs with a scratch `HOME`; the test holds the
// event log's lock, `events.lock`, as a stuck hook would.
use crate::common::{feed, hold, hook as hook_command, scratch as scratch_home};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

fn scratch(name: &str) -> PathBuf {
    let home = scratch_home(&format!("deadline-{name}"));
    fs::create_dir_all(studio_hook::event_log(&home).parent().unwrap()).unwrap();
    home
}

/// Runs the hook with `event` and returns how long it took.
fn hook(home: &Path, patience_ms: u64, event: serde_json::Value) -> Duration {
    let start = Instant::now();
    let mut cmd = hook_command(home);
    cmd.env("PLOTKEEPER_HOOK_PATIENCE_MS", patience_ms.to_string())
        .stdout(Stdio::piped());
    let out = feed(cmd, &event).wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    start.elapsed()
}

fn event() -> serde_json::Value {
    serde_json::json!({ "session_id": "s-1", "hook_event_name": "PostToolUse", "cwd": "/" })
}

// @law: log-writers-hold-the-log-lock
#[test]
fn a_held_lock_gives_a_drops_line_and_no_log_line_once_its_patience_runs_out() {
    let home = scratch("held");
    let log = studio_hook::event_log(&home);
    let held = hold(&log);

    let took = hook(&home, 200, event());
    drop(held);

    assert!(took >= Duration::from_millis(200), "took {took:?}");
    assert!(took < Duration::from_secs(3), "took {took:?}");
    assert_eq!(fs::read_to_string(&log).unwrap_or_default(), "");
    let drops = fs::read_to_string(studio_hook::drops_file(&log)).unwrap();
    let fields: Vec<&str> = drops.trim_end().split(' ').collect();
    assert_eq!(drops.lines().count(), 1);
    assert_eq!(&fields[1..], ["PostToolUse"]);
    assert!(fields[0].parse::<u64>().is_ok());
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&home).ok();
}

#[test]
fn a_free_lock_writes_the_log_and_leaves_the_drops_file_alone() {
    let home = scratch("free");
    let log = studio_hook::event_log(&home);

    hook(&home, 200, event());

    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    assert!(!studio_hook::drops_file(&log).exists());
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&home).ok();
}
