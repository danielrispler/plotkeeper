//! The studio app's shell (#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line live and hands the core's card requests to the card writer. The
//! pane shows the session Daniel prompted last, the guess prompt on its check, and its cards, which
//! the card store keeps on disk; a session that newly needs Daniel gets a macOS notification while
//! the window is not focused.

use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use studio_core::{Output, Session, Studio};
use studio_understanding::{
    CardStore, CardWriter, ClaudeWriter, FakeWriter, Fingerprints, StoredCard, find_claude, millis,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// The card store, which the log thread adds to and the pane reveals in.
type Cards = Mutex<CardStore>;

#[tauri::command]
fn now(studio: State<Mutex<Studio>>) -> Option<Session> {
    studio.lock().unwrap().prompted_last().cloned()
}

/// The cards of the session the pane follows.
#[tauri::command]
fn cards(studio: State<Mutex<Studio>>, cards: State<Cards>) -> Vec<StoredCard> {
    let Some(followed) = studio.lock().unwrap().prompted_last().map(|s| s.id.clone()) else {
        return Vec::new();
    };
    cards
        .lock()
        .unwrap()
        .cards()
        .iter()
        .filter(|kept| kept.request.session_id == followed)
        .cloned()
        .collect()
}

/// The pane drew the question of the card on step `tool_use_id` in a visible window.
#[tauri::command]
fn shown(tool_use_id: String, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().show(&tool_use_id, millis()));
}

/// Daniel revealed the answer of the card on step `tool_use_id`.
#[tauri::command]
fn reveal(tool_use_id: String, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().reveal(&tool_use_id, millis()));
}

/// Logs a card store change that could not be saved; it stays in memory until the next save.
fn unsaved(saved: io::Result<()>) {
    if let Err(e) = saved {
        eprintln!("studio: save the card store: {e}");
    }
}

/// Daniel tapped yes or no on "Will it pass?" for check `tool_use_id`.
#[tauri::command]
fn guess(tool_use_id: String, will_pass: bool, app: AppHandle) {
    let studio = app.state::<Mutex<Studio>>();
    studio.lock().unwrap().guess(&tool_use_id, will_pass);
    let _ = app.emit("now", now(studio));
}

/// The card writer: `claude -p`, or the fake when `STUDIO_CARD_WRITER=fake`, for checks that must
/// not run `claude -p`.
type Writer = Arc<dyn CardWriter + Send + Sync>;

fn main() {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    // The store sits beside the log, in the studio's data directory.
    let store = CardStore::open_beside(&log).expect("read the card store");
    // The card writer's neutral folder, beside the log.
    let folder = log.with_file_name("card-writer");
    fs::create_dir_all(&folder).expect("create the card writer's folder");
    let mut studio = Studio::default();
    let carded = store
        .cards()
        .iter()
        .map(|kept| kept.request.tool_use_id.clone());
    studio.set_carded_steps(carded);
    let writer: Writer = match std::env::var("STUDIO_CARD_WRITER").as_deref() {
        Ok("fake") => Arc::new(FakeWriter),
        _ => {
            let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
            Arc::new(ClaudeWriter::new(find_claude(shell.as_ref()), folder))
        }
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(Mutex::new(studio))
        .manage(Cards::new(store))
        .invoke_handler(tauri::generate_handler![now, cards, shown, reveal, guess])
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
/// get the worktree fingerprint the core asks for; replayed ones get none.
// shortcut: polls every 200 ms; switch to file-system events if the delay or the wake-ups matter.
fn follow(app: &AppHandle, log: &Path, writer: &Writer) {
    let mut read_to = 0;
    let mut live = false;
    let mut fingerprints = Fingerprints::default();
    let mut followed = None;
    loop {
        if let Ok(mut file) = File::open(log)
            && file.seek(SeekFrom::Start(read_to)).is_ok()
        {
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            let mut outputs = Vec::new();
            let read_from = read_to;
            let state = app.state::<Mutex<Studio>>();
            // The core is locked per line, and not while the fingerprint runs `git`, so the pane's
            // commands do not wait on it.
            while reader.read_until(b'\n', &mut line).unwrap_or(0) > 0 && line.ends_with(b"\n") {
                read_to += line.len() as u64;
                let text = String::from_utf8_lossy(&line);
                if live {
                    // shortcut: fingerprints when the line is read, up to one poll after the step, so
                    // a step that starts in between counts toward this one; take the fingerprint in
                    // the hook if a card ever explains the wrong step.
                    let folder = state.lock().unwrap().fingerprint_folder(&text);
                    let change = folder.and_then(|folder| fingerprints.take(Path::new(&folder)));
                    outputs.extend(state.lock().unwrap().apply_with_change(&text, change));
                } else {
                    state.lock().unwrap().replay(&text);
                }
                line.clear();
            }
            // A worktree's first fingerprint only records it, so the waits still open after the
            // replay get theirs now, and their next Bash step has something to compare with.
            if !live {
                let waiting: Vec<String> = state
                    .lock()
                    .unwrap()
                    .waiting_folders()
                    .map(Into::into)
                    .collect();
                for folder in waiting {
                    fingerprints.take(Path::new(&folder));
                }
            }
            for output in outputs {
                match output {
                    // `claude -p` takes seconds, so each card is written on its own thread and shown
                    // when it is done.
                    Output::Card(request) => {
                        let (app, writer) = (app.clone(), writer.clone());
                        thread::spawn(move || {
                            let Some(card) = writer.write(&request) else {
                                return;
                            };
                            let store = app.state::<Cards>();
                            unsaved(store.lock().unwrap().add(request, card, millis()));
                            let _ = app.emit("cards", cards(app.state(), app.state()));
                        });
                    }
                    // The pane draws the open prompt from the session, which the "now" event below
                    // carries, so it also closes when the result arrives.
                    Output::Guess(_) => {}
                    Output::NeedsYou { session_id } => notify(app, &session_id),
                }
            }
            if read_to > read_from {
                let session = state.lock().unwrap().prompted_last().map(|s| s.id.clone());
                let _ = app.emit("now", now(state));
                // A new card emits the cards itself, so they change here only with the session.
                if session != followed {
                    followed = session;
                    let _ = app.emit("cards", cards(app.state(), app.state()));
                }
            }
        }
        live = true;
        thread::sleep(Duration::from_millis(200));
    }
}

/// One notification per notice, only while the window is not focused (#328, module 9). Called
/// without the core's lock held, because asking for focus waits on the main thread, which runs `now`.
fn notify(app: &AppHandle, session_id: &str) {
    let focused = app
        .get_webview_window("main")
        .and_then(|w| w.is_focused().ok());
    if focused == Some(true) {
        return;
    }
    let studio = app.state::<Mutex<Studio>>();
    let cwd = studio
        .lock()
        .unwrap()
        .session(session_id)
        .map(|s| s.cwd.clone());
    let folder = cwd.as_deref().and_then(|cwd| cwd.rsplit('/').next());
    let _ = app
        .notification()
        .builder()
        .title("Needs you")
        .body(folder.unwrap_or_default())
        .show();
}
