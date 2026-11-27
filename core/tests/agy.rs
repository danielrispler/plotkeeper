//! Seams 2 and 3 of danielrispler/plotkeeper#43 for the agy adapter: the fixture recorded from real
//! agy (`record-agy-run.sh`) replayed through the core, and hand-written cases for its rules.

use crate::common;
use serde_json::{Value, json};
use std::path::PathBuf;
use studio_core::{AgyDriftKind, State, Studio};

const RUN: &str = include_str!("fixtures/agy/run.jsonl");
const PARENT: &str = "145c3c4a-3fac-4c6c-8ffc-b1bd95c0f78a";
const CHILD: &str = "c267ee38-db35-4a6f-a334-e63925cb0847";

fn brain() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    dir.display().to_string()
}

fn fixture() -> Vec<String> {
    RUN.replace("@BRAIN@", &brain())
        .lines()
        .map(Into::into)
        .collect()
}

#[test]
fn the_recorded_run_replays_to_its_turns_steps_and_a_subagent_of_its_own() {
    let studio = Studio::replayed(fixture());

    let session = studio.session(PARENT).expect("the agy session");
    let senders: Vec<bool> = session
        .prompts
        .iter()
        .map(|p| p.sender.is_daniels())
        .collect();
    // Daniel's three prompts, and the wake-up the subagent's report caused.
    assert_eq!(senders, [true, true, true, false]);
    assert_eq!(session.turns.len(), 4);
    assert_eq!(session.model.as_deref(), Some("gemini-3.8-flash-high"));
    assert_eq!(session.state, State::YourTurn);
    // agy's payloads name no parent, so its subagent's conversation is a session of its own.
    assert!(session.agents.is_empty());
    let child = studio.session(CHILD).expect("the subagent's own row");
    assert_eq!(child.prompts.len(), 1);
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn the_recorded_run_has_an_edit_and_a_check_as_steps() {
    let lines = fixture();
    let studio = Studio::replayed(&lines[..9]);

    let session = studio.session(PARENT).unwrap();
    let step = session.agent.last_step.as_ref().unwrap();
    assert_eq!(step.tool, "run_command");
    assert_eq!(step.command.as_deref(), Some("cat hello.txt"));
}

#[test]
fn the_intention_comes_from_the_transcript() {
    let mut studio = Studio::replayed(fixture());
    studio.follow_transcripts(|_| {});

    let session = studio.session(PARENT).unwrap();
    assert!(session.transcript.unavailable.is_none());
    assert!(session.transcript.intention.is_some());
}

#[test]
fn the_adapter_declares_what_it_reports_and_emits_no_claude_only_variant() {
    let studio = Studio::replayed(fixture());
    let session = studio.session(PARENT).unwrap();
    assert_eq!(session.capabilities, studio_core::AGY_CAPABILITIES);
    assert!(!session.capabilities.asks);
    assert!(!session.capabilities.background_tasks);
    // No permission block, question or background task id ever shows.
    assert!(session.questions.is_empty());
    assert_ne!(session.state, State::NeedsYou);
}

fn line(session: &str, event: &str, at: u64, fields: Value) -> String {
    let base = json!({
        "conversationId": session,
        "hookEventName": event,
        "cwd": "/tmp/agy-fixture",
        "modelName": "m",
        "studio_received_ms": at,
        "transcriptPath": format!("{}/{session}/.system_generated/logs/transcript_full.jsonl", brain()),
    });
    common::with(base, fields)
}

fn tool(session: &str, event: &str, at: u64, step: u64) -> String {
    line(
        session,
        event,
        at,
        json!({ "stepIdx": step, "toolCall": { "name": "run_command", "args": { "CommandLine": "ls", "toolSummary": "List" } } }),
    )
}

#[test]
fn a_swapped_tool_pair_leaves_no_running_step() {
    let studio = Studio::replayed([
        line("s", "SessionStart", 1, json!({})),
        line(
            "s",
            "PreInvocation",
            2,
            json!({ "invocationNum": 0, "initialNumSteps": 1, "lastUserInput": "go" }),
        ),
        tool("s", "PostToolUse", 3, 2),
        tool("s", "PreToolUse", 4, 2),
    ]);

    let session = studio.session("s").unwrap();
    assert_eq!(
        session.agent.last_step, None,
        "the late start opened a step"
    );
}

#[test]
fn a_turn_origin_that_disagrees_at_the_stop_counts_as_drift() {
    // The transcript says step 0 is Daniel's prompt, the hook's wake-up guess is not asked: the
    // turn starts at step 3, which holds no `USER_INPUT`, then the check at its stop agrees; so
    // the disagreement is built by a turn that starts at a step the file lacks until later.
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("s/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let file = logs.join("transcript_full.jsonl");
    let path = file.display().to_string();
    let at = |event: &str, t: u64, fields: Value| {
        let mut l: Value = serde_json::from_str(&line("s", event, t, fields)).unwrap();
        l["transcriptPath"] = path.clone().into();
        l.to_string()
    };
    let mut studio = Studio::default();
    studio.replay(&at("SessionStart", 1, json!({})));
    // Not on disk yet when the turn starts, so it reads as a wake-up.
    std::fs::write(&file, "{\"step_index\":0,\"type\":\"GENERIC\"}\n").unwrap();
    studio.replay(&at(
        "PreInvocation",
        2,
        json!({ "invocationNum": 0, "initialNumSteps": 2, "lastUserInput": "x" }),
    ));
    std::fs::write(
        &file,
        "{\"step_index\":0,\"type\":\"GENERIC\"}\n{\"step_index\":1,\"type\":\"USER_INPUT\"}\n",
    )
    .unwrap();
    studio.replay(&at("Stop", 3, json!({ "fullyIdle": true })));

    let kinds: Vec<_> = studio.agy_drift().iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [AgyDriftKind::TurnOrigin]);
}

#[test]
fn an_unknown_shape_counts_as_drift() {
    let mut studio = Studio::default();
    studio.replay(&line("s", "SessionStart", 1, json!({})));
    studio.replay(&line("s", "Teleport", 2, json!({})));
    studio.replay(&line("s", "PreToolUse", 3, json!({})));

    let kinds: Vec<_> = studio.agy_drift().iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [AgyDriftKind::HookEvent, AgyDriftKind::ToolShape]);
}

#[test]
fn an_unknown_transcript_record_counts_as_drift() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("s/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let file = logs.join("transcript_full.jsonl");
    std::fs::write(
        &file,
        "{\"step_index\":0,\"type\":\"USER_INPUT\"}\n{\"step_index\":1,\"type\":\"HOLOGRAM\"}\n",
    )
    .unwrap();
    let path = file.display().to_string();
    let mut l: Value =
        serde_json::from_str(&line("s", "Stop", 2, json!({ "fullyIdle": true }))).unwrap();
    l["transcriptPath"] = path.into();
    let mut studio = Studio::default();
    studio.replay(&l.to_string());

    let kinds: Vec<_> = studio.agy_drift().iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [AgyDriftKind::Record]);
}

#[test]
fn a_session_names_its_agent_for_its_mark() {
    assert_eq!(studio_core::AGY_CAPABILITIES.agent, "agy");
    assert_eq!(studio_core::CLAUDE_CAPABILITIES.agent, "claude");
    let json = serde_json::to_value(Studio::replayed(fixture()).session(PARENT).unwrap()).unwrap();
    assert_eq!(json["capabilities"]["agent"], "agy");
    assert_eq!(json["capabilities"]["asks"], false);
}

#[test]
fn an_agy_sessions_first_stop_asks_for_agys_version_once() {
    let mut studio = Studio::default();
    let at = |n| {
        line(
            "s",
            "PreInvocation",
            n,
            json!({ "invocationNum": 0, "initialNumSteps": 1, "lastUserInput": "go" }),
        )
    };
    let asked = |outputs: Vec<studio_core::Output>| {
        outputs
            .iter()
            .filter(|o| matches!(o, studio_core::Output::AgyVersion))
            .count()
    };
    studio.apply(&line("s", "SessionStart", 1, json!({})));
    studio.apply(&at(2));
    let first = studio.apply(&line("s", "Stop", 3, json!({})));
    studio.apply(&at(4));
    let second = studio.apply(&line("s", "Stop", 5, json!({})));
    assert_eq!((asked(first), asked(second)), (1, 0));
}

#[test]
fn a_json_line_with_no_conversation_or_event_name_counts_as_drift() {
    let mut studio = Studio::default();
    studio.replay(&json!({ "studio_received_ms": 5, "hookEventName": "Stop" }).to_string());
    studio.replay(&json!({ "studio_received_ms": 6, "conversationId": "s" }).to_string());

    let seen: Vec<_> = studio.agy_drift().iter().map(|d| (d.at, d.kind)).collect();
    assert_eq!(
        seen,
        [(5, AgyDriftKind::HookEvent), (6, AgyDriftKind::HookEvent)]
    );
}

#[test]
fn a_command_of_an_agy_session_that_runs_a_list_entry_is_a_check() {
    let call = |event: &str, at: u64| {
        line(
            "s",
            event,
            at,
            json!({ "stepIdx": 2, "toolCall": { "name": "run_command", "args": { "CommandLine": "cargo test", "toolSummary": "Test" } } }),
        )
    };
    let studio = Studio::replayed([
        line("s", "SessionStart", 1, json!({})),
        line(
            "s",
            "PreInvocation",
            2,
            json!({ "invocationNum": 0, "initialNumSteps": 1, "lastUserInput": "go" }),
        ),
        call("PreToolUse", 3),
        call("PostToolUse", 4),
    ]);

    let checks = &studio.session("s").unwrap().turns[0].checks;

    let found: Vec<&str> = checks.iter().map(|c| c.suite.as_str()).collect();
    assert_eq!(found, ["cargo test"]);
}

/// A line the server writes itself into the event log for session `id`, shaped as Claude Code's
/// lines are: the process check's `SessionEnd` (#330) or an interrupted turn's line (#352).
fn server_line(id: &str, name: &str, at: u64, fields: Value) -> String {
    let mut line = json!({ "session_id": id, "hook_event_name": name, "studio_received_ms": at });
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
}

fn ended_by_process_check(id: &str, at: u64) -> String {
    server_line(id, "SessionEnd", at, json!({ "reason": "process_gone" }))
}

fn interrupted(id: &str, at: u64) -> String {
    server_line(id, "StudioInterrupted", at, json!({ "prompt_id": "p-1" }))
}

/// The session keeps agy's capabilities, so its transcript, read once selected (an ended session's
/// is read only then), is still read as agy's.
fn assert_still_agy(studio: &mut Studio) {
    let session = studio.session(PARENT).unwrap();
    assert_eq!(session.capabilities, studio_core::AGY_CAPABILITIES);
    studio.select(PARENT);
    studio.follow_transcripts(|_| {});
    let session = studio.session(PARENT).unwrap();
    assert!(
        session.transcript.unavailable.is_none(),
        "{:?}",
        session.transcript.unavailable
    );
    assert!(session.transcript.intention.is_some());
}

#[test]
fn an_agy_session_ended_by_the_process_check_stays_agy_live_and_after_a_restart() {
    let mut lines = fixture();
    let end = ended_by_process_check(PARENT, 1_794_738_300_000);

    let mut studio = Studio::replayed(&lines);
    studio.apply(&end);
    assert_eq!(studio.session(PARENT).unwrap().state, State::Ended);
    assert_still_agy(&mut studio);

    lines.push(end);
    let mut restarted = Studio::replayed(&lines);
    assert_eq!(restarted.session(PARENT).unwrap().state, State::Ended);
    assert_still_agy(&mut restarted);
}

#[test]
fn an_interrupted_agy_turn_stays_agy_live_and_after_a_restart() {
    let mut lines = fixture();
    let line = interrupted(PARENT, 1_794_738_300_000);

    let mut studio = Studio::replayed(&lines);
    studio.apply(&line);
    assert_still_agy(&mut studio);

    lines.push(line);
    let mut restarted = Studio::replayed(&lines);
    assert_still_agy(&mut restarted);
}

#[test]
fn a_claude_session_ended_or_interrupted_by_the_server_stays_claude() {
    let claude = |name: &str, at: u64, extra: Value| common::event("c1", name, "/repo", at, extra);
    let lines = [
        claude("SessionStart", 1, json!({ "source": "startup" })),
        claude(
            "UserPromptSubmit",
            2,
            json!({ "prompt": "go", "prompt_id": "p-1" }),
        ),
        interrupted("c1", 3),
        ended_by_process_check("c1", 4),
    ];
    let studio = Studio::replayed(lines.iter().map(String::as_str));
    let session = studio.session("c1").unwrap();
    assert_eq!(session.state, State::Ended);
    assert_eq!(session.capabilities, studio_core::CLAUDE_CAPABILITIES);
}

/// The outputs of one agy turn whose transcript holds Daniel's prompt at step 0 and `reply` as the
/// final response at step 1, as the trial of #358 recorded them.
fn turn_outputs(reply: &str) -> Vec<studio_core::Output> {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("s/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let file = logs.join("transcript_full.jsonl");
    let records = [
        json!({ "step_index": 0, "type": "USER_INPUT", "content": "explain" }),
        json!({ "step_index": 1, "type": "PLANNER_RESPONSE", "content": reply }),
    ];
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&file, text).unwrap();
    let at = |event: &str, t: u64, fields: Value| {
        let mut l: Value = serde_json::from_str(&line("s", event, t, fields)).unwrap();
        l["transcriptPath"] = file.display().to_string().into();
        l.to_string()
    };
    let mut studio = Studio::default();
    studio.apply(&at("SessionStart", 1, json!({})));
    studio.apply(&at(
        "PreInvocation",
        2,
        json!({ "invocationNum": 0, "initialNumSteps": 1, "lastUserInput": "explain" }),
    ));
    studio.apply(&at("Stop", 3, json!({ "fullyIdle": true })))
}

#[test]
fn an_agy_turn_with_a_long_final_response_asks_for_its_summary() {
    let reply = "word ".repeat(200);
    let summaries: Vec<_> = turn_outputs(&reply)
        .into_iter()
        .filter_map(|o| match o {
            studio_core::Output::Summary(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(summaries.len(), 1, "one summary request");
    assert!(summaries[0].wanted);
    assert_eq!(
        summaries[0].responses.last().map(|r| r.trim()),
        Some(reply.trim())
    );
}

#[test]
fn an_agy_turn_with_a_short_final_response_asks_for_no_summary() {
    let outputs = turn_outputs("Hi! The folder is empty.");
    assert!(
        !outputs
            .iter()
            .any(|o| matches!(o, studio_core::Output::Summary(_)))
    );
}

#[test]
fn a_replayed_agy_run_gives_each_turn_its_own_final_response() {
    // A replay reads the final transcript, which holds the later turns too.
    let studio = Studio::replayed(fixture());
    let session = studio.session(PARENT).unwrap();
    let firsts: Vec<String> = session
        .turns
        .iter()
        .map(|t| {
            t.reply
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    assert_eq!(
        firsts,
        ["I have created", "ok", "I have started", "The subagent has"]
    );
}

/// The final response each turn of a replayed session gets, for a transcript of `records` and turns
/// starting at `starts` (their `initialNumSteps`).
fn replayed_replies(records: &[Value], starts: &[u64]) -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("s/.system_generated/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let file = logs.join("transcript_full.jsonl");
    let text: String = records.iter().map(|r| format!("{r}\n")).collect();
    std::fs::write(&file, text).unwrap();
    let at = |event: &str, t: u64, fields: Value| {
        let mut l: Value = serde_json::from_str(&line("s", event, t, fields)).unwrap();
        l["transcriptPath"] = file.display().to_string().into();
        l.to_string()
    };
    let mut lines = vec![at("SessionStart", 1, json!({}))];
    for (n, start) in starts.iter().enumerate() {
        let t = 10 * (n as u64 + 1);
        lines.push(at(
            "PreInvocation",
            t,
            json!({ "invocationNum": 0, "initialNumSteps": start, "lastUserInput": format!("p{n}") }),
        ));
        lines.push(at("Stop", t + 1, json!({ "fullyIdle": true })));
    }
    let studio = Studio::replayed(lines.iter().map(String::as_str));
    let session = studio.session("s").unwrap();
    session.turns.iter().map(|t| t.reply.clone()).collect()
}

#[test]
fn a_replayed_turn_that_ends_on_a_tool_result_takes_no_later_reply() {
    let records = [
        json!({ "step_index": 0, "type": "USER_INPUT" }),
        json!({ "step_index": 1, "type": "PLANNER_RESPONSE", "content": "", "tool_calls": [{}] }),
        json!({ "step_index": 2, "type": "GENERIC" }),
        json!({ "step_index": 3, "type": "USER_INPUT" }),
        json!({ "step_index": 4, "type": "PLANNER_RESPONSE", "content": "second" }),
    ];
    assert_eq!(replayed_replies(&records, &[1, 4]), ["", "second"]);
}

#[test]
fn a_system_message_while_a_tool_runs_does_not_end_the_turn() {
    let records = [
        json!({ "step_index": 0, "type": "USER_INPUT" }),
        json!({ "step_index": 1, "type": "PLANNER_RESPONSE", "content": "Starting it now.", "tool_calls": [{}] }),
        json!({ "step_index": 2, "type": "SYSTEM_MESSAGE" }),
        json!({ "step_index": 3, "type": "PLANNER_RESPONSE", "content": "All done." }),
    ];
    assert_eq!(replayed_replies(&records, &[1]), ["All done."]);
}
