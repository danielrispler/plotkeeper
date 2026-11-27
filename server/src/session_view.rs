//! The session view's commands: the list, the selection, the subagent log, the brief and its risk notes (spec #720).

use super::*;

// The session view's commands.

/// The session list, in its order.
pub fn sessions(app: &App) -> Vec<Row> {
    let studio = app.core();
    let now = studio.now();
    let list: Vec<Session> = studio.sessions().into_iter().cloned().collect();
    let landed: HashSet<String> = (list.iter())
        .filter(|s| studio.landed(s))
        .map(|s| s.agent.id.clone())
        .collect();
    drop(studio);
    let cards = app.cards();
    let row = |mut session: Session| Row {
        line: cards.line(&session, now),
        session: {
            session.landed = landed.contains(&session.agent.id);
            session
        },
    };
    list.into_iter().map(row).collect()
}

/// Daniel clicked session `id`'s row, or, with no `id`, went back to the bare map; returns the selection,
/// which no longer names a subagent, and is `null` with none selected.
pub fn select(app: &App, id: Option<String>) -> serde_json::Value {
    let mut studio = app.core();
    match id {
        Some(id) => studio.select(&id),
        None => studio.select_none(),
    }
    let shown = selection(&studio);
    drop(studio);
    shown
}

/// Daniel opened subagent `agent_id`'s line; returns the selection, which names it. The card store
/// keeps the opening of a failed line, so it stays faded after a restart.
pub fn open(app: &App, agent_id: String) -> serde_json::Value {
    let mut studio = app.core();
    if studio.open(&agent_id) {
        unsaved(app.cards().opened(&agent_id));
    }
    let shown = selection(&studio);
    drop(studio);
    shown
}

/// The log of subagent `agent_id` of session `session` from byte `offset`: its entries after it and
/// the offset read to, `null` when there is no log (danielrispler/plotkeeper#54). Not pushed on events: the pane
/// asks when the selection changes, and reads on from the offset it kept.
pub fn subagent_log(
    app: &App,
    session: String,
    agent_id: String,
    offset: u64,
) -> Option<studio_core::SubagentLog> {
    subagent_log_through(
        app,
        session,
        agent_id,
        offset,
        studio_core::SubagentLogFile::read,
    )
}

/// `subagent_log` with the file read by `read`, which runs with no lock on the studio held (spec
/// danielrispler/plotkeeper#548): the lock is held only to copy where the log is. The seam its test reads through.
pub fn subagent_log_through(
    app: &App,
    session: String,
    agent_id: String,
    offset: u64,
    read: impl FnOnce(&studio_core::SubagentLogFile) -> Option<studio_core::SubagentLog>,
) -> Option<studio_core::SubagentLog> {
    let file = {
        let studio = app.core();
        studio.subagent_log_file(&session, &agent_id, offset)?
    };
    read(&file)
}

/// The session Daniel selected and the subagent of it he opened, or `null` before his first click.
pub fn selected(app: &App) -> serde_json::Value {
    selection(&app.core())
}

fn selection(studio: &Studio) -> serde_json::Value {
    serde_json::to_value(studio.selection()).expect("selection serializes")
}

/// Daniel renamed session `id`: a `StudioRename` line in the event log, which the log thread then
/// applies and a restart replays (danielrispler/Falinks#469). An empty `name` gives the row its folder's again.
pub fn rename(id: String, name: String) -> Result<(), String> {
    append(&[serde_json::json!({
        "session_id": id,
        "hook_event_name": studio_core::RENAME,
        "studio_name": name.trim(),
        "studio_received_ms": millis(),
    })])
}

/// Daniel deleted session `id`: a `StudioDelete` line in the event log, which hides its row now and
/// after a restart.
pub fn delete_session(id: String) -> Result<(), String> {
    append(&[serde_json::json!({
        "session_id": id,
        "hook_event_name": studio_core::DELETE,
        "studio_received_ms": millis(),
    })])
}

/// How often the follower checks each session's agent process (#330).
pub const PROCESS_CHECK: Duration = Duration::from_secs(30);

/// The `SessionEnd` lines for the sessions whose agent process `alive` no longer lists (#330, spec #319,
/// "Session ending"). A session with no pid is never listed.
pub fn process_ends(studio: &Mutex<Studio>, alive: impl Fn(u32) -> bool) -> Vec<serde_json::Value> {
    let gone = studio.lock().unwrap().gone(alive);
    gone.into_iter()
        .map(|id| {
            serde_json::json!({
                "session_id": id,
                "hook_event_name": "SessionEnd",
                "reason": "process_gone",
                "studio_received_ms": millis(),
            })
        })
        .collect()
}

/// Daniel marked session `id` unseen (danielrispler/Falinks#556), from the rail's menu.
pub fn mark_unseen(app: &App, id: String) {
    let marked = app.core().mark_unseen(&id);
    if let Some((id, turns)) = marked {
        unsaved(app.cards().seen(&id, turns));
        app.list_changed();
    }
}

/// The UI's rail showed or hid.
pub fn rail_shown(app: &App, shown: bool) {
    app.rail_shown.store(shown, Ordering::Release);
}

// The session's brief and its risk notes.

/// The brief, decision history and risk notes of the selected session, or of the worker of it Daniel opened.
pub fn cards(app: &App) -> SessionCards {
    let (studio, cards) = app.core_and_cards();
    let Some(owner) = studio.viewed() else {
        return SessionCards::default();
    };
    let session = studio.session(owner);
    let mut shown = cards.session_cards(owner);
    // The Open slot is the session's: a worker's view has none.
    if let Some(session) = session {
        shown.open = open_items(session, studio.task_facts(owner), &cards);
    }
    drop(cards);
    // Each blocked command's note is joined here, so the core's type holds none.
    join_notes(&studio, &app.notes(), &mut shown);
    drop(studio);
    shown
}

/// The session whose blocked commands the window docks: the selected one, also while Daniel views a worker of it, whose
/// own id is no session's.
pub fn noted_session(studio: &Studio) -> Option<&Session> {
    studio.selected()
}

/// The risk notes the window shows for its session's cards and its decision history, which stay empty with no session.
fn risk_views(studio: &Studio, notes: &RiskNotes) -> (Vec<RiskView>, Vec<RiskView>) {
    noted_session(studio).map_or_default(|s| (notes.views(s), notes.decision_views(s)))
}

/// Joins the cards of the session the window shows to their risk notes, in `shown`.
pub fn join_notes(studio: &Studio, notes: &RiskNotes, shown: &mut SessionCards) {
    (shown.risk_notes, shown.decision_notes) = risk_views(studio, notes);
}

/// What the session's brief was built for.
#[derive(PartialEq)]
pub struct CardsKey {
    viewed: Option<String>,
    revision: u64,
    open: Vec<OpenItem>,
    /// What the window shows of the risk notes: a note arriving or failing changes nothing stored.
    notes: (Vec<RiskView>, Vec<RiskView>),
}

/// The key of the cards the window would be sent now.
pub fn cards_key(studio: &Studio, cards: &CardStore, notes: &RiskNotes) -> CardsKey {
    let viewed = studio.viewed();
    let session = viewed.and_then(|id| studio.session(id));
    // The Open slot moves with the core's state, not only with the store's.
    let open = session.map(|s| open_items(s, studio.task_facts(&s.agent.id), cards));
    CardsKey {
        viewed: viewed.map(String::from),
        revision: cards.revision(),
        open: open.unwrap_or_default(),
        notes: risk_views(studio, notes),
    }
}

/// The key of the cards the window would be sent now, from the state's own locks.
pub fn current_cards_key(app: &App) -> CardsKey {
    let (studio, cards, notes) = app.core_cards_and_notes();
    cards_key(&studio, &cards, &notes)
}
