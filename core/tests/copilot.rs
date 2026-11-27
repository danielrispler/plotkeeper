//! The Copilot adapter (#361; spec #320, Harnesses): Copilot's hook payloads, in both casings,
//! replayed into sessions, turns and tools.
//!
//! `fixtures/copilot/run.jsonl` is built from Copilot's hooks reference
//! (https://docs.github.com/en/copilot/reference/hooks-configuration, read 2026-11-23), not from a
//! real run. Session `cs-7a1f` is camelCase, as Copilot sends it to the camelCase entries install
//! wires: `sessionId`, `toolName`, `toolArgs` as a JSON string (the "use hooks" page's example), and no
//! event name, which the entry passes as `--event` and the hook stores as `studio_event`. Session
//! `vs-3c9e` is snake_case, as Copilot sends it to PascalCase entries, with `hook_event_name` and
//! Claude-style tool names, and no `studio_pid`, as for a VS Code-hosted session whose pid the hook
//! told as the IDE's. The tool names `edit` and `Read`, the edit's arguments, the ids and the
//! transcript paths are made up: the docs give no example of them.

use serde_json::json;
use studio_core::{COPILOT_CAPABILITIES, Output, State, Studio};

const RUN: &str = include_str!("fixtures/copilot/run.jsonl");
const CAMEL: &str = "cs-7a1f";
const SNAKE: &str = "vs-3c9e";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

/// The tool, turn and file or command of each call of session `id`, in order.
fn calls(studio: &Studio, id: &str) -> Vec<(String, Option<usize>, Option<String>)> {
    let session = studio.session(id).unwrap();
    let rows = session.ledger.rows().iter();
    rows.map(|r| (r.tool.clone(), r.turn, r.file.clone().or(r.command.clone())))
        .collect()
}

#[test]
fn the_documented_run_replays_into_its_sessions_turns_and_tools_in_both_casings() {
    let studio = Studio::replayed(fixture());

    let camel = studio
        .session(CAMEL)
        .expect("the camelCase Copilot session");
    assert_eq!(camel.capabilities, COPILOT_CAPABILITIES);
    assert_eq!(camel.capabilities.agent, "copilot");
    assert_eq!(camel.cwd, "/Users/me/work/hello");
    assert_eq!(camel.pid, Some(5151));
    assert_eq!(
        calls(&studio, CAMEL),
        [
            ("bash".into(), Some(0), Some("cat hello.txt".into())),
            (
                "edit".into(),
                Some(0),
                Some("/Users/me/work/hello/hello.txt".into())
            ),
            ("bash".into(), Some(1), Some("npm test".into())),
        ]
    );
    let rows = camel.ledger.rows();
    assert!(rows.iter().all(|r| r.end.is_some()), "every call ended");

    let snake = studio
        .session(SNAKE)
        .expect("the snake_case Copilot session");
    assert_eq!(snake.capabilities, COPILOT_CAPABILITIES);
    assert_eq!(snake.pid, None);
    assert_eq!(snake.turns.len(), 1);
    assert_eq!(snake.turns[0].prompt, "What does this repo do?");
    assert!(snake.turns[0].ended_at.is_some());
    assert_eq!(snake.state, State::YourTurn);
    assert_eq!(
        calls(&studio, SNAKE),
        [(
            "Read".into(),
            Some(0),
            Some("/Users/me/work/other/README.md".into())
        )]
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_running_tool_keeps_its_turn_running_until_its_call_ends() {
    let lines = fixture();
    let state_after = |n: usize| Studio::replayed(&lines[..n]).session(CAMEL).unwrap().state;
    // The first `preToolUse`, then the camelCase session's first `agentStop`.
    assert_eq!(state_after(3), State::Running);
    assert_eq!(state_after(10), State::YourTurn);
}

/// Copilot's `agentStop` carries no reply, so each turn's is read from its transcript: the last
/// `assistant.message` with text before the stop. Copilot CLI's transcript logs the `agentStop`
/// hook's input in a `hook.start`, found by its numeric `timestamp` (records as Copilot CLI 1.0.114
/// wrote them in wave 1's trial, #369). VS Code's agent logs no hook records and sends an ISO
/// `timestamp`, so the stop is placed among the records by their own (as VS Code 1.139.1 wrote them
/// in #423's trial). A live long reply asks for its summary, and a replay, whose transcript holds
/// the later turns too, gives each turn its own.
#[test]
fn each_copilot_turn_takes_its_reply_from_the_transcript_and_a_long_one_asks_for_its_summary() {
    let long = format!("The add function subtracted. {}", "word ".repeat(200));
    let iso = |ms: u64| format!("2026-11-27T13:27:{:02}.{:03}Z", ms / 1000, ms % 1000);
    for vs_code in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("events.jsonl");
        let at = |ms: u64| json!(if vs_code { json!(iso(ms)) } else { json!(ms) });
        let record = |kind: &str, ms: u64, data: serde_json::Value| json!({ "type": kind, "data": data, "timestamp": iso(ms) });
        let message = |ms, content: &str| {
            record(
                "assistant.message",
                ms,
                json!({ "content": content, "phase": "final_answer" }),
            )
        };
        let stop_hook = |ms: u64| {
            let input = json!({ "sessionId": "s", "timestamp": ms });
            record(
                "hook.start",
                ms,
                json!({ "hookType": "agentStop", "input": input }),
            )
        };
        let user = |ms| record("user.message", ms, json!({ "content": "go" }));
        let mut records = vec![
            user(2),
            message(300, "I'll run calc.py first."),
            message(400, ""),
            message(998, "Fixed: 2+3 is 5."),
            stop_hook(1000),
            user(1500),
            message(1998, &long),
            stop_hook(2000),
        ];
        if vs_code {
            records.retain(|r| r["type"] != "hook.start");
        }
        let text: String = records.iter().map(|r| format!("{r}\n")).collect();
        std::fs::write(&path, text).unwrap();
        let line = |event: &str, ms: u64, extra: serde_json::Value| {
            let mut l = json!({
                "cwd": "/w/calc", "timestamp": at(ms), "studio_event": event,
                "studio_agent": "copilot", "studio_received_ms": ms,
            });
            let more = if vs_code {
                json!({ "session_id": "s", "transcript_path": path.display().to_string() })
            } else {
                json!({ "sessionId": "s", "studio_pid": 84386, "transcriptPath": path.display().to_string() })
            };
            let l2 = l.as_object_mut().unwrap();
            l2.extend(more.as_object().unwrap().clone());
            l2.extend(extra.as_object().unwrap().clone());
            l.to_string()
        };
        let lines = [
            line("sessionStart", 1, json!({ "source": "new" })),
            line("userPromptSubmitted", 2, json!({ "prompt": "fix calc" })),
            line("agentStop", 1000, json!({ "stopReason": "end_turn" })),
            line("userPromptSubmitted", 1500, json!({ "prompt": "explain" })),
            line("agentStop", 2000, json!({ "stopReason": "end_turn" })),
        ];

        let mut studio = Studio::default();
        let outputs: Vec<_> = lines.iter().flat_map(|l| studio.apply(l)).collect();
        let summaries: Vec<_> = outputs
            .iter()
            .filter_map(|o| match o {
                Output::Summary(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(
            summaries.len(),
            1,
            "only the long reply asks (VS Code: {vs_code})"
        );
        assert!(summaries[0].wanted);
        assert_eq!(
            summaries[0].responses.last().map(|r| r.trim()),
            Some(long.trim())
        );

        let replayed = Studio::replayed(lines.iter().map(String::as_str));
        for studio in [&studio, &replayed] {
            let replies: Vec<_> = studio
                .session("s")
                .unwrap()
                .turns
                .iter()
                .map(|t| t.reply.clone())
                .collect();
            assert_eq!(
                replies,
                ["Fixed: 2+3 is 5.", long.trim()],
                "VS Code: {vs_code}"
            );
        }
    }
}

/// Each edit and run tool seen in #423's trial records its file or command, so a turn's summary
/// has its changes and runs: Copilot CLI 1.0.114's `bash` and `apply_patch` (camelCase, the patch
/// naming its file relative to the session's folder), and VS Code 1.139.1's `run_in_terminal` and
/// `insert_edit_into_file` (snake_case, the file in `filePath`). A read is neither.
#[test]
fn each_copilot_edit_and_run_tool_records_its_file_or_command() {
    let patch = "*** Begin Patch\n*** Update File: calc.py\n@@\n-x\n+y\n*** End Patch\n";
    let cases = [
        (
            "cli bash",
            false,
            "bash",
            json!({ "command": "python3 calc.py" }),
            Some("python3 calc.py"),
        ),
        (
            "cli apply_patch",
            false,
            "apply_patch",
            json!(patch),
            Some("/w/calc/calc.py"),
        ),
        (
            "vs code run_in_terminal",
            true,
            "run_in_terminal",
            json!({ "command": "python3 calc.py", "mode": "sync" }),
            Some("python3 calc.py"),
        ),
        (
            "vs code insert_edit_into_file",
            true,
            "insert_edit_into_file",
            json!({ "filePath": "/w/calc/calc.py", "code": "def median(xs): ..." }),
            Some("/w/calc/calc.py"),
        ),
        (
            "vs code read_file",
            true,
            "read_file",
            json!({ "filePath": "/w/calc/calc.py" }),
            None,
        ),
    ];
    for (case, vs_code, tool, args, expected) in cases {
        let line = |event: &str, pascal: &str, ms: u64| {
            if vs_code {
                json!({
                    "hook_event_name": pascal, "session_id": "s", "cwd": "/w/calc",
                    "timestamp": "2026-11-27T13:27:43.294Z", "tool_name": tool, "tool_input": args,
                    "tool_use_id": "call_1__vscode-1", "studio_event": event,
                    "studio_agent": "copilot", "studio_received_ms": ms,
                })
            } else {
                json!({
                    "sessionId": "s", "cwd": "/w/calc", "timestamp": ms, "toolName": tool,
                    "toolArgs": args, "studio_event": event, "studio_agent": "copilot",
                    "studio_pid": 53959, "studio_received_ms": ms,
                })
            }
            .to_string()
        };
        let prompt = json!({
            "sessionId": "s", "cwd": "/w/calc", "timestamp": 1, "prompt": "go",
            "studio_event": "userPromptSubmitted", "studio_agent": "copilot", "studio_received_ms": 1,
        })
        .to_string();
        let lines = [
            prompt,
            line("preToolUse", "PreToolUse", 2),
            line("postToolUse", "PostToolUse", 3),
        ];
        let studio = Studio::replayed(lines.iter().map(String::as_str));
        let rows = studio.session("s").unwrap().ledger.rows();
        let got = rows[0].file.clone().or(rows[0].command.clone());
        assert_eq!(got.as_deref(), expected, "{case}");
    }
}
