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
    /// `None` on a turn with a card, which teaches it instead, on a turn with nothing to teach
    /// (#583), or when the writer's was unusable.
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

/// A right option longer than this many times the longest wrong option gives the answer away
/// (#583). Of Haiku's 3 real questions, the 2 guessable ones' right options were 1.30 and 1.60
/// times it, and the fair one's 0.85.
const KEY_RATIO: f64 = 1.25;

/// Why a learning section whose right option is clearly the longest is unusable; a reply with one
/// is asked for once more.
const GUESSABLE: &str = "its right option is much longer than every wrong one";

impl TurnSummary {
    /// The summary in a writer's reply, which must hold one JSON object with a summary. On a turn
    /// with a `card`, any learning section is dropped; a `null` one means the turn has nothing to
    /// teach. A learning section that is unusable is dropped too, with one line on why.
    pub fn from_reply(reply: &str, card: bool) -> Result<(TurnSummary, Option<String>), String> {
        let head: String = reply.chars().take(300).collect();
        let value: serde_json::Value = serde_json::from_str(object(reply))
            .map_err(|e| format!("the summary reply does not parse ({e}): {head:?}"))?;
        let summary = value["summary"].as_str().map(str::trim).unwrap_or_default();
        if summary.is_empty() {
            return Err(format!("the summary reply holds no summary: {head:?}"));
        }
        let learning = match card || value["learning"].is_null() {
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
    let length = |o: &crate::Choice| o.text.chars().count() as f64;
    let key = question.options.get(question.key).map_or(0.0, length);
    let wrong = (question.options.iter().enumerate())
        .filter(|(i, _)| *i != question.key)
        .map(|(_, o)| length(o))
        .fold(0.0, f64::max);
    if key > KEY_RATIO * wrong {
        return Err(GUESSABLE.into());
    }
    Ok(Learning {
        idea: idea.into(),
        question,
    })
}

/// The summary of `request`'s turn from `writer`, read with the session's last 3 summaries in
/// `store`, which keeps it. The writer is asked once more when its reply gives none, or gives a
/// learning section whose right option is guessable by its length; the second reply stands, or the
/// first when the second gives no summary. Each problem
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
    let first = summary();
    let retry = match &first {
        Err(_) => true,
        Ok((_, problem)) => problem.as_deref() == Some(GUESSABLE),
    };
    match if retry { summary().or(first) } else { first } {
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
        self.run_with(
            &options(setup, SUMMARY),
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

/// The options of a summary run: `setup`'s model and effort, and `system`, the studio's own system
/// prompt.
fn options<'a>(setup: &'a SummarySetup, system: &'a str) -> Vec<&'a str> {
    let mut options = vec!["--model", &setup.model];
    if !setup.effort.is_empty() {
        options.extend(["--effort", &setup.effort]);
    }
    options.extend(["--system-prompt", system]);
    options
}

/// The summary of subagent `id`'s purpose and result (#535): the one `store` keeps, or else one
/// call to `writer` on its spawn's `prompt` and its last `message`, which the store then keeps.
/// When the call gives none, nothing is kept, one line goes to `log`, and the rail keeps the
/// subagent's name and first line. The store is not locked while the writer runs.
pub fn write_agent_summary(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    setup: &SummarySetup,
    id: &str,
    prompt: &str,
    message: &str,
    mut log: impl FnMut(String),
) -> Option<String> {
    if let Some(kept) = store.lock().unwrap().agent_summary(id) {
        return Some(kept.into());
    }
    let summary = writer
        .summarize_agent(setup, prompt, message)
        .and_then(|reply| {
            let value: serde_json::Value = serde_json::from_str(object(&reply)).unwrap_or_default();
            match value["summary"].as_str().map(str::trim) {
                Some(summary) if !summary.is_empty() => Ok(summary.to_string()),
                _ => Err(format!(
                    "the reply holds none: {:?}",
                    reply.chars().take(300).collect::<String>()
                )),
            }
        });
    match summary {
        Ok(summary) => {
            if let Err(e) = store.lock().unwrap().add_agent_summary(id, &summary) {
                log(format!("save the summary of subagent {id}: {e}"));
            }
            Some(summary)
        }
        Err(line) => {
            log(format!("no summary on subagent {id}: {line}"));
            None
        }
    }
}

impl ClaudeWriter {
    /// The `claude -p` run for a subagent's summary, at `setup`'s model and effort with the studio's
    /// own system prompt, on its spawn's `prompt` and its last `message`, each cut at `MAX_CHANGE`.
    pub fn agent_summary_command(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        message: &str,
    ) -> (Command, String) {
        let cut = |text: &str| text[..text.floor_char_boundary(MAX_CHANGE)].to_string();
        self.run_with(
            &options(setup, AGENT_SUMMARY),
            &format!(
                "The prompt it was spawned with:\n<prompt>\n{}\n</prompt>\n\n\
                 Its last message:\n<message>\n{}\n</message>",
                cut(prompt),
                cut(message)
            ),
        )
    }
}

/// The subagent summary writer's system prompt (#535), which replaces Claude Code's own.
const AGENT_SUMMARY: &str = concat!(
    r#"You write a short summary of one subagent of a coding agent: what it was asked to do, and what it came back with, for the developer who runs it and reads the summary instead of its raw output.

Answer from the input: the prompt the subagent was spawned with, and its last message. Use only this input: do not use tools or read files. Skip any preamble such as "Done." and say the result itself.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string}
- summary: at most 3 short lines. The first says what it was asked to do, the next what it found or changed, or why it failed.
"#
);

/// The summary writer's system prompt (#517, decision 14), which replaces Claude Code's own.
const SUMMARY: &str = concat!(
    r#"You write the turn summary of one turn of a coding agent's session, for the developer who runs it and wants to learn from the turn instead of reading its raw output. Under the summary sits, when the turn has something to teach, a learning section: the turn's main idea and one multiple-choice question on it, which he answers when he chooses.

Answer from the input: his prompt, the agent's responses in the turn, its intentions, and the session's latest turn summaries with their questions. Connect to those earlier turns where it helps, and never ask the same question twice. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string, "learning": {"idea": string, "question": {"text": string, "options": [{"text": string, "why": string}], "key": number}}}
- summary: the turn's main ideas in at most 4 short lines. Its first line says what the turn did or found, or what it asks the developer.
- learning.idea: the one idea from the turn worth keeping, in one or two sentences.
- learning.question.text: one question on that idea that asks why something works or how a mechanism behaves, so its answer carries over beyond this session. Never ask what the agent did or decided.
- learning.question.options: 3 or 4 options. Each wrong option is plausible: a mistake a developer could really make, of the right option's length, specificity and tone. The right option is not the longest or most specific one. Never mark the right option in its text, with a sign, an emoji or a word: the options are shown in another order, and the key alone says which is right.
- learning.question.options[].why: one line on why that option is right or wrong, never naming an option by its letter or place.
- learning.question.key: the place of the right option, counting from 0.

A number taken from the input keeps its bound or condition, such as "upper bound", "on fixtures" or "median", wherever it appears.

When the turn holds no idea that carries over beyond this session, such as orchestration, status, merges or bookkeeping, reply with {"summary": string, "learning": null}. The summary is still written: a question made up for such a turn wastes the developer's time.

When the input says the turn has a card, reply with {"summary": string} only.
"#
);
