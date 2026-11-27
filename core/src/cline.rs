//! The Cline adapter (#371; spec #320, Harnesses): turns one of the lines plotkeeper's Cline plugin
//! appends into the core's neutral `Event`. Cline's event names, tool names and fields stay here.
//!
//! Cline's hooks are its SDK plugins' runtime hooks; its plugin (`plugin-cline/plotkeeper.js`)
//! passes each to `plotkeeper-hook --agent cline` as one JSON object with `session_id` (Cline's
//! session id, which `cline --id` resumes), `cwd`, the run's `run_id`, which is read as the
//! prompt id, its `agent_id` and `parent_agent_id`, and
//! `hook_event_name`, the hook's name: `beforeRun` with the run's prompt, `beforeTool` and
//! `afterTool` with the tool's name, call id and input, and at the end whether it failed
//! (`is_error`) and, for a tool that does several things in one call, each one's outcome
//! (`outcomes`), and `afterRun` with the run's status, its final text (`last_assistant_message`)
//! and the text of its last message that has any (`last_text`). A run is a turn. Cline names no
//! model to its plugins and reports no session start or end, nor any permission ask. A run whose
//! `parent_agent_id` is set is a subagent's, which `spawn_agent` starts and waits for: its calls go
//! under the session with its own id, its run's start is not the session's turn's, and its run's end
//! ends the subagent (unverified: no real run of Cline's subagents was recorded). The tool names and inputs are those of a real run of Cline CLI 3.0.76
//! (`core/tests/fixtures/cline/run.jsonl`) and of its built-in tools (its binary's `read_files`,
//! `search_codebase`, `run_commands`, `fetch_web_content`, `apply_patch`, `editor`, `skills`,
//! `ask_question` and `submit_and_exit`, and `spawn_agent` when subagents are on).

use crate::event::{Capabilities, Event, Happened, Tool, ToolKind, TranscriptFormat};
use crate::ledger::Bytes;
use serde::Deserialize;
use serde_json::Value;

/// The name Cline's plugin passes (`--agent cline`), which its lines carry.
pub(crate) const CLINE: &str = "cline";

/// What Cline reports to its plugins: its runs, their prompts and last text, and its tool calls. No
/// permission or question text, no model, no transcript plotkeeper reads, no background task ids,
/// no permission mode, no durations, and no turn trees (spec #320, "Turn trees and pruning per
/// harness").
pub const CAPABILITIES: Capabilities = Capabilities {
    agent: CLINE,
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
    transcript_format: TranscriptFormat::Unread,
};

/// The fields of a line of Cline's plugin the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Raw {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
    /// The run, the turn's prompt id.
    run_id: Option<String>,
    agent_id: Option<String>,
    /// The agent that started this run's agent, on a subagent's line.
    parent_agent_id: Option<String>,
    /// On `beforeRun`.
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    #[serde(default)]
    is_error: bool,
    /// Each part's outcome, for a tool that does several things in one call.
    #[serde(default)]
    outcomes: Option<Vec<Part>>,
    /// On `afterRun`: Cline's final text for the run (its `outputText`), named as the other harnesses' reply.
    #[serde(default)]
    last_assistant_message: String,
    /// On `afterRun`: the text of the run's last message that has any. A run that ends with `submit_and_exit` has
    /// Cline's `Submission recorded (…)` line as its final text, and its answer here (#372).
    #[serde(default)]
    last_text: String,
    #[serde(default)]
    studio_received_ms: u64,
    studio_pid: Option<u32>,
}

/// How one part of a call went: one of `run_commands`' commands, or of `read_files`' files.
#[derive(Deserialize)]
struct Part {
    #[serde(default = "succeeded")]
    success: bool,
    /// Its error, such as `Command exited with code 1`.
    error: Option<String>,
}

fn succeeded() -> bool {
    true
}

/// A run's reply: the longer of Cline's final text and the text of its last message, by words, the final text on a
/// tie. A headless run's final text is its `submit_and_exit` summary, shorter than the answer it showed; a short
/// message before the submission keeps the summary.
fn reply(raw: &Raw) -> &str {
    let words = |text: &str| text.split_whitespace().count();
    if words(&raw.last_text) > words(&raw.last_assistant_message) {
        &raw.last_text
    } else {
        &raw.last_assistant_message
    }
}

/// The event of Cline plugin line `line`; an error when it has no such line's shape.
pub(crate) fn parse(line: &str) -> Result<Event, serde_json::Error> {
    serde_json::from_str::<Raw>(line).map(adapt)
}

/// What a call of tool `name` is.
fn kind(name: &str) -> ToolKind {
    match name {
        "editor" | "apply_patch" => ToolKind::Edit,
        "run_commands" => ToolKind::Command,
        // Not `Question`: its input is one `question` with its `options`, not the `questions` list the page's
        // question cards read, and Cline reports no answer to its plugins.
        "spawn_agent" => ToolKind::Spawn,
        _ => ToolKind::Other,
    }
}

/// The first file a patch in apply_patch's grammar names (`*** Update File: <path>`).
// shortcut: a patch that changes several files is taken as touching its first, as Codex's is.
fn patched_file(patch: &str) -> Option<String> {
    patch.lines().find_map(|line| {
        let line = line.trim_start().strip_prefix("*** ")?;
        let (_, path) = line.split_once(" File: ")?;
        Some(path.trim().into())
    })
}

fn tool(name: &str, input: &Value) -> Tool {
    let kind = kind(name);
    let path = match name {
        "editor" => input["path"].as_str().map(Into::into),
        // Its schema takes the patch as `input`, or as the whole input.
        "apply_patch" => (input["input"].as_str())
            .or(input.as_str())
            .and_then(patched_file),
        _ => None,
    };
    // `run_commands` runs each of its commands in turn, one a line here.
    let command = (kind == ToolKind::Command).then(|| {
        let commands = input["commands"].as_array().into_iter().flatten();
        let commands: Vec<_> = commands.filter_map(Value::as_str).collect();
        commands.join("\n")
    });
    Tool {
        path,
        command,
        input: input.clone(),
        name: name.to_owned(),
        kind,
    }
}

/// The prompt without the tag Cline wraps a typed prompt in (`<user_input mode="act">…</user_input>`).
fn prompt(text: &str) -> &str {
    let inner = (text.trim_end())
        .strip_prefix("<user_input")
        .and_then(|rest| rest.split_once('>'))
        .and_then(|(_, rest)| rest.strip_suffix("</user_input>"));
    inner.unwrap_or(text)
}

/// The exit code an error such as `Command exited with code 1` names.
fn exit_code(error: &str) -> Option<i32> {
    error
        .rsplit_once("exited with code ")?
        .1
        .trim()
        .parse()
        .ok()
}

fn adapt(raw: Raw) -> Event {
    let id = raw.tool_use_id.clone();
    let call = || tool(&raw.tool_name, &raw.tool_input);
    let subagent = raw.parent_agent_id.is_some();
    let mut happened = Vec::new();
    match raw.hook_event_name.as_str() {
        "beforeRun" if !subagent => happened.push(Happened::TurnStart {
            prompt: prompt(&raw.prompt).to_owned(),
            origin: None,
        }),
        "afterRun" => {
            happened.push(if subagent {
                Happened::SubagentEnd
            } else {
                Happened::TurnEnd
            });
            happened.push(Happened::Reply(reply(&raw).to_owned()));
        }
        "beforeTool" => happened.push(Happened::ToolStart {
            tool: call(),
            id: id.clone(),
        }),
        "afterTool" => {
            let parts = raw.outcomes.as_deref().unwrap_or_default();
            let failed = parts.iter().find(|p| !p.success);
            happened.push(Happened::ToolEnd {
                tool: call(),
                id: id.clone(),
                failed: raw.is_error || failed.is_some(),
            });
            let code = failed.and_then(|p| exit_code(p.error.as_deref()?));
            if let Some(code) = code.filter(|_| kind(&raw.tool_name) == ToolKind::Command) {
                happened.push(Happened::ExitedNonZero);
                happened.push(Happened::ExitCode(code));
            }
        }
        _ => {}
    }
    Event {
        session_id: raw.session_id,
        agent_id: raw.agent_id.filter(|_| subagent),
        agent_type: None,
        cwd: raw.cwd,
        transcript_path: String::new(),
        prompt_id: raw.run_id,
        studio_received_ms: raw.studio_received_ms,
        studio_pid: raw.studio_pid,
        studio_turn_start: Default::default(),
        studio_turn_end: Default::default(),
        studio_touched_roots: None,
        capabilities: CAPABILITIES,
        line: Bytes::default(),
        happened,
    }
}
