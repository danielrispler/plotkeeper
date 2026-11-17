//! Seam 2 (danielrispler/plotkeeper#170, Testing Decisions) for blocked commands: `PermissionDenied`
//! lines and the answer hook's `StudioAnswer` lines replayed through the core. The lines follow the
//! documented payloads until a recorded run replaces them.

use serde_json::{Value, json};
use studio_core::{BlockedStatus, Source, State, Studio};

const SESSION: &str = "5c1e7a42-0b9d-4f36-8a15-3d2e9c6b7f08";
const COMMAND: &str = "claude -p \"summarise\" > /tmp/out.txt";
const CWD: &str = "/work/repo";

fn line(name: &str, at: u64, extra: Value) -> String {
    let mut line = json!({ "session_id": SESSION, "hook_event_name": name, "cwd": CWD,
        "studio_received_ms": at });
    line.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    line.to_string()
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

#[test]
fn the_notice_shows_once_with_others_within_three_seconds_and_not_in_a_focused_window() {
    use std::time::Duration;
    let mut notices = studio_core::Notifications::default();
    let title = "Auto mode blocked a command";
    notices.push(title, "repo s-1".into(), Duration::from_secs(10));
    let due = notices.due(Duration::from_secs(13), || false, |_| false, || None);
    assert_eq!(due.notification, Some((title.into(), "repo s-1".into())));
    notices.push(title, "repo s-1".into(), Duration::from_secs(20));
    let due = notices.due(Duration::from_secs(23), || true, |_| false, || None);
    assert_eq!(due.notification, None);
}
