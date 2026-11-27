//! The OpenCode adapter (#370; spec #320, Harnesses): the lines plotkeeper's OpenCode plugin
//! (`plugin-opencode/plotkeeper.js`) appends, replayed into sessions, turns and tools.
//!
//! `fixtures/opencode/run.jsonl` is a real run, recorded on 2026-11-27 with OpenCode 2.4.1 and its
//! free `opencode/big-pickle` model: the plugin, rendered into two projects' `.opencode/plugins/`,
//! ran this checkout's `plotkeeper-hook` with a scratch home. Two sessions ran at once in two
//! folders. In `hello`, a first turn read, wrote and ran two commands, one failing; a second turn,
//! by `opencode run -s`, started a background subagent that read a file, whose result woke the
//! session for a turn of its own. In `other`, a rule asked before any
//! shell command (`--auto` approved it). The scratch folders' path is replaced by `/Users/me/work`.
//! It was recorded before #718, so its spawn's end names no child.

use serde_json::{Value, json};
use studio_core::{OPENCODE_CAPABILITIES, Outcome, Parent, State, Status, Studio};

const RUN: &str = include_str!("fixtures/opencode/run.jsonl");
const HELLO: &str = "ses_de5fa80f2ffeXThZwnlgn61UPk";
const OTHER: &str = "ses_de5fa80f3ffekjMj5xJv5CPObP";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

/// The tool and turn of each call of session `id`, in order.
fn calls(studio: &Studio, id: &str) -> Vec<(String, String, Option<usize>)> {
    let session = studio.session(id).unwrap();
    let rows = session.ledger.rows().iter();
    rows.map(|r| (r.call.clone(), r.tool.clone(), r.turn))
        .collect()
}

#[test]
fn the_recorded_run_replays_into_its_sessions_turns_and_tools() {
    let studio = Studio::replayed(fixture());

    let hello = studio.session(HELLO).expect("the hello session");
    assert_eq!(hello.capabilities, OPENCODE_CAPABILITIES);
    assert_eq!(hello.capabilities.agent, "opencode");
    assert_eq!(hello.model.as_deref(), Some("big-pickle"));
    assert_eq!(hello.pid, None);
    let prompts: Vec<_> = hello.turns.iter().map(|t| t.prompt.as_str()).collect();
    assert_eq!(prompts.len(), 3);
    assert!(prompts[0].starts_with("Read hello.txt"), "{prompts:?}");
    assert!(
        prompts[1].starts_with("Use the subagent tool"),
        "{prompts:?}"
    );
    // The turn the subagent's result woke. The plugin before #718 did not send the spawn's child, so
    // the child is unattached and named by its id.
    assert_eq!(prompts[2], "worker ses_d returned");
    let messages: Vec<_> = hello.turns.iter().map(|t| t.message.as_deref()).collect();
    assert_eq!(
        messages,
        [
            Some(
                "I updated hello.txt to 'hello, world', confirmed it with cat, and the ls command failed as expected."
            ),
            Some("I've launched the subagent; I'll report the first line once it finishes."),
            Some("# Other"),
        ]
    );
    assert!(hello.turns.iter().all(|t| t.ended_at.is_some()));
    assert_eq!(hello.state, State::YourTurn);
    let id = |s: &str| format!("chatcmpl-tool-{s}");
    assert_eq!(
        calls(&studio, HELLO),
        [
            (id("bdc61c68a0b20e8d"), "read".into(), Some(0)),
            (id("91d1c5c4edfc168b"), "write".into(), Some(0)),
            (id("a44a13ff386275fd"), "shell".into(), Some(0)),
            (id("a193a0a15e56efb7"), "shell".into(), Some(0)),
            (id("a21c0e2f6994fb35"), "subagent".into(), Some(1)),
            // The subagent's call, under the session that spawned it.
            (id("bea84a086f0fa20c"), "read".into(), Some(1)),
        ]
    );
    let write = hello.ledger.row(None, &id("91d1c5c4edfc168b")).unwrap();
    assert_eq!(
        write.file.as_deref(),
        Some("/Users/me/work/hello/hello.txt")
    );
    assert_eq!(write.outcome, Some(Outcome::Succeeded));
    let cat = hello.ledger.row(None, &id("a44a13ff386275fd")).unwrap();
    assert_eq!(cat.command.as_deref(), Some("cat hello.txt"));
    assert_eq!(cat.outcome, Some(Outcome::Succeeded));
    let ls = hello.ledger.row(None, &id("a193a0a15e56efb7")).unwrap();
    assert_eq!(ls.outcome, Some(Outcome::Failed { exit_code: Some(1) }));

    let other = studio.session(OTHER).expect("the other session");
    assert_eq!(other.capabilities, OPENCODE_CAPABILITIES);
    assert_eq!(other.turns.len(), 1);
    assert_eq!(
        other.turns[0].message.as_deref(),
        Some("The command printed `hi`.")
    );
    assert_eq!(other.state, State::YourTurn);
    assert_eq!(
        calls(&studio, OTHER),
        [(id("80fcdaa1553f80f9"), "shell".into(), Some(0))]
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_permission_ask_needs_you_until_its_call_ends() {
    let lines = fixture();
    let state_after = |n: usize| {
        let studio = Studio::replayed(&lines[..n]);
        studio.session(OTHER).unwrap().state
    };
    // The `permission.asked` of `echo hi`, which came before its call's start, then its result.
    assert_eq!(state_after(5), State::NeedsYou);
    assert_eq!(state_after(6), State::NeedsYou);
    assert_eq!(state_after(7), State::Running);
}

/// `fixtures/opencode/asks.jsonl`, a second real run (2026-11-27, OpenCode 2.4.1), where rules asked
/// before every edit and shell command: OpenCode asks a `write` call's permission as action `edit`,
/// so the plugin sends the ask with its call's own tool and input, by which it is the call's.
#[test]
fn each_ask_of_a_write_or_a_command_needs_you_until_its_own_call_ends() {
    let lines: Vec<_> = include_str!("fixtures/opencode/asks.jsonl")
        .lines()
        .collect();
    let state_after = |n: usize| {
        let studio = Studio::replayed(&lines[..n]);
        let session = studio.session("ses_de5e2b218ffezl2tnMweO2Le0q").unwrap();
        session.state
    };
    let states: Vec<_> = (4..=9).map(state_after).collect();
    use State::{NeedsYou, Running};
    assert_eq!(
        states,
        [Running, NeedsYou, Running, Running, NeedsYou, Running]
    );
    assert_eq!(state_after(lines.len()), State::YourTurn);
}

#[test]
fn an_opencode_session_records_no_turn_trees() {
    let studio = Studio::replayed(fixture());
    let session = studio.session(HELLO).unwrap();
    assert!(!session.capabilities.turn_trees);
    assert!(session.turns.iter().all(|t| t.files.is_none()));
}

#[test]
fn an_event_the_adapter_does_not_map_is_no_drift() {
    let mut lines: Vec<String> = fixture().iter().map(ToString::to_string).collect();
    lines.insert(
        2,
        json!({
            "session_id": HELLO, "cwd": "/Users/me/work/hello", "hook_event_name": "session.compacted",
            "studio_agent": "opencode", "studio_received_ms": 1_795_732_713_000u64,
        })
        .to_string(),
    );
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.session(HELLO).unwrap().state, State::YourTurn);
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

/// A line the server writes itself for session `id`, shaped as Claude Code's lines are.
fn server_line(id: &str, name: &str, at: u64, fields: Value) -> String {
    let mut line = json!({ "session_id": id, "hook_event_name": name, "studio_received_ms": at });
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
}

#[test]
fn an_opencode_session_ended_by_the_server_stays_opencode() {
    let lines: Vec<String> = fixture()[..12].iter().map(ToString::to_string).collect();
    let synthetic = [server_line(
        HELLO,
        "SessionEnd",
        1_795_732_721_000,
        json!({ "reason": "process_gone" }),
    )];
    let mut live = Studio::replayed(&lines);
    for line in &synthetic {
        live.apply(line);
    }
    let mut all = lines.clone();
    all.extend(synthetic);
    let restarted = Studio::replayed(&all);
    for studio in [&live, &restarted] {
        let session = studio.session(HELLO).unwrap();
        assert_eq!(session.state, State::Ended);
        assert_eq!(session.capabilities, OPENCODE_CAPABILITIES);
    }
}

/// `fixtures/opencode/subagent.jsonl`, a real run (2026-11-27, OpenCode 2.4.1's service) of the
/// plugin since #718: one turn started a background subagent that read a file, and its result woke
/// the session for a turn of its own.
#[test]
fn a_background_subagent_replays_as_its_sessions_subagent_with_its_calls_and_reply() {
    let lines: Vec<_> = include_str!("fixtures/opencode/subagent.jsonl")
        .lines()
        .collect();
    let studio = Studio::replayed(&lines);
    let session = studio.session("ses_de36d8bc3ffeKHlJcfIDXNryLS").unwrap();
    let child = "ses_de36d7ee3ffesJGKi1dM4DiGOK";

    let [agent] = session.agents.as_slice() else {
        panic!("one subagent: {:?}", session.agents);
    };
    assert_eq!(agent.agent.id, child);
    assert_eq!(agent.parent, Parent::Session);
    assert_eq!(agent.name, "Read README first line");
    assert_eq!(agent.agent_type.as_deref(), Some("general"));
    assert_eq!(agent.turn, Some(0));
    assert_eq!(agent.status, Status::Completed);
    assert_eq!(agent.result.as_deref(), Some("# Hello"));
    assert!(session.capabilities.replies);

    let rows: Vec<_> = (session.ledger.rows().iter())
        .map(|r| (r.tool.as_str(), r.agent.as_deref()))
        .collect();
    assert_eq!(rows, [("subagent", None), ("read", Some(child))]);

    let prompts: Vec<_> = session.turns.iter().map(|t| t.prompt.as_str()).collect();
    assert_eq!(prompts[1], "worker Read README first line returned");
    let messages: Vec<_> = session.turns.iter().map(|t| t.message.as_deref()).collect();
    assert_eq!(
        messages,
        [
            Some(
                "I've started the subagent to read `README.md`. I'll report the first line once it returns."
            ),
            Some("# Hello"),
        ]
    );
    assert_eq!(session.model.as_deref(), Some("big-pickle"));
    assert_eq!(session.state, State::YourTurn);
}
