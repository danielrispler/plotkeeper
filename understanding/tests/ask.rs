//! "Ask about this" (danielrispler/Falinks#461, decision 20): a side question on a response, card or intention, answered
//! by the card writer's own call with the turn's intentions; the session never sees it.

use std::cell::RefCell;
use studio_core::{CardRequest, Transcript};
use studio_understanding::{Card, CardWriter, ClaudeWriter, ask_about};

/// What a writer was asked: the item, the turn's intentions and the question.
type Asking = (String, Vec<String>, String);

/// A fake writer that records what it was asked and gives `reply`.
struct Asked {
    reply: Result<String, String>,
    asked: RefCell<Vec<Asking>>,
}

impl Asked {
    fn replying(reply: Result<&str, &str>) -> Self {
        Self {
            reply: reply.map(Into::into).map_err(Into::into),
            asked: RefCell::default(),
        }
    }
}

impl CardWriter for Asked {
    fn reply(&self, _: &CardRequest, _: &[String]) -> Result<String, String> {
        unreachable!()
    }
    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        unreachable!()
    }
    fn ask(&self, item: &str, intentions: &[String], question: &str) -> Result<String, String> {
        let asked = (item.into(), intentions.to_vec(), question.into());
        self.asked.borrow_mut().push(asked);
        self.reply.clone()
    }
}

fn transcript() -> Transcript {
    Transcript {
        intention: Some("Now I run the tests.".into()),
        steps_ago: 2,
        responses: vec![
            "I read the issue first.\nThen I look at the writer.".into(),
            "The writer is fine.\n\nNow I run the tests.".into(),
        ],
        unavailable: None,
        ..Transcript::default()
    }
}

#[test]
fn the_request_carries_the_item_the_question_and_the_turns_intentions_and_the_answer_comes_back() {
    let writer = Asked::replying(Ok(r#"{"answer": "It checks the writer first."}"#));

    let answer = ask_about(&writer, &transcript(), "The writer is fine.", "Why first?");

    assert_eq!(answer, Ok("It checks the writer first.".into()));
    let intentions = vec![
        "Then I look at the writer.".to_string(),
        "Now I run the tests.".to_string(),
    ];
    assert_eq!(
        writer.asked.into_inner(),
        [(
            "The writer is fine.".to_string(),
            intentions,
            "Why first?".to_string()
        )]
    );
}

#[test]
fn an_ended_turns_final_intention_is_its_first_line_not_its_footer() {
    let writer = Asked::replying(Ok(r#"{"answer": "It ran them."}"#));
    let mut transcript = transcript();
    transcript.responses[1].push_str("\n\nNot checked: the CI run.");
    transcript.ended = true;

    ask_about(&writer, &transcript, "item", "Did it test?").unwrap();

    let (_, intentions, _) = writer.asked.into_inner().remove(0);
    assert_eq!(
        intentions,
        ["Then I look at the writer.", "The writer is fine."]
    );
}

#[test]
fn a_reply_that_does_not_parse_or_a_failed_call_gives_an_error() {
    let replies = [
        Ok("Shall I read the file?"),
        Ok(r#"{"answer": "  "}"#),
        Err("`claude -p` printed nothing"),
    ];
    for reply in replies {
        let writer = Asked::replying(reply);
        let answer = ask_about(&writer, &transcript(), "item", "question");
        assert!(answer.is_err(), "{reply:?} gave {answer:?}");
    }
}

#[test]
fn the_real_call_runs_as_the_card_writer_does_with_tools_off_in_english() {
    let writer = ClaudeWriter::new("/opt/homebrew/bin/claude".into(), "/f".into());
    let (card_run, _) = writer.goal_command(&["a prompt"]);
    let intentions = ["Then I look at the writer.".to_string()];

    let (run, prompt) = writer.ask_command("The writer is fine.", &intentions, "Why first?");

    assert!(run.get_args().eq(card_run.get_args()));
    assert!(run.get_envs().eq(card_run.get_envs()));
    assert_eq!(run.get_current_dir(), card_run.get_current_dir());
    assert!(
        prompt.contains("Write in English, in plain words"),
        "{prompt}"
    );
    assert!(prompt.contains("do not use tools"), "{prompt}");
    assert!(
        prompt.contains("- Then I look at the writer.\n"),
        "{prompt}"
    );
    assert!(
        prompt.contains("<item>\nThe writer is fine.\n</item>"),
        "{prompt}"
    );
    assert!(prompt.ends_with("Why first?"), "{prompt}");
}

#[test]
fn opening_ask_on_an_anchor_shows_its_facts_and_never_calls_the_writer() {
    // The facts are the core's, read from the ledger and the log: `ask_facts` takes no writer, and the stand-in
    // that would record a call stays untouched.
    let writer = Asked::replying(Ok("never"));
    let session = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
    let prompt = serde_json::json!({
        "session_id": session, "hook_event_name": "UserPromptSubmit", "cwd": "/Users/danielrispler/work/Falinks", "prompt_id": "p1",
        "prompt": "Fix it", "studio_received_ms": 1u64,
    })
    .to_string();
    let studio = studio_core::Studio::replayed([&prompt]);
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(&log, format!("{prompt}\n")).unwrap();
    let anchor = studio_core::Anchor::Headline { turn: 0 };
    let facts = studio.ask_facts(session, &anchor, &log).unwrap();
    assert_eq!(facts.sections[0].facts[0].text, "Fix it");
    assert!(writer.asked.borrow().is_empty());
}

fn parse_session() -> (studio_core::Studio, std::path::PathBuf, tempfile::TempDir) {
    let session = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
    let line = |name: &str, ms: u64, extra: serde_json::Value| {
        let mut l = serde_json::json!({ "session_id": session, "hook_event_name": name, "cwd": "/w/repo", "prompt_id": "p1",
            "studio_received_ms": 1_793_265_000_000u64 + ms });
        l.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        l.to_string()
    };
    let edit = serde_json::json!({ "tool_name": "Edit", "tool_input": { "file_path": "/w/repo/src/parse.rs" }, "tool_use_id": "e1" });
    let lines = vec![
        line(
            "UserPromptSubmit",
            0,
            serde_json::json!({ "prompt": "Fix the parser" }),
        ),
        line("PreToolUse", 1_000, edit.clone()),
        line("PostToolUse", 1_050, edit),
    ];
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    std::fs::write(
        &log,
        lines.iter().map(|l| format!("{l}\n")).collect::<String>(),
    )
    .unwrap();
    (studio_core::Studio::replayed(&lines), log, dir)
}

const SESSION: &str = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";

#[test]
fn a_question_on_a_file_gives_the_writer_its_editing_steps_the_prompt_and_the_hunk() {
    let (studio, log, _dir) = parse_session();
    let anchor = studio_core::Anchor::File {
        turn: 0,
        root: "/w/repo".into(),
        path: "src/parse.rs".into(),
        from: None,
    };
    let facts = studio.ask_facts(SESSION, &anchor, &log).unwrap();
    let writer = Asked::replying(Ok(r#"{"answer": "Because it fixes the parser."}"#));

    let item = studio_understanding::ask_item("this file", &facts, Some("-old\n+new"));
    ask_about(&writer, &transcript(), &item, "Why did it touch this file?").unwrap();

    let (item, _, question) = writer.asked.into_inner().remove(0);
    assert!(item.contains("Daniel asked:\nFix the parser"), "{item}");
    assert!(
        item.contains("Steps that edited it:\nEdit by the session's agent"),
        "{item}"
    );
    assert!(item.contains("The hunk:\n-old\n+new"), "{item}");
    assert_eq!(question, "Why did it touch this file?");
}

#[test]
fn a_question_on_a_failed_check_gives_its_command_exit_code_and_output_tail() {
    let facts = studio_core::AskFacts {
        sections: [
            "Command:npm run check",
            "Exit code:2",
            "End of the output:error: it broke",
        ]
        .iter()
        .map(|s| {
            let (label, text) = s.split_once(':').unwrap();
            studio_core::AskSection {
                label: label.into(),
                facts: vec![studio_core::AskFact {
                    at: None,
                    text: text.into(),
                }],
            }
        })
        .collect(),
    };
    let item = studio_understanding::ask_item("this check", &facts, None);
    assert!(
        item.contains(
            "Command:\nnpm run check\n\nExit code:\n2\n\nEnd of the output:\nerror: it broke"
        ),
        "{item}"
    );
    assert!(!item.contains("The hunk"), "{item}");
}

#[test]
fn why_didnt_it_touch_a_name_the_session_never_saw_says_so_with_no_model_call() {
    let (studio, _log, _dir) = parse_session();
    let writer = Asked::replying(Ok("never"));
    let facts = studio.ask_named(SESSION, 0, "lexer.rs").unwrap();

    let result = studio_understanding::why_not(&writer, &transcript(), "lexer.rs", facts).unwrap();

    assert!(result.facts.sections.is_empty());
    assert_eq!(result.reasons, None);
    assert!(writer.asked.borrow().is_empty());
}

#[test]
fn why_didnt_it_touch_a_name_it_read_gives_the_writer_the_recorded_facts_and_returns_the_reasons_apart()
 {
    let (studio, _log, _dir) = parse_session();
    let writer = Asked::replying(Ok(r#"{"answer": "Perhaps the prompt did not need it."}"#));
    let facts = studio.ask_named(SESSION, 0, "parser").unwrap();

    let result = studio_understanding::why_not(&writer, &transcript(), "parser", facts).unwrap();

    assert_eq!(
        result.reasons.as_deref(),
        Some("Perhaps the prompt did not need it.")
    );
    assert_eq!(result.facts.sections[0].label, "Named in Daniel's prompt");
    let (item, _, question) = writer.asked.into_inner().remove(0);
    assert!(
        item.contains("Named in Daniel's prompt:\nFix the parser"),
        "{item}"
    );
    assert!(question.contains("possible reasons"), "{question}");
}
