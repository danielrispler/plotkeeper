//! The transcript reader (danielrispler/Falinks#461, decision 12): a tolerant reader of a session's transcript at the
//! hook's `transcript_path`, for what the session is doing and why. It reads the lines of its own
//! agent and skips line types it does not know. A line that does not parse, or a user or assistant
//! line without `message.content`, makes the transcript unavailable: the session view falls back to
//! step text only. The Claude Code version notice (danielrispler/Falinks#373) is the early warning of a format change.
//!
//! It is the one place the core reads a file, and it opens only a `.jsonl` path, so a payload that
//! names a credential file is never read (`AGENTS.md`, "Hard rules").

use serde::Serialize;
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
};

/// What the transcript says of the session's latest turn, which the session view shows.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Transcript {
    /// The intention: the last line of the agent's latest text while the turn runs, its first line
    /// once the turn ended (danielrispler/Falinks#517, decision 16), so a plugin's footer is never the intention.
    pub intention: Option<String>,
    /// The steps (tool calls) the agent made since that text.
    pub steps_ago: usize,
    /// The agent's full texts in the turn, oldest first.
    pub responses: Vec<String>,
    /// Why the transcript cannot be read, `None` while it can.
    pub unavailable: Option<String>,
    /// How full the context is, from the agent's latest message (danielrispler/Falinks#461, decision 19); kept across
    /// turns.
    pub context: Option<Context>,
    /// The git branch of the session's folder, from its latest line; kept across turns.
    pub branch: Option<String>,
    /// The model the session runs at, from the agent's latest message (danielrispler/Falinks#517, decision 8); kept
    /// across turns.
    pub model: Option<String>,
    /// The session's effort, from its latest line that carries one (danielrispler/Falinks#517, decision 8); kept across
    /// turns.
    pub effort: Option<String>,
    /// The first line of the turn's summary, once it arrived (danielrispler/Falinks#526): the turn-end intention then.
    pub summary: Option<String>,
    /// Whether the session's turn has ended, which the core sets from its hooks; kept across turns.
    #[serde(skip)]
    pub ended: bool,
}

/// How full the context is, as Claude Code's status line measures it: from the session's status line
/// snapshot when there is one, else the input tokens of the agent's latest message with no share
/// (danielrispler/Falinks#517, decisions 1 and 2; danielrispler/Falinks#519).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Context {
    pub tokens: u64,
    pub window: Option<u64>,
    /// The percent of the window used, Claude's own `used_percentage`, else rounded to nearest.
    pub share: Option<u64>,
    /// The percent left, Claude's own `remaining_percentage`, else `100 - share`.
    pub left: Option<u64>,
    /// The transcript's tokens, which count when the snapshot has no `current_usage` yet.
    #[serde(skip)]
    pub(crate) measured: u64,
    /// The snapshot's `context_window`, which the shell sets (`Studio::set_snapshot`).
    #[serde(skip)]
    pub(crate) snapshot: Option<Value>,
}

impl Context {
    pub(crate) fn new(measured: u64, snapshot: Option<Value>) -> Self {
        let field = |key: &str| snapshot.as_ref().and_then(|s| s[key].as_u64());
        let window = field("context_window_size").filter(|&w| w > 0);
        // `current_usage` is null before the session's first API call.
        let usage = snapshot.as_ref().map(|s| &s["current_usage"]);
        let tokens = match usage {
            Some(usage) if window.is_some() && usage.is_object() => input_tokens(usage),
            _ => measured,
        };
        let share =
            window.map(|w| field("used_percentage").unwrap_or_else(|| (100 * tokens + w / 2) / w));
        let left = share.map(|s| field("remaining_percentage").unwrap_or(100 - s.min(100)));
        Context {
            tokens,
            window,
            share,
            left,
            measured,
            snapshot,
        }
    }
}

/// The input tokens of an API call's `usage`: fresh, cache-written and cache-read.
fn input_tokens(usage: &Value) -> u64 {
    [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .map(|k| usage[k].as_u64().unwrap_or(0))
    .sum()
}

impl Transcript {
    /// Takes in transcript line `line`, or says why it cannot.
    pub fn read(&mut self, line: &str) -> Result<(), String> {
        let line: Value =
            serde_json::from_str(line).map_err(|e| format!("a line does not parse: {e}"))?;
        let kind = line["type"].as_str().unwrap_or_default();
        // A helper's lines are its own; meta lines (a skill's text, a command's caveat) are no prompt.
        if !matches!(kind, "user" | "assistant")
            || line["isSidechain"] == true
            || line["isMeta"] == true
        {
            return Ok(());
        }
        if let Some(branch) = line["gitBranch"].as_str().filter(|b| !b.is_empty()) {
            self.branch = Some(branch.into());
        }
        if let Some(effort) = line["effort"].as_str() {
            self.effort = Some(effort.into());
        }
        // An API error's line is `<synthetic>`, no model the session runs at.
        if let Some(model) = line["message"]["model"]
            .as_str()
            .filter(|m| *m != "<synthetic>")
        {
            self.model = Some(model.into());
        }
        let usage = &line["message"]["usage"];
        if kind == "assistant" && usage.is_object() {
            // The snapshot stays, which the shell sets (`Studio::set_snapshot`).
            let snapshot = self.context.as_mut().and_then(|c| c.snapshot.take());
            self.context = Some(Context::new(input_tokens(usage), snapshot));
        }
        let content = &line["message"]["content"];
        let blocks = match content {
            Value::String(_) if kind == "user" => {
                self.new_turn();
                return Ok(());
            }
            Value::Array(blocks) => blocks,
            _ => return Err(format!("a {kind} line has no message.content")),
        };
        for block in blocks {
            match (kind, block["type"].as_str()) {
                ("user", Some("text")) if line["isCompactSummary"] != true => self.new_turn(),
                ("assistant", Some("tool_use")) => self.steps_ago += 1,
                ("assistant", Some("text")) => {
                    let text = block["text"].as_str().unwrap_or_default().trim();
                    if text.is_empty() {
                        continue;
                    }
                    self.intention = line_of(text, self.ended).map(Into::into);
                    self.steps_ago = 0;
                    self.responses.push(text.into());
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// The turn's intentions, one per response, oldest first: each its last line, but the final
    /// response's first line once the turn ended, or its summary's when it has one.
    pub fn intentions(&self) -> Vec<String> {
        let last = self.responses.len().saturating_sub(1);
        let lines = self.responses.iter().enumerate();
        lines
            .filter_map(|(i, text)| match &self.summary {
                Some(summary) if self.ended && i == last => Some(summary.as_str()),
                _ => line_of(text, self.ended && i == last),
            })
            .map(Into::into)
            .collect()
    }

    /// Notes whether the session's turn has `ended`, which picks the latest response's line, or
    /// the summary's once the turn ended with one (danielrispler/Falinks#517, decision 16).
    pub(crate) fn end(&mut self, ended: bool) {
        self.ended = ended;
        if let Some(text) = self.responses.last() {
            self.intention = match &self.summary {
                Some(summary) if ended => Some(summary.clone()),
                _ => line_of(text, ended).map(Into::into),
            };
        }
    }

    fn new_turn(&mut self) {
        *self = Transcript {
            context: self.context.take(),
            branch: self.branch.take(),
            model: self.model.take(),
            effort: self.effort.take(),
            ended: self.ended,
            ..Transcript::default()
        };
    }
}

/// The first or else the last non-empty line of `text`, trimmed.
fn line_of(text: &str, first: bool) -> Option<&str> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    if first {
        lines.next()
    } else {
        lines.next_back()
    }
}

/// Where a session's transcript is, and how far it was read.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TranscriptFile {
    pub(crate) path: String,
    read_to: u64,
}

impl TranscriptFile {
    /// Reads the complete lines added to the transcript since the last read into `transcript`. Returns
    /// `transcript.unavailable`'s new reason when it changed to one, for one stderr line.
    pub(crate) fn follow(&mut self, transcript: &mut Transcript) -> Option<String> {
        let before = transcript.unavailable.clone();
        // A corrupted transcript stays so; a gone file may come back.
        if before.as_deref().is_some_and(|why| why != GONE) {
            return None;
        }
        match self.read(transcript) {
            Ok(()) => transcript.unavailable = None,
            Err(why) => {
                *transcript = Transcript::default();
                transcript.unavailable = Some(why);
                // From the start again, once a gone file comes back.
                self.read_to = 0;
            }
        }
        (transcript.unavailable != before)
            .then(|| transcript.unavailable.clone())
            .flatten()
    }

    fn read(&mut self, transcript: &mut Transcript) -> Result<(), String> {
        let path = Path::new(&self.path);
        if path.extension().is_none_or(|e| e != "jsonl") {
            return Err("the transcript path is not a .jsonl file".into());
        }
        let mut file = File::open(path).map_err(|_| GONE.to_string())?;
        file.seek(SeekFrom::Start(self.read_to))
            .map_err(|e| e.to_string())?;
        let mut reader = BufReader::new(file);
        let mut line = Vec::new();
        // A line counts once it ends in a newline, so one Claude Code is still writing waits.
        while reader
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?
            > 0
            && line.ends_with(b"\n")
        {
            self.read_to += line.len() as u64;
            transcript.read(&String::from_utf8_lossy(&line))?;
            line.clear();
        }
        Ok(())
    }
}

const GONE: &str = "the transcript file is gone";
