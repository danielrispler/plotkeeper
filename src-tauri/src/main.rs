//! The studio app's shell (#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line live and hands the core's card requests to the card writer. The
//! window lists every session (#369, decision 18) and shows the one Daniel selected: its cards, with
//! their quizzes, his guesses, his ratings and the longer explanations he asked for, which the card
//! store keeps on disk, and its check lines. A session that newly needs Daniel or reaches your turn,
//! a new card and a failed worker raise a macOS notification while the window is not focused, and
//! the Dock badge counts the sessions that need him (#369, decision 22; #517, decision 6).

use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, Seek, SeekFrom, Write},
    path::Path,
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
    CardRequest, Notifications, Output, Session,
    State::{Ended, NeedsYou},
    Studio, SummaryRequest,
};
use studio_understanding::{
    CardStore, CardWriter, ClaudeWriter, FakeWriter, Fingerprints, Rating, Row, SessionCards,
    StoredSummary, SummarySetup, Versions, backfilled, fingerprint_changes, millis, worktree_root,
    write_agent_summary, write_brief, write_stop, write_summary,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

mod answer;
mod presence;
mod restart;
mod settings;
mod terminal;

/// The card store, which the log thread adds to and the pane records guesses in.
type Cards = Mutex<CardStore>;

/// The notices not shown yet, which the log thread and the card writer's threads add to.
type Notices = Mutex<Notifications>;

/// The summary writer's setup, from the settings the window sends (#526), and the turns whose
/// summary waits for Daniel to open it while summaries are written on open, by turn id.
// shortcut: the waiting turns live in memory, so a restart loses them; keep them in the store if
// that bites.
#[derive(Default)]
struct Summaries {
    setup: Mutex<SummarySetup>,
    waiting: Mutex<HashMap<String, SummaryRequest>>,
}

/// The settings' `summaries` (#517, decision 17): the writer's model and effort, and whether a
/// summary is written when Daniel opens its turn instead of at the turn's end.
#[tauri::command]
fn summary_settings(
    model: String,
    effort: String,
    on_open: bool,
    summaries: State<Summaries>,
    studio: State<Mutex<Studio>>,
) {
    *summaries.setup.lock().unwrap() = SummarySetup { model, effort };
    studio.lock().unwrap().set_summaries_on_open(on_open);
}

/// Daniel opened turn `id` (#526): its summary, written now when it waits for him, which takes
/// seconds, so the command runs off the main thread. `None` when the turn has none.
#[tauri::command(async)]
fn turn_summary(
    id: String,
    cards: State<Cards>,
    summaries: State<Summaries>,
    writer: State<Writer>,
) -> Option<StoredSummary> {
    if let Some(kept) = cards.lock().unwrap().turn_summary(&id) {
        return Some(kept.clone());
    }
    let request = summaries.waiting.lock().unwrap().remove(&id)?;
    let setup = summaries.setup.lock().unwrap().clone();
    let log = |line: String| eprintln!("studio: {line}");
    write_summary(&cards, &**writer, &setup, request, log)?;
    cards.lock().unwrap().turn_summary(&id).cloned()
}

/// Daniel opened subagent `agent_id` (#535): the summary of its purpose and result, written now
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
    let setup = summaries.setup.lock().unwrap().clone();
    let log = |line: String| eprintln!("studio: {line}");
    write_agent_summary(&cards, &**writer, &setup, &agent_id, &prompt, &message, log)
}

/// The session list, in its order.
#[tauri::command]
fn sessions(studio: State<Mutex<Studio>>, cards: State<Cards>) -> Vec<Row> {
    let studio = studio.lock().unwrap();
    let now = studio.now();
    let list: Vec<Session> = studio.sessions().into_iter().cloned().collect();
    drop(studio);
    let cards = cards.lock().unwrap();
    let row = |session: Session| Row {
        line: cards.line(&session, now),
        session,
    };
    list.into_iter().map(row).collect()
}

/// Daniel clicked session `id`'s row; returns the selection, which no longer names a subagent.
#[tauri::command]
fn select(id: String, studio: State<Mutex<Studio>>) -> serde_json::Value {
    let mut studio = studio.lock().unwrap();
    studio.select(&id);
    selection(&studio)
}

/// Daniel started a session in studio terminal `terminal`; returns the selection, `null` until the
/// session's first event names that terminal and selects it (#567).
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
    let (viewed, now) = {
        let studio = studio.lock().unwrap();
        (studio.viewed().map(String::from), studio.now())
    };
    match viewed {
        Some(owner) => cards.lock().unwrap().session_cards(&owner, now),
        None => SessionCards::default(),
    }
}

/// The plan's 5-hour and weekly usage from the status line's newest snapshot in the `usage` folder
/// beside the event log, `null` when there is none (#481).
#[tauri::command]
fn usage() -> Option<serde_json::Value> {
    studio_workbench::plan_usage(&studio_hook::data_dir(&std::env::home_dir()?).join("usage"))
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
}

impl Sent {
    /// Sends the session list, the Dock badge and the selection, each only if it changed.
    fn list(&mut self, app: &AppHandle) {
        let list = sessions(app.state(), app.state());
        let needing = list.iter().filter(|r| r.session.state == NeedsYou).count();
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
            let revision = app.state::<Cards>().lock().unwrap().revision();
            CardsKey {
                viewed: studio.viewed().map(String::from),
                revision,
                minute: studio.now() / 60_000,
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

/// Settles the guesses on the cards of the checks that ended (#461, decision 15). Takes the core's
/// lock before the store's, as the read loop does.
fn settle_checks(studio: &Studio, store: &mut CardStore) {
    for session in studio.sessions() {
        unsaved(store.settle_checks(session));
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
    let log = |line: String| eprintln!("studio: {line}");
    studio_understanding::explain_more(&cards, &**writer, &id, log)
}

/// Daniel's side `question` on `item`, a response, card or intention of session `session` (#461,
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
        eprintln!("studio: no answer on {session}: {line}");
    }
    answer
}

/// Daniel renamed session `id`: a `StudioRename` line in the event log, which the log thread then
/// applies and a restart replays (#469). An empty `name` gives the row its folder's again.
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

/// Daniel closed terminal `terminal` (#461, story 15 and decision 8): a `SessionEnd` line in the event
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

/// Appends `lines` to the event log in one write under the lock the hook takes, so they never
/// interleave with a hook's line.
fn append(lines: &[serde_json::Value]) -> Result<(), String> {
    let home = std::env::home_dir().ok_or("no home directory")?;
    let open = OpenOptions::new()
        .append(true)
        .open(studio_hook::event_log(&home));
    let mut log = open.map_err(|e| e.to_string())?;
    log.lock().map_err(|e| e.to_string())?;
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    log.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

/// Logs a card store change that could not be saved; it stays in memory until the next save.
fn unsaved(saved: io::Result<()>) {
    if let Err(e) = saved {
        eprintln!("studio: save the card store: {e}");
    }
}

/// The card writer: `claude -p`, or the fake when `STUDIO_CARD_WRITER=fake`, for checks that must
/// not run `claude -p`.
type Writer = Arc<dyn CardWriter + Send + Sync>;

/// Whether a navigation stays on the app's own pages (#525, decision 21): a backstop, as a rendered
/// link opens in the browser through `open_link` and never navigates the window.
fn in_app(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

fn main() {
    uptime();
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    // The store sits beside the log, in the studio's data directory.
    let store = CardStore::open_beside(&log).expect("read the card store");
    let studio = Studio::with_roots(worktree_root);
    let writer: Writer = match std::env::var("STUDIO_CARD_WRITER").as_deref() {
        Ok("fake") => Arc::new(FakeWriter),
        _ => Arc::new(ClaudeWriter::beside(&log).expect("create the card writer's folder")),
    };
    tauri::Builder::default()
        // First, so a second instance exits before it sets anything up (#470).
        .plugin(tauri_plugin_single_instance::init(restart::second_instance))
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
            cards,
            shown,
            guess,
            key_wrong,
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
            terminal::places,
            terminal::claude_argv,
            terminal::create_task,
            terminal::create_named_task,
            terminal::remove_task,
            terminal::delete_merged_branch,
            terminal::terminal_lost,
            terminal::terminal_upgrade,
            restart::revival,
            restart::restart_into,
            answer::watch,
            answer::alive,
            answer::answer,
            settings::settings_files,
            settings::set_zoom,
            summary_settings,
            turn_summary,
            agent_summary,
            notify_while_focused
        ])
        .on_window_event(|_, event| {
            if let Some(path) = presence::path() {
                presence::window_event(&path, event);
            }
        })
        .setup(move |app| {
            if let Some(path) = presence::path() {
                presence::start(&path);
            }
            let app = app.handle().clone();
            restart::watch_build(app.clone());
            settings::watch(app.clone());
            let prs_app = app.clone();
            thread::spawn(move || prs(&prs_app));
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
/// turns of the last 24 hours with no card, which get one on the hook's trees (#486). Live lines
/// get the worktree fingerprint the core asks for; replayed ones get none. Each pass ends with a
/// tick, the first right after the replay.
// shortcut: polls every 200 ms; switch to file-system events if the delay or the wake-ups matter.
fn follow(app: &AppHandle, log: &Path, writer: &Writer) {
    let mut read_to = 0;
    let mut live = false;
    // Each session's and each subagent's own, so another's prompt or step in the same worktree moves
    // no base.
    let mut fingerprints = HashMap::<String, Fingerprints>::new();
    // Each session's card writer calls, one at a time and in order, so a parent's card or brief reads
    // its workers' cards, and two cards never update one brief at once.
    let mut queues = HashMap::<String, mpsc::Sender<Job>>::new();
    let versions = Versions::beside(log);
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
                let store = app.state::<Cards>();
                let store = store.lock().unwrap();
                store.restore(&mut studio);
                // The turns that ended while the app was down get their cards now (#486).
                let requests: Vec<CardRequest> =
                    store.cards().iter().map(|k| k.request.clone()).collect();
                let backfill = studio.backfill(millis(), &requests);
                drop((store, studio));
                state.lock().unwrap().tick(millis());
                sent.list(app);
                let waiting = state.lock().unwrap().waiting_folders();
                fingerprint_changes(waiting, &mut fingerprints);
                let cards = backfill.into_iter().filter_map(backfilled);
                outputs.extend(cards.map(Output::Card));
            }
            for output in outputs {
                match output {
                    Output::Card(request) => write(app, writer, &mut queues, request, true),
                    Output::Brief(request) => write(app, writer, &mut queues, request, false),
                    Output::Summary(request) => summarize(app, writer, &mut queues, request),
                    Output::NeedsYou { session_id, agent } => {
                        tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
                    }
                    Output::YourTurn { session_id } => {
                        tell(app, Notice::YourTurn, &session_id, None);
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
                    } => eprintln!(
                        "studio: unrecognised wake-up: a prompt of session {session_id} names \
                         task {task_id} in markup the studio does not know"
                    ),
                    // Reads the transcript's lines and nothing else (`apps/studio/AGENTS.md`).
                    Output::Version { transcript_path } => {
                        match versions.check(Path::new(&transcript_path), millis()) {
                            Ok(Some(notice)) => {
                                eprintln!("studio: {notice}");
                                let _ = app.emit("notice", &notice);
                            }
                            Ok(None) => {}
                            Err(e) => eprintln!("studio: keep Claude Code's version: {e}"),
                        }
                    }
                }
            }
            read = read_to > read_from;
            // Dropping a queue's sender closes it: its thread finishes the jobs it holds, then ends.
            if read {
                let keys = state.lock().unwrap().live_keys();
                fingerprints.retain(|key, _| keys.contains(key));
                queues.retain(|key, _| keys.contains(key));
                // The decision history goes to the store, which keeps it across restarts (#479).
                let studio = state.lock().unwrap();
                let mut store = app.state::<Cards>().inner().lock().unwrap();
                for session in studio.sessions() {
                    unsaved(store.keep_decisions(&session.agent.id, &session.decisions));
                }
                settle_checks(&studio, &mut store);
            }
        }
        // Reads the transcripts' lines and nothing else (`apps/studio/AGENTS.md`).
        let log = |line: String| eprintln!("studio: {line}");
        let transcripts_changed = state.lock().unwrap().follow_transcripts(log);
        // Each live session's context, from its status line snapshot (#517, decision 1; #519); the
        // snapshot is named by the conversation's id, which a `/clear` changes.
        let running: Vec<_> = (state.lock().unwrap().sessions().into_iter())
            .filter(|s| s.state != Ended)
            .map(|s| (s.agent.id.clone(), s.resume_id.clone()))
            .collect();
        let home = std::env::home_dir().unwrap_or_default();
        let usage = studio_hook::data_dir(&home).join("usage");
        let windows_changed = running.into_iter().fold(false, |changed, (id, resume_id)| {
            let snapshot = studio_workbench::context_window(&usage, &resume_id);
            state.lock().unwrap().set_snapshot(&id, snapshot) | changed
        });
        let names_changed = NAMES_CHANGED.swap(false, Ordering::AcqRel);
        let changed = read || transcripts_changed || windows_changed || names_changed;
        // The open request goes in the same pass as the list that shows "needs you" (#569).
        if let Ok(data) = answer::data()
            && let Some(ask) = app
                .state::<Mutex<answer::Watching>>()
                .lock()
                .unwrap()
                .pass(&data, studio_hook::answer::now_ms())
        {
            let _ = app.emit("ask", ask);
        }
        if state.lock().unwrap().tick(millis()) || changed {
            sent.list(app);
        }
        // Asking for focus waits on the main thread, which never takes this lock.
        let focused = || {
            let window = app.get_webview_window("main");
            window.and_then(|w| w.is_focused().ok()) == Some(true)
        };
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
                studio.selected().map(|s| s.agent.id.clone())
            });
        if let Some((title, body)) = due {
            let _ = app.notification().builder().title(title).body(body).show();
        }
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        sent.cards(app);
        live = true;
        restart::REPLAYED.store(true, std::sync::atomic::Ordering::Release);
        thread::sleep(Duration::from_millis(200));
    }
}

/// A session's name changed since the read loop last sent the list: a PR was found.
static NAMES_CHANGED: AtomicBool = AtomicBool::new(false);

/// Finds the PR of each session's branch with `gh pr view` (#461, decision 19), on its own thread so
/// the read loop never waits on the network, and asks again after 5 minutes. `gh` runs as Daniel
/// with its own login; no credential file is read.
fn prs(app: &AppHandle) {
    // An app started from the Dock gets launchd's `PATH`, which lacks Homebrew's folder.
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let found = Command::new(shell).args(["-lc", "command -v gh"]).output();
    let found = found.ok().filter(|o| o.status.success()).map(|o| o.stdout);
    let gh = found
        .as_deref()
        .map(String::from_utf8_lossy)
        .and_then(|out| out.lines().last().map(str::to_owned))
        .filter(|path| path.starts_with('/'))
        .unwrap_or_else(|| "gh".into());
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
                NAMES_CHANGED.store(true, Ordering::Release);
            }
        }
        thread::sleep(Duration::from_secs(5));
    }
}

/// A card writer call, run on its session's queue.
type Job = Box<dyn FnOnce() + Send>;

/// Writes the card `request` asks for, or only the brief when `card` is false, on its session's
/// queue in `queues`. `claude -p` takes seconds, so the read loop goes on and draws a card once it
/// is in the store; the row's line changes with no line read, so the list is sent from here.
fn write(
    app: &AppHandle,
    writer: &Writer,
    queues: &mut HashMap<String, mpsc::Sender<Job>>,
    request: CardRequest,
    card: bool,
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
    let job = move || {
        let log = |line: String| eprintln!("studio: {line}");
        let store = app.state::<Cards>();
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
        let written = match card {
            true => write_stop(&store, &*writer, request, &session, stored, log),
            false => write_brief(&store, &*writer, request, &session, log),
        };
        // The brief, which a row's line may show, changed again.
        if written {
            sent();
        }
    };
    let _ = queue(queues, id).send(Box::new(job));
}

/// Writes the summary of `request`'s turn on its session's queue, after the turn's card, then
/// switches the turn's intention to its first line (#517, decision 16). While summaries are written
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
        let waiting = &app.state::<Summaries>().waiting;
        waiting.lock().unwrap().insert(request.id.clone(), request);
        return;
    }
    let (app, writer) = (app.clone(), writer.clone());
    let id = request.session_id.clone();
    let job = move || {
        let log = |line: String| eprintln!("studio: {line}");
        let setup = app.state::<Summaries>().setup.lock().unwrap().clone();
        let (session, turn) = (request.session_id.clone(), request.id.clone());
        let Some(summary) = write_summary(&app.state::<Cards>(), &*writer, &setup, request, log)
        else {
            return;
        };
        let first = summary.summary.lines().next().unwrap_or_default();
        let studio = app.state::<Mutex<Studio>>();
        if studio.lock().unwrap().summarized(&session, &turn, first) {
            let _ = app.emit("sessions", sessions(app.state(), app.state()));
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

/// The setting that also notifies "your turn" while the window is focused (#517, decision 6), from
/// `settings.json`, which the window parses.
#[tauri::command]
fn notify_while_focused(on: bool, notices: State<Notices>) {
    notices.lock().unwrap().while_focused = on;
}

/// Time since the app started, a monotonic clock for combining notices (#434, decision 28).
fn uptime() -> Duration {
    static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
    STARTED.elapsed()
}

#[cfg(test)]
mod tests {
    use super::in_app;

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
