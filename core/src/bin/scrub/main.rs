//! Turns event log lines into a test fixture (danielrispler/Falinks#370): reads the log on stdin and writes the events of
//! the sessions named as arguments, or of every session when none is named, to stdout. Each event
//! keeps its shape and loses its content (danielrispler/Falinks#369, "Further Notes", "Fixtures and Daniel's data").
//!
//!     cargo run -p studio-core --bin scrub -- <session-id>... < events.jsonl > fixture.jsonl

use serde_json::{Map, Value};
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let sessions: Vec<String> = std::env::args().skip(1).collect();
    let mut out = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let Ok(Value::Object(event)) = serde_json::from_str(&line?) else {
            continue;
        };
        let session = event.get("session_id").and_then(Value::as_str);
        if sessions.is_empty() || sessions.iter().any(|s| Some(s.as_str()) == session) {
            writeln!(out, "{}", Value::Object(scrub(event)))?;
        }
    }
    Ok(())
}

/// Top-level text fields that stay as they are: ids, names, folders and states. Numbers and flags,
/// timings among them, stay anywhere.
const TOP_LEVEL_TEXT: &[&str] = &[
    "agent_id",
    "agent_type",
    "cwd",
    "hook_event_name",
    "notification_type",
    "permission_mode",
    "prompt_id",
    "reason",
    "session_id",
    "source",
    "studio_asked_by",
    "tool_name",
    "tool_use_id",
];

/// Text fields of a built-in tool's input or result, or of a background task, that stay: ids,
/// types, statuses and paths. An MCP tool keeps none.
const LABELS: &[&str] = &[
    "agentId",
    "agent_type",
    "backgroundTaskId",
    "commandName",
    "filePath",
    "file_path",
    "id",
    "isolation",
    "skill",
    "status",
    "subagent_type",
    "task_id",
    "task_type",
    "type",
];

/// The tools whose `description` is the short label an agent gives a spawn or a command, as a
/// background task's is.
const DESCRIBED: &[&str] = &["Agent", "Bash", "background_tasks"];

/// An event with its content gone: prompt text, messages, tool inputs and results, file contents.
fn scrub(event: Map<String, Value>) -> Map<String, Value> {
    let tool = event.get("tool_name").and_then(Value::as_str);
    let tool = tool.unwrap_or_default().to_string();
    let field = |(key, value): (String, Value)| {
        let text = value.as_str().unwrap_or_default();
        let value = match key.as_str() {
            "prompt" => markup(text).into(),
            // The core tells a command that ran by the exit code on the error's first line.
            "error" => match text.lines().next().filter(|line| exit_code(line)) {
                Some(line) => line.into(),
                None => blank(value),
            },
            "tool_input" | "tool_response" => labels_only(value, &tool),
            // Daniel's choice on a card, `allow` or `deny`; the nonce is already gone.
            "studio_answer" => match value {
                Value::Object(answer) => answer
                    .into_iter()
                    .filter(|(key, _)| key == "decision")
                    .collect(),
                other => blank(other),
            },
            "background_tasks" => match value {
                Value::Array(tasks) => tasks
                    .into_iter()
                    .map(|task| labels_only(task, "background_tasks"))
                    .collect(),
                other => blank(other),
            },
            key if TOP_LEVEL_TEXT.contains(&key) => value,
            _ => blank(value),
        };
        (key, value)
    };
    event.into_iter().map(field).collect()
}

/// Whether a line is a command's exit code alone, as in `Exit code 101`.
fn exit_code(line: &str) -> bool {
    line.strip_prefix("Exit code ")
        .is_some_and(|code| !code.is_empty() && code.bytes().all(|b| b.is_ascii_digit()))
}

/// Tool `tool`'s input or result, or a background task, with only its labels' text. A label of
/// several lines or over 200 bytes is content.
fn labels_only(value: Value, tool: &str) -> Value {
    let Value::Object(fields) = value else {
        return blank(value);
    };
    let label = |key: &str, value: &Value| {
        let kept = match key {
            "description" => DESCRIBED.contains(&tool),
            "name" => tool == "Agent",
            "command" => fixture_command(tool, value),
            key => LABELS.contains(&key) && !tool.starts_with("mcp__"),
        };
        kept && value
            .as_str()
            .is_some_and(|s| s.len() <= 200 && !s.contains('\n'))
    };
    let field = |(key, value): (String, Value)| {
        let value = if label(&key, &value) {
            value
        } else {
            blank(value)
        };
        (key, value)
    };
    Value::Object(fields.into_iter().map(field).collect())
}

/// The word that marks a command as a recording script's own (the blocked-run fixture's, recorded before #572),
/// so its text may stay in a fixture. Any other command is content.
const FIXTURE_MARK: &str = "studio-fixture-";

/// Whether `value` is a Bash command or background task's one-line command that carries `FIXTURE_MARK`.
fn fixture_command(tool: &str, value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|c| (tool == "Bash" || tool == "background_tasks") && c.contains(FIXTURE_MARK))
}

/// A value with its content gone and its type kept: text becomes `…`, a list or an object empty.
fn blank(value: Value) -> Value {
    match value {
        Value::String(_) => "…".into(),
        Value::Array(_) => Value::Array(Vec::new()),
        Value::Object(_) => Value::Object(Map::new()),
        scalar => scalar,
    }
}

/// The tags Claude Code wraps around the prompts it sends and around pasted text.
const TAGS: &[&str] = &[
    "agent-message",
    "duration_ms",
    "note",
    "output-file",
    "pasted_content",
    "result",
    "status",
    "subagent_tokens",
    "summary",
    "task-id",
    "task-notification",
    "tool-use-id",
    "tool_uses",
    "usage",
    "worktree",
    "worktreeBranch",
    "worktreePath",
];

/// A prompt's markup without its text, so a wake-up stays one (danielrispler/Falinks#369, decision 7) and Daniel's prompt
/// keeps none of its words. Each tag in `TAGS` stays, with the attributes whose key and value look
/// like ids, as in `<agent-message from="a2526f0d7353298ee">`. So does an id or a status right after
/// `<status>` or a tag ending in `id`, as in `<task-id>a1191df87fb730aa0</task-id>`. Any other tag
/// becomes `<…>`, which shows that new markup came without its name, and other text becomes `…`.
fn markup(prompt: &str) -> String {
    let mut out = String::new();
    let mut text = String::new();
    // Whether the tag right before `text` may hold an id or a status, and its name.
    let mut holds_id = false;
    let mut open = "";
    let mut rest = prompt;
    while let Some(at) = rest.find('<') {
        text.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some((tag, len, name)) = tag(rest) else {
            text.push('<');
            rest = &rest[1..];
            continue;
        };
        let kept = text.trim();
        if holds_id && id_like(kept) {
            out.push_str(kept);
        } else if let Some(summary) = summary_label(open, kept) {
            out.push_str(&summary);
        } else if !kept.is_empty() {
            out.push('…');
        }
        out.push_str(&tag);
        holds_id = !tag.starts_with("</") && (name == "status" || name.ends_with("id"));
        open = if tag.starts_with("</") { "" } else { name };
        text.clear();
        rest = &rest[len..];
    }
    if !(text + rest).trim().is_empty() {
        out.push('…');
    }
    out
}

/// A background command's wake-up summary, as in `Background command "Run the gate" completed (exit code 0)`,
/// without the quoted description: `Background command "…" completed (exit code 0)`, `… failed with exit code
/// 144`, or `… completed (exit code 1: …)` without the explanation. `None` for any other summary, whose text is
/// content.
fn summary_label(open: &str, text: &str) -> Option<String> {
    let (first, last) = (text.find('"')?, text.rfind('"')?);
    let (head, tail) = (&text[..first], text.get(last + 1..)?);
    let word = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase());
    let number = |s: &str| s.parse::<i32>().is_ok();
    let mut words = tail.split_whitespace();
    let status = words.next().filter(|w| word(w))?;
    let rest = words.collect::<Vec<_>>().join(" ");
    let code = match rest
        .strip_prefix("(exit code ")
        .and_then(|r| r.strip_suffix(')'))
    {
        _ if rest.is_empty() => String::new(),
        Some(n) if number(n) => format!(" {rest}"),
        Some(inner) => match inner.split_once(':') {
            Some((n, _)) if number(n) => format!(" (exit code {n}: …)"),
            _ => return None,
        },
        None => match rest.strip_prefix("with exit code ") {
            Some(n) if number(n) => format!(" {rest}"),
            _ => return None,
        },
    };
    let shaped =
        open == "summary" && last > first && head == "Background command " && text.len() <= 400;
    shaped.then(|| format!("{head}\"…\" {status}{code}"))
}

/// The tag at the start of `s`, rebuilt: a tag in `TAGS` with its attributes whose key and value look
/// like ids, any other as `<…>`. Also its length in `s`, and its name, `…` for any other. `None` when
/// the `<` opens no tag, as in `3 < 4`.
fn tag(s: &str) -> Option<(String, usize, &str)> {
    let end = s.find('>')?;
    let inner = s[1..end].trim_end_matches('/');
    let (close, inner) = match inner.strip_prefix('/') {
        Some(inner) => ("/", inner),
        None => ("", inner),
    };
    let name_end = inner
        .find(|c: char| !(c.is_ascii_alphanumeric() || "-_:".contains(c)))
        .unwrap_or(inner.len());
    let name = &inner[..name_end];
    if inner.contains('<') || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    if !TAGS.contains(&name) {
        return Some((format!("<{close}…>"), end + 1, "…"));
    }
    let mut tag = format!("<{close}{name}");
    // Quoted values sit between every other pair of quotes, each after its `key=`.
    let parts: Vec<&str> = inner[name_end..].split('"').collect();
    for pair in parts.chunks(2) {
        if let [before, value] = pair
            && let Some(key) = before.trim_end().strip_suffix('=')
            && let Some(key) = key.split_whitespace().last()
            && id_like(key)
            && id_like(value)
        {
            // Writing to a `String` cannot fail.
            write!(tag, " {key}=\"{value}\"").ok();
        }
    }
    if s[1..end].ends_with('/') {
        tag.push('/');
    }
    tag.push('>');
    Some((tag, end + 1, name))
}

/// Text shaped like an agent, task or tool call id, or a status: one short word of letters, digits,
/// `-` and `_`.
fn id_like(text: &str) -> bool {
    (1..=32).contains(&text.len())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests;
