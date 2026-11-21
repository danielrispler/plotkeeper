//! Seam 1 (danielrispler/Falinks#517, Testing Decisions) for the turn summary (danielrispler/Falinks#526; danielrispler/Falinks#517, decisions 12, 13 and 16):
//! which turns get one. The turn-end intention no longer switches to the summary's first line
//! (danielrispler/plotkeeper#116).

use crate::common;
use serde_json::json;
use studio_core::{Change, Output, Studio, SummaryRequest};

const SESSION: &str = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
const CWD: &str = "/Users/danielrispler/work/Falinks";

/// An assistant line of the main agent writing `text`.
fn assistant(text: &str) -> String {
    let text = serde_json::to_string(text).unwrap();
    format!(
        r#"{{"isSidechain":false,"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":{text}}}]}}}}"#
    ) + "\n"
}

/// A transcript: Daniel's prompt, then `reply`.
fn transcript(reply: &str) -> tempfile::NamedTempFile {
    let prompt = r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"Explain the parser"}}"#;
    common::temp_file(&format!("{prompt}\n{}", assistant(reply)))
}

/// A hook event of the session at transcript `path`.
fn event(name: &str, path: &std::path::Path, at: u64, extra: serde_json::Value) -> String {
    let mut base = common::base(SESSION, name, CWD, at);
    base["transcript_path"] = json!(path);
    common::with(base, extra)
}

/// A session prompted and at its first step, with its transcript at `path`.
fn prompted(path: &std::path::Path) -> Studio {
    let step = json!({"tool_name": "Read", "tool_use_id": "toolu_1",
        "tool_input": {"file_path": format!("{CWD}/src/parser.rs")}});
    let prompt = json!({"prompt": "Explain the parser"});
    Studio::replayed([
        event("UserPromptSubmit", path, 1793265000000, prompt),
        event("PreToolUse", path, 1793265001000, step),
    ])
}

fn stop(path: &std::path::Path, reply: &str) -> String {
    let fields = json!({"last_assistant_message": reply});
    event("Stop", path, 1793265009000, fields)
}

/// The summary request among a live `Stop`'s outputs, if any.
fn requested(outputs: Vec<Output>) -> Option<SummaryRequest> {
    outputs.into_iter().find_map(|o| match o {
        Output::Summary(request) => Some(request),
        _ => None,
    })
}

/// The turn ends on `reply`, after the transcript was read; the summary request, if any.
fn ended(
    reply: &str,
    change: Option<Change>,
) -> (Studio, Option<SummaryRequest>, tempfile::NamedTempFile) {
    let file = transcript(reply);
    let path = file.path();
    let mut studio = prompted(path);
    studio.follow_transcripts(|_| {});
    let changes = change
        .map(|c| (SESSION.to_owned(), c))
        .into_iter()
        .collect();
    let outputs = studio.apply_with_changes(&stop(path, reply), changes);
    studio.follow_transcripts(|_| {});
    (studio, requested(outputs), file)
}

/// The outputs of a live `Stop` that changed code, in order.
fn stop_outputs() -> Vec<Output> {
    let reply = long_reply();
    let file = transcript(&reply);
    let path = file.path();
    let mut studio = prompted(path);
    studio.follow_transcripts(|_| {});
    let change = Change {
        base: Default::default(),
        diff: "--- a/src/parser.rs\n+++ b/src/parser.rs\n".into(),
        head: None,
    };
    let changes = [(SESSION.to_owned(), change)].into_iter().collect();
    studio.apply_with_changes(&stop(path, &reply), changes)
}

fn long_reply() -> String {
    let paragraph = "The parser reads one token at a time and keeps the open brackets on a stack. ";
    format!("Rewrote the parser.\n\n{}", paragraph.repeat(12))
}

#[test]
fn a_long_reply_gets_a_summary_request_with_the_turns_prompt_and_responses() {
    let reply = long_reply();
    let (_, request, _file) = ended(&reply, None);

    let request = request.expect("a long reply gets a summary");
    assert_eq!(request.session_id, SESSION);
    assert_eq!(request.id, format!("{SESSION}@1793265009000"));
    assert_eq!(request.prompt, "Explain the parser");
    assert_eq!(request.responses, [reply.trim()]);
    assert_eq!(request.intentions, ["Rewrote the parser."]);
    assert_eq!(request.card, None);
}

#[test]
fn a_turn_ending_in_a_question_gets_one_and_a_short_reply_does_not() {
    let (_, asks, _file) = ended("Two ways fit.\n\nShall I keep the old parser?", None);
    assert!(asks.is_some());

    let (_, short, _file) = ended("Saved as #09.", None);
    assert_eq!(short, None);
}

#[test]
fn a_turn_with_a_card_gets_a_summary_that_points_to_its_card() {
    let change = Change {
        base: Default::default(),
        diff: "--- a/src/parser.rs\n+++ b/src/parser.rs\n".into(),
        head: None,
    };
    let (_, request, _file) = ended(&long_reply(), Some(change));

    let request = request.expect("a long reply gets a summary");
    assert_eq!(request.card, Some(format!("{SESSION}@1793265009000")));
}

#[test]
fn the_turn_end_intention_stays_the_replys_first_line_when_the_summary_arrives() {
    let (mut studio, request, _file) = ended(&long_reply(), None);
    assert!(request.is_some());
    studio.follow_transcripts(|_| {});
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(transcript.intention.as_deref(), Some("Rewrote the parser."));
    assert_eq!(transcript.intentions(), ["Rewrote the parser."]);
}

#[test]
fn a_turns_card_is_queued_before_its_summary() {
    let outputs = stop_outputs();
    let at = |f: fn(&Output) -> bool| outputs.iter().position(f).expect("both are emitted");
    let card = at(|o| matches!(o, Output::Card(_)));
    let summary = at(|o| matches!(o, Output::Summary(_)));
    assert!(card < summary);
}
