//! The studio app's shell (#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line. The pane shows the session Daniel prompted last.

use std::{
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
    sync::Mutex,
    thread,
    time::Duration,
};
use studio_core::{Session, Studio};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
fn now(studio: State<Mutex<Studio>>) -> Option<Session> {
    studio.lock().unwrap().prompted_last().cloned()
}

fn main() {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    tauri::Builder::default()
        .manage(Mutex::new(Studio::default()))
        .invoke_handler(tauri::generate_handler![now])
        .setup(move |app| {
            let app = app.handle().clone();
            thread::spawn(move || follow(&app, &log));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run the studio");
}

/// Reads the log from its start, then from where it stopped. A line counts once it ends in a newline,
/// so a line the hook is still writing waits for the next read.
// shortcut: polls every 200 ms; switch to file-system events if the delay or the wake-ups matter.
fn follow(app: &AppHandle, log: &Path) {
    let mut read_to = 0;
    loop {
        if let Ok(mut file) = File::open(log)
            && file.seek(SeekFrom::Start(read_to)).is_ok()
        {
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            let read_from = read_to;
            let state = app.state::<Mutex<Studio>>();
            let mut studio = state.lock().unwrap();
            while reader.read_until(b'\n', &mut line).unwrap_or(0) > 0 && line.ends_with(b"\n") {
                read_to += line.len() as u64;
                studio.apply(&String::from_utf8_lossy(&line));
                line.clear();
            }
            drop(studio);
            if read_to > read_from {
                let _ = app.emit("now", now(state));
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
}
