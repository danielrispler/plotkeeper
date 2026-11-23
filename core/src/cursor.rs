//! The Cursor adapter (#362; spec #320, Harnesses): turns one of Cursor's hook lines, from its IDE or its
//! CLI (`agent`), into the core's neutral `Event`. Cursor's event names, tool names and fields stay here.
//!
//! Cursor's hooks have their own names (https://cursor.com/docs/hooks, read 2026-11-23): every payload
//! has `conversation_id`, the session, `generation_id`, which changes with each user message and is read
//! as the prompt id, `hook_event_name`, `model`, `cursor_version`, `workspace_roots` and
//! `transcript_path`. Install wires only the events that wait on no answer: `preToolUse` and the other
//! permission hooks block the action on an empty reply, so a call is recorded whole at its
//! `postToolUse` or `postToolUseFailure`, which carry its `tool_use_id`.

use crate::event::{Capabilities, Event, Happened, Start, Tool, ToolKind, TranscriptFormat};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;

/// The name Cursor's hook entries pass (`--agent cursor`), which its lines carry.
pub const CURSOR: &str = "cursor";

/// What Cursor reports. No permission or question reaches plotkeeper, as its permission hooks are not
/// wired; every event names the model. Its transcript has no reader here, so no version, model or
/// context share comes from it; no background task ids, and no turn trees until the hook maps its
/// turns (spec #320, "Turn trees and pruning per harness").
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: CURSOR,
    asks: false,
    background_tasks: false,
    replies: false,
    permission_mode: false,
    durations: false,
    transcript_version: false,
    transcript_model: false,
    context: false,
    turn_trees: false,
    transcript_format: TranscriptFormat::Unread,
};

/// The fields of a Cursor hook payload the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Raw {
    conversation_id: String,
    hook_event_name: String,
    generation_id: Option<String>,
    /// A tool event's folder; other events name only the workspace's roots.
    cwd: Option<String>,
    #[serde(default)]
    workspace_roots: Vec<String>,
    transcript_path: Option<String>,
    model: Option<String>,
    #[serde(default)]
    prompt: String,
    /// The reply of `afterAgentResponse`.
    #[serde(default)]
    text: String,
    /// How a `stop` ended its turn: `completed`, `aborted` or `error`.
    #[serde(default)]
    status: String,
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    #[serde(default)]
    studio_received_ms: u64,
    studio_terminal_id: Option<String>,
    studio_pid: Option<u32>,
}

/// The event of Cursor hook line `line`; an error when it has no Cursor payload's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str::<Raw>(line).map(adapt)
}

fn tool(raw: &Raw) -> Tool {
    let name = raw.tool_name.clone().unwrap_or_default();
    let input = &raw.tool_input;
    let kind = match name.as_str() {
        "Write" | "Delete" => ToolKind::Edit,
        "Shell" => ToolKind::Command,
        "Task" => ToolKind::Spawn,
        _ if name.starts_with("MCP:") => ToolKind::External,
        _ => ToolKind::Other,
    };
    let text = |key: &str| input[key].as_str().map(String::from);
    Tool {
        path: (kind == ToolKind::Edit)
            .then(|| text("file_path").or_else(|| text("path")))
            .flatten(),
        command: (kind == ToolKind::Command)
            .then(|| text("command"))
            .flatten(),
        input: input.clone(),
        name,
        kind,
    }
}

fn adapt(raw: Raw) -> Event {
    let id = raw.tool_use_id.clone();
    let mut happened = Vec::new();
    match raw.hook_event_name.as_str() {
        "sessionStart" => {
            happened.push(Happened::SessionStart);
            happened.push(Happened::StartedFrom(Start::Other));
        }
        "sessionEnd" => happened.push(Happened::SessionEnd),
        "beforeSubmitPrompt" => happened.push(Happened::TurnStart {
            prompt: raw.prompt.clone(),
            origin: None,
        }),
        "afterAgentResponse" => happened.push(Happened::Reply(raw.text.clone())),
        "stop" if raw.status == "aborted" => happened.push(Happened::Interrupted),
        "stop" => happened.push(Happened::TurnEnd {
            background_pending: false,
        }),
        // The call is recorded at its end: its start, then its end.
        "postToolUse" | "postToolUseFailure" => {
            happened.push(Happened::ToolStart {
                tool: tool(&raw),
                id: id.clone(),
            });
            happened.push(Happened::ToolEnd {
                tool: tool(&raw),
                id: id.clone(),
                failed: raw.hook_event_name == "postToolUseFailure",
            });
        }
        _ => {}
    }
    if let Some(model) = raw.model.clone().filter(|m| !m.is_empty()) {
        happened.push(Happened::Model(model));
    }
    let cwd = raw
        .cwd
        .or_else(|| raw.workspace_roots.first().cloned())
        .unwrap_or_default();
    Event {
        session_id: raw.conversation_id,
        agent_id: None,
        agent_type: None,
        cwd,
        transcript_path: raw.transcript_path.unwrap_or_default(),
        prompt_id: raw.generation_id.filter(|g| !g.is_empty()),
        studio_received_ms: raw.studio_received_ms,
        studio_terminal_id: raw.studio_terminal_id,
        studio_pid: raw.studio_pid,
        studio_turn_start: Default::default(),
        studio_turn_end: Default::default(),
        studio_touched_roots: None,
        capabilities: CAPABILITIES,
        line: Bytes::default(),
        happened,
    }
}
