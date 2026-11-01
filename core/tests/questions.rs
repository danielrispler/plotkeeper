//! Seam 1 (#369, Testing Decisions) for agent questions (#379): replay hook events through the core
//! and check each session's questions: what was asked, who asks, and whether Daniel answered it in
//! the terminal.

use serde_json::json;
use studio_core::{Answer, State, Studio, Wait};

/// The studio after the log's first `upto` lines, replayed.
fn replay_upto(log: &str, upto: usize) -> Studio {
    let mut studio = Studio::default();
    for line in log.lines().take(upto) {
        studio.replay(line);
    }
    studio
}

const ASK: &str = include_str!("fixtures/ask-then-plan.jsonl");
const ASKER: &str = "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95";

#[test]
fn a_question_shows_its_options_until_its_answer_marks_the_choice() {
    let asked = replay_upto(ASK, 2);
    let session = asked.session(ASKER).expect("the session");
    let [question] = &session.questions[..] else {
        panic!("one question: {:?}", session.questions);
    };
    assert_eq!(question.agent, None);
    assert_eq!(
        question.questions,
        json!([{
            "question": "Which README section should I fix?",
            "header": "Section",
            "multiSelect": false,
            "options": [
                {"label": "Install", "description": "The install steps"},
                {"label": "Usage", "description": "The usage examples"},
            ],
        }])
    );
    assert_eq!(
        (&question.answer, session.state),
        (&Answer::Open, State::NeedsYou)
    );

    // Its `PostToolUse` carries Daniel's choice.
    let answered = replay_upto(ASK, 5);
    let session = answered.session(ASKER).expect("the session");
    assert_eq!(
        session.questions[0].answer,
        Answer::Answered(json!({"Which README section should I fix?": "Install"}))
    );
    assert_eq!(session.state, State::Running);
}

/// A hand-written log: a session's background helper asks a question across the session's `Stop`,
/// and Daniel declines it.
const CHILD: &str = include_str!("fixtures/child-question.jsonl");
const CHILD_SESSION: &str = "b8d4f2a6-3c71-4e9b-8a05-6f1d2c7e9b43";
const HELPER: &str = "a2c6e8f0b4d1a3c5e";

#[test]
fn a_childs_question_is_its_own_and_names_it_on_its_sessions_line() {
    // Through the session's `Stop` (line 8).
    let studio = replay_upto(CHILD, 8);

    let session = studio.session(CHILD_SESSION).expect("the session");
    let [question] = &session.questions[..] else {
        panic!("one question: {:?}", session.questions);
    };
    assert_eq!(
        (question.agent.as_deref(), &question.answer),
        (Some(HELPER), &Answer::Open)
    );
    assert_eq!(question.questions[0]["options"][1]["label"], "Usage");
    let wait = Wait {
        agent: Some(HELPER.into()),
        since: 1_793_490_009_000,
    };
    assert_eq!(
        (session.state, &session.waiting),
        (State::NeedsYou, &Some(wait))
    );
}

#[test]
fn a_failed_question_is_not_answered_and_stops_needing_you() {
    // Through its `PostToolUseFailure` (line 9).
    let studio = replay_upto(CHILD, 9);

    let session = studio.session(CHILD_SESSION).expect("the session");
    assert_eq!(session.questions[0].answer, Answer::NotAnswered);
    assert_eq!((session.state, &session.waiting), (State::YourTurn, &None));
}

#[test]
fn a_question_whose_turn_ends_with_no_result_is_not_answered() {
    let lines: Vec<&str> = ASK.lines().collect();
    // The question (line 2), then the next prompt (line 7) with no result between: an interrupt.
    let log = [lines[0], lines[1], lines[6]].join("\n");

    let studio = replay_upto(&log, 3);

    let session = studio.session(ASKER).expect("the session");
    assert_eq!(session.questions[0].answer, Answer::NotAnswered);
}
