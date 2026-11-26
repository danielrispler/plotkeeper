//! The OpenCode adapter (#370; spec #320, Harnesses): turns one of the lines plotkeeper's OpenCode
//! plugin appends into the core's neutral `Event`. OpenCode's event names, tool names and fields
//! stay here.
//!
//! OpenCode 2 has no command hooks; its in-process plugin (`plugin-opencode/plotkeeper.js`) passes
//! each event to `plotkeeper-hook --agent opencode` as one JSON object with `session_id`, `cwd` and
//! `hook_event_name`, the name of what it saw: its `session` `prompt` hook (`session.prompt`, with
//! the prompt and its message id), its `tool` hooks (`tool.execute.before` and
//! `tool.execute.after`, with the tool's name, call id and input, and at the end its `status`,
//! error and a shell's `exit`), and from its event stream `permission.asked` and a turn's end
//! (`session.execution.succeeded`, `.failed` or `.interrupted`; `.started` only for a turn that
//! starts with no prompt, as when a background subagent's result wakes its parent), which carries
//! the model the turn's steps named and the turn's last text. OpenCode reports no session start or end. A subagent is a
//! child session whose calls the plugin puts under the session that spawned it. The tool names
//! and inputs are those of a real run of OpenCode 2.4.1 (`core/tests/fixtures/opencode/run.jsonl`)
//! and of its tools' sources (anomalyco/opencode at v2.4.2, `packages/core/src/tool/plugin/`).

use crate::event::{Capabilities, Event, Happened, Origin, Tool, ToolKind, TranscriptFormat};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;

/// The name OpenCode's plugin passes (`--agent opencode`), which its lines carry.
pub(crate) const OPENCODE: &str = "opencode";

/// What OpenCode reports: a permission that waits on Daniel, and the model a turn ran at. Its
/// transcript has no reader here, so no version or context size; no background task ids, no
/// subagents' replies, no permission mode, no durations, and no turn trees (spec #320, "Turn trees
/// and pruning per harness").
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: OPENCODE,
    asks: true,
    background_tasks: false,
    replies: false,
    permission_mode: false,
    durations: false,
    transcript_version: false,
    transcript_model: false,
    context: false,
    turn_trees: false,
    empty_window_starts: false,
    transcript_format: TranscriptFormat::Unread,
};

/// The fields of a line of OpenCode's plugin the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Raw {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
    /// The prompt's message id, on `session.prompt`.
    prompt_id: Option<String>,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    /// A call's end: `completed` or `error`.
    #[serde(default)]
    status: String,
    /// A shell call's exit code.
    exit: Option<i32>,
    /// What a `permission.asked` asks: its `action` and `resources`.
    #[serde(default)]
    permission: Value,
    /// On a turn's end.
    model: Option<String>,
    #[serde(default)]
    last_assistant_message: String,
    #[serde(default)]
    studio_received_ms: u64,
    studio_terminal_id: Option<String>,
    studio_pid: Option<u32>,
}

/// The event of OpenCode plugin line `line`; an error when it has no such line's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str::<Raw>(line).map(adapt)
}

/// A call of tool `name` with `input`, in session folder `cwd`. OpenCode's tools take a `path`
/// relative to it, or absolute; the call's input names it absolute, as the ledger reads it there.
fn tool(name: &str, input: &Value, cwd: &str) -> Tool {
    let kind = match name {
        "write" | "edit" | "patch" => ToolKind::Edit,
        "shell" => ToolKind::Command,
        "question" => ToolKind::Question,
        _ => ToolKind::Other,
    };
    let mut input = input.clone();
    let path = input["path"].as_str().map(|path| {
        let path = std::path::Path::new(cwd).join(path);
        path.to_string_lossy().into_owned()
    });
    if let Some(path) = &path {
        input["path"] = path.as_str().into();
    }
    Tool {
        path: (kind == ToolKind::Edit).then_some(path).flatten(),
        command: (kind == ToolKind::Command)
            .then(|| input["command"].as_str().map(Into::into))
            .flatten(),
        input,
        name: name.to_owned(),
        kind,
    }
}

fn adapt(raw: Raw) -> Event {
    let id = raw.tool_use_id.clone();
    let call = || tool(&raw.tool_name, &raw.tool_input, &raw.cwd);
    let mut happened = Vec::new();
    match raw.hook_event_name.as_str() {
        "session.prompt" => happened.push(Happened::TurnStart {
            prompt: raw.prompt.clone(),
            origin: None,
        }),
        // Sent only for a turn that starts with no prompt: a background subagent's result woke it.
        "session.execution.started" => happened.push(Happened::TurnStart {
            prompt: String::new(),
            origin: Some(Origin::WakeUp),
        }),
        "session.execution.succeeded"
        | "session.execution.failed"
        | "session.execution.interrupted" => {
            happened.push(Happened::TurnEnd {
                background_pending: false,
            });
            happened.push(Happened::Reply(raw.last_assistant_message.clone()));
            if let Some(model) = raw.model.clone() {
                happened.push(Happened::Model(model));
            }
        }
        "tool.execute.before" => happened.push(Happened::ToolStart {
            tool: call(),
            id: id.clone(),
        }),
        "tool.execute.after" => {
            let nonzero_exit = raw.exit.filter(|code| *code != 0);
            happened.push(Happened::ToolEnd {
                tool: call(),
                id: id.clone(),
                failed: raw.status == "error" || nonzero_exit.is_some(),
            });
            if let Some(code) = nonzero_exit {
                happened.push(Happened::ExitedNonZero);
                happened.push(Happened::ExitCode(code));
            }
        }
        "permission.asked" => {
            // An ask names an action and its resources: a shell's command, an edit's path, or a URL, a query, an
            // agent's or a skill's id (https://opencode.ai/docs/permissions, at v2.4.2).
            let action = raw.permission["action"].as_str().unwrap_or_default();
            let resources = &raw.permission["resources"];
            let input = match action {
                "shell" => serde_json::json!({ "command": resources[0] }),
                "write" | "edit" | "patch" | "read" => serde_json::json!({ "path": resources[0] }),
                _ => serde_json::json!({ "resources": resources }),
            };
            happened.push(Happened::PermissionAsked {
                tool: tool(action, &input, &raw.cwd),
            });
        }
        _ => {}
    }
    Event {
        session_id: raw.session_id,
        agent_id: None,
        agent_type: None,
        cwd: raw.cwd,
        transcript_path: String::new(),
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
