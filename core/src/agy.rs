//! The agy adapter: turns the hook lines of Antigravity CLI (`agy`) and its transcript into the
//! core's neutral `Event`s (danielrispler/plotkeeper#43, decisions 2, 3, 6, 7, 10 and 15). agy's
//! event names, tool names, record types and files stay here.
//!
//! Unlike Claude's, the adapter has state: a subagent is a conversation of its own with no parent id
//! in its payload, a turn's origin is read from the transcript once, and hooks that arrive out of
//! order are tolerated by remembering which steps ended.

use crate::event::{
    Capabilities, Event, Happened, Origin, Spawned, Start, Tool, ToolKind, TranscriptFormat,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Drift {
    pub at: u64,
    pub kind: DriftKind,
}

/// A turn's start: its `initialNumSteps` and where it came from.
#[derive(Clone, Copy)]
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

/// One conversation: a session's own, or a subagent's.
#[derive(Clone, Default)]
struct Conversation {
    /// The conversation it is a subagent of, once found.
    parent: Option<String>,
    /// The child's file `state` when first seen.
    state: Option<String>,
    ended: bool,
    transcript: String,
    cwd: String,
    terminal: Option<String>,
    /// The open turn's start.
    turn: Option<Turn>,
    last_input: String,
    /// The calls (by `stepIdx`) that started and that ended.
    open: HashSet<String>,
    done: HashSet<String>,
    /// The transcript lines checked for unknown records.
    scanned: usize,
}

/// The adapter's state, one per studio.
#[derive(Clone, Default)]
pub(crate) struct Adapter {
    conversations: HashMap<String, Conversation>,
    /// The latest session's conversation on each terminal, which a new conversation there is a
    /// subagent of.
    sessions: HashMap<String, String>,
    pub drift: Vec<Drift>,
}

impl Adapter {
    /// The events of hook line `line`, or `None` when it is no JSON.
    pub fn events(&mut self, line: &str) -> Option<Vec<Event>> {
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
        self.know(&id, raw["studio_terminal_id"].as_str());
        let conversation = self.conversations.get_mut(&id)?;
        if let Some(text) = raw["transcriptPath"].as_str().filter(|t| !t.is_empty()) {
            conversation.transcript = text.into();
        }
        if let Some(text) = raw["cwd"].as_str().filter(|t| !t.is_empty()) {
            conversation.cwd = text.into();
        }
        let parent = conversation.parent.clone();
        let own = parent.is_none();
        let mut events = Vec::new();
        // A child of a session whose file names it goes under it by its own spawn event, which
        // the session's own agent sends.
        if parent.is_some() {
            events.extend(self.attach(&id, at));
        }
        let session = parent.clone().unwrap_or_else(|| id.clone());
        let mut event = frame(
            &self.conversations[&id],
            session,
            parent.map(|_| id.clone()),
            at,
        );
        event.studio_turn_start = strings(&raw["studio_turn_start"]);
        event.studio_turn_end = strings(&raw["studio_turn_end"]);
        if own && let Some(model) = raw["modelName"].as_str().filter(|m| !m.is_empty()) {
            event.happened.push(Happened::Model(model.into()));
        }
        match hook {
            Hook::SessionStart if own => {
                event.happened.push(Happened::SessionStart);
                event.happened.push(Happened::StartedFrom(Start::Other));
                event
                    .happened
                    .extend(branch(&event.cwd).map(Happened::Branch));
            }
            Hook::PreInvocation if own && raw["invocationNum"] == 0 => {
                self.turn_starts(&id, &raw, &mut event);
            }
            Hook::PreToolUse | Hook::PostToolUse => {
                self.tool_event(&id, hook, &raw, &mut event);
            }
            Hook::Stop => {
                let idle = raw["fullyIdle"].as_bool().unwrap_or(true);
                if own {
                    event.happened.push(Happened::TurnEnd {
                        background_pending: !idle,
                    });
                    self.check_turn(&id, at);
                } else if idle {
                    event.happened.push(Happened::SubagentEnd);
                    if let Some(conversation) = self.conversations.get_mut(&id) {
                        conversation.ended = true;
                    }
                }
                self.scan(&id, at);
            }
            _ => {}
        }
        if own && matches!(hook, Hook::PreInvocation | Hook::Stop) {
            events.extend(self.refresh(&id, at));
        }
        events.push(event);
        Some(events)
    }

    /// The main event of `line` with the adapter's state left as it is: only the conversation of
    /// the line, its parent's and its terminal's session are copied, so it costs the same however
    /// many sessions there are.
    pub fn peek(&self, line: &str) -> Option<Event> {
        let raw: Value = serde_json::from_str(line).ok()?;
        let id = raw["conversationId"].as_str()?;
        let terminal = raw["studio_terminal_id"].as_str();
        let session = terminal.and_then(|t| self.sessions.get(t));
        let parent = self.conversations.get(id).and_then(|c| c.parent.as_ref());
        let mut scratch = Adapter::default();
        for known in [
            Some(id),
            parent.map(String::as_str),
            session.map(String::as_str),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(conversation) = self.conversations.get(known) {
                scratch
                    .conversations
                    .insert(known.into(), conversation.clone());
            }
        }
        if let (Some(t), Some(session)) = (terminal, session) {
            scratch.sessions.insert(t.into(), session.clone());
        }
        scratch.events(line)?.pop()
    }

    fn drift(&mut self, at: u64, kind: DriftKind) {
        self.drift.push(Drift { at, kind });
    }

    /// Notes conversation `id`, first seen on `terminal`: a subagent of the session last seen on
    /// that terminal, or else a session itself.
    fn know(&mut self, id: &str, terminal: Option<&str>) {
        if self.conversations.contains_key(id) {
            return;
        }
        let parent = terminal.and_then(|t| self.sessions.get(t));
        let parent = parent.filter(|p| *p != id).cloned();
        if let (None, Some(t)) = (&parent, terminal) {
            self.sessions.insert(t.into(), id.into());
        }
        self.conversations.insert(
            id.into(),
            Conversation {
                parent,
                terminal: terminal.map(Into::into),
                ..Conversation::default()
            },
        );
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

    /// Looks for child `id`'s file in its parent's `subagents/` folder, and when it is there the
    /// first time, returns the spawn the session's own agent reports. A child whose file is not
    /// there yet is tried again at each of its events, until its `Stop`; a child that ends with none
    /// stays unattached (decision 7). A child's end by its file's `state` changing is told here too.
    fn attach(&mut self, id: &str, at: u64) -> Option<Event> {
        let conversation = self.conversations.get(id)?;
        let parent = conversation.parent.clone()?;
        if conversation.state.is_some() {
            return None;
        }
        let file = self.subagent_file(&parent, id)?;
        let step = file["spawnStepIndex"].as_u64();
        let prompt = step
            .and_then(|step| self.spawn_prompt(&parent, step))
            .unwrap_or_default();
        let descriptor = &file["subagentDescriptor"];
        let spawn = Spawned {
            child: id.into(),
            call: step.map(|s| s.to_string()).unwrap_or_default(),
            name: descriptor["role"].as_str().map(Into::into),
            prompt,
            agent_type: descriptor["typeName"].as_str().map(Into::into),
            worktree: false,
            finished: false,
        };
        self.conversations.get_mut(id)?.state = Some(file_state(&file).unwrap_or_default());
        let mut event = self.bare(&parent, None, at);
        event.happened.push(Happened::SubagentSpawned(spawn));
        Some(event)
    }

    /// The subagents of `parent` whose file `state` changed since it was first seen have ended.
    fn refresh(&mut self, parent: &str, at: u64) -> Vec<Event> {
        let children: Vec<String> = self
            .conversations
            .iter()
            .filter(|(_, c)| c.parent.as_deref() == Some(parent) && !c.ended && c.state.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        let mut events = Vec::new();
        for child in children {
            let seen = self.conversations[&child].state.clone();
            let now = self
                .subagent_file(parent, &child)
                .and_then(|f| file_state(&f));
            if now.is_some() && now != seen {
                let conversation = self.conversations.get_mut(&child).expect("known");
                conversation.ended = true;
                let mut event = self.bare(parent, Some(&child), at);
                event.happened.push(Happened::SubagentEnd);
                events.push(event);
            }
        }
        events
    }

    /// An event of conversation `session`'s session with nothing in it yet.
    fn bare(&self, session: &str, agent: Option<&str>, at: u64) -> Event {
        frame(
            &self.conversations[session],
            session.into(),
            agent.map(Into::into),
            at,
        )
    }

    /// The `.system_generated` folder of conversation `id`, from its transcript's path.
    fn generated(&self, id: &str) -> Option<&Path> {
        let transcript = Path::new(&self.conversations.get(id)?.transcript);
        transcript.parent()?.parent()
    }

    fn subagent_file(&self, parent: &str, child: &str) -> Option<Value> {
        let path = self
            .generated(parent)?
            .join(format!("subagents/{child}.json"));
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }

    /// The prompt of the `invoke_subagent` call at step `step` of `parent`'s transcript.
    fn spawn_prompt(&self, parent: &str, step: u64) -> Option<String> {
        let transcript = &self.conversations.get(parent)?.transcript;
        // The call's step comes after the response that made it.
        let before = step.checked_sub(1)?;
        records(transcript)?
            .into_iter()
            .find(|r| r["step_index"] == before)?["tool_calls"][0]["args"]["Prompt"]
            .as_str()
            .map(Into::into)
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

/// The `state` of a subagent's file.
fn file_state(file: &Value) -> Option<String> {
    file["state"].as_str().map(Into::into)
}

/// An event of `session`, from `conversation`'s place and files, with nothing in it yet.
fn frame(conversation: &Conversation, session: String, agent_id: Option<String>, at: u64) -> Event {
    Event {
        session_id: session,
        agent_id,
        agent_type: None,
        cwd: conversation.cwd.clone(),
        transcript_path: conversation.transcript.clone(),
        prompt_id: None,
        studio_received_ms: at,
        studio_terminal_id: conversation.terminal.clone(),
        studio_turn_start: BTreeMap::new(),
        studio_turn_end: BTreeMap::new(),
        capabilities: CAPABILITIES,
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

fn records(path: &str) -> Option<Vec<Value>> {
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

fn tool_of(name: &str, args: &Value) -> Tool {
    let kind = match name {
        "write_file" | "replace_file_content" | "multi_replace_file_content" | "edit_file" => {
            ToolKind::Edit
        }
        "run_command" => ToolKind::Command,
        "invoke_subagent" => ToolKind::Spawn,
        _ => ToolKind::Other,
    };
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
