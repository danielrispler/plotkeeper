//! The Codex adapter (#359; spec #320, Harnesses): turns one of Codex's hook lines into the core's
//! neutral `Event`. Codex's event names, tool names and fields stay here.
//!
//! Codex's hooks are near Claude Code's shape (https://learn.chatgpt.com/docs/hooks, read 2026-11-23):
//! every payload has `session_id`, `transcript_path`, `cwd`, `hook_event_name` and `model`, and a
//! turn's events its `turn_id`, which is read as the prompt id. Its own events are `Interrupt`, which
//! ends a turn as the server's `StudioInterrupted` does for Claude Code, and the model-request and
//! compaction events, which install does not wire. Subagent events carry the parent's session id and
//! are not wired either: the adapter maps no subagents yet.

use crate::event::{Capabilities, Event, Happened, Start, Tool, ToolKind, TranscriptFormat};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The name Codex's hook entries pass (`--agent codex`), which its lines carry.
pub const CODEX: &str = "codex";

/// What Codex reports. Its `PermissionRequest` waits on Daniel, and every turn's event names the
/// permission mode and the model. Its transcript (a rollout file) has no reader here, so no version,
/// model or context size comes from it; no call durations and no background task ids. The hook maps
/// its turns, so they have trees (#426; spec #320, "Turn trees and pruning per harness").
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: CODEX,
    asks: true,
    background_tasks: false,
    replies: false,
    permission_mode: true,
    durations: false,
    transcript_version: false,
    transcript_model: false,
    context: false,
    turn_trees: true,
    transcript_format: TranscriptFormat::Unread,
};

/// The fields of a Codex hook payload the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Raw {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    transcript_path: String,
    model: Option<String>,
    permission_mode: Option<String>,
    /// The turn the event belongs to, on a turn's events.
    turn_id: Option<String>,
    #[serde(default)]
    prompt: String,
    /// The start of a session: `startup`, `resume`, `clear`, `compact` or `fork`.
    #[serde(default)]
    source: String,
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    #[serde(default)]
    last_assistant_message: String,
    #[serde(default)]
    studio_received_ms: u64,
    studio_terminal_id: Option<String>,
    studio_pid: Option<u32>,
    /// The trees the hook recorded for the turn, as for Claude Code's (#426).
    #[serde(default)]
    studio_turn_start: BTreeMap<String, String>,
    #[serde(default)]
    studio_turn_end: BTreeMap<String, String>,
    studio_touched_roots: Option<Vec<String>>,
}

/// The event of Codex hook line `line`; an error when it has no Codex payload's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str::<Raw>(line).map(adapt)
}

/// The first file a patch names (`*** Update File: …`, `*** Add File: …`, `*** Delete File: …`).
// shortcut: a patch that changes several files is taken as touching its first.
fn patched_file(patch: &str) -> Option<String> {
    patch.lines().find_map(|line| {
        let line = line.trim_start().strip_prefix("*** ")?;
        let (_, path) = line.split_once(" File: ")?;
        Some(path.trim().into())
    })
}

fn tool(raw: &Raw) -> Tool {
    let name = raw.tool_name.clone().unwrap_or_default();
    let input = &raw.tool_input;
    // `Bash` and `apply_patch` both carry their text in `command`.
    let command = input["command"].as_str();
    let kind = match name.as_str() {
        "apply_patch" => ToolKind::Edit,
        "Bash" => ToolKind::Command,
        "spawn_agent" => ToolKind::Spawn,
        _ if name.starts_with("mcp__") => ToolKind::External,
        _ => ToolKind::Other,
    };
    Tool {
        path: (kind == ToolKind::Edit)
            .then(|| command.and_then(patched_file))
            .flatten(),
        command: (kind == ToolKind::Command)
            .then(|| command.map(Into::into))
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
                "fork" => Start::Fork,
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
        "Interrupt" => happened.push(Happened::Interrupted),
        "PreToolUse" => happened.push(Happened::ToolStart {
            tool: tool(&raw),
            id: id.clone(),
        }),
        // Codex reports a command that exited non-zero here too, with no exit code the docs name.
        "PostToolUse" => happened.push(Happened::ToolEnd {
            tool: tool(&raw),
            id: id.clone(),
            failed: false,
        }),
        "PermissionRequest" => happened.push(Happened::PermissionAsked { tool: tool(&raw) }),
        _ => {}
    }
    if let Some(model) = raw.model.clone() {
        happened.push(Happened::Model(model));
    }
    if let Some(mode) = raw.permission_mode.clone() {
        happened.push(Happened::PermissionMode(mode));
    }
    Event {
        session_id: raw.session_id,
        agent_id: None,
        agent_type: None,
        cwd: raw.cwd,
        transcript_path: raw.transcript_path,
        prompt_id: raw.turn_id,
        studio_received_ms: raw.studio_received_ms,
        studio_terminal_id: raw.studio_terminal_id,
        studio_pid: raw.studio_pid,
        studio_turn_start: raw.studio_turn_start,
        studio_turn_end: raw.studio_turn_end,
        studio_touched_roots: raw.studio_touched_roots,
        capabilities: CAPABILITIES,
        line: Bytes::default(),
        happened,
    }
}
