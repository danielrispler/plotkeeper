//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for times (danielrispler/Falinks#461, decision 13): replay a scrubbed fixture and check
//! the session's, turn's and step's times, a backgrounded check's real duration, and typical check
//! durations from history. `times.jsonl` holds an earlier session that ran the checks in 6 and 8
//! minutes, then a session that runs them in the foreground in 7, in the background in 8.5 (launch
//! to its `<task-notification>`), and once more, still running at the log's end.

use studio_core::{Span, Studio};

const TIMES: &str = include_str!("fixtures/times.jsonl");
const SESSION: &str = "0b1c2d3e-0000-4a00-8000-00000000b002";
const CHECKS: &str = "cargo run --bin ci-checks";
const T1: u64 = 1_793_503_600_000;
const MIN: u64 = 60_000;

/// The studio after the fixture's first `n` lines.
fn first(n: usize) -> Studio {
    Studio::replayed(TIMES.lines().take(n))
}

fn span(start: u64, end: Option<u64>) -> Option<Span> {
    Some(Span { start, end })
}

#[test]
fn session_turn_and_step_times_match_the_fixtures_timestamps() {
    // Up to the Read's start: the turn and the step run.
    let studio = first(10);
    let session = studio.session(SESSION).unwrap();
    assert_eq!(session.agent.started_at, T1);
    assert_eq!(session.agent.last_event_at, T1 + 8_000);
    assert_eq!(session.turn_span, span(T1 + 5_000, None));
    assert_eq!(session.agent.step_span, span(T1 + 8_000, None));

    // Its result ends the step.
    let studio = first(11);
    let session = studio.session(SESSION).unwrap();
    assert_eq!(session.agent.step_span, span(T1 + 8_000, Some(T1 + 9_000)));

    // `Stop` ends the turn; the wake-up opens the next one.
    let studio = first(16);
    let session = studio.session(SESSION).unwrap();
    assert_eq!(session.turn_span, span(T1 + 5_000, Some(T1 + 630_000)));
    let studio = first(18);
    let session = studio.session(SESSION).unwrap();
    assert_eq!(session.turn_span, span(T1 + 1_110_000, None));
    assert_eq!(session.agent.step_span, span(T1 + 1_140_000, None));
}

#[test]
fn a_backgrounded_check_runs_from_its_launch_to_its_completion_notification() {
    // Its call returns in about a second, and the check still runs.
    let studio = first(16);
    let line = &studio.session(SESSION).unwrap().checks[1];
    assert_eq!(
        (line.started_at, line.ended_at, line.passed),
        (T1 + 600_000, None, None)
    );

    let studio = first(17);
    let line = &studio.session(SESSION).unwrap().checks[1];
    assert_eq!(line.command, CHECKS);
    assert_eq!(line.started_at, T1 + 600_000);
    assert_eq!(line.ended_at, Some(T1 + 1_110_000));
    assert_eq!(line.passed, Some(true));
}

#[test]
fn typical_durations_come_from_history_and_a_running_check_shows_them() {
    // The earlier session's 6 and 8 minutes, before this session ran any.
    assert_eq!(first(7).typical(CHECKS), Some(6 * MIN));
    // Then 6, 7, 8 and 8.5 minutes, the backgrounded run's included.
    let studio = Studio::replayed(TIMES.lines());
    assert_eq!(studio.typical(CHECKS), Some(7 * MIN));
    assert_eq!(studio.typical("cargo test"), None);

    let checks = &studio.session(SESSION).unwrap().checks;
    let running = checks.last().unwrap();
    assert_eq!(
        (running.started_at, running.ended_at),
        (T1 + 1_140_000, None)
    );
    assert_eq!(running.typical_ms, Some(7 * MIN));
    // The first run had only the earlier session's history.
    assert_eq!(checks[0].typical_ms, Some(6 * MIN));
    assert_eq!(checks[0].ended_at, Some(T1 + 440_000));
}
