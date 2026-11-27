//! Seam 1 (danielrispler/Falinks#461, decision 8): replay hook events through the core and check that each session
//! carries its studio terminal, `PLOTKEEPER_TERMINAL_ID` as the hook records it, so a row maps to the
//! terminal its pane shows.

use crate::common::{TWO, replay};
use serde_json::json;
use studio_core::Studio;

/// Of `two-sessions.jsonl`: the first started outside the studio, the second in its terminal `t-2`.
const OUTSIDE: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
const IN_STUDIO: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";

fn terminal<'a>(studio: &'a Studio, session: &str) -> Option<&'a str> {
    studio.session(session)?.terminal.as_deref()
}

#[test]
fn a_session_carries_its_terminal_and_one_started_elsewhere_none() {
    let studio = replay(TWO);

    assert_eq!(terminal(&studio, IN_STUDIO), Some("t-2"));
    assert_eq!(terminal(&studio, OUTSIDE), None);
    // The list's rows carry it too, so the window maps a row to its pane.
    let rows = studio.sessions();
    let row = rows.iter().find(|s| s.agent.id == IN_STUDIO).unwrap();
    let row = serde_json::to_value(row).unwrap();
    assert_eq!(row["terminal"], "t-2");
}

#[test]
fn a_session_resumed_in_a_new_terminal_maps_to_the_new_one() {
    let mut studio = replay(TWO);
    let event = |name: &str, terminal: Option<&str>, at: u64| {
        let mut event = json!({
            "session_id": IN_STUDIO,
            "hook_event_name": name,
            "cwd": "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-322-terminals",
            "studio_received_ms": at,
        });
        if let Some(terminal) = terminal {
            event["studio_terminal_id"] = terminal.into();
        }
        event.to_string()
    };

    studio.apply(&event("SessionEnd", Some("t-2"), 1_793_262_000_000));
    assert_eq!(terminal(&studio, IN_STUDIO), Some("t-2"));
    studio.apply(&event("SessionStart", Some("t-9"), 1_793_263_000_000));
    assert_eq!(terminal(&studio, IN_STUDIO), Some("t-9"));
    // Resumed again outside the studio: no studio terminal shows it.
    studio.apply(&event("SessionStart", None, 1_793_264_000_000));
    assert_eq!(terminal(&studio, IN_STUDIO), None);
}

/// A session Daniel started that resumes another drops its throwaway startup id (danielrispler/Falinks#562); the
/// selection follows to the resumed session, so it stays live and visible.
#[test]
fn a_resume_in_a_new_terminal_moves_the_selection_to_the_resumed_session() {
    let mut studio = replay(TWO);
    let start = |id: &str, source: &str, at: u64| {
        json!({
            "session_id": id,
            "hook_event_name": "SessionStart",
            "source": source,
            "cwd": "/Users/danielrispler/work/Falinks",
            "studio_terminal_id": "t-5",
            "studio_received_ms": at,
        })
        .to_string()
    };

    studio.select("t-5");
    studio.apply(&start("throwaway-id", "startup", 1_793_263_000_000));
    studio.apply(&start(OUTSIDE, "resume", 1_793_263_001_000));

    let selected = studio.selected().expect("a selection");
    assert_eq!(selected.agent.id, OUTSIDE);
    assert!(studio.sessions().iter().any(|s| s.agent.id == OUTSIDE));
}
