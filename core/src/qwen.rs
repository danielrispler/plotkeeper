//! The Qwen Code adapter (#360; spec #320, Harnesses): turns one of Qwen Code's hook lines into the
//! core's neutral `Event`. Qwen Code's event names, tool names and fields stay here.
//!
//! Qwen Code's hooks are near Claude Code's shape
//! (https://qwenlm.github.io/qwen-code-docs/en/users/features/hooks/, read 2026-11-23): every
//! payload has `session_id`, `transcript_path`, `cwd`, `hook_event_name`, `timestamp` and
//! `permission_mode`, and a model turn's events its `prompt_id`. `SessionStart` names the model.
//! Tool events carry the runtime tool id (`run_shell_command`, `write_file`, `edit`, …) and
//! `tool_use_id`; a failed call is its own event, `PostToolUseFailure`, and a call's events give
//! `duration_ms`. Subagent events are not mapped yet; a subagent's tool calls count as its parent's.

use crate::event::{Capabilities, Event, Happened, Start, Tool, ToolKind, TranscriptFormat};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;

/// The name Qwen Code's hook entries pass (`--agent qwen`), which its lines carry.
pub const QWEN: &str = "qwen";

/// What Qwen Code reports. Its `PermissionRequest` waits on Daniel, every event names the
/// permission mode, `SessionStart` names the model, and a call's end says how long it took. Its
/// transcript has no reader here, so no version or context size comes from it; no background task
/// ids (the docs give `Stop`'s `background_tasks` no shape), no subagents' replies, and no turn
/// trees (spec #320, "Turn trees and pruning per harness").
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: QWEN,
    asks: true,
    background_tasks: false,
    replies: false,
    permission_mode: true,
    durations: true,
    transcript_version: false,
    transcript_model: false,
    context: false,
    turn_trees: false,
    transcript_format: TranscriptFormat::Unread,
};

/// The fields of a Qwen Code hook payload the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Raw {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    transcript_path: String,
    /// On `SessionStart`.
    model: Option<String>,
    permission_mode: Option<String>,
    /// The model turn the event belongs to.
    prompt_id: Option<String>,
    #[serde(default)]
    prompt: String,
    /// The start of a session: `startup`, `resume`, `clear` or `compact`.
    #[serde(default)]
    source: String,
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    duration_ms: Option<u64>,
    #[serde(default)]
    last_assistant_message: String,
    /// The kind of a `Notification`, as its matcher names it: `permission_prompt`, `idle_prompt`, ….
    #[serde(default)]
    notification_type: String,
    #[serde(default)]
    studio_received_ms: u64,
    studio_terminal_id: Option<String>,
    studio_pid: Option<u32>,
}

/// The event of Qwen Code hook line `line`; an error when it has no Qwen Code payload's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str::<Raw>(line).map(adapt)
}

fn tool(raw: &Raw) -> Tool {
    let name = raw.tool_name.clone().unwrap_or_default();
    let input = &raw.tool_input;
    let kind = match name.as_str() {
        "write_file" | "edit" => ToolKind::Edit,
        "run_shell_command" => ToolKind::Command,
        // shortcut: the docs do not show how an MCP tool is named; Claude Code's prefix is assumed.
        _ if name.starts_with("mcp__") => ToolKind::External,
        _ => ToolKind::Other,
    };
    Tool {
        path: (kind == ToolKind::Edit)
            .then(|| input["file_path"].as_str().map(Into::into))
            .flatten(),
        command: (kind == ToolKind::Command)
            .then(|| input["command"].as_str().map(Into::into))
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
        "SessionStart" => {
            happened.push(Happened::SessionStart);
            happened.push(Happened::StartedFrom(match raw.source.as_str() {
                "clear" => Start::Clear,
                "resume" => Start::Resume,
                "compact" => Start::Compact,
                _ => Start::Other,
            }));
        }
        "SessionEnd" => happened.push(Happened::SessionEnd),
        "UserPromptSubmit" => happened.push(Happened::TurnStart {
            prompt: raw.prompt.clone(),
            origin: None,
        }),
        "Stop" => {
            happened.push(Happened::TurnEnd {
                background_pending: false,
            });
            happened.push(Happened::Reply(raw.last_assistant_message.clone()));
        }
        "PreToolUse" => happened.push(Happened::ToolStart {
            tool: tool(&raw),
            id: id.clone(),
        }),
        "PostToolUse" | "PostToolUseFailure" => happened.push(Happened::ToolEnd {
            tool: tool(&raw),
            id: id.clone(),
            failed: raw.hook_event_name == "PostToolUseFailure",
        }),
        "PermissionRequest" => happened.push(Happened::PermissionAsked { tool: tool(&raw) }),
        // The field's name is assumed Claude Code's: the docs name only the matcher's values.
        "Notification" => happened.push(match raw.notification_type.as_str() {
            "permission_prompt" => Happened::PermissionNotice,
            "idle_prompt" => Happened::IdlePrompt,
            _ => Happened::OtherNotice,
        }),
        _ => {}
    }
    if let Some(model) = raw.model.clone() {
        happened.push(Happened::Model(model));
    }
    if let Some(mode) = raw.permission_mode.clone() {
        happened.push(Happened::PermissionMode(mode));
    }
    if let Some(ms) = raw.duration_ms {
        happened.push(Happened::Took(ms));
    }
    Event {
        session_id: raw.session_id,
        agent_id: None,
        agent_type: None,
        cwd: raw.cwd,
        transcript_path: raw.transcript_path,
        prompt_id: raw.prompt_id,
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
