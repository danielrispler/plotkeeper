//! Saved state beside a pruned event log (danielrispler/plotkeeper#134): a prune moves lines, so it
//! removes the saved state, and the replay that follows takes its offsets from the new log, which
//! every row of the ledger reads its details by.

use serde_json::{Value, json};
use std::fs;
use studio_core::Studio;
use studio_hook::{prune, state_file};

const DAY: u64 = 24 * 60 * 60 * 1000;
const NOW: u64 = 100 * DAY;
const OLD: &str = "0a1b2c3d-0000-4000-8000-000000000001";
const NEW: &str = "0a1b2c3d-0000-4000-8000-000000000002";

fn line(session: &str, name: &str, ms: u64, extra: Value) -> String {
    let mut line = json!({
        "session_id": session,
        "hook_event_name": name,
        "cwd": "/w/app",
        "prompt_id": "p1",
        "studio_received_ms": ms,
    });
    line.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    format!("{line}\n")
}

fn call(session: &str, id: &str, tool: &str, input: Value, response: Value, ms: u64) -> String {
    let both = json!({ "tool_name": tool, "tool_input": input, "tool_use_id": id });
    let mut end = both.clone();
    end["tool_response"] = response;
    line(session, "PreToolUse", ms, both) + &line(session, "PostToolUse", ms + 500, end)
}

fn log() -> String {
    let old = DAY;
    let now = NOW - DAY;
    [
        line(OLD, "UserPromptSubmit", old, json!({ "prompt": "old" })),
        call(
            OLD,
            "o1",
            "Read",
            json!({ "file_path": "/w/old.rs" }),
            json!({ "file": { "content": "x" } }),
            old + 1_000,
        ),
        line(NEW, "UserPromptSubmit", now, json!({ "prompt": "new" })),
        call(
            NEW,
            "e1",
            "Edit",
            json!({ "file_path": "/w/app/a.rs" }),
            json!({
                "filePath": "/w/app/a.rs",
                "originalFile": "the whole file\n",
                "structuredPatch": [{ "oldStart": 1, "lines": ["-a", "+b"] }],
            }),
            now + 1_000,
        ),
        call(
            NEW,
            "c1",
            "Bash",
            json!({ "command": "cargo test" }),
            json!({ "stdout": "test result: ok. 3 passed", "stderr": "" }),
            now + 3_000,
        ),
        call(
            NEW,
            "q1",
            "AskUserQuestion",
            json!({ "questions": [{ "question": "Which one?", "options": [{ "label": "A" }] }] }),
            json!({ "answers": { "Which one?": "A" } }),
            now + 5_000,
        ),
    ]
    .concat()
}

fn replayed(text: &str) -> Studio {
    let mut studio = Studio::default();
    text.split_inclusive('\n').for_each(|l| studio.replay(l));
    studio
}

#[test]
fn after_a_prune_every_row_reads_its_details_by_its_offset() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    fs::write(&path, log()).unwrap();
    fs::write(state_file(&path), replayed(&log()).save()).unwrap();

    let pruned = prune(&path, NOW).unwrap();

    assert_eq!((pruned.archived, pruned.lightened), (3, 1));
    assert!(!state_file(&path).exists(), "the saved state is gone");
    let studio = replayed(&fs::read_to_string(&path).unwrap());
    assert!(studio.session(OLD).is_none());
    let ledger = &studio.session(NEW).expect("the session").ledger;
    assert_eq!(ledger.rows().len(), 3);
    let details = |call: &str| ledger.row(None, call).unwrap().details(&path).unwrap();

    let edit = details("e1").end.unwrap();
    assert_eq!(
        edit["tool_response"]["structuredPatch"][0]["lines"],
        json!(["-a", "+b"])
    );
    assert_eq!(edit["tool_response"].get("originalFile"), None);
    let check = details("c1").end.unwrap();
    assert_eq!(
        check["tool_response"]["stdout"],
        "test result: ok. 3 passed"
    );
    let ask = details("q1");
    assert_eq!(
        ask.end.unwrap()["tool_response"]["answers"]["Which one?"],
        "A"
    );
    assert_eq!(ask.start.unwrap()["tool_name"], "AskUserQuestion");
}
