//! The Cline adapter (#371; spec #320, Harnesses): the lines plotkeeper's Cline plugin
//! (`plugin-cline/plotkeeper.js`) appends, replayed into sessions, turns and tools.
//!
//! `fixtures/cline/run.jsonl` is a real run, recorded on 2026-11-27 with Cline CLI 3.0.76 and
//! `claude-haiku-5-5`: the plugin, installed into a scratch project's `.cline/plugins/` with
//! `cline plugin install --cwd`, ran this checkout's `plotkeeper-hook` with a scratch home. One
//! headless `cline --json` run read a file, edited it and ran two commands in one call, the second
//! failing, then called Cline's `submit_and_exit`. The scratch folder's path is replaced by
//! `/Users/me/work`.

use serde_json::{Value, json};
use studio_core::{CLINE_CAPABILITIES, Outcome, State, Status, Studio};

const RUN: &str = include_str!("fixtures/cline/run.jsonl");
const HELLO: &str = "1795785548678_7h0oj";

fn fixture() -> Vec<&'static str> {
    RUN.lines().collect()
}

#[test]
fn the_recorded_run_replays_into_its_session_turn_and_tools() {
    let studio = Studio::replayed(fixture());

    let hello = studio.session(HELLO).expect("the hello session");
    assert_eq!(hello.capabilities, CLINE_CAPABILITIES);
    assert_eq!(hello.capabilities.agent, "cline");
    assert_eq!(hello.pid, Some(78277));
    assert_eq!(hello.model, None);
    let prompts: Vec<_> = hello.turns.iter().map(|t| t.prompt.as_str()).collect();
    // The prompt without the `<user_input>` tag Cline wraps it in.
    assert_eq!(
        prompts,
        [
            "Read hello.txt, change its contents to 'hello, world' with your file editing tool, run the shell command 'cat hello.txt', then run 'ls /nonexistent-dir-xyz' (it will fail). Reply with one short sentence."
        ]
    );
    let message = hello.turns[0].message.as_deref().unwrap();
    assert!(
        message.starts_with("Submission recorded (verified): Read hello.txt"),
        "{message}"
    );
    assert!(hello.turns[0].ended_at.is_some());
    assert_eq!(hello.state, State::YourTurn);
    let calls: Vec<_> = (hello.ledger.rows().iter())
        .map(|r| (r.tool.as_str(), r.turn))
        .collect();
    assert_eq!(
        calls,
        [
            ("read_files", Some(0)),
            ("editor", Some(0)),
            ("run_commands", Some(0)),
            ("submit_and_exit", Some(0)),
        ]
    );
    let edit = hello
        .ledger
        .row(None, "toolu_01PsEjJH4cF9XRBzdz3dMa2P")
        .unwrap();
    assert_eq!(edit.file.as_deref(), Some("/Users/me/work/hello/hello.txt"));
    assert_eq!(edit.outcome, Some(Outcome::Succeeded));
    // One call ran both commands; the second failed, which Cline's result tells only in that command's entry.
    let commands = hello
        .ledger
        .row(None, "toolu_01Dpr4ANYJh7Ud9SZxKUVqaa")
        .unwrap();
    assert_eq!(
        commands.command.as_deref(),
        Some("cd /Users/me/work/hello && cat hello.txt\nls /nonexistent-dir-xyz")
    );
    assert_eq!(
        commands.outcome,
        Some(Outcome::Failed { exit_code: Some(1) })
    );
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

#[test]
fn a_cline_session_records_no_turn_trees() {
    let studio = Studio::replayed(fixture());
    let session = studio.session(HELLO).unwrap();
    assert!(!session.capabilities.turn_trees);
    assert!(session.turns.iter().all(|t| t.files.is_none()));
}

/// Line `n` of the fixture with `fields` set.
fn changed(n: usize, fields: Value) -> String {
    let mut line: Value = serde_json::from_str(fixture()[n]).unwrap();
    line.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    line.to_string()
}

/// A subagent's runs have hooks of their own: neither its start nor its end is the session's turn's,
/// its calls go under it, and its run's end ends it (unverified: no real run of Cline's subagents was
/// recorded).
#[test]
fn a_subagents_run_is_its_own_not_the_sessions_turn() {
    let sub =
        json!({ "agent_id": "agent_sub", "parent_agent_id": "agent_26ef", "run_id": "run_sub" });
    let mut lines: Vec<String> = fixture()[..3].iter().map(ToString::to_string).collect();
    // Its run's start, a call of its own, and its run's end, in the session's turn.
    for n in [0, 1, 2, 9] {
        lines.push(changed(n, sub.clone()));
    }

    let studio = Studio::replayed(&lines);
    let session = studio.session(HELLO).unwrap();
    assert_eq!(session.turns.len(), 1);
    assert_eq!(session.state, State::Running);
    let [agent] = session.agents.as_slice() else {
        panic!("one subagent: {:?}", session.agents);
    };
    assert_eq!(agent.agent.id, "agent_sub");
    assert_eq!(agent.status, Status::Completed);
    let read = fixture_line(1)["tool_use_id"].as_str().unwrap().to_owned();
    assert!(session.ledger.row(Some("agent_sub"), &read).is_some());
}

/// Line `n` of the fixture.
fn fixture_line(n: usize) -> Value {
    serde_json::from_str(fixture()[n]).unwrap()
}

/// Cline wraps a typed prompt in `<user_input mode="…">`; the tag goes, with anything around it kept as it is.
#[test]
fn a_prompt_loses_cline_s_tag() {
    for (sent, prompt) in [
        ("<user_input mode=\"act\">fix it</user_input>", "fix it"),
        ("<user_input mode=\"plan\">fix it</user_input>\n", "fix it"),
        ("fix it", "fix it"),
        (
            "<user_input mode=\"act\">fix it",
            "<user_input mode=\"act\">fix it",
        ),
    ] {
        let studio = Studio::replayed([changed(0, json!({ "prompt": sent }))]);
        let session = studio.session(HELLO).unwrap();
        assert_eq!(session.turns[0].prompt, prompt, "{sent:?}");
    }
}

/// `apply_patch` takes its patch as `input` or as its whole input; either names the file it edits.
#[test]
fn an_apply_patch_edits_the_first_file_its_patch_names() {
    let patch = "*** Begin Patch\n*** Update File: /Users/me/work/hello/hello.txt\n@@\n-hi\n+bye\n*** End Patch";
    for input in [json!({ "input": patch }), json!(patch)] {
        let mut lines: Vec<String> = fixture()[..1].iter().map(ToString::to_string).collect();
        let call = json!({ "tool_name": "apply_patch", "tool_use_id": "toolu_patch", "tool_input": input });
        lines.push(changed(3, call.clone()));
        lines.push(changed(5, call));
        let studio = Studio::replayed(&lines);
        let row = studio
            .session(HELLO)
            .unwrap()
            .ledger
            .row(None, "toolu_patch")
            .unwrap();
        assert_eq!(
            row.file.as_deref(),
            Some("/Users/me/work/hello/hello.txt"),
            "{input}"
        );
    }
}

/// A headless `cline` ends its run with `submit_and_exit`, and Cline's final text is then its `Submission recorded
/// (…): <summary>` line, while the answer it showed is the text of its last message (`last_text`), 396 words against 87
/// in #372's trial. The reply is the longer of the two, so a short message before the submission keeps the summary.
#[test]
fn a_cline_reply_is_the_longer_of_its_final_text_and_its_last_message() {
    let submitted =
        "Submission recorded (verified): ran the tests, four passed, and explained calc.py.";
    let answer = "Test run: four tests passed. calc.py has add, sub, mul, div and pow_, and the tests miss pow_.";
    let end = fixture().len() - 1;
    for (last_text, reply) in [
        (Some(answer), answer),
        (Some("Done."), submitted),
        (Some(""), submitted),
        (None, submitted),
    ] {
        let mut lines: Vec<String> = fixture()[..end].iter().map(ToString::to_string).collect();
        let mut fields = json!({ "last_assistant_message": submitted });
        if let Some(text) = last_text {
            fields["last_text"] = json!(text);
        }
        lines.push(changed(end, fields));
        let studio = Studio::replayed(&lines);
        let session = studio.session(HELLO).unwrap();
        assert_eq!(
            session.turns[0].message.as_deref(),
            Some(reply),
            "{last_text:?}"
        );
    }
}

#[test]
fn a_line_the_adapter_does_not_map_is_no_drift() {
    let mut lines: Vec<String> = fixture().iter().map(ToString::to_string).collect();
    lines.insert(2, changed(1, json!({ "hook_event_name": "beforeModel" })));
    let studio = Studio::replayed(&lines);
    assert_eq!(studio.session(HELLO).unwrap().state, State::YourTurn);
    assert!(studio.agy_drift().is_empty(), "{:?}", studio.agy_drift());
}

/// Cline's `ask_question` takes one `question` and its `options`, not the `questions` list the page's question cards
/// read, and Cline reports no answer; so it is an ordinary call, not a question that waits on Daniel.
#[test]
fn an_ask_question_call_is_no_open_question() {
    let call = json!({
        "tool_name": "ask_question", "tool_use_id": "toolu_ask",
        "tool_input": { "question": "Which file?", "options": ["a", "b"] },
    });
    let lines = [fixture()[0].to_owned(), changed(3, call)];
    let studio = Studio::replayed(&lines);
    let session = studio.session(HELLO).unwrap();
    assert!(session.questions.is_empty(), "{:?}", session.questions);
    assert_eq!(session.state, State::Running);
}
