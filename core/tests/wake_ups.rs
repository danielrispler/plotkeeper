//! Seam 1 (danielrispler/Falinks#369, Testing Decisions) for wake-ups (danielrispler/Falinks#373): replay hook events through the core and
//! check which of a session's prompts Claude Code sent to wake it, and which are Daniel's.

use serde_json::json;
use studio_core::{CardRequest, Output, Sender, State, Studio};

fn replay(log: &str) -> Studio {
    Studio::replayed(log.lines())
}

/// The checkpoint day of danielrispler/Falinks#347, scrubbed (danielrispler/Falinks#370).
const DAY: &str = include_str!("fixtures/checkpoint-day.jsonl");
/// The day's `implement-spec` session.
const IMPLEMENT_SPEC: &str = "5d7f69ab-91ca-427e-9d9c-9180c2c0c74b";

#[test]
fn the_implement_spec_sessions_13_wake_ups_are_marked_and_its_5_other_prompts_are_not() {
    let studio = replay(DAY);

    let session = studio.session(IMPLEMENT_SPEC).unwrap();
    let senders: Vec<String> = session
        .prompts
        .iter()
        .map(|prompt| match &prompt.sender {
            Sender::Daniel => "Daniel".into(),
            Sender::WakeUp(id) => id[..5].into(),
            Sender::Unrecognised(id) => format!("drift {id}"),
        })
        .collect();

    // By the Further Notes' "Numbers": the 13 prompts that start with `<agent-message from=` or
    // `<task-notification>`, the last but one from the background command `brh86k1fz`.
    assert_eq!(
        senders,
        [
            "Daniel", "ac088", "ac088", "a2526", "a2526", "a1191", "Daniel", "Daniel", "Daniel",
            "a1191", "a1191", "aed99", "a3c3f", "a96cd", "a96cd", "a96cd", "brh86", "Daniel",
        ]
    );
}

/// Session `IMPLEMENT_SPEC`'s prompt `prompt`, received after the day.
fn prompt(prompt: &str) -> String {
    json!({
        "session_id": IMPLEMENT_SPEC,
        "hook_event_name": "UserPromptSubmit",
        "cwd": "/Users/danielrispler/work/Falinks",
        "prompt": prompt,
        "studio_received_ms": 1_793_400_000_000u64,
    })
    .to_string()
}

#[test]
fn a_wake_up_from_a_task_with_no_subagent_names_it_by_its_short_id() {
    let mut studio = replay(DAY);

    // `brh86k1fz` is the day's background command, which no subagent record names.
    studio.apply(&prompt(
        "<task-notification><task-id>brh86k1fz</task-id><status>completed</status></task-notification>",
    ));

    let session = studio.session(IMPLEMENT_SPEC).unwrap();
    assert_eq!(session.returned.as_deref(), Some("worker brh86 returned"));
}

/// The senders of the prompts after the day's 18.
fn new_senders(studio: &Studio) -> Vec<&Sender> {
    let session = studio.session(IMPLEMENT_SPEC).unwrap();
    session.prompts[18..].iter().map(|p| &p.sender).collect()
}

#[test]
fn a_wake_up_is_the_rows_text_until_the_sessions_next_step() {
    let mut studio = replay(DAY);
    let session = |studio: &Studio| studio.session(IMPLEMENT_SPEC).unwrap().clone();
    let before = session(&studio).agent.last_step.clone();
    assert!(before.is_some());

    studio.apply(&prompt(
        "<task-notification><task-id>a1191df87fb730aa0</task-id><status>completed</status></task-notification>",
    ));

    // The row reads "worker Implement danielrispler/Falinks#355 relocation returned", and the last step stays true.
    let woken = session(&studio);
    assert_eq!(woken.agent.last_step, before);
    assert_eq!(
        woken.returned.as_deref(),
        Some("worker Implement #355 relocation returned")
    );
    // Daniel's own prompt ends the line too.
    studio.apply(&prompt("carry on"));
    assert_eq!(session(&studio).returned, None);
    studio.apply(&prompt(
        "<task-notification><task-id>a1191df87fb730aa0</task-id><status>completed</status></task-notification>",
    ));
    assert!(session(&studio).returned.is_some());
    let step = json!({
        "session_id": IMPLEMENT_SPEC,
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "git log -1" },
        "tool_use_id": "toolu_after_wake_up",
        "studio_received_ms": 1_793_400_000_001u64,
    });
    studio.apply(&step.to_string());
    let stepped = session(&studio);
    assert_ne!(stepped.agent.last_step, before);
    assert_eq!(stepped.returned, None);
}

/// Session `session`'s `Stop` at `at`, with its transcript.
fn stop(session: &str, at: u64) -> String {
    json!({
        "session_id": session,
        "hook_event_name": "Stop",
        "cwd": "/Users/danielrispler/work/Falinks",
        "transcript_path": format!("/Users/danielrispler/.claude/projects/-Users-danielrispler-work-Falinks/{session}.jsonl"),
        "last_assistant_message": "Done.",
        "studio_received_ms": at,
    })
    .to_string()
}

fn versions(outputs: Vec<Output>) -> Vec<Output> {
    outputs
        .into_iter()
        .filter(|output| matches!(output, Output::Version { .. }))
        .collect()
}

#[test]
fn a_sessions_first_stop_asks_for_claude_codes_version_from_its_transcript_only() {
    let mut studio = replay(DAY);
    let new = "3b2f7c1e-8d4a-4e6b-9f0c-2a5d7e9b1c43";

    let first = studio.apply(&stop(new, 1_793_400_000_000));
    let second = studio.apply(&stop(new, 1_793_400_060_000));
    // The day's sessions had their first `Stop` in the replay.
    let replayed = studio.apply(&stop(IMPLEMENT_SPEC, 1_793_400_120_000));

    let transcript = format!(
        "/Users/danielrispler/.claude/projects/-Users-danielrispler-work-Falinks/{new}.jsonl"
    );
    assert_eq!(
        versions(first),
        [Output::Version {
            transcript_path: transcript
        }]
    );
    assert_eq!((versions(second), versions(replayed)), (vec![], vec![]));
}

#[test]
fn a_prompt_naming_a_known_id_in_plain_text_is_daniels() {
    let mut studio = replay(DAY);

    let outputs = studio.apply(&prompt("check what a1191df87fb730aa0 did on #355"));

    assert_eq!(new_senders(&studio), [&Sender::Daniel]);
    assert_eq!(outputs, []);
}

#[test]
fn a_known_id_under_a_tag_the_studio_does_not_know_is_drift_not_a_wake_up() {
    let mut studio = replay(DAY);
    let (agent, command) = ("a1191df87fb730aa0", "brh86k1fz");

    let attribute = studio.apply(&prompt(&format!(
        "<agent-report task=\"{agent}\">…</agent-report>"
    )));
    let element = studio.apply(&prompt(&format!("<task-ref>{command}</task-ref>")));

    assert_eq!(
        new_senders(&studio),
        [
            &Sender::Unrecognised(agent.into()),
            &Sender::Unrecognised(command.into())
        ]
    );
    // The shell logs each on stderr.
    let drift = |task_id: &str| Output::Unrecognised {
        session_id: IMPLEMENT_SPEC.into(),
        task_id: task_id.into(),
    };
    assert_eq!(
        (attribute, element),
        (vec![drift(agent)], vec![drift(command)])
    );
}

/// The glossary: a wake-up never counts as Daniel's prompt, so the permission the session waits on
/// still needs him until its step resolves (danielrispler/Falinks#369, story 12).
#[test]
fn a_wake_up_leaves_the_session_needing_daniel_until_the_step_that_asks_resolves() {
    let mut studio = Studio::default();
    let event = |name: &str, fields: serde_json::Value| {
        let mut event = json!({ "session_id": "s1", "hook_event_name": name });
        let fields = fields.as_object().unwrap().clone();
        event.as_object_mut().unwrap().extend(fields);
        event.to_string()
    };
    let push = json!({"tool_name": "Bash", "tool_input": {"command": "git push"},
        "tool_use_id": "toolu_push"});
    let state = |studio: &Studio| studio.session("s1").unwrap().state;
    studio.apply(&event(
        "UserPromptSubmit",
        json!({ "prompt": "Implement #378" }),
    ));
    studio.apply(&event(
        "PostToolUse",
        json!({"tool_name": "Agent", "tool_input": {}, "tool_response": {"agentId": "a1"}}),
    ));
    studio.apply(&event("PreToolUse", push.clone()));
    studio.apply(&event("PermissionRequest", push.clone()));
    assert_eq!(state(&studio), State::NeedsYou);

    let wake_up =
        "<task-notification><task-id>a1</task-id><status>completed</status></task-notification>";
    studio.apply(&event("UserPromptSubmit", json!({ "prompt": wake_up })));
    assert_eq!(state(&studio), State::NeedsYou);

    studio.apply(&event("PostToolUse", push));
    assert_eq!(state(&studio), State::Running);
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
    event
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    event.to_string()
}

/// A background spawn's response, which names its child `child` at once.
fn launched(at: u64, call: &str, child: &str) -> String {
    let fields = json!({"tool_name": "Agent", "tool_use_id": call, "tool_input": {},
        "tool_response": {"agentId": child, "status": "async_launched"}});
    ev("PostToolUse", at, "p1", fields)
}

/// The wake-up that names `tasks`, which opens the turn of prompt `prompt`.
fn woken(at: u64, prompt: &str, tasks: &[&str]) -> String {
    let notices: String = tasks
        .iter()
        .map(|t| format!("<task-notification><task-id>{t}</task-id><status>completed</status></task-notification>"))
        .collect();
    ev("UserPromptSubmit", at, prompt, json!({ "prompt": notices }))
}

/// The turn of the card `id` of worker `worker`, as session `s1`'s rail places it.
fn card_turn(studio: &Studio, worker: &str, at: u64) -> Option<usize> {
    let request = CardRequest {
        session_id: "s1".into(),
        id: format!("{worker}@{at}"),
        agent: Some(worker.into()),
        ..CardRequest::default()
    };
    studio.session("s1").unwrap().worker_card_turn(&request)
}

/// Turn 0 spawns `w1`, `w2` and `w3`; `w1` and `w2` end at 10 and 11; Daniel's prompt at 12 opens turn 1; the one wake-up
/// that names `w1` and `w2` at 20 opens turn 2. `w3` is named by no wake-up.
fn spawned_and_woken() -> Studio {
    let mut studio = Studio::default();
    studio.apply(&ev("UserPromptSubmit", 1, "p1", json!({"prompt": "Do it"})));
    for (at, call, child) in [(2, "t1", "w1"), (3, "t2", "w2"), (4, "t3", "w3")] {
        studio.apply(&launched(at, call, child));
    }
    let tasks = json!({"background_tasks": [{"id": "w1"}, {"id": "w2"}, {"id": "w3"}]});
    studio.apply(&ev("Stop", 8, "p1", tasks));
    for (at, child) in [(10, "w1"), (11, "w2")] {
        studio.apply(&ev("SubagentStop", at, "p1", json!({"agent_id": child})));
    }
    studio.apply(&ev(
        "UserPromptSubmit",
        12,
        "p2",
        json!({"prompt": "Meanwhile, rename it"}),
    ));
    studio.apply(&ev("Stop", 14, "p2", json!({"background_tasks": []})));
    studio.apply(&woken(20, "p3", &["w1", "w2"]));
    studio
}

#[test]
fn a_workers_card_of_its_end_is_in_the_turn_its_wake_up_opened_not_the_one_daniel_opened_between() {
    let studio = spawned_and_woken();
    let turns = &studio.session("s1").unwrap().turns;
    assert_eq!(turns.len(), 3);
    assert_eq!(turns[2].prompt, "worker w1 returned");

    assert_eq!(card_turn(&studio, "w1", 10), Some(2));
    assert_eq!(card_turn(&studio, "w1", 13), Some(2));
}

#[test]
fn one_wake_up_takes_the_cards_of_both_workers_it_names() {
    let studio = spawned_and_woken();

    assert_eq!(card_turn(&studio, "w1", 10), Some(2));
    assert_eq!(card_turn(&studio, "w2", 11), Some(2));
}

#[test]
fn a_card_asked_for_while_the_worker_runs_is_in_the_turn_that_spawned_it() {
    let studio = spawned_and_woken();

    assert_eq!(card_turn(&studio, "w1", 9), Some(0));
}

#[test]
fn the_cards_of_a_worker_no_wake_up_names_are_in_the_turn_that_spawned_it() {
    let studio = spawned_and_woken();

    assert_eq!(card_turn(&studio, "w3", 9), Some(0));
    assert_eq!(card_turn(&studio, "w3", 30), Some(0));
}

#[test]
fn a_card_of_a_worker_that_ended_waits_in_the_spawning_turn_until_its_wake_up_arrives() {
    let mut studio = Studio::default();
    studio.apply(&ev("UserPromptSubmit", 1, "p1", json!({"prompt": "Do it"})));
    studio.apply(&launched(2, "t1", "w1"));
    studio.apply(&ev("SubagentStop", 10, "p1", json!({"agent_id": "w1"})));
    studio.apply(&ev("Stop", 11, "p1", json!({"background_tasks": []})));
    studio.apply(&ev(
        "UserPromptSubmit",
        12,
        "p2",
        json!({"prompt": "Meanwhile"}),
    ));

    assert_eq!(card_turn(&studio, "w1", 10), Some(0));
}
