//! Settings and keymap (#524): `settings.json` and `keymap.json` in the studio's data directory. The
//! shell hands the window their text, at start and each time one is saved; the window parses them
//! (`ui/settings.ts`), so a broken file is reported there and the last good one stays in effect.

use std::{fs, path::PathBuf, thread, time::Duration};
use tauri::{AppHandle, Emitter, WebviewWindow};

const NAMES: [&str; 2] = ["settings.json", "keymap.json"];

fn paths() -> Option<[PathBuf; 2]> {
    let dir = studio_hook::data_dir(&std::env::home_dir()?);
    Some(NAMES.map(|name| dir.join(name)))
}

/// Each file's text, null when it is missing.
#[tauri::command]
pub fn settings_files() -> serde_json::Value {
    let [settings, keymap] = paths()
        .map(|p| p.map(|p| fs::read_to_string(p).ok()))
        .unwrap_or_default();
    serde_json::json!({ "settings": settings, "keymap": keymap })
}

/// Opens `settings.json` in Daniel's editor (⌘,, #575), first creating it empty when missing; an
/// empty file is the defaults.
#[tauri::command(async)]
pub fn open_settings(editor: Option<Vec<String>>) -> Result<(), String> {
    let [settings, _] = paths().ok_or("no home folder")?;
    if let Some(dir) = settings.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Never truncates: a file saved meanwhile stays.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&settings)
    {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    crate::terminal::open_file(settings.display().to_string(), 1, 1, None, editor)
}

/// Sends `settings-files` when either file is saved, created or removed.
// shortcut: polls every 500 ms, as `restart::watch_build` does; switch to file-system events if
// the delay matters.
pub fn watch(app: AppHandle) {
    let Some(paths) = paths() else {
        return;
    };
    let modified = move || {
        paths
            .clone()
            .map(|p| fs::metadata(p).and_then(|m| m.modified()).ok())
    };
    thread::spawn(move || {
        let mut seen = modified();
        loop {
            thread::sleep(Duration::from_millis(500));
            let now = modified();
            if now != seen {
                seen = now;
                let _ = app.emit("settings-files", settings_files());
            }
        }
    });
}

/// The window's zoom, as the window's bounds already clamped it.
#[tauri::command]
pub fn set_zoom(zoom: f64, window: WebviewWindow) -> Result<(), String> {
    window.set_zoom(zoom).map_err(|e| e.to_string())
}
