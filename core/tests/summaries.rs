//! Seam 1 (danielrispler/Falinks#517, Testing Decisions) for the turn summary (danielrispler/Falinks#526; danielrispler/Falinks#517, decisions 12, 13 and 16):
//! which turns get one, and the turn-end intention's one switch to its first line.

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

/// A transcript of its own for test `name`: Daniel's prompt, then `reply`.
fn transcript(name: &str, reply: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "studio-summaries-{name}-{}.jsonl",
        std::process::id()
    ));
    let prompt = r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"Explain the parser"}}"#;
    std::fs::write(&path, format!("{prompt}\n{}", assistant(reply))).unwrap();
    path
}

/// A session prompted and at its first step, with its transcript at `path`.
fn prompted(path: &std::path::Path) -> Studio {
    let path = path.display();
    Studio::replayed([
        format!(
            r#"{{"session_id":"{SESSION}","hook_event_name":"UserPromptSubmit","prompt":"Explain the parser","transcript_path":"{path}","cwd":"{CWD}","studio_received_ms":1793265000000}}"#
        ),
        format!(
            r#"{{"session_id":"{SESSION}","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{{"file_path":"{CWD}/src/parser.rs"}},"tool_use_id":"toolu_1","transcript_path":"{path}","cwd":"{CWD}","studio_received_ms":1793265001000}}"#
        ),
    ])
}

fn stop(path: &std::path::Path, reply: &str) -> String {
    let reply = serde_json::to_string(reply).unwrap();
    let path = path.display();
    format!(
        r#"{{"session_id":"{SESSION}","hook_event_name":"Stop","last_assistant_message":{reply},"transcript_path":"{path}","cwd":"{CWD}","studio_received_ms":1793265009000}}"#
    )
}

/// The summary request among a live `Stop`'s outputs, if any.
fn requested(outputs: Vec<Output>) -> Option<SummaryRequest> {
    outputs.into_iter().find_map(|o| match o {
        Output::Summary(request) => Some(request),
        _ => None,
    })
}

/// The turn ends on `reply`, after the transcript was read; the summary request, if any.
fn ended(name: &str, reply: &str, change: Option<Change>) -> (Studio, Option<SummaryRequest>) {
    let path = transcript(name, reply);
    let mut studio = prompted(&path);
    studio.follow_transcripts(|_| {});
    let changes = change
        .map(|c| (SESSION.to_owned(), c))
        .into_iter()
        .collect();
    let outputs = studio.apply_with_changes(&stop(&path, reply), changes);
    studio.follow_transcripts(|_| {});
    (studio, requested(outputs))
}

fn long_reply() -> String {
    let paragraph = "The parser reads one token at a time and keeps the open brackets on a stack. ";
    format!("Rewrote the parser.\n\n{}", paragraph.repeat(12))
}

#[test]
fn a_long_reply_gets_a_summary_request_with_the_turns_prompt_and_responses() {
    let reply = long_reply();
    let (_, request) = ended("long", &reply, None);

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
    let (_, asks) = ended(
        "asks",
        "Two ways fit.\n\nShall I keep the old parser?",
        None,
    );
    assert!(asks.is_some());

    let (_, short) = ended("short", "Saved as #09.", None);
    assert_eq!(short, None);
}

#[test]
fn a_turn_with_a_card_gets_a_summary_that_points_to_its_card() {
    let change = Change {
        base: Default::default(),
        diff: "--- a/src/parser.rs\n+++ b/src/parser.rs\n".into(),
        head: None,
    };
    let (_, request) = ended("card", &long_reply(), Some(change));

    let request = request.expect("a long reply gets a summary");
    assert_eq!(request.card, Some(format!("{SESSION}@1793265009000")));
}

#[test]
fn the_intention_switches_once_to_the_summarys_first_line() {
    let (mut studio, request) = ended("switch", &long_reply(), None);
    let request = request.unwrap();
    let intention = |studio: &Studio| {
        studio
            .session(SESSION)
            .unwrap()
            .transcript
            .intention
            .clone()
    };
    assert_eq!(intention(&studio).as_deref(), Some("Rewrote the parser."));

    assert!(studio.summarized(
        SESSION,
        &request.id,
        "The parser now keeps a bracket stack."
    ));
    studio.follow_transcripts(|_| {});
    assert_eq!(
        intention(&studio).as_deref(),
        Some("The parser now keeps a bracket stack.")
    );
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(
        transcript.intentions(),
        ["The parser now keeps a bracket stack."]
    );

    // A summary of an earlier turn moves nothing.
    assert!(!studio.summarized(SESSION, "an-earlier-turn", "Old news."));
}

#[test]
fn with_summaries_on_open_the_intention_stays_the_replys_first_line() {
    let (mut studio, request) = ended("on-open", &long_reply(), None);
    studio.set_summaries_on_open(true);

    assert!(!studio.summarized(
        SESSION,
        &request.unwrap().id,
        "The parser now keeps a bracket stack."
    ));
    studio.follow_transcripts(|_| {});
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(transcript.intention.as_deref(), Some("Rewrote the parser."));
}
