//! Seam 1 (#369, Testing Decisions) for wake-ups (#373): replay hook events through the core and
//! check which of a session's prompts Claude Code sent to wake it, and which are Daniel's.

use serde_json::json;
use studio_core::{Output, Sender, Studio};

fn replay(log: &str) -> Studio {
    let mut studio = Studio::default();
    for line in log.lines() {
        studio.replay(line);
    }
    studio
}

/// The checkpoint day of #347, scrubbed (#370).
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

/// The senders of the prompts after the day's 18.
fn new_senders(studio: &Studio) -> Vec<&Sender> {
    let session = studio.session(IMPLEMENT_SPEC).unwrap();
    session.prompts[18..].iter().map(|p| &p.sender).collect()
}

#[test]
fn a_wake_up_is_the_rows_text_until_the_sessions_next_step() {
    let mut studio = replay(DAY);
    let session = |studio: &Studio| studio.session(IMPLEMENT_SPEC).unwrap().clone();
    assert!(session(&studio).last_step.is_some());

    studio.apply(&prompt(
        "<task-notification><task-id>a1191df87fb730aa0</task-id><status>completed</status></task-notification>",
    ));

    // The row reads "worker Implement #355 relocation returned": no step since its latest prompt,
    // a wake-up from `a1191`.
    let woken = session(&studio);
    assert_eq!(woken.last_step, None);
    assert_eq!(
        woken.prompts.last().unwrap().sender,
        Sender::WakeUp("a1191df87fb730aa0".into())
    );
    let step = json!({
        "session_id": IMPLEMENT_SPEC,
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "git log -1" },
        "tool_use_id": "toolu_after_wake_up",
        "studio_received_ms": 1_793_400_000_001u64,
    });
    studio.apply(&step.to_string());
    assert!(session(&studio).last_step.is_some());
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
