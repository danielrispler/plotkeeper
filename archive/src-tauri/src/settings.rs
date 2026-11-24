//! The shell's side of settings and keymap (danielrispler/Falinks#524): the commands that open the files in an editor, the
//! write command's wrapper, the watch that tells the window a file changed and the window's zoom. Reading and writing the
//! files is `studio_server::settings`.

use std::{fs, thread, time::Duration};
use tauri::{AppHandle, Emitter, WebviewWindow};

/// Each file's text, null when it is missing.
#[tauri::command]
pub fn settings_files() -> serde_json::Value {
    studio_server::settings::settings_files()
}

/// One of the two files, in the order of `studio_server::settings::paths`.
#[derive(Clone, Copy)]
enum Kind {
    Settings,
    Keymap,
}

/// Opens one of the two files in Daniel's editor (`studio::OpenSettingsFile`, the settings page's rows), first creating it
/// empty when missing; an empty file is the defaults.
fn open_in_editor(kind: Kind, editor: Option<Vec<String>>) -> Result<(), String> {
    let path = studio_server::settings::paths().ok_or("no home folder")?[kind as usize].clone();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Never truncates: a file saved meanwhile stays.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.to_string()),
    }
    crate::terminal::open_file(path.display().to_string(), 1, 1, None, editor)
}

/// Opens `settings.json` (`studio::OpenSettingsFile`, and the settings page's row).
#[tauri::command(async)]
pub fn open_settings(editor: Option<Vec<String>>) -> Result<(), String> {
    open_in_editor(Kind::Settings, editor)
}

/// Opens `keymap.json`, the same way.
#[tauri::command(async)]
pub fn open_keymap(editor: Option<Vec<String>>) -> Result<(), String> {
    open_in_editor(Kind::Keymap, editor)
}

/// Sets the setting at `path` in `settings.json` (the settings page, #195).
#[tauri::command(async)]
pub fn write_setting(
    expected: Option<String>,
    path: Vec<String>,
    value: Option<serde_json::Value>,
) -> Result<String, String> {
    studio_server::settings::write_setting(expected, path, value)
}

/// Sends `settings-files` when either file is saved, created or removed.
// shortcut: polls every 500 ms, as `restart::watch_build` does; switch to file-system events if
// the delay matters.
pub fn watch(app: AppHandle) {
    let Some(paths) = studio_server::settings::paths() else {
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
