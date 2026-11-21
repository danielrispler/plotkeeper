//! Helpers the test modules share. A module keeps its own wrapper where its events differ in more
//! than a session, a folder and a time.

use serde_json::{Value, json};
use studio_core::{Change, Fingerprint, State, Studio};

/// `two-sessions.jsonl`'s sessions, both in a turn at its end: the first in `Falinks`, the second in
/// the `issue-322-terminals` worktree.
pub const TWO: &str = include_str!("fixtures/two-sessions.jsonl");
pub const FIRST: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
pub const SECOND: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";

/// Session `id` of `TWO`: its `Stop`, under the prompt id its turn runs under.
pub fn stop(id: &str) -> String {
    let prompt_id = match id {
        FIRST => "e2a7c9d1-3f5b-4a8e-9c6d-0b1a2f3e4d5c",
        _ => "71c3e5a9-b2d4-4f6e-8a0c-1e3d5f7a9b2c",
    };
    format!(
        r#"{{"session_id":"{id}","prompt_id":"{prompt_id}","hook_event_name":"Stop","stop_hook_active":false,"studio_received_ms":1793261020000}}"#
    )
}

/// The fields every hook event line carries.
pub fn base(session: &str, name: &str, cwd: &str, at: u64) -> Value {
    json!({
        "session_id": session,
        "hook_event_name": name,
        "cwd": cwd,
        "studio_received_ms": at,
    })
}

/// The line of `base` with the fields of `extra`, an object, added over it.
pub fn with(mut base: Value, extra: Value) -> String {
    let fields = base.as_object_mut().unwrap();
    fields.extend(extra.as_object().unwrap().clone());
    base.to_string()
}

/// A hook event line: `base`'s fields and those of `extra`.
pub fn event(session: &str, name: &str, cwd: &str, at: u64, extra: Value) -> String {
    with(base(session, name, cwd, at), extra)
}

/// A core that replayed the lines of `log`.
pub fn replay(log: &str) -> Studio {
    Studio::replayed(log.lines())
}

/// The session's state after each line of `log`, replayed: `None` before its first line.
pub fn states(log: &str, session: &str) -> Vec<Option<State>> {
    let mut studio = Studio::default();
    log.lines()
        .map(|line| {
            studio.replay(line);
            studio.session(session).map(|s| s.state)
        })
        .collect()
}

/// The `<task-notification>` that wakes a session when background task `task` ends as `status`.
pub fn task_notification(task: &str, status: &str) -> String {
    format!(
        "<task-notification><task-id>{task}</task-id><status>{status}</status></task-notification>"
    )
}

/// Takes `line` live, as the shell does: each fingerprint the core asks for is passed to `find`,
/// and the change it returns, if any, is handed back to the core with the line.
pub fn apply_looking(
    studio: &mut Studio,
    line: &str,
    mut find: impl FnMut(&Fingerprint) -> Option<Change>,
) -> Vec<studio_core::Output> {
    let looks = studio.fingerprint_folders(line);
    let changes = looks
        .iter()
        .filter_map(|look| Some((look.key.clone(), find(look)?)));
    studio.apply_with_changes(line, changes.collect())
}

/// A `.jsonl` file holding `contents`, deleted when the guard is dropped: keep it for as long as the test
/// reads it.
pub fn temp_file(contents: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .expect("make a temp file");
    std::io::Write::write_all(&mut file, contents.as_bytes()).expect("write the temp file");
    file
}
