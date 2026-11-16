//! The Claude Code adapter: turns one of its hook lines into the core's neutral `Event`
//! (danielrispler/plotkeeper#43, decisions 2 and 3). Claude's event names, tool names and markup
//! stay here.

use crate::event::{
    Capabilities, Event, Happened, Spawned, Start, Tool, ToolKind, TranscriptFormat,
};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// What Claude Code reports: everything the core reads today.
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: "claude",
    asks: true,
    background_tasks: true,
    replies: true,
    permission_mode: true,
    durations: true,
    transcript_version: true,
    transcript_format: TranscriptFormat::Claude,
};

/// Tools that ask Daniel in any permission mode, before any `PermissionRequest`.
const QUESTION_TOOL: &str = "AskUserQuestion";
const PLAN_TOOL: &str = "ExitPlanMode";

/// The line the shell appends when Daniel renames a session (`crate::RENAME`), deletes one
/// (`crate::DELETE`), or answers from a card (`crate::STUDIO_ANSWER`).
use crate::{DELETE, PROMPT_NAME, RENAME, STUDIO_ANSWER};

/// The four fields of a tool response the core reads (danielrispler/Falinks#410, decision 20). The rest, file contents
/// included, is skipped without building a JSON tree.
#[derive(Default, Deserialize)]
struct ToolResponse {
    #[serde(rename = "backgroundTaskId")]
    background_task_id: Option<String>,
    /// On an `Agent` call's `PostToolUse`, the child's id and status.
    #[serde(rename = "agentId")]
    agent_id: Option<String>,
    status: Option<String>,
    #[serde(default)]
    answers: Value,
}

/// A response that is not a JSON object yields none of the four, and keeps its event.
fn tool_response<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ToolResponse, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    Ok(serde_json::from_str(raw.get()).unwrap_or_default())
}

/// The fields of a hook payload the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Raw {
    session_id: String,
    hook_event_name: String,
    /// The subagent that sent the event, or `None` for the session's own agent.
    agent_id: Option<String>,
    agent_type: Option<String>,
    #[serde(default)]
    prompt: String,
    /// On `Stop`, the session's running tasks.
    #[serde(default)]
    background_tasks: Value,
    #[serde(default)]
    cwd: String,
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    #[serde(default, deserialize_with = "tool_response")]
    tool_response: ToolResponse,
    /// A failed call's error, which for a command that ran holds its exit code and output.
    #[serde(default)]
    error: String,
    #[serde(default)]
    last_assistant_message: String,
    /// The session's transcript, whose lines carry Claude Code's version.
    #[serde(default)]
    transcript_path: String,
    #[serde(default)]
    notification_type: String,
    /// Claude Code's id of the prompt the event belongs to, absent until the first user input.
    prompt_id: Option<String>,
    /// The start of a session: `startup`, `resume`, `clear` or `compact`.
    #[serde(default)]
    source: String,
    /// The end of a session: `clear`, `logout`, `prompt_input_exit`, `other` and the like.
    #[serde(default)]
    reason: String,
    #[serde(default)]
    studio_received_ms: u64,
    /// The session's permission mode, which `SessionStart` leaves out.
    permission_mode: Option<String>,
    /// On a tool's result, how long its call took.
    duration_ms: Option<u64>,
    /// The terminal the session runs in, which the hook records from `STUDIO_TERMINAL_ID`.
    studio_terminal_id: Option<String>,
    /// The trees the hook recorded at a turn's first touch of each worktree, by root (danielrispler/Falinks#486).
    #[serde(default)]
    studio_turn_start: BTreeMap<String, String>,
    /// The trees the hook recorded at the end of the turn this event ends, by root.
    #[serde(default)]
    studio_turn_end: BTreeMap<String, String>,
    /// On the studio's own `StudioRename` line, the session's new name; empty for none.
    #[serde(default)]
    studio_name: String,
    /// On the answer hook's `StudioAnswer` line, Daniel's answer from the card: `answers` for a
    /// question, `decision` for a plan or a permission.
    #[serde(default)]
    studio_answer: Value,
}

/// The event of hook line `line`; an error when it is no Claude Code line.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str::<Raw>(line).map(adapt)
}

fn tool(raw: &Raw) -> Tool {
    let name = raw.tool_name.clone().unwrap_or_default();
    let input = &raw.tool_input;
    let kind = match name.as_str() {
        "Edit" | "Write" => ToolKind::Edit,
        "Bash" => ToolKind::Command,
        "Agent" => ToolKind::Spawn,
        "Skill" => ToolKind::Skill(input["skill"].as_str().map(Into::into)),
        QUESTION_TOOL => ToolKind::Question,
        PLAN_TOOL => ToolKind::Plan,
        _ if name.starts_with("mcp__") => ToolKind::External,
        _ => ToolKind::Other,
    };
    Tool {
        path: (kind == ToolKind::Edit)
            .then(|| input["file_path"].as_str().map(Into::into))
            .flatten(),
        command: input["command"].as_str().map(Into::into),
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
        "SessionEnd" => {
            happened.push(Happened::SessionEnd);
            if raw.reason == "clear" {
                happened.push(Happened::EndedByClear);
            }
        }
        "UserPromptSubmit" => {
            happened.push(Happened::TurnStart {
                prompt: raw.prompt.clone(),
                origin: None,
            });
            let reported: Vec<(String, String)> = reported(&raw.prompt)
                .map(|(id, status)| (id.into(), status.into()))
                .collect();
            if !reported.is_empty() {
                happened.push(Happened::TasksReported(reported));
            }
        }
        "Stop" => {
            let pending = raw
                .background_tasks
                .as_array()
                .is_some_and(|t| !t.is_empty());
            happened.push(Happened::TurnEnd {
                background_pending: pending,
            });
            happened.push(Happened::Reply(raw.last_assistant_message.clone()));
        }
        "SubagentStop" => {
            happened.push(Happened::SubagentEnd);
            happened.push(Happened::Reply(raw.last_assistant_message.clone()));
        }
        "PreToolUse" => happened.push(Happened::ToolStart {
            tool: tool(&raw),
            id: id.clone(),
        }),
        name @ ("PostToolUse" | "PostToolUseFailure") => {
            let failed = name == "PostToolUseFailure";
            let tool = tool(&raw);
            if !failed
                && tool.kind == ToolKind::Spawn
                && let Some(child) = raw.tool_response.agent_id.clone()
            {
                let input = &raw.tool_input;
                happened.push(Happened::SubagentSpawned(Spawned {
                    child,
                    call: id.clone(),
                    name: input["description"].as_str().map(Into::into),
                    prompt: input["prompt"].as_str().unwrap_or_default().into(),
                    agent_type: input["subagent_type"].as_str().map(Into::into),
                    worktree: input["isolation"] == "worktree",
                    finished: raw.tool_response.status.as_deref() == Some("completed"),
                }));
            }
            happened.push(Happened::ToolEnd {
                tool,
                id: id.clone(),
                failed,
            });
            if failed && raw.error.starts_with("Exit code") {
                happened.push(Happened::ExitedNonZero);
                let code = raw.error["Exit code".len()..].trim_start();
                let digits = code.split(|c: char| !c.is_ascii_digit() && c != '-').next();
                if let Some(code) = digits.and_then(|d| d.parse().ok()) {
                    happened.push(Happened::ExitCode(code));
                }
            }
            if failed && raw.error.contains("doesn't want to proceed") {
                happened.push(Happened::Rejected);
            }
            happened.push(Happened::Answers(raw.tool_response.answers.clone()));
        }
        "PermissionRequest" => happened.push(Happened::PermissionAsked { tool: tool(&raw) }),
        "Notification" => happened.push(match raw.notification_type.as_str() {
            "permission_prompt" => Happened::PermissionNotice,
            "idle_prompt" => Happened::IdlePrompt,
            _ => Happened::OtherNotice,
        }),
        "Elicitation" => happened.push(Happened::ElicitationAsked),
        RENAME => happened.push(Happened::Renamed(raw.studio_name.clone())),
        PROMPT_NAME => happened.push(Happened::PromptNamed(raw.studio_name.clone())),
        DELETE => happened.push(Happened::Deleted),
        STUDIO_ANSWER => happened.push(Happened::Answered {
            tool: tool(&raw),
            id: id.clone(),
            answer: raw.studio_answer.clone(),
        }),
        _ => {}
    }
    if let Some(task) = raw.tool_response.background_task_id.clone() {
        happened.push(Happened::BackgroundTask(task));
    }
    if let Some(tasks) = raw.background_tasks.as_array() {
        let ids = tasks.iter().filter_map(|task| task["id"].as_str());
        happened.push(Happened::BackgroundTasks(ids.map(Into::into).collect()));
    }
    if let Some(mode) = raw.permission_mode.clone() {
        happened.push(Happened::PermissionMode(mode));
    }
    if let Some(ms) = raw.duration_ms {
        happened.push(Happened::Took(ms));
    }
    Event {
        session_id: raw.session_id,
        agent_id: raw.agent_id,
        agent_type: raw.agent_type,
        cwd: raw.cwd,
        transcript_path: raw.transcript_path,
        prompt_id: raw.prompt_id,
        studio_received_ms: raw.studio_received_ms,
        studio_terminal_id: raw.studio_terminal_id,
        studio_turn_start: raw.studio_turn_start,
        studio_turn_end: raw.studio_turn_end,
        capabilities: CAPABILITIES,
        line: Bytes::default(),
        happened,
    }
}

/// How a prompt names task `id`: in the markup of a wake-up, in other markup (drift), or not at all.
pub(crate) enum Names {
    WakeUp,
    Unrecognised,
    No,
}

/// Whether `prompt` names task `id`. A wake-up names a known task id of the session in markup the
/// studio knows: `<agent-message from="…">` or `<task-id>…</task-id>`. A known id in other markup
/// is drift, and one in plain text is Daniel's.
pub(crate) fn names(prompt: &str, id: &str) -> Names {
    if prompt.contains(&format!("<agent-message from=\"{id}\""))
        || prompt.contains(&format!("<task-id>{id}</task-id>"))
    {
        Names::WakeUp
    } else if prompt.contains(&format!("=\"{id}\"")) || prompt.contains(&format!(">{id}</")) {
        Names::Unrecognised
    } else {
        Names::No
    }
}

/// The tasks a wake-up reports on, each with how it ended: its `<task-id>` and its `<status>`
/// (danielrispler/Falinks#370 note).
fn reported(prompt: &str) -> impl Iterator<Item = (&str, &str)> {
    let notices = prompt.split("<task-notification>").skip(1);
    notices.filter_map(|notice| Some((tag(notice, "task-id")?, tag(notice, "status")?)))
}

/// The text of the first `<name>` element in `text`.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(&format!("<{name}>"))?;
    Some(rest.split_once(&format!("</{name}>"))?.0)
}
