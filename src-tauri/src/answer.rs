//! Answering from the card (#461, decision 16): the shell's side of the answer hook's files
//! (`studio_hook::answer`). The window calls `watch` when the session whose view shows changes; each
//! pass of the read loop then keeps the watching file's heartbeat fresh and sends that session's open
//! request as an `ask` event when it changes, in the same pass as the session list (#569). The
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
}

impl Watching {
    /// Watches `session`, or none, and returns its open request, if any.
    pub fn watch(&mut self, data: &Path, session: Option<String>) -> Option<Value> {
        self.session = session.filter(|s| answer::is_file_safe(s));
        self.ask = Value::Null;
        let now = answer::now_ms();
        self.alive(now);
        self.pass(data, now).filter(|ask| !ask.is_null())
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
/// `allow` or `deny` (#478). Neither answers in the terminal.
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
    use super::Watching;
    use serde_json::{Value, json};
    use studio_hook::answer;

    /// #569: the read loop's pass that reads the request carries it to the window, so a question
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

    /// #569: the read loop keeps the heartbeat only while the window shows life, so a frozen window
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
}
