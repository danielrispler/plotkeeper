// The event log's growth (danielrispler/plotkeeper#132): the hook leaves out the two heavy fields,
// and a prune moves the sessions that ended more than 30 days ago to the archive.
use crate::common::{feed, hook, scratch as scratch_home};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    thread,
    time::Duration,
};
use studio_hook::{Pruned, archive_file, prune};

const DAY: u64 = 24 * 60 * 60 * 1000;
const NOW: u64 = 100 * DAY;

fn scratch(name: &str) -> PathBuf {
    let home = scratch_home(&format!("prune-{name}"));
    fs::create_dir_all(studio_hook::event_log(&home).parent().unwrap()).unwrap();
    home
}

fn lines(path: &Path) -> Vec<Value> {
    let text = fs::read_to_string(path).unwrap_or_default();
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn write(path: &Path, events: &[Value]) {
    let text: String = events.iter().map(|e| format!("{e}\n")).collect();
    fs::write(path, text).unwrap();
}

fn at(session: &str, ms: u64) -> Value {
    json!({ "session_id": session, "hook_event_name": "PostToolUse", "studio_received_ms": ms })
}

fn edit() -> Value {
    json!({
        "session_id": "s-1",
        "hook_event_name": "PostToolUse",
        "cwd": "/",
        "tool_name": "Edit",
        "tool_response": {
            "filePath": "/w/a.rs",
            "originalFile": "the whole file\n",
            "structuredPatch": [{ "oldStart": 1, "lines": ["-a", "+b"] }],
        },
    })
}

fn read() -> Value {
    json!({
        "session_id": "s-1",
        "hook_event_name": "PostToolUse",
        "cwd": "/",
        "tool_name": "Read",
        "tool_response": { "type": "text", "file": { "filePath": "/w/a.rs", "content": "a\n", "numLines": 1 } },
    })
}

#[test]
fn the_hook_leaves_out_an_edits_original_file_and_a_reads_content() {
    let home = scratch("hook");
    for event in [edit(), read()] {
        let mut cmd = hook(&home);
        cmd.stdout(Stdio::null());
        assert!(feed(cmd, &event).wait().unwrap().success());
    }

    let logged = lines(&studio_hook::event_log(&home));

    let edit = &logged[0]["tool_response"];
    assert_eq!(edit.get("originalFile"), None);
    assert_eq!(edit["structuredPatch"][0]["lines"], json!(["-a", "+b"]));
    assert_eq!(edit["filePath"], "/w/a.rs");
    let file = &logged[1]["tool_response"]["file"];
    assert_eq!(file.get("content"), None);
    assert_eq!(file["filePath"], "/w/a.rs");
    assert_eq!(file["numLines"], 1);
}

#[test]
fn a_prune_archives_the_sessions_whose_last_event_is_older_than_30_days() {
    let home = scratch("archive");
    let log = studio_hook::event_log(&home);
    let old = at("old", NOW - 40 * DAY);
    let long = at("long", NOW - 40 * DAY);
    let rename =
        json!({ "session_id": "old", "hook_event_name": "StudioRename", "studio_name": "x" });
    let recent = at("recent", NOW - DAY);
    let long_again = at("long", NOW - 2 * DAY);
    write(
        &log,
        &[
            old.clone(),
            long.clone(),
            rename.clone(),
            recent.clone(),
            long_again.clone(),
        ],
    );

    let pruned = prune(&log, NOW).unwrap();

    assert_eq!(
        pruned,
        Pruned {
            archived: 2,
            lightened: 0
        }
    );
    assert_eq!(lines(&log), [long, recent, long_again]);
    assert_eq!(lines(&archive_file(&log)), [old, rename]);
}

#[test]
fn a_prune_appends_to_the_archive_and_strips_the_heavy_fields_from_the_lines_it_keeps() {
    let home = scratch("strip");
    let log = studio_hook::event_log(&home);
    let earlier = at("earlier", NOW - 90 * DAY);
    write(&archive_file(&log), std::slice::from_ref(&earlier));
    let old = at("old", NOW - 31 * DAY);
    let mut kept = edit();
    kept["studio_received_ms"] = json!(NOW);
    write(&log, &[old.clone(), kept.clone(), read()]);

    let pruned = prune(&log, NOW).unwrap();

    assert_eq!(
        pruned,
        Pruned {
            archived: 1,
            lightened: 2
        }
    );
    assert_eq!(lines(&archive_file(&log)), [earlier, old]);
    let logged = lines(&log);
    assert_eq!(logged.len(), 2);
    assert_eq!(logged[0]["tool_response"].get("originalFile"), None);
    assert_eq!(
        logged[0]["tool_response"]["structuredPatch"],
        kept["tool_response"]["structuredPatch"]
    );
    assert_eq!(logged[1]["tool_response"]["file"].get("content"), None);
}

#[test]
fn a_log_whose_first_line_is_recent_is_left_as_it_is() {
    let home = scratch("recent");
    let log = studio_hook::event_log(&home);
    let mut first = edit();
    first["studio_received_ms"] = json!(NOW - DAY);
    write(&log, &[first, at("s-2", NOW)]);
    let before = fs::read(&log).unwrap();

    assert_eq!(prune(&log, NOW).unwrap(), Pruned::default());
    assert_eq!(fs::read(&log).unwrap(), before);
    assert!(!archive_file(&log).exists());
}

#[test]
fn no_log_prunes_nothing() {
    let home = scratch("none");
    let log = studio_hook::event_log(&home);

    assert_eq!(prune(&log, NOW).unwrap(), Pruned::default());
    assert!(!log.exists());
}

/// A prune replaces the log's file while it holds the lock. A hook that opened the old file and waits
/// for the lock must write to the new one, or its line is lost.
#[test]
fn a_line_waiting_for_the_lock_lands_in_the_log_that_replaced_it() {
    let home = scratch("replaced");
    let log = studio_hook::event_log(&home);
    fs::write(&log, "").unwrap();
    let held = fs::File::open(&log).unwrap();
    held.lock().unwrap();
    let waiting = {
        let log = log.clone();
        thread::spawn(move || studio_hook::append(&log, &at("s-1", NOW), Duration::from_secs(5)))
    };
    thread::sleep(Duration::from_millis(200));
    let next = log.with_file_name("next.jsonl");
    write(&next, &[at("s-0", NOW)]);
    fs::rename(&next, &log).unwrap();
    drop(held);

    assert!(waiting.join().unwrap().unwrap());
    assert_eq!(lines(&log), [at("s-0", NOW), at("s-1", NOW)]);
}

/// A line's session is read by the agent its hook entry named (#355): an agy line goes by its
/// conversation, whatever other id its payload holds.
#[test]
fn a_prune_keeps_a_sessions_lines_together_by_the_id_its_agent_uses() {
    let home = scratch("agent");
    let log = studio_hook::event_log(&home);
    let agy = |ms: u64, extra: Value| {
        let mut line = json!({
            "conversationId": "c-1",
            "hookEventName": "PreToolUse",
            "studio_agent": "agy",
            "studio_received_ms": ms,
        });
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line
    };
    let old = agy(NOW - 40 * DAY, json!({ "session_id": "a-tools-id" }));
    let recent = agy(NOW - DAY, json!({}));
    let claude_old = json!({
        "session_id": "s-1",
        "conversationId": "c-1",
        "hook_event_name": "Stop",
        "studio_agent": "claude",
        "studio_received_ms": NOW - 40 * DAY,
    });
    write(&log, &[old.clone(), claude_old.clone(), recent.clone()]);

    prune(&log, NOW).unwrap();

    assert_eq!(lines(&log), [old, recent]);
    assert_eq!(lines(&archive_file(&log)), [claude_old]);
}

/// A Cursor line goes by its `conversation_id` (#362): its `sessionStart` also carries a
/// `session_id`, and its other events carry none.
#[test]
fn a_prune_keeps_a_cursor_sessions_lines_together_by_its_conversation_id() {
    let home = scratch("cursor");
    let log = studio_hook::event_log(&home);
    let start = json!({
        "conversation_id": "c-1",
        "session_id": "c-1-start",
        "hook_event_name": "sessionStart",
        "studio_agent": "cursor",
        "studio_received_ms": NOW - 40 * DAY,
    });
    let recent = json!({
        "conversation_id": "c-1",
        "hook_event_name": "stop",
        "studio_agent": "cursor",
        "studio_received_ms": NOW - DAY,
    });
    write(&log, &[start.clone(), recent.clone()]);

    prune(&log, NOW).unwrap();

    assert_eq!(lines(&log), [start, recent]);
}
