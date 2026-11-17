//! Seams 2 and 3 of danielrispler/plotkeeper#43 for the agy adapter: the fixture recorded from real
//! agy (`record-agy-run.sh`) replayed through the core, and hand-written cases for its rules.

use serde_json::{Value, json};
use std::path::PathBuf;
use studio_core::{AgyDriftKind, LogEntry, Parent, Sender, State, Status, Studio};

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
fn the_recorded_run_replays_to_its_turns_steps_and_subagent() {
    let studio = Studio::replayed(fixture());

    let session = studio.session(PARENT).expect("the agy session");
    let senders: Vec<bool> = session
        .prompts
        .iter()
        .map(|p| p.sender.is_daniels())
        .collect();
    // Daniel's three prompts, and the wake-up the subagent's report caused.
    assert_eq!(senders, [true, true, true, false]);
    assert!(matches!(session.prompts[3].sender, Sender::WakeUp(ref id) if id == CHILD));
    assert_eq!(session.turns.len(), 4);
    assert_eq!(session.model.as_deref(), Some("gemini-3.8-flash-high"));
    assert_eq!(session.state, State::YourTurn);

    assert_eq!(session.agents.len(), 1);
    let child = &session.agents[0];
    assert_eq!(child.agent.id, CHILD);
    assert_eq!(child.parent, Parent::Session);
    assert_eq!(child.name, "Command Runner");
    assert_eq!(child.status, Status::Completed);
    assert!(child.ended_at.is_some());
    assert!(child.prompt.contains("sleep 8"));
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
    let mut line = json!({
        "conversationId": session,
        "hookEventName": event,
        "cwd": "/tmp/agy-fixture",
        "modelName": "m",
        "studio_received_ms": at,
        "studio_terminal_id": "t-1",
        "transcriptPath": format!("{}/{session}/.system_generated/logs/transcript_full.jsonl", brain()),
    });
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
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
fn a_subagent_file_that_appears_late_still_attaches_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let generated = dir.path().join("p/.system_generated");
    std::fs::create_dir_all(generated.join("logs")).unwrap();
    let path = |s: &str| {
        format!(
            "{}/{s}/.system_generated/logs/transcript_full.jsonl",
            dir.path().display()
        )
    };
    let with = |session: &str, event: &str, at: u64, fields: Value| {
        let mut l: Value = serde_json::from_str(&line(session, event, at, fields)).unwrap();
        l["transcriptPath"] = path(session).into();
        l.to_string()
    };
    let mut studio = Studio::default();
    studio.replay(&with("p", "SessionStart", 1, json!({})));
    studio.replay(&with("c", "SessionStart", 2, json!({})));
    studio.replay(&with(
        "c",
        "PreInvocation",
        3,
        json!({ "invocationNum": 0, "initialNumSteps": 0 }),
    ));
    assert_eq!(
        studio.session("p").unwrap().agents[0].parent,
        Parent::Unattached
    );

    std::fs::create_dir_all(generated.join("subagents")).unwrap();
    std::fs::write(
        generated.join("subagents/c.json"),
        json!({ "subagentDescriptor": { "typeName": "self", "role": "Runner" }, "state": "ALIVE", "spawnStepIndex": 5 }).to_string(),
    )
    .unwrap();
    studio.replay(&with(
        "c",
        "PreToolUse",
        4,
        json!({ "stepIdx": 2, "toolCall": { "name": "run_command", "args": {} } }),
    ));

    let child = &studio.session("p").unwrap().agents[0];
    assert_eq!(child.parent, Parent::Session);
    assert_eq!(child.name, "Runner");
}

#[test]
fn a_missing_subagent_file_sends_the_child_to_unattached_at_its_stop() {
    let mut studio = Studio::default();
    for l in [
        line("p", "SessionStart", 1, json!({})),
        line("c", "SessionStart", 2, json!({})),
        line("c", "Stop", 3, json!({ "fullyIdle": true })),
    ] {
        studio.replay(&l);
    }
    let session = studio.session("p").unwrap();
    assert_eq!(session.agents[0].parent, Parent::Unattached);
    assert_eq!(session.agents[0].status, Status::Completed);
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

fn terminal(state: &str) -> studio_core::Terminal {
    serde_json::from_value(json!({
        "id": "fixture-terminal", "argv": [], "cwd": "/tmp", "pid": 1, "exited": null,
        "foreground": "agy", "processes": [], "state": state, "app": "agy",
        "status_ms": 1, "turn": 0,
    }))
    .unwrap()
}

#[test]
fn an_agy_session_needs_you_while_its_terminal_reports_blocked() {
    let mut studio = Studio::replayed(fixture());
    assert_eq!(studio.session(PARENT).unwrap().state, State::YourTurn);

    let out = studio.terminals(&[terminal("blocked")], 10);
    let session = studio.session(PARENT).unwrap();
    assert_eq!(session.state, State::NeedsYou);
    assert!(session.questions.is_empty());
    assert!(out.iter().any(
        |o| matches!(o, studio_core::Output::NeedsYou { session_id, .. } if session_id == PARENT)
    ));

    studio.terminals(&[terminal("done")], 20);
    assert_eq!(studio.session(PARENT).unwrap().state, State::YourTurn);
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
fn a_child_ends_when_its_files_state_changes_with_no_stop() {
    let dir = tempfile::tempdir().unwrap();
    let generated = dir.path().join("p/.system_generated");
    std::fs::create_dir_all(generated.join("subagents")).unwrap();
    let file = generated.join("subagents/c.json");
    let write = |state: &str| {
        let body = json!({ "subagentDescriptor": { "typeName": "self", "role": "Runner" }, "state": state, "spawnStepIndex": 5 });
        std::fs::write(&file, body.to_string()).unwrap();
    };
    let with = |session: &str, event: &str, at: u64, fields: Value| {
        let mut l: Value = serde_json::from_str(&line(session, event, at, fields)).unwrap();
        l["transcriptPath"] = format!(
            "{}/{session}/.system_generated/logs/transcript_full.jsonl",
            dir.path().display()
        )
        .into();
        l.to_string()
    };
    let mut studio = Studio::default();
    write("ALIVE");
    studio.replay(&with("p", "SessionStart", 1, json!({})));
    studio.replay(&with("c", "SessionStart", 2, json!({})));
    studio.replay(&with("p", "Stop", 3, json!({ "fullyIdle": false })));
    assert_eq!(
        studio.session("p").unwrap().agents[0].status,
        Status::Running
    );

    write("COMPLETED");
    studio.replay(&with("p", "Stop", 4, json!({ "fullyIdle": true })));

    let child = &studio.session("p").unwrap().agents[0];
    assert_eq!(child.status, Status::Completed);
    assert!(child.ended_at.is_some());
}

#[test]
fn a_blocked_agy_session_stops_needing_you_when_its_terminal_goes_or_stops_reporting() {
    let silent = || {
        let mut t = terminal("blocked");
        t.state = None;
        t
    };
    for gone in [vec![], vec![silent()]] {
        let mut studio = Studio::replayed(fixture());
        studio.terminals(&[terminal("blocked")], 10);
        assert_eq!(studio.session(PARENT).unwrap().state, State::NeedsYou);

        studio.terminals(&gone, 20);
        assert_eq!(studio.session(PARENT).unwrap().state, State::YourTurn);
    }
}

#[test]
fn an_agy_subagents_log_is_its_own_transcript() {
    let studio = Studio::replayed(fixture());

    let log = studio
        .subagent_log(PARENT, CHILD, 0)
        .expect("the child's log");
    let call = |id: &str, tool: &str, detail: Option<&str>| LogEntry::Call {
        id: id.into(),
        tool: tool.into(),
        detail: detail.map(Into::into),
    };
    let result = |id: &str, text: &str| LogEntry::Result {
        id: id.into(),
        text: text.into(),
        failed: false,
        cut: false,
    };
    assert_eq!(
        log.entries,
        [
            LogEntry::Prompt {
                text: "Please run the shell command 'sleep 8; echo done' and report back once completed."
                    .into()
            },
            call("2", "run_command", Some("Run sleep test command")),
            result("2", "Command exited with code 0.\n\nCommand output:\ndone"),
            call("4", "send_message", None),
            result("4", &format!("Message sent to \"{PARENT}\".")),
            LogEntry::Reply {
                text: "The command `sleep 8; echo done` has completed, and the result has been reported back to the parent agent."
                    .into()
            },
        ]
    );
}
