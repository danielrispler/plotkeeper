//! Each hook entry names its agent, and the core reads a line by that name (#355; #320, Harnesses,
//! "Each hook entry names its agent"). A line with no name, written before, is read by its shape.

use crate::common::{base, with};
use serde_json::{Value, json};
use std::path::PathBuf;
use studio_core::{AgyDriftKind, Studio};

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
