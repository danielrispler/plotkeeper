//! Seam 1 (#369, Testing Decisions) for notifications (#380): replay hook events live through the
//! core and check its notices. Each belongs to a top-level session and names the subagent it comes
//! from (#369, decision 22).

use studio_core::{Notifications, Output, Studio};

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
    let mut studio = Studio::default();
    for line in TREE.lines() {
        studio.replay(line);
    }

    assert_eq!(studio.notice_body(TREE_SESSION, None), "Falinks e3b1c7a2");
    assert_eq!(
        studio.notice_body(TREE_SESSION, Some(TREE_EXPLORE)),
        "Falinks e3b1c7a2 · Find the session tests"
    );
}

/// The notification due at `now`, from a window that is not focused.
fn due(notifications: &mut Notifications, now: u64) -> Option<(String, String)> {
    notifications.due(now, || false)
}

#[test]
fn notices_within_3_seconds_of_the_first_give_one_notification() {
    let mut notifications = Notifications::default();
    notifications.push("Needs you", "Falinks e3b1c7a2".into(), 10_000);
    notifications.push("New card", "Falinks 5d7f69ab".into(), 11_500);

    assert_eq!(due(&mut notifications, 12_999), None);
    assert_eq!(
        due(&mut notifications, 13_000),
        Some((
            "2 notices".into(),
            "Needs you: Falinks e3b1c7a2\nNew card: Falinks 5d7f69ab".into()
        ))
    );
    assert_eq!(due(&mut notifications, 20_000), None);

    // A later notice starts its own.
    notifications.push(
        "Worker failed",
        "Falinks 5d7f69ab · Implement #355".into(),
        21_000,
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
    notifications.push("Needs you", "Falinks e3b1c7a2".into(), 10_000);

    // Daniel is looking at the list, so the notice is dropped, not held for later.
    assert_eq!(notifications.due(13_000, || true), None);
    assert_eq!(due(&mut notifications, 14_000), None);
}
