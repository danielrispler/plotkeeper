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
    let prompts: Vec<_> = camel.turns.iter().map(|t| t.prompt.as_str()).collect();
    assert_eq!(
        prompts,
        ["Add a greeting to hello.txt", "Now run the tests"]
    );
    assert!(camel.turns.iter().all(|t| t.ended_at.is_some()));
    assert_eq!(camel.state, State::Ended);
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

#[test]
fn a_copilot_session_records_no_turn_trees() {
    let studio = Studio::replayed(fixture());
    let session = studio.session(CAMEL).unwrap();
    assert!(!session.capabilities.turn_trees);
    assert!(session.turns.iter().all(|t| t.files.is_none()));
}

/// A session whose reported pid was the IDE's has none: the process check never ends it, and the
/// quiet period marks it "idle?" (#320, "IDE-hosted pids").
#[test]
fn a_session_with_the_ides_pid_goes_idle_and_is_never_ended_by_the_process_check() {
    let lines = fixture();
    // The snake_case session up to its tool's start: in a turn.
    let at = lines
        .iter()
        .position(|l| l.contains("\"PreToolUse\""))
        .unwrap();
    let mut studio = Studio::replayed(&lines[..=at]);
    assert_eq!(studio.session(SNAKE).unwrap().state, State::Running);
    assert!(!studio.gone(|_| false).contains(&SNAKE.to_owned()));
    studio.tick(1_794_700_005_500 + 2 * 60 * 60 * 1000);
    assert_eq!(studio.session(SNAKE).unwrap().state, State::Idle);
    assert!(!studio.gone(|_| false).contains(&SNAKE.to_owned()));
}

#[test]
fn a_copilot_session_ended_by_the_process_check_or_interrupted_stays_copilot() {
    // The camelCase session up to its second turn's tool, before its own stop and end, then the server's lines for
    // an interrupt and for its gone process.
    let lines: Vec<String> = fixture()[..14].iter().map(ToString::to_string).collect();
    let synthetic = [
        json!({ "session_id": CAMEL, "hook_event_name": "StudioInterrupted", "prompt_id": "p_2", "studio_received_ms": 1_794_700_011_000_u64 }).to_string(),
        json!({ "session_id": CAMEL, "hook_event_name": "SessionEnd", "reason": "process_gone", "studio_received_ms": 1_794_700_012_000_u64 }).to_string(),
    ];
    let mut live = Studio::replayed(&lines);
    for line in &synthetic {
        live.apply(line);
    }
    let mut all = lines.clone();
    all.extend(synthetic);
    let restarted = Studio::replayed(&all);
    for studio in [&live, &restarted] {
        let session = studio.session(CAMEL).unwrap();
        assert_eq!(session.state, State::Ended);
        assert_eq!(session.capabilities, COPILOT_CAPABILITIES);
    }
}

/// Copilot's `agentStop` carries no reply, so each turn's is read from its transcript: the last
/// `assistant.message` with text before the `hook.start` that logged this `agentStop`'s input, found
/// by its `timestamp` (records as Copilot CLI 1.0.114 wrote them in wave 1's trial, #369). A live
/// long reply asks for its summary, and a replay, whose transcript holds the later turns too, gives
/// each turn its own.
#[test]
fn each_copilot_turn_takes_its_reply_from_the_transcript_and_a_long_one_asks_for_its_summary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let long = format!("The add function subtracted. {}", "word ".repeat(200));
    let message = |content: &str| json!({ "type": "assistant.message", "data": { "content": content, "phase": "final_answer" } });
    let stop_hook = |ts: u64| json!({ "type": "hook.start", "data": { "hookType": "agentStop", "input": { "sessionId": "s", "timestamp": ts } } });
    let user = json!({ "type": "user.message", "data": { "content": "go" } });
    let records = [
        user.clone(),
        message("I'll run calc.py first."),
        message(""),
        message("Fixed: 2+3 is 5."),
        stop_hook(1000),
        user,
        message(&long),
        stop_hook(2000),
    ];
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&path, text).unwrap();
    let line = |event: &str, ts: u64, extra: serde_json::Value| {
        let mut l = json!({
            "sessionId": "s", "cwd": "/w/calc", "timestamp": ts, "studio_event": event,
            "studio_agent": "copilot", "studio_pid": 84386, "studio_received_ms": ts,
        });
        l.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        l.to_string()
    };
    let stop = |ts| {
        line(
            "agentStop",
            ts,
            json!({ "transcriptPath": path.display().to_string(), "stopReason": "end_turn" }),
        )
    };
    let lines = [
        line("sessionStart", 1, json!({ "source": "new" })),
        line("userPromptSubmitted", 2, json!({ "prompt": "fix calc" })),
        stop(1000),
        line("userPromptSubmitted", 1500, json!({ "prompt": "explain" })),
        stop(2000),
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
    assert_eq!(summaries.len(), 1, "only the long reply asks");
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
        assert_eq!(replies, ["Fixed: 2+3 is 5.", long.trim()]);
    }
}
