//! Settings and keymap (danielrispler/Falinks#524): `settings.json` and `keymap.json` in the studio's data directory. The
//! shell hands the window their text, at start and each time one is saved; the window parses them
//! (`ui/settings.ts`), so a broken file is reported there and the last good one stays in effect.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use studio_understanding::WriterKind;

const NAMES: [&str; 2] = ["settings.json", "keymap.json"];

/// The two files' paths, in the order of `NAMES`; `None` with no home folder.
pub fn paths() -> Option<[PathBuf; 2]> {
    let dir = studio_hook::data_dir(&std::env::home_dir()?);
    Some(NAMES.map(|name| dir.join(name)))
}

/// Each file's text, null when it is missing.
pub fn settings_files() -> serde_json::Value {
    let [settings, keymap] = paths()
        .map(|p| p.map(|p| fs::read_to_string(p).ok()))
        .unwrap_or_default();
    serde_json::json!({ "settings": settings, "keymap": keymap })
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
/// is none. `expected` is the text the page last read. Returns the new text,
/// or why the write was refused; the written file reaches the window through `watch` like any save.
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

/// Saves the writer the page's Test passed, with its model and effort (an empty effort: the key removed, the model's
/// default), in `settings.json`, under the one lock of the page's writes. `expected` is the text the page last read. Returns the new text.
pub fn save_writer(
    expected: Option<String>,
    writer: studio_understanding::WriterKind,
    model: &str,
    effort: &str,
) -> Result<String, String> {
    let [settings, _] = paths().ok_or("no home folder")?;
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    save_writer_in(&settings, expected.as_deref(), writer, model, effort)
}

fn save_writer_in(
    settings: &Path,
    expected: Option<&str>,
    writer: studio_understanding::WriterKind,
    model: &str,
    effort: &str,
) -> Result<String, String> {
    let key = |leaf: &str| vec!["summaries".to_string(), leaf.to_string()];
    let text = write_in(
        settings,
        expected,
        &key("writer"),
        // The serde name `summary_writer` reads back: Cursor's program is `agent`, its name `cursor`.
        Some(&serde_json::json!(writer)),
    )?;
    let text = write_in(
        settings,
        Some(&text),
        &key("model"),
        Some(&serde_json::json!(model)),
    )?;
    let effort = (!effort.is_empty()).then(|| serde_json::json!(effort));
    write_in(settings, Some(&text), &key("effort"), effort.as_ref())
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

/// The card writer `settings.json` names, `summaries.writer`: `claude` unless the file says `agy`, or is missing or broken
/// (danielrispler/plotkeeper#325: the web server applies it at start, as the page's first save did in the app).
pub fn summary_writer() -> studio_understanding::WriterKind {
    let text = paths().and_then(|[settings, _]| fs::read_to_string(settings).ok());
    summary_writer_in(&text.unwrap_or_default())
}

fn summary_writer_in(text: &str) -> studio_understanding::WriterKind {
    serde_json::from_str::<serde_json::Value>(&plain_json(text))
        .ok()
        .and_then(|json| serde_json::from_value(json["summaries"]["writer"].clone()).ok())
        .unwrap_or_default()
}

/// The OpenAI-compatible writer's base URL, `summaries.base_url` in `settings.json` (#367), read from the file on each call;
/// `None` when unset, empty or not a string: OpenAI's own.
pub fn openai_base_url() -> Option<String> {
    let text = paths().and_then(|[settings, _]| fs::read_to_string(settings).ok());
    base_url_in(&text.unwrap_or_default())
}

fn base_url_in(text: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).ok()?;
    let url = json["summaries"]["base_url"].as_str()?.trim();
    (!url.is_empty()).then(|| url.to_string())
}

/// Saves the OpenAI-compatible writer's base URL in `settings.json`, or removes it when none; `expected` is the text the
/// page last read. Returns the new text.
pub fn save_base_url(expected: Option<String>, url: Option<&str>) -> Result<String, String> {
    let path = vec!["summaries".to_string(), "base_url".to_string()];
    write_setting(expected, path, url.map(|url| serde_json::json!(url)))
}

/// Whether summaries are written at this start (spec #320, "The Test gate"): yes for a user who was configured before, or
/// once a writer passed Test; else no, until one does. Read at start, before the writer makes its folder.
///
/// Today a missing `summaries.writer` reads as Claude, so the key alone cannot tell who was configured: a user is, when
/// `settings.json` names `summaries.writer`, or when the data folder held the writer's folder (`card-writer`, which only a
/// start of plotkeeper or the app before it makes) at the first start of this version. That first look is kept in
/// `writer-gate.json`, so later starts, after the writer has made its folder, read the same answer.
pub fn summaries_open(data: &Path) -> bool {
    let gate = data.join(GATE);
    let open = match fs::read_to_string(&gate)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| json["open"].as_bool())
    {
        Some(open) => open,
        None => {
            let open = data
                .join(studio_understanding::WriterKind::Claude.folder())
                .is_dir();
            if let Err(e) = save(&gate, &serde_json::json!({ "open": open }).to_string()) {
                log::error!("save {GATE}: {e}");
            }
            open
        }
    };
    open || names_writer(&fs::read_to_string(data.join(NAMES[0])).unwrap_or_default())
}

/// The file that keeps whether summaries are written without a Test, beside the settings.
const GATE: &str = "writer-gate.json";

/// A writer passed Test: summaries are written from now on, at every start.
pub fn open_summaries(data: &Path) -> Result<(), String> {
    save(
        &data.join(GATE),
        &serde_json::json!({ "open": true }).to_string(),
    )
}

/// Whether `settings.json`'s text names a writer in `summaries.writer`.
fn names_writer(text: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(&plain_json(text))
        .ok()
        .and_then(|json| {
            serde_json::from_value::<studio_understanding::WriterKind>(
                json["summaries"]["writer"].clone(),
            )
            .ok()
        })
        .is_some()
}

/// The catch-up at start from `settings.json`: `catchUp.hours` (default 2), how far back a ready PR's head may have moved for its
/// brief to be written at start (danielrispler/plotkeeper#334); the default where the file leaves it out or gives something else than a
/// number.
pub fn catch_up() -> studio_core::CatchUp {
    let text = paths().and_then(|[settings, _]| fs::read_to_string(settings).ok());
    catch_up_in(&text.unwrap_or_default())
}

fn catch_up_in(text: &str) -> studio_core::CatchUp {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap_or_default();
    let mut catch_up = studio_core::CatchUp::default();
    if let Some(hours) = json["catchUp"]["hours"].as_f64().filter(|h| *h >= 0.0) {
        catch_up.window_ms = (hours * 3_600_000.0) as u64;
    }
    catch_up
}

/// The model of the PR briefs' writer and claim check, `prBriefs.model` (default Opus), read from the file on each call so a saved
/// change takes effect (danielrispler/plotkeeper#334).
pub fn pr_briefs_model() -> String {
    let text = paths().and_then(|[settings, _]| fs::read_to_string(settings).ok());
    pr_briefs_model_in(&text.unwrap_or_default())
}

fn pr_briefs_model_in(text: &str) -> String {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap_or_default();
    match json["prBriefs"]["model"].as_str() {
        Some(model) if !model.trim().is_empty() => model.trim().to_string(),
        _ => studio_understanding::pr_brief::DEFAULT_MODEL.to_string(),
    }
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

/// Where the API-key writers' keys are kept (spec #320, "API-key writers"): the macOS Keychain under plotkeeper's own
/// entries, one per writer, or memory in a test. Never another tool's config or the environment. A key goes in and is read
/// back by the writer; the page only ever learns whether one is set.
pub trait ApiKeys: Send + Sync {
    /// Writer `kind`'s key, when one is set.
    fn get(&self, kind: WriterKind) -> Option<String>;
    /// Saves `key` as writer `kind`'s, replacing the one set.
    fn set(&self, kind: WriterKind, key: &str) -> Result<(), String>;
    /// Removes writer `kind`'s key; none set is no error.
    fn remove(&self, kind: WriterKind) -> Result<(), String>;
}

/// The Keychain entries: service `plotkeeper`, account `anthropic-api-key` or `openai-api-key`, through the `security`
/// program, which only ever touches these entries.
pub struct Keychain {
    program: PathBuf,
}

const SERVICE: &str = "plotkeeper";

/// Writer `kind`'s Keychain account: `anthropic-api-key` or `openai-api-key`.
fn account(kind: WriterKind) -> String {
    format!("{}-api-key", kind.program())
}

impl Default for Keychain {
    fn default() -> Self {
        Self::with_program("/usr/bin/security".into())
    }
}

impl Keychain {
    /// The Keychain through `program` in place of `security`, for a test, which must not touch the real Keychain.
    pub fn with_program(program: PathBuf) -> Self {
        Self { program }
    }

    fn run(&self, args: &[&str], stdin: Option<&str>) -> Result<std::process::Output, String> {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let mut child = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("the Keychain could not be reached: {e}"))?;
        let mut input = child.stdin.take().expect("piped");
        if let Some(stdin) = stdin {
            input
                .write_all(stdin.as_bytes())
                .map_err(|e| format!("the Keychain could not be reached: {e}"))?;
        }
        drop(input);
        child
            .wait_with_output()
            .map_err(|e| format!("the Keychain could not be reached: {e}"))
    }
}

/// Whether `key` looks like an API key: letters, digits, `-` and `_` only, so it goes to `security` unquoted and whole.
fn plain_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 512
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

impl ApiKeys for Keychain {
    fn get(&self, kind: WriterKind) -> Option<String> {
        let account = account(kind);
        let out = self
            .run(
                &["find-generic-password", "-s", SERVICE, "-a", &account, "-w"],
                None,
            )
            .ok()
            .filter(|out| out.status.success())?;
        let key = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!key.is_empty()).then_some(key)
    }

    fn set(&self, kind: WriterKind, key: &str) -> Result<(), String> {
        if !plain_key(key) {
            return Err("That is not an API key: it holds only letters, digits, - and _.".into());
        }
        // In interactive mode the key goes on standard input, so it never shows in the process list.
        let account = account(kind);
        let command = format!("add-generic-password -U -s {SERVICE} -a {account} -w {key}\n");
        let out = self.run(&["-i"], Some(&command))?;
        match out.status.success() && self.get(kind).as_deref() == Some(key) {
            true => Ok(()),
            false => Err("The Keychain did not save the key.".into()),
        }
    }

    fn remove(&self, kind: WriterKind) -> Result<(), String> {
        let account = account(kind);
        self.run(
            &["delete-generic-password", "-s", SERVICE, "-a", &account],
            None,
        )?;
        match self.get(kind) {
            None => Ok(()),
            Some(_) => Err("The Keychain did not remove the key.".into()),
        }
    }
}

/// Keys kept in memory: a test's, and the server's until it is given the Keychain at start.
#[derive(Default)]
pub struct MemoryKeys(Mutex<std::collections::HashMap<WriterKind, String>>);

impl ApiKeys for MemoryKeys {
    fn get(&self, kind: WriterKind) -> Option<String> {
        self.0.lock().unwrap().get(&kind).cloned()
    }

    fn set(&self, kind: WriterKind, key: &str) -> Result<(), String> {
        if !plain_key(key) {
            return Err("That is not an API key: it holds only letters, digits, - and _.".into());
        }
        self.0.lock().unwrap().insert(kind, key.into());
        Ok(())
    }

    fn remove(&self, kind: WriterKind) -> Result<(), String> {
        self.0.lock().unwrap().remove(&kind);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        catch_up_in, edit_text, plain_json, pr_briefs_model_in, repository_in, save_writer_in,
        summary_writer_in, write_in,
    };
    use serde_json::json;
    use studio_understanding::WriterKind;

    #[test]
    fn a_saved_writer_is_read_back_as_the_same_writer() {
        let home = tempfile::tempdir().unwrap();
        let file = home.path().join("settings.json");
        for kind in WriterKind::ALL {
            let text = save_writer_in(&file, None, kind, "auto", "").unwrap();
            assert_eq!(summary_writer_in(&text), kind, "{text}");
            std::fs::remove_file(&file).unwrap();
        }
    }

    #[test]
    fn a_fresh_home_gets_no_summaries_until_a_writer_passes_test() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("plotkeeper");
        assert!(!super::summaries_open(&data));
        // The start then makes the writer's folder; the next start is still gated.
        std::fs::create_dir_all(data.join("card-writer")).unwrap();
        assert!(!super::summaries_open(&data));
        super::open_summaries(&data).unwrap();
        assert!(super::summaries_open(&data));
    }

    #[test]
    fn a_data_folder_an_earlier_plotkeeper_used_writes_summaries_with_no_test() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("plotkeeper");
        std::fs::create_dir_all(data.join("card-writer")).unwrap();
        std::fs::write(data.join("events.jsonl"), "").unwrap();
        assert!(super::summaries_open(&data));
        assert!(super::summaries_open(&data), "and at every start after");
    }

    #[test]
    fn a_settings_file_that_names_the_writer_writes_summaries_with_no_test() {
        let home = tempfile::tempdir().unwrap();
        let data = home.path().join("plotkeeper");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("settings.json"),
            "{ \"summaries\": { \"writer\": \"agy\", }, }",
        )
        .unwrap();
        assert!(super::summaries_open(&data));
        // A key that names no writer is not that.
        std::fs::write(
            data.join("settings.json"),
            r#"{ "summaries": { "model": "haiku" } }"#,
        )
        .unwrap();
        assert!(!super::summaries_open(&data));
    }

    fn path(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| k.to_string()).collect()
    }

    fn parsed(text: &str) -> serde_json::Value {
        serde_json::from_str(&plain_json(text)).unwrap()
    }

    #[test]
    fn the_catch_up_is_two_hours_unless_the_settings_say_otherwise() {
        let default = studio_core::CatchUp::default();
        assert_eq!(default.window_ms, 7_200_000);
        assert_eq!(catch_up_in(""), default);
        assert_eq!(
            catch_up_in("{ \"catchUp\": { \"hours\": \"x\" } }"),
            default
        );
        let set = catch_up_in("{ // wider\n \"catchUp\": { \"hours\": 6, }, }");
        assert_eq!(set.window_ms, 21_600_000);
    }

    #[test]
    fn the_pr_briefs_model_is_opus_unless_the_settings_name_another() {
        assert_eq!(pr_briefs_model_in(""), "opus");
        assert_eq!(pr_briefs_model_in("not json"), "opus");
        assert_eq!(
            pr_briefs_model_in(r#"{ "prBriefs": { "model": "" } }"#),
            "opus"
        );
        assert_eq!(
            pr_briefs_model_in(r#"{ "prBriefs": { "model": 3 } }"#),
            "opus"
        );
        let set = "{ // the briefs\n \"prBriefs\": { \"model\": \"sonnet\", }, }";
        assert_eq!(pr_briefs_model_in(set), "sonnet");
        // The summaries' model is its own setting.
        assert_eq!(
            pr_briefs_model_in(r#"{ "summaries": { "model": "haiku" } }"#),
            "opus"
        );
    }

    #[test]
    fn the_summary_writer_is_claude_unless_the_settings_say_agy() {
        let agy = "{ // cards on my subscription\n \"summaries\": { \"writer\": \"agy\", }, }";
        assert_eq!(summary_writer_in(agy), WriterKind::Agy);
        assert_eq!(
            summary_writer_in(r#"{ "summaries": { "model": "haiku" } }"#),
            WriterKind::Claude
        );
        assert_eq!(
            summary_writer_in(r#"{ "summaries": { "writer": "nope" } }"#),
            WriterKind::Claude
        );
        assert_eq!(summary_writer_in("not json"), WriterKind::Claude);
        assert_eq!(summary_writer_in(""), WriterKind::Claude);
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

    /// The OpenAI-compatible writer's base URL (#367): a non-empty string under `summaries`, in Zed's format.
    #[test]
    fn the_base_url_is_read_from_summaries() {
        use super::base_url_in;
        let read = |text: &str| base_url_in(text);
        assert_eq!(
            read("{ // Ollama\n \"summaries\": { \"base_url\": \"http://localhost:11434/v1\", } }")
                .as_deref(),
            Some("http://localhost:11434/v1")
        );
        for text in [
            "",
            "{}",
            r#"{ "summaries": { "base_url": "" } }"#,
            r#"{ "summaries": { "base_url": 1 } }"#,
            r#"{ "base_url": "http://x" }"#,
        ] {
            assert_eq!(read(text), None, "{text}");
        }
    }

    /// The Keychain through a stand-in `security` that keeps one entry in a file: the key goes in on standard input, never
    /// in the arguments, and only plotkeeper's own entry is named. The real Keychain is never touched.
    #[test]
    fn the_key_is_saved_replaced_and_removed_in_plotkeepers_own_keychain_entry() {
        use super::{ApiKeys, Keychain};
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let (store, log) = (dir.path().join("store"), dir.path().join("log"));
        let program = dir.path().join("security");
        std::fs::write(
            &program,
            format!(
                r#"#!/bin/sh
echo "ARGS $*" >> '{log}'
case "$1" in
  -i) read line; for w in $line; do last=$w; done; echo "$last" > '{store}';;
  find-generic-password) [ -f '{store}' ] && cat '{store}' || exit 44;;
  delete-generic-password) [ -f '{store}' ] && rm '{store}' || exit 44;;
esac
"#,
                log = log.display(),
                store = store.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let keys = Keychain::with_program(program);
        use studio_understanding::WriterKind::{Anthropic, OpenAI};

        assert_eq!(keys.get(Anthropic), None);
        keys.set(Anthropic, "sk-ant-first-0001").unwrap();
        assert_eq!(keys.get(Anthropic).as_deref(), Some("sk-ant-first-0001"));
        keys.set(Anthropic, "sk-ant-second-0002").unwrap();
        assert_eq!(keys.get(Anthropic).as_deref(), Some("sk-ant-second-0002"));
        keys.remove(Anthropic).unwrap();
        assert_eq!(keys.get(Anthropic), None);
        keys.remove(Anthropic).unwrap();
        assert!(keys.set(Anthropic, "not a key; rm -rf").is_err());
        // The OpenAI-compatible API's key, in its own entry (#367).
        keys.set(OpenAI, "sk-proj-third-0003").unwrap();
        assert_eq!(keys.get(OpenAI).as_deref(), Some("sk-proj-third-0003"));
        keys.remove(OpenAI).unwrap();

        let log = std::fs::read_to_string(log).unwrap();
        for line in log.lines() {
            assert!(
                !line.contains("sk-ant") && !line.contains("sk-proj"),
                "a key in the arguments: {line}"
            );
            assert!(
                line == "ARGS -i"
                    || line.contains("-s plotkeeper -a anthropic-api-key")
                    || line.contains("-s plotkeeper -a openai-api-key"),
                "{line}"
            );
        }
    }
}
