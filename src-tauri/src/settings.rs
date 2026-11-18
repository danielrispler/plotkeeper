//! Settings and keymap (danielrispler/Falinks#524): `settings.json` and `keymap.json` in the studio's data directory. The
//! shell hands the window their text, at start and each time one is saved; the window parses them
//! (`ui/settings.ts`), so a broken file is reported there and the last good one stays in effect.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    thread,
    time::Duration,
};
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

/// One of the two files, in the order of `NAMES`.
#[derive(Clone, Copy)]
enum Kind {
    Settings,
    Keymap,
}

/// Opens one of the two files in Daniel's editor (`studio::OpenSettingsFile`, the settings page's rows), first creating it
/// empty when missing; an empty file is the defaults.
fn open_in_editor(kind: Kind, editor: Option<Vec<String>>) -> Result<(), String> {
    let path = paths().ok_or("no home folder")?[kind as usize].clone();
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

/// `text` with the setting at `path` set to `value`, or removed when `value` is none, as one minimal edit that leaves
/// every other byte alone. Refuses a text that does not parse.
fn edit_text(
    text: &str,
    path: &[String],
    value: Option<&serde_json::Value>,
) -> Result<String, String> {
    use jsonc_parser::{
        ParseOptions,
        cst::{CstInputValue, CstObject, CstRootNode},
    };
    let (leaf, parents) = path.split_last().ok_or("no setting named")?;
    let value = value
        .map(|value| match value {
            serde_json::Value::Bool(b) => Ok(CstInputValue::from(*b)),
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(CstInputValue::from)
                .ok_or("a number out of range".to_string()),
            serde_json::Value::String(s) => Ok(CstInputValue::from(s.as_str())),
            other => Err(format!("a {other} is not a setting the page writes")),
        })
        .transpose()?;
    // An empty file is the defaults: an empty object.
    let source = if text.trim().is_empty() { "{}" } else { text };
    // Only what the page's parser reads (`ui/settings.ts`): comments and trailing commas, nothing else JSON5 allows.
    let jsonc = ParseOptions {
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
        ..ParseOptions::default()
    };
    let root = CstRootNode::parse(source, &jsonc)
        .map_err(|e| format!("settings.json could not be parsed: {e}"))?;
    let mut object = root
        .object_value()
        .ok_or("settings.json is not an object")?;
    let mut walked = String::new();
    let step =
        |object: &CstObject, name: &str, walked: &str| -> Result<Option<CstObject>, String> {
            match (object.get(name), value.is_some()) {
                (None, true) => Ok(object.object_value_or_create(name)),
                (None, false) => Ok(None),
                (Some(_), _) => object
                    .object_value(name)
                    .map(Some)
                    .ok_or(format!("{walked}{name} is not an object")),
            }
        };
    for name in parents {
        match step(&object, name, &walked)? {
            Some(inner) => object = inner,
            // Nothing to reset under a parent that is not there.
            None => return Ok(text.to_string()),
        }
        walked.push_str(&format!("{name}."));
    }
    match (object.get(leaf), value) {
        (Some(prop), Some(value)) => prop.set_value(value),
        (Some(prop), None) => prop.remove(),
        (None, Some(value)) => {
            object.append(leaf, value);
        }
        (None, None) => return Ok(text.to_string()),
    }
    Ok(root.to_string())
}

/// Where a write to `file` goes: the end of a chain of symbolic links (dotfiles keep `settings.json` as one), which may not
/// exist yet.
fn resolved(file: &Path) -> PathBuf {
    let mut at = file.to_path_buf();
    // A loop of links gives up after as many steps as the system follows.
    for _ in 0..40 {
        let Ok(to) = fs::read_link(&at) else { break };
        at = at.parent().unwrap_or(Path::new("")).join(to);
    }
    at
}

/// Saves `text` as `file` whole: written beside it and renamed over it, so a crash never leaves it cut short. A link is
/// written through to its target and stays a link.
fn save(file: &Path, text: &str) -> Result<(), String> {
    let target = resolved(file);
    if let Some(dir) = target.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut name = target.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.tmp", std::process::id()));
    let temp = target.with_file_name(name);
    let written = fs::write(&temp, text)
        .and_then(|()| match fs::metadata(&target) {
            // The file keeps the mode it had.
            Ok(meta) => fs::set_permissions(&temp, meta.permissions()),
            Err(_) => Ok(()),
        })
        .and_then(|()| fs::rename(&temp, &target));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written.map_err(|e| e.to_string())
}

/// The write: edits `file`'s text as `edit_text` does and saves it, returning the new text. `expected` is the text the page last
/// read (none: no file); a file that differs now, such as one saved from an editor meanwhile, is refused and left as it
/// was, so the page can show the fresh text and Daniel try again. A missing file is created.
fn write_in(
    file: &Path,
    expected: Option<&str>,
    path: &[String],
    value: Option<&serde_json::Value>,
) -> Result<String, String> {
    let now = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.to_string()),
    };
    if now != expected.unwrap_or_default() {
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        return Err(format!("{name} changed since the page read it: try again"));
    }
    let edited = edit_text(&now, path, value)?;
    if edited != now {
        save(file, &edited)?;
    }
    Ok(edited)
}

/// Held across `write_setting`'s read-compare-save.
static WRITING: Mutex<()> = Mutex::new(());

/// Sets the setting at `path`, such as `["terminal", "font_size"]`, to `value` in `settings.json`, or removes it when `value`
/// is none (the settings page, #195). `expected` is the text the page last read. Returns the new text,
/// or why the write was refused; the written file reaches the window through `watch` like any save.
#[tauri::command(async)]
pub fn write_setting(
    expected: Option<String>,
    path: Vec<String>,
    value: Option<serde_json::Value>,
) -> Result<String, String> {
    let [settings, _] = paths().ok_or("no home folder")?;
    // One write at a time, so two never interleave their read, compare and save.
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    write_in(&settings, expected.as_deref(), &path, value.as_ref())
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

/// The `repository` setting (danielrispler/Falinks#605): the folder the shell, Workspaces and the picker start in. Read from
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
    use super::{edit_text, plain_json, repository_in, write_in};
    use serde_json::json;

    fn path(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    fn parsed(text: &str) -> serde_json::Value {
        serde_json::from_str(&plain_json(text)).unwrap()
    }

    #[test]
    fn a_comment_and_a_trailing_comma_beside_the_edited_key_survive_byte_for_byte() {
        let text = "{\n  // The sounds.\n  \"sounds\": true, /* kept */\n  \"zoom\": 1,\n}\n";
        let edited = edit_text(text, &path(&["sounds"]), Some(&json!(false))).unwrap();
        assert_eq!(
            edited,
            "{\n  // The sounds.\n  \"sounds\": false, /* kept */\n  \"zoom\": 1,\n}\n"
        );
    }

    #[test]
    fn a_value_is_replaced_in_place() {
        let text = "{ \"zoom\": 1, \"terminal\": { \"font_size\": 12 } }";
        let edited = edit_text(text, &path(&["terminal", "font_size"]), Some(&json!(15))).unwrap();
        assert_eq!(
            edited,
            "{ \"zoom\": 1, \"terminal\": { \"font_size\": 15 } }"
        );
        let edited = edit_text(text, &path(&["zoom"]), Some(&json!(1.2))).unwrap();
        assert_eq!(
            edited,
            "{ \"zoom\": 1.2, \"terminal\": { \"font_size\": 12 } }"
        );
        let edited = edit_text(
            r#"{ "summaries": { "model": "haiku" } }"#,
            &path(&["summaries", "model"]),
            Some(&json!("opus")),
        )
        .unwrap();
        assert_eq!(edited, r#"{ "summaries": { "model": "opus" } }"#);
    }

    #[test]
    fn a_key_whose_parent_is_missing_is_inserted_with_it() {
        let text = "{\n  // Mine.\n  \"zoom\": 1.2, // kept\n}\n";
        let edited = edit_text(text, &path(&["terminal", "font_size"]), Some(&json!(15))).unwrap();
        assert!(
            edited.starts_with("{\n  // Mine.\n  \"zoom\": 1.2, // kept\n"),
            "{edited}"
        );
        assert_eq!(
            parsed(&edited),
            json!({ "zoom": 1.2, "terminal": { "font_size": 15 } })
        );
        // A key missing from an existing parent is added beside its siblings.
        let edited = edit_text(
            r#"{ "terminal": { "other": 1 } }"#,
            &path(&["terminal", "font_size"]),
            Some(&json!(15)),
        )
        .unwrap();
        assert_eq!(
            parsed(&edited),
            json!({ "terminal": { "other": 1, "font_size": 15 } })
        );
    }

    #[test]
    fn reset_removes_the_key_and_leaves_the_rest_of_the_text() {
        let text = "{\n  // Mine.\n  \"zoom\": 1.2,\n  \"sounds\": false, // quiet\n}\n";
        let edited = edit_text(text, &path(&["zoom"]), None).unwrap();
        assert_eq!(edited, "{\n  // Mine.\n  \"sounds\": false, // quiet\n}\n");
        // A key that is not there is already reset: the text comes back as it was.
        assert_eq!(
            edit_text(text, &path(&["terminal", "font_size"]), None).unwrap(),
            text
        );
    }

    #[test]
    fn a_text_that_does_not_parse_is_refused_with_its_reason() {
        for (text, key, reason) in [
            ("{ \"zoom\": ", &["zoom"][..], "could not be parsed"),
            ("{ \"zoom\" 1 }", &["zoom"], "could not be parsed"),
            ("[1, 2]", &["zoom"], "settings.json is not an object"),
            (
                "{ \"terminal\": 5 }",
                &["terminal", "font_size"],
                "terminal is not an object",
            ),
        ] {
            let refused = edit_text(text, &path(key), Some(&json!(1))).unwrap_err();
            assert!(refused.contains(reason), "{text}: {refused}");
        }
    }

    #[test]
    fn an_empty_text_is_the_defaults() {
        for text in ["", "  \n"] {
            let edited = edit_text(text, &path(&["zoom"]), Some(&json!(1.5))).unwrap();
            assert_eq!(parsed(&edited), json!({ "zoom": 1.5 }), "{text:?}");
        }
    }

    #[test]
    fn a_write_saves_the_edited_text_and_returns_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let text = "{\n  // Mine.\n  \"sounds\": true,\n}\n";
        std::fs::write(&file, text).unwrap();
        let written = write_in(&file, Some(text), &path(&["sounds"]), Some(&json!(false))).unwrap();
        let on_disk = "{\n  // Mine.\n  \"sounds\": false,\n}\n";
        assert_eq!(written, on_disk);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), on_disk);
    }

    #[test]
    fn a_file_that_differs_from_the_expected_text_is_refused_and_left_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let saved = "{ \"zoom\": 1.5 } // saved from the editor";
        std::fs::write(&file, saved).unwrap();
        let refused = write_in(
            &file,
            Some("{ \"zoom\": 1 }"),
            &path(&["sounds"]),
            Some(&json!(false)),
        )
        .unwrap_err();
        assert!(refused.contains("changed"), "{refused}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), saved);
        // Nor does a file appear where the page read none.
        let refused = write_in(&file, None, &path(&["sounds"]), Some(&json!(false))).unwrap_err();
        assert!(refused.contains("changed"), "{refused}");
    }

    #[test]
    fn a_file_that_does_not_parse_is_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        let broken = "{ \"zoom\": 1,, }";
        std::fs::write(&file, broken).unwrap();
        let refused =
            write_in(&file, Some(broken), &path(&["sounds"]), Some(&json!(false))).unwrap_err();
        assert!(refused.contains("could not be parsed"), "{refused}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), broken);
    }

    #[test]
    fn a_text_that_is_more_than_jsonc_is_refused_and_the_file_left_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        for text in [
            "{\n \"zoom\": 1\n \"sounds\": true\n}",
            "{ zoom: 1 }",
            "{ 'zoom': 1 }",
            "{ \"zoom\": 0x10 }",
            "{ \"zoom\": +1 }",
            "{ \"zoom\": .5 }",
            "{ \"zoom\": NaN }",
        ] {
            std::fs::write(&file, text).unwrap();
            let refused =
                write_in(&file, Some(text), &path(&["sounds"]), Some(&json!(false))).unwrap_err();
            assert!(refused.contains("could not be parsed"), "{text}: {refused}");
            assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        }
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_nothing_beside_it() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");
        std::fs::write(&file, "{ \"sounds\": true }").unwrap();
        let before = std::fs::metadata(&file).unwrap().ino();
        write_in(
            &file,
            Some("{ \"sounds\": true }"),
            &path(&["sounds"]),
            Some(&json!(false)),
        )
        .unwrap();
        // A new file renamed over the old one, so a crash mid-write never leaves a truncated settings.json.
        assert_ne!(std::fs::metadata(&file).unwrap().ino(), before);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
    }

    #[test]
    fn a_settings_file_that_is_a_symlink_is_written_through_and_stays_a_link() {
        let dir = tempfile::tempdir().unwrap();
        let dotfiles = dir.path().join("dotfiles");
        std::fs::create_dir(&dotfiles).unwrap();
        let target = dotfiles.join("settings.json");
        std::fs::write(&target, "{ \"sounds\": true }").unwrap();
        let link = dir.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        write_in(
            &link,
            Some("{ \"sounds\": true }"),
            &path(&["sounds"]),
            Some(&json!(false)),
        )
        .unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(std::fs::read_link(&link).unwrap(), target);
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "{ \"sounds\": false }"
        );
        // A relative link, as dotfile managers make, is followed from the link's folder.
        let relative = dir.path().join("keymap.json");
        std::os::unix::fs::symlink("dotfiles/keymap.json", &relative).unwrap();
        std::fs::write(dotfiles.join("keymap.json"), "{}").unwrap();
        write_in(&relative, Some("{}"), &path(&["zoom"]), Some(&json!(1.5))).unwrap();
        assert!(std::fs::symlink_metadata(&relative).unwrap().is_symlink());
        assert_eq!(
            parsed(&std::fs::read_to_string(dotfiles.join("keymap.json")).unwrap()),
            json!({ "zoom": 1.5 })
        );
    }

    #[test]
    fn a_missing_file_is_created_with_the_one_key() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data").join("settings.json");
        let written = write_in(
            &file,
            None,
            &path(&["terminal", "font_size"]),
            Some(&json!(15)),
        )
        .unwrap();
        assert_eq!(parsed(&written), json!({ "terminal": { "font_size": 15 } }));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), written);
        // Resetting in a missing file creates nothing.
        let none = dir.path().join("none.json");
        write_in(&none, None, &path(&["zoom"]), None).unwrap();
        assert!(!none.exists());
    }

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
