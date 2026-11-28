// The event log's growth (danielrispler/plotkeeper#132): the hook leaves out the two heavy fields,
// and a prune moves the sessions that ended more than 30 days ago to the archive.
use crate::common::{feed, hold, hook, scratch as scratch_home};
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

/// A prune replaces the log's file while it holds the lock. A hook that waits for the lock must write
/// to the new one, or its line is lost.
// @law: log-writers-hold-the-log-lock
#[test]
fn a_line_waiting_for_the_lock_lands_in_the_log_that_replaced_it() {
    let home = scratch("replaced");
    let log = studio_hook::event_log(&home);
    fs::write(&log, "").unwrap();
    let held = hold(&log);
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

/// A camelCase Copilot line has its session in `sessionId` (#361): its old and recent lines stay
/// together.
#[test]
fn a_prune_keeps_a_copilot_sessions_camel_case_lines_together() {
    let home = scratch("copilot");
    let log = studio_hook::event_log(&home);
    let copilot = |ms: u64| {
        json!({
            "sessionId": "cs-1",
            "studio_event": "preToolUse",
            "studio_agent": "copilot",
            "studio_received_ms": ms,
        })
    };
    let (old, recent) = (copilot(NOW - 40 * DAY), copilot(NOW - DAY));
    write(
        &log,
        &[old.clone(), at("s-0", NOW - 40 * DAY), recent.clone()],
    );

    prune(&log, NOW).unwrap();

    assert_eq!(lines(&log), [old, recent]);
}

/// No reader meets the log's lock: while another writer holds `events.lock`, the log opens and reads whole, and an
/// append waits for the lock and lands once it is let go.
// @law: log-writers-hold-the-log-lock
#[test]
fn while_the_lock_is_held_the_log_reads_whole_and_an_append_waits_then_lands() {
    let home = scratch("held");
    let log = studio_hook::event_log(&home);
    write(&log, &[at("s-0", NOW)]);
    let held = hold(&log);
    let waiting = {
        let log = log.clone();
        thread::spawn(move || studio_hook::append(&log, &at("s-1", NOW), Duration::from_secs(5)))
    };
    thread::sleep(Duration::from_millis(200));

    assert_eq!(lines(&log), [at("s-0", NOW)]);
    assert!(!waiting.is_finished());
    drop(held);
    assert!(waiting.join().unwrap().unwrap());
    assert_eq!(lines(&log), [at("s-0", NOW), at("s-1", NOW)]);
}

/// An append whose patience runs out before the holder lets go gives `false` and writes nothing.
// @law: log-writers-hold-the-log-lock
#[test]
fn an_append_whose_patience_is_shorter_than_the_hold_writes_nothing() {
    let home = scratch("patience");
    let log = studio_hook::event_log(&home);
    write(&log, &[at("s-0", NOW)]);
    let held = hold(&log);

    let appended = studio_hook::append(&log, &at("s-1", NOW), Duration::from_millis(100));

    drop(held);
    assert!(!appended.unwrap());
    assert_eq!(lines(&log), [at("s-0", NOW)]);
}

/// A data folder from before the log had a lock file of its own gets one on the first append, and its log keeps its
/// lines.
// @law: log-writers-hold-the-log-lock
#[test]
fn a_data_folder_with_no_lock_file_gets_one_on_the_first_append_and_keeps_its_lines() {
    let home = scratch("no-lock");
    let log = studio_hook::event_log(&home);
    write(&log, &[at("s-0", NOW)]);
    let lock = log.with_file_name("events.lock");
    assert!(!lock.exists());

    assert!(studio_hook::append(&log, &at("s-1", NOW), Duration::from_secs(1)).unwrap());

    assert!(lock.is_file());
    assert_eq!(lines(&log), [at("s-0", NOW), at("s-1", NOW)]);
}

/// Many appends at once, from threads, give whole lines, none interleaved, in a log that ends with a newline.
// @law: log-writers-hold-the-log-lock
#[test]
fn appends_from_many_threads_at_once_give_whole_lines() {
    let home = scratch("threads");
    let log = studio_hook::event_log(&home);
    let line = |thread: usize, n: usize| json!({ "session_id": format!("s-{thread}-{n}"), "studio_received_ms": NOW, "pad": "x".repeat(20_000) });
    let threads: Vec<_> = (0..8)
        .map(|thread| {
            let log = log.clone();
            thread::spawn(move || {
                for n in 0..20 {
                    let event = line(thread, n);
                    assert!(studio_hook::append(&log, &event, Duration::from_secs(30)).unwrap());
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }

    let text = fs::read_to_string(&log).unwrap();
    assert!(text.ends_with('\n'));
    let mut logged = lines(&log);
    logged.sort_by_key(|l| l["session_id"].as_str().unwrap().to_owned());
    let mut expected: Vec<_> = (0..8)
        .flat_map(|t| (0..20).map(move |n| line(t, n)))
        .collect();
    expected.sort_by_key(|l| l["session_id"].as_str().unwrap().to_owned());
    assert_eq!(logged, expected);
}

/// A prune reads the log, then waits for the lock to swap the new log in; lines appended between its read and its swap
/// are copied to the new log's tail.
// @law: log-writers-hold-the-log-lock
#[test]
fn a_prunes_tail_copy_keeps_the_lines_appended_between_its_read_and_its_swap() {
    let home = scratch("tail");
    let log = studio_hook::event_log(&home);
    let (old, recent, late) = (
        at("old", NOW - 40 * DAY),
        at("recent", NOW - DAY),
        at("late", NOW),
    );
    write(&log, &[old.clone(), recent.clone()]);
    let held = hold(&log);
    let pruning = {
        let log = log.clone();
        thread::spawn(move || prune(&log, NOW))
    };
    thread::sleep(Duration::from_millis(300));
    assert!(
        !pruning.is_finished(),
        "the prune swapped the log without its lock"
    );
    // This test holds the lock, so it may write the log, as a hook that got the lock before the prune would.
    let mut file = fs::OpenOptions::new().append(true).open(&log).unwrap();
    std::io::Write::write_all(&mut file, format!("{late}\n").as_bytes()).unwrap();
    drop(file);
    drop(held);

    let pruned = pruning.join().unwrap().unwrap();

    assert_eq!(pruned.archived, 1);
    assert_eq!(lines(&log), [recent, late]);
    assert_eq!(lines(&archive_file(&log)), [old]);
}

/// Every writer of the log goes through the hook crate's log code, which takes the lock: outside it, `archive/` and the
/// tests' own helpers, no source file names the event log and opens a file to write.
// @law: log-writers-hold-the-log-lock
#[test]
fn no_file_but_the_log_code_names_the_log_and_opens_a_file_to_write() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let listed = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", "*.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed = String::from_utf8(listed.stdout).unwrap();
    let ours = listed.split('\0').filter(|file| {
        let helper = file.starts_with("test-support/")
            || file.contains("/tests/")
            || file.ends_with("tests.rs");
        !file.is_empty() && !helper && !file.starts_with("archive/") && *file != "hook/src/lib.rs"
    });
    let names = ["events.jsonl", "EVENT_LOG"];
    let opens = ["OpenOptions", "File::create", "fs::write"];
    let mut found = Vec::new();
    for file in ours {
        let text = fs::read_to_string(root.join(file)).unwrap();
        // A file's own test module, inline or in its `tests.rs`, is a test helper.
        let shipped = text.split("#[cfg(test)]").next().unwrap();
        let has = |words: &[&str]| words.iter().any(|w| shipped.contains(w));
        if has(&names) && has(&opens) {
            found.push(file);
        }
    }
    assert!(
        found.is_empty(),
        "names the log and opens a file to write: {found:#?}"
    );
}
