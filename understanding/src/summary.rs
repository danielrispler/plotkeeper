//! The turn summary and its learning section (#526; #517, decisions 12 to 15): one writer call
//! when a long turn ends, on a small model with the studio's own system prompt, read with the
//! session's last 3 summaries.

use crate::{CardStore, CardWriter, ClaudeWriter, MAX_CHANGE, Quiz, object};
use serde::{Deserialize, Serialize};
use std::{process::Command, sync::Mutex};

use crate::explanation_language;
use studio_core::SummaryRequest;

/// A turn's summary and, unless the turn has a card, its learning section.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnSummary {
    /// The turn's main ideas in a few lines; its first line is the turn-end intention.
    pub summary: String,
    /// `None` on a turn with a card, which teaches it instead, or when the writer's was unusable.
    pub learning: Option<Learning>,
}

/// The learning section: the turn's main idea and one question on it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Learning {
    pub idea: String,
    pub question: Quiz,
}

/// The summary writer's model and effort, from the settings (#517, decision 17). An empty effort
/// leaves the model's default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SummarySetup {
    pub model: String,
    pub effort: String,
}

impl Default for SummarySetup {
    /// Haiku at its default effort (#517, decision 14).
    fn default() -> Self {
        Self {
            model: "haiku".into(),
            effort: String::new(),
        }
    }
}

/// How many of the session's latest summaries the writer reads (#517, decision 15).
const EARLIER: usize = 3;

/// Marks a writer may put on the right option, which would give the answer away (#517, decision
/// 14).
const MARKS: [&str; 7] = ["✅", "✔️", "✔", "✓", "☑️", "(correct)", "(Correct)"];

impl TurnSummary {
    /// The summary in a writer's reply, which must hold one JSON object with a summary. On a turn
    /// with a `card`, any learning section is dropped. A learning section that is unusable is
    /// dropped too, with one line on why.
    pub fn from_reply(reply: &str, card: bool) -> Result<(TurnSummary, Option<String>), String> {
        let head: String = reply.chars().take(300).collect();
        let value: serde_json::Value = serde_json::from_str(object(reply))
            .map_err(|e| format!("the summary reply does not parse ({e}): {head:?}"))?;
        let summary = value["summary"].as_str().map(str::trim).unwrap_or_default();
        if summary.is_empty() {
            return Err(format!("the summary reply holds no summary: {head:?}"));
        }
        let learning = match card {
            true => Ok(None),
            false => learning(&value["learning"]).map(Some),
        };
        let (learning, problem) = match learning {
            Ok(learning) => (learning, None),
            Err(problem) => (None, Some(problem)),
        };
        let summary = summary.into();
        Ok((TurnSummary { summary, learning }, problem))
    }
}

/// The learning section in `value`, its options' marks removed, or why it is unusable.
fn learning(value: &serde_json::Value) -> Result<Learning, String> {
    let idea = value["idea"].as_str().map(str::trim).unwrap_or_default();
    if idea.is_empty() {
        return Err("its learning section has no idea".into());
    }
    let mut question = Quiz::from_value(value["question"].clone())?;
    for option in &mut question.options {
        let text = MARKS
            .iter()
            .fold(option.text.clone(), |t, m| t.replace(m, ""));
        option.text = text.trim().into();
    }
    Ok(Learning {
        idea: idea.into(),
        question,
    })
}

/// The summary of `request`'s turn from `writer`, read with the session's last 3 summaries in
/// `store`, which keeps it. The writer is asked once more when its reply gives none. Each problem
/// is one line for `log`. The store is not locked while the writer runs.
pub fn write_summary(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    setup: &SummarySetup,
    request: SummaryRequest,
    mut log: impl FnMut(String),
) -> Option<TurnSummary> {
    let earlier = store
        .lock()
        .unwrap()
        .latest_summaries(&request.session_id, EARLIER);
    let card = request.card.is_some();
    let summary = || TurnSummary::from_reply(&writer.summarize(setup, &request, &earlier)?, card);
    match summary().or_else(|_| summary()) {
        Ok((summary, problem)) => {
            if let Some(problem) = problem {
                log(format!("no learning section on {}: {problem}", request.id));
            }
            let id = request.id.clone();
            if let Err(e) = store.lock().unwrap().add_summary(request, summary.clone()) {
                log(format!("save the summaries of {id}: {e}"));
            }
            Some(summary)
        }
        Err(line) => {
            log(format!("no summary on {}: {line}", request.id));
            None
        }
    }
}

impl ClaudeWriter {
    /// The `claude -p` run for `request`'s turn summary, at `setup`'s model and effort with the
    /// studio's own system prompt, read with `earlier`, the session's latest summaries, oldest
    /// first. Past `MAX_CHANGE`, the responses keep their end, where the final reply is.
    pub fn summary_command(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> (Command, String) {
        let responses: String = (request.responses.iter())
            .map(|r| format!("<response>\n{r}\n</response>\n"))
            .collect();
        let responses =
            &responses[responses.ceil_char_boundary(responses.len().saturating_sub(MAX_CHANGE))..];
        let intentions: String = (request.intentions.iter())
            .map(|line| format!("- {line}\n"))
            .collect();
        let earlier: String = match earlier {
            [] => "none\n".into(),
            earlier => earlier
                .iter()
                .map(|s| {
                    let question = s
                        .learning
                        .as_ref()
                        .map_or("none", |l| l.question.text.as_str());
                    format!(
                        "<summary>\n{}\nIts question: {question}\n</summary>\n",
                        s.summary
                    )
                })
                .collect(),
        };
        let card = match request.card {
            Some(_) => "This turn has a card, which teaches it: write the summary only.",
            None => "Write the summary and the learning section.",
        };
        let mut options = vec!["--model", &setup.model];
        if !setup.effort.is_empty() {
            options.extend(["--effort", &setup.effort]);
        }
        options.extend(["--system-prompt", SUMMARY]);
        self.run_with(
            &options,
            &format!(
                "The developer's prompt:\n<prompt>\n{}\n</prompt>\n\n\
                 The agent's responses in the turn, oldest first:\n{responses}\n\
                 The turn's intentions, oldest first:\n{intentions}\n\
                 The session's latest turn summaries and their questions, oldest first:\n\
                 {earlier}\n{card}",
                request.prompt
            ),
        )
    }
}

/// The summary writer's system prompt (#517, decision 14), which replaces Claude Code's own.
const SUMMARY: &str = concat!(
    r#"You write the turn summary of one turn of a coding agent's session, for the developer who runs it and wants to learn from the turn instead of reading its raw output. Under the summary sits a learning section: the turn's main idea and one multiple-choice question on it, which he answers when he chooses.

Answer from the input: his prompt, the agent's responses in the turn, its intentions, and the session's latest turn summaries with their questions. Connect to those earlier turns where it helps, and never ask the same question twice. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string, "learning": {"idea": string, "question": {"text": string, "options": [{"text": string, "why": string}], "key": number}}}
- summary: the turn's main ideas in at most 4 short lines. Its first line says what the turn did or found, or what it asks the developer.
- learning.idea: the one idea from the turn worth keeping, in one or two sentences.
- learning.question.text: one question on that idea that the turn answers.
- learning.question.options: 3 or 4 options. Each wrong option is plausible, of the right option's length and tone. Never mark the right option in its text, with a sign, an emoji or a word: the options are shown in another order, and the key alone says which is right.
- learning.question.options[].why: one line on why that option is right or wrong, never naming an option by its letter or place.
- learning.question.key: the place of the right option, counting from 0.

When the input says the turn has a card, reply with {"summary": string} only.
"#
);
