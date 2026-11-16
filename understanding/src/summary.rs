//! The turn summary and its Quiz (danielrispler/Falinks#526; danielrispler/Falinks#517, decisions 12 to 15): one writer call
//! when a long turn ends, on a small model with the studio's own system prompt, read with the
//! session's last 3 summaries.

use crate::grounding::{HEADLINE_WORDS, agent_says, facts_text, grounded, material, words};
use crate::{
    CardStore, CardWriter, ClaudeWriter, MAX_CHANGE, Place, Quiz, millis, object, quiz_rules,
};
use serde::{Deserialize, Serialize};
use std::{process::Command, sync::Mutex};

use crate::explanation_language;
use studio_core::SummaryRequest;

/// What a Body line is (danielrispler/plotkeeper#115).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// Why the agent did it.
    Why,
    /// What the agent says, which no recorded fact shows.
    Reported,
    /// What was not done or not checked.
    Open,
    /// What the agent asks Daniel.
    Ask,
}

/// A line under the Headline.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub kind: LineKind,
    pub text: String,
}

/// The most Body lines a turn has.
const LINES: usize = 3;

/// A turn's summary and, unless the turn has a card, its Quiz.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnSummary {
    /// The Headline and the Body lines' texts, one per line. Summaries stored before the Headline
    /// existed hold only this: their first line reads as the Headline, the rest as `reported` lines.
    pub summary: String,
    /// The Headline; `None` on a stored summary of before it, and empty when the guards stripped
    /// it, so that the rail shows the turn's own first line.
    #[serde(default)]
    pub headline: Option<String>,
    /// The Body lines, at most 3, typed.
    #[serde(default)]
    pub lines: Vec<Line>,
    /// `None` on a turn with a card, whose Quiz is the card's, on a turn with nothing to teach
    /// (danielrispler/Falinks#583), or when the writer's was rejected twice. A learning section stored before
    /// danielrispler/plotkeeper#118 reads as its question (`stored_quiz`).
    #[serde(default, alias = "learning", deserialize_with = "stored_quiz")]
    pub quiz: Option<Quiz>,
}

/// A stored Quiz, or a stored learning section, `{idea, question}`, whose question is the Quiz.
fn stored_quiz<'de, D: serde::Deserializer<'de>>(from: D) -> Result<Option<Quiz>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Learning { question: Quiz },
        Quiz(Quiz),
    }
    Ok(match Option::<Stored>::deserialize(from)? {
        Some(Stored::Learning { question }) | Some(Stored::Quiz(question)) => Some(question),
        None => None,
    })
}

/// The summary writer's model and effort, from the settings (danielrispler/Falinks#517, decision 17). An empty effort
/// leaves the model's default.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SummarySetup {
    pub model: String,
    pub effort: String,
}

impl Default for SummarySetup {
    /// Haiku at its default effort (danielrispler/Falinks#517, decision 14).
    fn default() -> Self {
        Self {
            model: "haiku".into(),
            effort: String::new(),
        }
    }
}

/// How many of the session's latest summaries the writer reads (danielrispler/Falinks#517, decision 15).
const EARLIER: usize = 3;

/// Why a summary's Quiz or Headline is unusable.
enum Unusable {
    /// Its Quiz breaks a rule (`Quiz::from_value`); a reply with one is asked for once more.
    Quiz(String),
    /// Its Headline held a number or id the turn's material lacks, or ran over its words; the
    /// reply is asked for once more.
    Headline(String),
    Other(String),
}

impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Unusable::Quiz(why) | Unusable::Headline(why) | Unusable::Other(why) => {
                f.write_str(why)
            }
        }
    }
}

impl TurnSummary {
    /// The summary in a writer's reply, which must hold one JSON object with a headline. Its Body
    /// lines of an unknown kind, or with a number or id that `material` lacks, are dropped. A
    /// Headline with such a number is stripped, and one over `HEADLINE_WORDS` stands, each with why
    /// so the reply is asked for once more; a claim of a fix, a pass or a verification gets "Agent
    /// says:" unless a check backs it. On a turn with a `card`, any Quiz is dropped; a `null` one
    /// means the turn has nothing to teach, and a missing one is unusable. A Quiz that breaks a
    /// rule is dropped too, with why, and the reply is asked for once more.
    fn from_reply(
        reply: &str,
        request: &SummaryRequest,
        material: &str,
    ) -> Result<(TurnSummary, Option<Unusable>), String> {
        let card = request.card.is_some();
        let head: String = reply.chars().take(300).collect();
        let value: serde_json::Value = serde_json::from_str(object(reply))
            .map_err(|e| format!("the summary reply does not parse ({e}): {head:?}"))?;
        let headline = value["headline"]
            .as_str()
            .map(str::trim)
            .unwrap_or_default();
        if headline.is_empty() {
            return Err(format!("the summary reply holds no headline: {head:?}"));
        }
        let lines: Vec<Line> = (value["lines"].as_array().into_iter().flatten())
            .filter_map(|line| {
                let kind = serde_json::from_value(line["kind"].clone()).ok()?;
                let text = line["text"].as_str()?.trim();
                Some(Line {
                    kind,
                    text: text.into(),
                })
            })
            .filter(|line| !line.text.is_empty() && grounded(&line.text, material))
            .take(LINES)
            .collect();
        let mut headline_problem = None;
        let headline = if !grounded(headline, material) {
            headline_problem = Some(Unusable::Headline(format!(
                "its headline holds a number or id the turn lacks: {headline:?}"
            )));
            String::new()
        } else {
            if words(headline) > HEADLINE_WORDS {
                headline_problem = Some(Unusable::Headline(format!(
                    "its headline has {} words",
                    words(headline)
                )));
            }
            agent_says(headline, &request.facts)
        };
        let quiz = match card || value.get("quiz") == Some(&serde_json::Value::Null) {
            true => Ok(None),
            false => quiz(&value["quiz"]).map(Some),
        };
        let (quiz, problem) = match quiz {
            Ok(quiz) => (quiz, headline_problem),
            Err(problem) => (None, headline_problem.or(Some(problem))),
        };
        let texts = lines.iter().map(|l| l.text.as_str());
        let summary = std::iter::once(headline.as_str())
            .filter(|h| !h.is_empty())
            .chain(texts)
            .collect::<Vec<_>>()
            .join("\n");
        let summary = TurnSummary {
            summary,
            headline: Some(headline),
            lines,
            quiz,
        };
        Ok((summary, problem))
    }
}

/// The Quiz in `value`, or why it is unusable.
fn quiz(value: &serde_json::Value) -> Result<Quiz, Unusable> {
    if value.is_null() {
        return Err(Unusable::Other("the reply holds no quiz".into()));
    }
    Quiz::from_value(value.clone()).map_err(Unusable::Quiz)
}

/// The summary of `request`'s turn from `writer`, read with the session's last 3 summaries in
/// `store`, which keeps it. The writer is asked once more when its reply gives none, gives a
/// Headline with an invented number or over 16 words, or gives a Quiz that breaks a rule
/// (`Quiz::from_value`); the second reply stands, with no Quiz when it breaks a rule again, or the
/// first when the second gives no summary. Each problem
/// is one line for `log`. The store is not locked while the writer runs.
pub fn write_summary(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    setup: &SummarySetup,
    mut request: SummaryRequest,
    mut log: impl FnMut(String),
) -> Option<TurnSummary> {
    // A card the offer did not write is no card: the turn's summary may carry its Quiz instead.
    if let Some(card) = &request.card
        && !store
            .lock()
            .unwrap()
            .cards()
            .iter()
            .any(|k| k.request.id == *card)
    {
        request.card = None;
    }
    let earlier = store
        .lock()
        .unwrap()
        .latest_summaries(&request.session_id, EARLIER);
    let texts: Vec<&str> = earlier.iter().map(|s| s.summary.as_str()).collect();
    let material = material(&request, &texts.join("\n"));
    let summary = || {
        TurnSummary::from_reply(
            &writer.summarize(setup, &request, &earlier)?,
            &request,
            &material,
        )
    };
    let first = summary();
    let retry = match &first {
        Err(_) => true,
        Ok((_, problem)) => matches!(problem, Some(Unusable::Quiz(_) | Unusable::Headline(_))),
    };
    match if retry { summary().or(first) } else { first } {
        Ok((mut summary, problem)) => {
            if let Some(problem) = problem {
                log(format!("no quiz on {}: {problem}", request.id));
            }
            let id = request.id.clone();
            let mut store = store.lock().unwrap();
            quiz_cap(&mut store, &request, &mut summary, &mut log);
            if let Err(e) = store.add_summary(request, summary.clone()) {
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

/// Holds the Quiz of `summary` and of its turn's card to the rules for when a Quiz shows
/// (danielrispler/plotkeeper#119): none beside a decision, a turn that asks Daniel something or ended in Needs
/// you, and at most 3 in an hour, which a lone Quiz past the cap does not get.
fn quiz_cap(
    store: &mut CardStore,
    request: &SummaryRequest,
    summary: &mut TurnSummary,
    mut log: impl FnMut(String),
) {
    let asks = request.asks || summary.lines.iter().any(|l| l.kind == LineKind::Ask);
    if asks {
        summary.quiz = None;
        if let Some(card) = &request.card
            && let Err(e) = store.drop_card_quiz(card)
        {
            log(format!("save the card store: {e}"));
        }
    } else if summary.quiz.is_some() && store.place_quiz(false, millis()) != Place::Offer {
        summary.quiz = None;
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
                    let question = s.quiz.as_ref().map_or("none", |q| q.text.as_str());
                    format!(
                        "<summary>\n{}\nIts question: {question}\n</summary>\n",
                        s.summary
                    )
                })
                .collect(),
        };
        let card = match request.card {
            Some(_) => {
                "This turn has a card, which asks its quiz: write the headline and lines only."
            }
            None => "Write the headline, the lines and the quiz.",
        };
        self.run_with(
            &options(setup, SUMMARY),
            &format!(
                "The developer's prompt:\n<prompt>\n{}\n</prompt>\n\n\
                 The agent's responses in the turn, oldest first:\n{responses}\n\
                 The turn's intentions, oldest first:\n{intentions}\n\
                 What the hooks and git recorded of the turn:\n{facts}\n\
                 The session's latest turn summaries and their questions, oldest first:\n\
                 {earlier}\n{card}",
                request.prompt,
                facts = facts_text(&request.facts)
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

/// The summary of subagent `id`'s purpose and result (danielrispler/Falinks#535): the one `store` keeps, or else one
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

/// The subagent summary writer's system prompt (danielrispler/Falinks#535), which replaces Claude Code's own.
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

/// The summary writer's system prompt (danielrispler/Falinks#517, decision 14; danielrispler/plotkeeper#115), which replaces
/// Claude Code's own.
const SUMMARY: &str = concat!(
    r#"You write the turn summary of one turn of a coding agent's session, for the developer who runs it and wants to check what happened instead of reading its raw output: a headline and up to three lines. Under them sits, when the turn has something to teach, a quiz: one multiple-choice question that he answers when he chooses.

Answer from the input: his prompt, the agent's responses in the turn, its intentions, what the hooks and git recorded of the turn (its files with their line counts, its checks and its hunks), and the session's latest turn summaries with their questions. Connect to those earlier turns where it helps, and never ask the same question twice. Use only this input: do not use tools or read files.

The studio shows the recorded files and checks itself, next to your headline. Do not restate them: no file lists, no line counts, no check results. Write what they cannot say. Never write a number, an issue or PR id or a commit SHA that the input does not hold; say "one" or "a few" in words instead. Say "fixed", "passes" or "verified" only when a check in the recorded facts ended passed; otherwise attribute the claim to the agent.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"headline": string, "lines": [{"kind": "why" | "reported" | "open" | "ask", "text": string}], "quiz": {"text": string, "options": [{"text": string, "why": string, "right": boolean}], "key": number}}
- headline: at most 16 words, one sentence that states an outcome, or what the turn waits on. Never narrate ("Looked at", "Started by"). When the turn ends by asking the developer something, the headline is that ask.
- lines: at most 3, each at most 20 words, each typed. why: the reason for a choice the turn made. reported: something the agent claims that the recorded facts do not show. open: what was left undone or was not checked, such as "Nobody looked at the rows in a real window." ask: a question or decision the agent puts to the developer. Leave out a line that says nothing more than the headline.
- quiz.text: the quiz's question, under the rules below.
- quiz.options: 3 or 4 options, each {"text", "why", "right"}. why: one line on why that option is right or wrong, never naming an option by its letter or place. right: true for the one right option, false for the others.
- quiz.key: the place of the right option, counting from 0.

A number taken from the input keeps its bound or condition, such as "upper bound", "on fixtures" or "median", wherever it appears.

"#,
    quiz_rules!("your headline"),
    r#"

When the turn holds nothing worth a quiz, such as orchestration, status, merges or bookkeeping, reply with {"headline": string, "lines": [...], "quiz": null}. The headline is still written: a question made up for such a turn wastes the developer's time.

When the input says the turn has a card, reply with {"headline": string, "lines": [...]} only.
"#
);

/// How many words a session's name may have at most.
const NAME_WORDS: usize = 6;

/// The name of a session from `writer`, on Daniel's first `prompt` and, for a bare slash command,
/// the turn's final `response`: the reply's `name`, cut to a few words. One line goes to `log`
/// when the call gives none.
pub fn write_name(
    writer: &(impl CardWriter + ?Sized),
    setup: &SummarySetup,
    prompt: &str,
    response: Option<&str>,
    mut log: impl FnMut(String),
) -> Option<String> {
    let name = writer.name(setup, prompt, response).and_then(|reply| {
        let value: serde_json::Value = serde_json::from_str(object(&reply)).unwrap_or_default();
        let name = value["name"].as_str().unwrap_or_default();
        let words: Vec<&str> = name.split_whitespace().take(NAME_WORDS).collect();
        match words.is_empty() {
            true => Err(format!(
                "the reply holds none: {:?}",
                reply.chars().take(300).collect::<String>()
            )),
            false => Ok(words.join(" ")),
        }
    });
    name.map_err(|line| log(format!("no session name: {line}")))
        .ok()
}

impl ClaudeWriter {
    /// The `claude -p` run for a session's name, at `setup`'s model and effort with the studio's
    /// own system prompt, on Daniel's first `prompt` and the final `response`, each cut at
    /// `MAX_CHANGE`.
    pub fn name_command(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
    ) -> (Command, String) {
        let cut = |text: &str| text[..text.floor_char_boundary(MAX_CHANGE)].to_string();
        let response = response.map_or(String::new(), |r| {
            format!(
                "\n\nThe agent's final response to it:\n<response>\n{}\n</response>",
                cut(r)
            )
        });
        self.run_with(
            &options(setup, NAME),
            &format!(
                "The developer's first prompt:\n<prompt>\n{}\n</prompt>{response}",
                cut(prompt)
            ),
        )
    }
}

/// The session name writer's system prompt, which replaces Claude Code's own.
const NAME: &str = concat!(
    r#"You name one session of a coding agent in a few words, so that the developer who runs it can tell it from his other sessions in a list.

Answer from the input: his first prompt and, when the prompt is only a slash command, the agent's final response to it. Use only this input: do not use tools or read files. Name what the session is about, not the command.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"name": string}
- name: at most 6 words, with no issue number and no quotes.
"#
);
