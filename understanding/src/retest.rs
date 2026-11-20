//! The one-week retest (danielrispler/plotkeeper#129): a week after a Quiz was offered, one fresh Quiz on the same decision,
//! from the turn's stored hunk and the reason of the first Quiz's key. It follows the same rules and guards as any Quiz
//! (`docs/studio/question-rules.md`) and waits on Home.

use crate::{CardStore, CardWriter, RETESTS_WAITING};
use crate::{ClaudeWriter, Quiz, RetestAsk, explanation_language, object, quiz_rules};
use std::{process::Command, sync::Mutex};

/// The retest writer's instructions.
const PROMPT: &str = concat!(
    r#"A week ago a developer was asked a multiple-choice question on a change a coding agent made. You write one new question on the same decision, to find out whether he still holds the model of the code that the first question taught. Ask it from another side: not the first question reworded, and not its options reshuffled.

Answer from the input below: the change's hunks, the first question and the reason of its right option. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"text": string, "options": [{"text": string, "why": string, "right": boolean}], "key": number}
- text: the question, under the rules below.
- options: 3 or 4 options, each {"text", "why", "right"}. why: one line on why that option is right or wrong; the options are shown in another order, so never refer to an option by its letter or place. right: true for the one right option, false for the others.
- key: the place of the right option in options, counting from 0.

"#,
    quiz_rules!("the first question"),
    "\n"
);

impl ClaudeWriter {
    /// The `claude -p` run that writes the retest of `ask`'s Quiz.
    pub fn retest_command(&self, ask: &RetestAsk) -> (Command, String) {
        let reason = &ask.first.options[ask.first.key].why;
        let kept = &ask.hunk[..ask.hunk.floor_char_boundary(crate::MAX_CHANGE)];
        self.run(&format!(
            "{PROMPT}\nThe first question: {}\nThe reason of its right option: {reason}\n\nThe change's hunks:\n{kept}",
            ask.first.text
        ))
    }
}

/// The retest in a writer's `reply`, or why it breaks the Quiz rules.
pub fn retest_quiz(reply: &str) -> Result<Quiz, String> {
    Quiz::from_value(serde_json::from_str(object(reply)).unwrap_or_default())
}

/// Writes the retests due at `now`, the caller's clock, in `CardStore::retests_to_write`'s order, and keeps each, until
/// `RETESTS_WAITING` wait unanswered (danielrispler/plotkeeper#215); no writer runs while they do. A Quiz whose retest
/// is held stays due. A reply that breaks a rule is asked for once more; a second rejection keeps no Quiz and logs one
/// line, and the Quiz is not asked about again. A writer that gives no reply keeps nothing and ends the run, so the
/// next call tries again. The store is not locked while the writer runs. Returns how many retests were written.
pub fn write_retests(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    now: u64,
    mut log: impl FnMut(String),
) -> usize {
    let (due, waiting) = {
        let store = store.lock().unwrap();
        (store.retests_to_write(now), store.waiting_retests(now))
    };
    let room = RETESTS_WAITING.saturating_sub(waiting);
    let mut written = 0;
    for ask in due {
        if written == room {
            break;
        }
        // Err holds whether the writer replied, and the line for the log.
        let attempt = || {
            let reply = writer.retest(&ask).map_err(|line| (false, line))?;
            Quiz::from_value(serde_json::from_str(object(&reply)).unwrap_or_default())
                .map_err(|line| (true, line))
        };
        let quiz = match attempt().or_else(|_| attempt()) {
            Ok(quiz) => Some(quiz),
            Err((replied, line)) => {
                log(format!("no retest on {}: {line}", ask.of));
                // A writer that gave no reply is tried again at the next run; one whose replies broke the rules is not.
                if !replied {
                    break;
                }
                None
            }
        };
        let has_quiz = quiz.is_some();
        match store.lock().unwrap().add_retest(&ask, quiz, now) {
            Ok(kept) => written += usize::from(kept && has_quiz),
            Err(e) => log(format!("save the retests: {e}")),
        }
    }
    written
}
