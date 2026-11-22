//! The studio app's shell (danielrispler/Falinks#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line live and hands the core's card requests to the card writer. The
//! window lists every session (danielrispler/Falinks#369, decision 18) and shows the one Daniel selected: its cards, with
//! their quizzes, his guesses, his ratings and the longer explanations he asked for, which the card
//! store keeps on disk, and its check lines. A session that newly needs Daniel or reaches your turn,
//! a new card and a failed worker raise a macOS notification while the window is not focused; while
//! it is, a turn that ends off screen goes to the window's toast. The Dock badge counts the sessions
//! that need him (danielrispler/Falinks#369, decision 22; danielrispler/Falinks#517, decision 6).

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, LazyLock, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use studio_core::{
    CardRequest, Notifications, Output, Session, Sound,
    State::{Ended, NeedsYou},
    Studio, TaskFacts, TaskQuery, compute_turn_files,
};
use studio_understanding::{
    AGY_FIXTURE_VERSION, AgyVersions, Ask, CardStore, CardWriter, ClaudeWriter, Core, Effects,
    FakeWriter, Fingerprints, Offers, OpenItem, Rating, RiskNotes, Row, SessionCards,
    StoredSummary, SummarySetup, Tap, TurnWriting, Versions, diff_turn, fingerprint_changes,
    found_files, judge, live_roots, millis, open_items, turn_hunk, worktree_root,
    write_agent_summary, write_retests,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

mod answer;
mod presence;
mod restart;
mod settings;
mod skills;
mod terminal;

/// The card store, which the log thread adds to and the pane records guesses in.
type Cards = Arc<Mutex<CardStore>>;

/// The risk notes of blocked commands (danielrispler/plotkeeper#298), in a store of their own beside the card store: the log thread asks for
/// them, a thread of each judge keeps them, and the window's card read joins them to the cards.
type Notes = Arc<Mutex<RiskNotes>>;

/// The notices not shown yet, which the log thread and the card writer's threads add to.
type Notices = Mutex<Notifications>;

/// The settings' `summaries` (danielrispler/Falinks#517, decision 17): the writer's model and effort, and whether a
/// summary is written when Daniel opens its turn instead of at the turn's end.
#[tauri::command]
fn summary_settings(
    model: String,
    effort: String,
    on_open: bool,
    self_test: Option<u64>,
    writing: State<TurnWriting>,
    cards: State<Cards>,
) {
    cards.lock().unwrap().set_self_test(self_test);
    writing.settings(SummarySetup { model, effort }, on_open);
}

/// The settings' `checks` (danielrispler/plotkeeper#112): each project folder's list of commands that are checks.
#[tauri::command]
fn check_lists(lists: HashMap<String, Vec<String>>, studio: State<Core>) {
    studio.lock().unwrap().set_check_lists(lists);
    LIST_CHANGED.store(true, Ordering::Release);
}

/// A check run's exit code and output tail (danielrispler/plotkeeper#112), read from the event log through
/// its ledger row's offset, off the studio's lock. `None` when the session has no such call.
#[tauri::command(async)]
fn check_output(
    session: String,
    agent: Option<String>,
    call: String,
    studio: State<Core>,
) -> Result<Option<studio_core::CheckOutput>, String> {
    let row = {
        let studio = studio.lock().unwrap();
        let ledger = &studio.session(&session).map(|s| &s.ledger);
        ledger.and_then(|l| l.row(agent.as_deref(), &call)).cloned()
    };
    let Some(row) = row else { return Ok(None) };
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    studio_core::check_output(&row, &log)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The recorded facts of an Ask anchor (danielrispler/plotkeeper#125): from the ledger and the event log through its
/// offsets, with no writer in the call. `None` when the session or the thing is not known.
#[tauri::command(async)]
fn ask_facts(
    session: String,
    anchor: studio_core::Anchor,
    studio: State<Core>,
) -> Result<Option<studio_core::AskFacts>, String> {
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    Ok(studio.lock().unwrap().ask_facts(&session, &anchor, &log))
}

/// A file's hunk of turn `turn` (danielrispler/plotkeeper#114), whole for the diff pane (#131) when `all`: `git diff` between the turn's two trees of worktree
/// `root`, off the studio's lock. `None` when the turn has no trees there, or `git` cannot read them.
#[tauri::command(async)]
fn turn_file_hunk(
    session: String,
    turn: usize,
    root: String,
    path: String,
    from: Option<String>,
    all: Option<bool>,
    studio: State<Core>,
) -> Option<studio_understanding::Hunk> {
    let (start, end) = studio.lock().unwrap().turn_trees(&session, turn, &root)?;
    turn_hunk(
        std::path::Path::new(&root),
        &start,
        end.as_deref(),
        &path,
        from.as_deref(),
        all.unwrap_or(false),
    )
}

/// Daniel opened a hunk of turn `turn` (danielrispler/plotkeeper#130): the self-test counts the turns where he did.
#[tauri::command(async)]
fn hunk_opened(session: String, turn: usize, studio: State<Core>, cards: State<Cards>) {
    let id = (studio.lock().unwrap().session(&session)).and_then(|s| s.turns.get(turn)?.id.clone());
    if let Some(id) = id {
        unsaved(cards.lock().unwrap().quiz_tap(&id, Tap::Hunk, millis()));
    }
}

/// Daniel opened turn `id` (danielrispler/Falinks#526): its summary, written now when it waits for him, which takes
/// seconds, so the command runs off the main thread. `None` when the turn has none. A failed write
/// keeps the turn waiting and is an error, so the window asks again only at the next opening.
#[tauri::command(async)]
fn turn_summary(id: String, writing: State<TurnWriting>) -> Result<Option<StoredSummary>, String> {
    writing.opened(&id)
}

/// The whole final reply of turn `index` of session `id` (danielrispler/Falinks#574), which the rail fetches when Daniel
/// opens that turn's folded response; the session list never carries it.
#[tauri::command]
fn turn_reply(id: String, index: usize, studio: State<Core>) -> Option<String> {
    studio.lock().unwrap().turn_reply(&id, index)
}

/// Daniel opened subagent `agent_id` (danielrispler/Falinks#535): the summary of its purpose and result, written now
/// when it has none and has ended, which takes seconds, so the command runs off the main thread.
/// `None` while it runs or when the call failed, and the rail keeps its first line.
#[tauri::command(async)]
fn agent_summary(
    agent_id: String,
    studio: State<Core>,
    cards: State<Cards>,
    writing: State<TurnWriting>,
    writer: State<Writer>,
) -> Option<String> {
    let (prompt, message) = {
        let studio = studio.lock().unwrap();
        let mut agents = studio.sessions().into_iter().flat_map(|s| &s.agents);
        let agent = agents.find(|a| a.agent.id == agent_id && a.ended_at.is_some())?;
        (agent.prompt.clone(), agent.message.clone())
    };
    // A subagent that ended without a last message (killed, or its session ended) has no result to sum up.
    if message.is_empty() {
        return None;
    }
    let setup = writing.setup();
    write_agent_summary(
        &cards, &**writer, &setup, &agent_id, &prompt, &message, report,
    )
}

/// The session list, in its order.
#[tauri::command]
fn sessions(studio: State<Core>, cards: State<Cards>) -> Vec<Row> {
    let studio = studio.lock().unwrap();
    let now = studio.now();
    let list: Vec<Session> = studio.sessions().into_iter().cloned().collect();
    let landed: HashSet<String> = (list.iter())
        .filter(|s| studio.landed(s))
        .map(|s| s.agent.id.clone())
        .collect();
    drop(studio);
    let cards = cards.lock().unwrap();
    let row = |mut session: Session| Row {
        line: cards.line(&session, now),
        session: {
            session.landed = landed.contains(&session.agent.id);
            session
        },
    };
    list.into_iter().map(row).collect()
}

/// Daniel clicked session `id`'s row, or, with no `id`, went Home; returns the selection, which no
/// longer names a subagent, and is `null` at Home.
#[tauri::command]
fn select(id: Option<String>, studio: State<Core>) -> serde_json::Value {
    let mut studio = studio.lock().unwrap();
    match id {
        Some(id) => studio.select(&id),
        None => studio.select_none(),
    }
    selection(&studio)
}

/// Daniel started a session in studio terminal `terminal`; returns the selection, `null` until the
/// session's first event names that terminal and selects it (danielrispler/Falinks#567).
#[tauri::command]
fn select_terminal(terminal: String, studio: State<Core>) -> serde_json::Value {
    let mut studio = studio.lock().unwrap();
    studio.select_terminal(&terminal);
    selection(&studio)
}

/// Daniel opened subagent `agent_id`'s line; returns the selection, which names it. The card store
/// keeps the opening of a failed line, so it stays faded after a restart.
#[tauri::command]
fn open(agent_id: String, studio: State<Core>, cards: State<Cards>) -> serde_json::Value {
    let mut studio = studio.lock().unwrap();
    if studio.open(&agent_id) {
        unsaved(cards.lock().unwrap().opened(&agent_id));
    }
    selection(&studio)
}

/// The log of subagent `agent_id` of session `session` from byte `offset`: its entries after it and
/// the offset read to, `null` when there is no log (danielrispler/plotkeeper#54). Not pushed on events: the pane
/// asks when the selection changes, and reads on from the offset it kept.
#[tauri::command]
fn subagent_log(
    session: String,
    agent_id: String,
    offset: u64,
    studio: State<Core>,
) -> Option<studio_core::SubagentLog> {
    studio
        .lock()
        .unwrap()
        .subagent_log(&session, &agent_id, offset)
}

/// The session Daniel selected and the subagent of it he opened, or `null` before his first click.
#[tauri::command]
fn selected(studio: State<Core>) -> serde_json::Value {
    selection(&studio.lock().unwrap())
}

fn selection(studio: &Studio) -> serde_json::Value {
    serde_json::to_value(studio.selection()).expect("selection serializes")
}

/// The brief and cards of the selected session, or of the worker of it Daniel opened, split at the
/// latest tick, without each card's diff and base, which the pane never reads.
#[tauri::command]
fn cards(studio: State<Core>, cards: State<Cards>, notes: State<Notes>) -> SessionCards {
    let studio = studio.lock().unwrap();
    let Some(owner) = studio.viewed() else {
        return SessionCards::default();
    };
    let cards = cards.lock().unwrap();
    // A session's rail also holds its workers' cards, each with its turn; a worker's view is its own cards.
    let session = studio.session(owner);
    let mut shown = match session {
        Some(session) => cards.rail_cards(session, studio.now()),
        None => cards.session_cards(owner, studio.now()),
    };
    // The Open slot is the session's: a worker's view has none.
    if let Some(session) = session {
        shown.open = open_items(session, studio.task_facts(owner), &cards);
        // Each blocked command's card is joined to its note here, so the core's type holds none.
        shown.risk_notes = notes.lock().unwrap().views(session);
    }
    shown
}

/// Plan usage (danielrispler/plotkeeper#163; #165) of the `agents` asked for, or of every agent when `None`. The window asks for only
/// "claude" on its timer, which reads a snapshot file and runs no CLI; Codex and agy run theirs only when every agent is read, and
/// only if the CLI is on the login shell's `PATH`: an agent not installed has no entry. Claude's is from the status line's newest
/// snapshot in the `usage` folder beside the event log. An agent whose usage cannot be read says why.
#[tauri::command(async)]
fn usage(agents: Option<Vec<String>>) -> Vec<studio_workbench::AgentUsage> {
    let wanted = |agent: &str| {
        agents
            .as_ref()
            .is_none_or(|list| list.iter().any(|a| a == agent))
    };
    let mut read = Vec::new();
    if wanted("claude") {
        read.push(match std::env::home_dir() {
            Some(home) => {
                studio_workbench::claude_usage(&studio_hook::data_dir(&home).join("usage"))
            }
            None => studio_workbench::AgentUsage::failed(
                "claude",
                "No home folder to find Claude's usage in",
            ),
        });
    }
    let cli: Vec<&str> = ["codex", "agy"].into_iter().filter(|a| wanted(a)).collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if !cli.is_empty() {
        // The login shell's PATH, read once: a Dock-started app lacks it, and an npm-installed CLI needs it to find `node`.
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        let path = studio_workbench::login_shell_path(
            &shell.to_string_lossy(),
            std::time::Duration::from_secs(5),
        )
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
        let find = |program: &str| studio_workbench::find_in_path(program, &path);
        read.extend(studio_workbench::cli_usages(&cli, &find, Some(&path), now));
    }
    read
}

/// What the read loop last sent the window, so it sends each again only when it changes.
#[derive(Default)]
struct Sent {
    list: Option<serde_json::Value>,
    selected: Option<serde_json::Value>,
    badge: Option<usize>,
    cards: Option<CardsKey>,
}

/// What the cards were built for; the minute moves a card past 30 minutes, up to one minute late.
#[derive(PartialEq)]
struct CardsKey {
    viewed: Option<String>,
    revision: u64,
    minute: u64,
    open: Vec<OpenItem>,
    /// The turn each worker card is placed in, which a wake-up can move with no store change.
    placed: Vec<(String, Option<usize>)>,
    /// The risk notes' revision: a note arriving or failing changes no card.
    notes: u64,
}

/// The turns whose files the core asks for (danielrispler/plotkeeper#111): each is diffed by `git`
/// with the core unlocked, then handed back. Returns whether any was asked.
fn turn_files(state: &Mutex<Studio>) -> bool {
    /// At most this many turns a pass, so a launch over a long log does not hold the loop.
    const PER_PASS: usize = 8;
    let asks = state.lock().unwrap().turn_diffs(millis(), PER_PASS);
    for ask in &asks {
        let diffs = diff_turn(state, ask, &mut warn_log);
        let found = found_files(state, ask, diffs, compute_turn_files);
        state.lock().unwrap().store_turn_files(ask, found);
    }
    !asks.is_empty()
}

impl Sent {
    /// Sends the session list, the Dock badge and the selection, each only if it changed.
    fn list(&mut self, app: &AppHandle) {
        let list = sessions(app.state(), app.state());
        let needing = list.iter().filter(|r| r.session.wants_you()).count();
        // macOS shows `Some(0)` as a "0" badge.
        if self.badge != Some(needing)
            && let Some(window) = app.get_webview_window("main")
        {
            let _ = window.set_badge_count((needing > 0).then_some(needing as i64));
            self.badge = Some(needing);
        }
        let list = serde_json::to_value(list).expect("sessions serialize");
        if self.list.as_ref() != Some(&list) {
            let _ = app.emit("sessions", &list);
            self.list = Some(list);
        }
        let selected = selected(app.state());
        if self.selected.as_ref() != Some(&selected) {
            let _ = app.emit("selected", &selected);
            self.selected = Some(selected);
        }
    }

    /// Sends the cards when the viewed owner, the store's revision or the minute changed.
    fn cards(&mut self, app: &AppHandle) {
        // A card with buttons that is shown and has no note asks for one, before the key reads the notes' revision.
        let asks = {
            let studio = app.state::<Core>();
            let studio = studio.lock().unwrap();
            let shown = studio.viewed().and_then(|id| studio.session(id));
            shown.map(|session| app.state::<Notes>().lock().unwrap().asks(session))
        };
        ask_notes(app, asks.unwrap_or_default());
        let key = {
            let studio = app.state::<Core>();
            let studio = studio.lock().unwrap();
            let cards = app.state::<Cards>();
            let cards = cards.lock().unwrap();
            let viewed = studio.viewed();
            let session = viewed.and_then(|id| studio.session(id));
            // The Open slot moves with the core's state, not only with the store's.
            let open = session.map(|s| open_items(s, studio.task_facts(&s.agent.id), &cards));
            CardsKey {
                viewed: viewed.map(String::from),
                revision: cards.revision(),
                minute: studio.now() / 60_000,
                open: open.unwrap_or_default(),
                placed: session.map_or_default(|s| cards.placements(s)),
                notes: app.state::<Notes>().lock().unwrap().revision(),
            }
        };
        if self.cards.as_ref() != Some(&key) {
            let _ = app.emit("cards", cards(app.state(), app.state(), app.state()));
            self.cards = Some(key);
        }
    }
}

/// Starts the judge on each of `asks`, the risk notes a session wants (danielrispler/plotkeeper#298; `RiskNotes::asks`): a block that arrives
/// live, or a card with buttons that is shown and has none, once per command per run of the app. Each call runs on a thread of its
/// own, off the read loop and outside the turn-writing queue, as a side question does; the card shows with its buttons meanwhile.
fn ask_notes(app: &AppHandle, asks: Vec<Ask>) {
    for ask in asks {
        let notes: Notes = app.state::<Notes>().inner().clone();
        let writer: Writer = app.state::<Writer>().inner().clone();
        thread::spawn(move || judge(&notes, &*writer, &ask, millis(), report));
    }
}

// The four store commands run off the main thread, since saving the store fsyncs.

/// The pane drew the quizzes of cards `ids` in a visible window.
#[tauri::command(async)]
fn shown(ids: Vec<String>, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().show(&ids, millis()));
}

/// Daniel tapped option `option` of card `id`'s quiz, which on a check's card counts only before
/// the check's result.
#[tauri::command(async)]
fn guess(id: String, option: usize, studio: State<Core>, cards: State<Cards>) {
    let studio = studio.lock().unwrap();
    let mut store = cards.lock().unwrap();
    settle_checks(&studio, &mut store);
    unsaved(store.guess(&id, option, millis()));
}

/// Settles the guesses on the cards of the checks that ended (danielrispler/Falinks#461, decision 15). Takes the core's
/// lock before the store's, as the read loop does.
fn settle_checks(studio: &Studio, store: &mut CardStore) {
    for session in studio.sessions() {
        unsaved(store.settle_checks(session));
    }
}

/// Daniel picked, skipped or tapped a chip on the Quiz of `id`, a card's or a turn's own (danielrispler/plotkeeper#120): the turn's
/// summary with its taps, for a turn's own Quiz.
#[tauri::command(async)]
fn quiz_tap(id: String, tap: Tap, cards: State<Cards>) -> Option<StoredSummary> {
    let mut store = cards.lock().unwrap();
    unsaved(store.quiz_tap(&id, tap, millis()));
    store.turn_summary(&id).cloned()
}

/// What Home's learning slot serves at `now` (danielrispler/plotkeeper#240), given the start of Daniel's local day: the card store
/// chooses it, and reads and writes nothing.
#[tauri::command]
fn learning(
    now: u64,
    day_start: u64,
    cards: State<Cards>,
) -> Option<studio_understanding::Learning> {
    cards.lock().unwrap().learning(now, day_start)
}

/// Writes the retests that fell due, once an hour from a minute after the start, off the read loop; Home reads them
/// again when one was written (danielrispler/plotkeeper#129).
fn write_due_retests(app: &AppHandle) {
    thread::sleep(Duration::from_secs(60));
    loop {
        let (cards, writer) = (app.state::<Cards>(), app.state::<Writer>());
        if write_retests(&cards, &**writer, millis(), report) > 0 {
            let _ = app.emit("retests", ());
        }
        thread::sleep(Duration::from_secs(60 * 60));
    }
}

/// Daniel flagged that the key of card `id`'s quiz is wrong.
#[tauri::command(async)]
fn key_wrong(id: String, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().key_wrong(&id));
}

/// Daniel rated card `id`'s explanation.
#[tauri::command(async)]
fn rate(id: String, rating: Rating, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().rate(&id, rating));
}

/// Daniel asked card `id` to explain more: the kept text, or the writer's, which takes seconds, so the
/// command runs off the main thread. `None` when the writer gave none.
#[tauri::command(async)]
fn explain_more(id: String, cards: State<Cards>, writer: State<Writer>) -> Option<String> {
    studio_understanding::explain_more(&cards, &**writer, &id, report)
}

/// Daniel's side `question` on `item`, a response, card or intention of session `session` (danielrispler/Falinks#461,
/// decision 20): the writer's answer, read with the turn's intentions, or why there is none, which
/// the window shows with a retry. It takes seconds, so it runs off the main thread; nothing reaches
/// the session.
#[tauri::command(async)]
fn ask(
    session: String,
    item: String,
    question: String,
    studio: State<Core>,
    writer: State<Writer>,
) -> Result<String, String> {
    // The lock ends with the statement, before the writer runs.
    let transcript = studio
        .lock()
        .unwrap()
        .session(&session)
        .map(|s| s.transcript.clone());
    let answer = studio_understanding::ask_about(
        &**writer,
        &transcript.unwrap_or_default(),
        &item,
        &question,
    );
    if let Err(line) = &answer {
        log::error!("no answer on {session}: {}", scrubbed(line));
    }
    answer
}

/// Daniel's `question` on an Ask anchor (danielrispler/plotkeeper#126): the writer's answer, read with the anchor's recorded
/// facts and, for a file, its hunk. The facts stay above it in the window; this is the interpretation.
#[tauri::command(async)]
fn ask_anchor(
    session: String,
    anchor: studio_core::Anchor,
    what: String,
    question: String,
    studio: State<Core>,
    writer: State<Writer>,
) -> Result<String, String> {
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    let (facts, transcript, trees) = {
        let studio = studio.lock().unwrap();
        let trees = match &anchor {
            studio_core::Anchor::File { turn, root, .. } => {
                studio.turn_trees(&session, *turn, root)
            }
            _ => None,
        };
        let transcript = studio.session(&session).map(|s| s.transcript.clone());
        (studio.ask_facts(&session, &anchor, &log), transcript, trees)
    };
    let facts = facts.ok_or("nothing is recorded about this")?;
    let hunk = match (&anchor, trees) {
        (
            studio_core::Anchor::File {
                root, path, from, ..
            },
            Some((start, end)),
        ) => turn_hunk(
            std::path::Path::new(root),
            &start,
            end.as_deref(),
            path,
            from.as_deref(),
            false,
        )
        .filter(|h| !h.binary)
        .map(|h| {
            format!(
                "{}{}",
                h.text,
                if h.left_out > 0 {
                    format!("... {} more lines", h.left_out)
                } else {
                    String::new()
                }
            )
        }),
        _ => None,
    };
    let item = studio_understanding::ask_item(&what, &facts, hunk.as_deref());
    let answer = studio_understanding::ask_about(
        &**writer,
        &transcript.unwrap_or_default(),
        &item,
        &question,
    );
    if let Err(line) = &answer {
        log::error!("no answer on {session}: {}", scrubbed(line));
    }
    answer
}

/// "Why didn't it touch `name`?" in turn `turn` (danielrispler/plotkeeper#126): what was recorded about `name`, and the
/// writer's possible reasons only when something was.
#[tauri::command(async)]
fn ask_why_not(
    session: String,
    turn: usize,
    name: String,
    studio: State<Core>,
    writer: State<Writer>,
) -> Result<studio_understanding::WhyNot, String> {
    let (facts, transcript) = {
        let studio = studio.lock().unwrap();
        (
            studio.ask_named(&session, turn, &name),
            studio.session(&session).map(|s| s.transcript.clone()),
        )
    };
    let facts = facts.ok_or("this turn is not known")?;
    studio_understanding::why_not(&**writer, &transcript.unwrap_or_default(), &name, facts)
}

/// Daniel renamed session `id`: a `StudioRename` line in the event log, which the log thread then
/// applies and a restart replays (danielrispler/Falinks#469). An empty `name` gives the row its folder's again.
#[tauri::command(async)]
fn rename(id: String, name: String) -> Result<(), String> {
    append(&[serde_json::json!({
        "session_id": id,
        "hook_event_name": studio_core::RENAME,
        "studio_name": name.trim(),
        "studio_received_ms": millis(),
    })])
}

/// Daniel deleted session `id`: a `StudioDelete` line in the event log, which hides its row now and
/// after a restart.
#[tauri::command(async)]
fn delete_session(id: String) -> Result<(), String> {
    append(&[serde_json::json!({
        "session_id": id,
        "hook_event_name": studio_core::DELETE,
        "studio_received_ms": millis(),
    })])
}

/// The landed sessions Daniel cleared whose undo window is still open (danielrispler/plotkeeper#244): the window tells the shell,
/// as `(session, terminal)`, so a quit inside the window can commit them.
#[derive(Default)]
struct Clearing(Mutex<Vec<Cleared>>);

/// A cleared session and the terminal it runs in.
#[derive(serde::Deserialize)]
struct Cleared {
    id: String,
    terminal: Option<String>,
}

/// Replaces the cleared sessions pending their delete; an empty list after Undo or after the window's commit.
#[tauri::command]
fn clearing(sessions: Vec<Cleared>, pending: State<Clearing>) {
    *pending.0.lock().unwrap() = sessions;
}

/// What committing the cleared `sessions` sends: the daemon's `kill` and `forget` of each terminal, and a `StudioDelete`
/// line for each session, as Delete does. The cards stay in the store.
/// `ends` gives the lines that end the sessions of a terminal, as closing its pane does (`closed`), which come first.
fn clear_commit(
    sessions: &[Cleared],
    in_use: impl Fn(&str, &[Cleared]) -> bool,
    ends: impl Fn(&str) -> Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut requests = Vec::new();
    let mut lines = Vec::new();
    let mut ended = std::collections::HashSet::new();
    for terminal in sessions.iter().filter_map(|s| s.terminal.as_ref()) {
        // Each terminal once, and never one a session that is not being cleared still uses.
        if in_use(terminal, sessions) || !ended.insert(terminal) {
            continue;
        }
        requests.push(serde_json::json!({ "op": "kill", "id": terminal }));
        requests.push(serde_json::json!({ "op": "forget", "id": terminal }));
        lines.extend(ends(terminal));
    }
    lines.extend(sessions.iter().map(|s| {
        serde_json::json!({
            "session_id": s.id,
            "hook_event_name": studio_core::DELETE,
            "studio_received_ms": millis(),
        })
    }));
    (requests, lines)
}

/// The cleared `pending` sessions as they are now, from `current(id)`, which gives whether the session is still landed and the terminal
/// it runs in now: a session that is gone or no longer landed is dropped, and each keeps its current terminal.
fn resolve_clearing(
    pending: Vec<Cleared>,
    current: impl Fn(&str) -> Option<(bool, Option<String>)>,
) -> Vec<Cleared> {
    (pending.into_iter())
        .filter_map(|cleared| {
            let (landed, terminal) = current(&cleared.id)?;
            landed.then_some(Cleared {
                id: cleared.id,
                terminal,
            })
        })
        .collect()
}

/// The app is quitting: the sessions cleared inside their undo window are deleted now (#244, story 35).
fn commit_clearing(app: &AppHandle) {
    let pending = std::mem::take(&mut *app.state::<Clearing>().0.lock().unwrap());
    let studio = app.state::<Core>();
    // The window's list may be stale: what is landed and where it runs is read again from the studio now.
    let sessions = {
        let studio = studio.lock().unwrap();
        let all = studio.sessions();
        resolve_clearing(pending, |id| {
            let session = all.iter().find(|s| s.agent.id == id)?;
            Some((studio.landed(session), session.terminal.clone()))
        })
    };
    let in_use = |terminal: &str, cleared: &[Cleared]| {
        let studio = studio.lock().unwrap();
        (studio.sessions().into_iter()).any(|s| {
            s.terminal.as_deref() == Some(terminal)
                && s.state != Ended
                && !s.status_only
                && !cleared.iter().any(|c| c.id == s.agent.id)
        })
    };
    let (requests, lines) = clear_commit(&sessions, in_use, |terminal| {
        session_ends(&studio, terminal)
    });
    for request in requests {
        let _ = terminal::terminal_send(request, app.state());
    }
    if !lines.is_empty()
        && let Err(e) = append(&lines)
    {
        log::warn!("commit the cleared sessions: {e}");
    }
}

/// Daniel closed terminal `terminal` (danielrispler/Falinks#461, story 15 and decision 8): a `SessionEnd` line in the event
/// log for each session in it that has not ended, as its `claude` may die before its own hook runs.
/// A real `SessionEnd` after it changes nothing.
#[tauri::command(async)]
fn closed(terminal: String, studio: State<Core>) -> Result<(), String> {
    append(&session_ends(&studio, &terminal))
}

/// The `SessionEnd` lines for the sessions of `terminal` that have not ended.
fn session_ends(studio: &Mutex<Studio>, terminal: &str) -> Vec<serde_json::Value> {
    studio
        .lock()
        .unwrap()
        .sessions()
        .into_iter()
        .filter(|s| s.terminal.as_deref() == Some(terminal) && s.state != Ended)
        .filter(|s| !s.status_only)
        .map(|s| {
            serde_json::json!({
                "session_id": s.resume_id,
                "hook_event_name": "SessionEnd",
                "reason": "studio_close",
                "studio_received_ms": millis(),
            })
        })
        .collect()
}

/// The worktree roots file beside the event log `log` (danielrispler/plotkeeper#132).
fn roots_file(log: &Path) -> PathBuf {
    log.with_file_name("roots.json")
}

/// The worktree root of each folder a launch before looked up, none when there is no file or it
/// does not read.
// shortcut: a root is kept for good, so a folder looked up before its own worktree existed keeps its
// parent's root; check the saved ones with `git` if a session ever shows under the wrong worktree.
fn saved_roots(log: &Path) -> HashMap<String, String> {
    let saved = std::fs::read(roots_file(log)).ok();
    let saved: HashMap<String, String> = saved
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    // An entry that maps a removed worktree's folder to another root is dropped, and the file
    // saved without it (danielrispler/plotkeeper#205).
    let live = live_roots(saved.clone());
    if live.len() != saved.len() {
        save_roots(log, &live);
    }
    live
}

/// Takes the state a launch before saved, when it fits the log, and returns the offset to replay the
/// log from: 0 when there is none (danielrispler/plotkeeper#134).
fn resume(studio: &Mutex<Studio>, log: &Path) -> u64 {
    let Ok(saved) = std::fs::read(studio_hook::state_file(log)) else {
        return 0;
    };
    let len = std::fs::metadata(log).map_or(0, |m| m.len());
    studio.lock().unwrap().resume(&saved, len)
}

/// Keeps the replayed state beside the log, through a new file that takes the old one's place.
fn save_state(log: &Path, state: &[u8]) {
    let file = studio_hook::state_file(log);
    let next = file.with_extension("json.next");
    let saved = std::fs::write(&next, state).and_then(|()| std::fs::rename(&next, &file));
    if let Err(e) = saved {
        log::error!("keep the replayed state: {e}");
    }
}

/// Keeps `roots` beside the log, through a new file that takes the old one's place.
fn save_roots(log: &Path, roots: &HashMap<String, String>) {
    let next = roots_file(log).with_extension("json.next");
    let written = serde_json::to_vec(roots).map_err(io::Error::from);
    let saved = written.and_then(|bytes| std::fs::write(&next, bytes));
    if let Err(e) = saved.and_then(|()| std::fs::rename(&next, roots_file(log))) {
        log::error!("keep the worktree roots: {e}");
    }
}

/// Appends `lines` to the event log in one write under the lock the hook takes, so they never
/// interleave with a hook's line.
fn append(lines: &[serde_json::Value]) -> Result<(), String> {
    let home = std::env::home_dir().ok_or("no home directory")?;
    let locked = studio_hook::locked(&studio_hook::event_log(&home), Duration::from_secs(60));
    let mut log = locked
        .map_err(|e| e.to_string())?
        .ok_or("the event log stayed locked")?;
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    log.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

/// The daemon's `list` (danielrispler/Falinks#560): the core's status-only rows and mismatch lines follow its terminals'
/// OSC 7501 status. A row's notices go out as a Claude session's do.
fn listed(app: &AppHandle, terms: Vec<studio_core::Terminal>) {
    let outputs = app
        .state::<Core>()
        .lock()
        .unwrap()
        .terminals(&terms, millis());
    for output in outputs {
        match output {
            Output::NeedsYou { session_id, agent } => {
                tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
            }
            Output::YourTurn { session_id } => tell(app, Notice::YourTurn, &session_id, None),
            Output::Mismatch { line } => mismatch(&line),
            Output::Interrupted {
                session_id,
                prompt_id,
            } => {
                thread::spawn(move || interrupted(&session_id, &prompt_id));
            }
            _ => {}
        }
    }
    LIST_CHANGED.store(true, Ordering::Release);
}

/// Daniel interrupted turn `prompt_id` of session `session_id`, which fires no `Stop`: a
/// `StudioInterrupted` line in the event log, with the turn's end trees as the hook records them,
/// which the log thread then applies and a restart replays.
fn interrupted(session_id: &str, prompt_id: &str) {
    let mut line = serde_json::Map::new();
    line.insert("session_id".into(), session_id.into());
    line.insert("hook_event_name".into(), studio_core::INTERRUPTED.into());
    line.insert("prompt_id".into(), prompt_id.into());
    line.insert("studio_received_ms".into(), millis().into());
    let appended = std::env::home_dir()
        .ok_or("no home directory".to_owned())
        .and_then(|home| {
            let log = studio_hook::event_log(&home);
            let data = log.parent().ok_or("no data directory")?;
            // The line goes in the log even when the trees cannot be recorded, as the hook's does.
            if let Err(e) = studio_hook::trees(&mut line, data) {
                log::warn!("record the trees of {session_id}'s interrupted turn: {e}");
            }
            append(&[line.into()])
        });
    if let Err(e) = appended {
        log::error!("end {session_id}'s interrupted turn: {e}");
    }
}

/// Logs `line` at `info` under the `mismatch` target, for debugging (danielrispler/Falinks#560).
fn mismatch(line: &str) {
    log::info!(target: "mismatch", "{line}");
}

/// The Studio log (danielrispler/plotkeeper#188): `studio.log` in the app's log folder, beside stderr, 500 KB a file with
/// one older file kept, in local time. The studio's own crates log at `Debug` and libraries at `Warn`, in every build.
fn studio_log() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
    let mut log = tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets([
            Target::new(TargetKind::Stderr).format(stamped),
            Target::new(TargetKind::LogDir {
                file_name: Some("studio".into()),
            })
            .format(stamped),
            // Errors also show in the window's notice line (#191), as the bare message; warnings stay in the file.
            Target::new(TargetKind::Webview).filter(shows_in_window),
        ])
        .max_file_size(500_000)
        .rotation_strategy(RotationStrategy::KeepSome(1))
        .timezone_strategy(TimezoneStrategy::UseLocal)
        // The Builder's own format is the bare message: a target adds the prefix its output wants (the window's does not).
        .clear_format()
        .level(log::LevelFilter::Warn);
    for own in [
        "studio",
        "studio_core",
        "studio_hook",
        "studio_understanding",
        "studio_terminal_daemon",
        "studio_workbench",
        "mismatch",
    ] {
        log = log.level_for(own, log::LevelFilter::Debug);
    }
    log.build()
}

/// A line of `stderr` and `studio.log`: `[date][time][LEVEL][target] message`, the plugin's own shape, in local time.
fn log_line(at: [u32; 6], level: log::Level, target: &str, message: &str) -> String {
    let [year, month, day, hour, minute, second] = at;
    format!(
        "[{year:04}-{month:02}-{day:02}][{hour:02}:{minute:02}:{second:02}][{level}][{target}] {message}"
    )
}

/// `log_line` for a record now, in local time.
fn stamped(
    out: tauri_plugin_log::fern::FormatCallback,
    message: &std::fmt::Arguments,
    record: &log::Record,
) {
    let now = tauri_plugin_log::TimezoneStrategy::UseLocal.get_now();
    let at = [
        now.year() as u32,
        u8::from(now.month()).into(),
        now.day().into(),
        now.hour().into(),
        now.minute().into(),
        now.second().into(),
    ];
    out.finish(format_args!(
        "{}",
        log_line(at, record.level(), record.target(), &message.to_string())
    ));
}

/// Whether a record reaches the window's notice line: an `error` that did not come from the UI, which showed its own.
fn shows_in_window(metadata: &log::Metadata) -> bool {
    metadata.level() == log::Level::Error
        && !metadata
            .target()
            .starts_with(tauri_plugin_log::WEBVIEW_TARGET)
}

/// A panic is logged at `error` with its location, then reaches the default panic output.
fn keep_panics_in_the_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        log::error!("panic{at}: {}", panic_message(info.payload()));
        previous(info);
    }));
}

/// A panic's message when it is text, scrubbed like a writer's line: an `unwrap` on an `Err` can carry what the code was working on.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("not text");
    scrubbed(text)
}

/// Opens Finder with the Studio log selected.
#[tauri::command]
fn reveal_log(app: AppHandle) -> Result<(), String> {
    let file = app
        .path()
        .app_log_dir()
        .map_err(|e| e.to_string())?
        .join("studio.log");
    Command::new("open")
        .arg("-R")
        .arg(file)
        .status()
        .map_err(|e| e.to_string())?
        .success()
        .then_some(())
        .ok_or_else(|| "open -R failed".to_string())
}

/// What of a line from the writers' code may reach the Studio log: the line up to the first quote mark, so a reply's
/// words never do, whether quoted after a colon or inside a parse error (`invalid type: string "<reply>"`).
fn scrubbed(line: &str) -> &str {
    line.split('"')
        .next()
        .unwrap_or(line)
        .trim_end_matches([':', ' '])
}

/// Whether a line from the writers' code reports lost data (a save that failed), not a degraded step. Every such line of
/// the `understanding` crate starts `save `; a test reads their sources to keep it so.
fn lost_data(line: &str) -> bool {
    line.starts_with("save ")
}

/// Logs a line from the writers' code: an error when it lost data, a warning otherwise.
fn report(line: String) {
    let line = scrubbed(&line);
    if lost_data(line) {
        log::error!("{line}");
    } else {
        log::warn!("{line}");
    }
}

/// Logs a card store change that could not be saved; it stays in memory until the next save.
fn unsaved(saved: io::Result<()>) {
    if let Err(e) = saved {
        log::error!("save the card store: {e}");
    }
}

/// The card writer: `claude -p`, or the fake when `STUDIO_CARD_WRITER=fake`, for checks that must
/// not run `claude -p`.
type Writer = Arc<dyn CardWriter + Send + Sync>;

/// Whether a navigation stays on the app's own pages (danielrispler/Falinks#525, decision 21): a backstop, as a rendered
/// link opens in the browser through `open_link` and never navigates the window.
fn in_app(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

fn main() {
    uptime();
    hover_delay();
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    // The store sits beside the log, in the studio's data directory.
    let store = CardStore::open_beside(&log).expect("read the card store");
    let store: Cards = Arc::new(Mutex::new(store));
    let notes = RiskNotes::open_beside(&log).expect("read the risk notes");
    let notes: Notes = Arc::new(Mutex::new(notes));
    let mut studio = Studio::with_roots(worktree_root);
    studio.know_roots(saved_roots(&log));
    let core: Core = Arc::new(Mutex::new(studio));
    // The window does not exist yet, so the writer's notices wait here for the setup to forward them.
    let (writer_notices, writer_notice_reader) = mpsc::channel::<Option<String>>();
    let writer: Writer = match std::env::var("STUDIO_CARD_WRITER").as_deref() {
        Ok("fake") => Arc::new(FakeWriter),
        _ => {
            let writer = ClaudeWriter::beside(&log).expect("create the card writer's folder");
            let writer_notices = Mutex::new(writer_notices);
            Arc::new(writer.on_availability(move |notice| {
                let _ = writer_notices.lock().unwrap().send(notice);
            }))
        }
    };
    let effects = Arc::new(ShellEffects::default());
    let writing = TurnWriting::new(
        core.clone(),
        store.clone(),
        writer.clone(),
        offers(),
        effects.clone(),
    );
    keep_panics_in_the_log();
    tauri::Builder::default()
        .plugin(studio_log())
        // First, so a second instance exits before it sets anything up (danielrispler/Falinks#470).
        .plugin(tauri_plugin_single_instance::init(restart::second_instance))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri::plugin::Builder::<_, ()>::new("navigation")
                .on_navigation(|_, url| in_app(url))
                .build(),
        )
        .manage(core)
        .manage(store)
        .manage(notes)
        .manage(Notices::default())
        .manage(Clearing::default())
        // A reload loses the window's undo timer and its hidden rows, which it shows again: nothing is pending a commit then.
        .on_page_load(|webview, payload| {
            if let tauri::webview::PageLoadEvent::Started = payload.event() {
                webview.state::<Clearing>().0.lock().unwrap().clear();
            }
        })
        .manage(writer.clone())
        .manage(writing)
        .manage(terminal::Daemon::default())
        .manage(Mutex::new(answer::Watching::default()))
        .invoke_handler(tauri::generate_handler![
            sessions,
            select,
            select_terminal,
            open,
            selected,
            subagent_log,
            cards,
            shown,
            guess,
            key_wrong,
            quiz_tap,
            learning,
            rate,
            explain_more,
            ask,
            rename,
            delete_session,
            clearing,
            closed,
            usage,
            terminal::terminal_connect,
            terminal::terminal_send,
            terminal::open_link,
            terminal::open_file,
            skills::skills,
            terminal::projects,
            terminal::choose_folder,
            terminal::add_project,
            terminal::remove_project,
            terminal::reorder_projects,
            terminal::suggest_projects,
            terminal::repository_of,
            terminal::workspaces,
            terminal::workspace_git_state,
            terminal::fetch_repository,
            terminal::pull_main,
            terminal::claude_argv,
            terminal::agent_argv,
            terminal::agents,
            terminal::plugin_setup,
            terminal::fix_plugin_setup,
            terminal::agy_plugin_setup,
            terminal::fix_agy_plugin_setup,
            terminal::create_task,
            terminal::create_named_task,
            terminal::remove_task,
            terminal::delete_merged_branch,
            terminal::branches,
            terminal::switch_branch,
            terminal::terminal_lost,
            terminal::terminal_upgrade,
            restart::revival,
            restart::restart_into,
            answer::watch,
            answer::alive,
            answer::answer,
            answer::answer_blocked_command,
            answer::ask_again,
            settings::settings_files,
            settings::set_zoom,
            settings::open_settings,
            settings::open_keymap,
            settings::write_setting,
            summary_settings,
            hunk_opened,
            check_lists,
            check_output,
            turn_file_hunk,
            ask_facts,
            ask_anchor,
            ask_why_not,
            turn_summary,
            turn_reply,
            agent_summary,
            split_shown,
            mark_unseen,
            rail_shown,
            refresh_landed_tasks,
            reveal_log,
            sounds
        ])
        .on_window_event(|window, event| {
            if let Some(path) = presence::path() {
                presence::window_event(&path, event);
            }
            // Leaving full screen, tao queues making its own view first responder, a view wry took
            // out of the window, so no ⌘ key reached the page (#23). The resize that follows
            // queues the webview's focus after it.
            if let tauri::WindowEvent::Resized(_) = event {
                let webviews = window.webviews();
                dispatch2::DispatchQueue::main().exec_async(move || {
                    for webview in webviews {
                        let _ = webview.set_focus();
                    }
                });
            }
        })
        .setup(move |app| {
            log::info!("start, version {}", app.package_info().version);
            if let Some(path) = presence::path() {
                presence::start(&path);
            }
            let app = app.handle().clone();
            let _ = effects.0.set(app.clone());
            restart::watch_build(app.clone());
            settings::watch(app.clone());
            let prs_app = app.clone();
            thread::spawn(move || prs(&prs_app));
            let retest_app = app.clone();
            thread::spawn(move || write_due_retests(&retest_app));
            let notice_app = app.clone();
            thread::spawn(move || {
                for notice in writer_notice_reader {
                    // The notice is the writer's own words; the log keeps only that one arrived or cleared.
                    log::warn!(
                        "card writer notice {}",
                        if notice.is_some() { "shown" } else { "cleared" }
                    );
                    let _ = notice_app.emit("writer-notice", notice);
                }
            });
            thread::spawn(move || follow(&app, &log));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("build the studio")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commit_clearing(app);
            }
            if let Some(path) = presence::path() {
                presence::run_event(&path, &event);
            }
        });
}

/// Reads the log from its start, then from where it stopped. A line counts once it ends in a newline,
/// so a line the hook is still writing waits for the next read. The lines there at start are
/// replayed, so steps that ran while the app was down get no cards and no notifications, but for the
/// turns of the last 24 hours with no card, which get one on the hook's trees (danielrispler/Falinks#486). Live lines
/// get the worktree fingerprint the core asks for; replayed ones get none. Each pass ends with a
/// tick, the first right after the replay.
// shortcut: polls every 200 ms; switch to file-system events if the delay or the wake-ups matter.
fn follow(app: &AppHandle, log: &Path) {
    // Before the replay, so it reads none of what the prune moves (danielrispler/plotkeeper#132).
    match studio_hook::prune(log, millis()) {
        Ok(pruned) if pruned != studio_hook::Pruned::default() => log::info!(
            "pruned the event log: {} lines archived, {} lightened",
            pruned.archived,
            pruned.lightened
        ),
        Ok(_) => {}
        Err(e) => log::warn!("prune the event log: {e}"),
    }
    // The saved state, when there is one that fits the log, takes the replay from where it stopped
    // (danielrispler/plotkeeper#134).
    let mut read_to = resume(&app.state::<Core>(), log);
    let mut roots_saved = 0;
    let mut live = false;
    // Each session's and each subagent's own, so another's prompt or step in the same worktree moves
    // no base.
    let mut fingerprints = HashMap::<String, Fingerprints>::new();
    let writing = app.state::<TurnWriting>();
    let versions = Versions::beside(log);
    let agy_versions = AgyVersions::beside(log);
    let mut sent = Sent::default();
    loop {
        let state = app.state::<Core>();
        let mut read = false;
        if let Ok(mut file) = File::open(log)
            && file.seek(SeekFrom::Start(read_to)).is_ok()
        {
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            let mut outputs = Vec::new();
            let read_from = read_to;
            // The core is locked per line, and not while the fingerprint runs `git`, so the pane's
            // commands do not wait on it.
            while reader.read_until(b'\n', &mut line).unwrap_or(0) > 0 && line.ends_with(b"\n") {
                read_to += line.len() as u64;
                let text = String::from_utf8_lossy(&line);
                if live {
                    // shortcut: fingerprints when the line is read, up to one poll after the prompt,
                    // so an edit in between counts as Daniel's; take the fingerprint in the hook if a
                    // card ever misses one.
                    let looks = state.lock().unwrap().fingerprint_folders(&text);
                    let changes = fingerprint_changes(looks, &mut fingerprints);
                    outputs.extend(state.lock().unwrap().apply_with_changes(&text, changes));
                } else {
                    state.lock().unwrap().replay(&text);
                }
                line.clear();
            }
            // A worktree's first fingerprint only records it, so the turns still open after the
            // replay get their base now, and their `Stop` has something to compare with. The list
            // goes first, so Daniel sees it while `git` runs.
            if !live {
                let mut studio = state.lock().unwrap();
                // What the replay made, before the card store's facts and the backfill change it.
                let replayed = studio.save();
                let store = app.state::<Cards>();
                let store = store.lock().unwrap();
                store.restore(&mut studio);
                // The turns that ended while the app was down get their cards now (danielrispler/Falinks#486).
                let requests: Vec<CardRequest> =
                    store.cards().iter().map(|k| k.request.clone()).collect();
                let backfill = studio.backfill(millis(), &requests);
                drop((store, studio));
                let log = log.to_path_buf();
                thread::spawn(move || save_state(&log, &replayed));
                state.lock().unwrap().tick(millis());
                sent.list(app);
                // The terminals' revival waits only for the replay, not for this pass's `git` work, which
                // can take a minute on a long log, while a click's pane waits on it (#182).
                restart::REPLAYED.store(true, std::sync::atomic::Ordering::Release);
                let waiting = state.lock().unwrap().waiting_folders();
                fingerprint_changes(waiting, &mut fingerprints);
                writing.backfill(backfill);
            }
            for output in outputs {
                match output {
                    Output::Card(request) => writing.card(request, true),
                    Output::Brief(request) => writing.brief(request),
                    Output::Summary(request) => writing.summary(request),
                    Output::Name(request) => writing.name(request),
                    Output::NeedsYou { session_id, agent } => {
                        tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
                    }
                    Output::YourTurn { session_id } => {
                        refresh_landed(app, Some(session_id.clone()));
                        tell(app, Notice::YourTurn, &session_id, None);
                    }
                    Output::CommandBlocked { session_id, agent } => {
                        // The card's buttons are live now; its note is asked for beside the notification, which waits for nothing.
                        let asks = {
                            let studio = state.lock().unwrap();
                            let blocked = studio.session(&session_id);
                            blocked
                                .map(|session| app.state::<Notes>().lock().unwrap().asks(session))
                        };
                        ask_notes(app, asks.unwrap_or_default());
                        tell(
                            app,
                            Notice::Titled("Auto mode blocked a command"),
                            &session_id,
                            agent.as_deref(),
                        );
                    }
                    Output::Failed { session_id, agent } => {
                        tell(
                            app,
                            Notice::Titled("Worker failed"),
                            &session_id,
                            Some(&agent),
                        );
                    }
                    Output::Promoted { agent } => {
                        unsaved(app.state::<Cards>().lock().unwrap().promoted(&agent));
                    }
                    Output::Unrecognised {
                        session_id,
                        task_id,
                    } => log::warn!(
                        "unrecognised wake-up: a prompt of session {session_id} names \
                         task {task_id} in markup the studio does not know"
                    ),
                    Output::Mismatch { line } => mismatch(&line),
                    Output::Interrupted {
                        session_id,
                        prompt_id,
                    } => {
                        thread::spawn(move || interrupted(&session_id, &prompt_id));
                    }
                    Output::AgyVersion => match agy_version() {
                        Some(version) => {
                            match agy_versions.check(&version, AGY_FIXTURE_VERSION, millis()) {
                                Ok(Some(notice)) => {
                                    log::warn!("{notice}");
                                    let _ = app.emit("notice", &notice);
                                }
                                Ok(None) => {}
                                Err(e) => log::warn!("keep agy's version: {e}"),
                            }
                        }
                        None => log::warn!("agy's version: `agy --version` did not run"),
                    },
                    // Reads the transcript's lines and nothing else (`AGENTS.md`).
                    Output::Version { transcript_path } => {
                        match versions.check(Path::new(&transcript_path), millis()) {
                            Ok(Some(notice)) => {
                                log::warn!("{notice}");
                                let _ = app.emit("notice", &notice);
                            }
                            Ok(None) => {}
                            Err(e) => log::warn!("keep Claude Code's version: {e}"),
                        }
                    }
                }
            }
            read = read_to > read_from;
            if read {
                let roots = state.lock().unwrap().known_roots();
                if roots.len() != roots_saved {
                    roots_saved = roots.len();
                    save_roots(log, &roots);
                }
                let keys = state.lock().unwrap().live_keys();
                fingerprints.retain(|key, _| keys.contains(key));
                // The decision history goes to the store, which keeps it across restarts (danielrispler/Falinks#479).
                let studio = state.lock().unwrap();
                let mut store = app.state::<Cards>().inner().lock().unwrap();
                for session in studio.sessions() {
                    unsaved(store.keep_decisions(&session.agent.id, &session.decisions));
                }
                settle_checks(&studio, &mut store);
            }
        }
        // Reads the transcripts' lines and nothing else (`AGENTS.md`).
        let transcripts_changed = state.lock().unwrap().follow_transcripts(report);
        // Each live session's context, from its status line snapshot (danielrispler/Falinks#517, decision 1; danielrispler/Falinks#519); the
        // snapshot is named by the conversation's id, which a `/clear` changes.
        let running: Vec<_> = (state.lock().unwrap().sessions().into_iter())
            .filter(|s| s.state != Ended && !s.status_only)
            .map(|s| (s.agent.id.clone(), s.resume_id.clone()))
            .collect();
        let home = std::env::home_dir().unwrap_or_default();
        let usage = studio_hook::data_dir(&home).join("usage");
        let windows_changed = running.into_iter().fold(false, |changed, (id, resume_id)| {
            let snapshot = studio_workbench::context_window(&usage, &resume_id);
            state.lock().unwrap().set_snapshot(&id, snapshot) | changed
        });
        let list_changed = LIST_CHANGED.swap(false, Ordering::AcqRel);
        let files_changed = turn_files(&state);
        let changed =
            read || transcripts_changed || windows_changed || list_changed || files_changed;
        // The open request goes in the same pass as the list that shows "needs you" (danielrispler/Falinks#569).
        if let Ok(data) = answer::data()
            && let Some(ask) = app
                .state::<Mutex<answer::Watching>>()
                .lock()
                .unwrap()
                .pass(&data, studio_hook::answer::now_ms())
        {
            let _ = app.emit("ask", ask);
        }
        if let Ok(data) = answer::data()
            && let Some(blocked) = app
                .state::<Mutex<answer::Watching>>()
                .lock()
                .unwrap()
                .changed_blocked_requests(&data)
        {
            let _ = app.emit("blocked", blocked);
        }
        // Asking for focus waits on the main thread, which never takes this lock.
        let focused = || {
            let window = app.get_webview_window("main");
            window.and_then(|w| w.is_focused().ok()) == Some(true)
        };
        // Daniel sees the selected session in a focused window while the story rail shows it (danielrispler/Falinks#556);
        // the window is asked only while it is unseen, and not under the core's lock, which the main
        // thread's commands take.
        let unseen = state.lock().unwrap().selected().is_some_and(|s| s.unseen);
        let shown = unseen && RAIL_SHOWN.load(Ordering::Acquire) && focused();
        let seen = state.lock().unwrap().see_selected(shown);
        if let Some((id, turns)) = &seen {
            unsaved(app.state::<Cards>().lock().unwrap().seen(id, *turns));
        }
        if state.lock().unwrap().tick(millis()) || changed || seen.is_some() {
            sent.list(app);
        }
        // Takes the core's lock under the notices' lock; `tell` never holds both.
        let needs_you = |id: &str| {
            let studio = state.lock().unwrap();
            studio.session(id).is_some_and(|s| s.state == NeedsYou)
        };
        let due = app
            .state::<Notices>()
            .lock()
            .unwrap()
            .due(uptime(), focused, needs_you, || {
                let studio = state.lock().unwrap();
                let selected = studio.selected().map(|s| s.agent.id.clone());
                let mut on_screen: Vec<String> = selected.into_iter().collect();
                // The split's pane shows a terminal: a status-only row's id, or a session's in it.
                if let Some(terminal) = SPLIT_SHOWN.lock().unwrap().clone() {
                    let in_it = studio.sessions().into_iter();
                    let in_it = in_it.filter(|s| s.terminal.as_ref() == Some(&terminal));
                    on_screen.extend(in_it.map(|s| s.agent.id.clone()));
                    on_screen.push(terminal);
                }
                on_screen
            });
        if let Some((title, body)) = due.notification {
            let _ = app.notification().builder().title(title).body(body).show();
        }
        if let Some(sound) = due.sound {
            play(sound);
        }
        if !due.finished.is_empty() {
            let _ = app.emit("finished", &due.finished);
        }
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        sent.cards(app);
        live = true;
        // Also when the log did not open, so its replay block never ran.
        restart::REPLAYED.store(true, std::sync::atomic::Ordering::Release);
        thread::sleep(Duration::from_millis(200));
    }
}

/// The list changed outside the read loop since it last sent it: a PR was found, or Daniel marked a
/// session unseen.
static LIST_CHANGED: AtomicBool = AtomicBool::new(false);

/// Whether the story rail shows, as the UI last said: not in focus mode, behind a closed overlay or
/// under the skills browser, when a turn on it is not seen (danielrispler/Falinks#556).
static RAIL_SHOWN: AtomicBool = AtomicBool::new(true);

/// The UI's rail showed or hid.
#[tauri::command]
fn rail_shown(shown: bool) {
    RAIL_SHOWN.store(shown, Ordering::Release);
}

/// The terminal the split's second pane shows, as the UI last said, `None` with no split: its
/// session is on screen, so its turn's end raises no toast.
static SPLIT_SHOWN: Mutex<Option<String>> = Mutex::new(None);

/// The UI's split opened, closed or pinned another terminal.
#[tauri::command]
fn split_shown(terminal: Option<String>) {
    *SPLIT_SHOWN.lock().unwrap() = terminal;
}

/// agy's version, from `agy --version` run in Daniel's login shell, so a Dock-started app finds it
/// on his `PATH`; `None` when agy is missing or fails.
fn agy_version() -> Option<String> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let out = Command::new(shell)
        .args(["-lc", "agy --version"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = text.lines().last()?.trim();
    (out.status.success() && !version.is_empty()).then(|| version.to_owned())
}

/// The `state` of the pull request or issue that `gh <args>` shows, run in `cwd`: `MERGED`, `CLOSED`
/// and the like; `None` when `gh` fails, is missing or is offline.
fn gh_state(gh: &str, cwd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(gh)
        .args(args)
        .args(["--json", "state", "--jq", ".state"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// What git and GitHub say about `task`, for **Landed**; every fact that cannot be found is false.
fn lookup_task(gh: &str, task: &TaskQuery) -> TaskFacts {
    let safe = studio_workbench::losses(Path::new(&task.path))
        .is_ok_and(|l| l.uncommitted.is_empty() && l.unpushed.is_empty());
    let merged = task.branch.as_deref().is_some_and(|branch| {
        gh_state(gh, &task.path, &["pr", "view", branch]).as_deref() == Some("MERGED")
    });
    let issue_closed = task.issue.is_some_and(|n| {
        gh_state(gh, &task.path, &["issue", "view", &n.to_string()]).as_deref() == Some("CLOSED")
    });
    TaskFacts {
        safe,
        merged,
        issue_closed,
    }
}

/// Looks up each task of the list, or only the one that holds `session`, on its own thread so the
/// read loop never waits on the network. Asked at a turn's end, at the window's focus and on the
/// palette's refresh, never on a timer.
fn refresh_landed(app: &AppHandle, session: Option<String>) {
    let app = app.clone();
    thread::spawn(move || {
        let studio = app.state::<Core>();
        let tasks = {
            let mut studio = studio.lock().unwrap();
            studio.forget_gone_tasks();
            let mut tasks = studio.task_queries();
            if let Some(session) = session {
                let path = studio.task_of(&session);
                tasks.retain(|t| Some(&t.path) == path.as_ref());
            }
            tasks
        };
        let gh = studio_workbench::gh_path();
        for task in tasks {
            // A removed worktree has no facts.
            let facts = Path::new(&task.path)
                .exists()
                .then(|| lookup_task(gh, &task));
            let changed = studio.lock().unwrap().set_task_facts(&task.path, facts);
            if changed {
                LIST_CHANGED.store(true, Ordering::Release);
            }
        }
    });
}

/// The window gained focus, or Daniel chose the palette's refresh: ask about every task again.
#[tauri::command]
fn refresh_landed_tasks(app: AppHandle) {
    refresh_landed(&app, None);
}

/// Finds the PR of each session's branch with `gh pr view` (danielrispler/Falinks#461, decision 19), on its own thread so
/// the read loop never waits on the network, and asks again after 5 minutes. `gh` runs as Daniel
/// with its own login; no credential file is read.
fn prs(app: &AppHandle) {
    let gh = studio_workbench::gh_path();
    let mut asked = HashMap::<String, Instant>::new();
    loop {
        let branches = app.state::<Core>().lock().unwrap().branches();
        for (cwd, branch) in branches {
            if asked
                .get(&branch)
                .is_some_and(|at| at.elapsed() < Duration::from_secs(300))
            {
                continue;
            }
            asked.insert(branch.clone(), Instant::now());
            let output = Command::new(gh)
                .args(["pr", "view", &branch, "--json", "number", "--jq", ".number"])
                .current_dir(&cwd)
                .output();
            let output = output.ok().filter(|o| o.status.success());
            let pr = output.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
            if app.state::<Core>().lock().unwrap().set_pr(&branch, pr) {
                LIST_CHANGED.store(true, Ordering::Release);
            }
        }
        thread::sleep(Duration::from_secs(5));
    }
}

/// The shell's part of Turn writing: what only it can do. It holds the app once the window's setup has it.
#[derive(Default)]
struct ShellEffects(OnceLock<AppHandle>);

impl Effects for ShellEffects {
    fn list(&self) {
        if let Some(app) = self.0.get() {
            let _ = app.emit("sessions", sessions(app.state(), app.state()));
        }
    }

    fn new_card(&self, session_id: &str, agent: Option<&str>) {
        if let Some(app) = self.0.get() {
            tell(app, Notice::Titled("New card"), session_id, agent);
        }
    }

    fn settle_checks(&self) {
        let Some(app) = self.0.get() else { return };
        let (studio, store) = (app.state::<Core>(), app.state::<Cards>());
        settle_checks(&studio.lock().unwrap(), &mut store.lock().unwrap());
    }

    fn name(&self, session_id: &str, name: &str) {
        let line = serde_json::json!({
            "session_id": session_id,
            "hook_event_name": studio_core::PROMPT_NAME,
            "studio_name": name,
            "studio_received_ms": millis(),
        });
        if let Err(e) = append(&[line]) {
            log::error!("save the name of {session_id}: {e}");
        }
    }

    fn log(&self, line: String) {
        report(line);
    }
}

/// The warning for an oversized turn, which the understanding layer hands its callers, goes to the Studio log.
fn warn_log(warning: String) {
    log::warn!("{warning}");
}

/// The record of offers, beside the event log.
fn offers() -> Offers {
    let home = std::env::home_dir().unwrap_or_default();
    Offers::beside(&studio_hook::event_log(&home))
}

/// Adds a notice on session `session_id`'s line, naming its subagent `agent`, if any; the read loop
/// shows it. Called without the core's lock held.
fn tell(app: &AppHandle, notice: Notice, session_id: &str, agent: Option<&str>) {
    let body = app
        .state::<Core>()
        .lock()
        .unwrap()
        .notice_body(session_id, agent);
    let notices = app.state::<Notices>();
    let mut notices = notices.lock().unwrap();
    match notice {
        Notice::Titled(title) => notices.push(title, body, uptime()),
        Notice::NeedsYou => notices.push_needs_you(session_id, body, uptime()),
        Notice::YourTurn => notices.push_your_turn(session_id, body, uptime()),
    }
}

/// What a notice says: that a session needs you, which shows only if it still needs Daniel when
/// it is due, or its own title.
enum Notice<'a> {
    NeedsYou,
    YourTurn,
    Titled(&'a str),
}

/// Daniel marked session `id` unseen (danielrispler/Falinks#556), from the rail's menu.
#[tauri::command(async)]
fn mark_unseen(id: String, studio: State<Core>, cards: State<Cards>) {
    if let Some((id, turns)) = studio.lock().unwrap().mark_unseen(&id) {
        unsaved(cards.lock().unwrap().seen(&id, turns));
        LIST_CHANGED.store(true, Ordering::Release);
    }
}

/// The `sounds` setting (danielrispler/Falinks#556), from `settings.json`, which the window parses.
/// Off the main thread: the read loop asks the window for its focus under the notices' lock, and
/// that waits on the main thread.
#[tauri::command(async)]
fn sounds(on: bool, notices: State<Notices>) {
    notices.lock().unwrap().sounds = on;
}

/// Plays a macOS system sound, so the studio bundles none (danielrispler/Falinks#556).
fn play(sound: Sound) {
    let name = match sound {
        Sound::NeedsYou => "Glass",
        Sound::YourTurn => "Tink",
    };
    let file = format!("/System/Library/Sounds/{name}.aiff");
    // Waited on its own thread, so no finished `afplay` is left a zombie.
    thread::spawn(move || {
        if let Err(e) = Command::new("afplay").arg(file).status() {
            log::warn!("play the {name} sound: {e}");
        }
    });
}

// A hover's text shows after half a second, not macOS's 1.5 s (#220). WKWebView's `title` tooltips are AppKit's, which read
// NSInitialToolTipDelay; it is registered, not written, so `defaults write dev.falinks.studio NSInitialToolTipDelay` still wins.
fn hover_delay() {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSNumber, NSUserDefaults, ns_string};
    let delay = NSNumber::new_i32(500);
    let defaults = NSDictionary::<_, AnyObject>::from_slices(
        &[ns_string!("NSInitialToolTipDelay")],
        &[&*delay],
    );
    // SAFETY: the dictionary maps a string key to an NSNumber, the types registerDefaults expects.
    unsafe { NSUserDefaults::standardUserDefaults().registerDefaults(&defaults) };
}

/// Time since the app started, a monotonic clock for combining notices (danielrispler/Falinks#434, decision 28).
fn uptime() -> Duration {
    static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
    STARTED.elapsed()
}

#[cfg(test)]
mod tests {
    use super::{in_app, log_line, lost_data, saved_roots, scrubbed, shows_in_window};

    #[test]
    fn a_quit_inside_the_clear_window_kills_each_terminal_and_deletes_each_session() {
        let cleared = |id: &str, terminal: Option<&str>| super::Cleared {
            id: id.into(),
            terminal: terminal.map(Into::into),
        };
        // The sessions that run in terminal t1 and have not ended: the same lines the window's close of the pane writes.
        let ends = |terminal: &str| {
            vec![serde_json::json!({
                "session_id": format!("resume-{terminal}"),
                "hook_event_name": "SessionEnd",
            })]
        };
        let (requests, lines) = super::clear_commit(
            &[cleared("a", Some("t1")), cleared("b", None)],
            |_, _| false,
            ends,
        );
        assert_eq!(
            requests,
            vec![
                serde_json::json!({ "op": "kill", "id": "t1" }),
                serde_json::json!({ "op": "forget", "id": "t1" }),
            ]
        );
        let deleted: Vec<_> = lines
            .iter()
            .map(|l| {
                (
                    l["session_id"].as_str().unwrap(),
                    l["hook_event_name"].as_str().unwrap(),
                )
            })
            .collect();
        // Its sessions end first, as Delete's closing of the pane does, then each cleared session is deleted.
        assert_eq!(
            deleted,
            [
                ("resume-t1", "SessionEnd"),
                ("a", studio_core::DELETE),
                ("b", studio_core::DELETE)
            ]
        );
        assert!(
            super::clear_commit(&[], |_, _| false, |_| vec![])
                .1
                .is_empty()
        );
    }

    fn cleared(id: &str, terminal: Option<&str>) -> super::Cleared {
        super::Cleared {
            id: id.into(),
            terminal: terminal.map(Into::into),
        }
    }

    #[test]
    fn two_cleared_sessions_in_one_terminal_end_it_once() {
        let ends = |t: &str| vec![serde_json::json!({ "session_id": format!("r-{t}") })];
        let (requests, lines) = super::clear_commit(
            &[cleared("a", Some("t1")), cleared("b", Some("t1"))],
            |_, _| false,
            ends,
        );
        assert_eq!(requests.len(), 2);
        // One session end, then the two deletes.
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn a_terminal_a_session_not_being_cleared_uses_is_not_killed_but_its_cleared_session_is_still_deleted()
     {
        let ends = |t: &str| vec![serde_json::json!({ "session_id": format!("r-{t}") })];
        let (requests, lines) = super::clear_commit(
            &[cleared("a", Some("t1")), cleared("b", Some("t2"))],
            |t, _| t == "t1",
            ends,
        );
        assert_eq!(
            requests,
            vec![
                serde_json::json!({ "op": "kill", "id": "t2" }),
                serde_json::json!({ "op": "forget", "id": "t2" }),
            ]
        );
        let ids: Vec<_> = lines
            .iter()
            .map(|l| l["session_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["r-t2", "a", "b"]);
    }

    #[test]
    fn a_quit_commits_only_sessions_still_landed_each_in_the_terminal_it_has_now() {
        let current = |id: &str| match id {
            "landed" => Some((true, Some("t9".to_string()))),
            "moved on" => Some((false, Some("t2".to_string()))),
            _ => None,
        };
        let pending = vec![
            cleared("landed", None),
            cleared("moved on", Some("t2")),
            cleared("gone", Some("t3")),
        ];
        let kept = super::resolve_clearing(pending, current);
        assert_eq!(kept.len(), 1);
        assert_eq!(
            (kept[0].id.as_str(), kept[0].terminal.as_deref()),
            ("landed", Some("t9"))
        );
    }

    #[test]
    fn a_saved_root_of_a_removed_worktrees_folder_is_dropped_and_the_file_saved_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let root = root.to_str().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=S", "-c", "user.email=s@example.com"])
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        git(&["commit", "-q", "--allow-empty", "-m", "base"]);
        let gone = format!("{root}/worktrees/w1");
        git(&["worktree", "add", "-q", "-b", "w1", &gone]);
        git(&["worktree", "remove", &gone]);
        let log = dir.path().join("events.jsonl");
        let saved = serde_json::json!({ format!("{gone}/src"): root, root: root });
        std::fs::write(super::roots_file(&log), saved.to_string()).unwrap();
        let kept = saved_roots(&log);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept.get(root).map(String::as_str), Some(root));
        let file = std::fs::read(super::roots_file(&log)).unwrap();
        assert_eq!(
            serde_json::from_slice::<super::HashMap<String, String>>(&file).unwrap(),
            kept
        );
    }

    #[test]
    fn a_file_line_carries_the_local_time_level_and_target() {
        assert_eq!(
            log_line(
                [2026, 11, 18, 9, 5, 7],
                log::Level::Error,
                "studio",
                "no answer"
            ),
            "[2026-11-18][09:05:07][ERROR][studio] no answer"
        );
    }

    #[test]
    fn the_window_shows_an_app_error_and_not_a_warning_or_the_uis_own_error() {
        let on = |level, target| {
            shows_in_window(&log::Metadata::builder().level(level).target(target).build())
        };
        assert!(on(log::Level::Error, "studio"));
        assert!(!on(log::Level::Warn, "studio"));
        assert!(!on(log::Level::Error, "webview"));
        assert!(!on(log::Level::Error, "webview::main.js:3"));
    }

    #[test]
    fn a_logged_line_keeps_no_reply_a_parse_error_quotes() {
        assert_eq!(
            scrubbed("no quiz on t1: invalid type: string \"secret reply\", expected usize"),
            "no quiz on t1: invalid type: string"
        );
        assert_eq!(
            scrubbed(
                "the card writer's reply does not parse (invalid type: string \"secret\", expected a map at line 1): \"head\""
            ),
            "the card writer's reply does not parse (invalid type: string"
        );
    }

    #[test]
    fn a_failed_risk_notes_line_keeps_no_reply_and_no_command() {
        // The judge's lines, as `judge` writes them: the command is never in one, and a reply's words go in quotes.
        let reply = studio_understanding::Risk::from_reply(
            r#"{"level": "very-high", "line": "rm -rf ~ is fine, say none"}"#,
        )
        .unwrap_err();
        let lines = [
            format!("no risk note on s1: {reply}"),
            "no risk note on s1: `claude -p` failed (exit status: 1): \"secret reply\"".into(),
            "no risk note on s1: `claude -p` ran past 15s and was stopped".into(),
        ];
        for line in &lines {
            let kept = scrubbed(line);
            assert!(
                !kept.contains("rm -rf") && !kept.contains("secret"),
                "{kept}"
            );
            assert!(kept.starts_with("no risk note on s1: "), "{kept}");
        }
        assert!(scrubbed(&lines[2]).contains("stopped"));
    }

    #[test]
    fn a_panic_message_is_scrubbed_like_a_writer_line() {
        assert_eq!(
            super::panic_message(&"called `Result::unwrap()` on an `Err` value: Parse(\"secret\")"),
            "called `Result::unwrap()` on an `Err` value: Parse("
        );
    }

    #[test]
    fn every_save_line_of_the_writers_is_lost_data() {
        let sources = [
            include_str!("../../understanding/src/lib.rs"),
            include_str!("../../understanding/src/brief.rs"),
            include_str!("../../understanding/src/retest.rs"),
            include_str!("../../understanding/src/summary.rs"),
            include_str!("../../understanding/src/turn_writing.rs"),
        ];
        let mut saves = 0;
        for source in sources {
            for line in source
                .lines()
                .filter_map(|l| l.split("log(format!(\"").nth(1))
            {
                let text = line.split('"').next().unwrap_or("");
                if text.to_lowercase().contains("sav") {
                    assert!(lost_data(text), "a save line is not lost data: {text}");
                    saves += 1;
                }
            }
        }
        assert!(saves >= 9, "found only {saves} save lines");
    }

    #[test]
    fn a_logged_line_keeps_no_quoted_reply() {
        assert_eq!(
            scrubbed("no summary on t1: `claude -p` failed (exit status: 1): \"secret card text\""),
            "no summary on t1: `claude -p` failed (exit status: 1)"
        );
        assert_eq!(
            scrubbed("no session name: the reply holds none"),
            "no session name: the reply holds none"
        );
    }

    #[test]
    fn only_a_failed_save_is_lost_data() {
        assert!(lost_data("save the card store: disk full"));
        assert!(lost_data("save the summaries of s1: disk full"));
        assert!(!lost_data("no summary on t1: the writer is away"));
    }

    #[test]
    fn the_window_stays_on_the_app() {
        let url = |s: &str| tauri::Url::parse(s).unwrap();
        assert!(in_app(&url("tauri://localhost/index.html")));
        assert!(in_app(&url("http://tauri.localhost/index.html")));
        assert!(!in_app(&url("https://example.com/")));
        assert!(!in_app(&url("http://tauri.localhost.example.com/")));
        assert!(!in_app(&url("file:///etc/passwd")));
        assert!(!in_app(&url("javascript:alert(1)")));
    }
}
