//! Seam 1 (#369, Testing Decisions) for the subagent tree (#372): replay hook events through the core
//! and check each session's subagents: who spawned them, worker or helper, their status and result,
//! and the waits they raise on their session.

use serde_json::json;
use std::collections::BTreeMap;
use studio_core::{Asker, Base, Change, Kind, Output, Parent, State, Step, Studio, Subagent};

use Kind::{Helper, Worker};
use State::{NeedsYou, Running, YourTurn};

fn replay(log: &str) -> Studio {
    let mut studio = Studio::default();
    for line in log.lines() {
        studio.replay(line);
    }
    studio
}

/// The checkpoint day of #347, scrubbed (#370).
const DAY: &str = include_str!("fixtures/checkpoint-day.jsonl");

#[test]
fn the_days_subagents_are_5_workers_7_helpers_and_1_unattached_agent() {
    let studio = replay(DAY);

    let mut tree: Vec<(&str, &str, Kind, Parent)> = studio
        .sessions()
        .into_iter()
        .flat_map(|s| s.agents.iter().map(|a| (&s.id[..8], a)))
        .map(|(session, a)| (session, &a.id[..5], a.kind, a.parent.clone()))
        .collect();
    tree.sort();

    // By decision 6: 4 workers by their worktree, and the research helper of `2afc85f5` by its Write.
    // `a4376` has one Bash step and no known spawn.
    let session = || Parent::Session;
    assert_eq!(
        tree,
        [
            ("0222b62d", "a5532", Helper, session()),
            ("0222b62d", "ac32f", Helper, session()),
            ("2afc85f5", "a5824", Helper, session()),
            ("2afc85f5", "aaf57", Worker, session()),
            ("596c28a1", "a66b1", Helper, session()),
            ("596c28a1", "a6838", Helper, session()),
            ("5d7f69ab", "a1191", Worker, session()),
            ("5d7f69ab", "a2526", Worker, session()),
            ("5d7f69ab", "a3c3f", Helper, session()),
            ("5d7f69ab", "a4376", Helper, Parent::Unattached),
            ("5d7f69ab", "a96cd", Worker, session()),
            ("5d7f69ab", "ac088", Worker, session()),
            ("5d7f69ab", "aed99", Helper, session()),
        ]
    );
}

/// The scripted session of `record-subagent-run.sh` (#370): a worktree worker writes and commits a
/// file, and a helper is stopped mid-step.
const RUN: &str = include_str!("fixtures/subagent-run.jsonl");
const RUN_SESSION: &str = "40e8e6ef-9b2f-4467-9b55-87c2d053b616";
const RUN_WORKER: &str = "add0c7bbaced40ed0";
const RUN_HELPER: &str = "a551b1ece0a4430ce";

/// A subagent of the session, by id.
fn agent<'a>(studio: &'a Studio, session: &str, id: &str) -> &'a Subagent {
    let session = studio.session(session).expect("the session");
    session
        .agents
        .iter()
        .find(|a| a.id == id)
        .expect("the agent")
}

#[test]
fn the_runs_worker_completes_with_its_last_message_and_its_killed_helper_stays_visible() {
    let mut studio = replay(RUN);

    let worker = agent(&studio, RUN_SESSION, RUN_WORKER);
    assert_eq!(
        (worker.kind, &worker.parent, worker.description.as_deref()),
        (Worker, &Parent::Session, Some("Add hello file"))
    );
    // It ends at its `SubagentStop` (line 18), with that event's last message, scrubbed to `…`; its
    // wake-up (line 22) says `completed`.
    assert_eq!(
        (
            worker.status.as_str(),
            worker.ended_at,
            worker.result.as_deref()
        ),
        ("completed", Some(1_793_471_826_289), Some("…"))
    );
    let helper = agent(&studio, RUN_SESSION, RUN_HELPER);
    assert_eq!(
        (
            helper.kind,
            helper.agent_type.as_deref(),
            helper.description.as_deref()
        ),
        (Helper, Some("general-purpose"), Some("Wait and report"))
    );
    // Only its wake-up (line 25) tells it was killed; it has no `SubagentStop` and no result.
    assert_eq!(
        (
            helper.status.as_str(),
            helper.ended_at,
            helper.result.as_deref()
        ),
        ("killed", Some(1_793_471_842_241), None)
    );
    assert_eq!(helper.started_at, 1_793_471_818_599);
    // The pane draws it as failed from the core's flag, not from its status.
    assert!(helper.failed);
    assert!(!helper.faded, "a killed helper stays on its parent's line");

    studio.open(RUN_HELPER);

    assert!(agent(&studio, RUN_SESSION, RUN_HELPER).faded);
}

#[test]
fn a_helper_that_succeeds_fades_and_a_worker_keeps_its_row() {
    let studio = replay(DAY);

    // The review helper handed back (line 530); the session's next `Stop` (line 534) no longer lists
    // it among its running tasks.
    let review = agent(
        &studio,
        "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b",
        "aed99c8576ffec532",
    );
    assert_eq!(
        (review.status.as_str(), review.ended_at, review.faded),
        ("completed", Some(1_793_376_171_071), true)
    );
    let worker = agent(
        &studio,
        "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b",
        "a1191df87fb730aa0",
    );
    assert_eq!((worker.status.as_str(), worker.faded), ("completed", false));
}

#[test]
fn a_worker_its_parent_continues_runs_again_until_its_next_wake_up() {
    let lines: Vec<&str> = DAY.lines().collect();
    let status = |upto: usize| {
        let studio = replay(&lines[..upto].join("\n"));
        let worker = agent(
            &studio,
            "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b",
            "a1191df87fb730aa0",
        );
        (worker.status.clone(), worker.ended_at)
    };

    // Its first wake-up (line 378), then the parent's `SendMessage` (line 442) and its next steps.
    assert_eq!(status(378), ("completed".into(), Some(1_793_364_970_106)));
    assert_eq!(status(471), ("running".into(), None));
    assert_eq!(status(482), ("completed".into(), Some(1_793_375_994_142)));
}

/// Hand-written: the session spawns a worktree worker and a review helper; the worker spawns an
/// `Explore` helper, whose test command waits on Daniel's permission across the session's `Stop`;
/// the review helper then edits a file.
const TREE: &str = include_str!("fixtures/subagent-tree.jsonl");
const TREE_SESSION: &str = "e3b1c7a2-5d84-4f19-9a6e-2c8d0f4b7e15";
const TREE_WORKER: &str = "a7e2c4f6b8d0a1c3e";
const TREE_REVIEW: &str = "a3f8b2d6c1e9a7f40";
const TREE_EXPLORE: &str = "a5d9f1b3c7e2a4b6d";

/// The studio after the first `upto` lines of `TREE`.
fn tree_upto(upto: usize) -> Studio {
    let lines: Vec<&str> = TREE.lines().collect();
    replay(&lines[..upto].join("\n"))
}

#[test]
fn a_subagent_of_a_subagent_sits_under_it() {
    let studio = replay(TREE);

    let session = studio.session(TREE_SESSION).expect("the session");
    let tree: Vec<(&str, &Parent, Option<&str>, &str)> = session
        .agents
        .iter()
        .map(|a| {
            (
                a.id.as_str(),
                &a.parent,
                a.agent_type.as_deref(),
                a.status.as_str(),
            )
        })
        .collect();
    let worker = Parent::Agent(TREE_WORKER.into());
    assert_eq!(
        tree,
        [
            (
                TREE_WORKER,
                &Parent::Session,
                Some("general-purpose"),
                "running"
            ),
            (
                TREE_REVIEW,
                &Parent::Session,
                Some("general-purpose"),
                "running"
            ),
            (TREE_EXPLORE, &worker, Some("Explore"), "completed"),
        ]
    );
    let explore = agent(&studio, TREE_SESSION, TREE_EXPLORE);
    assert_eq!(
        explore.result.as_deref(),
        Some("The session tests are in core/tests/sessions.rs, and they pass.")
    );
}

#[test]
fn a_helper_that_edits_a_file_is_a_worker_from_that_step() {
    let kind = |upto| agent(&tree_upto(upto), TREE_SESSION, TREE_REVIEW).kind;

    // Its Read (lines 16 and 17), then its Edit's start (line 18) and result (line 19).
    assert_eq!([kind(17), kind(18), kind(19)], [Helper, Helper, Worker]);
}

#[test]
fn a_helper_whose_command_changed_its_worktree_is_a_worker_from_that_step() {
    let mut studio = tree_upto(17);
    let command = json!({
        "session_id": TREE_SESSION,
        "hook_event_name": "PostToolUse",
        "agent_id": TREE_REVIEW,
        "agent_type": "general-purpose",
        "cwd": "/Users/danielrispler/work/Falinks",
        "tool_name": "Bash",
        "tool_input": {"command": "cargo fmt", "description": "Format the code"},
        "tool_response": {"interrupted": false, "stderr": "", "stdout": ""},
        "tool_use_id": "toolu_01TreeHelperFormat000000007",
        "studio_received_ms": 1_793_480_193_500_u64,
    })
    .to_string();

    // The shell's fingerprint after the step: nothing changed, then a file changed.
    studio.apply_with_changes(&command, BTreeMap::new());
    assert_eq!(agent(&studio, TREE_SESSION, TREE_REVIEW).kind, Helper);
    let change = Change {
        base: Base::default(),
        diff: "+fn element<'a>\n".into(),
        head: None,
    };
    studio.apply_with_changes(&command, BTreeMap::from([(TREE_REVIEW.into(), change)]));
    assert_eq!(agent(&studio, TREE_SESSION, TREE_REVIEW).kind, Worker);
}

#[test]
fn a_childs_check_adds_no_line_to_its_session() {
    // The prompt (line 2) opens the wait, then the worker runs a check in its worktree.
    let mut studio = tree_upto(2);
    let check = |hook: &str| {
        json!({
            "session_id": TREE_SESSION,
            "hook_event_name": hook,
            "agent_id": TREE_WORKER,
            "agent_type": "general-purpose",
            "cwd": "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-372",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test", "description": "Run the tests"},
            "tool_response": {"interrupted": false, "stderr": "", "stdout": "ok"},
            "tool_use_id": "toolu_01TreeWorkerCheck0000000009",
            "studio_received_ms": 1_793_480_193_600_u64,
        })
        .to_string()
    };

    let outputs = [
        studio.apply(&check("PreToolUse")),
        studio.apply(&check("PostToolUse")),
    ];

    assert_eq!(outputs, [vec![], vec![]]);
    let session = studio.session(TREE_SESSION).expect("the session");
    assert_eq!(session.checks, []);
}

#[test]
fn a_childs_permission_request_makes_its_session_need_you_until_its_step_resolves() {
    let states: Vec<(State, Option<Asker>)> = (1..=TREE.lines().count())
        .map(|upto| {
            let studio = tree_upto(upto);
            let session = studio.session(TREE_SESSION).expect("the session");
            (session.state, session.asker.clone())
        })
        .collect();

    let explore = Some(Asker {
        agent: Some(TREE_EXPLORE.into()),
        since: 1_793_480_012_050,
    });
    let mut expected = vec![(YourTurn, None)];
    expected.extend(vec![(Running, None); 11]);
    // The Explore helper's PermissionRequest, then the session's `Stop`, which leaves the child's
    // wait in place.
    expected.extend([(NeedsYou, explore.clone()), (NeedsYou, explore)]);
    // Its command's result clears it.
    expected.extend(vec![(YourTurn, None); 6]);
    assert_eq!(states, expected);

    // One notice, on the session's line, naming the grandchild who asks (#369, decision 22). The
    // `Stop` while it waits gives no second one.
    let mut studio = Studio::default();
    let notices = TREE.lines().flat_map(|line| studio.apply(line));
    let notices: Vec<Output> = notices
        .filter(|o| matches!(o, Output::NeedsYou { .. }))
        .collect();
    assert_eq!(
        notices,
        [Output::NeedsYou {
            session_id: TREE_SESSION.into(),
            agent: Some(TREE_EXPLORE.into()),
        }]
    );
}

#[test]
fn a_child_that_ends_while_it_waits_clears_its_wait() {
    let lines: Vec<&str> = TREE.lines().collect();
    // The Explore helper asks (line 13) and ends (line 20) with no result for its command.
    let log = [&lines[..13], &lines[19..]].concat().join("\n");

    let studio = replay(&log);

    let session = studio.session(TREE_SESSION).expect("the session");
    assert_eq!((session.state, &session.asker), (Running, &None));
}

#[test]
fn a_childs_steps_are_its_own_not_its_sessions() {
    let lines: Vec<&str> = RUN.lines().collect();
    // Up to the worker's Write (line 11), after the session's own ToolSearch (lines 9 and 10).
    let studio = replay(&lines[..11].join("\n"));

    let session = studio.session(RUN_SESSION).expect("the session");
    let own = Step {
        tool: "ToolSearch".into(),
        detail: Some("…".into()),
    };
    assert_eq!(session.last_step.as_ref(), Some(&own));
    assert_eq!(
        session.cwd,
        "/private/var/folders/fx/plr3nrp54xbfybl8_00tqm5r0000gn/T/tmp.61uRkuHpH8"
    );
    let write = Step {
        tool: "Write".into(),
        detail: Some("hello.txt".into()),
    };
    assert_eq!(
        agent(&studio, RUN_SESSION, RUN_WORKER).last_step,
        Some(write)
    );
}
