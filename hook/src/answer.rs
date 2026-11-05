//! The answer hook (#461, decision 16): a synchronous hook that lets Daniel answer from the studio card.
//! While the studio watches the asking session, the hook leaves a request in `asks/<session>.json`
//! with a one-time nonce and waits for `asks/<session>.answer.json` echoing it. It returns no decision,
//! so the terminal asks as today, when nobody watches, the watching goes stale, Daniel answers in the
//! terminal, or 120 seconds pass.
//!
//! Files, all in the studio's data directory, written whole (temp file plus rename) with mode 0600:
//! - `watching.json`, by the studio every second: `{"session_id", "pid", "heartbeat_ms"}`.
//! - `asks/<session>.json`, by the hook: `{"nonce", "hook_event_name", "tool_name", "tool_use_id",
//!   "tool_input", "asked_ms"}`.
//! - `asks/<session>.answer.json`, by the studio: for a question `{"nonce", "answers"}`, each answer a
//!   label or, for a multi-select, a list of labels; for a plan or a permission (#478)
//!   `{"nonce", "decision"}`, `allow` or `deny`. With neither, Daniel answers in the terminal.
//!
//! A card answer that decides the call is also appended to the event log as a `StudioAnswer` line
//! (`answered_line`), so the core and the decision history see it.
//!
//! Trust boundary: a process that could write the answer file could approve a call, so the hook takes
//! only a file of its own user with mode 0600 that echoes this request's nonce.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// A heartbeat older than this counts as nobody watching (decision 22).
pub const STALE_MS: u64 = 5_000;
/// With no answer by then, the terminal asks (decision 22).
pub const TIMEOUT_MS: u64 = 120_000;

/// What a call asks Daniel: a question, a plan to approve, or a permission for any other tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Question,
    Plan,
    Permission,
}

impl Kind {
    /// The kind of a call to `tool`.
    pub fn of(tool: &str) -> Self {
        match tool {
            "AskUserQuestion" => Self::Question,
            "ExitPlanMode" => Self::Plan,
            _ => Self::Permission,
        }
    }
}

/// Daniel's verdict on a plan or a permission from its card (#478), `allow` or `deny` in JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Allow,
    Deny,
}

pub fn watching_file(data: &Path) -> PathBuf {
    data.join("watching.json")
}

pub fn ask_file(data: &Path, session: &str) -> PathBuf {
    data.join("asks").join(format!("{session}.json"))
}

pub fn answer_file(data: &Path, session: &str) -> PathBuf {
    data.join("asks").join(format!("{session}.answer.json"))
}

/// Whether a session id may name a file: letters, digits, `-` and `_` only.
pub fn is_file_safe(session: &str) -> bool {
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    !session.is_empty() && session.chars().all(plain)
}

pub fn now_ms() -> u64 {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    since.as_millis() as u64
}

/// Writes `value` to `path` whole, readable by its user only, so a reader never sees half a file.
pub fn write_private(path: &Path, value: &Value) -> io::Result<()> {
    let folder = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
    let temp = folder.join(format!(".{}.{}.tmp", name.display(), std::process::id()));
    // A new file, so the mode holds even if a crashed writer left its temp file.
    let _ = fs::remove_file(&temp);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)?;
    file.write_all(value.to_string().as_bytes())?;
    fs::rename(&temp, path)
}

/// Whether the studio watches `session`: the watching file names it, its pid lives and its heartbeat
/// is fresh.
pub fn watched(data: &Path, session: &str) -> bool {
    let Ok(text) = fs::read_to_string(watching_file(data)) else {
        return false;
    };
    let watching: Value = serde_json::from_str(&text).unwrap_or_default();
    let pid = watching["pid"].as_i64().and_then(|p| i32::try_from(p).ok());
    let heartbeat = watching["heartbeat_ms"].as_u64().unwrap_or(0);
    watching["session_id"] == session
        && now_ms().saturating_sub(heartbeat) <= STALE_MS
        // Signal 0 only checks that the process exists and is the user's own.
        // SAFETY: `kill` takes plain integers and touches no memory; `pid > 0` keeps it to one
        // process, never a process group.
        && pid.is_some_and(|pid| pid > 0 && unsafe { libc::kill(pid, 0) } == 0)
}

/// The kind of `payload`'s call.
fn kind(payload: &Value) -> Kind {
    Kind::of(payload["tool_name"].as_str().unwrap_or_default())
}

/// Whether the hook waits for the card on this call: a question or a plan at its `PreToolUse`, or
/// a permission request. A question or a plan asked already, so its permission request needs no
/// second wait.
fn waits_for(payload: &Value) -> bool {
    let permission = kind(payload) == Kind::Permission;
    match payload["hook_event_name"].as_str() {
        Some("PreToolUse") => !permission,
        Some("PermissionRequest") => permission,
        _ => false,
    }
}

/// The hook's output for the studio's `answer` to `payload`, or `None` for no decision: a question
/// takes `answers`, a plan or a permission a `decision` of `allow` or `deny`.
pub fn reply(payload: &Value, answer: &Value) -> Option<Value> {
    let kind = kind(payload);
    if kind != Kind::Question {
        let verdict = Verdict::deserialize(answer.get("decision")?).ok()?;
        return Some(decide(payload, kind, verdict));
    }
    let answers = answer.get("answers")?.as_object()?;
    let mut joined = Map::new();
    for (question, chosen) in answers {
        let label = match chosen {
            Value::String(label) => label.clone(),
            // A multi-select's labels, joined with commas as Claude Code does.
            Value::Array(labels) => {
                let labels: Option<Vec<&str>> = labels.iter().map(Value::as_str).collect();
                labels?.join(", ")
            }
            _ => return None,
        };
        joined.insert(question.clone(), label.into());
    }
    if joined.is_empty() {
        return None;
    }
    // `updatedInput` replaces the whole input, so it keeps the original questions.
    let mut input = payload["tool_input"].as_object()?.clone();
    input.insert("answers".into(), joined.into());
    Some(json!({ "hookSpecificOutput": {
        "hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": input } }))
}

/// A plan's or a permission's output for `verdict`.
fn decide(payload: &Value, kind: Kind, verdict: Verdict) -> Value {
    let out = if kind == Kind::Permission {
        let decision = match verdict {
            Verdict::Allow => json!({ "behavior": "allow" }),
            Verdict::Deny => json!({ "behavior": "deny",
                "message": "Daniel denied this from the studio card." }),
        };
        json!({ "hookEventName": "PermissionRequest", "decision": decision })
    } else if verdict == Verdict::Allow {
        // A plan needs its input back with `allow`.
        json!({ "hookEventName": "PreToolUse", "permissionDecision": "allow",
            "updatedInput": payload["tool_input"] })
    } else {
        json!({ "hookEventName": "PreToolUse", "permissionDecision": "deny",
            "permissionDecisionReason":
                "Daniel rejected this plan from the studio card. Ask him what to change." })
    };
    json!({ "hookSpecificOutput": out })
}

/// The event log line that records Daniel's card answer to `payload` (#461, decision 17): the
/// payload as `StudioAnswer`, with the event that asked, the answer without its nonce, the terminal
/// and the time, as the logging hook adds them. An answer in the terminal leaves no such line.
pub fn answered_line(payload: &Value, answer: &Value) -> Value {
    let mut line = payload.clone();
    let mut answer = answer.clone();
    if let Some(answer) = answer.as_object_mut() {
        answer.remove("nonce");
    }
    line["studio_asked_by"] = payload["hook_event_name"].clone();
    line["hook_event_name"] = "StudioAnswer".into();
    line["studio_answer"] = answer;
    line["studio_received_ms"] = now_ms().into();
    if let Ok(terminal) = std::env::var("STUDIO_TERMINAL_ID") {
        line["studio_terminal_id"] = terminal.into();
    }
    line
}

/// Runs the hook for `payload` with the studio's data directory `data`: the output to print, if any.
pub fn run(payload: &Value, data: &Path, timeout: Duration) -> io::Result<Option<Value>> {
    let session = payload["session_id"].as_str().unwrap_or_default();
    if !waits_for(payload) || !is_file_safe(session) || !watched(data, session) {
        return Ok(None);
    }
    let nonce = nonce()?;
    let (ask, answer_path) = (ask_file(data, session), answer_file(data, session));
    let _ = fs::remove_file(&answer_path);
    // `agent_id` (absent for the session's own agent) tells the studio whose card shows it.
    let request = json!({
        "nonce": nonce,
        "hook_event_name": payload["hook_event_name"],
        "tool_name": payload["tool_name"],
        "tool_use_id": payload["tool_use_id"],
        "tool_input": payload["tool_input"],
        "agent_id": payload["agent_id"],
        "asked_ms": now_ms(),
    });
    write_private(&ask, &request)?;
    let answer = wait(data, session, &nonce, timeout);
    // shortcut: one ask per session at a time; a second ask meanwhile replaces the request, so this
    // one only times out. Key the files by tool_use_id if parallel subagent questions show up.
    let request_still_ours = fs::read_to_string(&ask).is_ok_and(|text| text.contains(&nonce));
    if request_still_ours {
        let _ = fs::remove_file(&ask);
        let _ = fs::remove_file(&answer_path);
    }
    let Some(answer) = answer else {
        return Ok(None);
    };
    let out = reply(payload, &answer);
    if out.is_some() {
        // The decision stands even if its line cannot be logged.
        let line = answered_line(payload, &answer);
        if let Err(e) = crate::append(&data.join("events.jsonl"), &line) {
            eprintln!("studio-answer: log the answer: {e}");
        }
    }
    Ok(out)
}

/// The answer file that echoes `nonce`, once it comes; `None` when watching stops or time runs out.
fn wait(data: &Path, session: &str, nonce: &str, timeout: Duration) -> Option<Value> {
    let until = Instant::now() + timeout;
    loop {
        if let Some(answer) = take_private_answer(&answer_file(data, session)) {
            if answer["nonce"] == nonce {
                return Some(answer);
            }
            eprintln!("studio-answer: an answer with a wrong nonce was refused");
        }
        if !watched(data, session) || Instant::now() >= until {
            return None;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// The answer file's content, removed as it is read; `None` when there is none, or when it is not a
/// regular file of the user's own with mode 0600, which is refused. The checks and the read use one
/// handle, opened without following a link, so the file cannot be swapped between them.
fn take_private_answer(path: &Path) -> Option<Value> {
    // O_NONBLOCK: opening a planted FIFO must not hang the hook.
    let opened = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path);
    if opened
        .as_ref()
        .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
    {
        return None;
    }
    let _ = fs::remove_file(path);
    let refuse = || eprintln!("studio-answer: an answer file not private to its user was refused");
    // A link fails to open (ELOOP), so it is refused too.
    let Ok(mut file) = opened else {
        refuse();
        return None;
    };
    let meta = file.metadata().ok()?;
    // SAFETY: `getuid` takes no arguments, always succeeds and touches no memory.
    let uid = unsafe { libc::getuid() };
    if !meta.is_file() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        refuse();
        return None;
    }
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

/// A one-time nonce: 16 random bytes in hex.
fn nonce() -> io::Result<String> {
    let mut bytes = [0u8; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
