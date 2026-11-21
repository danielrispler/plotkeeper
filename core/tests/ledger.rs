//! Seam 1 (danielrispler/plotkeeper#102, Testing Decisions) for the step ledger
//! (danielrispler/plotkeeper#110): hook events replayed through the core, and the rows they give.

use crate::common;
use serde_json::{Value, json};
use std::path::PathBuf;
use studio_core::{Bytes, Outcome, Row, Studio};

const SESSION: &str = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
const CWD: &str = "/Users/danielrispler/work/Falinks";
const SUBAGENT_RUN: &str = include_str!("fixtures/subagent-run.jsonl");

/// A hook line of the session, at `ms` after the start, with `extra`'s fields.
fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut base = common::base(SESSION, name, CWD, 1_793_265_000_000u64 + ms);
    base["prompt_id"] = json!("p1");
    common::with(base, extra)
}

fn prompt(ms: u64) -> String {
    line("UserPromptSubmit", ms, json!({ "prompt": "Look around" }))
}

fn start(ms: u64, id: &str, tool: &str, input: Value) -> String {
    let call = json!({ "tool_name": tool, "tool_input": input, "tool_use_id": id });
    line("PreToolUse", ms, call)
}

fn done(ms: u64, id: &str, tool: &str, input: Value, response: Value) -> String {
    let call = json!({
        "tool_name": tool, "tool_input": input, "tool_use_id": id,
        "tool_response": response, "duration_ms": 7,
    });
    line("PostToolUse", ms, call)
}

fn failed(ms: u64, id: &str, input: Value, error: &str) -> String {
    let call = json!({
        "tool_name": "Bash", "tool_input": input, "tool_use_id": id, "error": error,
    });
    line("PostToolUseFailure", ms, call)
}

/// `lines` as the log file holds them: each ends in a newline.
fn log(lines: &[String]) -> Vec<u8> {
    lines
        .iter()
        .flat_map(|l| format!("{l}\n").into_bytes())
        .collect()
}

fn bytes_of(file: &[u8], at: Bytes) -> &[u8] {
    &file[at.offset as usize..(at.offset + at.len) as usize]
}

fn rows(studio: &Studio) -> Vec<Row> {
    studio
        .session(SESSION)
        .expect("the session")
        .ledger
        .rows()
        .to_vec()
}

#[test]
fn a_turns_calls_are_rows_with_their_fields() {
    let read_input = json!({ "file_path": format!("{CWD}/src/lib.rs") });
    let grep_input = json!({ "pattern": "fn main" });
    let bash_input = json!({ "command": "cargo test" });
    let studio = Studio::replayed([
        prompt(0),
        start(1_000, "t1", "Read", read_input.clone()),
        done(1_200, "t1", "Read", read_input, json!({ "content": "x" })),
        start(2_000, "t2", "Grep", grep_input.clone()),
        done(2_100, "t2", "Grep", grep_input, json!({})),
        start(3_000, "t3", "Bash", bash_input.clone()),
        failed(4_000, "t3", bash_input, "Exit code 101\nthe output"),
    ]);

    let rows = rows(&studio);

    let read = &rows[0];
    assert_eq!((read.tool.as_str(), read.call.as_str()), ("Read", "t1"));
    assert_eq!(read.file.as_deref(), Some(&*format!("{CWD}/src/lib.rs")));
    assert_eq!(read.turn, Some(0));
    assert_eq!(read.prompt_id.as_deref(), Some("p1"));
    assert_eq!(read.agent, None);
    assert_eq!(read.started_at, Some(1_793_265_001_000));
    assert_eq!(read.duration_ms, Some(7));
    assert_eq!(read.outcome, Some(Outcome::Succeeded));
    assert_eq!(rows[1].pattern.as_deref(), Some("fn main"));
    let bash = &rows[2];
    assert_eq!(bash.command.as_deref(), Some("cargo test"));
    assert_eq!(
        bash.outcome,
        Some(Outcome::Failed {
            exit_code: Some(101)
        })
    );
    // The agent reported no duration: from the start to the result.
    assert_eq!(bash.duration_ms, Some(1_000));
    assert_eq!(rows.len(), 3);
}

#[test]
fn a_call_still_running_has_no_outcome_and_a_new_turn_has_its_own_rows() {
    let studio = Studio::replayed([
        prompt(0),
        start(1_000, "t1", "Read", json!({ "file_path": "/a" })),
        line("Stop", 2_000, json!({ "last_assistant_message": "done" })),
        line(
            "UserPromptSubmit",
            3_000,
            json!({ "prompt": "More", "prompt_id": "p2" }),
        ),
        line(
            "PreToolUse",
            4_000,
            json!({ "prompt_id": "p2", "tool_name": "Read", "tool_use_id": "t2",
                    "tool_input": { "file_path": "/b" } }),
        ),
    ]);

    let session = studio.session(SESSION).unwrap();

    assert_eq!(session.ledger.rows()[0].outcome, None);
    let first: Vec<&str> = session.ledger.turn(0).map(|r| r.call.as_str()).collect();
    let second: Vec<&str> = session.ledger.turn(1).map(|r| r.call.as_str()).collect();
    assert_eq!((first, second), (vec!["t1"], vec!["t2"]));
    let of_file: Vec<&str> = session.ledger.file("/b").map(|r| r.call.as_str()).collect();
    assert_eq!(of_file, ["t2"]);
}

#[test]
fn a_workers_calls_are_rows_of_the_turn_that_spawned_it_with_its_subagent() {
    let studio = Studio::replayed(SUBAGENT_RUN.lines());
    let session = studio.sessions()[0];

    let rows = session.ledger.rows();

    let starts = SUBAGENT_RUN
        .lines()
        .filter(|l| l.contains(r#""hook_event_name":"PreToolUse""#))
        .count();
    assert_eq!(rows.len(), starts);
    assert!(rows.iter().all(|r| r.turn == Some(0)), "{rows:?}");
    let workers: Vec<&Row> = rows.iter().filter(|r| r.agent.is_some()).collect();
    assert!(workers.len() >= 4, "{workers:?}");
    assert!(workers.iter().all(|r| r.outcome.is_some()));
    let mut agents: Vec<&str> = workers.iter().filter_map(|r| r.agent.as_deref()).collect();
    agents.dedup();
    assert!(agents.contains(&"add0c7bbaced40ed0") && agents.contains(&"a551b1ece0a4430ce"));
}

#[test]
fn a_rows_offsets_give_back_the_log_lines_it_came_from_after_a_replay() {
    let lines: Vec<String> = SUBAGENT_RUN.lines().map(Into::into).collect();
    let file = log(&lines);
    let studio = Studio::replayed(&lines);

    let session = studio.sessions()[0];

    assert!(!session.ledger.rows().is_empty());
    for row in session.ledger.rows() {
        let (start, end) = (row.start.expect("a start"), row.end.expect("an end"));
        let text = |at| String::from_utf8(bytes_of(&file, at).to_vec()).unwrap();
        let (start, end) = (text(start), text(end));
        assert!(
            start.contains(r#""hook_event_name":"PreToolUse""#),
            "{start}"
        );
        assert!(end.contains(r#""hook_event_name":"PostToolUse"#), "{end}");
        assert!(lines.contains(&start) && lines.contains(&end));
        assert!(start.contains(&row.call) && end.contains(&row.call));
    }
}

#[test]
fn a_rows_offsets_hold_for_calls_followed_after_the_replay() {
    let input = json!({ "command": "ls" });
    let lines = [
        prompt(0),
        start(1_000, "t1", "Bash", input.clone()),
        done(1_100, "t1", "Bash", input.clone(), json!({})),
        start(2_000, "t2", "Bash", input.clone()),
        done(2_100, "t2", "Bash", input, json!({})),
    ];
    let file = log(&lines);
    let mut studio = Studio::replayed(&lines[..3]);
    // The shell hands the line with its newline, as `read_until` returns it.
    for l in &lines[3..] {
        studio.apply(&format!("{l}\n"));
    }

    let rows = rows(&studio);

    let second = &rows[1];
    assert_eq!(second.call, "t2");
    assert_eq!(bytes_of(&file, second.start.unwrap()), lines[3].as_bytes());
    assert_eq!(bytes_of(&file, second.end.unwrap()), lines[4].as_bytes());
    assert_eq!(bytes_of(&file, rows[0].end.unwrap()), lines[2].as_bytes());
}

#[test]
fn details_come_from_one_seek_and_one_read_of_the_rows_lines() {
    let input = json!({ "command": "cargo test" });
    let lines = [
        prompt(0),
        start(1_000, "t1", "Bash", json!({ "command": "ls" })),
        done(
            1_100,
            "t1",
            "Bash",
            json!({ "command": "ls" }),
            json!({ "stdout": "other" }),
        ),
        start(2_000, "t2", "Bash", input.clone()),
        done(
            2_100,
            "t2",
            "Bash",
            input,
            json!({ "stdout": "test result: ok" }),
        ),
        line("Stop", 3_000, json!({ "last_assistant_message": "done" })),
    ];
    let studio = Studio::replayed(&lines);
    let rows = rows(&studio);
    // Every other line is overwritten with filler of the same length.
    let mut file = log(&lines);
    let mut offset = 0;
    for (i, l) in lines.iter().enumerate() {
        if ![3, 4].contains(&i) {
            file[offset..offset + l.len()].fill(b'x');
        }
        offset += l.len() + 1;
    }
    let mut log = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut log, &file).unwrap();

    let details = rows[1].details(log.path()).unwrap();

    let start = details.start.expect("the start line");
    let end = details.end.expect("the result line");
    assert_eq!(start["tool_input"]["command"], "cargo test");
    assert_eq!(end["tool_response"]["stdout"], "test result: ok");
}

#[test]
fn a_row_holds_no_output() {
    let input = json!({ "command": "echo hi" });
    let studio = Studio::replayed([
        prompt(0),
        start(1_000, "t1", "Bash", input.clone()),
        done(
            1_100,
            "t1",
            "Bash",
            input,
            json!({ "stdout": "OUTPUT-MARKER" }),
        ),
    ]);

    let row = &rows(&studio)[0];

    let shown = format!("{row:?}{}", serde_json::to_string(row).unwrap());
    assert!(!shown.contains("OUTPUT-MARKER"), "{shown}");
}

#[test]
fn a_result_that_lands_before_its_start_is_one_row_with_both() {
    let input = json!({ "file_path": "/a" });
    let lines = [
        prompt(0),
        done(1_100, "t1", "Read", input.clone(), json!({})),
        start(1_000, "t1", "Read", input),
    ];
    let studio = Studio::replayed(&lines);

    let rows = rows(&studio);

    assert_eq!(rows.len(), 1);
    let file = log(&lines);
    assert_eq!(bytes_of(&file, rows[0].end.unwrap()), lines[1].as_bytes());
    assert_eq!(bytes_of(&file, rows[0].start.unwrap()), lines[2].as_bytes());
    assert_eq!(rows[0].started_at, Some(1_793_265_001_000));
    assert_eq!(rows[0].outcome, Some(Outcome::Succeeded));
}

#[test]
fn an_agy_session_gets_rows_from_its_tool_events() {
    let run = include_str!("fixtures/agy/run.jsonl");
    let brain = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    let run = run.replace("@BRAIN@", &brain.display().to_string());
    let lines: Vec<&str> = run.lines().collect();
    let file = log(&lines.iter().map(|l| l.to_string()).collect::<Vec<_>>());
    let studio = Studio::replayed(lines);

    let session = studio
        .session("145c3c4a-3fac-4c6c-8ffc-b1bd95c0f78a")
        .unwrap();

    let rows = session.ledger.rows();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r.turn.is_some() && !r.tool.is_empty()));
    for row in rows {
        let at = row.end.or(row.start).expect("a line");
        let text = std::str::from_utf8(bytes_of(&file, at)).unwrap();
        assert!(serde_json::from_str::<Value>(text).is_ok(), "{text}");
    }
}

#[test]
fn a_calls_row_keeps_the_roots_of_its_start_and_of_its_end() {
    let input = json!({ "command": "git worktree remove /w/second" });
    let touched = |name: &str, ms: u64, roots: Value| {
        let mut event: Value = serde_json::from_str(&line(
            name,
            ms,
            json!({
                "tool_name": "Bash", "tool_input": input.clone(), "tool_use_id": "t1",
                "tool_response": {},
            }),
        ))
        .unwrap();
        event["studio_touched_roots"] = roots;
        event.to_string()
    };
    let studio = Studio::replayed([
        prompt(0),
        touched("PreToolUse", 1_000, json!(["/w", "/w/second"])),
        touched("PostToolUse", 1_200, json!(["/w"])),
    ]);
    let roots = &rows(&studio)[0].roots;
    assert!(roots.iter().any(|r| r == "/w/second"), "{roots:?}");
    assert!(roots.iter().any(|r| r == "/w"), "{roots:?}");
}
