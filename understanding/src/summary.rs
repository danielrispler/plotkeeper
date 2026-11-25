//! The turn summary, a Headline and a Body (danielrispler/Falinks#526; danielrispler/Falinks#517, decisions 12 to 15): one
//! writer call when a long turn ends, on a small model with the studio's own system prompt, read with the session's last 3
//! summaries. It writes no Quiz since danielrispler/plotkeeper#347.

use crate::grounding::{HEADLINE_WORDS, agent_says, facts_text, grounded, material, words};
use crate::{CardStore, CardWriter, ClaudeWriter, MAX_CHANGE, Quiz, WriterRequest};
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

/// A turn's summary: its Headline and Body.
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
    /// The Quiz a summary stored before danielrispler/plotkeeper#347 holds; the writer no longer writes one, so a new summary has
    /// `None`. A learning section stored before danielrispler/plotkeeper#118 reads as its question (`stored_quiz`).
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
    /// The program that writes, `summaries.writer` in the settings: `claude` unless the file says `agy`.
    #[serde(default)]
    pub writer: crate::WriterKind,
}

impl Default for SummarySetup {
    /// Haiku at its default effort (danielrispler/Falinks#517, decision 14).
    fn default() -> Self {
        Self {
            model: "haiku".into(),
            effort: String::new(),
            writer: crate::WriterKind::Claude,
        }
    }
}

/// How many of the session's latest summaries the writer reads (danielrispler/Falinks#517, decision 15).
const EARLIER: usize = 3;

impl TurnSummary {
    /// The summary in a writer's reply, which must hold one JSON object with a headline. Its Body
    /// lines of an unknown kind, or with a number or id that `material` lacks, are dropped. A
    /// Headline with such a number is stripped, and one over `HEADLINE_WORDS` stands, each with why
    /// so the reply is asked for once more; a claim of a fix, a pass or a verification gets "Agent
    /// says:" unless a check backs it. A Quiz in the reply is ignored.
    fn from_reply(
        reply: &str,
        request: &SummaryRequest,
        material: &str,
    ) -> Result<(TurnSummary, Option<String>), String> {
        let head: String = reply.chars().take(300).collect();
        let value: serde_json::Value = crate::parse_object(reply)
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
            headline_problem = Some(format!(
                "its headline holds a number or id the turn lacks: {headline:?}"
            ));
            String::new()
        } else {
            if words(headline) > HEADLINE_WORDS {
                headline_problem = Some(format!("its headline has {} words", words(headline)));
            }
            agent_says(headline, &request.facts)
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
            quiz: None,
        };
        Ok((summary, headline_problem))
    }
}

/// The summary of `request`'s turn from `writer`, read with the session's last 3 summaries in
/// `store`, which keeps it. The writer is asked once more when its reply gives none, or gives a
/// Headline with an invented number or over 16 words; the second reply stands, or the first when
/// the second gives no summary. Each problem is one line for `log`. The store is not locked while
/// the writer runs.
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
    let retry = |first: &Result<(TurnSummary, Option<String>), String>| match first {
        Err(_) => true,
        Ok((_, problem)) => problem.is_some(),
    };
    match crate::once_more(first, retry, summary, Result::or) {
        Ok((summary, problem)) => {
            if let Some(problem) = problem {
                log(format!("headline of {}: {problem}", request.id));
            }
            let id = request.id.clone();
            let added = store.lock().unwrap().add_summary(request, summary.clone());
            if let Err(e) = added {
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
    /// `summary_request` as the process path runs it with the program that writes now.
    pub fn summary_command(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> (Command, String) {
        self.build(&self.summary_request(setup, request, earlier))
    }

    /// The `claude -p` run for `request`'s turn summary, at `setup`'s model and effort with the
    /// studio's own system prompt, read with `earlier`, the session's latest summaries, oldest
    /// first. Past `MAX_CHANGE`, the responses keep their end, where the final reply is.
    pub fn summary_request(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> WriterRequest {
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
                .map(|s| format!("<summary>\n{}\n</summary>\n", s.summary))
                .collect(),
        };
        self.request_with(
            Some(setup),
            Some(SUMMARY),
            &format!(
                "The developer's prompt:\n<prompt>\n{}\n</prompt>\n\n\
                 The agent's responses in the turn, oldest first:\n{responses}\n\
                 The turn's intentions, oldest first:\n{intentions}\n\
                 What the hooks and git recorded of the turn:\n{facts}\n\
                 The session's latest turn summaries, oldest first:\n\
                 {earlier}",
                request.prompt,
                facts = facts_text(&request.facts)
            ),
        )
    }
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
            let value = crate::parse_object(&reply).unwrap_or_default();
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
            let added = store.lock().unwrap().add_agent_summary(id, &summary);
            if let Err(e) = added {
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
    /// `agent_summary_request` as the process path runs it with the program that writes now.
    pub fn agent_summary_command(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        message: &str,
    ) -> (Command, String) {
        self.build(&self.agent_summary_request(setup, prompt, message))
    }

    /// The `claude -p` run for a subagent's summary, at `setup`'s model and effort with the studio's
    /// own system prompt, on its spawn's `prompt` and its last `message`, each cut at `MAX_CHANGE`.
    pub fn agent_summary_request(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        message: &str,
    ) -> WriterRequest {
        let cut = |text: &str| text[..text.floor_char_boundary(MAX_CHANGE)].to_string();
        self.request_with(
            Some(setup),
            Some(AGENT_SUMMARY),
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
    r#"You write the turn summary of one turn of a coding agent's session, for the developer who runs it and wants to check what happened instead of reading its raw output: a headline and up to three lines.

Answer from the input: his prompt, the agent's responses in the turn, its intentions, what the hooks and git recorded of the turn (its files with their line counts, its checks and its hunks), and the session's latest turn summaries. Connect to those earlier turns where it helps. Use only this input: do not use tools or read files.

The studio shows the recorded files and checks itself, next to your headline. Do not restate them: no file lists, no line counts, no check results. Write what they cannot say. Never write a number, an issue or PR id or a commit SHA that the input does not hold; say "one" or "a few" in words instead. Say "fixed", "passes" or "verified" only when a check in the recorded facts ended passed; otherwise attribute the claim to the agent.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"headline": string, "lines": [{"kind": "why" | "reported" | "open" | "ask", "text": string}]}
- headline: at most 16 words, one sentence that states an outcome, or what the turn waits on. Never narrate ("Looked at", "Started by"). When the turn ends by asking the developer something, the headline is that ask.
- lines: at most 3, each at most 20 words, each typed. why: the reason for a choice the turn made. reported: something the agent claims that the recorded facts do not show. open: what was left undone or was not checked, such as "Nobody looked at the rows in a real window." ask: a question or decision the agent puts to the developer. Leave out a line that says nothing more than the headline.

A number taken from the input keeps its bound or condition, such as "upper bound", "on fixtures" or "median", wherever it appears.
"#
);

/// How many characters a stored session name may have before its "…": a bound on a writer that ignores its rules
/// (#443), not a layout device. The page fits the name to its width with a CSS ellipsis.
const NAME_CHARS: usize = 60;

/// The name of a session from `writer`, on Daniel's first `prompt` and, for a bare slash command,
/// the turn's final `response`: the reply's `name`, whole, or cut at the last space before
/// `NAME_CHARS` with "…" when it runs longer. One line goes to `log` when the call gives none.
pub fn write_name(
    writer: &(impl CardWriter + ?Sized),
    setup: &SummarySetup,
    prompt: &str,
    response: Option<&str>,
    folder: &str,
    mut log: impl FnMut(String),
) -> Option<String> {
    let name = writer
        .name(setup, prompt, response, folder)
        .and_then(|reply| {
            let value = crate::parse_object(&reply).unwrap_or_default();
            let name = value["name"].as_str().unwrap_or_default();
            let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
            if name.is_empty() {
                Err(format!(
                    "the reply holds none: {:?}",
                    reply.chars().take(300).collect::<String>()
                ))
            } else {
                Ok(bounded_name(name))
            }
        });
    name.map_err(|line| log(format!("no session name: {line}")))
        .ok()
}

/// `name` when it has at most `NAME_CHARS` characters; otherwise its first `NAME_CHARS`, cut back to the last space
/// when there is one and the next character is not a space, and "…".
fn bounded_name(name: String) -> String {
    let Some((end, _)) = name.char_indices().nth(NAME_CHARS) else {
        return name;
    };
    let head = &name[..end];
    // A space right after the head means the head ends on a whole word.
    let head = if name[end..].starts_with(' ') {
        head
    } else {
        head.rfind(' ').map_or(head, |space| &head[..space])
    };
    format!("{}…", head.trim_end())
}

impl ClaudeWriter {
    /// The `claude -p` run for a session's name, at `setup`'s model and effort with the studio's
    /// own system prompt, on Daniel's first `prompt` and the final `response`, each cut at
    /// `MAX_CHANGE`, with the session's `folder`, which "here" in the prompt means (#408).
    pub fn name_request(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
        folder: &str,
    ) -> WriterRequest {
        let cut = |text: &str| text[..text.floor_char_boundary(MAX_CHANGE)].to_string();
        let response = response.map_or(String::new(), |r| {
            format!(
                "\n\nThe agent's final response to it:\n<response>\n{}\n</response>",
                cut(r)
            )
        });
        // `claude -p` names its own working directory to the model even with `--system-prompt`, so "here" needs the
        // session's folder beside it (#408).
        let folder = if folder.is_empty() {
            String::new()
        } else {
            format!("The session's folder: {folder}\n\n")
        };
        self.request_with(
            Some(setup),
            Some(NAME),
            &format!(
                "{folder}The developer's first prompt:\n<prompt>\n{}\n</prompt>{response}",
                cut(prompt)
            ),
        )
    }
}

/// The session name writer's system prompt, which replaces Claude Code's own.
const NAME: &str = concat!(
    r#"You give one session of a coding agent a short title, so that the developer who runs it can tell it from his other sessions in a list.

Answer from the input: his first prompt and, when the prompt is only a slash command, the agent's final response to it. Use only this input: do not use tools or read files. Name what the session is about, not the command.

You run in a folder of your own, not the session's: never name the folder you run in. "Here" or "this folder" in the prompt means the session's folder, given in the input when known.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"name": string}
- name: a title of at most 5 words and 36 characters, with no issue number and no quotes. Say what the session is about in your own words: not the start of the prompt, and never its first words cut short.
"#
);
