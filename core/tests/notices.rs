//! Seam 1 (#369, Testing Decisions) for notifications (#380): replay hook events live through the
//! core and check its notices. Each belongs to a top-level session and names the subagent it comes
//! from (#369, decision 22).

use std::time::Duration;
use studio_core::{Notifications, Output, State, Studio};

/// The scripted session of `record-subagent-run.sh` (#370): a worktree worker completes, and a
/// helper is stopped mid-step (line 25 says `killed`).
const RUN: &str = include_str!("fixtures/subagent-run.jsonl");
const RUN_SESSION: &str = "40e8e6ef-9b2f-4467-9b55-87c2d053b616";
const RUN_HELPER: &str = "a551b1ece0a4430ce";

/// The notices other than "needs you" when the log's lines arrive live.
fn failures(log: &str) -> Vec<Output> {
    let mut studio = Studio::default();
    let outputs = log.lines().flat_map(|line| studio.apply(line));
    outputs
        .filter(|o| matches!(o, Output::Failed { .. }))
        .collect()
}

#[test]
fn a_failed_helper_raises_no_notice() {
    assert_eq!(failures(RUN), []);
}

#[test]
fn a_failed_worker_raises_one_notice_on_its_sessions_line_naming_it() {
    // The same run, with the stopped subagent spawned in its own worktree (lines 6 and 7).
    let log = RUN.replace(
        r#""description":"Wait and report","prompt""#,
        r#""description":"Wait and report","isolation":"worktree","prompt""#,
    );

    assert_eq!(
        failures(&log),
        [Output::Failed {
            session_id: RUN_SESSION.into(),
            agent: RUN_HELPER.into(),
        }]
    );
}

/// Hand-written (#379): a background helper asks a question across its session's `Stop`.
const CHILD: &str = include_str!("fixtures/child-question.jsonl");

#[test]
fn a_childs_question_raises_one_needs_you_naming_the_child() {
    let mut studio = Studio::default();
    let outputs = CHILD.lines().flat_map(|line| studio.apply(line));
    let notices: Vec<Output> = outputs
        .filter(|o| matches!(o, Output::NeedsYou { .. } | Output::Failed { .. }))
        .collect();

    assert_eq!(
        notices,
        [Output::NeedsYou {
            session_id: "b8d4f2a6-3c71-4e9b-8a05-6f1d2c7e9b43".into(),
            agent: Some("a2c6e8f0b4d1a3c5e".into()),
        }]
    );
}

/// Hand-written: a worktree worker's `Explore` helper waits on Daniel's permission.
const TREE: &str = include_str!("fixtures/subagent-tree.jsonl");
const TREE_SESSION: &str = "e3b1c7a2-5d84-4f19-9a6e-2c8d0f4b7e15";
const TREE_EXPLORE: &str = "a5d9f1b3c7e2a4b6d";

#[test]
fn a_notice_says_its_sessions_folder_and_short_id_and_the_child_it_names() {
    let studio = Studio::replayed(TREE.lines());

    assert_eq!(studio.notice_body(TREE_SESSION, None), "Falinks e3b1c7a2");
    assert_eq!(
        studio.notice_body(TREE_SESSION, Some(TREE_EXPLORE)),
        "Falinks e3b1c7a2 · Find the session tests"
    );
}

#[test]
fn a_notice_names_a_child_with_no_description_by_its_short_id() {
    let log = TREE.replace(r#""description":"Find the session tests","#, "");
    let studio = Studio::replayed(log.lines());

    assert_eq!(
        studio.notice_body(TREE_SESSION, Some(TREE_EXPLORE)),
        "Falinks e3b1c7a2 · a5d9f"
    );
}

#[test]
fn a_folder_ending_in_a_slash_shows_its_whole_cwd() {
    let log = TREE.replace(
        r#""cwd":"/Users/danielrispler/work/Falinks""#,
        r#""cwd":"/Users/danielrispler/work/Falinks/""#,
    );
    let studio = Studio::replayed(log.lines());

    assert_eq!(
        studio.notice_body(TREE_SESSION, None),
        "/Users/danielrispler/work/Falinks/ e3b1c7a2"
    );
    let session = studio.session(TREE_SESSION).expect("the session");
    assert_eq!(session.folder_name, "/Users/danielrispler/work/Falinks/");
}

/// `ms` milliseconds after the app started, on the monotonic clock the notices are combined on
/// (#434, decision 28).
fn at(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// The notification due at `ms`, from a window that is not focused, with no session needing Daniel.
fn due(notifications: &mut Notifications, ms: u64) -> Option<(String, String)> {
    notifications
        .due(at(ms), || false, |_| false, || None)
        .notification
}

#[test]
fn notices_within_3_seconds_of_the_first_give_one_notification() {
    let mut notifications = Notifications::default();
    notifications.push("Worker failed", "Falinks e3b1c7a2".into(), at(10_000));
    notifications.push("New card", "Falinks 5d7f69ab".into(), at(11_500));

    assert_eq!(due(&mut notifications, 12_999), None);
    assert_eq!(
        due(&mut notifications, 13_000),
        Some((
            "2 notices".into(),
            "Worker failed: Falinks e3b1c7a2\nNew card: Falinks 5d7f69ab".into()
        ))
    );
    assert_eq!(due(&mut notifications, 20_000), None);

    // A later notice starts its own.
    notifications.push(
        "Worker failed",
        "Falinks 5d7f69ab · Implement #355".into(),
        at(21_000),
    );
    assert_eq!(
        due(&mut notifications, 24_000),
        Some((
            "Worker failed".into(),
            "Falinks 5d7f69ab · Implement #355".into()
        ))
    );
}

#[test]
fn a_focused_window_gets_no_notification() {
    let mut notifications = Notifications::default();
    notifications.push("New card", "Falinks e3b1c7a2".into(), at(10_000));

    // Daniel is looking at the list, so the notice is dropped, not held for later.
    assert_eq!(
        notifications
            .due(at(13_000), || true, |_| false, || None)
            .notification,
        None
    );
    assert_eq!(due(&mut notifications, 14_000), None);
}

/// `permission-write.jsonl`'s session: a `Write` waits on Daniel's permission (line 3), and he
/// allows it (line 5).
const PERMISSION: &str = include_str!("fixtures/permission-write.jsonl");

/// The notification due 3 seconds after the log's first `upto` lines arrive live, each queued as
/// the shell queues it, with the window unfocused.
fn needs_you_notification(upto: usize) -> Option<(String, String)> {
    let mut studio = Studio::default();
    let mut notifications = Notifications::default();
    for line in PERMISSION.lines().take(upto) {
        for output in studio.apply(line) {
            if let Output::NeedsYou { session_id, agent } = output {
                let body = studio.notice_body(&session_id, agent.as_deref());
                notifications.push_needs_you(&session_id, body, at(1_000));
            }
        }
    }
    let needs_you = |id: &str| {
        studio
            .session(id)
            .is_some_and(|s| s.state == State::NeedsYou)
    };
    notifications
        .due(at(4_000), || false, needs_you, || None)
        .notification
}

#[test]
fn a_needs_you_notice_shows_while_its_session_still_needs_you() {
    assert_eq!(
        needs_you_notification(3),
        Some(("Needs you".into(), "Falinks 7f3c2a9e".into()))
    );
}

#[test]
fn a_permission_answered_within_3_seconds_shows_no_needs_you_notice() {
    assert_eq!(needs_you_notification(5), None);
}

/// `two-sessions.jsonl`'s sessions, both in a turn at its end: the first in `Falinks`, the second in
/// the `issue-322-terminals` worktree.
const TWO: &str = include_str!("fixtures/two-sessions.jsonl");
const FIRST: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
const SECOND: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";

/// Session `id`'s `Stop`, under the prompt id its turn runs under.
fn stop(id: &str) -> String {
    let prompt_id = match id {
        FIRST => "e2a7c9d1-3f5b-4a8e-9c6d-0b1a2f3e4d5c",
        _ => "71c3e5a9-b2d4-4f6e-8a0c-1e3d5f7a9b2c",
    };
    format!(
        r#"{{"session_id":"{id}","prompt_id":"{prompt_id}","hook_event_name":"Stop","stop_hook_active":false,"studio_received_ms":1793261020000}}"#
    )
}

/// The notification due 3 seconds after the sessions `stopped` reach "your turn" live, each queued
/// as the shell queues it, with the first session selected, the window `focused` or not, and the
/// setting for notifying while focused `while_focused`.
fn your_turn_notification(
    stopped: &[&str],
    focused: bool,
    while_focused: bool,
) -> Option<(String, String)> {
    let mut studio = Studio::replayed(TWO.lines());
    studio.select(FIRST);
    let mut notifications = Notifications::default();
    notifications.while_focused = while_focused;
    for id in stopped {
        for output in studio.apply(&stop(id)) {
            if let Output::YourTurn { session_id } = output {
                let body = studio.notice_body(&session_id, None);
                notifications.push_your_turn(&session_id, body, at(1_000));
            }
        }
    }
    let selected = studio.selected().map(|s| s.agent.id.clone());
    notifications
        .due(at(4_000), || focused, |_| false, || selected)
        .notification
}

#[test]
fn a_session_starting_raises_no_your_turn() {
    // Line 4 is the second session's `SessionStart`: at your turn, with no turn ended.
    let mut studio = Studio::default();
    let outputs = TWO.lines().flat_map(|line| studio.apply(line));
    assert!(
        !outputs
            .into_iter()
            .any(|o| matches!(o, Output::YourTurn { .. }))
    );
}

#[test]
fn your_turn_notifies_while_not_focused_for_the_selected_session_too() {
    assert_eq!(
        your_turn_notification(&[FIRST], false, false),
        Some(("Your turn".into(), "Falinks c41d8e27".into()))
    );
}

#[test]
fn your_turn_does_not_notify_while_focused_by_default() {
    assert_eq!(your_turn_notification(&[FIRST, SECOND], true, false), None);
}

#[test]
fn your_turn_notifies_while_focused_with_the_setting_on_for_another_session_only() {
    assert_eq!(
        your_turn_notification(&[FIRST, SECOND], true, true),
        Some(("Your turn".into(), "issue-322-terminals 9e0b5f13".into()))
    );
}

#[test]
fn two_sessions_reaching_your_turn_together_give_one_notification() {
    assert_eq!(
        your_turn_notification(&[FIRST, SECOND], false, false),
        Some((
            "2 notices".into(),
            "Your turn: Falinks c41d8e27\nYour turn: issue-322-terminals 9e0b5f13".into()
        ))
    );
}
