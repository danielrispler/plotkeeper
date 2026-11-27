//! Each hook entry names its agent, and the core reads a line by that name (#355; #320, Harnesses,
//! "Each hook entry names its agent"). A line with no name, written before, is read by its shape.

use crate::common::{base, with};
use serde_json::{Value, json};
use std::path::PathBuf;
use studio_core::{
    AgyDriftKind, CODEX_CAPABILITIES, COPILOT_CAPABILITIES, CURSOR_CAPABILITIES, Capabilities,
    QWEN_CAPABILITIES, State, Studio,
};

/// `log`'s lines, each with `studio_agent` set to `agent`, as the hook now writes them.
fn named(log: &str, agent: &str) -> Vec<String> {
    log.lines()
        .map(|line| {
            let mut event: Value = serde_json::from_str(line).unwrap();
            event["studio_agent"] = agent.into();
            event.to_string()
        })
        .collect()
}

/// What a replay makes of the sessions, which the line offsets left out of the saved form do not
/// change.
fn sessions(studio: &Studio) -> Value {
    serde_json::to_value(studio.sessions()).unwrap()
}

#[test]
fn a_named_claude_and_agy_log_replays_into_the_same_sessions() {
    let brain = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    let agy =
        include_str!("fixtures/agy/run.jsonl").replace("@BRAIN@", &brain.display().to_string());
    for (log, agent) in [
        (
            include_str!("fixtures/subagent-run.jsonl").to_owned(),
            "claude",
        ),
        (
            include_str!("fixtures/checkpoint-day.jsonl").to_owned(),
            "claude",
        ),
        (agy, "agy"),
    ] {
        let before = Studio::replayed(log.lines());
        let after = Studio::replayed(named(&log, agent));

        assert!(!before.sessions().is_empty(), "{agent}");
        assert_eq!(sessions(&after), sessions(&before), "{agent}");
        assert!(after.agy_drift().is_empty(), "{:?}", after.agy_drift());
    }
}

#[test]
fn a_line_of_an_agent_plotkeeper_cannot_read_is_drift_not_claudes() {
    let line = with(
        base("s-1", "SessionStart", "/w", 1),
        json!({ "source": "startup", "studio_agent": "nobody" }),
    );
    let studio = Studio::replayed([line]);

    assert!(studio.session("s-1").is_none());
    let kinds: Vec<_> = studio.agy_drift().iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [AgyDriftKind::Agent]);
}

#[test]
fn a_claude_shaped_line_named_agy_is_read_by_agys_parser() {
    let line = with(
        base("s-1", "SessionStart", "/w", 1),
        json!({ "source": "startup", "studio_agent": "agy" }),
    );
    let studio = Studio::replayed([line]);

    assert!(studio.session("s-1").is_none());
    let kinds: Vec<_> = studio.agy_drift().iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [AgyDriftKind::HookEvent]);
}

/// Only agy's version comes from running it: no other agent's session asks for `agy --version`.
#[test]
fn only_an_agy_session_asks_for_agys_version() {
    for (agent, run) in [
        ("codex", include_str!("fixtures/codex/run.jsonl")),
        ("copilot", include_str!("fixtures/copilot/run.jsonl")),
        ("cursor", include_str!("fixtures/cursor/run.jsonl")),
        ("qwen", include_str!("fixtures/qwen/run.jsonl")),
        ("opencode", include_str!("fixtures/opencode/run.jsonl")),
    ] {
        let mut studio = Studio::default();
        let asked = run
            .lines()
            .flat_map(|line| studio.apply(line))
            .filter(|o| matches!(o, studio_core::Output::AgyVersion))
            .count();
        assert_eq!(asked, 0, "{agent}");
    }
}

/// The four hook adapters' documented runs (`fixtures/<agent>/run.jsonl`), each with the id of its
/// first session, which runs two turns.
struct Adapter {
    agent: &'static str,
    run: &'static str,
    first: &'static str,
    capabilities: Capabilities,
}

fn adapters() -> [Adapter; 4] {
    [
        Adapter {
            agent: "codex",
            run: include_str!("fixtures/codex/run.jsonl"),
            first: "thr_7f3c2a",
            capabilities: CODEX_CAPABILITIES,
        },
        Adapter {
            agent: "qwen",
            run: include_str!("fixtures/qwen/run.jsonl"),
            first: "5c0e1f7a-2b3d-4e8f-9a10-b2c3d4e5f601",
            capabilities: QWEN_CAPABILITIES,
        },
        Adapter {
            agent: "cursor",
            run: include_str!("fixtures/cursor/run.jsonl"),
            first: "c7a1e2f0-3b4d-4c5e-9f60-1a2b3c4d5e6f",
            capabilities: CURSOR_CAPABILITIES,
        },
        Adapter {
            agent: "copilot",
            run: include_str!("fixtures/copilot/run.jsonl"),
            first: "cs-7a1f",
            capabilities: COPILOT_CAPABILITIES,
        },
    ]
}

#[test]
fn a_documented_run_replays_into_its_first_sessions_turns_and_tools() {
    let tools = |tools: &[(&'static str, usize)]| tools.to_vec();
    let rows = [
        (
            "codex",
            tools(&[("Bash", 0), ("apply_patch", 0), ("Bash", 1)]),
        ),
        (
            "qwen",
            tools(&[
                ("run_shell_command", 0),
                ("write_file", 0),
                ("run_shell_command", 1),
            ]),
        ),
        (
            "cursor",
            tools(&[("Read", 0), ("Write", 0), ("Shell", 1), ("Shell", 1)]),
        ),
        ("copilot", tools(&[("bash", 0), ("edit", 0), ("bash", 1)])),
    ];
    for adapter in adapters() {
        let studio = Studio::replayed(adapter.run.lines());
        let session = studio.session(adapter.first).expect(adapter.agent);

        assert_eq!(
            session.capabilities, adapter.capabilities,
            "{}",
            adapter.agent
        );
        assert_eq!(session.capabilities.agent, adapter.agent);
        let prompts: Vec<_> = session.turns.iter().map(|t| t.prompt.as_str()).collect();
        assert_eq!(
            prompts,
            ["Add a greeting to hello.txt", "Now run the tests"],
            "{}",
            adapter.agent
        );
        assert!(session.turns.iter().all(|t| t.ended_at.is_some()));
        assert_eq!(session.state, State::Ended, "{}", adapter.agent);
        let calls: Vec<_> = session
            .ledger
            .rows()
            .iter()
            .map(|r| (r.tool.as_str(), r.turn.unwrap()))
            .collect();
        let want = &rows.iter().find(|(a, _)| *a == adapter.agent).unwrap().1;
        assert_eq!(&calls, want, "{}", adapter.agent);
    }
}

/// A line the server writes itself for session `id`, shaped as Claude Code's lines are.
fn server_line(id: &str, name: &str, at: u64, fields: Value) -> String {
    let mut line = json!({ "session_id": id, "hook_event_name": name, "studio_received_ms": at });
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
}

#[test]
fn a_hook_agents_session_ended_by_the_process_check_or_interrupted_stays_its_agent() {
    // Each run up to its second turn's tool, before its own stop or end, then the server's lines
    // for an interrupt and for its gone process.
    for (adapter, cut, prompt_id) in adapters()
        .into_iter()
        .zip([(15, "turn_2"), (15, "p_2"), (11, "gen_2"), (14, "p_2")])
        .map(|(a, (cut, id))| (a, cut, id))
    {
        let lines: Vec<String> = adapter
            .run
            .lines()
            .take(cut)
            .map(ToString::to_string)
            .collect();
        let synthetic = [
            server_line(
                adapter.first,
                "StudioInterrupted",
                1_794_900_011_000,
                json!({ "prompt_id": prompt_id }),
            ),
            server_line(
                adapter.first,
                "SessionEnd",
                1_794_900_012_000,
                json!({ "reason": "process_gone" }),
            ),
        ];

        let mut live = Studio::replayed(&lines);
        for line in &synthetic {
            live.apply(line);
        }
        let mut all = lines.clone();
        all.extend(synthetic);
        let restarted = Studio::replayed(&all);
        for studio in [&live, &restarted] {
            let session = studio.session(adapter.first).unwrap();
            assert_eq!(session.state, State::Ended, "{}", adapter.agent);
            assert_eq!(
                session.capabilities, adapter.capabilities,
                "{}",
                adapter.agent
            );
        }
    }
}

/// Qwen Code and Copilot record no turn trees; Codex's and Cursor's capabilities say they do.
#[test]
fn a_hook_agent_without_turn_trees_records_no_files_for_its_turns() {
    for adapter in adapters() {
        let studio = Studio::replayed(adapter.run.lines());
        let session = studio.session(adapter.first).unwrap();
        let none = ["qwen", "copilot"].contains(&adapter.agent);
        assert_eq!(!session.capabilities.turn_trees, none, "{}", adapter.agent);
        if none {
            assert!(
                session.turns.iter().all(|t| t.files.is_none()),
                "{}",
                adapter.agent
            );
        }
    }
}

#[test]
fn an_interrupt_or_an_aborted_stop_ends_its_turn() {
    // Codex's `Interrupt` and Cursor's aborted `stop`.
    for (agent, marker) in [("codex", "\"Interrupt\""), ("cursor", "\"aborted\"")] {
        let adapter = adapters().into_iter().find(|a| a.agent == agent).unwrap();
        let lines: Vec<&str> = adapter.run.lines().collect();
        let at = lines.iter().position(|l| l.contains(marker)).unwrap();

        let before = Studio::replayed(&lines[..at]);
        assert_eq!(
            before.session(adapter.first).unwrap().state,
            State::Running,
            "{agent}"
        );
        let after = Studio::replayed(&lines[..=at]);
        let session = after.session(adapter.first).unwrap();
        assert_ne!(session.state, State::Running, "{agent}");
        assert!(session.turns[1].ended_at.is_some(), "{agent}");
    }
}

#[test]
fn a_permission_request_needs_you_until_its_call_ends() {
    // The `PermissionRequest` of the file edit (the run's seventh line), then its result.
    for adapter in adapters()
        .into_iter()
        .filter(|a| ["codex", "qwen"].contains(&a.agent))
    {
        let lines: Vec<&str> = adapter.run.lines().collect();
        let state_after = |n: usize| {
            Studio::replayed(&lines[..n])
                .session(adapter.first)
                .unwrap()
                .state
        };
        assert_eq!(state_after(7), State::NeedsYou, "{}", adapter.agent);
        assert_eq!(state_after(9), State::Running, "{}", adapter.agent);
    }
}

/// An IDE-hosted session's pid is not recorded (#320, "IDE-hosted pids"), so a running one with no
/// event for a while goes idle ("idle?"), and the process check never ends it.
#[test]
fn an_ide_session_with_no_pid_goes_idle_and_is_never_ended_by_the_process_check() {
    // Each run's IDE session: its id, the line it is mid-turn at, the time of its last line and a
    // quiet period long enough to idle it.
    let rows = [
        (
            "cursor",
            "d81f9c3a-6e2b-47d0-8a15-0f9e8d7c6b5a",
            "What does this repo do?",
            1_794_800_003_500,
            24 * 60 * 60 * 1000,
        ),
        (
            "copilot",
            "vs-3c9e",
            "\"PreToolUse\"",
            1_794_700_005_500,
            2 * 60 * 60 * 1000,
        ),
    ];
    for (agent, id, marker, last, quiet) in rows {
        let adapter = adapters().into_iter().find(|a| a.agent == agent).unwrap();
        let lines: Vec<&str> = adapter.run.lines().collect();
        let at = lines.iter().position(|l| l.contains(marker)).unwrap();
        let mut studio = Studio::replayed(&lines[..=at]);
        let session = studio.session(id).unwrap();
        assert_eq!(session.pid, None, "{agent}");
        assert_eq!(session.state, State::Running, "{agent}");
        assert!(studio.gone(|_| false).iter().all(|g| g != id), "{agent}");

        studio.tick(last + quiet);
        assert_eq!(studio.session(id).unwrap().state, State::Idle, "{agent}");
        assert!(studio.gone(|_| false).iter().all(|g| g != id), "{agent}");
    }
}
