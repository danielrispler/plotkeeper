//! Seam 2 (danielrispler/plotkeeper#170, Testing Decisions) for blocked commands: `PermissionDenied`
//! lines and the answer hook's `StudioAnswer` lines replayed through the core. The lines follow the
//! documented payloads until a recorded run replaces them.

use crate::common;
use serde_json::{Value, json};
use studio_core::{BlockedStatus, Source, State, Studio};

const SESSION: &str = "5c1e7a42-0b9d-4f36-8a15-3d2e9c6b7f08";
const COMMAND: &str = "claude -p \"summarise\" > /tmp/out.txt";
const CWD: &str = "/work/repo";

fn line(name: &str, at: u64, extra: Value) -> String {
    common::event(SESSION, name, CWD, at, extra)
}

/// A session in a turn, ready for a block.
fn started() -> Vec<String> {
    vec![
        line("SessionStart", 1_000, json!({ "source": "startup" })),
        line(
            "UserPromptSubmit",
            2_000,
            json!({ "prompt": "Go.", "prompt_id": "p-1" }),
        ),
    ]
}

/// Auto mode blocks `command` at `at`, by `agent` (a subagent) or the session's own agent.
fn denied(at: u64, id: &str, command: &str, agent: Option<&str>) -> String {
    let mut extra = json!({ "tool_name": "Bash", "tool_use_id": id, "prompt_id": "p-1",
        "tool_input": { "command": command }, "reason": "Nested agent runs are blocked.",
        "permission_mode": "auto" });
    if let Some(agent) = agent {
        extra["agent_id"] = agent.into();
    }
    line("PermissionDenied", at, extra)
}

/// The answer hook's line for Daniel's card answer to a blocked command.
fn answer(at: u64, id: &str, command: &str, decision: &str) -> String {
    line(
        "StudioAnswer",
        at,
        json!({ "studio_asked_by": "PermissionDenied", "tool_name": "Bash", "tool_use_id": id,
            "tool_input": { "command": command }, "studio_answer": { "decision": decision } }),
    )
}

fn session(lines: &[String]) -> studio_core::Session {
    let studio = Studio::replayed(lines);
    studio.session(SESSION).unwrap().clone()
}

fn then(mut lines: Vec<String>, more: Vec<String>) -> Vec<String> {
    lines.extend(more);
    lines
}

#[test]
fn a_block_raises_a_card_with_the_command_folder_and_rule_and_the_session_needs_you() {
    let lines = then(started(), vec![denied(3_000, "toolu_1", COMMAND, None)]);
    let session = session(&lines);
    assert_eq!(session.state, State::NeedsYou);
    assert_eq!(session.blocked_commands.len(), 1);
    let card = &session.blocked_commands[0];
    assert_eq!(card.command, COMMAND);
    assert_eq!(card.cwd, CWD);
    assert_eq!(card.reason, "Nested agent runs are blocked.");
    assert_eq!(card.agent, None);
    assert_eq!(card.status, BlockedStatus::Blocked);
}

#[test]
fn a_subagents_block_names_it_and_cards_stack_in_block_order() {
    let lines = then(
        started(),
        vec![
            denied(3_000, "toolu_1", "first", Some("a-7aaaa")),
            denied(4_000, "toolu_2", "second", None),
        ],
    );
    let session = session(&lines);
    let cards: Vec<(&str, Option<&str>)> = session
        .blocked_commands
        .iter()
        .map(|c| (c.command.as_str(), c.agent.as_deref()))
        .collect();
    assert_eq!(cards, [("first", Some("a-7aaaa")), ("second", None)]);
    assert_eq!(session.agent_name("a-7aaaa"), "a-7aa");
}

#[test]
fn needs_you_ends_at_the_agents_next_event() {
    let lines = then(started(), vec![denied(3_000, "toolu_1", COMMAND, None)]);
    assert_eq!(session(&lines).state, State::NeedsYou);
    let step = line(
        "PreToolUse",
        5_000,
        json!({ "tool_name": "Read", "tool_use_id": "toolu_2", "prompt_id": "p-1",
            "tool_input": { "file_path": "/work/repo/a.rs" } }),
    );
    let session = session(&then(lines, vec![step]));
    assert_eq!(session.state, State::Running);
    // The card stays: Daniel has not chosen.
    assert_eq!(session.blocked_commands[0].status, BlockedStatus::Blocked);
}

#[test]
fn allow_and_decline_end_the_wait_set_the_status_and_enter_the_decision_history_on_the_card() {
    let base = then(
        started(),
        vec![
            denied(3_000, "toolu_1", "first", None),
            denied(3_500, "toolu_2", "second", None),
        ],
    );
    let lines = then(
        base,
        vec![
            answer(4_000, "toolu_1", "first", "allow"),
            answer(4_500, "toolu_2", "second", "deny"),
        ],
    );
    let session = session(&lines);
    let statuses: Vec<_> = session.blocked_commands.iter().map(|c| c.status).collect();
    assert_eq!(statuses, [BlockedStatus::Allowed, BlockedStatus::Declined]);
    assert_eq!(session.state, State::Running);
    let history: Vec<_> = session
        .decisions
        .iter()
        .map(|d| {
            (
                d.tool.as_str(),
                d.answer.clone(),
                d.input["command"].clone(),
                d.at,
                d.source,
            )
        })
        .collect();
    assert_eq!(
        history,
        [
            ("Bash", json!("allow"), json!("first"), 4_000, Source::Card),
            ("Bash", json!("deny"), json!("second"), 4_500, Source::Card),
        ]
    );
}

#[test]
fn the_session_needs_you_until_the_last_of_its_cards_is_chosen() {
    let lines = then(
        started(),
        vec![
            denied(3_000, "toolu_1", "first", None),
            denied(3_500, "toolu_2", "second", None),
            answer(4_000, "toolu_1", "first", "allow"),
        ],
    );
    assert_eq!(session(&lines).state, State::NeedsYou);
}

#[test]
fn a_denied_call_of_another_tool_raises_no_card() {
    let mut web = denied(3_000, "toolu_1", COMMAND, None);
    web = web.replace("\"Bash\"", "\"WebFetch\"");
    let session = session(&then(started(), vec![web]));
    assert!(session.blocked_commands.is_empty());
    assert_eq!(session.state, State::Running);
}

#[test]
fn a_session_wants_you_while_a_card_is_blocked_or_allowed_and_not_after_it_is_declined() {
    let step = line(
        "PreToolUse",
        5_000,
        json!({ "tool_name": "Read", "tool_use_id": "toolu_5", "prompt_id": "p-1",
            "tool_input": { "file_path": "/work/repo/a.rs" } }),
    );
    let blocked = then(started(), vec![denied(3_000, "toolu_1", "first", None)]);
    let blocked = then(
        blocked,
        vec![denied(3_500, "toolu_2", "second", None), step],
    );
    // The agent went on, so the session shows its real state, but two cards wait.
    let s = session(&blocked);
    assert_eq!(
        (s.state, s.blocked_waiting(), s.wants_you()),
        (State::Running, 2, true)
    );
    let allowed = then(
        blocked.clone(),
        vec![answer(6_000, "toolu_1", "first", "allow")],
    );
    assert_eq!(session(&allowed).blocked_waiting(), 2);
    let declined = then(allowed, vec![answer(6_500, "toolu_2", "second", "deny")]);
    assert_eq!(session(&declined).blocked_waiting(), 1);
}

#[test]
fn a_late_allow_after_the_agent_went_on_is_recorded_on_the_card_and_in_the_history() {
    let step = line(
        "PreToolUse",
        5_000,
        json!({ "tool_name": "Read", "tool_use_id": "toolu_5", "prompt_id": "p-1",
            "tool_input": { "file_path": "/work/repo/a.rs" } }),
    );
    let lines = then(
        started(),
        vec![denied(3_000, "toolu_1", COMMAND, None), step],
    );
    let lines = then(lines, vec![answer(9_000, "toolu_1", COMMAND, "allow")]);
    let s = session(&lines);
    assert_eq!(s.state, State::Running);
    assert_eq!(s.blocked_commands[0].status, BlockedStatus::Allowed);
    assert_eq!(s.decisions.len(), 1);
    assert_eq!(
        (s.decisions[0].at, s.decisions[0].source),
        (9_000, Source::Card)
    );
}

/// The live outputs of `lines`, in order.
fn live(lines: &[String]) -> Vec<studio_core::Output> {
    let mut studio = Studio::default();
    lines.iter().flat_map(|l| studio.apply(l)).collect()
}

#[test]
fn a_live_block_raises_one_notice_naming_its_subagent_and_no_needs_you() {
    let lines = then(
        started(),
        vec![denied(3_000, "toolu_1", COMMAND, Some("a-7aaaa"))],
    );
    let notices: Vec<_> = live(&lines)
        .into_iter()
        .filter(|o| {
            matches!(
                o,
                studio_core::Output::CommandBlocked { .. } | studio_core::Output::NeedsYou { .. }
            )
        })
        .collect();
    assert_eq!(
        notices,
        [studio_core::Output::CommandBlocked {
            session_id: SESSION.into(),
            agent: Some("a-7aaaa".into())
        }]
    );
}

#[test]
fn a_block_of_another_tool_raises_no_blocked_notice() {
    let web = denied(3_000, "toolu_1", COMMAND, None).replace("\"Bash\"", "\"WebFetch\"");
    let outputs = live(&then(started(), vec![web]));
    assert!(
        !outputs
            .iter()
            .any(|o| matches!(o, studio_core::Output::CommandBlocked { .. }))
    );
}

/// The agent's retry of `command` after Allow: its `PreToolUse`.
fn retry(at: u64, id: &str, command: &str) -> String {
    line(
        "PreToolUse",
        at,
        json!({ "tool_name": "Bash", "tool_use_id": id, "prompt_id": "p-1",
            "tool_input": { "command": command } }),
    )
}

fn allowed() -> Vec<String> {
    then(
        started(),
        vec![
            denied(3_000, "toolu_1", COMMAND, None),
            answer(4_000, "toolu_1", COMMAND, "allow"),
        ],
    )
}

#[test]
fn the_matching_pre_tool_use_after_allow_runs_the_card_from_its_start() {
    let session = session(&then(allowed(), vec![retry(5_000, "toolu_2", COMMAND)]));
    let card = &session.blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::Running);
    assert_eq!(card.run.as_deref(), Some("toolu_2"));
    assert_eq!(card.run_at, Some(5_000));
}

#[test]
fn a_different_command_or_folder_leaves_the_card_allowed() {
    let other = retry(5_000, "toolu_2", "claude -p \"other\"");
    let mut elsewhere = retry(5_500, "toolu_3", COMMAND);
    elsewhere = elsewhere.replace(CWD, "/work/other");
    let session = session(&then(allowed(), vec![other, elsewhere]));
    assert_eq!(session.blocked_commands[0].status, BlockedStatus::Allowed);
}

fn ended(at: u64, id: &str, extra: Value) -> String {
    let mut extra = extra;
    extra["tool_name"] = "Bash".into();
    extra["tool_use_id"] = id.into();
    extra["prompt_id"] = "p-1".into();
    extra["tool_input"] = json!({ "command": COMMAND });
    let name = if extra.get("error").is_some() {
        "PostToolUseFailure"
    } else {
        "PostToolUse"
    };
    line(name, at, extra)
}

#[test]
fn a_retry_that_succeeds_finishes_the_card_with_exit_code_zero() {
    let done = ended(
        9_000,
        "toolu_2",
        json!({ "tool_response": { "stdout": "ok" } }),
    );
    let lines = then(allowed(), vec![retry(5_000, "toolu_2", COMMAND), done]);
    let card = &session(&lines).blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::Finished);
    assert_eq!(card.exit_code, Some(0));
}

#[test]
fn a_retry_that_exits_non_zero_finishes_the_card_with_that_code() {
    let done = ended(9_000, "toolu_2", json!({ "error": "Exit code 2\nboom" }));
    let lines = then(allowed(), vec![retry(5_000, "toolu_2", COMMAND), done]);
    let card = &session(&lines).blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::Finished);
    assert_eq!(card.exit_code, Some(2));
}

#[test]
fn a_backgrounded_retry_runs_in_the_background_until_the_wake_up_names_its_task() {
    let started_bg = ended(
        6_000,
        "toolu_2",
        json!({ "tool_response": { "backgroundTaskId": "bash-7" } }),
    );
    let lines = then(
        allowed(),
        vec![retry(5_000, "toolu_2", COMMAND), started_bg],
    );
    let card = &session(&lines).blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::RunningInBackground);
    assert_eq!(card.task.as_deref(), Some("bash-7"));
    let wake = line(
        "UserPromptSubmit",
        9_000,
        json!({ "prompt": common::task_notification("bash-7", "completed"),
            "prompt_id": "p-2" }),
    );
    let card = &session(&then(lines, vec![wake])).blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::Finished);
}

#[test]
fn a_new_block_of_the_same_command_after_allow_blocks_the_card_again() {
    let lines = then(
        allowed(),
        vec![
            retry(5_000, "toolu_2", COMMAND),
            denied(5_100, "toolu_2", COMMAND, None),
        ],
    );
    let session = session(&lines);
    assert_eq!(session.blocked_commands.len(), 1);
    let card = &session.blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::BlockedAgain);
    assert_eq!(card.call, "toolu_2");
    assert_eq!(session.state, State::NeedsYou);
}

/// The agent's turn ends at `at`.
fn stop(at: u64, agent: Option<&str>) -> String {
    let mut extra = json!({ "prompt_id": "p-1" });
    if let Some(agent) = agent {
        extra["agent_id"] = agent.into();
    }
    line(
        if agent.is_some() {
            "SubagentStop"
        } else {
            "Stop"
        },
        at,
        extra,
    )
}

#[test]
fn an_allowed_command_whose_turn_ends_without_the_retry_is_allowed_not_run_and_still_counts_on_the_row()
 {
    let session = session(&then(allowed(), vec![stop(6_000, None)]));
    assert_eq!(
        session.blocked_commands[0].status,
        BlockedStatus::AllowedNotRun
    );
    assert_eq!(session.blocked_waiting(), 1);
}

#[test]
fn a_retry_that_runs_before_the_turn_ends_is_not_allowed_not_run() {
    let lines = then(
        allowed(),
        vec![retry(5_000, "toolu_2", COMMAND), stop(6_000, None)],
    );
    assert_eq!(
        session(&lines).blocked_commands[0].status,
        BlockedStatus::Running
    );
}

#[test]
fn a_late_retry_after_allowed_not_run_runs_the_card_again() {
    let lines = then(
        allowed(),
        vec![stop(6_000, None), retry(7_000, "toolu_2", COMMAND)],
    );
    let card = &session(&lines).blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::Running);
    assert_eq!(card.run.as_deref(), Some("toolu_2"));
}

#[test]
fn a_new_block_of_an_allowed_not_run_command_blocks_the_card_again() {
    let lines = then(
        allowed(),
        vec![stop(6_000, None), denied(7_000, "toolu_3", COMMAND, None)],
    );
    assert_eq!(
        session(&lines).blocked_commands[0].status,
        BlockedStatus::BlockedAgain
    );
}

#[test]
fn an_ended_session_does_not_want_you_for_its_waiting_cards() {
    let end = line("SessionEnd", 8_000, json!({ "reason": "other" }));
    let s = session(&then(allowed(), vec![end]));
    assert_eq!(s.state, State::Ended);
    assert_eq!((s.blocked_waiting(), s.wants_you()), (1, false));
}

/// A subagent `a-7aaaa`'s block of `COMMAND`, which Daniel allowed.
fn allowed_for_subagent() -> Vec<String> {
    then(
        started(),
        vec![
            denied(3_000, "toolu_1", COMMAND, Some("a-7aaaa")),
            answer(4_000, "toolu_1", COMMAND, "allow"),
        ],
    )
}

#[test]
fn the_sessions_turn_end_leaves_a_running_subagents_allowed_card_allowed() {
    let s = session(&then(allowed_for_subagent(), vec![stop(6_000, None)]));
    assert_eq!(s.blocked_commands[0].status, BlockedStatus::Allowed);
}

#[test]
fn a_subagents_own_turn_end_makes_its_allowed_card_allowed_not_run() {
    let lines = then(allowed_for_subagent(), vec![stop(6_000, Some("a-7aaaa"))]);
    let s = session(&lines);
    assert_eq!(s.blocked_commands[0].status, BlockedStatus::AllowedNotRun);
}

#[test]
fn the_sessions_turn_end_covers_the_allowed_card_of_a_subagent_that_had_already_ended() {
    let lines = then(
        started(),
        vec![
            denied(3_000, "toolu_1", COMMAND, Some("a-7aaaa")),
            stop(3_500, Some("a-7aaaa")),
            answer(4_000, "toolu_1", COMMAND, "allow"),
            stop(6_000, None),
        ],
    );
    let s = session(&lines);
    assert_eq!(s.blocked_commands[0].status, BlockedStatus::AllowedNotRun);
}

#[test]
fn a_block_of_the_same_command_that_is_not_the_running_retrys_own_gets_its_own_card() {
    let lines = then(
        allowed(),
        vec![
            retry(5_000, "toolu_2", COMMAND),
            denied(5_100, "toolu_3", COMMAND, Some("a-7aaaa")),
        ],
    );
    let s = session(&lines);
    let cards: Vec<_> = s
        .blocked_commands
        .iter()
        .map(|c| (c.call.as_str(), c.status))
        .collect();
    assert_eq!(
        cards,
        [
            ("toolu_1", BlockedStatus::Running),
            ("toolu_3", BlockedStatus::Blocked)
        ]
    );
}

/// The payloads of a real run (session 407caba5 of the first recordings of #174): the classifier's real
/// `reason`, a `description` and `run_in_background` on the input, a retry under a new `tool_use_id`, and a
/// wake-up with its task, tool call and output file.
#[test]
fn a_real_shaped_background_block_goes_from_blocked_to_finished_at_its_wake_up() {
    let (running, finished) = background_block_woken_by("…");
    assert_eq!(
        running.reason,
        "The server-side auto mode classifier judged this action dangerous (it gave no explanation)"
    );
    assert_eq!(running.status, BlockedStatus::RunningInBackground);
    assert_eq!(finished.status, BlockedStatus::Finished);
    assert_eq!(finished.exit_code, None);
}

/// Each way a real wake-up's summary words how a background command ended (Daniel's event log, 2026-11-17:
/// 94 completed with 0, 3 failed, 1 completed with a code and a note), and the exit code the card shows.
#[test]
fn a_background_commands_exit_code_is_read_from_each_real_summary_shape() {
    for (summary, code) in [
        ("Background command \"…\" completed (exit code 0)", Some(0)),
        (
            "Background command \"…\" failed with exit code 144",
            Some(144),
        ),
        ("Background command \"…\" failed with exit code 1", Some(1)),
        (
            "Background command \"…\" completed (exit code 1: No matches found)",
            Some(1),
        ),
        ("Background command \"…\" was stopped", None),
    ] {
        let (_, finished) = background_block_woken_by(summary);
        assert_eq!(finished.exit_code, code, "{summary}");
    }
}

/// The card of a real-shaped background block (session 407caba5 of the first recordings of #174) before its
/// wake-up, and after a wake-up whose summary is `summary`.
fn background_block_woken_by(
    summary: &str,
) -> (studio_core::BlockedCommand, studio_core::BlockedCommand) {
    const BG: &str =
        "claude -p \"Reply with the word ok\" --model haiku # studio-fixture-background";
    let input = json!({ "command": BG, "description": "Run claude CLI in background",
        "run_in_background": true });
    let reason = "The server-side auto mode classifier judged this action dangerous (it gave no explanation)";
    let denied = line(
        "PermissionDenied",
        3_000,
        json!({ "tool_name": "Bash", "tool_use_id": "toolu_011", "prompt_id": "p-1",
            "tool_input": input, "reason": reason, "permission_mode": "auto" }),
    );
    let answered = answer(4_000, "toolu_011", BG, "allow");
    let pre = |at, id: &str| {
        line(
            "PreToolUse",
            at,
            json!({ "tool_name": "Bash", "tool_use_id": id, "prompt_id": "p-1", "tool_input": input }),
        )
    };
    let wake = line(
        "UserPromptSubmit",
        9_000,
        json!({ "prompt_id": "p-2", "prompt": format!("<task-notification><task-id>b7f03l9m0</task-id>\
            <tool-use-id>toolu_01Ut</tool-use-id><output-file>/private/tmp/c/s/tasks/b7f03l9m0.output</output-file>\
            <status>completed</status><summary>{summary}</summary></task-notification>") }),
    );
    let mut lines = then(started(), vec![denied, answered, pre(5_000, "toolu_01Ut")]);
    let started_bg = line(
        "PostToolUse",
        6_000,
        json!({ "tool_name": "Bash", "tool_use_id": "toolu_01Ut", "prompt_id": "p-1", "tool_input": input,
            "tool_response": { "backgroundTaskId": "b7f03l9m0", "interrupted": false } }),
    );
    lines.push(started_bg);

    let running = session(&lines).blocked_commands[0].clone();
    let finished = session(&then(lines, vec![wake])).blocked_commands[0].clone();
    (running, finished)
}

/// The recorded run (`core/tests/record-blocked-run.sh`): a foreground and a background `claude -p`, each
/// blocked, allowed from the card, retried and run, then the background one's wake-up.
const RECORDED: &str = include_str!("fixtures/blocked-run.jsonl");

/// The recorded run's lines up to and with the `n`th (from 1) event named `name`.
fn recorded_through(name: &str, n: usize) -> Studio {
    let lines: Vec<&str> = RECORDED.lines().collect();
    let mut seen = 0;
    let end = lines
        .iter()
        .position(|l| {
            seen += usize::from(l.contains(&format!("\"hook_event_name\":\"{name}\"")));
            seen == n
        })
        .unwrap_or_else(|| panic!("no {n}th {name}"));
    Studio::replayed(&lines[..=end])
}

fn recorded_cards(name: &str, n: usize) -> Vec<studio_core::BlockedCommand> {
    let studio = recorded_through(name, n);
    let id = RECORDED
        .lines()
        .next()
        .and_then(|l| serde_json::from_str::<Value>(l).ok())
        .and_then(|v| v["session_id"].as_str().map(String::from))
        .unwrap();
    studio.session(&id).unwrap().blocked_commands.clone()
}

#[test]
fn the_recorded_foreground_command_goes_blocked_allowed_running_finished() {
    let status = |name, n| recorded_cards(name, n)[0].status;
    assert_eq!(status("PermissionDenied", 1), BlockedStatus::Blocked);
    assert_eq!(status("StudioAnswer", 1), BlockedStatus::Allowed);
    // The retry runs under a new call id, and its `PermissionRequest` follows.
    assert_eq!(status("PreToolUse", 2), BlockedStatus::Running);
    let card = &recorded_cards("PostToolUse", 1)[0];
    assert_eq!(card.status, BlockedStatus::Finished);
    assert_eq!(card.exit_code, Some(0));
    assert!(card.command.ends_with("# studio-fixture-foreground"));
    assert!(
        card.reason
            .starts_with("The server-side auto mode classifier")
    );
}

#[test]
fn the_recorded_background_command_runs_in_the_background_until_its_wake_up_gives_its_exit_code() {
    let cards = recorded_cards("PostToolUse", 2);
    assert_eq!(cards[1].status, BlockedStatus::RunningInBackground);
    assert_eq!(cards[1].task.as_deref(), Some("brug0dna7"));
    // The turn ends with the task still running: the card stays, not "not run".
    assert_eq!(
        recorded_cards("Stop", 1)[1].status,
        BlockedStatus::RunningInBackground
    );

    let card = &recorded_cards("UserPromptSubmit", 2)[1];
    assert_eq!(card.status, BlockedStatus::Finished);
    assert_eq!(card.exit_code, Some(0));
}

/// The `PermissionRequest` that a claimed pass's "ask" sends `command` to.
fn permission_request(at: u64, command: &str) -> String {
    line(
        "PermissionRequest",
        at,
        json!({ "tool_name": "Bash", "prompt_id": "p-1", "tool_input": { "command": command },
            "permission_mode": "auto" }),
    )
}

#[test]
fn the_recorded_retries_pass_their_permission_request_without_needing_you_or_a_terminal_decision() {
    let lines: Vec<String> = RECORDED.lines().map(String::from).collect();
    let id = serde_json::from_str::<Value>(&lines[0]).unwrap()["session_id"]
        .as_str()
        .unwrap()
        .to_string();
    let studio = recorded_through("PermissionRequest", 1);
    assert_eq!(studio.session(&id).unwrap().state, State::Running);
    let studio = Studio::replayed(&lines);
    let sources: Vec<Source> = studio
        .session(&id)
        .unwrap()
        .decisions
        .iter()
        .map(|d| d.source)
        .collect();
    assert_eq!(sources, [Source::Card, Source::Card]);
    let mut live = Studio::default();
    let outputs: Vec<_> = lines.iter().flat_map(|l| live.apply(l)).collect();
    assert!(
        !outputs
            .iter()
            .any(|o| matches!(o, studio_core::Output::NeedsYou { .. }))
    );
}

#[test]
fn a_retrys_permission_request_that_lands_before_its_start_does_not_need_you() {
    let lines = then(
        allowed(),
        vec![
            permission_request(5_000, COMMAND),
            retry(5_010, "toolu_2", COMMAND),
        ],
    );
    assert_eq!(session(&lines).state, State::Running);
    let s = session(&then(
        lines,
        vec![ended(
            6_000,
            "toolu_2",
            json!({ "tool_response": { "stdout": "ok" } }),
        )],
    ));
    let sources: Vec<Source> = s.decisions.iter().map(|d| d.source).collect();
    assert_eq!(sources, [Source::Card]);
}

#[test]
fn a_blocked_check_that_never_ran_leaves_no_check_line() {
    let check = "npm run check";
    let lines = then(
        started(),
        vec![
            line(
                "PreToolUse",
                2_500,
                json!({ "tool_name": "Bash", "tool_use_id": "toolu_1", "prompt_id": "p-1",
                    "tool_input": { "command": check } }),
            ),
            denied(3_000, "toolu_1", check, None),
            answer(4_000, "toolu_1", check, "deny"),
            stop(5_000, None),
        ],
    );
    let session = session(&lines);
    assert!(
        session
            .turns
            .iter()
            .all(|t| t.checks.is_empty() && t.listed.is_empty())
    );
}

#[test]
fn a_retry_records_which_agent_ran_it_for_its_output() {
    let parent = then(
        allowed_for_subagent(),
        vec![retry(5_000, "toolu_2", COMMAND)],
    );
    let card = &session(&parent).blocked_commands[0];
    assert_eq!(card.status, BlockedStatus::Running);
    assert_eq!(card.run_by, None);
    let mut own = retry(5_000, "toolu_2", COMMAND);
    own = own.replace("\"tool_name\"", "\"agent_id\":\"a-7aaaa\",\"tool_name\"");
    let card = &session(&then(allowed_for_subagent(), vec![own])).blocked_commands[0];
    assert_eq!(card.run_by.as_deref(), Some("a-7aaaa"));
}

#[test]
fn another_subagents_run_of_the_same_command_leaves_the_card_and_its_block_gets_its_own() {
    let other = retry(5_000, "toolu_3", COMMAND)
        .replace("\"tool_name\"", "\"agent_id\":\"a-8bbbb\",\"tool_name\"");
    let lines = then(
        allowed_for_subagent(),
        vec![other, denied(5_100, "toolu_3", COMMAND, Some("a-8bbbb"))],
    );
    let s = session(&lines);
    let cards: Vec<_> = s
        .blocked_commands
        .iter()
        .map(|c| (c.call.as_str(), c.agent.as_deref(), c.status))
        .collect();
    assert_eq!(
        cards,
        [
            ("toolu_1", Some("a-7aaaa"), BlockedStatus::Allowed),
            ("toolu_3", Some("a-8bbbb"), BlockedStatus::Blocked)
        ]
    );
}

#[test]
fn a_wake_up_reporting_two_background_retries_finishes_both_cards() {
    let bash = |name: &str, at, id: &str, command: &str, extra: Value| {
        let mut extra = extra;
        extra["tool_name"] = "Bash".into();
        extra["tool_use_id"] = id.into();
        extra["prompt_id"] = "p-1".into();
        extra["tool_input"] = json!({ "command": command });
        line(name, at, extra)
    };
    let bg = |task: &str| json!({ "tool_response": { "backgroundTaskId": task } });
    let notice = |task: &str, code| {
        format!(
            "<task-notification><task-id>{task}</task-id><status>completed</status>\
            <summary>Background command \"x\" completed (exit code {code})</summary></task-notification>"
        )
    };
    let lines = then(
        started(),
        vec![
            denied(3_000, "toolu_1", "first", None),
            denied(3_100, "toolu_2", "second", None),
            answer(4_000, "toolu_1", "first", "allow"),
            answer(4_100, "toolu_2", "second", "allow"),
            bash("PreToolUse", 5_000, "toolu_3", "first", json!({})),
            bash("PostToolUse", 5_100, "toolu_3", "first", bg("bash-1")),
            bash("PreToolUse", 5_200, "toolu_4", "second", json!({})),
            bash("PostToolUse", 5_300, "toolu_4", "second", bg("bash-2")),
            line(
                "UserPromptSubmit",
                9_000,
                json!({ "prompt_id": "p-2", "prompt": format!("{}{}", notice("bash-1", 0), notice("bash-2", 3)) }),
            ),
        ],
    );
    let s = session(&lines);
    let cards: Vec<_> = s
        .blocked_commands
        .iter()
        .map(|c| (c.status, c.exit_code))
        .collect();
    assert_eq!(
        cards,
        [
            (BlockedStatus::Finished, Some(0)),
            (BlockedStatus::Finished, Some(3))
        ]
    );
}
