//! Seam 1 (#369, Testing Decisions) for the subagent tree (#372): replay hook events through the core
//! and check each session's subagents: who spawned them, worker or helper, their status and result,
//! and the waits they raise on their session.

use serde_json::json;
use std::collections::BTreeMap;
use studio_core::{
    Asker, Base, Change, Fingerprint, Kind, Output, Parent, Purpose, State, Status, Step, Studio,
    Subagent,
};

use Kind::{Helper, Worker};
use State::{NeedsYou, Running, YourTurn};

fn replay(log: &str) -> Studio {
    Studio::replayed(log.lines())
}

/// The checkpoint day of #347, scrubbed (#370).
const DAY: &str = include_str!("fixtures/checkpoint-day.jsonl");

#[test]
fn the_days_subagents_are_5_workers_7_helpers_and_1_unattached_agent() {
    let studio = replay(DAY);

    let mut tree: Vec<(&str, &str, Kind, Parent)> = studio
        .sessions()
        .into_iter()
        .flat_map(|s| s.agents.iter().map(|a| (&s.agent.id[..8], a)))
        .map(|(session, a)| (session, &a.agent.id[..5], a.kind, a.parent.clone()))
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
        .find(|a| a.agent.id == id)
        .expect("the agent")
}

#[test]
fn the_runs_worker_completes_with_its_last_message_and_its_killed_helper_stays_visible() {
    let mut studio = replay(RUN);

    let worker = agent(&studio, RUN_SESSION, RUN_WORKER);
    assert_eq!(
        (worker.kind, &worker.parent, worker.name.as_str()),
        (Worker, &Parent::Session, "Add hello file")
    );
    // It ends at its `SubagentStop` (line 18), with that event's last message, scrubbed to `…`; its
    // wake-up (line 22) says `completed`.
    assert_eq!(
        (
            worker.status.to_string().as_str(),
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
            helper.name.as_str()
        ),
        (Helper, Some("general-purpose"), "Wait and report")
    );
    // Only its wake-up (line 25) tells it was killed; it has no `SubagentStop` and no result.
    assert_eq!(
        (
            helper.status.to_string().as_str(),
            helper.ended_at,
            helper.result.as_deref()
        ),
        ("killed", Some(1_793_471_842_241), None)
    );
    assert_eq!(helper.agent.started_at, 1_793_471_818_599);
    // The pane draws it as failed from the core's flag, not from its status.
    assert!(helper.failed());
    assert!(
        !helper.faded(),
        "a killed helper stays on its parent's line"
    );

    studio.open(RUN_HELPER);

    assert!(agent(&studio, RUN_SESSION, RUN_HELPER).faded());
}

/// #434, decision 39: opening a running subagent's line records nothing, so a helper opened while it
/// ran and then killed stays on its parent's line as failed.
#[test]
fn a_helper_opened_while_running_and_then_killed_stays_visible_and_failed() {
    let lines: Vec<&str> = RUN.lines().collect();
    // Up to the wake-up that reports the helper killed (line 25).
    let mut studio = replay(&lines[..24].join("\n"));
    assert!(
        !studio.open(RUN_HELPER),
        "a running line's opening counts for nothing"
    );

    for line in &lines[24..] {
        studio.replay(line);
    }

    let helper = agent(&studio, RUN_SESSION, RUN_HELPER);
    assert!(helper.failed() && !helper.opened && !helper.faded());
    assert!(studio.open(RUN_HELPER), "a failed line's opening counts");
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
        (
            review.status.to_string().as_str(),
            review.ended_at,
            review.faded()
        ),
        ("completed", Some(1_793_376_171_071), true)
    );
    let worker = agent(
        &studio,
        "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b",
        "a1191df87fb730aa0",
    );
    assert_eq!(
        (worker.status.to_string().as_str(), worker.faded()),
        ("completed", false)
    );
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
        (worker.status.to_string(), worker.ended_at)
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
    let tree: Vec<(&str, &Parent, Option<&str>, &Status)> = session
        .agents
        .iter()
        .map(|a| {
            (
                a.agent.id.as_str(),
                &a.parent,
                a.agent_type.as_deref(),
                &a.status,
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
                &Status::Running
            ),
            (
                TREE_REVIEW,
                &Parent::Session,
                Some("general-purpose"),
                &Status::Running
            ),
            (TREE_EXPLORE, &worker, Some("Explore"), &Status::Completed),
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
    assert_eq!(session.agent.last_step.as_ref(), Some(&own));
    assert_eq!(
        session.cwd,
        "/private/var/folders/fx/plr3nrp54xbfybl8_00tqm5r0000gn/T/tmp.61uRkuHpH8"
    );
    let write = Step {
        tool: "Write".into(),
        detail: Some("hello.txt".into()),
    };
    assert_eq!(
        agent(&studio, RUN_SESSION, RUN_WORKER).agent.last_step,
        Some(write)
    );
}

#[test]
fn the_wire_keeps_each_subagents_status_failed_and_faded() {
    let wire = |studio: &Studio, id: &str| {
        let session = studio
            .sessions()
            .into_iter()
            .find(|s| s.agent.id == RUN_SESSION);
        let json = serde_json::to_value(session.unwrap()).unwrap();
        let agents = json["agents"].as_array().unwrap().clone();
        let agent = agents.into_iter().find(|a| a["id"] == id).unwrap();
        (
            agent["status"].clone(),
            agent["failed"].clone(),
            agent["faded"].clone(),
        )
    };
    // The killed wake-up (line 25) names the worker instead of the helper.
    let killed = RUN.replace(
        &format!("<task-id>{RUN_HELPER}</task-id>"),
        &format!("<task-id>{RUN_WORKER}</task-id>"),
    );
    let studio = replay(&killed);
    assert_eq!(
        wire(&studio, RUN_WORKER),
        (json!("killed"), json!(true), json!(false))
    );

    let mut studio = replay(RUN);
    studio.open(RUN_HELPER);
    assert_eq!(
        wire(&studio, RUN_HELPER),
        (json!("killed"), json!(true), json!(true))
    );
}

/// Decision 18 of #410: a session's live keys last until its `SessionEnd`, and come back when it
/// starts again.
#[test]
fn a_session_and_its_subagents_are_live_until_it_ends_and_again_once_it_resumes() {
    let lines: Vec<&str> = include_str!("fixtures/subagent-run.jsonl")
        .lines()
        .collect();
    let (last, before) = lines.split_last().unwrap();
    let session = "40e8e6ef-9b2f-4467-9b55-87c2d053b616";
    let mut studio = Studio::replayed(before);
    let mut expected: Vec<String> = studio
        .session(session)
        .unwrap()
        .agents
        .iter()
        .map(|a| a.agent.id.clone())
        .collect();
    assert_eq!(expected.len(), 2);
    expected.push(session.into());
    expected.sort();
    let mut live: Vec<String> = studio.live_keys().into_iter().collect();
    live.sort();
    assert_eq!(live, expected);

    studio.replay(last);
    assert!(studio.live_keys().is_empty());

    studio.replay(&lines[0].replace("\"source\":\"startup\"", "\"source\":\"resume\""));
    assert!(studio.live_keys().contains(session));
}

#[test]
fn the_selection_names_the_opened_subagent_until_a_session_is_selected() {
    let mut studio = replay(RUN);
    assert!(studio.selection().is_none());

    studio.select(RUN_SESSION);
    studio.open(RUN_WORKER);
    let selection = studio.selection().expect("a selection");
    assert_eq!(
        (selection.session.agent.id.as_str(), selection.agent),
        (RUN_SESSION, Some(RUN_WORKER))
    );
    assert_eq!(
        serde_json::to_value(&selection).unwrap()["agent"],
        json!(RUN_WORKER)
    );

    studio.select(RUN_SESSION);
    assert_eq!(studio.selection().expect("a selection").agent, None);
}

/// Session `s1`'s event `name` at `at`, in prompt `prompt`'s turn, with `fields` added.
fn ev(name: &str, at: u64, prompt: &str, fields: serde_json::Value) -> String {
    let mut event = json!({
        "session_id": "s1",
        "hook_event_name": name,
        "cwd": "/w",
        "prompt_id": prompt,
        "studio_received_ms": at,
    });
    let fields = fields.as_object().unwrap().clone();
    event.as_object_mut().unwrap().extend(fields);
    event.to_string()
}

/// The `PreToolUse` of a spawn by `caller`, `None` for the session's own agent.
fn spawn(at: u64, call: &str, caller: Option<&str>) -> String {
    let mut fields = json!({"tool_name": "Agent", "tool_use_id": call, "tool_input": {}});
    if let Some(caller) = caller {
        fields["agent_id"] = caller.into();
    }
    ev("PreToolUse", at, "p1", fields)
}

/// A step of subagent `agent` in `cwd`: a Bash `PreToolUse`.
fn step(at: u64, agent: &str, cwd: &str) -> String {
    let fields = json!({"agent_id": agent, "cwd": cwd, "tool_name": "Bash",
        "tool_use_id": format!("{agent}-{at}"), "tool_input": {"command": "sleep 60"}});
    ev("PreToolUse", at, "p1", fields)
}

/// A background spawn's response by `caller`, which names its child at once.
fn launched(
    at: u64,
    call: &str,
    caller: Option<&str>,
    child: &str,
    input: serde_json::Value,
) -> String {
    let mut fields = json!({"tool_name": "Agent", "tool_use_id": call, "tool_input": input,
        "tool_response": {"agentId": child, "status": "async_launched"}});
    if let Some(caller) = caller {
        fields["agent_id"] = caller.into();
    }
    ev("PostToolUse", at, "p1", fields)
}

/// Subagent `id` of `s1`: its status and when it ended.
fn status(studio: &Studio, id: &str) -> (String, Option<u64>) {
    let agent = agent(studio, "s1", id);
    (agent.status.to_string(), agent.ended_at)
}

#[test]
fn a_foreground_subagent_whose_spawn_is_interrupted_ends_as_interrupted() {
    let mut studio = Studio::default();
    studio.apply(&ev(
        "UserPromptSubmit",
        1,
        "p1",
        json!({"prompt": "Review it"}),
    ));
    studio.apply(&spawn(2, "t1", None));
    studio.apply(&step(3, "c1", "/w"));

    let failure = json!({"tool_name": "Agent", "tool_use_id": "t1", "tool_input": {},
        "error": "Interrupted by user", "is_interrupt": true});
    studio.apply(&ev("PostToolUseFailure", 4, "p1", failure));

    assert_eq!(status(&studio, "c1"), ("interrupted".into(), Some(4)));
}

#[test]
fn a_turn_that_ends_without_its_stop_ends_its_foreground_subagents_not_its_background_ones() {
    let mut studio = Studio::default();
    studio.apply(&ev("UserPromptSubmit", 1, "p1", json!({"prompt": "Do it"})));
    studio.apply(&spawn(2, "t1", None));
    studio.apply(&launched(3, "t1", None, "w1", json!({})));
    studio.apply(&step(4, "w1", "/w"));
    studio.apply(&spawn(5, "t2", None));
    studio.apply(&step(6, "c1", "/w"));

    // Esc, then the next prompt under a new prompt id.
    studio.apply(&ev(
        "UserPromptSubmit",
        10,
        "p2",
        json!({"prompt": "Stop that"}),
    ));

    assert_eq!(status(&studio, "c1"), ("interrupted".into(), Some(10)));
    assert_eq!(status(&studio, "w1"), ("running".into(), None));
}

#[test]
fn a_sessions_subagents_end_at_its_session_end() {
    let studio = replay(DAY);

    // `a4376` has one Bash step and no known spawn; its session ends at line 816.
    let session = studio
        .sessions()
        .into_iter()
        .find(|s| s.agent.id.starts_with("5d7f69ab"))
        .unwrap();
    let orphan = session
        .agents
        .iter()
        .find(|a| a.agent.id.starts_with("a4376"));
    assert!(orphan.unwrap().ended_at.is_some());
    assert!(session.agents.iter().all(|a| a.ended_at.is_some()));
}

#[test]
fn a_killed_subagents_running_descendants_end_with_it() {
    let mut studio = Studio::default();
    studio.apply(&ev("UserPromptSubmit", 1, "p1", json!({"prompt": "Do it"})));
    studio.apply(&launched(2, "t1", None, "w1", json!({})));
    studio.apply(&step(3, "w1", "/w"));
    studio.apply(&launched(4, "t2", Some("w1"), "c1", json!({})));
    studio.apply(&step(5, "c1", "/w"));
    // A foreground child of the child, which no spawn response has named yet.
    studio.apply(&spawn(6, "t3", Some("c1")));
    studio.apply(&step(7, "g1", "/w"));
    studio.apply(&ev(
        "Stop",
        8,
        "p1",
        json!({"background_tasks": [{"id": "w1"}]}),
    ));

    let killed =
        "<task-notification><task-id>w1</task-id><status>killed</status></task-notification>";
    studio.apply(&ev(
        "UserPromptSubmit",
        9,
        "p2",
        json!({ "prompt": killed }),
    ));

    assert_eq!(status(&studio, "c1"), ("killed".into(), Some(9)));
    assert_eq!(status(&studio, "g1"), ("killed".into(), Some(9)));
}

#[test]
fn a_killed_worker_that_its_sessions_stop_ended_first_still_gets_its_end_look() {
    let mut studio = Studio::default();
    studio.apply(&ev("UserPromptSubmit", 1, "p1", json!({"prompt": "Do it"})));
    let worktree = json!({"isolation": "worktree"});
    studio.apply(&launched(2, "t1", None, "w1", worktree));
    studio.apply(&step(3, "w1", "/w/.claude/worktrees/w1"));

    // The `Stop` no longer lists it, and its "killed" wake-up comes later.
    let stop = ev("Stop", 8, "p1", json!({"background_tasks": []}));
    let looks = studio.fingerprint_folders(&stop);

    let end = Fingerprint {
        key: "w1".into(),
        folder: "/w/.claude/worktrees/w1".into(),
        purpose: Purpose::Card,
    };
    assert!(looks.contains(&end), "{looks:?}");
}

#[test]
fn a_subagents_result_is_the_first_non_blank_line_of_its_last_message() {
    let mut studio = Studio::default();
    studio.apply(&step(1, "c1", "/w"));

    let message = "\n   \n  Found 3 callers.\nDetails";
    let stop = json!({"agent_id": "c1", "last_assistant_message": message});
    studio.apply(&ev("SubagentStop", 2, "p1", stop));

    assert_eq!(
        agent(&studio, "s1", "c1").result.as_deref(),
        Some("Found 3 callers.")
    );
}

/// Helper `h1`'s step `name` with `tool` in session `s1`'s worktree `/w` at `at`, failed with
/// `error` when it is a `PostToolUseFailure`.
fn helper_step(name: &str, tool: &str, at: u64, error: &str) -> String {
    json!({
        "session_id": "s1",
        "hook_event_name": name,
        "agent_id": "h1",
        "agent_type": "general-purpose",
        "cwd": "/w",
        "tool_name": tool,
        "tool_input": {"command": "cargo test", "file_path": "/w/lib.rs"},
        "tool_use_id": "toolu_h1",
        "error": error,
        "studio_received_ms": at,
    })
    .to_string()
}

/// Session `s1`, in a turn at `/w` since 0, whose helper `h1` started a Bash step at `at`.
fn helper_in_turn(at: u64) -> Studio {
    let prompt = json!({"session_id": "s1", "hook_event_name": "UserPromptSubmit", "cwd": "/w",
        "prompt": "Run the tests", "studio_received_ms": 0});
    let mut studio = replay(&prompt.to_string());
    studio.apply(&helper_step("PreToolUse", "Bash", at, ""));
    studio
}

/// A change found in the worktree of fingerprint key `key`.
fn changed(key: &str) -> BTreeMap<String, Change> {
    let change = Change {
        base: Base::default(),
        diff: "+fn element<'a>\n".into(),
        head: None,
    };
    BTreeMap::from([(key.into(), change)])
}

/// #434, decision 21: a failed command that ran changed files all the same.
#[test]
fn a_helper_whose_failed_command_changed_its_worktree_is_a_worker() {
    let mut studio = helper_in_turn(1_000);
    let error = "Exit code 101\nerror: test failed";
    let failed = helper_step("PostToolUseFailure", "Bash", 2_000, error);

    studio.apply_with_changes(&failed, changed("h1"));

    assert_eq!(agent(&studio, "s1", "h1").kind, Worker);
}

#[test]
fn a_helper_whose_edit_failed_stays_a_helper() {
    let mut studio = helper_in_turn(1_000);
    let error = "String to replace not found";
    let failed = helper_step("PostToolUseFailure", "Edit", 2_000, error);

    studio.apply_with_changes(&failed, changed("h1"));

    assert_eq!(agent(&studio, "s1", "h1").kind, Helper);
}

/// #434, decision 22: the due fallback's look comes beside the helper's own.
#[test]
fn a_helper_whose_command_changed_its_worktree_while_the_fallback_was_due_is_a_worker() {
    let late = 21 * 60 * 1000;
    let mut studio = helper_in_turn(late);
    let step = helper_step("PostToolUse", "Bash", late + 1_000, "");

    let looks = studio.fingerprint_folders(&step);
    let keys: Vec<&str> = looks.iter().map(|look| look.key.as_str()).collect();
    assert_eq!(keys, ["s1", "h1"]);
    let mut changes = changed("h1");
    changes.extend(changed("s1"));
    let outputs = studio.apply_with_changes(&step, changes);

    assert_eq!(agent(&studio, "s1", "h1").kind, Worker);
    assert!(outputs.iter().any(|o| matches!(o, Output::Card(_))));
}
