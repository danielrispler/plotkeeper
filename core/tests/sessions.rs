//! Seam 1 (#369, Testing Decisions) for the session list (#371): replay hook events through the core,
//! with the shell's ticks at explicit times, and check each session's state, the list and the
//! selection.

use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use studio_core::{State, Studio};

use State::{Ended, Idle, NeedsYou, Running, YourTurn};

const HOUR: u64 = 60 * 60 * 1000;

fn replay(log: &str) -> Studio {
    let mut studio = Studio::default();
    for line in log.lines() {
        studio.replay(line);
    }
    studio
}

/// One prompt and a failed command, ending at its `Stop`.
const PROBE: &str = "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31";
const PROBE_STOPPED_AT: u64 = 1_793_260_011_904;

/// An event of session `session`, received at `at`.
fn event(session: &str, name: &str, at: u64) -> Value {
    json!({
        "session_id": session,
        "hook_event_name": name,
        "cwd": "/Users/danielrispler/work/Falinks",
        "studio_received_ms": at,
    })
}

fn state(studio: &Studio, session: &str) -> Option<State> {
    studio.session(session).map(|s| s.state)
}

#[test]
fn a_session_left_at_your_turn_for_2_hours_is_idle_at_the_next_tick() {
    let mut studio = replay(include_str!("fixtures/failing-bash.jsonl"));

    studio.tick(PROBE_STOPPED_AT + 2 * HOUR - 1);
    assert_eq!(state(&studio, PROBE), Some(YourTurn));
    studio.tick(PROBE_STOPPED_AT + 2 * HOUR);
    assert_eq!(state(&studio, PROBE), Some(Idle));
}

#[test]
fn an_idle_sessions_next_event_brings_it_back() {
    let later = PROBE_STOPPED_AT + 3 * HOUR;
    let idle = || {
        let mut studio = replay(include_str!("fixtures/failing-bash.jsonl"));
        studio.tick(later);
        studio
    };

    // An event that starts no turn, such as Claude Code's idle reminder, leaves it at your turn.
    let mut reminded = idle();
    let mut reminder = event(PROBE, "Notification", later + 1);
    reminder["notification_type"] = "idle_prompt".into();
    reminded.apply(&reminder.to_string());
    assert_eq!(state(&reminded, PROBE), Some(YourTurn));
    // The 2 hours count again from that event.
    reminded.tick(later + 2 * HOUR);
    assert_eq!(state(&reminded, PROBE), Some(YourTurn));
    reminded.tick(later + 1 + 2 * HOUR);
    assert_eq!(state(&reminded, PROBE), Some(Idle));

    let mut prompted = idle();
    prompted.apply(&event(PROBE, "UserPromptSubmit", later + 1).to_string());
    assert_eq!(state(&prompted, PROBE), Some(Running));
    // A running session never goes idle.
    prompted.tick(later + 5 * HOUR);
    assert_eq!(state(&prompted, PROBE), Some(Running));
}

/// The checkpoint day of #347, scrubbed (#370): 8 sessions, 5 of them with a `SessionEnd`, cut at
/// midnight, 2026-10-31 00:00 in Israel.
const DAY: &str = include_str!("fixtures/checkpoint-day.jsonl");
const MIDNIGHT: u64 = 1_793_397_600_000;

/// A change of state: from (`None` for a new session), to, and its cause, an event's
/// `hook_event_name` or `tick`.
type Change = (Option<State>, State, String);

fn states(studio: &Studio) -> HashMap<String, State> {
    let sessions = studio.sessions().into_iter();
    sessions.map(|s| (s.id.clone(), s.state)).collect()
}

/// Every change of state in `DAY`, when its lines arrive live and the shell ticks before each line
/// at the line's time, as its read loop does, and once more at midnight.
fn day_changes(studio: &mut Studio) -> BTreeSet<Change> {
    let mut changes = BTreeSet::new();
    let mut record = |studio: &Studio, before: HashMap<String, State>, cause: &str| {
        for (id, after) in states(studio) {
            let from = before.get(&id).copied();
            if from != Some(after) {
                changes.insert((from, after, cause.to_string()));
            }
        }
    };
    for line in DAY.lines() {
        let event: Value = serde_json::from_str(line).unwrap();
        let before = states(studio);
        studio.tick(event["studio_received_ms"].as_u64().unwrap());
        record(studio, before, "tick");
        let before = states(studio);
        studio.apply(line);
        record(studio, before, event["hook_event_name"].as_str().unwrap());
    }
    let before = states(studio);
    studio.tick(MIDNIGHT);
    record(studio, before, "tick");
    changes
}

#[test]
fn the_days_sessions_change_state_only_by_their_named_setters_and_clearers() {
    let mut studio = Studio::default();

    let changes = day_changes(&mut studio);

    let named = [
        // A session starts at your turn, before its first prompt.
        (None, YourTurn, "SessionStart"),
        (Some(YourTurn), Running, "UserPromptSubmit"),
        (Some(Running), YourTurn, "Stop"),
        // An `AskUserQuestion` or `ExitPlanMode`, and a `Stop` that asks.
        (Some(Running), NeedsYou, "PreToolUse"),
        (Some(Running), NeedsYou, "Stop"),
        // The step that asked resolves, or Daniel prompts.
        (Some(NeedsYou), Running, "PostToolUse"),
        (Some(NeedsYou), Running, "UserPromptSubmit"),
        (Some(YourTurn), Idle, "tick"),
        (Some(Idle), Running, "UserPromptSubmit"),
        (Some(YourTurn), Ended, "SessionEnd"),
    ];
    let named: [Change; 10] = named.map(|(from, to, cause)| (from, to, cause.into()));
    assert_eq!(changes, BTreeSet::from(named));
}

#[test]
fn an_ended_session_stays_ended_until_it_starts_again_under_its_id() {
    let mut studio = replay(include_str!("fixtures/failing-bash.jsonl"));
    let end = PROBE_STOPPED_AT + 1_000;

    studio.apply(&event(PROBE, "SessionEnd", end).to_string());
    assert_eq!(state(&studio, PROBE), Some(Ended));
    // The hooks are async, so a turn's last event can land after its end.
    studio.apply(&event(PROBE, "Notification", end + 1).to_string());
    studio.tick(end + 3 * HOUR);
    assert_eq!(state(&studio, PROBE), Some(Ended));

    let mut resumed = event(PROBE, "SessionStart", end + 4 * HOUR);
    resumed["source"] = "resume".into();
    studio.apply(&resumed.to_string());
    assert_eq!(state(&studio, PROBE), Some(YourTurn));
}

#[test]
fn at_midnight_the_days_8_sessions_are_5_ended_2_at_your_turn_and_1_idle() {
    let mut studio = Studio::default();
    day_changes(&mut studio);

    let states: Vec<(&str, State)> = studio
        .sessions()
        .into_iter()
        .map(|s| (&s.id[..8], s.state))
        .collect();

    // In list order: the groups by state, the newest session first in each.
    assert_eq!(
        states,
        [
            ("0222b62d", YourTurn),
            ("596c28a1", YourTurn),
            ("2afc85f5", Idle),
            ("5bb84a4a", Ended),
            ("57e9c1ab", Ended),
            ("6641e809", Ended),
            ("1ac172be", Ended),
            ("5d7f69ab", Ended),
        ]
    );
}

#[test]
fn the_list_shows_needs_you_first_then_running_then_ended() {
    let mut studio = Studio::default();
    for line in DAY.lines().take(729) {
        let event: Value = serde_json::from_str(line).unwrap();
        studio.tick(event["studio_received_ms"].as_u64().unwrap());
        studio.apply(line);
    }

    let list: Vec<(&str, State)> = studio
        .sessions()
        .into_iter()
        .map(|s| (&s.id[..8], s.state))
        .collect();

    // The ended session started between the other two.
    assert_eq!(
        list,
        [
            ("2afc85f5", NeedsYou),
            ("5d7f69ab", Running),
            ("1ac172be", Ended)
        ]
    );
}

#[test]
fn the_selection_holds_all_day_while_its_session_changes_group_and_others_are_prompted() {
    let mut studio = Studio::default();
    let first = "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b";
    let mut groups = BTreeSet::new();

    for line in DAY.lines() {
        studio.apply(line);
        if studio.session(first).is_some() && studio.selected().is_none() {
            studio.select(first);
        }
        groups.insert(studio.selected().map(|s| s.state));
    }

    assert_eq!(studio.selected().map(|s| s.id.as_str()), Some(first));
    assert_eq!(
        groups,
        BTreeSet::from([NeedsYou, Running, YourTurn, Ended].map(Some))
    );
}
