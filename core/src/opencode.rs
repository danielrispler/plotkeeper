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
//! starts with no prompt, as when a background subagent's result wakes its parent, with the subagent
//! results its turn took, `reported`), which carries the model the turn's steps named and the turn's
//! last text; a subagent's result delivered into a running turn comes as `session.inbox.delivered`
//! (#779). OpenCode reports no session start or end. A subagent is a
//! child session that always runs in the background (#718): its `subagent` call's end names it
//! (`spawned_session_id`), and the plugin puts its calls, asks and the end of each of its runs
//! under the session that spawned it, with its own id (`subagent_session_id`). The tool names
//! and inputs are those of real runs of OpenCode 2.4.1 (`core/tests/fixtures/opencode/run.jsonl`
//! and `subagent.jsonl`)
//! and of its tools' sources (anomalyco/opencode at v2.4.2, `packages/core/src/tool/plugin/`).

use crate::event::{
    Capabilities, Event, Happened, Origin, Spawned, Tool, ToolKind, TranscriptFormat,
};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The name OpenCode's plugin passes (`--agent opencode`), which its lines carry.
pub(crate) const OPENCODE: &str = "opencode";

/// What OpenCode reports: a permission that waits on Daniel, the model a turn ran at, a
/// subagent's reply, and the trees the hook records of each turn (#717). Its transcript has no
/// reader here, so no version or context size; no background task ids, no permission mode and no
/// durations.
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: OPENCODE,
    asks: true,
    background_tasks: false,
    replies: true,
    permission_mode: false,
    durations: false,
    transcript_version: false,
    transcript_model: false,
    context: false,
    turn_trees: true,
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
    /// The subagent's own session, on a line of a subagent's.
    subagent_session_id: Option<String>,
    /// The child session a `subagent` call started, on its end.
    spawned_session_id: Option<String>,
    /// The subagent results a turn took (#779): on a wake-up, those it took before its first step,
    /// empty when none was a subagent's, as a shell job's; on `session.inbox.delivered`, one that
    /// joined a running turn. A log from the plugin before #779 has none.
    reported: Option<Vec<Reported>>,
    /// When the plugin sent the line (#717); a log from the plugin before it has none.
    sent_ms: Option<u64>,
    #[serde(default)]
    studio_received_ms: u64,
    /// The hook took the line after its turn was past (#744), which a subagent's line never is.
    #[serde(default)]
    studio_late: bool,
    studio_pid: Option<u32>,
    /// The trees the hook recorded for the turn, as for Claude Code's (#717).
    #[serde(default)]
    studio_turn_start: BTreeMap<String, String>,
    #[serde(default)]
    studio_turn_end: BTreeMap<String, String>,
    studio_touched_roots: Option<Vec<String>>,
}

/// A subagent's result, from OpenCode's `session.inbox.enqueued` item (`metadata.childID` and
/// `metadata.state`).
#[derive(Deserialize)]
struct Reported {
    subagent_session_id: String,
    /// `completed`, `error` or `cancelled`
    /// (`docs/studio/research/opencode-background-results.md`, section 1).
    state: String,
}

impl Reported {
    /// The child and the status it ends with, in the studio's words: an `error` failed, and
    /// `cancelled` is `interrupted`, as a Claude subagent's are.
    fn task(&self) -> (String, String) {
        let status = match self.state.as_str() {
            "error" => "failed",
            "cancelled" => "interrupted",
            other => other,
        };
        (self.subagent_session_id.clone(), status.into())
    }
}

/// The tasks a line reports on, each with how it ended.
fn reported(reported: Option<&[Reported]>) -> Option<Happened> {
    let tasks = reported?.iter().map(Reported::task);
    Some(Happened::TasksReported(tasks.collect()))
}

/// The event of OpenCode plugin line `line`; an error when it has no such line's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    let raw = serde_json::from_str::<Raw>(line)?;
    let tasks = reported(raw.reported.as_deref());
    let mut event = adapt(raw);
    // The subagent results a wake-up took, or one a running turn took on a line of its own,
    // `session.inbox.delivered` (#779).
    event.happened.extend(tasks);
    Ok(event)
}

/// What a call of tool `name` is.
// hook/src/lib.rs's `opencode_shaped` keeps a copy of the edit and shell names, as the hook does not depend on core.
pub(crate) fn kind(name: &str) -> ToolKind {
    match name {
        "write" | "edit" | "patch" => ToolKind::Edit,
        "shell" => ToolKind::Command,
        "subagent" => ToolKind::Spawn,
        "question" => ToolKind::Question,
        _ => ToolKind::Other,
    }
}

/// A call of tool `name` with `input`, in session folder `cwd`. OpenCode's tools take a `path`
/// relative to it, or absolute; the call's input names it absolute, as the ledger reads it there.
fn tool(name: &str, input: &Value, cwd: &str) -> Tool {
    let kind = kind(name);
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
    let mut prompt_id = raw.prompt_id;
    match raw.hook_event_name.as_str() {
        "session.prompt" => happened.push(Happened::TurnStart {
            prompt: raw.prompt.clone(),
            origin: None,
        }),
        // Sent only for a turn that starts with no prompt: a background subagent's result woke it.
        // It has no prompt id, but starts a turn of its own, also while the last one has not ended
        // in the log: its id is when it was sent, as the hook's is (`opencode_shaped`, #744).
        "session.execution.started" => {
            let sent = raw.sent_ms.unwrap_or(raw.studio_received_ms);
            prompt_id = Some(format!("wake-up {sent}"));
            happened.push(Happened::TurnStart {
                prompt: String::new(),
                origin: Some(Origin::WakeUp),
            });
        }
        // A turn's end, or a subagent's run's end, with its reply; a subagent's names no model. The
        // end of a turn already past when the hook took it would end the turn running now, so it
        // ends none, and its reply, of a turn the next start ended, is not this one's.
        "session.execution.succeeded"
        | "session.execution.failed"
        | "session.execution.interrupted" => {
            if raw.subagent_session_id.is_some() {
                happened.push(Happened::SubagentEnd);
            } else if !raw.studio_late {
                happened.push(Happened::TurnEnd);
            }
            if !raw.studio_late {
                happened.push(Happened::Reply(raw.last_assistant_message.clone()));
            }
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
            // A `subagent` call returns at its child's launch, which runs in the background.
            if let Some(child) = raw.spawned_session_id.clone()
                && raw.status != "error"
            {
                let input = &raw.tool_input;
                happened.push(Happened::SubagentSpawned(Spawned {
                    child,
                    call: id.clone(),
                    name: input["description"].as_str().map(Into::into),
                    prompt: input["prompt"].as_str().unwrap_or_default().into(),
                    agent_type: input["agent"].as_str().map(Into::into),
                    worktree: false,
                    // OpenCode runs every subagent in the background.
                    finished: false,
                }));
            }
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
            // An ask of a tool call carries that call's tool and input, so the studio finds the call
            // it waits on. Any other names an action and its resources: a shell's command, an
            // edit's path, or a URL, a query, an agent's or a skill's id
            // (https://opencode.ai/docs/permissions, at v2.4.2).
            let action = raw.permission["action"].as_str().unwrap_or_default();
            let resources = &raw.permission["resources"];
            let input = match action {
                "shell" => serde_json::json!({ "command": resources[0] }),
                "write" | "edit" | "patch" | "read" => serde_json::json!({ "path": resources[0] }),
                _ => serde_json::json!({ "resources": resources }),
            };
            happened.push(Happened::PermissionAsked {
                tool: if raw.tool_name.is_empty() {
                    tool(action, &input, &raw.cwd)
                } else {
                    call()
                },
            });
        }
        _ => {}
    }
    // A start of a turn already past has no turn of its own to start, nor the running one to end.
    if raw.studio_late {
        prompt_id = None;
        happened.push(Happened::Late);
    }
    Event {
        session_id: raw.session_id,
        agent_id: raw.subagent_session_id,
        agent_type: None,
        cwd: raw.cwd,
        transcript_path: String::new(),
        prompt_id,
        studio_received_ms: raw.studio_received_ms,
        studio_pid: raw.studio_pid,
        studio_turn_start: raw.studio_turn_start,
        studio_turn_end: raw.studio_turn_end,
        studio_touched_roots: raw.studio_touched_roots,
        capabilities: CAPABILITIES,
        line: Bytes::default(),
        happened,
    }
}
