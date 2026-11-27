//! Settings and keymap (danielrispler/Falinks#524): `settings.json` and `keymap.json` in the studio's data directory. The
//! page asks for their text at start, and the read loop sends it each time a file changes; the page parses them
//! (`ui/settings.ts`), so a broken file is reported there and the last good one stays in effect.

use std::fmt::Write as _;
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
        // Writing to a `String` cannot fail.
        write!(walked, "{name}.").ok();
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
        // Best effort: the write's own error is the one returned.
        fs::remove_file(&temp).ok();
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
/// or why the write was refused; the read loop's next pass sends the written file to every open page (`settings-files`).
pub fn write_setting(
    expected: Option<String>,
    path: Vec<String>,
    value: Option<serde_json::Value>,
) -> Result<String, String> {
    let [settings, _] = paths().ok_or("no home folder")?;
    // One write at a time, so two never interleave their read, compare and save.
    let _one_at_a_time = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    let _one_at_a_time = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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

/// Turns the page's explanations (spec #520, #522) on or off: `explanations` in `settings.json`, under the one lock of the page's
/// writes. `expected` is the text the page last read. Returns the new text.
pub fn save_explanations(expected: Option<String>, on: bool) -> Result<String, String> {
    let [settings, _] = paths().ok_or("no home folder")?;
    let _one_at_a_time = WRITING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    save_explanations_in(&settings, expected.as_deref(), on)
}

fn save_explanations_in(
    settings: &Path,
    expected: Option<&str>,
    on: bool,
) -> Result<String, String> {
    write_in(
        settings,
        expected,
        &["explanations".to_string()],
        Some(&serde_json::json!(on)),
    )
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

/// The brief backfill at start from `settings.json`: `briefBackfill.hours` (default 2), how far back a ready PR's head may have moved
/// for its brief to be written at start (danielrispler/plotkeeper#334); the default where the file leaves it out or gives something
/// else than a number. The key was `catchUp.hours` until "catch-up" came to mean the part catch-up (#391): the old key is still read
/// when the new one is absent.
pub fn brief_backfill() -> studio_core::CatchUp {
    let text = paths().and_then(|[settings, _]| fs::read_to_string(settings).ok());
    brief_backfill_in(&text.unwrap_or_default())
}

fn brief_backfill_in(text: &str) -> studio_core::CatchUp {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap_or_default();
    let mut backfill = studio_core::CatchUp::default();
    let hours = |key: &str| json[key]["hours"].as_f64().filter(|h| *h >= 0.0);
    if let Some(hours) = hours("briefBackfill").or_else(|| hours("catchUp")) {
        #[allow(
            clippy::cast_sign_loss,
            reason = "`hours` is at least 0 (the filter above), and a cast from `f64` saturates"
        )]
        let window_ms = (hours * 3_600_000.0) as u64;
        backfill.window_ms = window_ms;
    }
    backfill
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

const PR_BRIEFS_EFFORT: &str = "low";

/// The effort of the PR briefs' and part catch-ups' writer and retry calls, `prBriefs.effort`, read on each call: `low` by
/// default, kept by #450's measurement (`docs/studio/behaviour/pr-briefs.md`); `default` or empty gives `claude`'s own (no
/// `--effort`); a value `claude` does not take reads as the default. The claim check always runs at `claude`'s own.
pub fn pr_briefs_effort() -> String {
    let text = paths().and_then(|[settings, _]| fs::read_to_string(settings).ok());
    pr_briefs_effort_in(&text.unwrap_or_default())
}

fn pr_briefs_effort_in(text: &str) -> String {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap_or_default();
    match json["prBriefs"]["effort"].as_str().map(str::trim) {
        Some("default") => String::new(),
        Some(effort) if WriterKind::Claude.efforts().contains(&effort) => effort.to_string(),
        _ => PR_BRIEFS_EFFORT.to_string(),
    }
}

/// What a background run reads from `settings.json` (spec danielrispler/plotkeeper#378): the harness and model of
/// `backgroundRuns`, and the project's `checks` and `setup`.
#[derive(Debug, Default, PartialEq)]
pub struct RunSettings {
    /// `backgroundRuns.harness`: `claude` unless the file names another.
    pub harness: String,
    /// `backgroundRuns.model`; none: the harness's own default.
    pub model: Option<String>,
    /// The project's `checks` entry; empty: it has none.
    pub checks: Vec<String>,
    /// The project's `setup` command, run before the agent starts.
    pub setup: Option<String>,
}

/// The run settings for `project` in `settings.json`'s `text`. `checks` and `setup` are per project folder, the longest
/// folder that is the project's or holds it winning, as a session's check list is picked (`studio_core`'s `CheckLists`);
/// there is no default list here. A file that does not parse reads as an empty one.
pub fn run_settings_in(text: &str, project: &str) -> RunSettings {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap_or_default();
    let word = |value: &serde_json::Value| {
        let word = value.as_str()?.trim();
        (!word.is_empty()).then(|| word.to_string())
    };
    // The entry of the longest folder that holds the project.
    let entry = |key: &str| {
        let holds = |folder: &str| {
            let rest = project.strip_prefix(folder.trim_end_matches('/'));
            rest.is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        };
        let folders = json[key].as_object()?;
        let found = folders.iter().filter(|(folder, _)| holds(folder));
        found
            .max_by_key(|(folder, _)| folder.len())
            .map(|(_, value)| value)
    };
    let checks = entry("checks").and_then(|list| list.as_array());
    RunSettings {
        harness: word(&json["backgroundRuns"]["harness"]).unwrap_or_else(|| "claude".into()),
        model: word(&json["backgroundRuns"]["model"]),
        checks: checks.map_or_else(Vec::new, |list| list.iter().filter_map(word).collect()),
        setup: entry("setup").and_then(word),
    }
}

/// `microWorlds.dshVisual` in `settings.json`'s `text`: the folder that holds dsh-visual's `build.py` and `verify.mjs`, which
/// a micro-world's host check runs (#399), a leading `~/` read as `home`. None when it is unset, empty or not a string: then
/// "Explore this" offers only the prompt to copy.
pub fn micro_worlds_dsh_visual_in(text: &str, home: &Path) -> Option<PathBuf> {
    let json: serde_json::Value = serde_json::from_str(&plain_json(text)).unwrap_or_default();
    let folder = json["microWorlds"]["dshVisual"].as_str()?.trim();
    match folder.strip_prefix("~/") {
        Some(rest) => Some(home.join(rest)),
        None => (!folder.is_empty()).then(|| PathBuf::from(folder)),
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

/// Whether `key` looks like an API key: printable ASCII with no space, so it is one word on `security -i`'s line.
fn plain_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 512 && key.chars().all(|c| c.is_ascii_graphic())
}

/// `key` as one word of a `security -i` line: in double quotes, its `"` and `\` each after a backslash, which
/// `security` takes as the next character as it is (`split_line`, Apple's `SecurityTool/macOS/security.c`).
fn quoted(key: &str) -> String {
    let mut word = String::from('"');
    for c in key.chars() {
        if c == '"' || c == '\\' {
            word.push('\\');
        }
        word.push(c);
    }
    word.push('"');
    word
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
            return Err("That is not an API key: it is printable characters with no space.".into());
        }
        // In interactive mode the key goes on standard input, so it never shows in the process list.
        let account = account(kind);
        let command = format!(
            "add-generic-password -U -s {SERVICE} -a {account} -w {}\n",
            quoted(key)
        );
        let out = self.run(&["-i"], Some(&command))?;
        if out.status.success() && self.get(kind).as_deref() == Some(key) {
            Ok(())
        } else {
            Err("The Keychain did not save the key.".into())
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
            return Err("That is not an API key: it is printable characters with no space.".into());
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
mod tests;
