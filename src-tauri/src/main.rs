//! The studio app's shell (#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line live and hands the core's card requests to the card writer. The
//! window lists every session (#369, decision 18) and shows the one Daniel selected: its cards, with
//! their quizzes, his guesses, his ratings and the longer explanations he asked for, which the card
//! store keeps on disk, and its check lines. A session that newly needs Daniel, a new card and a
//! failed worker raise a macOS notification while the window is not focused, and the Dock badge
//! counts the sessions that need him (#369, decision 22).

use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};
use studio_core::{CardRequest, Notifications, Output, Session, State::NeedsYou, Studio};
use studio_understanding::{
    CardStore, CardWriter, ClaudeWriter, FakeWriter, Fingerprints, Rating, Row, SessionCards,
    Versions, fingerprint_changes, millis, write_brief, write_stop,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// The card store, which the log thread adds to and the pane records guesses in.
type Cards = Mutex<CardStore>;

/// The notices not shown yet, which the log thread and the card writer's threads add to.
type Notices = Mutex<Notifications>;

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

/// Daniel opened subagent `agent_id`'s line; returns the selection, which names it.
#[tauri::command]
fn open(agent_id: String, studio: State<Mutex<Studio>>) -> serde_json::Value {
    let mut studio = studio.lock().unwrap();
    studio.open(&agent_id);
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

/// Daniel tapped option `option` of card `id`'s quiz.
#[tauri::command(async)]
fn guess(id: String, option: usize, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().guess(&id, option, millis()));
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

/// Logs a card store change that could not be saved; it stays in memory until the next save.
fn unsaved(saved: io::Result<()>) {
    if let Err(e) = saved {
        eprintln!("studio: save the card store: {e}");
    }
}

/// The card writer: `claude -p`, or the fake when `STUDIO_CARD_WRITER=fake`, for checks that must
/// not run `claude -p`.
type Writer = Arc<dyn CardWriter + Send + Sync>;

fn main() {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    // The store sits beside the log, in the studio's data directory.
    let store = CardStore::open_beside(&log).expect("read the card store");
    let studio = Studio::default();
    let writer: Writer = match std::env::var("STUDIO_CARD_WRITER").as_deref() {
        Ok("fake") => Arc::new(FakeWriter),
        _ => Arc::new(ClaudeWriter::beside(&log).expect("create the card writer's folder")),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(Mutex::new(studio))
        .manage(Cards::new(store))
        .manage(Notices::default())
        .manage(writer.clone())
        .invoke_handler(tauri::generate_handler![
            sessions,
            select,
            open,
            selected,
            cards,
            shown,
            guess,
            key_wrong,
            rate,
            explain_more
        ])
        .setup(move |app| {
            let app = app.handle().clone();
            thread::spawn(move || follow(&app, &log, &writer));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run the studio");
}

/// Reads the log from its start, then from where it stopped. A line counts once it ends in a newline,
/// so a line the hook is still writing waits for the next read. The lines there at start are
/// replayed, so steps that ran while the app was down get no cards and no notifications. Live lines
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
                state.lock().unwrap().tick(millis());
                sent.list(app);
                let waiting = state.lock().unwrap().waiting_folders();
                fingerprint_changes(waiting, &mut fingerprints);
            }
            for output in outputs {
                match output {
                    Output::Card(request) => write(app, writer, &mut queues, request, true),
                    Output::Brief(request) => write(app, writer, &mut queues, request, false),
                    Output::NeedsYou { session_id, agent } => {
                        tell(app, "Needs you", &session_id, agent.as_deref());
                    }
                    Output::Failed { session_id, agent } => {
                        tell(app, "Worker failed", &session_id, Some(&agent));
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
            }
        }
        if state.lock().unwrap().tick(millis()) || read {
            sent.list(app);
        }
        // Asking for focus waits on the main thread, which never takes this lock.
        let focused = || {
            let window = app.get_webview_window("main");
            window.and_then(|w| w.is_focused().ok()) == Some(true)
        };
        let due = app
            .state::<Notices>()
            .lock()
            .unwrap()
            .due(millis(), focused);
        if let Some((title, body)) = due {
            let _ = app.notification().builder().title(title).body(body).show();
        }
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        sent.cards(app);
        live = true;
        thread::sleep(Duration::from_millis(200));
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
        let written = match card {
            true => write_stop(&store, &*writer, request, &session, log),
            false => write_brief(&store, &*writer, request, &session, log),
        };
        if written {
            let _ = app.emit("sessions", sessions(app.state(), app.state()));
        }
        if written && card {
            tell(&app, "New card", &session.agent.id, agent.as_deref());
        }
    };
    let queue = queues.entry(id).or_insert_with(|| {
        let (queue, jobs) = mpsc::channel::<Job>();
        thread::spawn(move || jobs.into_iter().for_each(|job| job()));
        queue
    });
    let _ = queue.send(Box::new(job));
}

/// Adds a notice on session `session_id`'s line, naming its subagent `agent`, if any; the read loop
/// shows it. Called without the core's lock held.
fn tell(app: &AppHandle, title: &str, session_id: &str, agent: Option<&str>) {
    let body = app
        .state::<Mutex<Studio>>()
        .lock()
        .unwrap()
        .notice_body(session_id, agent);
    app.state::<Notices>()
        .lock()
        .unwrap()
        .push(title, body, millis());
}
