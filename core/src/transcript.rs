//! The transcript reader (#461, decision 12): a tolerant reader of a session's transcript at the
//! hook's `transcript_path`, for what the session is doing and why. It reads the lines of its own
//! agent and skips line types it does not know. A line that does not parse, or a user or assistant
//! line without `message.content`, makes the transcript unavailable: the session view falls back to
//! step text only. The Claude Code version notice (#373) is the early warning of a format change.
//!
//! It is the one place the core reads a file, and it opens only a `.jsonl` path, so a payload that
//! names a credential file is never read (`apps/studio/AGENTS.md`, "Hard rules").

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
    /// The intention: the last line of the agent's latest text in the turn.
    pub intention: Option<String>,
    /// The steps (tool calls) the agent made since that text.
    pub steps_ago: usize,
    /// The agent's full texts in the turn, oldest first.
    pub responses: Vec<String>,
    /// Why the transcript cannot be read, `None` while it can.
    pub unavailable: Option<String>,
    /// How full the context is, from the agent's latest message (#461, decision 19); kept across
    /// turns.
    pub context: Option<Context>,
    /// The git branch of the session's folder, from its latest line; kept across turns.
    pub branch: Option<String>,
}

/// The input tokens of the agent's latest message, out of the context window Claude Code's status
/// line snapshot gives (#517, decisions 1 and 2); `None` with no snapshot, which has no share.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Context {
    pub tokens: u64,
    pub window: Option<u64>,
    /// The percent of the window used, rounded up as Claude Code's status line shows it (#517,
    /// measurement 1: 84.8k of 1M is its 9%).
    pub share: Option<u64>,
}

impl Context {
    pub(crate) fn new(tokens: u64, window: Option<u64>) -> Self {
        let share = window
            .filter(|&w| w > 0)
            .map(|w| (100 * tokens).div_ceil(w));
        Context {
            tokens,
            window,
            share,
        }
    }
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
        let usage = &line["message"]["usage"];
        if kind == "assistant" && usage.is_object() {
            let tokens = [
                "input_tokens",
                "cache_creation_input_tokens",
                "cache_read_input_tokens",
            ]
            .iter()
            .map(|k| usage[k].as_u64().unwrap_or(0))
            .sum();
            // The window stays the snapshot's, which the shell sets (`Studio::set_window`).
            let window = self.context.as_ref().and_then(|c| c.window);
            self.context = Some(Context::new(tokens, window));
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
                    let Some(last) = text.lines().map(str::trim).rfind(|l| !l.is_empty()) else {
                        continue;
                    };
                    self.intention = Some(last.into());
                    self.steps_ago = 0;
                    self.responses.push(text.into());
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn new_turn(&mut self) {
        *self = Transcript {
            context: self.context.take(),
            branch: self.branch.take(),
            ..Transcript::default()
        };
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
