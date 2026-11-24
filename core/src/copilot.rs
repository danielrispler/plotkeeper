//! The Copilot adapter (#361; spec #320, Harnesses): turns one of Copilot's hook lines, from its CLI
//! or VS Code's agent, into the core's neutral `Event`. Copilot's event names, tool names and fields
//! stay here.
//!
//! Copilot sends each payload in one of two casings, by the casing of the event name its entry is
//! wired to (https://docs.github.com/en/copilot/reference/hooks-configuration, read 2026-11-23):
//! camelCase names (`sessionStart`, `preToolUse`, …) get camelCase payloads (`sessionId`,
//! `toolName`, `toolArgs` as a JSON string) with no event name in them, and PascalCase names
//! (`SessionStart`, `PreToolUse`, …) get snake_case payloads (`hook_event_name`, `session_id`,
//! `tool_input`) with Claude-style tool names. Install wires the camelCase names, which VS Code's
//! agent documents too, and each entry passes its event as `--event`, which the hook stores as
//! `studio_event`. Neither casing has a tool call id, a turn id or a model, so a call is known by its
//! tool and arguments, and turns by their prompts and stops.

use crate::event::{Capabilities, Event, Happened, Start, Tool, ToolKind, TranscriptFormat};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;

/// The name Copilot's hook entries pass (`--agent copilot`), which its lines carry.
pub const COPILOT: &str = "copilot";

/// What Copilot reports: its prompts, tool calls and turn ends. Its `permissionRequest` payload is
/// not documented, so install does not wire it and nothing waits on Daniel; no reply text, model or
/// permission mode reach its hooks, so a turn's reply is read from its transcript (#369), which is
/// read for nothing else, and no turn trees until the hook maps its turns (spec #320, "Turn trees
/// and pruning per harness").
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: COPILOT,
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

/// The fields of a Copilot hook payload the core reads, in either casing; the rest is ignored.
#[derive(Deserialize)]
struct Raw {
    #[serde(alias = "sessionId")]
    session_id: String,
    /// The event, in a snake_case payload.
    hook_event_name: Option<String>,
    /// The event the hook entry named (`--event`), for a camelCase payload, which has none.
    studio_event: Option<String>,
    #[serde(default)]
    cwd: String,
    #[serde(default, alias = "transcriptPath")]
    transcript_path: String,
    #[serde(default)]
    prompt: String,
    /// The start of a session: `startup`, `resume` or `new`.
    #[serde(default)]
    source: String,
    #[serde(alias = "toolName")]
    tool_name: Option<String>,
    /// An object in snake_case payloads, a JSON string in camelCase ones.
    #[serde(default, alias = "toolArgs")]
    tool_input: Value,
    /// When Copilot sent the event, in ms in a camelCase payload.
    #[serde(default)]
    timestamp: Value,
    #[serde(default)]
    studio_received_ms: u64,
    studio_terminal_id: Option<String>,
    studio_pid: Option<u32>,
}

/// The event of Copilot hook line `line`; an error when it has no Copilot payload's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    let raw = serde_json::from_str::<Raw>(line)?;
    if raw.hook_event_name.is_none() && raw.studio_event.is_none() {
        return Err(serde::de::Error::custom("a Copilot line names no event"));
    }
    Ok(adapt(raw))
}

/// A call's id: Copilot gives none, so its tool and arguments, which its start and end both carry.
// shortcut: two calls of one tool with the same arguments at once share an id.
fn call_id(name: &str, input: &Value) -> String {
    // FNV-1a, stable across builds, unlike the standard library's hasher.
    let text = format!("{name}\0{input}");
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("copilot-{hash:016x}")
}

fn tool(raw: &Raw) -> Tool {
    let name = raw.tool_name.clone().unwrap_or_default();
    let input = match &raw.tool_input {
        Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| text.clone().into()),
        other => other.clone(),
    };
    let kind = match name.as_str() {
        "bash" | "Bash" => ToolKind::Command,
        "edit" | "create" | "Edit" | "Write" => ToolKind::Edit,
        _ => ToolKind::Other,
    };
    let path = input["path"].as_str().or(input["file_path"].as_str());
    Tool {
        path: (kind == ToolKind::Edit)
            .then(|| path.map(Into::into))
            .flatten(),
        command: (kind == ToolKind::Command)
            .then(|| input["command"].as_str().map(Into::into))
            .flatten(),
        input,
        name,
        kind,
    }
}

fn adapt(raw: Raw) -> Event {
    let name = raw
        .hook_event_name
        .as_deref()
        .or(raw.studio_event.as_deref());
    let mut happened = Vec::new();
    let call = || {
        let tool = tool(&raw);
        let id = call_id(&tool.name, &tool.input);
        (tool, id)
    };
    match name.unwrap_or_default() {
        "sessionStart" | "SessionStart" => {
            happened.push(Happened::SessionStart);
            happened.push(Happened::StartedFrom(match raw.source.as_str() {
                "resume" => Start::Resume,
                _ => Start::Other,
            }));
        }
        "sessionEnd" | "SessionEnd" => happened.push(Happened::SessionEnd),
        "userPromptSubmitted" | "UserPromptSubmit" => happened.push(Happened::TurnStart {
            prompt: raw.prompt.clone(),
            origin: None,
        }),
        "agentStop" | "Stop" => {
            if let Some(reply) = final_response(&raw.transcript_path, &raw.timestamp) {
                happened.push(Happened::Reply(reply));
            }
            happened.push(Happened::TurnEnd {
                background_pending: false,
            });
        }
        "preToolUse" | "PreToolUse" => {
            let (tool, id) = call();
            happened.push(Happened::ToolStart { tool, id });
        }
        "postToolUse" | "PostToolUse" => {
            let (tool, id) = call();
            happened.push(Happened::ToolEnd {
                tool,
                id,
                failed: false,
            });
        }
        "postToolUseFailure" | "PostToolUseFailure" => {
            let (tool, id) = call();
            happened.push(Happened::ToolEnd {
                tool,
                id,
                failed: true,
            });
        }
        _ => {}
    }
    Event {
        session_id: raw.session_id,
        agent_id: None,
        agent_type: None,
        cwd: raw.cwd,
        transcript_path: raw.transcript_path,
        prompt_id: None,
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

/// The reply of the turn whose `agentStop` was sent at `timestamp`, from Copilot's transcript at
/// `path`: the last `assistant.message` with text after the turn's `user.message` and before the
/// `hook.start` that logged that `agentStop`'s input. A transcript read live may not yet hold that
/// record, so with none found it is the last such message. `None` when the turn has none or the
/// transcript cannot be read.
fn final_response(path: &str, timestamp: &Value) -> Option<String> {
    let mut last = None;
    for record in crate::agy::records(path)? {
        match record["type"].as_str() {
            Some("user.message") => last = None,
            Some("assistant.message") => {
                if let Some(text) = record["data"]["content"].as_str().map(str::trim)
                    && !text.is_empty()
                {
                    last = Some(text.to_owned());
                }
            }
            Some("hook.start")
                if record["data"]["hookType"] == "agentStop"
                    && timestamp.is_u64()
                    && record["data"]["input"]["timestamp"] == *timestamp =>
            {
                return last;
            }
            _ => {}
        }
    }
    last
}
