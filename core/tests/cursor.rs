//! The Cursor adapter (#362; spec #320, Harnesses): Cursor's hook payloads replayed into sessions,
//! turns and tools.
//!
//! `fixtures/cursor/run.jsonl` is built from Cursor's hook docs (https://cursor.com/docs/hooks, read
//! 2026-11-23), not from a real run: the common fields (`conversation_id`, `generation_id`, `model`,
//! `hook_event_name`, `cursor_version`, `workspace_roots`, `user_email`, `transcript_path`), each
//! event's own fields and the tool names (`Shell`, `Read`, `Write`, `MCP:…`) are the docs'; the ids,
//! the values and the transcript path are made up, since the docs give no example of them. Two
//! sessions run at once: the first from the CLI, with its pid, the second from the IDE, whose pid the
//! hook does not record.

use crate::common;
use studio_core::{CURSOR_CAPABILITIES, Change, Output, State, Studio};

const RUN: &str = include_str!("fixtures/cursor/run.jsonl");
const TRIAL: &str = include_str!("fixtures/cursor/trial.jsonl");
const FIRST: &str = "c7a1e2f0-3b4d-4c5e-9f60-1a2b3c4d5e6f";
const SECOND: &str = "d81f9c3a-6e2b-47d0-8a15-0f9e8d7c6b5a";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

/// The call, tool and turn of each call of session `id`, in order.
fn calls(studio: &Studio, id: &str) -> Vec<(String, String, Option<usize>)> {
    let session = studio.session(id).unwrap();
    let rows = session.ledger.rows().iter();
    rows.map(|r| (r.call.clone(), r.tool.clone(), r.turn))
        .collect()
}

#[test]
fn the_documented_run_replays_into_its_sessions_turns_and_tools() {
    let studio = Studio::replayed(fixture());

    let first = studio.session(FIRST).expect("the first Cursor session");
    assert_eq!(first.capabilities, CURSOR_CAPABILITIES);
    assert_eq!(first.capabilities.agent, "cursor");
    assert_eq!(first.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(first.cwd, "/Users/me/work/hello");
    assert_eq!(first.pid, Some(5151));
    let prompts: Vec<_> = first.turns.iter().map(|t| t.prompt.as_str()).collect();
    assert_eq!(
        prompts,
        ["Add a greeting to hello.txt", "Now run the tests"]
    );
    assert_eq!(
        first.turns[0].message.as_deref(),
        Some("hello.txt now says hello, world.")
    );
    assert!(first.turns.iter().all(|t| t.ended_at.is_some()));
    assert_eq!(first.state, State::Ended);
    assert_eq!(
        calls(&studio, FIRST),
        [
            ("tool_1".into(), "Read".into(), Some(0)),
            ("tool_2".into(), "Write".into(), Some(0)),
            ("tool_3".into(), "Shell".into(), Some(1)),
            ("tool_4".into(), "Shell".into(), Some(1)),
        ]
    );
    let test = first.ledger.row(None, "tool_3").unwrap();
    assert_eq!(test.command.as_deref(), Some("npm test"));

    let second = studio.session(SECOND).expect("the second Cursor session");
    assert_eq!(second.capabilities, CURSOR_CAPABILITIES);
    assert_eq!(second.cwd, "/Users/me/work/other");
    assert_eq!(second.turns.len(), 1);
    assert_eq!(second.turns[0].prompt, "What does this repo do?");
    assert_eq!(
        second.turns[0].message.as_deref(),
        Some("It greets people.")
    );
    assert_eq!(second.state, State::YourTurn);
    assert_eq!(
        calls(&studio, SECOND),
        [("tool_b1".into(), "MCP:read_file".into(), Some(0))]
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_call_runs_from_its_turn_until_its_post_event() {
    let lines = fixture();
    let state_after = |n: usize| Studio::replayed(&lines[..n]).session(FIRST).unwrap().state;
    // The first prompt, then its first call's result.
    assert_eq!(state_after(2), State::Running);
    assert_eq!(state_after(4), State::Running);
    // Its `stop`.
    assert_eq!(state_after(8), State::YourTurn);
}

#[test]
fn an_aborted_stop_ends_its_turn() {
    let lines = fixture();
    let at = lines
        .iter()
        .position(|l| l.contains("\"aborted\""))
        .unwrap();

    let before = Studio::replayed(&lines[..at]);
    assert_eq!(before.session(FIRST).unwrap().state, State::Running);
    let after = Studio::replayed(&lines[..=at]);
    let session = after.session(FIRST).unwrap();
    assert_ne!(session.state, State::Running);
    assert!(session.turns[1].ended_at.is_some());
}

/// Wave 1's trial (#369): a Cursor CLI turn that ran `calc.py`, rewrote it and ran it again, its
/// `stop` landing before its reply, as Cursor 2026.11.22 sent its lines
/// (`fixtures/cursor/trial.jsonl`, the trial's folder written `/w/trial`). Its summary said nothing
/// was recorded, as Cursor recorded no turn trees (#426).
#[test]
fn a_cursor_turn_that_writes_a_file_lists_it_and_its_summarys_facts_carry_it() {
    let (listed, carried) = common::trial_files(TRIAL, "beforeSubmitPrompt", "stop");
    assert_eq!(listed, ["calc.py"]);
    assert_eq!(carried, ["calc.py"]);
}

/// The IDE's pid is not recorded (#320, "IDE-hosted pids"), so a running IDE session with no event for
/// a while goes idle ("idle?"), and the process check never ends it.
#[test]
fn an_ide_session_with_no_pid_goes_idle_and_is_never_ended_by_the_process_check() {
    let lines = fixture();
    // The IDE session's prompt, mid-turn.
    let at = lines
        .iter()
        .position(|l| l.contains("What does this repo do?"))
        .unwrap();
    let mut studio = Studio::replayed(&lines[..=at]);
    let session = studio.session(SECOND).unwrap();
    assert_eq!(session.pid, None);
    assert_eq!(session.state, State::Running);
    assert!(studio.gone(|_| false).iter().all(|id| id != SECOND));

    let last = 1_794_800_003_500;
    studio.tick(last + 24 * 60 * 60 * 1000);
    assert_eq!(studio.session(SECOND).unwrap().state, State::Idle);
    assert!(studio.gone(|_| false).iter().all(|id| id != SECOND));
}

#[test]
fn a_cursor_session_ended_by_the_process_check_or_interrupted_stays_cursor() {
    // The CLI session up to its second turn's first call, then the server's lines for an interrupt and for its
    // gone process.
    let lines: Vec<String> = fixture()[..11].iter().map(|l| l.to_string()).collect();
    let synthetic = [
        serde_json::json!({
            "session_id": FIRST,
            "hook_event_name": "StudioInterrupted",
            "prompt_id": "gen_2",
            "studio_received_ms": 1_794_800_011_000u64,
        })
        .to_string(),
        serde_json::json!({
            "session_id": FIRST,
            "hook_event_name": "SessionEnd",
            "reason": "process_gone",
            "studio_received_ms": 1_794_800_012_000u64,
        })
        .to_string(),
    ];

    let mut live = Studio::replayed(&lines);
    for line in &synthetic {
        live.apply(line);
    }
    let mut all = lines.clone();
    all.extend(synthetic);
    let restarted = Studio::replayed(&all);
    for studio in [&live, &restarted] {
        let session = studio.session(FIRST).unwrap();
        assert_eq!(session.state, State::Ended);
        assert_eq!(session.capabilities, CURSOR_CAPABILITIES);
    }
}

/// Cursor runs `afterAgentResponse` and `stop` as hooks of their own a few milliseconds apart, so
/// their lines land in either order (wave 1's trial, #369: reply first in one run, `stop` first in
/// the next, 4 ms apart). The reply is the turn's either way, and a long one asks for its wanted
/// summary once, also when the turn changed code, so its `stop` raised a card and asked for a summary
/// it did not yet want (the live case the trial met). Lines as Cursor CLI 2026.11.22 sent them, cut to
/// the fields read.
#[test]
fn a_reply_that_lands_after_its_stop_is_still_its_turns_and_asks_for_its_summary() {
    let line = |event: &str, t: u64, extra: &str| {
        format!(
            r#"{{"conversation_id":"d07b","cursor_version":"2026.11.22-e9042b2","generation_id":"db6e","hook_event_name":"{event}","model":"default","session_id":"d07b","studio_agent":"cursor","studio_pid":86263,"studio_received_ms":{t},"workspace_roots":["/w/calc"]{extra}}}"#
        )
    };
    let reply = format!("The add function subtracted. {}", "word ".repeat(200));
    let start = line("sessionStart", 1, "");
    let prompt = line(
        "beforeSubmitPrompt",
        2,
        r#","prompt":"Fix calc.py and explain""#,
    );
    let stop = line("stop", 3, r#","status":"completed","loop_count":0"#);
    let answer = line(
        "afterAgentResponse",
        4,
        &format!(r#","text":{}"#, serde_json::to_string(&reply).unwrap()),
    );
    let edited = || {
        let change = Change {
            base: Default::default(),
            diff: "--- a/calc.py\n+++ b/calc.py\n".into(),
            head: None,
        };
        [("d07b".to_owned(), change)].into_iter().collect()
    };
    let orders = [[&stop, &answer], [&answer, &stop]];
    for (order, changed) in orders.iter().flat_map(|o| [(o, false), (o, true)]) {
        let mut studio = Studio::default();
        let mut outputs = Vec::new();
        for l in [&start, &prompt, order[0], order[1]] {
            let changes = match changed && *l == stop {
                true => edited(),
                false => Default::default(),
            };
            outputs.extend(studio.apply_with_changes(l, changes));
        }
        let summaries: Vec<_> = outputs
            .iter()
            .filter_map(|o| match o {
                Output::Summary(request) if request.wanted => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(
            summaries.len(),
            1,
            "one wanted summary request, changed: {changed}"
        );
        assert_eq!(summaries[0].card.is_some(), changed);
        assert_eq!(
            summaries[0].responses.last().map(|r| r.trim()),
            Some(reply.trim())
        );
        let session = studio.session("d07b").unwrap();
        assert_eq!(session.state, State::YourTurn);
        assert_eq!(session.turns[0].reply, reply.trim());
        assert_eq!(
            session.turns[0].message.as_deref(),
            Some(reply.lines().next().unwrap().trim())
        );
    }
}
