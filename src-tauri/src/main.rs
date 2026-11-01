//! The studio app's shell (#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line live and hands the core's card requests to the card writer. The
//! window lists every session (#369, decision 18) and shows the one Daniel selected: its guess
//! prompt on its check, its cards, which the card store keeps on disk, and its check lines. A
//! session that newly needs Daniel gets a macOS notification while the window is not focused.

use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use studio_core::{Output, Session, Studio};
use studio_understanding::{
    CardStore, CardWriter, ClaudeWriter, FakeWriter, Fingerprints, SessionCards, find_claude,
    millis, write_card,
};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// The card store, which the log thread adds to and the pane reveals in.
type Cards = Mutex<CardStore>;

/// The session list, in its order.
#[tauri::command]
fn sessions(studio: State<Mutex<Studio>>) -> Vec<Session> {
    studio
        .lock()
        .unwrap()
        .sessions()
        .into_iter()
        .cloned()
        .collect()
}

/// Daniel clicked session `id`'s row.
#[tauri::command]
fn select(id: String, studio: State<Mutex<Studio>>) {
    studio.lock().unwrap().select(&id);
}

/// The session Daniel selected, or `None` before his first click.
#[tauri::command]
fn selected(studio: State<Mutex<Studio>>) -> Option<Session> {
    studio.lock().unwrap().selected().cloned()
}

/// The cards of the selected session, split at the latest tick.
#[tauri::command]
fn cards(studio: State<Mutex<Studio>>, cards: State<Cards>) -> SessionCards {
    let (selected, now) = {
        let studio = studio.lock().unwrap();
        (studio.selected().map(|s| s.id.clone()), studio.now())
    };
    match selected {
        Some(id) => cards.lock().unwrap().session_cards(&id, now),
        None => SessionCards::default(),
    }
}

/// The pane drew the question of card `id` in a visible window.
#[tauri::command]
fn shown(id: String, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().show(&id, millis()));
}

/// Daniel revealed the answer of card `id`.
#[tauri::command]
fn reveal(id: String, cards: State<Cards>) {
    unsaved(cards.lock().unwrap().reveal(&id, millis()));
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
    let _ = app.emit("selected", selected(studio));
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
    let studio = Studio::default();
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
        .invoke_handler(tauri::generate_handler![
            sessions, select, selected, cards, shown, reveal, guess
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
    // Each session's own, so another session's prompt in the same worktree moves no base.
    let mut fingerprints = HashMap::<String, Fingerprints>::new();
    let mut drawn_cards = None;
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
                    let found = state.lock().unwrap().fingerprint_folder(&text);
                    let change = found.and_then(|(session, folder)| {
                        let prints = fingerprints.entry(session).or_default();
                        prints.take(Path::new(&folder))
                    });
                    outputs.extend(state.lock().unwrap().apply_with_change(&text, change));
                } else {
                    state.lock().unwrap().replay(&text);
                }
                line.clear();
            }
            // A worktree's first fingerprint only records it, so the turns still open after the
            // replay get their base now, and their `Stop` has something to compare with.
            if !live {
                let waiting: Vec<(String, String)> = state
                    .lock()
                    .unwrap()
                    .waiting_folders()
                    .map(|(session, folder)| (session.into(), folder.into()))
                    .collect();
                for (session, folder) in waiting {
                    let prints = fingerprints.entry(session).or_default();
                    prints.take(Path::new(&folder));
                }
            }
            for output in outputs {
                match output {
                    // `claude -p` takes seconds, so each card is written on its own thread, and the
                    // loop below draws it once it is in the store.
                    Output::Card(request) => {
                        let (app, writer) = (app.clone(), writer.clone());
                        thread::spawn(move || match write_card(&*writer, &request) {
                            Ok(card) => {
                                let store = app.state::<Cards>();
                                unsaved(store.lock().unwrap().add(request, card, millis()));
                            }
                            Err(line) => eprintln!("studio: {line}"),
                        });
                    }
                    // The pane draws the open prompt from the session, which the "selected" event
                    // below carries, so it also closes when the result arrives.
                    Output::Guess(_) => {}
                    Output::NeedsYou { session_id } => notify(app, &session_id),
                }
            }
            read = read_to > read_from;
        }
        if state.lock().unwrap().tick(millis()) || read {
            let _ = app.emit("sessions", sessions(app.state()));
            let _ = app.emit("selected", selected(state));
        }
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        let shown = cards(app.state(), app.state());
        if drawn_cards.as_ref() != Some(&shown) {
            let _ = app.emit("cards", &shown);
            drawn_cards = Some(shown);
        }
        live = true;
        thread::sleep(Duration::from_millis(200));
    }
}

/// One notification per notice, only while the window is not focused (#328, module 9). Called
/// without the core's lock held, because asking for focus waits on the main thread, which runs the
/// pane's commands.
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
