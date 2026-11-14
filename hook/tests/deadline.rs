// Seam 4 (#43): the hook's deadline. The binary runs with a scratch `HOME`; the test holds the
// event log's lock, as a stuck hook would.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn scratch(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("studio-deadline-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(studio_hook::event_log(&home).parent().unwrap()).unwrap();
    home
}

/// Runs the hook with `event` and returns how long it took.
fn hook(home: &Path, event: serde_json::Value) -> Duration {
    let start = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_studio-hook"))
        .env("HOME", home)
        .env("STUDIO_TERMINAL_ID", "t-1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(event.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    start.elapsed()
}

fn event() -> serde_json::Value {
    serde_json::json!({ "session_id": "s-1", "hook_event_name": "PostToolUse", "cwd": "/" })
}

#[test]
fn a_held_lock_gives_a_drops_line_and_no_log_line_within_about_5_seconds() {
    let home = scratch("held");
    let log = studio_hook::event_log(&home);
    let held = fs::File::create(&log).unwrap();
    held.lock().unwrap();

    let took = hook(&home, event());

    assert!(took < Duration::from_secs(8), "took {took:?}");
    assert_eq!(fs::read_to_string(&log).unwrap(), "");
    let drops = fs::read_to_string(studio_hook::drops_file(&log)).unwrap();
    let fields: Vec<&str> = drops.trim_end().split(' ').collect();
    assert_eq!(drops.lines().count(), 1);
    assert_eq!(&fields[1..], ["PostToolUse", "t-1"]);
    assert!(fields[0].parse::<u64>().is_ok());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_free_lock_writes_the_log_and_leaves_the_drops_file_alone() {
    let home = scratch("free");
    let log = studio_hook::event_log(&home);

    hook(&home, event());

    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    assert!(!studio_hook::drops_file(&log).exists());
    let _ = fs::remove_dir_all(&home);
}
