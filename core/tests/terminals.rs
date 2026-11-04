//! Seam 1 (#461, decision 8): replay hook events through the core and check that each session
//! carries its studio terminal, `STUDIO_TERMINAL_ID` as the hook records it, so a row maps to the
//! terminal its pane shows.

use serde_json::json;
use studio_core::Studio;

/// Of `two-sessions.jsonl`: the first started outside the studio, the second in its terminal `t-2`.
const OUTSIDE: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
const IN_STUDIO: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";

fn terminal<'a>(studio: &'a Studio, session: &str) -> Option<&'a str> {
    studio.session(session)?.terminal.as_deref()
}

fn replay() -> Studio {
    Studio::replayed(include_str!("fixtures/two-sessions.jsonl").lines())
}

#[test]
fn a_session_carries_its_terminal_and_one_started_elsewhere_none() {
    let studio = replay();

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
    let mut studio = replay();
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
