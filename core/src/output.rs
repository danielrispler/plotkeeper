//! What the studio asks the shell to do, and what the shell tells it back.

use crate::{Check, TurnFile, event::ToolKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// What the shell must act on after a live event.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Output {
    /// A session's or a worker's worktree changed since its last card, or a session's workers'
    /// did; ask the card writer.
    Card(CardRequest),
    /// A session woken by its workers changed nothing itself since its last card: its brief only
    /// takes in its workers' cards (danielrispler/Falinks#369, decision 11).
    Brief(CardRequest),
    /// A session newly needs Daniel: its own agent asks, or `agent`, a subagent at any depth, whose
    /// notice goes on its session's line (danielrispler/Falinks#369, decision 22).
    NeedsYou {
        session_id: String,
        agent: Option<String>,
    },
    /// Auto mode blocked a Bash command live, `agent` (a subagent, `None` for the session's own agent)
    /// asking: the macOS notification "Auto mode blocked a command". A replay raises none.
    CommandBlocked {
        session_id: String,
        agent: Option<String>,
    },
    /// A session newly reached your turn: its turn ended, with nothing blocked on Daniel (danielrispler/Falinks#517,
    /// decision 6).
    YourTurn { session_id: String },
    /// Worker `agent` of a session, at any depth, newly failed. A failed helper raises none.
    Failed { session_id: String, agent: String },
    /// Helper `agent` became a worker, which the card store keeps, since a replay takes no
    /// fingerprint to see its command's change (danielrispler/Falinks#434, decision 37).
    Promoted { agent: String },
    /// A prompt named a known task id in markup the studio does not know: drift in Claude Code's
    /// wake-ups, for stderr (danielrispler/Falinks#369, decision 7).
    Unrecognised { session_id: String, task_id: String },
    /// A session's turn ended with a long reply or by asking Daniel something: its turn summary
    /// (danielrispler/Falinks#517, decisions 12 and 13).
    Summary(SummaryRequest),
    /// Daniel's first prompt of a session arrived, or its turn ended when that prompt was a bare
    /// slash command: its prompt name.
    Name(NameRequest),
    /// A Claude session's hook state and its OSC 7501 state disagreed past `MISMATCH_AFTER_MS`:
    /// `line` goes to the local mismatch log, for debugging (danielrispler/Falinks#560).
    Mismatch { line: String },
    /// A Claude session's turn has been running by its hooks while its terminal's OSC 7501 state
    /// said `idle` past `MISMATCH_AFTER_MS`: Daniel interrupted it, which fires no `Stop`. The
    /// shell appends a `StudioInterrupted` line for turn `prompt_id`, with its end trees, and that
    /// line ends the turn.
    Interrupted {
        session_id: String,
        prompt_id: String,
    },
    /// A session's first `Stop`: read Claude Code's version from its transcript, and nothing else
    /// (danielrispler/Falinks#369, decision 9).
    Version { transcript_path: String },
    /// An agy session's first `Stop`: the shell runs `agy --version` (agy's payloads and transcript
    /// carry no version) and compares it with the fixture's (#43, decision 14).
    AgyVersion,
}

/// What the live app learned that the event log does not hold, which the card store keeps as it
/// happens and the core applies after the replay (danielrispler/Falinks#434, decision 37).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveFacts {
    /// The helpers promoted to workers (`Output::Promoted`).
    #[serde(default)]
    pub promoted: BTreeSet<String>,
    /// The failed lines Daniel opened (`Studio::open`).
    #[serde(default)]
    pub opened: BTreeSet<String>,
    /// How many turns of each session Daniel has seen, by its row's id (danielrispler/Falinks#556).
    #[serde(default)]
    pub seen: BTreeMap<String, usize>,
}

/// A session's change since its last card (danielrispler/Falinks#369, decision 10), with what the card writer needs: the
/// diff, not the payloads. The card store keeps it with its card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardRequest {
    pub session_id: String,
    /// Names the card: its worker, else its session, `@`, and when the hook received the event it
    /// was written at.
    pub id: String,
    pub base: Base,
    /// The diff of each file that changed since the base, commits included. A request with none of
    /// its own writes no card (danielrispler/plotkeeper#260); older stored cards hold their workers'
    /// text here instead.
    #[serde(default)]
    pub change: Option<String>,
    /// When the hook received the prompt of Daniel's that the card belongs to: its session's latest
    /// prompt that is not a wake-up; for a worker's card, when the worker started. `None` in cards
    /// stored before danielrispler/Falinks#377.
    #[serde(default)]
    pub prompt: Option<u64>,
    /// The worker whose card it is, `None` for its session's own.
    #[serde(default)]
    pub agent: Option<String>,
    /// The files other subagents edited or wrote in its worktree, by subagent (danielrispler/Falinks#369, story 20).
    #[serde(default)]
    pub changed_by: BTreeMap<String, BTreeSet<String>>,
    /// A session's workers' cards since its last card, at any depth. No longer read to write a card;
    /// kept because older stored cards carry it.
    #[serde(default)]
    pub workers: Vec<String>,
    /// The command of the check whose start requested it, which the card is on (danielrispler/Falinks#461, decision 15);
    /// `None` for any other card.
    #[serde(default)]
    pub check: Option<String>,
    /// Whether the turn ended in Needs you, as the core recorded at its end: a Quiz is never beside a decision
    /// (danielrispler/plotkeeper#257). A request stored before this reads as "did not".
    #[serde(default)]
    pub asks: bool,
}

/// A turn that gets a summary (danielrispler/Falinks#517, decisions 12, 13 and 15): what the summary writer reads of it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SummaryRequest {
    pub session_id: String,
    /// Names the turn: its session, `@`, and when its `Stop` arrived, as `Stopped::card`.
    pub id: String,
    /// Daniel's latest prompt.
    pub prompt: String,
    /// The agent's texts in the turn, oldest first, the final reply last.
    pub responses: Vec<String>,
    /// The turn's intentions, one per response.
    pub intentions: Vec<String>,
    /// The card its `Stop` requested.
    #[serde(default)]
    pub card: Option<String>,
    /// What the hooks and git recorded of the turn (danielrispler/plotkeeper#115); empty until the
    /// shell fills it in, and for an agent that reports no trees.
    #[serde(default)]
    pub facts: Facts,
    /// Whether the turn's reply is long or asks Daniel something (`wants_summary`), so that its
    /// summary is wanted for itself; a summary of a turn that only changed code is not written.
    #[serde(default = "wanted")]
    pub wanted: bool,
}

fn wanted() -> bool {
    true
}

/// A file a turn changed, with its whole hunk from git.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FactFile {
    #[serde(flatten)]
    pub file: TurnFile,
    /// Its diff between the turn's trees. Not stored: the guards read it while the writer runs.
    #[serde(skip)]
    pub hunk: String,
}

/// The turn's recorded facts, which its writer is given and its guards check the writer against
/// (danielrispler/plotkeeper#115).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Facts {
    pub files: Vec<FactFile>,
    /// Each check's last run, retries and commands.
    pub checks: Vec<Check>,
    /// Unused: kept, always 0, so that the previous build, whose `Facts` requires it, can still read
    /// `summaries.json`.
    #[serde(default)]
    pub external: u64,
}

/// What the writer reads to name a session from its first prompt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NameRequest {
    pub session_id: String,
    /// Daniel's first prompt.
    pub prompt: String,
    /// The final response of its turn, for a prompt that is a bare slash command; `None` otherwise.
    pub response: Option<String>,
    /// The session's folder, which "here" in the prompt means: the writer runs in a folder of its own, which its
    /// harness names to it (#408).
    #[serde(default)]
    pub cwd: String,
}

/// Whether `prompt` is a slash command with nothing after it, such as `/wayfinder`.
pub(crate) fn is_bare_command(prompt: &str) -> bool {
    let prompt = prompt.trim();
    prompt.starts_with('/') && !prompt.contains(char::is_whitespace)
}

/// A reply of at least this many words gets a summary (danielrispler/Falinks#517, decision 12).
const SUMMARY_WORDS: usize = 150;

/// Whether a turn whose final reply is `reply` gets a summary: the reply is long, or the turn ends
/// asking Daniel something, in a question or a block that outlasts it (`asks`).
pub fn wants_summary(reply: &str, asks: bool) -> bool {
    let last = reply.lines().map(str::trim).rfind(|l| !l.is_empty());
    asks || reply.split_whitespace().count() >= SUMMARY_WORDS
        || last.is_some_and(|l| l.ends_with('?'))
}

/// How far back a start writes PR briefs for ready PRs whose head moved, `catchUp.hours` in the settings
/// (danielrispler/plotkeeper#334).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatchUp {
    pub window_ms: u64,
}

impl Default for CatchUp {
    fn default() -> Self {
        CatchUp {
            window_ms: 2 * 60 * 60 * 1000,
        }
    }
}

impl CardRequest {
    /// The change the writer reads: the diff, or the workers' text; empty when there is none.
    pub fn diff(&self) -> &str {
        self.change.as_deref().unwrap_or_default()
    }

    /// Whose card it is, and whose brief it updates: its worker's id, else its session's.
    pub fn owner(&self) -> &str {
        self.agent.as_deref().unwrap_or(&self.session_id)
    }
}

/// A worktree the shell fingerprints before it applies a live line, and the card stream it is for:
/// `key` is a session's id, or a subagent's.
#[derive(Clone, Debug, PartialEq)]
pub struct Fingerprint {
    pub key: String,
    pub folder: String,
    pub purpose: Purpose,
    /// When the turn or worker `key` names began, in Unix milliseconds: a worktree made after that is the
    /// agent's own creation, one made before it is not (danielrispler/plotkeeper#204). `None` when it does not say.
    pub began: Option<u64>,
}

/// The worktree a card's change starts from (danielrispler/Falinks#369, decision 10): the session's previous card, or its
/// prompt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Base {
    /// `HEAD`'s sha, or `None` before the first commit.
    pub head: Option<String>,
    /// The files `git status` listed, whose content the fingerprint kept in memory.
    pub files: Vec<String>,
}

/// A worktree's change since its base, as the shell's fingerprint finds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub base: Base,
    /// The diff of each file that changed, commits included.
    pub diff: String,
    /// `HEAD`'s sha now, so that a commit since the base shows as `head` other than `base.head`.
    pub head: Option<String>,
}

/// What a fingerprint is for (danielrispler/Falinks#410, decision 23). Its record becomes the next change's base for
/// every purpose but `Commit`, which keeps it only when `HEAD` moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// A base, whose change is never a card: a turn's first touch of a worktree, a subagent's first
    /// folder, a helper's look before its command, and the start-up bases.
    Base,
    /// After a worker's or a helper's command: a card only when `HEAD` moved; the change it saw
    /// otherwise stays for the card at its next commit, its end or its fallback.
    Commit,
    /// A card when the worktree changed, such as a worker's end, or a session's change between its
    /// turns (danielrispler/Falinks#434, decisions 16 and 17).
    Card,
    /// 20 minutes or more after the worktree's last card or its last quiet fallback: a card when
    /// it changed, else a new 20-minute wait (danielrispler/Falinks#369, decision 11, as danielrispler/Falinks#410 changes it).
    Fallback,
    /// A turn's end, at its `Stop` or without one (danielrispler/Falinks#434, decision 5): its change, or else its
    /// workers' cards, makes its card or brief.
    TurnEnd,
}

/// One tool call: its `tool_use_id`, and its tool and input, which are all `PermissionRequest`
/// names it by. Also the subagent that made it, `None` for the session's own agent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Call {
    pub(crate) id: Option<String>,
    pub(crate) tool: String,
    pub(crate) kind: ToolKind,
    pub(crate) input: Value,
    pub(crate) agent: Option<String>,
}

/// Who a session that needs Daniel waits for: its oldest block's asker, a subagent or `None` for the
/// session's own agent, and since when, in Unix milliseconds (danielrispler/Falinks#369, decision 21).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Asker {
    pub agent: Option<String>,
    pub since: u64,
}

/// The event the shell appends to the log when Daniel renames a session (danielrispler/Falinks#469). The log is what a
/// restart replays, so the name survives it.
pub const RENAME: &str = "StudioRename";

/// The line the shell appends when the writer named a session from its first prompt: its
/// `studio_name`. The log is what a restart replays, so the name survives it.
pub const PROMPT_NAME: &str = "StudioPromptName";

/// The line the shell appends when Daniel interrupted a session's turn (`Output::Interrupted`): its
/// `prompt_id` and the turn's end trees. An interrupt fires no `Stop` and no `idle_prompt`, so this
/// line ends the turn, at once and after a restart; it ends no later turn.
pub const INTERRUPTED: &str = "StudioInterrupted";

/// The event the shell appends to the log when Daniel deletes a session: its row leaves the list,
/// at once and after a restart, until a `SessionStart` of it brings it back. Its cards stay.
pub const DELETE: &str = "StudioDelete";

/// The line the answer hook appends when Daniel answers from the plotkeeper card (danielrispler/Falinks#478): the call's hook
/// payload under this name, with `studio_asked_by` (the event that asked) and `studio_answer`. An
/// answer in the terminal leaves none.
pub const STUDIO_ANSWER: &str = "StudioAnswer";

/// The tool input that says what a step works on: a command's `description`, which the agent sets
/// on every Bash call (danielrispler/Falinks#461, decision 12), else its first line; a path relative to the session's
/// folder, the first question asked, or a search pattern, URL, query or task description.
pub(crate) fn step_detail(input: &Value, cwd: &str) -> Option<String> {
    if let Some(command) = input["command"].as_str() {
        if let Some(description) = input["description"]
            .as_str()
            .filter(|d| !d.trim().is_empty())
        {
            return Some(description.into());
        }
        return command.lines().next().map(Into::into);
    }
    if let Some(question) = input["questions"][0]["question"].as_str() {
        return Some(question.into());
    }
    if let Some(path) = input["file_path"].as_str() {
        return Some(relative(path, cwd).into());
    }
    ["pattern", "url", "query", "description"]
        .iter()
        .find_map(|key| input[key].as_str())
        .map(Into::into)
}

/// `path` relative to `folder`, or else `path` as it is.
fn relative<'a>(path: &'a str, folder: &str) -> &'a str {
    let relative = path.strip_prefix(folder).and_then(|p| p.strip_prefix('/'));
    relative.unwrap_or(path)
}
