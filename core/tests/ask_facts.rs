//! Seam 1 (danielrispler/plotkeeper#102, Testing Decisions) for the facts Ask shows first
//! (danielrispler/plotkeeper#125): hook events replayed through the core, and what is recorded of each anchor.

use crate::common;
use serde_json::{Value, json};
use studio_core::{Anchor, Studio};

const SESSION: &str = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
const CWD: &str = "/w/repo";

fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut base = common::base(SESSION, name, CWD, 1_793_265_000_000u64 + ms);
    base["prompt_id"] = json!("p1");
    common::with(base, extra)
}

fn call(ms: u64, id: &str, tool: &str, input: Value, agent: Option<&str>) -> Vec<String> {
    let mut start = json!({ "tool_name": tool, "tool_input": input, "tool_use_id": id });
    if let Some(agent) = agent {
        start["agent_id"] = agent.into();
    }
    let mut end = start.clone();
    end["tool_response"] = json!({ "stdout": "fine", "stderr": "" });
    vec![
        line("PreToolUse", ms, start),
        line("PostToolUse", ms + 50, end),
    ]
}

fn failing(ms: u64, id: &str, command: &str, error: &str) -> Vec<String> {
    let start =
        json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": id });
    let mut end = start.clone();
    end["error"] = error.into();
    vec![
        line("PreToolUse", ms, start),
        line("PostToolUseFailure", ms + 50, end),
    ]
}

/// The session's log lines, and a log file holding them.
fn session() -> (Studio, std::path::PathBuf, tempfile::TempDir) {
    let mut lines = vec![line(
        "UserPromptSubmit",
        0,
        json!({ "prompt": "Fix the parser" }),
    )];
    lines.extend(call(
        1_000,
        "r1",
        "Read",
        json!({ "file_path": "/w/repo/src/other.rs" }),
        None,
    ));
    lines.extend(call(
        2_000,
        "g1",
        "Grep",
        json!({ "pattern": "fn parse" }),
        None,
    ));
    lines.extend(call(
        3_000,
        "e1",
        "Edit",
        json!({ "file_path": "/w/repo/src/parse.rs" }),
        None,
    ));
    lines.push(line(
        "PostToolUse",
        3_500,
        json!({ "tool_name": "Agent", "tool_use_id": "s1", "tool_input": { "description": "Edit the parser", "prompt": "Change parse.rs" },
            "tool_response": { "agentId": "c1", "status": "async_launched" } }),
    ));
    lines.extend(call(
        4_000,
        "e2",
        "Edit",
        json!({ "file_path": "/w/repo/src/parse.rs" }),
        Some("c1"),
    ));
    lines.extend(failing(
        5_000,
        "b1",
        "npm run check",
        "Exit code 2\nline one\nerror: it broke",
    ));
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(
        &log,
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )
    .unwrap();
    (Studio::replayed(&lines), log, dir)
}

fn labels(facts: &studio_core::AskFacts) -> Vec<&str> {
    facts.sections.iter().map(|s| s.label.as_str()).collect()
}

#[test]
fn a_files_facts_list_its_editing_steps_in_order_with_the_prompt_and_what_came_before() {
    let (studio, log, _dir) = session();
    let anchor = Anchor::File {
        turn: 0,
        root: "/w/repo".into(),
        path: "src/parse.rs".into(),
        from: None,
    };
    let facts = studio.ask_facts(SESSION, &anchor, &log).unwrap();
    assert_eq!(
        labels(&facts),
        [
            "Daniel asked",
            "Steps that edited it",
            "Read or searched before the first edit"
        ]
    );
    assert_eq!(facts.sections[0].facts[0].text, "Fix the parser");
    let steps: Vec<&str> = facts.sections[1]
        .facts
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0], "Edit by the session's agent");
    assert!(steps[1].starts_with("Edit by "), "{steps:?}");
    assert_ne!(steps[1], steps[0], "the worker is named");
    let before: Vec<&str> = facts.sections[2]
        .facts
        .iter()
        .map(|f| f.text.as_str())
        .collect();
    assert_eq!(before, ["Read /w/repo/src/other.rs", "Grep fn parse"]);
    assert!(facts.sections[1].facts[0].at < facts.sections[1].facts[1].at);
}

#[test]
fn a_failed_checks_facts_show_its_command_exit_code_and_output_tail() {
    let (studio, log, _dir) = session();
    let anchor = Anchor::Check {
        agent: None,
        call: "b1".into(),
    };
    let facts = studio.ask_facts(SESSION, &anchor, &log).unwrap();
    let text = |i: usize| facts.sections[i].facts[0].text.as_str();
    assert_eq!(
        labels(&facts),
        ["Command", "Exit code", "End of the output"]
    );
    assert_eq!(text(0), "npm run check");
    assert_eq!(text(1), "2");
    assert!(text(2).contains("error: it broke"));
}

#[test]
fn the_headlines_facts_are_the_turns_prompt_and_unknown_things_have_none() {
    let (studio, log, _dir) = session();
    let facts = studio
        .ask_facts(SESSION, &Anchor::Headline { turn: 0 }, &log)
        .unwrap();
    assert_eq!(labels(&facts), ["Daniel asked", "Checks"]);
    assert_eq!(facts.sections[1].facts[0].text, "npm run check failed");
    assert!(
        studio
            .ask_facts(SESSION, &Anchor::Headline { turn: 9 }, &log)
            .is_none()
    );
    assert!(
        studio
            .ask_facts("nope", &Anchor::Headline { turn: 0 }, &log)
            .is_none()
    );
}

#[test]
fn a_subagents_facts_are_its_prompt_steps_and_changed_files() {
    let (studio, log, _dir) = session();
    let facts = studio
        .ask_facts(SESSION, &Anchor::Subagent { agent: "c1".into() }, &log)
        .unwrap();
    assert_eq!(
        labels(&facts),
        ["Its prompt", "Its steps", "Files it changed"]
    );
    assert_eq!(facts.sections[0].facts[0].text, "Change parse.rs");
    assert_eq!(facts.sections[1].facts[0].text, "Edit /w/repo/src/parse.rs");
    assert_eq!(facts.sections[2].facts[0].text, "/w/repo/src/parse.rs");
}

#[test]
fn a_named_things_facts_list_its_reads_searches_and_mentions_and_an_unknown_name_has_none() {
    let (studio, _log, _dir) = session();
    let read = studio.ask_named(SESSION, 0, "other.rs").unwrap();
    assert_eq!(labels(&read), ["Read it"]);
    assert_eq!(read.sections[0].facts[0].text, "Read /w/repo/src/other.rs");
    let searched = studio.ask_named(SESSION, 0, "fn parse").unwrap();
    assert_eq!(labels(&searched), ["Searched for it"]);
    let said = studio.ask_named(SESSION, 0, "the parser").unwrap();
    assert_eq!(labels(&said), ["Named in Daniel's prompt"]);
    let changed = studio.ask_named(SESSION, 0, "parse.rs").unwrap();
    assert_eq!(labels(&changed), ["Changed it"]);
    let never = studio.ask_named(SESSION, 0, "lexer.rs").unwrap();
    assert!(never.sections.is_empty());
    assert!(studio.ask_named(SESSION, 9, "x").is_none());
    assert!(
        read.text()
            .starts_with("Read it:\nRead /w/repo/src/other.rs")
    );
}

#[test]
fn a_files_facts_leave_out_another_worktrees_file_of_the_same_relative_path() {
    let (studio, log, _dir) = session();
    let anchor = Anchor::File {
        turn: 0,
        root: "/w/other".into(),
        path: "src/parse.rs".into(),
        from: None,
    };
    let facts = studio.ask_facts(SESSION, &anchor, &log).unwrap();
    assert!(
        !labels(&facts).contains(&"Steps that edited it"),
        "{:?}",
        labels(&facts)
    );
}

/// `lines` as the log file holds them, and a log file holding them.
fn logged(lines: &[String]) -> (std::path::PathBuf, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&log, text).unwrap();
    (log, dir)
}

/// The studio restored from the state a replay of `lines` saved, as the build before a ledger row
/// recorded its tool kind wrote it: its rows have no kind.
fn restored_without_kinds(lines: &[String], log_len: u64) -> Studio {
    let saved = Studio::replayed(lines).save();
    let at = saved.iter().position(|b| *b == b'\n').unwrap() + 1;
    let mut state: Value = serde_json::from_slice(&saved[at..]).unwrap();
    for session in state["sessions"].as_object_mut().unwrap().values_mut() {
        for row in session["ledger"].as_array_mut().unwrap() {
            row.as_object_mut().unwrap().remove("kind");
        }
    }
    let mut old = saved[..at].to_vec();
    old.extend(serde_json::to_vec(&state).unwrap());
    let mut studio = Studio::default();
    assert_eq!(studio.resume(&old, log_len), log_len, "it reads");
    studio
}

/// A Copilot CLI session in `/w/hello` whose one turn creates `hello.txt`.
fn copilot_create() -> Vec<String> {
    let line = |name: &str, ms: u64, extra: Value| {
        let base = json!({
            "sessionId": "cs-1", "timestamp": ms, "cwd": "/w/hello", "studio_event": name,
            "studio_agent": "copilot", "studio_received_ms": ms,
        });
        common::with(base, extra)
    };
    let args = json!({ "path": "/w/hello/hello.txt", "file_text": "hello\n" }).to_string();
    let result = json!({ "resultType": "success", "textResultForLlm": "Created" });
    vec![
        line("sessionStart", 1_000, json!({ "source": "new" })),
        line(
            "userPromptSubmitted",
            2_000,
            json!({ "prompt": "Add hello.txt" }),
        ),
        line(
            "preToolUse",
            3_000,
            json!({ "toolName": "create", "toolArgs": args }),
        ),
        line(
            "postToolUse",
            4_000,
            json!({ "toolName": "create", "toolArgs": args, "toolResult": result }),
        ),
        line("agentStop", 5_000, json!({ "stopReason": "end_turn" })),
    ]
}

/// A Claude Code session whose one turn makes the call `tool` with `input`.
fn claude_edit(tool: &str, input: Value) -> Vec<String> {
    let mut lines = vec![line(
        "UserPromptSubmit",
        0,
        json!({ "prompt": "Fix the parser" }),
    )];
    lines.extend(call(1_000, "e1", tool, input, None));
    lines
}

/// What edits a file is the kind of call each agent's adapter records, not a list of tool names
/// (#723): every agent's edit is a step that edited the file, and so it is in a saved state written
/// before a row recorded its kind.
#[test]
fn a_files_facts_list_every_agents_edit_as_a_step_that_edited_it() {
    let agy_brain =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    let agy =
        include_str!("fixtures/agy/run.jsonl").replace("@BRAIN@", &agy_brain.display().to_string());
    let fixture = |text: &str| text.lines().map(String::from).collect::<Vec<_>>();
    let notebook = json!({ "notebook_path": "/w/repo/src/parse.ipynb", "new_source": "x" });
    let cases = [
        (
            "qwen write_file",
            fixture(include_str!("fixtures/qwen/run.jsonl")),
            "5c0e1f7a-2b3d-4e8f-9a10-b2c3d4e5f601",
            ("/Users/me/work/hello", "hello.txt"),
            "write_file by the session's agent",
        ),
        (
            "copilot create",
            copilot_create(),
            "cs-1",
            ("/w/hello", "hello.txt"),
            "create by the session's agent",
        ),
        (
            "agy write_file",
            fixture(&agy),
            "145c3c4a-3fac-4c6c-8ffc-b1bd95c0f78a",
            ("/tmp/agy-fixture", "hello.txt"),
            "write_file by the session's agent",
        ),
        (
            "claude MultiEdit",
            claude_edit("MultiEdit", json!({ "file_path": "/w/repo/src/parse.rs" })),
            SESSION,
            (CWD, "src/parse.rs"),
            "MultiEdit by the session's agent",
        ),
        (
            "claude NotebookEdit",
            claude_edit("NotebookEdit", notebook),
            SESSION,
            (CWD, "src/parse.ipynb"),
            "NotebookEdit by the session's agent",
        ),
    ];
    for (case, lines, session, (root, path), step) in cases {
        let (log, _dir) = logged(&lines);
        let anchor = Anchor::File {
            turn: 0,
            root: root.into(),
            path: path.into(),
            from: None,
        };
        let log_len = std::fs::metadata(&log).unwrap().len();
        for (studio, how) in [
            (Studio::replayed(&lines), "replayed"),
            (restored_without_kinds(&lines, log_len), "restored"),
        ] {
            let facts = studio.ask_facts(session, &anchor, &log).unwrap();
            let edited = facts
                .sections
                .iter()
                .find(|s| s.label == "Steps that edited it");
            let steps: Option<Vec<&str>> =
                edited.map(|s| s.facts.iter().map(|f| f.text.as_str()).collect());
            assert_eq!(
                steps,
                Some(vec![step]),
                "{case}, {how}: {:?}",
                labels(&facts)
            );
        }
    }
}
