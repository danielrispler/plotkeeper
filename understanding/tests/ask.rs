//! "Ask about this" (#461, decision 20): a side question on a response, card or intention, answered
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
    fn reply(&self, _: &CardRequest, _: &[String], _: &str) -> Result<String, String> {
        unreachable!()
    }
    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        unreachable!()
    }
    fn rewrite(&self, _: &[&str], _: &[String], _: &str) -> Result<String, String> {
        unreachable!()
    }
    fn update(&self, _: &str, _: &str) -> Result<String, String> {
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
    let (card_run, _) = writer.update_command("brief", "workers");
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
