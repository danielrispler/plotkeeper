//! The agy adapter: turns the hook lines of Antigravity CLI (`agy`) and its transcript into the
//! core's neutral `Event`s (danielrispler/plotkeeper#43, decisions 2, 3, 6, 7, 10 and 15). agy's
//! event names, tool names, record types and files stay here.
//!
//! Unlike Claude's, the adapter has state: a turn's origin is read from the transcript once, and
//! hooks that arrive out of order are tolerated by remembering which steps ended. A subagent's
//! payload names no parent, so each conversation is a session of its own.

use crate::event::{
    Capabilities, Event, Happened, Origin, Start, Tool, ToolKind, TranscriptFormat,
};
use crate::ledger::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
};

/// What agy reports. It has no permission or question hook, no background task ids, no last message
/// on a stop, no permission mode and no call durations; its transcript carries no version
/// (`agy --version` does).
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: "agy",
    asks: false,
    background_tasks: false,
    replies: false,
    permission_mode: false,
    durations: false,
    transcript_version: false,
    transcript_model: false,
    context: false,
    turn_trees: false,
    empty_window_starts: false,
    transcript_format: TranscriptFormat::Agy,
};

/// The record types of agy's transcript the studio knows (`transcript_full.jsonl`).
const RECORDS: [&str; 4] = [
    "USER_INPUT",
    "PLANNER_RESPONSE",
    "GENERIC",
    "SYSTEM_MESSAGE",
];

/// What the adapter did not recognise, which `weekly` counts (decisions 6 and 15).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    /// A hook event name outside agy's six, or a line with no event name or conversation.
    HookEvent,
    /// A transcript record that does not parse or has a type the studio does not know.
    Record,
    /// A tool event with no tool call or no step index.
    ToolShape,
    /// A turn's origin, decided at its start, that its `Stop` check disagreed with.
    TurnOrigin,
    /// A hook line whose `studio_agent` names an agent plotkeeper has no parser for, or a line its
    /// own agent's parser cannot read (#355).
    Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drift {
    pub at: u64,
    pub kind: DriftKind,
}

/// A turn's start: its `initialNumSteps` and where it came from.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct Turn {
    initial: u64,
    origin: Origin,
}

/// agy's six hook events.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hook {
    SessionStart,
    PreInvocation,
    PostInvocation,
    PreToolUse,
    PostToolUse,
    Stop,
}

impl Hook {
    fn named(name: &str) -> Option<Hook> {
        Some(match name {
            "SessionStart" => Hook::SessionStart,
            "PreInvocation" => Hook::PreInvocation,
            "PostInvocation" => Hook::PostInvocation,
            "PreToolUse" => Hook::PreToolUse,
            "PostToolUse" => Hook::PostToolUse,
            "Stop" => Hook::Stop,
            _ => return None,
        })
    }
}

/// One conversation, a session.
#[derive(Clone, Default, Serialize, Deserialize)]
struct Conversation {
    transcript: String,
    cwd: String,
    /// The open turn's start.
    turn: Option<Turn>,
    last_input: String,
    /// The calls (by `stepIdx`) that started and that ended.
    open: BTreeSet<String>,
    done: BTreeSet<String>,
    /// The transcript lines checked for unknown records.
    scanned: usize,
}

/// The adapter's state, one per studio.
#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Adapter {
    conversations: HashMap<String, Conversation>,
    pub drift: Vec<Drift>,
}

impl Adapter {
    /// The events of hook line `line`, or `None` when it is no JSON.
    pub(crate) fn events(&mut self, line: &str) -> Option<Vec<Event>> {
        let raw: Value = serde_json::from_str(line).ok()?;
        let at = raw["studio_received_ms"].as_u64().unwrap_or_default();
        let named = (raw["conversationId"].as_str()).zip(raw["hookEventName"].as_str());
        let Some((id, hook)) = named.and_then(|(id, name)| Some((id, Hook::named(name)?))) else {
            // A line of the hook's with neither agent's shape, or an event of agy's the studio
            // does not know: the payload changed.
            self.drift(at, DriftKind::HookEvent);
            return Some(Vec::new());
        };
        let id = id.to_owned();
        let conversation = self.conversations.entry(id.clone()).or_default();
        if let Some(text) = raw["transcriptPath"].as_str().filter(|t| !t.is_empty()) {
            conversation.transcript = text.into();
        }
        if let Some(text) = raw["cwd"].as_str().filter(|t| !t.is_empty()) {
            conversation.cwd = text.into();
        }
        let mut event = frame(&self.conversations[&id], id.clone(), at);
        event.studio_pid = raw["studio_pid"]
            .as_u64()
            .and_then(|p| u32::try_from(p).ok());
        event.studio_turn_start = strings(&raw["studio_turn_start"]);
        event.studio_turn_end = strings(&raw["studio_turn_end"]);
        if let Some(model) = raw["modelName"].as_str().filter(|m| !m.is_empty()) {
            event.happened.push(Happened::Model(model.into()));
        }
        match hook {
            Hook::SessionStart => {
                event.happened.push(Happened::SessionStart);
                event.happened.push(Happened::StartedFrom(Start::Other));
                event
                    .happened
                    .extend(branch(&event.cwd).map(Happened::Branch));
            }
            Hook::PreInvocation if raw["invocationNum"] == 0 => {
                self.turn_starts(&id, &raw, &mut event);
            }
            Hook::PreToolUse | Hook::PostToolUse => {
                self.tool_event(&id, hook, &raw, &mut event);
            }
            Hook::Stop => {
                // agy's `Stop` carries no reply, so the turn's final response is read from its
                // transcript, which agy has written by then (#358).
                if let Some(reply) = self.final_response(&id) {
                    event.happened.push(Happened::Reply(reply));
                }
                event.happened.push(Happened::TurnEnd);
                self.check_turn(&id, at);
                self.scan(&id, at);
            }
            _ => {}
        }
        Some(vec![event])
    }

    /// The main event of `line` with the adapter's state left as it is: only the conversation of
    /// the line is copied, so it costs the same however many sessions there are.
    pub(crate) fn peek(&self, line: &str) -> Option<Event> {
        let raw: Value = serde_json::from_str(line).ok()?;
        let id = raw["conversationId"].as_str()?;
        let mut scratch = Adapter::default();
        if let Some(conversation) = self.conversations.get(id) {
            scratch
                .conversations
                .insert(id.into(), conversation.clone());
        }
        scratch.events(line)?.pop()
    }

    pub(crate) fn drift(&mut self, at: u64, kind: DriftKind) {
        self.drift.push(Drift { at, kind });
    }

    /// A session's first `PreInvocation` of a turn: where the turn came from, decided once.
    fn turn_starts(&mut self, id: &str, raw: &Value, event: &mut Event) {
        let Some(conversation) = self.conversations.get_mut(id) else {
            return;
        };
        let initial = raw["initialNumSteps"].as_u64().unwrap_or_default();
        let input = raw["lastUserInput"].as_str().unwrap_or_default();
        let origin = turn_origin(&conversation.transcript, initial).unwrap_or(
            if input == conversation.last_input && !input.is_empty() {
                Origin::WakeUp
            } else {
                Origin::Daniel
            },
        );
        conversation.turn = Some(Turn { initial, origin });
        conversation.last_input = input.into();
        event.happened.push(Happened::TurnStart {
            prompt: match origin {
                Origin::Daniel => input.into(),
                Origin::WakeUp => String::new(),
            },
            origin: Some(origin),
        });
        event
            .happened
            .extend(branch(&event.cwd).map(Happened::Branch));
    }

    /// A tool's start or end. The two may arrive in either order: a start that lands after its end
    /// is dropped, and an end with no open step closes nothing.
    fn tool_event(&mut self, id: &str, hook: Hook, raw: &Value, event: &mut Event) {
        let call = &raw["toolCall"];
        let (Some(name), Some(step)) = (call["name"].as_str(), raw["stepIdx"].as_u64()) else {
            self.drift(event.studio_received_ms, DriftKind::ToolShape);
            return;
        };
        let step = step.to_string();
        let tool = tool_of(name, &call["args"]);
        let Some(conversation) = self.conversations.get_mut(id) else {
            return;
        };
        if hook == Hook::PreToolUse {
            if !conversation.done.contains(&step) {
                conversation.open.insert(step.clone());
                event.happened.push(Happened::ToolStart { tool, id: step });
            }
        } else {
            conversation.open.remove(&step);
            conversation.done.insert(step.clone());
            let failed = raw["error"].as_str().is_some_and(|e| !e.is_empty());
            event.happened.push(Happened::ToolEnd {
                tool,
                id: step,
                failed,
            });
        }
    }

    /// The open turn's last `PLANNER_RESPONSE` with text in conversation `id`'s transcript. The turn's
    /// steps start at its `initialNumSteps` and stop before the next turn's: a `USER_INPUT`, or a
    /// wake-up's `SYSTEM_MESSAGE` after a response that called no tool (a message while a tool runs
    /// stays in the turn). A replay reads the final transcript, which
    /// holds later turns too. `None` when no turn is open or it wrote no text.
    fn final_response(&self, id: &str) -> Option<String> {
        let conversation = self.conversations.get(id)?;
        let initial = conversation.turn?.initial;
        // Whether the turn's latest response called a tool, so a system message is no wake-up.
        let mut calling = true;
        let mut last = None;
        for record in records(&conversation.transcript)? {
            let Some(step) = record["step_index"].as_u64().filter(|s| *s >= initial) else {
                continue;
            };
            let later = step > initial;
            match record["type"].as_str() {
                Some("USER_INPUT") if later => break,
                Some("SYSTEM_MESSAGE") if later && !calling => break,
                Some("PLANNER_RESPONSE") => {
                    calling = record["tool_calls"]
                        .as_array()
                        .is_some_and(|calls| !calls.is_empty());
                    if let Some(text) = record["content"].as_str().map(str::trim)
                        && !text.is_empty()
                    {
                        last = Some(text.to_owned());
                    }
                }
                _ => {}
            }
        }
        last
    }

    /// At a turn's `Stop`, the origin read again: a mismatch is drift (decision 6).
    fn check_turn(&mut self, id: &str, at: u64) {
        let Some(conversation) = self.conversations.get_mut(id) else {
            return;
        };
        let Some(turn) = conversation.turn.take() else {
            return;
        };
        let now = turn_origin(&conversation.transcript, turn.initial);
        if now.is_some_and(|now| now != turn.origin) {
            self.drift(at, DriftKind::TurnOrigin);
        }
    }

    /// The transcript's records since the last check, counting those the studio does not know.
    fn scan(&mut self, id: &str, at: u64) {
        let Some(conversation) = self.conversations.get_mut(id) else {
            return;
        };
        let Some(text) = read_jsonl(&conversation.transcript) else {
            return;
        };
        let lines: Vec<&str> = text.lines().collect();
        for line in lines.iter().skip(conversation.scanned) {
            let known = serde_json::from_str::<Value>(line)
                .is_ok_and(|r| r["type"].as_str().is_some_and(|t| RECORDS.contains(&t)));
            if !known {
                self.drift.push(Drift {
                    at,
                    kind: DriftKind::Record,
                });
            }
        }
        conversation.scanned = lines.len();
    }
}

/// The string fields of the object `value`.
fn strings(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.into())))
                .collect()
        })
        .unwrap_or_default()
}

/// The transcript of subagent `child`, beside its parent's transcript `parent`:
/// `brain/<conversation>/.system_generated/logs/transcript_full.jsonl`, as every conversation's.
pub(crate) fn subagent_transcript(parent: &Path, child: &str) -> Option<PathBuf> {
    let brain = parent.ancestors().nth(4)?;
    Some(
        brain
            .join(child)
            .join(".system_generated/logs/transcript_full.jsonl"),
    )
}

/// An event of `session`, from `conversation`'s place and files, with nothing in it yet.
fn frame(conversation: &Conversation, session: String, at: u64) -> Event {
    Event {
        session_id: session,
        agent_id: None,
        agent_type: None,
        cwd: conversation.cwd.clone(),
        transcript_path: conversation.transcript.clone(),
        prompt_id: None,
        studio_received_ms: at,
        studio_pid: None,
        studio_turn_start: BTreeMap::new(),
        studio_turn_end: BTreeMap::new(),
        studio_touched_roots: None,
        capabilities: CAPABILITIES,
        line: Bytes::default(),
        happened: Vec::new(),
    }
}

/// A `.jsonl` file's text. Only a `.jsonl` path is opened, so a payload that names a credential
/// file reads nothing (`AGENTS.md`, "Hard rules").
fn read_jsonl(path: &str) -> Option<String> {
    let path = Path::new(path);
    if path.extension()? != "jsonl" {
        return None;
    }
    fs::read_to_string(path).ok()
}

/// The JSON records of JSONL transcript `path`; `None` when it cannot be read.
pub(crate) fn records(path: &str) -> Option<Vec<Value>> {
    let text = read_jsonl(path)?;
    Some(
        text.lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect(),
    )
}

/// Where the turn that starts after `initial` steps came from: Daniel's prompt is a `USER_INPUT` at
/// the step before, and a wake-up has none (decision 6). `None` when the transcript cannot be read.
fn turn_origin(transcript: &str, initial: u64) -> Option<Origin> {
    let records = records(transcript)?;
    let before = initial.checked_sub(1)?;
    let last = records.iter().find(|r| r["step_index"] == before);
    Some(match last {
        Some(r) if r["type"] == "USER_INPUT" => Origin::Daniel,
        _ => Origin::WakeUp,
    })
}

/// What a call of tool `name` is.
pub(crate) fn kind(name: &str) -> ToolKind {
    match name {
        "write_file" | "replace_file_content" | "multi_replace_file_content" | "edit_file" => {
            ToolKind::Edit
        }
        "run_command" => ToolKind::Command,
        "invoke_subagent" => ToolKind::Spawn,
        _ => ToolKind::Other,
    }
}

pub(crate) fn tool_of(name: &str, args: &Value) -> Tool {
    let kind = kind(name);
    let text = |keys: &[&str]| {
        keys.iter()
            .find_map(|key| args[key].as_str())
            .map(str::to_owned)
    };
    let command = (kind == ToolKind::Command)
        .then(|| text(&["CommandLine"]))
        .flatten();
    let path = (kind == ToolKind::Edit)
        .then(|| text(&["Path", "TargetFile", "AbsolutePath"]))
        .flatten();
    // The step's line is agy's own summary of the call, else what the call touches.
    let mut input = Map::new();
    let summary = text(&["toolSummary"]);
    if let Some(command) = &command {
        input.insert("command".into(), command.clone().into());
    }
    match (&summary, &path) {
        (Some(summary), _) => {
            input.insert("description".into(), summary.clone().into());
        }
        (None, Some(path)) => {
            input.insert("file_path".into(), path.clone().into());
        }
        _ => {}
    }
    Tool {
        name: name.into(),
        input: input.into(),
        kind,
        command,
        path,
    }
}

/// The branch checked out in the git workspace of `cwd`, read from `HEAD`.
fn branch(cwd: &str) -> Option<String> {
    let mut folder = Some(Path::new(cwd));
    while let Some(dir) = folder {
        let git = dir.join(".git");
        let head = if git.is_dir() {
            git.join("HEAD")
        } else if git.is_file() {
            // A linked worktree's `.git` file names its own folder.
            let text = fs::read_to_string(&git).ok()?;
            Path::new(text.strip_prefix("gitdir:")?.trim()).join("HEAD")
        } else {
            folder = dir.parent();
            continue;
        };
        let head = fs::read_to_string(head).ok()?;
        return head.trim().strip_prefix("ref: refs/heads/").map(Into::into);
    }
    None
}
