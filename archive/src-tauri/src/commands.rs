//! The window's learning-layer and session-view commands: each handler only forwards to the function of the same name in
//! `studio_server`, which holds the state and the logic (danielrispler/plotkeeper#321). The workbench's commands stay in
//! their own modules.

use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use studio_server::{self as server, App, Cleared, answer as server_answer, stop_rules};
use studio_understanding::{Rating, Row, SessionCards, StoredSummary, Tap, WriterKind};
use tauri::State;

type Shared<'a> = State<'a, Arc<App>>;

#[tauri::command]
pub fn summary_settings(
    model: String,
    effort: String,
    writer: WriterKind,
    on_open: bool,
    self_test: Option<u64>,
    app: Shared,
) {
    server::summary_settings(&app, model, effort, writer, on_open, self_test);
}

#[tauri::command]
pub fn check_lists(lists: HashMap<String, Vec<String>>, app: Shared) {
    server::check_lists(&app, lists);
}

#[tauri::command(async)]
pub fn check_output(
    session: String,
    agent: Option<String>,
    call: String,
    app: Shared,
) -> Result<Option<studio_core::CheckOutput>, String> {
    server::check_output(&app, session, agent, call)
}

#[tauri::command(async)]
pub fn ask_facts(
    session: String,
    anchor: studio_core::Anchor,
    app: Shared,
) -> Result<Option<studio_core::AskFacts>, String> {
    server::ask_facts(&app, session, anchor)
}

#[tauri::command(async)]
pub fn turn_file_hunk(
    session: String,
    turn: usize,
    root: String,
    path: String,
    from: Option<String>,
    all: Option<bool>,
    app: Shared,
) -> Option<studio_understanding::Hunk> {
    server::turn_file_hunk(&app, session, turn, root, path, from, all)
}

#[tauri::command(async)]
pub fn hunk_opened(session: String, turn: usize, app: Shared) {
    server::hunk_opened(&app, session, turn);
}

#[tauri::command(async)]
pub fn turn_summary(id: String, app: Shared) -> Result<Option<StoredSummary>, String> {
    server::turn_summary(&app, id)
}

#[tauri::command]
pub fn turn_reply(id: String, index: usize, app: Shared) -> Option<String> {
    server::turn_reply(&app, id, index)
}

#[tauri::command(async)]
pub fn agent_summary(agent_id: String, app: Shared) -> Option<String> {
    server::agent_summary(&app, agent_id)
}

#[tauri::command]
pub fn sessions(app: Shared) -> Vec<Row> {
    server::sessions(&app)
}

#[tauri::command]
pub fn select(id: Option<String>, app: Shared) -> Value {
    server::select(&app, id)
}

#[tauri::command]
pub fn select_terminal(terminal: String, app: Shared) -> Value {
    server::select_terminal(&app, terminal)
}

#[tauri::command]
pub fn open(agent_id: String, app: Shared) -> Value {
    server::open(&app, agent_id)
}

#[tauri::command]
pub fn subagent_log(
    session: String,
    agent_id: String,
    offset: u64,
    app: Shared,
) -> Option<studio_core::SubagentLog> {
    server::subagent_log(&app, session, agent_id, offset)
}

#[tauri::command]
pub fn selected(app: Shared) -> Value {
    server::selected(&app)
}

#[tauri::command]
pub fn cards(app: Shared) -> SessionCards {
    server::cards(&app)
}

#[tauri::command(async)]
pub fn shown(ids: Vec<String>, app: Shared) {
    server::shown(&app, ids);
}

#[tauri::command(async)]
pub fn guess(id: String, option: usize, app: Shared) {
    server::guess(&app, id, option);
}

#[tauri::command(async)]
pub fn quiz_tap(id: String, tap: Tap, app: Shared) -> Option<StoredSummary> {
    server::quiz_tap(&app, id, tap)
}

#[tauri::command]
pub fn learning(now: u64, day_start: u64, app: Shared) -> Option<studio_understanding::Learning> {
    server::learning(&app, now, day_start)
}

#[tauri::command(async)]
pub fn key_wrong(id: String, app: Shared) {
    server::key_wrong(&app, id);
}

#[tauri::command(async)]
pub fn rate(id: String, rating: Rating, app: Shared) {
    server::rate(&app, id, rating);
}

#[tauri::command(async)]
pub fn explain_more(id: String, app: Shared) -> Option<String> {
    server::explain_more(&app, id)
}

#[tauri::command(async)]
pub fn ask(session: String, item: String, question: String, app: Shared) -> Result<String, String> {
    server::ask(&app, session, item, question)
}

#[tauri::command(async)]
pub fn ask_anchor(
    session: String,
    anchor: studio_core::Anchor,
    what: String,
    question: String,
    app: Shared,
) -> Result<String, String> {
    server::ask_anchor(&app, session, anchor, what, question)
}

#[tauri::command(async)]
pub fn ask_why_not(
    session: String,
    turn: usize,
    name: String,
    app: Shared,
) -> Result<studio_understanding::WhyNot, String> {
    server::ask_why_not(&app, session, turn, name)
}

#[tauri::command(async)]
pub fn rename(id: String, name: String) -> Result<(), String> {
    server::rename(id, name)
}

#[tauri::command(async)]
pub fn delete_session(id: String) -> Result<(), String> {
    server::delete_session(id)
}

#[tauri::command]
pub fn clearing(sessions: Vec<Cleared>, app: Shared) {
    server::clearing(&app, sessions);
}

#[tauri::command(async)]
pub fn closed(terminal: String, app: Shared) -> Result<(), String> {
    server::closed(&app, terminal)
}

#[tauri::command]
pub fn rail_shown(shown: bool, app: Shared) {
    server::rail_shown(&app, shown);
}

#[tauri::command]
pub fn split_shown(terminal: Option<String>, app: Shared) {
    server::split_shown(&app, terminal);
}

#[tauri::command(async)]
pub fn mark_unseen(id: String, app: Shared) {
    server::mark_unseen(&app, id);
}

#[tauri::command]
pub fn refresh_landed_tasks(app: Shared) {
    server::refresh_landed_tasks(&app);
}

/// Off the main thread: the read loop asks the window for its focus under the notices' lock, and that waits on the main thread.
#[tauri::command(async)]
pub fn sounds(on: bool, app: Shared) {
    server::sounds(&app, on);
}

// The Map view.

#[tauri::command(async)]
pub fn map_state(
    project: String,
    fetch: bool,
    app: Shared,
) -> Result<server::map::MapState, String> {
    server::map_state(&app, project, fetch)
}

#[tauri::command(async)]
pub fn map_know(project: String, part: String) -> Result<server::map::UserState, String> {
    server::map::map_know(project, part)
}

#[tauri::command(async)]
pub fn map_history(project: String, part: String) -> Result<Vec<server::history::Entry>, String> {
    server::map::map_history(project, part)
}

#[tauri::command(async)]
pub fn map_at(project: String, commit: String) -> Result<server::history::At, String> {
    server::map::map_at(project, commit)
}

#[tauri::command(async)]
pub fn map_commit(project: String, commit: String) -> Result<server::history::Commit, String> {
    server::map::map_commit(project, commit)
}

#[tauri::command(async)]
pub fn map_marks(projects: Vec<String>, app: Shared) -> Result<Vec<String>, String> {
    server::map_marks(&app, projects)
}

#[tauri::command(async)]
pub fn map_proof(project: String, law: String) -> Vec<server::ci_laws::Proof> {
    server::map::map_proof(project, law)
}

#[tauri::command(async)]
pub fn map_save_positions(
    project: String,
    positions: BTreeMap<String, [f64; 2]>,
) -> Result<server::map::UserState, String> {
    server::map::map_save_positions(project, positions)
}

#[tauri::command(async)]
pub fn map_set_tier(
    project: String,
    part: String,
    tier: Option<String>,
) -> Result<server::map::UserState, String> {
    server::map::map_set_tier(project, part, tier)
}

#[tauri::command(async)]
pub fn map_place(
    project: String,
    groups: BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    server::map::map_place(project, groups)
}

// The stop rules.

#[tauri::command(async)]
pub fn stop_log(
    kind: stop_rules::Kind,
    project: Option<String>,
    day: String,
) -> Result<(), String> {
    stop_rules::stop_log(kind, project, day)
}

#[tauri::command(async)]
pub fn stop_facts(project: String) -> Result<stop_rules::Facts, String> {
    stop_rules::stop_facts(project)
}

#[tauri::command(async)]
pub fn stop_dismiss(projects: Vec<String>, day: String) -> Result<(), String> {
    stop_rules::stop_dismiss(projects, day)
}

// Answering from the card.

#[tauri::command]
pub fn answer_blocked_command(
    session: String,
    nonce: String,
    decision: studio_hook::answer::Verdict,
) -> Result<server_answer::Answered, String> {
    server_answer::answer_blocked_command(session, nonce, decision)
}

#[tauri::command]
pub fn ask_again(session: String, command: String, cwd: String) -> Result<(), String> {
    server_answer::ask_again(session, command, cwd)
}

#[tauri::command]
pub fn watch(session: Option<String>, app: Shared) -> Result<Option<Value>, String> {
    server_answer::watch(session, &app.watching)
}

#[tauri::command]
pub fn alive(app: Shared) {
    server_answer::alive(&app.watching);
}

#[tauri::command]
pub fn answer(
    session: String,
    nonce: String,
    answers: Option<Value>,
    decision: Option<studio_hook::answer::Verdict>,
) -> Result<(), String> {
    server_answer::answer(session, nonce, answers, decision)
}
