//! Answering from the card (danielrispler/Falinks#461, decision 16): the shell's side of the answer hook's files
//! (`studio_hook::answer`). The window calls `watch` when the session whose view shows changes; each
//! pass of the read loop then keeps the watching file's heartbeat fresh and sends that session's open
//! request as an `ask` event when it changes, in the same pass as the session list (danielrispler/Falinks#569). The
//! window calls `alive` every second, and the heartbeat stops once it has been silent for
//! `STALE_MS`, so a frozen window hands the question to the terminal. `answer` writes the reply that
//! echoes the request's nonce.

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use studio_hook::answer;
use tauri::State;

pub fn data() -> Result<PathBuf, String> {
    Ok(studio_hook::data_dir(
        &std::env::home_dir().ok_or("no home directory")?,
    ))
}

/// The session whose view the window shows, and its open request as last sent.
#[derive(Default)]
pub struct Watching {
    session: Option<String>,
    ask: Value,
    /// When the window last showed life, by `watch` or `alive`.
    alive_ms: u64,
    /// The watched session's blocked-command requests as last sent.
    blocked: Value,
}

impl Watching {
    /// Watches `session`, or none, and returns its open request, if any.
    pub fn watch(&mut self, data: &Path, session: Option<String>) -> Option<Value> {
        self.session = session.filter(|s| answer::is_file_safe(s));
        self.ask = Value::Null;
        self.blocked = Value::Null;
        let now = answer::now_ms();
        self.alive(now);
        self.pass(data, now).filter(|ask| !ask.is_null())
    }

    /// The watched session's blocked-command requests when they changed since the last call.
    pub fn blocked_pass(&mut self, data: &Path) -> Option<Value> {
        let session = self.session.as_deref()?;
        let blocked = blocked_requests(data, session);
        (blocked != self.blocked).then(|| {
            self.blocked = blocked.clone();
            blocked
        })
    }

    /// The window showed life at `now`.
    pub fn alive(&mut self, now: u64) {
        self.alive_ms = now;
    }

    /// One pass of the read loop at `now`: refreshes the heartbeat, or removes the watching file when
    /// no session is watched or the window has been silent for `STALE_MS`, and returns the open
    /// request (`null` for none) when it changed.
    pub fn pass(&mut self, data: &Path, now: u64) -> Option<Value> {
        let watching = answer::watching_file(data);
        let silent = now.saturating_sub(self.alive_ms) > answer::STALE_MS;
        let ask = match &self.session {
            None => {
                let _ = fs::remove_file(watching);
                Value::Null
            }
            Some(session) => {
                if silent {
                    let _ = fs::remove_file(watching);
                } else {
                    let heartbeat = json!({ "session_id": session, "pid": std::process::id(), "heartbeat_ms": now });
                    if let Err(e) = answer::write_private(&watching, &heartbeat) {
                        eprintln!("studio: keep watching {session}: {e}");
                    }
                }
                let ask = fs::read_to_string(answer::ask_file(data, session)).ok();
                ask.and_then(|text| serde_json::from_str(&text).ok())
                    .unwrap_or_default()
            }
        };
        (ask != self.ask).then(|| {
            self.ask = ask.clone();
            ask
        })
    }
}

/// The blocked-command requests of `session` the hook left, each with whether its hook still waits.
pub fn blocked_requests(data: &Path, session: &str) -> Value {
    let Ok(entries) = fs::read_dir(answer::blocked_dir(data, session)) else {
        return json!([]);
    };
    let requests = entries
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_name().to_string_lossy().contains(".answer."))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|r| {
            let pid = r["pid"].as_i64().and_then(|p| i32::try_from(p).ok());
            json!({ "nonce": r["nonce"], "tool_use_id": r["tool_use_id"],
                "waiting": pid.is_some_and(answer::pid_lives) })
        });
    Value::Array(requests.collect())
}

/// Daniel's Allow or Decline of `session`'s blocked-command request `nonce`: Allow grants the one-time
/// pass for the request's exact command and folder. Either writes the answer a waiting hook reads, or,
/// when the hook has returned, logs the decision itself.
pub fn answer_blocked(
    data: &Path,
    session: &str,
    nonce: &str,
    decision: answer::Verdict,
) -> Result<(), String> {
    if !answer::is_file_safe(session) || !answer::is_file_safe(nonce) {
        return Err("not a session or request id".into());
    }
    let dir = answer::blocked_dir(data, session);
    let text = fs::read_to_string(dir.join(format!("{nonce}.json"))).map_err(|e| e.to_string())?;
    let request: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if decision == answer::Verdict::Allow {
        let command = request["tool_input"]["command"].as_str();
        let cwd = request["cwd"].as_str();
        let (Some(command), Some(cwd)) = (command, cwd) else {
            return Err("the request names no command or folder".into());
        };
        answer::write_pass(data, session, command, cwd, answer::now_ms())
            .map_err(|e| e.to_string())?;
    }
    let reply = json!({ "nonce": nonce, "decision": decision });
    let hook_waits = request["pid"]
        .as_i64()
        .and_then(|p| i32::try_from(p).ok())
        .is_some_and(answer::pid_lives);
    if hook_waits {
        return answer::write_private(&dir.join(format!("{nonce}.answer.json")), &reply)
            .map_err(|e| e.to_string());
    }
    // A late answer: the hook has returned, so no one logs the decision but the studio, which
    // writes the line the hook would have and closes the request.
    let mut asked = request;
    asked["session_id"] = session.into();
    answer::log_answer(data, &answer::answered_line(&asked, &reply));
    fs::remove_file(dir.join(format!("{nonce}.json"))).map_err(|e| e.to_string())
}

/// Daniel chose Allow or Decline on a blocked command's card.
#[tauri::command]
pub fn answer_blocked_command(
    session: String,
    nonce: String,
    decision: answer::Verdict,
) -> Result<(), String> {
    answer_blocked(&data()?, &session, &nonce, decision)
}

/// The window shows `session`'s view, or none (`None`, also while the window is hidden). Returns the
/// session's open request, if any.
#[tauri::command]
pub fn watch(
    session: Option<String>,
    watching: State<Mutex<Watching>>,
) -> Result<Option<Value>, String> {
    Ok(watching.lock().unwrap().watch(&data()?, session))
}

/// The window is alive: it calls this every second while it watches a session.
#[tauri::command]
pub fn alive(watching: State<Mutex<Watching>>) {
    watching.lock().unwrap().alive(answer::now_ms());
}

/// Daniel answered `session`'s request `nonce` from its card: for a question, `answers` maps each
/// question to a label, or a multi-select's to its labels; for a plan or a permission, `decision` is
/// `allow` or `deny` (danielrispler/Falinks#478). Neither answers in the terminal.
#[tauri::command]
pub fn answer(
    session: String,
    nonce: String,
    answers: Option<Value>,
    decision: Option<answer::Verdict>,
) -> Result<(), String> {
    if !answer::is_file_safe(&session) {
        return Err("not a session id".into());
    }
    let mut reply = json!({ "nonce": nonce });
    if let Some(answers) = answers {
        reply["answers"] = answers;
    }
    if let Some(decision) = decision {
        reply["decision"] = json!(decision);
    }
    answer::write_private(&answer::answer_file(&data()?, &session), &reply)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Watching, answer_blocked, blocked_requests};
    use serde_json::{Value, json};
    use studio_hook::answer;

    /// danielrispler/Falinks#569: the read loop's pass that reads the request carries it to the window, so a question
    /// docks within the loop's delay; a pass with nothing new sends nothing.
    #[test]
    fn a_pass_sends_the_watched_sessions_request_once_it_appears() {
        let data = tempfile::tempdir().unwrap();
        let mut watching = Watching::default();
        assert_eq!(watching.watch(data.path(), Some("s1".into())), None);
        assert!(answer::watched(data.path(), "s1"));
        assert_eq!(watching.pass(data.path(), answer::now_ms()), None);

        let ask = json!({ "nonce": "n1", "tool_name": "AskUserQuestion" });
        answer::write_private(&answer::ask_file(data.path(), "s1"), &ask).unwrap();

        assert_eq!(watching.pass(data.path(), answer::now_ms()), Some(ask));
        assert_eq!(watching.pass(data.path(), answer::now_ms()), None);

        std::fs::remove_file(answer::ask_file(data.path(), "s1")).unwrap();
        assert_eq!(
            watching.pass(data.path(), answer::now_ms()),
            Some(Value::Null)
        );
    }

    /// danielrispler/Falinks#569: once the answer hook's request and the `PreToolUse` line are both there, the read loop's
    /// pass that reads the line, and sets needs you, also carries the request to the window.
    #[test]
    fn an_open_question_reaches_the_window_in_the_pass_that_sets_needs_you() {
        let data = tempfile::tempdir().unwrap();
        let mut studio = studio_core::Studio::default();
        let line = |name: &str, at: u64, extra: Value| {
            let mut line = json!({ "session_id": "s1", "hook_event_name": name, "cwd": "/",
                "studio_received_ms": at });
            line.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            line.to_string()
        };
        studio.apply(&line("SessionStart", 1_000, json!({ "source": "startup" })));
        studio.apply(&line(
            "UserPromptSubmit",
            2_000,
            json!({ "prompt": "Plan it.", "prompt_id": "p-1" }),
        ));
        let mut watching = Watching::default();
        watching.watch(data.path(), Some("s1".into()));
        let input = json!({ "questions": [{ "question": "Which base?", "options": [] }] });
        let call = json!({ "tool_name": "AskUserQuestion", "tool_use_id": "toolu_ask",
            "tool_input": input, "prompt_id": "p-1" });

        let ask =
            json!({ "nonce": "n1", "tool_name": "AskUserQuestion", "tool_use_id": "toolu_ask" });
        answer::write_private(&answer::ask_file(data.path(), "s1"), &ask).unwrap();
        studio.apply(&line("PreToolUse", 3_000, call));
        let sent = watching.pass(data.path(), answer::now_ms());

        let state = studio.session("s1").unwrap().state;
        assert_eq!(state, studio_core::State::NeedsYou);
        assert_eq!(sent, Some(ask));
    }

    /// danielrispler/Falinks#569: the read loop keeps the heartbeat only while the window shows life, so a frozen window
    /// hands a question to the terminal after `STALE_MS`, not after the hook's whole timeout.
    #[test]
    fn a_window_that_stops_answering_stops_the_hook_waiting() {
        let data = tempfile::tempdir().unwrap();
        let mut watching = Watching::default();
        let now = answer::now_ms();
        watching.watch(data.path(), Some("s1".into()));

        watching.pass(data.path(), now + answer::STALE_MS);
        assert!(answer::watched(data.path(), "s1"));

        watching.pass(data.path(), now + answer::STALE_MS + 1_000);
        assert!(!answer::watched(data.path(), "s1"));

        watching.alive(now + answer::STALE_MS + 1_000);
        watching.pass(data.path(), now + answer::STALE_MS + 1_000);
        assert!(answer::watched(data.path(), "s1"));
    }

    #[test]
    fn watching_no_session_stops_the_hook_waiting() {
        let data = tempfile::tempdir().unwrap();
        let mut watching = Watching::default();
        watching.watch(data.path(), Some("s1".into()));

        watching.watch(data.path(), None);

        assert!(!answer::watched(data.path(), "s1"));
    }

    /// A blocked command's request, as the hook leaves it.
    fn request(data: &std::path::Path, nonce: &str, pid: u32) {
        let request = json!({ "nonce": nonce, "hook_event_name": "PermissionDenied", "tool_name": "Bash",
            "tool_use_id": "toolu_9", "pid": pid, "cwd": "/work/repo",
            "tool_input": { "command": "claude -p hi" } });
        let path = answer::blocked_dir(data, "s1").join(format!("{nonce}.json"));
        answer::write_private(&path, &request).unwrap();
    }

    #[test]
    fn the_window_gets_a_sessions_blocked_requests_with_whether_their_hook_waits() {
        let data = tempfile::tempdir().unwrap();
        assert_eq!(blocked_requests(data.path(), "s1"), json!([]));
        request(data.path(), "n1", std::process::id());
        request(data.path(), "n2", 0);
        let mut sent = blocked_requests(data.path(), "s1");
        sent.as_array_mut()
            .unwrap()
            .sort_by_key(|r| r["nonce"].as_str().unwrap().to_owned());
        assert_eq!(
            sent,
            json!([{ "nonce": "n1", "tool_use_id": "toolu_9", "waiting": true },
                   { "nonce": "n2", "tool_use_id": "toolu_9", "waiting": false }])
        );
    }

    #[test]
    fn allow_grants_a_pass_for_the_exact_command_and_decline_does_not() {
        let data = tempfile::tempdir().unwrap();
        request(data.path(), "n1", std::process::id());
        request(data.path(), "n2", std::process::id());
        let passes = || std::fs::read_dir(answer::pass_dir(data.path(), "s1")).map(|d| d.count());

        answer_blocked(data.path(), "s1", "n2", answer::Verdict::Deny).unwrap();
        assert!(passes().is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound));
        let denied = answer::blocked_dir(data.path(), "s1").join("n2.answer.json");
        let text = std::fs::read_to_string(denied).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({ "nonce": "n2", "decision": "deny" })
        );

        answer_blocked(data.path(), "s1", "n1", answer::Verdict::Allow).unwrap();
        assert_eq!(passes().unwrap(), 1);
        let allowed = answer::blocked_dir(data.path(), "s1").join("n1.answer.json");
        let text = std::fs::read_to_string(allowed).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({ "nonce": "n1", "decision": "allow" })
        );
    }

    #[test]
    fn a_late_answer_whose_hook_has_returned_is_logged_by_the_studio_and_closes_the_request() {
        let data = tempfile::tempdir().unwrap();
        request(data.path(), "n1", 0);
        answer_blocked(data.path(), "s1", "n1", answer::Verdict::Allow).unwrap();
        // No hook waits to read an answer file or to log the decision.
        let blocked = answer::blocked_dir(data.path(), "s1");
        assert_eq!(std::fs::read_dir(&blocked).unwrap().count(), 0);
        let log = std::fs::read_to_string(data.path().join("events.jsonl")).unwrap();
        let line: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
        assert_eq!(line["hook_event_name"], "StudioAnswer");
        assert_eq!(line["studio_asked_by"], "PermissionDenied");
        assert_eq!(line["session_id"], "s1");
        assert_eq!(line["tool_use_id"], "toolu_9");
        assert_eq!(line["studio_answer"], json!({ "decision": "allow" }));
        assert_eq!(line["tool_input"]["command"], "claude -p hi");
        // The pass is granted all the same, for the agent's retry.
        assert_eq!(
            std::fs::read_dir(answer::pass_dir(data.path(), "s1"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn an_answer_for_a_request_that_is_not_there_writes_nothing() {
        let data = tempfile::tempdir().unwrap();
        assert!(answer_blocked(data.path(), "s1", "nope", answer::Verdict::Allow).is_err());
        assert!(!answer::blocked_dir(data.path(), "s1").exists());
    }
}
