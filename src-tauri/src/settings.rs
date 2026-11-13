//! Settings and keymap (danielrispler/Falinks#524): `settings.json` and `keymap.json` in the studio's data directory. The
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

/// Opens `settings.json` in Daniel's editor (⌘,, danielrispler/Falinks#575), first creating it empty when missing; an
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

/// The `repository` setting (danielrispler/Falinks#605): the folder the shell, Places and the picker start in. Read from
/// the file on each call, so a saved change takes effect; `None` when unset, empty or not a string.
pub fn repository() -> Option<String> {
    let [settings, _] = paths()?;
    repository_in(&fs::read_to_string(settings).ok()?)
}

fn repository_in(text: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).ok()?;
    let repo = json.get("repository")?.as_str()?;
    (!repo.is_empty()).then(|| repo.to_string())
}

/// Zed's format as plain JSON: comments and trailing commas outside strings dropped, as
/// `ui/settings.ts`'s `parseJsonc` does.
fn plain_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                out.push(c);
                while let Some(c) = chars.next() {
                    out.push(c);
                    match c {
                        '\\' => out.extend(chars.next()),
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => while chars.next_if(|&c| c != '\n').is_some() {},
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut last = ' ';
                for c in chars.by_ref() {
                    if last == '*' && c == '/' {
                        break;
                    }
                    last = c;
                }
            }
            ',' => {
                let rest = chars.clone().find(|c| !c.is_whitespace());
                if !matches!(rest, Some('}' | ']')) {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::repository_in;

    #[test]
    fn the_repository_setting_reads_zeds_format() {
        let text = r#"{
            // The engine, not the studio's own checkout.
            "repository": "/Users/d/work/Falinks", /* a comment */
            "editor": ["zed", "{path}//x"],
        }"#;
        assert_eq!(
            repository_in(text).as_deref(),
            Some("/Users/d/work/Falinks")
        );
        assert_eq!(
            repository_in(r#"{ "repository": "/a//b,}" }"#).as_deref(),
            Some("/a//b,}")
        );
    }

    #[test]
    fn an_unset_empty_or_wrong_repository_setting_is_none() {
        for text in [
            "",
            "{}",
            r#"{"repository": ""}"#,
            r#"{"repository": 3}"#,
            "{ broken",
        ] {
            assert_eq!(repository_in(text), None, "{text}");
        }
    }
}
