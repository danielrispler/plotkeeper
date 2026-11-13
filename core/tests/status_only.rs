//! Seam 1 (danielrispler/Falinks#560): replay hook events and scripted daemon `list` status through the core. A terminal
//! whose program is not Claude Code and reports OSC 7501 gets a status-only row; a Claude session
//! keeps its hook state, and a lasting disagreement with its 7501 state appends one mismatch line.

use serde_json::{Value, json};
use studio_core::{Output, State, Studio, Terminal};

/// Of `two-sessions.jsonl`: the session in studio terminal `t-2`.
const IN_STUDIO: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";
const T0: u64 = 1_793_270_000_000;

/// One terminal of the daemon's `list`, with its 7501 status.
fn term(id: &str, state: Option<&str>, app: Option<&str>, at: u64) -> Terminal {
    term_at_turn(id, state, app, at, 0)
}

/// As `term`, with the daemon's count of how often the program left `working`.
fn term_at_turn(id: &str, state: Option<&str>, app: Option<&str>, at: u64, turn: u64) -> Terminal {
    let row: Value = json!({
        "id": id,
        "argv": [],
        "cwd": "/Users/danielrispler/work/other-project",
        "pid": 100,
        "exited": null,
        "foreground": "node",
        "processes": [],
        "state": state,
        "kind": null,
        "app": app,
        "status_ms": state.map(|_| at),
        "turn": turn,
    });
    serde_json::from_value(row).unwrap()
}

fn listed(studio: &mut Studio, terms: &[Terminal], now: u64) -> Vec<Output> {
    studio.tick(now);
    studio.terminals(terms, now)
}

fn state(studio: &Studio, id: &str) -> Option<State> {
    studio.session(id).map(|s| s.state)
}

#[test]
fn a_codex_like_terminal_shows_running_then_needs_you_then_your_turn() {
    let mut studio = Studio::default();

    listed(
        &mut studio,
        &[term("t-7", Some("working"), Some("codex"), T0)],
        T0,
    );
    assert_eq!(state(&studio, "t-7"), Some(State::Running));
    let row = studio.session("t-7").unwrap();
    assert!(row.status_only);
    assert_eq!(row.identity.name, "codex · other-project");
    assert_eq!(row.terminal.as_deref(), Some("t-7"));
    assert!(studio.sessions().iter().any(|s| s.agent.id == "t-7"));

    let blocked = listed(
        &mut studio,
        &[term_at_turn(
            "t-7",
            Some("blocked"),
            Some("codex"),
            T0 + 1_000,
            1,
        )],
        T0 + 1_000,
    );
    assert_eq!(state(&studio, "t-7"), Some(State::NeedsYou));
    // Needs you notifies as a Claude session's does (danielrispler/Falinks#522).
    assert!(blocked.contains(&Output::NeedsYou {
        session_id: "t-7".into(),
        agent: None
    }));

    let done = listed(
        &mut studio,
        &[term_at_turn(
            "t-7",
            Some("done"),
            Some("codex"),
            T0 + 2_000,
            1,
        )],
        T0 + 2_000,
    );
    assert_eq!(state(&studio, "t-7"), Some(State::YourTurn));
    assert!(done.contains(&Output::YourTurn {
        session_id: "t-7".into()
    }));
}

#[test]
fn with_no_app_the_row_is_named_by_its_foreground_program() {
    let mut studio = Studio::default();

    listed(&mut studio, &[term("t-8", Some("idle"), None, T0)], T0);

    assert_eq!(
        studio.session("t-8").unwrap().identity.name,
        "node · other-project"
    );
    assert_eq!(state(&studio, "t-8"), Some(State::YourTurn));
}

#[test]
fn an_agent_revived_into_its_latest_conversation_says_so_on_its_row() {
    // danielrispler/Falinks#561: revival with no known conversation id resumes the folder's latest one.
    let mut studio = Studio::default();
    let mut latest = term("t-9", Some("idle"), Some("codex"), T0);
    latest.argv = vec!["codex".into(), "resume".into(), "--last".into()];
    let mut known = term("t-10", Some("idle"), Some("codex"), T0);
    known.argv = vec!["codex".into(), "resume".into(), "c-1".into()];

    listed(&mut studio, &[latest, known], T0);

    let name = |id| studio.session(id).unwrap().identity.name.clone();
    assert_eq!(name("t-9"), "codex · other-project · latest conversation");
    assert_eq!(name("t-10"), "codex · other-project");
}

#[test]
fn a_terminal_that_never_reported_gets_no_row() {
    let mut studio = Studio::default();

    listed(&mut studio, &[term("t-7", None, None, T0)], T0);
    assert!(studio.sessions().is_empty());

    // A row goes when its status clears, or its terminal leaves the list.
    listed(
        &mut studio,
        &[term("t-7", Some("working"), Some("codex"), T0)],
        T0,
    );
    listed(
        &mut studio,
        &[term("t-7", None, None, T0 + 1_000)],
        T0 + 1_000,
    );
    assert!(studio.sessions().is_empty());
    listed(
        &mut studio,
        &[term("t-7", Some("working"), Some("codex"), T0)],
        T0,
    );
    listed(&mut studio, &[], T0 + 2_000);
    assert!(studio.sessions().is_empty());
}

#[test]
fn a_turn_that_ends_and_restarts_between_two_lists_is_your_turn() {
    // The program went `working`, `done`, `working` inside one poll: only `turn` shows it.
    let mut studio = Studio::default();
    let at = |turn, now| {
        [term_at_turn(
            "t-7",
            Some("working"),
            Some("codex"),
            now,
            turn,
        )]
    };

    listed(&mut studio, &at(0, T0), T0);
    let out = listed(&mut studio, &at(1, T0 + 1_000), T0 + 1_000);

    assert_eq!(state(&studio, "t-7"), Some(State::Running));
    assert!(out.contains(&Output::YourTurn {
        session_id: "t-7".into()
    }));
}

#[test]
fn a_row_already_blocked_at_start_is_quiet_and_one_appearing_later_needs_you() {
    // As Claude sessions, which replay quietly at start (danielrispler/Falinks#522), rows the first list finds tell nothing.
    let mut studio = Studio::default();
    let blocked = |id| term(id, Some("blocked"), Some("codex"), T0);

    let start = listed(&mut studio, &[blocked("t-7")], T0);
    assert!(start.is_empty(), "{start:?}");
    assert_eq!(state(&studio, "t-7"), Some(State::NeedsYou));

    let later = listed(&mut studio, &[blocked("t-7"), blocked("t-8")], T0 + 1_000);
    assert_eq!(
        later,
        vec![Output::NeedsYou {
            session_id: "t-8".into(),
            agent: None
        }]
    );
}

#[test]
fn an_unknown_state_leaves_the_row_as_it_was() {
    let mut studio = Studio::default();

    listed(
        &mut studio,
        &[term("t-7", Some("thinking"), Some("codex"), T0)],
        T0,
    );
    assert!(studio.session("t-7").is_none());

    listed(
        &mut studio,
        &[term("t-7", Some("working"), Some("codex"), T0)],
        T0,
    );
    let out = listed(
        &mut studio,
        &[term("t-7", Some("thinking"), Some("codex"), T0 + 1_000)],
        T0 + 1_000,
    );
    assert_eq!(state(&studio, "t-7"), Some(State::Running));
    assert!(out.is_empty(), "{out:?}");
}

fn replay() -> Studio {
    Studio::replayed(include_str!("fixtures/two-sessions.jsonl").lines())
}

#[test]
fn a_claude_sessions_state_is_unchanged_by_its_7501_reports() {
    let mut studio = replay();
    let before = state(&studio, IN_STUDIO).unwrap();
    let rows = studio.sessions().len();
    let other = if before == State::NeedsYou {
        "working"
    } else {
        "blocked"
    };

    // Claude Code names itself; a Claude terminal with no `app` is known by its hook session.
    listed(
        &mut studio,
        &[term("t-2", Some(other), Some("claude-code"), T0)],
        T0,
    );
    listed(
        &mut studio,
        &[term("t-2", Some(other), None, T0)],
        T0 + 1_000,
    );

    assert_eq!(state(&studio, IN_STUDIO), Some(before));
    assert_eq!(studio.sessions().len(), rows);
    assert!(studio.session("t-2").is_none());
}

fn mismatches(outputs: &[Output]) -> Vec<&str> {
    outputs
        .iter()
        .filter_map(|o| match o {
            Output::Mismatch { line } => Some(line.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_lasting_disagreement_appends_one_mismatch_line_and_a_brief_one_none() {
    let mut studio = replay();
    let hook = state(&studio, IN_STUDIO).unwrap();
    let other = if hook == State::NeedsYou {
        "working"
    } else {
        "blocked"
    };
    let agrees = match hook {
        State::Running => "working",
        State::NeedsYou => "blocked",
        _ => "done",
    };
    let t2 = |state: &str| term("t-2", Some(state), Some("claude-code"), T0);

    // Brief: back in step within the threshold.
    let mut out = listed(&mut studio, &[t2(other)], T0);
    out.extend(listed(&mut studio, &[t2(other)], T0 + 2_000));
    out.extend(listed(&mut studio, &[t2(agrees)], T0 + 3_000));
    out.extend(listed(&mut studio, &[t2(other)], T0 + 4_000));
    out.extend(listed(&mut studio, &[t2(agrees)], T0 + 8_000));
    assert!(mismatches(&out).is_empty());

    // Lasting: one line, however long it lasts.
    let mut out = Vec::new();
    for s in 0..20 {
        out.extend(listed(&mut studio, &[t2(other)], T0 + 10_000 + s * 1_000));
    }
    let lines = mismatches(&out);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains(IN_STUDIO), "{}", lines[0]);
    assert!(lines[0].contains(&format!("7501 {other}")), "{}", lines[0]);
}
