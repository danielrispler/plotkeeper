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
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use studio_core::{
    CardRequest, Facts, NameRequest, Notifications, Output, RootDiff, Session, Sound,
    State::{Ended, NeedsYou},
    Studio, SummaryRequest, TaskFacts, TaskQuery, TurnDiff, TurnFiles, TurnFilesSnapshot,
    compute_turn_files,
};
use studio_understanding::{
    AGY_FIXTURE_VERSION, AgyVersions, CardStore, CardWriter, Changed, ClaudeWriter, Decision,
    FakeWriter, Fingerprints, Offer, Offers, OpenItem, Rating, Row, SessionCards, StoredSummary,
    SummarySetup, Tap, TurnFacts, Versions, add_hunks, backfilled, changed_in, fingerprint_changes,
    live_roots, millis, offer_stop, offer_summary, open_items, settle, turn_diffs, turn_hunk,
    turns_card, worktree_root, write_agent_summary, write_design, write_goal, write_name,
    write_retests,
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
type Cards = Mutex<CardStore>;

/// The notices not shown yet, which the log thread and the card writer's threads add to.
type Notices = Mutex<Notifications>;

/// The summary writer's setup, from the settings the window sends (danielrispler/Falinks#526), and the turns whose
/// summary waits for Daniel to open it while summaries are written on open, by turn id.
// shortcut: the waiting turns live in memory, so a restart loses them; keep them in the store if
// that bites.
#[derive(Default)]
struct Summaries {
    setup: Mutex<SummarySetup>,
    waiting: Mutex<HashMap<String, SummaryRequest>>,
    /// The files of each turn card's own change, for a summary of an agent that reports no trees: it is
    /// judged on the same change as its card (danielrispler/plotkeeper#209).
    changes: Mutex<HashMap<String, Vec<Changed>>>,
    /// What the card path decided for each live turn card, by turn id, until the turn's summary takes it:
    /// one decision for a turn (danielrispler/plotkeeper#209), however late its summary runs.
    decided: Mutex<HashMap<String, Decision>>,
    /// The turns whose decision was a lone Quiz, by turn id, until their summary puts it on record.
    lone: Mutex<HashMap<String, Decision>>,
}

/// The settings' `summaries` (danielrispler/Falinks#517, decision 17): the writer's model and effort, and whether a
/// summary is written when Daniel opens its turn instead of at the turn's end.
#[tauri::command]
fn summary_settings(
    model: String,
    effort: String,
    on_open: bool,
    self_test: Option<u64>,
    summaries: State<Summaries>,
    studio: State<Mutex<Studio>>,
    cards: State<Cards>,
) {
    cards.lock().unwrap().set_self_test(self_test);
    *summaries.setup.lock().unwrap() = SummarySetup { model, effort };
    studio.lock().unwrap().set_summaries_on_open(on_open);
}

/// The settings' `checks` (danielrispler/plotkeeper#112): each project folder's list of commands that are checks.
#[tauri::command]
fn check_lists(lists: HashMap<String, Vec<String>>, studio: State<Mutex<Studio>>) {
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
    studio: State<Mutex<Studio>>,
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
    studio: State<Mutex<Studio>>,
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
    studio: State<Mutex<Studio>>,
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
fn hunk_opened(session: String, turn: usize, studio: State<Mutex<Studio>>, cards: State<Cards>) {
    let id = (studio.lock().unwrap().session(&session)).and_then(|s| s.turns.get(turn)?.id.clone());
    if let Some(id) = id {
        unsaved(cards.lock().unwrap().quiz_tap(&id, Tap::Hunk, millis()));
    }
}

/// Daniel opened turn `id` (danielrispler/Falinks#526): its summary, written now when it waits for him, which takes
/// seconds, so the command runs off the main thread. `None` when the turn has none. A failed write
/// keeps the turn waiting and is an error, so the window asks again only at the next opening.
#[tauri::command(async)]
fn turn_summary(
    id: String,
    cards: State<Cards>,
    studio: State<Mutex<Studio>>,
    summaries: State<Summaries>,
    writer: State<Writer>,
) -> Result<Option<StoredSummary>, String> {
    if let Some(kept) = cards.lock().unwrap().turn_summary(&id) {
        return Ok(Some(kept.clone()));
    }
    let Some(request) = summaries.waiting.lock().unwrap().remove(&id) else {
        return Ok(None);
    };
    let setup = summaries.setup.lock().unwrap().clone();
    let lone = summaries.lone.lock().unwrap().remove(&id);
    if offer_summary(
        &cards,
        &**writer,
        &setup,
        request.clone(),
        lone.as_ref(),
        &offers(),
        report,
    )
    .is_none()
    {
        if let Some(lone) = lone {
            summaries.lone.lock().unwrap().insert(id.clone(), lone);
        }
        summaries
            .waiting
            .lock()
            .unwrap()
            .insert(id.clone(), request);
        return Err(format!("no summary on {id}"));
    }
    design(&studio, &cards, &**writer, &request.session_id);
    Ok(cards.lock().unwrap().turn_summary(&id).cloned())
}

/// Writes "Design so far" of session `id` when its sources changed (danielrispler/plotkeeper#123). Returns whether it did.
fn design(
    studio: &Mutex<Studio>,
    cards: &Cards,
    writer: &(impl CardWriter + ?Sized),
    id: &str,
) -> bool {
    let turns: Vec<String> = match studio.lock().unwrap().session(id) {
        Some(session) => session.turns.iter().filter_map(|t| t.id.clone()).collect(),
        None => return false,
    };
    write_design(cards, writer, id, &turns, report)
}

/// The whole final reply of turn `index` of session `id` (danielrispler/Falinks#574), which the rail fetches when Daniel
/// opens that turn's folded response; the session list never carries it.
#[tauri::command]
fn turn_reply(id: String, index: usize, studio: State<Mutex<Studio>>) -> Option<String> {
    studio.lock().unwrap().turn_reply(&id, index)
}

/// Daniel opened subagent `agent_id` (danielrispler/Falinks#535): the summary of its purpose and result, written now
/// when it has none and has ended, which takes seconds, so the command runs off the main thread.
/// `None` while it runs or when the call failed, and the rail keeps its first line.
#[tauri::command(async)]
fn agent_summary(
    agent_id: String,
    studio: State<Mutex<Studio>>,
    cards: State<Cards>,
    summaries: State<Summaries>,
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
    let setup = summaries.setup.lock().unwrap().clone();
    write_agent_summary(
        &cards, &**writer, &setup, &agent_id, &prompt, &message, report,
    )
}

/// The session list, in its order.
#[tauri::command]
fn sessions(studio: State<Mutex<Studio>>, cards: State<Cards>) -> Vec<Row> {
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
fn select(id: Option<String>, studio: State<Mutex<Studio>>) -> serde_json::Value {
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
fn select_terminal(terminal: String, studio: State<Mutex<Studio>>) -> serde_json::Value {
    let mut studio = studio.lock().unwrap();
    studio.select_terminal(&terminal);
    selection(&studio)
}

/// Daniel opened subagent `agent_id`'s line; returns the selection, which names it. The card store
/// keeps the opening of a failed line, so it stays faded after a restart.
#[tauri::command]
fn open(agent_id: String, studio: State<Mutex<Studio>>, cards: State<Cards>) -> serde_json::Value {
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
    studio: State<Mutex<Studio>>,
) -> Option<studio_core::SubagentLog> {
    studio
        .lock()
        .unwrap()
        .subagent_log(&session, &agent_id, offset)
}

/// The session Daniel selected and the subagent of it he opened, or `null` before his first click.
#[tauri::command]
fn selected(studio: State<Mutex<Studio>>) -> serde_json::Value {
    selection(&studio.lock().unwrap())
}

fn selection(studio: &Studio) -> serde_json::Value {
    serde_json::to_value(studio.selection()).expect("selection serializes")
}

/// The brief and cards of the selected session, or of the worker of it Daniel opened, split at the
/// latest tick, without each card's diff and base, which the pane never reads.
#[tauri::command]
fn cards(studio: State<Mutex<Studio>>, cards: State<Cards>) -> SessionCards {
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
}

/// `ask`'s worktrees diffed by `git` with the core unlocked. A turn skipped for holding too many files is warned of in
/// the Studio log and settled, so no pass asks for it again.
fn diff_turn(state: &Mutex<Studio>, ask: &TurnDiff) -> Option<Vec<RootDiff>> {
    let mut skipped = false;
    let diffs = turn_diffs(ask, &mut |warning| {
        log::warn!("{warning}");
        skipped = true;
    });
    if skipped && diffs.is_none() {
        state.lock().unwrap().settle_turn_files(ask);
    }
    diffs
}

/// The files of `ask` among `diffs`, found by `compute` with the core unlocked: the core is locked only to
/// copy out what the matching reads. `None` when git could not diff the turn.
fn found_files(
    state: &Mutex<Studio>,
    ask: &TurnDiff,
    diffs: Option<Vec<RootDiff>>,
    compute: impl FnOnce(&TurnFilesSnapshot, Vec<RootDiff>) -> TurnFiles,
) -> Option<TurnFiles> {
    let diffs = diffs?;
    let snapshot = state.lock().unwrap().turn_files_snapshot(ask, &diffs);
    Some(compute(&snapshot, diffs))
}

/// The turns whose files the core asks for (danielrispler/plotkeeper#111): each is diffed by `git`
/// with the core unlocked, then handed back. Returns whether any was asked.
fn turn_files(state: &Mutex<Studio>) -> bool {
    /// At most this many turns a pass, so a launch over a long log does not hold the loop.
    const PER_PASS: usize = 8;
    let asks = state.lock().unwrap().turn_diffs(millis(), PER_PASS);
    for ask in &asks {
        let diffs = diff_turn(state, ask);
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
        let key = {
            let studio = app.state::<Mutex<Studio>>();
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
            }
        };
        if self.cards.as_ref() != Some(&key) {
            let _ = app.emit("cards", cards(app.state(), app.state()));
            self.cards = Some(key);
        }
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
fn guess(id: String, option: usize, studio: State<Mutex<Studio>>, cards: State<Cards>) {
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

/// The retests that have a Quiz (danielrispler/plotkeeper#129), which Home offers.
#[tauri::command]
fn retests(cards: State<Cards>) -> Vec<studio_understanding::Retest> {
    let store = cards.lock().unwrap();
    let kept = store.retests().iter();
    kept.filter(|r| r.quiz.is_some()).cloned().collect()
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
    studio: State<Mutex<Studio>>,
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
    studio: State<Mutex<Studio>>,
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
    studio: State<Mutex<Studio>>,
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

/// Daniel closed terminal `terminal` (danielrispler/Falinks#461, story 15 and decision 8): a `SessionEnd` line in the event
/// log for each session in it that has not ended, as its `claude` may die before its own hook runs.
/// A real `SessionEnd` after it changes nothing.
#[tauri::command(async)]
fn closed(terminal: String, studio: State<Mutex<Studio>>) -> Result<(), String> {
    let lines: Vec<serde_json::Value> = studio
        .lock()
        .unwrap()
        .sessions()
        .into_iter()
        .filter(|s| s.terminal.as_deref() == Some(&terminal) && s.state != Ended)
        .filter(|s| !s.status_only)
        .map(|s| {
            serde_json::json!({
                "session_id": s.resume_id,
                "hook_event_name": "SessionEnd",
                "reason": "studio_close",
                "studio_received_ms": millis(),
            })
        })
        .collect();
    append(&lines)
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
        .state::<Mutex<Studio>>()
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
    let mut studio = Studio::with_roots(worktree_root);
    studio.know_roots(saved_roots(&log));
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
        .manage(Mutex::new(studio))
        .manage(Cards::new(store))
        .manage(Notices::default())
        .manage(writer.clone())
        .manage(Summaries::default())
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
            retests,
            rate,
            explain_more,
            ask,
            rename,
            delete_session,
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
            thread::spawn(move || follow(&app, &log, &writer));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("build the studio")
        .run(|_, event| {
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
fn follow(app: &AppHandle, log: &Path, writer: &Writer) {
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
    let mut read_to = resume(&app.state::<Mutex<Studio>>(), log);
    let mut roots_saved = 0;
    let mut live = false;
    // Each session's and each subagent's own, so another's prompt or step in the same worktree moves
    // no base.
    let mut fingerprints = HashMap::<String, Fingerprints>::new();
    // Each session's card writer calls, one at a time and in order, so a parent's card or brief reads
    // its workers' cards, and two cards never update one brief at once.
    let mut queues = HashMap::<String, mpsc::Sender<Job>>::new();
    let versions = Versions::beside(log);
    let agy_versions = AgyVersions::beside(log);
    let mut sent = Sent::default();
    loop {
        let state = app.state::<Mutex<Studio>>();
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
                // A turn already decided and on record, a refused one too, is not asked again.
                let recorded = offers();
                let cards = backfill
                    .into_iter()
                    .filter(|b| !recorded.settled(&b.request.session_id, &b.request.id))
                    .filter_map(backfilled);
                // No summary follows a backfilled card, so nothing is kept for one.
                for request in cards {
                    write(app, writer, &mut queues, request, true, false);
                }
            }
            for output in outputs {
                match output {
                    Output::Card(request) => write(app, writer, &mut queues, request, true, true),
                    Output::Brief(request) => write(app, writer, &mut queues, request, false, true),
                    Output::Summary(request) => summarize(app, writer, &mut queues, request),
                    Output::Name(request) => name(app, writer, &mut queues, request),
                    Output::NeedsYou { session_id, agent } => {
                        tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
                    }
                    Output::YourTurn { session_id } => {
                        refresh_landed(app, Some(session_id.clone()));
                        tell(app, Notice::YourTurn, &session_id, None);
                    }
                    Output::CommandBlocked { session_id, agent } => {
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
            // Dropping a queue's sender closes it: its thread finishes the jobs it holds, then ends.
            if read {
                let roots = state.lock().unwrap().known_roots();
                if roots.len() != roots_saved {
                    roots_saved = roots.len();
                    save_roots(log, &roots);
                }
                let keys = state.lock().unwrap().live_keys();
                fingerprints.retain(|key, _| keys.contains(key));
                queues.retain(|key, _| keys.contains(key));
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

/// The `gh` to run: the first on Daniel's login shell's `PATH`, found once.
fn gh_path() -> String {
    static GH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    GH.get_or_init(|| {
        // An app started from the Dock gets launchd's `PATH`, which lacks Homebrew's folder.
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        let found = Command::new(shell).args(["-lc", "command -v gh"]).output();
        let found = found.ok().filter(|o| o.status.success()).map(|o| o.stdout);
        found
            .as_deref()
            .map(String::from_utf8_lossy)
            .and_then(|out| out.lines().last().map(str::to_owned))
            .filter(|path| path.starts_with('/'))
            .unwrap_or_else(|| "gh".into())
    })
    .clone()
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
        let studio = app.state::<Mutex<Studio>>();
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
        let gh = gh_path();
        for task in tasks {
            // A removed worktree has no facts.
            let facts = Path::new(&task.path)
                .exists()
                .then(|| lookup_task(&gh, &task));
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
    let gh = gh_path();
    let mut asked = HashMap::<String, Instant>::new();
    loop {
        let branches = app.state::<Mutex<Studio>>().lock().unwrap().branches();
        for (cwd, branch) in branches {
            if asked
                .get(&branch)
                .is_some_and(|at| at.elapsed() < Duration::from_secs(300))
            {
                continue;
            }
            asked.insert(branch.clone(), Instant::now());
            let output = Command::new(&gh)
                .args(["pr", "view", &branch, "--json", "number", "--jq", ".number"])
                .current_dir(&cwd)
                .output();
            let output = output.ok().filter(|o| o.status.success());
            let pr = output.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok());
            if app
                .state::<Mutex<Studio>>()
                .lock()
                .unwrap()
                .set_pr(&branch, pr)
            {
                LIST_CHANGED.store(true, Ordering::Release);
            }
        }
        thread::sleep(Duration::from_secs(5));
    }
}

/// A card writer call, run on its session's queue.
type Job = Box<dyn FnOnce() + Send>;

/// Writes the card `request` asks for, or only the brief when `card` is false, on its session's
/// queue in `queues`; `live` is false for a card of a turn that ended while the app was down. `claude -p` takes seconds, so the read loop goes on and draws a card once it
/// is in the store; the row's line changes with no line read, so the list is sent from here.
fn write(
    app: &AppHandle,
    writer: &Writer,
    queues: &mut HashMap<String, mpsc::Sender<Job>>,
    mut request: CardRequest,
    card: bool,
    live: bool,
) {
    let (app, writer) = (app.clone(), writer.clone());
    let id = request.session_id.clone();
    let Some(session) = app
        .state::<Mutex<Studio>>()
        .lock()
        .unwrap()
        .session(&id)
        .cloned()
    else {
        return;
    };
    // Only a live turn's own card has a summary to come, which takes what is kept here and clears it.
    let turn_card = card && live && turns_card(&session, &request);
    if turn_card {
        let files = changed_in(request.diff());
        let changes = &app.state::<Summaries>().changes;
        changes.lock().unwrap().insert(request.id.clone(), files);
    }
    let job = move || {
        let store = app.state::<Cards>();
        // Where the card's code is pulled from: its turn's end trees (danielrispler/plotkeeper#121).
        request.ends = app
            .state::<Mutex<Studio>>()
            .lock()
            .unwrap()
            .card_ends(&request);
        let agent = request.agent.clone();
        let sent = || {
            let _ = app.emit("sessions", sessions(app.state(), app.state()));
        };
        // The list and the notice go out once the card is stored, before a brief rewrite.
        let stored = || {
            // A check that ended while its card was written settles it before the pane draws it.
            let studio = app.state::<Mutex<Studio>>();
            settle_checks(&studio.lock().unwrap(), &mut store.lock().unwrap());
            sent();
            tell(
                &app,
                Notice::Titled("New card"),
                &session.agent.id,
                agent.as_deref(),
            );
        };
        let card_id = request.id.clone();
        // A turn the table gives no card is written none, but still feeds the design.
        let written = match card {
            true => {
                let turn = turn_of(&app.state(), &request.id);
                let offers = offers();
                let (written, decided) = offer_stop(
                    &store, &*writer, request, &session, &turn, &offers, turn_card, stored, report,
                );
                if let Some(decided) = decided.filter(|_| turn_card) {
                    let summaries = app.state::<Summaries>();
                    summaries
                        .decided
                        .lock()
                        .unwrap()
                        .insert(card_id.clone(), decided);
                }
                written
            }
            false => write_goal(&store, &*writer, &request, &session, report),
        };
        // A Quiz is never beside a decision (danielrispler/plotkeeper#119).
        if card && session.needs_you() {
            unsaved(store.lock().unwrap().drop_card_quiz(&card_id));
        }
        let designed = design(
            &app.state::<Mutex<Studio>>(),
            &store,
            &*writer,
            &session.agent.id,
        );
        // The brief, which a row's line may show, changed again.
        if written || designed {
            sent();
        }
    };
    let _ = queue(queues, id).send(Box::new(job));
}

/// What the hooks and git recorded of the turn named `id`, for its summary writer (danielrispler/plotkeeper#115):
/// its files with their counts and hunks, and its checks. Empty for a turn with no trees.
fn turn_facts(state: &Mutex<Studio>, id: &str) -> Facts {
    let Some(ask) = state.lock().unwrap().summary_ask(id) else {
        return Facts::default();
    };
    let diffs = diff_turn(state, &ask);
    let found = found_files(state, &ask, diffs, compute_turn_files);
    let mut facts = state.lock().unwrap().facts_of(&ask, found);
    add_hunks(&ask, &mut facts);
    facts
}

/// The turn named `id` as the offer reads it (danielrispler/plotkeeper#209): its facts, and whether it merged.
fn turn_of(state: &Mutex<Studio>, id: &str) -> TurnFacts {
    TurnFacts {
        facts: turn_facts(state, id),
        merge: state.lock().unwrap().turn_merged(id),
        workers: state.lock().unwrap().turn_workers(id),
        traced: state.lock().unwrap().summary_ask(id).is_some(),
    }
}

/// The record of offers, beside the event log.
fn offers() -> Offers {
    let home = std::env::home_dir().unwrap_or_default();
    Offers::beside(&studio_hook::event_log(&home))
}

/// Fills in `request`'s facts and, by the one offer function (danielrispler/plotkeeper#209), whether its summary may
/// keep a Quiz. Returns whether the summary is written: one asked for the sake of a lone Quiz is dropped when the
/// turn gets none.
fn settle_summary(app: &AppHandle, request: &mut SummaryRequest) -> bool {
    let state = app.state::<Mutex<Studio>>();
    let summaries = app.state::<Summaries>();
    let change = summaries.changes.lock().unwrap().remove(&request.id);
    let decided = summaries.decided.lock().unwrap().remove(&request.id);
    let settled = settle(
        request,
        &turn_of(&state, &request.id),
        &change.unwrap_or_default(),
        decided.as_ref(),
    );
    settled.record(&offers(), request);
    if settled.decision.offer == Offer::Quiz {
        let mut lone = summaries.lone.lock().unwrap();
        lone.insert(request.id.clone(), settled.decision);
    }
    settled.write
}

/// Writes the summary of `request`'s turn on its session's queue, after the turn's card. While summaries are written
/// on open, the turn waits for Daniel instead (`turn_summary`).
fn summarize(
    app: &AppHandle,
    writer: &Writer,
    queues: &mut HashMap<String, mpsc::Sender<Job>>,
    request: SummaryRequest,
) {
    if app
        .state::<Mutex<Studio>>()
        .lock()
        .unwrap()
        .summaries_on_open()
    {
        // On its session's queue, after the card's job that makes the decision the summary takes.
        let app = app.clone();
        let id = request.session_id.clone();
        let job = move || {
            let mut request = request;
            if settle_summary(&app, &mut request) {
                let waiting = &app.state::<Summaries>().waiting;
                waiting.lock().unwrap().insert(request.id.clone(), request);
                app.state::<Cards>().lock().unwrap().waiting_summary();
            }
        };
        queue(queues, id).send(Box::new(job)).ok();
        return;
    }
    let (app, writer) = (app.clone(), writer.clone());
    let id = request.session_id.clone();
    let job = move || {
        let setup = app.state::<Summaries>().setup.lock().unwrap().clone();
        let mut request = request;
        if !settle_summary(&app, &mut request) {
            return;
        }
        let session = request.session_id.clone();
        let (studio, cards) = (app.state::<Mutex<Studio>>(), app.state::<Cards>());
        let lone = app
            .state::<Summaries>()
            .lone
            .lock()
            .unwrap()
            .remove(&request.id);
        let offers = offers();
        if offer_summary(
            &cards,
            &*writer,
            &setup,
            request,
            lone.as_ref(),
            &offers,
            report,
        )
        .is_some()
        {
            design(&studio, &cards, &*writer, &session);
        }
    };
    queue(queues, id).send(Box::new(job)).ok();
}

/// Names the session of `request` on its queue, from Daniel's first prompt, at the summary writer's
/// model and effort. The name is a `StudioPromptName` line in the event log, which the log thread
/// applies and a restart replays.
fn name(
    app: &AppHandle,
    writer: &Writer,
    queues: &mut HashMap<String, mpsc::Sender<Job>>,
    request: NameRequest,
) {
    let (app, writer) = (app.clone(), writer.clone());
    let id = request.session_id.clone();
    let job = move || {
        let setup = app.state::<Summaries>().setup.lock().unwrap().clone();
        let response = request.response.as_deref();
        let Some(name) = write_name(&*writer, &setup, &request.prompt, response, report) else {
            return;
        };
        let line = serde_json::json!({
            "session_id": request.session_id,
            "hook_event_name": studio_core::PROMPT_NAME,
            "studio_name": name,
            "studio_received_ms": millis(),
        });
        if let Err(e) = append(&[line]) {
            log::error!("save the name of {}: {e}", request.session_id);
        }
    };
    queue(queues, id).send(Box::new(job)).ok();
}

/// Session `id`'s queue in `queues`, started when it has none.
fn queue(queues: &mut HashMap<String, mpsc::Sender<Job>>, id: String) -> &mpsc::Sender<Job> {
    queues.entry(id).or_insert_with(|| {
        let (queue, jobs) = mpsc::channel::<Job>();
        thread::spawn(move || jobs.into_iter().for_each(|job| job()));
        queue
    })
}

/// Adds a notice on session `session_id`'s line, naming its subagent `agent`, if any; the read loop
/// shows it. Called without the core's lock held.
fn tell(app: &AppHandle, notice: Notice, session_id: &str, agent: Option<&str>) {
    let body = app
        .state::<Mutex<Studio>>()
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
fn mark_unseen(id: String, studio: State<Mutex<Studio>>, cards: State<Cards>) {
    if let Some((id, turns)) = studio.lock().unwrap().mark_unseen(&id) {
        unsaved(cards.lock().unwrap().seen(&id, turns));
        LIST_CHANGED.store(true, Ordering::Release);
    }
}

/// The `sounds` setting (danielrispler/Falinks#556), from `settings.json`, which the window parses.
#[tauri::command]
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
    use super::{
        compute_turn_files, found_files, in_app, log_line, lost_data, saved_roots, scrubbed,
        shows_in_window,
    };
    use std::sync::Mutex;
    use studio_core::{Numstat, RootDiff, Studio};

    /// A turn of one finished edit of `src/a.rs`, asked for with the trees `s1` and `e1`.
    fn ended_turn() -> Studio {
        let at = 1_800_000_000_000_u64;
        let line = |name: &str, ms: u64, extra: serde_json::Value| {
            let mut event = serde_json::json!({
                "session_id": "5b1c7e30-9d2a-4f8b-8c63-1e0a7d4b9f52", "hook_event_name": name,
                "cwd": "/w/repo", "prompt_id": "p1", "studio_received_ms": at + ms,
            });
            event
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            event.to_string()
        };
        Studio::replayed([
            line(
                "UserPromptSubmit",
                0,
                serde_json::json!({
                "prompt": "go", "studio_turn_start": { "/w/repo": "s1" } }),
            ),
            line(
                "PostToolUse",
                1_000,
                serde_json::json!({
                "tool_name": "Edit", "tool_input": { "file_path": "/w/repo/src/a.rs" },
                "tool_use_id": "e1", "tool_response": {} }),
            ),
            line(
                "Stop",
                9_000,
                serde_json::json!({ "studio_turn_end": { "/w/repo": "e1" } }),
            ),
        ])
    }

    #[test]
    fn a_turns_files_are_matched_with_the_core_unlocked_and_stored_after() {
        let mut studio = ended_turn();
        let ask = studio.turn_diffs(1_800_000_020_000, 10).remove(0);
        let state = Mutex::new(studio);
        let diffs = vec![RootDiff {
            root: "/w/repo".into(),
            moved: vec![],
            files: vec![Numstat {
                path: "src/a.rs".into(),
                from: None,
                added: Some(1),
                removed: Some(0),
            }],
        }];
        let found = found_files(&state, &ask, Some(diffs), |snapshot, diffs| {
            // Another thread takes the core while the matching runs.
            let free = std::thread::scope(|s| s.spawn(|| state.try_lock().is_ok()).join().unwrap());
            assert!(free, "the core is locked while the files are matched");
            compute_turn_files(snapshot, diffs)
        });
        let found = found.expect("files");
        assert_eq!(found.files.len(), 1);
        state
            .lock()
            .unwrap()
            .store_turn_files(&ask, Some(found.clone()));
        let guard = state.lock().unwrap();
        let session = guard
            .session("5b1c7e30-9d2a-4f8b-8c63-1e0a7d4b9f52")
            .unwrap();
        assert_eq!(session.turns[0].files, Some(found));
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
