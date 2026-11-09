//! #481 (#461, decision 19): the status line writes one snapshot of its stdin per session into the
//! studio's `usage` folder; the studio shows the plan's 5-hour and weekly usage from the newest one.

use serde_json::json;
use std::{fs, thread, time::Duration};
use studio_workbench::{context_window, plan_usage};

const LIMITS: &str = r#"{"five_hour":{"used_percentage":23.5,"resets_at":1738425600},"seven_day":{"used_percentage":41.2,"resets_at":1738857600}}"#;

fn snapshot(limits: &str) -> String {
    format!(r#"{{"session_id":"s","model":{{"id":"x"}},"rate_limits":{limits}}}"#)
}

#[test]
fn the_newest_snapshot_gives_the_rate_limits() {
    let dir = tempfile::tempdir().unwrap();
    let old = r#"{"five_hour":{"used_percentage":1,"resets_at":1}}"#;
    fs::write(dir.path().join("old.json"), snapshot(old)).unwrap();
    thread::sleep(Duration::from_millis(20));
    fs::write(dir.path().join("new.json"), snapshot(LIMITS)).unwrap();

    let usage = plan_usage(dir.path()).unwrap();

    assert_eq!(
        usage["five_hour"],
        json!({"used_percentage": 23.5, "resets_at": 1738425600})
    );
    assert_eq!(usage["seven_day"]["used_percentage"], json!(41.2));
}

#[test]
fn a_missing_folder_or_a_snapshot_without_limits_is_no_usage() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(plan_usage(&dir.path().join("usage")), None);
    assert_eq!(plan_usage(dir.path()), None);
    // Before the session's first API response, or on a plan without limits, stdin has none.
    fs::write(dir.path().join("s.json"), r#"{"session_id":"s"}"#).unwrap();
    assert_eq!(plan_usage(dir.path()), None);
}

#[test]
fn a_half_written_snapshot_is_no_usage() {
    let dir = tempfile::tempdir().unwrap();
    let whole = snapshot(LIMITS);
    fs::write(dir.path().join("s.json"), &whole[..whole.len() / 2]).unwrap();
    assert_eq!(plan_usage(dir.path()), None);
}

#[test]
fn the_status_lines_temp_file_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("s.json"), snapshot(LIMITS)).unwrap();
    thread::sleep(Duration::from_millis(20));
    let partial = snapshot(r#"{"five_hour":{"used_percentage":99,"resets_at":1}}"#);
    fs::write(dir.path().join("s.json.tmp"), partial).unwrap();
    assert_eq!(
        plan_usage(dir.path()).unwrap()["five_hour"]["used_percentage"],
        json!(23.5)
    );
}

/// #517, decision 1; #519: a session's context is its own snapshot's `context_window`.
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
