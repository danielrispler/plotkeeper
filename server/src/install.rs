//! `plotkeeper install` and `plotkeeper uninstall` (#357; spec #320, "Install and distribution").
//!
//! Install copies the hook binaries into the data folder's `bin/`, renders each harness's plugin with that path into
//! the data folder, and wires the harnesses it finds on the PATH at user level, each through its own CLI where it has
//! one: Claude Code with `claude plugin`, agy with `agy plugin`, Copilot with `copilot plugin` (#361). Codex has none for its hooks, so install adds its
//! entries to `~/.codex/hooks.json` itself (#359). It also removes the archived app's setup, so no event is recorded twice:
//! the `CLAUDE_CODE_PLUGIN_DIRS` entries in `~/.zshrc` that name a data folder's `checkout/plugin`, and agy's plugin
//! `studio`. What it changes it backs up first (of `~/.zshrc`, only the lines it removes), and `uninstall` puts it
//! back.
//!
//! plotkeeper never opens Claude Code's settings files (AGENTS.md, Hard rules; spec #320, "Install obeys the
//! credentials rule"): `claude plugin` writes them. `~/.codex/hooks.json` holds no credentials: Codex's hook docs
//! (https://learn.chatgpt.com/docs/hooks, read 2026-11-23) give it a `description` and a `hooks` object of handlers
//! (`type`, `command`, `timeout`, `async`, `statusMessage`, an MCP tool's `server`, `tool` and `input`), and no field
//! for a key, a token or an environment; Codex keeps its login elsewhere. `~/.codex/config.toml`, which can hold a
//! provider's token, is never opened.
//!
//! Cursor's plugins have no install command its docs give, so install adds its entries to the user-level
//! `~/.cursor/hooks.json` (#362), never a project's `.cursor/hooks.json`. That file holds no credentials: Cursor's hook
//! docs (https://cursor.com/docs/hooks, read 2026-11-23) give it a `version` and a `hooks` object of entries
//! (`command`, `type`, `prompt`, `timeout`, `loop_limit`, `failClosed`, `matcher`), and no field for a key or a token.
//! No other file in `~/.cursor` is opened.
//!
//! Copilot's plugin goes in with `copilot plugin install` of the folder install renders, and out with `copilot plugin
//! uninstall`; no file under `~/.copilot`, where Copilot keeps its settings and may keep its login, is opened (#361).
//!
//! Qwen Code's hooks would otherwise go in `~/.qwen/settings.json`, which can hold API keys (its `env` block and
//! `security.auth.apiKey`), so plotkeeper never opens it (#379). Its hooks go in an extension instead: a native
//! extension's `qwen-extension.json` may carry a `hooks` object, which Qwen Code loads for every active extension
//! (QwenLM/qwen-code at c134a03e5, `packages/core/src/extension/extensionManager.ts` `loadExtension`, and
//! `packages/core/src/hooks/hookRegistry.ts`, "Extension hooks are always loaded"). Install renders `plugin-qwen/` and
//! runs `qwen extensions install <folder> --consent`, which copies it into Qwen Code's own extension folder; uninstall
//! runs `qwen extensions uninstall plotkeeper`. No file under `~/.qwen` is opened.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use studio_workbench::agy_plugin;

use crate::hook_copies::{self, Refresh};

/// The plugin's and its marketplace's name for Claude Code.
const NAME: &str = "plotkeeper";
/// The plugin id `claude plugin` knows it by.
const CLAUDE_ID: &str = "plotkeeper@plotkeeper";
const PLACEHOLDER: &str = "__PLOTKEEPER_BIN__";
const PLUGIN_JSON: &str = include_str!("../../plugin/.claude-plugin/plugin.json");
const HOOKS_JSON: &str = include_str!("../../plugin/hooks/hooks.json");
/// plotkeeper's entries for Codex's hook file, one recording group per event.
const CODEX_HOOKS_JSON: &str = include_str!("../../plugin-codex/hooks.json");
/// plotkeeper's plugin for Copilot (#361): its manifest and its hooks, one camelCase event an entry.
const COPILOT_PLUGIN_JSON: &str = include_str!("../../plugin-copilot/plugin.json");
const COPILOT_HOOKS_JSON: &str = include_str!("../../plugin-copilot/hooks.json");
/// plotkeeper's entries for Cursor's user-level hook file, one per event (#362).
const CURSOR_HOOKS_JSON: &str = include_str!("../../plugin-cursor/hooks.json");
/// plotkeeper's extension for Qwen Code (#379): its manifest, which holds its hooks.
const QWEN_EXTENSION_JSON: &str = include_str!("../../plugin-qwen/qwen-extension.json");
/// The archived app's plugin for agy, by its folder's name.
const OLD_AGY_PLUGIN: &str = "studio";

/// What install did, kept in the data folder so uninstall undoes that and nothing else.
#[derive(Default, Serialize, Deserialize)]
struct State {
    /// The marketplace was added with `claude plugin`.
    claude: bool,
    /// An uninstall that failed partway took the Claude Code plugin out but not its marketplace.
    #[serde(default)]
    claude_plugin_gone: bool,
    /// plotkeeper's agy plugin was installed with `agy plugin`.
    agy: bool,
    /// A record from before #376's review: `~/.zshrc` was rewritten to this text, and its old bytes are in
    /// `backups/zshrc`. Install no longer writes it; uninstall still undoes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zshrc: Option<String>,
    /// The lines install took out of `~/.zshrc`, and where.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    zshrc_edits: Option<ZshrcRecord>,
    /// agy plugins whose folders were backed up under `backups/agy/<name>` before install replaced or removed them.
    agy_backups: Vec<String>,
    /// What install did to `~/.codex/hooks.json`.
    codex: Option<HookFileRecord>,
    /// What install did to `~/.cursor/hooks.json` (#362), recorded as Codex's file is.
    cursor: Option<HookFileRecord>,
    /// plotkeeper's Copilot plugin was installed with `copilot plugin` (#361).
    #[serde(default)]
    copilot: bool,
    /// plotkeeper's Qwen Code extension was installed with `qwen extensions` (#379).
    #[serde(default)]
    qwen: bool,
}

/// The folders install writes in the data folder: the plugins it renders, and its own record with the backups.
fn plugins_dir(home: &Path) -> PathBuf {
    studio_hook::data_dir(home).join("plugins")
}

fn record_dir(home: &Path) -> PathBuf {
    studio_hook::data_dir(home).join("install")
}

fn load(home: &Path) -> Result<Option<State>, String> {
    let path = record_dir(home).join("state.json");
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("could not read {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("could not read {}: {e}", path.display())),
    }
}

fn save(home: &Path, state: &State) -> Result<(), String> {
    let dir = record_dir(home);
    fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(dir.join("state.json"), text)
        .map_err(|e| format!("could not record the install: {e}"))
}

/// The program `name` in a folder of `path` (a PATH value), as `command -v` finds it.
fn on_path(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path?)
        .map(|dir| dir.join(name))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// Runs `program` with `args`, with no input; its stderr is the error.
fn run(program: &Path, args: &[&OsStr]) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run {}: {e}", program.display()))?;
    if out.status.success() {
        return Ok(());
    }
    let words: Vec<_> = args.iter().map(|a| a.to_string_lossy()).collect();
    Err(format!(
        "{} {} failed: {}",
        program.display(),
        words.join(" "),
        String::from_utf8_lossy(&out.stderr).trim()
    ))
}

fn say(out: &mut dyn Write, line: impl AsRef<str>) {
    let _ = writeln!(out, "  {}", line.as_ref());
}

/// Whether an entry of `CLAUDE_CODE_PLUGIN_DIRS` is the archived app's plugin, through a data folder's `checkout`
/// link, by either of its folder's names (spec #320, "Install cleans up the old names").
fn old_plugin_dir(entry: &str) -> bool {
    let entry = entry.trim_end_matches('/');
    entry.ends_with("/plotkeeper/checkout/plugin")
        || entry.ends_with("/dev.falinks.studio/checkout/plugin")
}

/// Lines install took out of `~/.zshrc` at one place: what they were and what it wrote instead. Only these are kept,
/// never the rest of the file, which may export a token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ZshrcEdit {
    /// The index, among the lines of the file install wrote, of the first line after the removed ones.
    at: usize,
    /// The removed lines, each with its line ending.
    removed: Vec<String>,
    /// The line install wrote in their place, with the entries that were not plotkeeper's, if any were left.
    kept: Option<String>,
}

/// What install did to `~/.zshrc`: the line count of the text it wrote, and its edits in order.
#[derive(Serialize, Deserialize)]
struct ZshrcRecord {
    lines: usize,
    edits: Vec<ZshrcEdit>,
}

/// `profile` without the old plugin entries of its `export CLAUDE_CODE_PLUGIN_DIRS=` lines, and the edits that undo
/// it: a line left with no entry goes, one with others keeps them, and the archived app's comment above a changed
/// line goes too.
// shortcut: reads `export NAME=value` lines only, quoted or bare, as the archived app wrote them.
fn without_old_plugin_dirs(profile: &str) -> (String, Vec<ZshrcEdit>) {
    const EXPORT: &str = "export CLAUDE_CODE_PLUGIN_DIRS=";
    let mut lines: Vec<String> = Vec::new();
    let mut edits = Vec::new();
    for piece in profile.split_inclusive('\n') {
        let line = piece.trim_end_matches('\n');
        let ending = &piece[line.len()..];
        let indent = &line[..line.len() - line.trim_start().len()];
        let Some(value) = line.trim_start().strip_prefix(EXPORT) else {
            lines.push(piece.to_owned());
            continue;
        };
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if !value.split(':').any(old_plugin_dir) {
            lines.push(piece.to_owned());
            continue;
        }
        let mut removed = Vec::new();
        // The comment is the previous source line only when no edit already holds it.
        let after_edit = edits
            .last()
            .is_some_and(|e: &ZshrcEdit| e.at + usize::from(e.kept.is_some()) == lines.len());
        if !after_edit
            && lines.last().is_some_and(|l| {
                l.trim_start().starts_with('#')
                    && l.trim_end().ends_with("for every claude session (#21).")
            })
        {
            removed.extend(lines.pop());
        }
        removed.push(piece.to_owned());
        let kept: Vec<_> = value
            .split(':')
            .filter(|d| !d.is_empty() && !old_plugin_dir(d))
            .collect();
        let kept =
            (!kept.is_empty()).then(|| format!("{indent}{EXPORT}\"{}\"{ending}", kept.join(":")));
        edits.push(ZshrcEdit {
            at: lines.len(),
            removed,
            kept: kept.clone(),
        });
        lines.extend(kept);
    }
    (lines.concat(), edits)
}

/// `now`, the text install wrote to `~/.zshrc`, with `record`'s removed lines put back, or `None` when the lines it
/// wrote are no longer where it wrote them.
fn with_removed_lines(now: &str, record: &ZshrcRecord) -> Option<String> {
    let lines: Vec<&str> = now.split_inclusive('\n').collect();
    if lines.len() != record.lines {
        return None;
    }
    let mut out = String::new();
    let mut next = 0;
    for edit in &record.edits {
        out.push_str(&lines.get(next..edit.at)?.concat());
        out.push_str(&edit.removed.concat());
        next = edit.at;
        if let Some(kept) = &edit.kept {
            if lines.get(next) != Some(&kept.as_str()) {
                return None;
            }
            next += 1;
        }
    }
    out.push_str(&lines.get(next..)?.concat());
    Some(out)
}

/// `template`, a hook file, with the path of `bin` in place of its placeholder. The path sits in a JSON string, inside
/// the shell's double quotes, so it is escaped as one.
fn render(template: &str, bin: &Path) -> Result<String, String> {
    let path = serde_json::to_string(&bin.display().to_string()).map_err(|e| e.to_string())?;
    Ok(template.replace(PLACEHOLDER, &path[1..path.len() - 1]))
}

/// Writes Claude Code's plugin for the binaries in `bin` into `dir`, as a marketplace of one plugin, which is how
/// `claude plugin install` takes a local folder.
fn render_claude(dir: &Path, bin: &Path) -> Result<(), String> {
    let marketplace = serde_json::json!({
        "name": NAME,
        "owner": { "name": "plotkeeper" },
        "metadata": { "description": "plotkeeper's hooks, rendered by plotkeeper install" },
        "plugins": [{ "name": NAME, "source": format!("./{NAME}"), "description": "Appends Claude Code hook events to plotkeeper's event log" }]
    });
    let files = [
        (
            ".claude-plugin/marketplace.json".to_owned(),
            marketplace.to_string(),
        ),
        (
            format!("{NAME}/.claude-plugin/plugin.json"),
            PLUGIN_JSON.to_owned(),
        ),
        (format!("{NAME}/hooks/hooks.json"), render(HOOKS_JSON, bin)?),
    ];
    for (name, text) in files {
        let file = dir.join(name);
        fs::create_dir_all(file.parent().unwrap_or(dir)).map_err(|e| e.to_string())?;
        fs::write(&file, text).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    }
    Ok(())
}

/// Copies the folder `from` to `to`, files and folders only.
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// `plotkeeper install` for `home`, with the hook binaries taken from `beside` and the harnesses found on `path`.
/// Prints each step to `out`. A harness not found is skipped with one line; an error stops at that step, with what
/// was done before it recorded for uninstall.
pub fn install(
    home: &Path,
    beside: &Path,
    path: Option<&OsStr>,
    out: &mut dyn Write,
) -> Result<(), String> {
    let bin = studio_hook::bin_dir(home);
    match hook_copies::refresh(beside, &bin)
        .map_err(|e| format!("could not copy the hook binaries: {e}"))?
    {
        Refresh::Missing(missing) => {
            return Err(format!(
                "{} not found in {}; nothing was changed",
                missing.join(" and "),
                beside.display()
            ));
        }
        _ => say(out, format!("hook binaries: copied into {}", bin.display())),
    }
    let mut state = load(home)?.unwrap_or_default();
    let backups = record_dir(home).join("backups");

    // The archived app's line in `~/.zshrc`, once: a second install keeps the first record. Only the removed lines
    // are recorded, never a copy of the file, which may export a token.
    let zshrc = home.join(".zshrc");
    if state.zshrc.is_none()
        && state.zshrc_edits.is_none()
        && let Ok(text) = fs::read_to_string(&zshrc)
    {
        let (cleaned, edits) = without_old_plugin_dirs(&text);
        if !edits.is_empty() {
            state.zshrc_edits = Some(ZshrcRecord {
                lines: cleaned.split_inclusive('\n').count(),
                edits,
            });
            save(home, &state)?;
            fs::write(&zshrc, cleaned).map_err(|e| format!("could not write ~/.zshrc: {e}"))?;
            say(
                out,
                "~/.zshrc: removed the old plugin path from CLAUDE_CODE_PLUGIN_DIRS (the removed lines are kept for uninstall)",
            );
        }
    }
    save(home, &state)?;

    match on_path("claude", path) {
        None => say(out, "Claude Code: `claude` not found, skipped"),
        Some(claude) => {
            let dir = plugins_dir(home).join("claude");
            render_claude(&dir, &bin)?;
            if state.claude {
                // A second install starts from none; either may already be gone.
                let _ = run(
                    &claude,
                    &["plugin".as_ref(), "uninstall".as_ref(), CLAUDE_ID.as_ref()],
                );
                let _ = run(
                    &claude,
                    &[
                        "plugin".as_ref(),
                        "marketplace".as_ref(),
                        "remove".as_ref(),
                        NAME.as_ref(),
                    ],
                );
            }
            run(
                &claude,
                &[
                    "plugin".as_ref(),
                    "marketplace".as_ref(),
                    "add".as_ref(),
                    dir.as_os_str(),
                ],
            )?;
            state.claude = true;
            save(home, &state)?;
            say(
                out,
                format!(
                    "Claude Code: claude plugin marketplace add {}",
                    dir.display()
                ),
            );
            run(
                &claude,
                &["plugin".as_ref(), "install".as_ref(), CLAUDE_ID.as_ref()],
            )?;
            state.claude_plugin_gone = false;
            save(home, &state)?;
            say(
                out,
                format!("Claude Code: claude plugin install {CLAUDE_ID}"),
            );
        }
    }

    match on_path("agy", path) {
        None => say(out, "agy: `agy` not found, skipped"),
        Some(agy) => {
            for name in [OLD_AGY_PLUGIN, agy_plugin::NAME] {
                let installed = agy_plugin::installed(home, name);
                let ours = name == agy_plugin::NAME && state.agy;
                if ours || state.agy_backups.iter().any(|n| n == name) || !installed.is_dir() {
                    continue;
                }
                copy_dir(&installed, &backups.join("agy").join(name))
                    .map_err(|e| format!("could not back up {}: {e}", installed.display()))?;
                state.agy_backups.push(name.to_owned());
                save(home, &state)?;
                if name == OLD_AGY_PLUGIN {
                    agy_plugin::uninstall(&agy, name, home)?;
                    say(
                        out,
                        format!(
                            "agy: removed the old plugin `{name}` (backup in {})",
                            backups.display()
                        ),
                    );
                }
            }
            let dir = plugins_dir(home).join("agy").join(agy_plugin::NAME);
            agy_plugin::render(&dir, &bin.join("plotkeeper-hook"))?;
            state.agy = true;
            save(home, &state)?;
            agy_plugin::install(&agy, &dir, home)?;
            say(out, format!("agy: agy plugin install {}", dir.display()));
        }
    }

    match on_path("codex", path) {
        None => say(out, "Codex: `codex` not found, skipped"),
        Some(_) => {
            codex_file(home).install(home, &bin, &mut state, |s| &mut s.codex, out)?;
            say(
                out,
                "Codex: once, run `/hooks` in Codex and trust plotkeeper's hooks; Codex skips a hook until it is trusted",
            );
        }
    }

    // Cursor is found by its folder, which its IDE and its CLI make, or by its programs: the IDE's
    // `cursor` and the CLI's `agent`.
    let cursor = home.join(".cursor").is_dir()
        || on_path("cursor", path).is_some()
        || on_path("agent", path).is_some();
    match cursor {
        false => say(
            out,
            "Cursor: `~/.cursor`, `cursor` and `agent` not found, skipped",
        ),
        true => cursor_file(home).install(home, &bin, &mut state, |s| &mut s.cursor, out)?,
    }

    match on_path("copilot", path) {
        None => say(out, "Copilot: `copilot` not found, skipped"),
        Some(copilot) => install_copilot(home, &bin, &copilot, &mut state, out)?,
    }

    match on_path("qwen", path) {
        None => say(out, "Qwen Code: `qwen` not found, skipped"),
        Some(qwen) => install_qwen(home, &bin, &qwen, &mut state, out)?,
    }
    Ok(())
}

/// Renders `plugin-qwen/` for the binaries in `bin` into the data folder and installs it with `qwen extensions install
/// --consent`, which copies it into Qwen Code's own extension folder without a prompt; a second install replaces the
/// first, as Qwen Code refuses an extension already installed. plotkeeper opens no file of Qwen Code's.
fn install_qwen(
    home: &Path,
    bin: &Path,
    qwen: &Path,
    state: &mut State,
    out: &mut dyn Write,
) -> Result<(), String> {
    let dir = plugins_dir(home).join("qwen").join(NAME);
    fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    let file = dir.join("qwen-extension.json");
    fs::write(&file, render(QWEN_EXTENSION_JSON, bin)?)
        .map_err(|e| format!("could not write {}: {e}", file.display()))?;
    if state.qwen {
        // Already gone is fine: the install below puts it back.
        let _ = run(
            qwen,
            &["extensions".as_ref(), "uninstall".as_ref(), NAME.as_ref()],
        );
    }
    run(
        qwen,
        &[
            "extensions".as_ref(),
            "install".as_ref(),
            dir.as_os_str(),
            "--consent".as_ref(),
        ],
    )?;
    state.qwen = true;
    save(home, state)?;
    say(
        out,
        format!("Qwen Code: qwen extensions install {}", dir.display()),
    );
    Ok(())
}

/// Renders `plugin-copilot/` for the binaries in `bin` into the data folder and installs it with `copilot plugin
/// install`, which copies it into Copilot's own plugin folder; a second install replaces the first. plotkeeper opens
/// no file of Copilot's: `copilot` writes what it keeps of the install.
fn install_copilot(
    home: &Path,
    bin: &Path,
    copilot: &Path,
    state: &mut State,
    out: &mut dyn Write,
) -> Result<(), String> {
    let dir = plugins_dir(home).join("copilot").join(NAME);
    fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    for (name, text) in [
        ("plugin.json", COPILOT_PLUGIN_JSON.to_owned()),
        ("hooks.json", render(COPILOT_HOOKS_JSON, bin)?),
    ] {
        let file = dir.join(name);
        fs::write(&file, text).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    }
    if state.copilot {
        // Already gone is fine: the install below puts it back.
        let _ = run(
            copilot,
            &["plugin".as_ref(), "uninstall".as_ref(), NAME.as_ref()],
        );
    }
    run(
        copilot,
        &["plugin".as_ref(), "install".as_ref(), dir.as_os_str()],
    )?;
    state.copilot = true;
    save(home, state)?;
    say(
        out,
        format!("Copilot: copilot plugin install {}", dir.display()),
    );
    Ok(())
}

/// Install's record of a hook file it edits itself, Codex's or Cursor's.
#[derive(Serialize, Deserialize)]
struct HookFileRecord {
    /// The file's text as install last wrote it.
    written: String,
    /// The file was there before install; its old bytes, without plotkeeper's entries, are in the file's backup.
    existed: bool,
    /// Install made the file's folder for it.
    made_dir: bool,
}

/// A harness's user-level hook file that install edits itself, as neither Codex nor Cursor has a command for its hooks:
/// plotkeeper's entries go in beside the user's own, and come out again at uninstall.
struct HookFile {
    /// The harness's name in what install prints.
    label: &'static str,
    file: PathBuf,
    /// The backup's file name in the record's `backups/`.
    backup: &'static str,
    /// plotkeeper's entries, with the binaries' path a placeholder.
    template: &'static str,
    /// The file's content when install makes it.
    fresh: fn() -> Value,
}

/// `~/.codex/hooks.json`: one recording group per event (#359).
fn codex_file(home: &Path) -> HookFile {
    HookFile {
        label: "Codex",
        file: home.join(".codex").join("hooks.json"),
        backup: "codex-hooks.json",
        template: CODEX_HOOKS_JSON,
        fresh: || Value::Object(Default::default()),
    }
}

/// `~/.cursor/hooks.json`, never a project's: one entry per event (#362).
fn cursor_file(home: &Path) -> HookFile {
    HookFile {
        label: "Cursor",
        file: home.join(".cursor").join("hooks.json"),
        backup: "cursor-hooks.json",
        template: CURSOR_HOOKS_JSON,
        fresh: || serde_json::json!({ "version": 1 }),
    }
}

impl HookFile {
    fn shown(&self) -> std::path::Display<'_> {
        self.file.display()
    }

    /// The file's text; `None` when it is missing.
    fn read(&self) -> Result<Option<String>, String> {
        match fs::read_to_string(&self.file) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!(
                "{}: could not read {}: {e}",
                self.label,
                self.shown()
            )),
        }
    }

    fn parse(&self, text: &str) -> Result<Value, String> {
        serde_json::from_str(text)
            .map_err(|e| format!("{}: could not read {}: {e}", self.label, self.shown()))
    }

    /// The events of hook file `hooks`, which must be an object whose `hooks`, if any, is an object of arrays.
    fn events<'a>(
        &self,
        hooks: &'a mut Value,
    ) -> Result<&'a mut serde_json::Map<String, Value>, String> {
        let bad = || {
            format!(
                "{0}: {1} is not a hook file {0} reads",
                self.label,
                self.shown()
            )
        };
        let events = hooks
            .as_object_mut()
            .ok_or_else(bad)?
            .entry("hooks")
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
            .ok_or_else(bad)?;
        if events.values().all(Value::is_array) {
            Ok(events)
        } else {
            Err(bad())
        }
    }

    /// plotkeeper's entries, rendered for the binaries in `bin`, by event.
    fn ours(&self, bin: &Path) -> Result<serde_json::Map<String, Value>, String> {
        let rendered: Value =
            serde_json::from_str(&render(self.template, bin)?).map_err(|e| e.to_string())?;
        match rendered.get("hooks") {
            Some(Value::Object(events)) => Ok(events.clone()),
            _ => Err(format!(
                "{}: plotkeeper's hooks.json has no hooks",
                self.label
            )),
        }
    }

    /// `text` without plotkeeper's entries; `text` itself, byte for byte, when it has none.
    // shortcut: a file that had some is written back with its keys sorted, as serde_json writes them.
    fn without_ours(&self, text: &str, bin: &Path) -> Result<String, String> {
        let mut hooks = self.parse(text)?;
        let ours = self.ours(bin)?;
        let events = self.events(&mut hooks)?;
        let mut removed = false;
        for (event, ours) in ours {
            let Some(Value::Array(entries)) = events.get_mut(&event) else {
                continue;
            };
            let ours = ours.as_array().cloned().unwrap_or_default();
            let before = entries.len();
            entries.retain(|entry| !ours.contains(entry));
            removed |= entries.len() != before;
            if entries.is_empty() {
                events.remove(&event);
            }
        }
        if !removed {
            return Ok(text.to_owned());
        }
        Ok(serde_json::to_string_pretty(&hooks).map_err(|e| e.to_string())? + "\n")
    }

    /// Adds plotkeeper's entries, once each, beside the user's own. Before the first install, and before one that finds
    /// the file changed since install last wrote it, the file is backed up without plotkeeper's entries, so uninstall
    /// gives back the user's latest hooks.
    // shortcut: the file is written back with its keys sorted, as serde_json writes them; uninstall restores its bytes.
    fn install(
        &self,
        home: &Path,
        bin: &Path,
        state: &mut State,
        record: fn(&mut State) -> &mut Option<HookFileRecord>,
        out: &mut dyn Write,
    ) -> Result<(), String> {
        let dir = self.file.parent().unwrap_or(home);
        let old = self.read()?;
        let mut hooks = match &old {
            Some(text) => self.parse(text)?,
            None => (self.fresh)(),
        };
        let events = self.events(&mut hooks)?;
        for (event, ours) in self.ours(bin)? {
            let entries = events
                .entry(event)
                .or_insert_with(|| Value::Array(Vec::new()));
            let entries = entries.as_array_mut().expect("checked to be an array");
            for entry in ours.as_array().into_iter().flatten() {
                if !entries.contains(entry) {
                    entries.push(entry.clone());
                }
            }
        }
        let text = serde_json::to_string_pretty(&hooks).map_err(|e| e.to_string())? + "\n";
        let changed = record(state)
            .as_ref()
            .is_none_or(|r| old.as_deref() != Some(r.written.as_str()));
        if changed {
            if let Some(old) = &old {
                let backups = record_dir(home).join("backups");
                fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
                fs::write(backups.join(self.backup), self.without_ours(old, bin)?)
                    .map_err(|e| format!("could not back up {}: {e}", self.shown()))?;
            }
            let made_dir = record(state).as_ref().map_or(!dir.is_dir(), |r| r.made_dir);
            *record(state) = Some(HookFileRecord {
                written: String::new(),
                existed: old.is_some(),
                // A second install keeps knowing the folder was install's.
                made_dir,
            });
            save(home, state)?;
        }
        fs::create_dir_all(dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
        fs::write(&self.file, &text)
            .map_err(|e| format!("could not write {}: {e}", self.shown()))?;
        if let Some(record) = record(state).as_mut() {
            record.written = text;
        }
        save(home, state)?;
        say(
            out,
            format!(
                "{}: added plotkeeper's hooks to {}",
                self.label,
                self.shown()
            ),
        );
        Ok(())
    }

    /// Takes install's change back: the file as it was when it is as install left it, else the file without
    /// plotkeeper's entries.
    fn uninstall(
        &self,
        home: &Path,
        record: &HookFileRecord,
        bin: &Path,
        out: &mut dyn Write,
    ) -> Result<(), String> {
        let Some(now) = self.read()? else {
            return Ok(());
        };
        if now == record.written {
            if record.existed {
                let old = fs::read(record_dir(home).join("backups").join(self.backup))
                    .map_err(|e| format!("could not read the backup of {}: {e}", self.shown()))?;
                fs::write(&self.file, old)
                    .map_err(|e| format!("could not write {}: {e}", self.shown()))?;
            } else {
                fs::remove_file(&self.file)
                    .map_err(|e| format!("could not remove {}: {e}", self.shown()))?;
                if record.made_dir {
                    let _ = fs::remove_dir(self.file.parent().unwrap_or(home));
                }
            }
            say(
                out,
                format!("{}: {} put back as it was", self.label, self.shown()),
            );
            return Ok(());
        }
        let text = self.without_ours(&now, bin)?;
        if text != now {
            fs::write(&self.file, text)
                .map_err(|e| format!("could not write {}: {e}", self.shown()))?;
        }
        say(
            out,
            format!(
                "{}: {} changed since install, so only plotkeeper's hooks were taken out of it",
                self.label,
                self.shown()
            ),
        );
        Ok(())
    }
}

/// `plotkeeper uninstall` for `home`, with the harnesses found on `path`: undoes what install recorded, restores its
/// backups, and removes the data folder's `bin/`, the rendered plugins and the record. Prints each step to `out`. Every
/// recorded step runs: one that fails, such as a harness's CLI no longer on the PATH, is said and stays recorded, with
/// everything in the data folder kept, so running uninstall again retries only the steps that failed.
pub fn uninstall(home: &Path, path: Option<&OsStr>, out: &mut dyn Write) -> Result<(), String> {
    let Some(mut state) = load(home)? else {
        say(out, "plotkeeper install has not run here; nothing to undo");
        return Ok(());
    };
    let backups = record_dir(home).join("backups");
    let bin = studio_hook::bin_dir(home);
    let mut failed = 0;
    let mut fail = |out: &mut dyn Write, e: String| {
        say(out, format!("failed: {e}"));
        failed += 1;
    };

    if let Some(codex) = &state.codex {
        match codex_file(home).uninstall(home, codex, &bin, out) {
            Ok(()) => state.codex = None,
            Err(e) => fail(out, e),
        }
    }
    if let Some(cursor) = &state.cursor {
        match cursor_file(home).uninstall(home, cursor, &bin, out) {
            Ok(()) => state.cursor = None,
            Err(e) => fail(out, e),
        }
    }

    if state.copilot {
        let done = on_path("copilot", path)
            .ok_or_else(|| {
                "Copilot: `copilot` not found, so its plugin cannot be uninstalled".to_owned()
            })
            .and_then(|copilot| {
                run(
                    &copilot,
                    &["plugin".as_ref(), "uninstall".as_ref(), NAME.as_ref()],
                )
            });
        match done {
            Ok(()) => {
                say(out, format!("Copilot: copilot plugin uninstall {NAME}"));
                state.copilot = false;
            }
            Err(e) => fail(out, e),
        }
    }

    if state.qwen {
        let done = on_path("qwen", path)
            .ok_or_else(|| {
                "Qwen Code: `qwen` not found, so its extension cannot be uninstalled".to_owned()
            })
            .and_then(|qwen| {
                run(
                    &qwen,
                    &["extensions".as_ref(), "uninstall".as_ref(), NAME.as_ref()],
                )
            });
        match done {
            Ok(()) => {
                say(out, format!("Qwen Code: qwen extensions uninstall {NAME}"));
                state.qwen = false;
            }
            Err(e) => fail(out, e),
        }
    }

    if state.agy || !state.agy_backups.is_empty() {
        match on_path("agy", path) {
            None => fail(
                out,
                "agy: `agy` not found, so its plugin cannot be uninstalled".into(),
            ),
            Some(agy) => {
                if state.agy {
                    match agy_plugin::uninstall(&agy, agy_plugin::NAME, home) {
                        Ok(()) => {
                            say(
                                out,
                                format!("agy: agy plugin uninstall {}", agy_plugin::NAME),
                            );
                            state.agy = false;
                        }
                        Err(e) => fail(out, e),
                    }
                }
                let mut left = Vec::new();
                for name in std::mem::take(&mut state.agy_backups) {
                    match agy_plugin::install(&agy, &backups.join("agy").join(&name), home) {
                        Ok(()) => say(out, format!("agy: put back the plugin `{name}`")),
                        Err(e) => {
                            fail(out, e);
                            left.push(name);
                        }
                    }
                }
                state.agy_backups = left;
            }
        }
    }

    if state.claude {
        match on_path("claude", path) {
            None => fail(
                out,
                "Claude Code: `claude` not found, so its plugin cannot be uninstalled".into(),
            ),
            Some(claude) => {
                if !state.claude_plugin_gone {
                    match run(
                        &claude,
                        &["plugin".as_ref(), "uninstall".as_ref(), CLAUDE_ID.as_ref()],
                    ) {
                        Ok(()) => {
                            say(
                                out,
                                format!("Claude Code: claude plugin uninstall {CLAUDE_ID}"),
                            );
                            state.claude_plugin_gone = true;
                        }
                        Err(e) => fail(out, e),
                    }
                }
                if state.claude_plugin_gone {
                    match run(
                        &claude,
                        &[
                            "plugin".as_ref(),
                            "marketplace".as_ref(),
                            "remove".as_ref(),
                            NAME.as_ref(),
                        ],
                    ) {
                        Ok(()) => {
                            say(
                                out,
                                format!("Claude Code: claude plugin marketplace remove {NAME}"),
                            );
                            state.claude = false;
                            state.claude_plugin_gone = false;
                        }
                        Err(e) => fail(out, e),
                    }
                }
            }
        }
    }

    if let Some(written) = &state.zshrc {
        let zshrc = home.join(".zshrc");
        if fs::read_to_string(&zshrc).is_ok_and(|now| &now == written) {
            let put_back = fs::read(backups.join("zshrc"))
                .map_err(|e| format!("could not read the backup of ~/.zshrc: {e}"))
                .and_then(|old| {
                    fs::write(&zshrc, old).map_err(|e| format!("could not write ~/.zshrc: {e}"))
                });
            match put_back.and_then(|()| {
                fs::remove_file(backups.join("zshrc"))
                    .map_err(|e| format!("could not delete the backup of ~/.zshrc: {e}"))
            }) {
                Ok(()) => {
                    say(out, "~/.zshrc: put back as it was");
                    state.zshrc = None;
                }
                Err(e) => fail(out, e),
            }
        } else {
            // Not a failure: only the backup of `~/.zshrc` is left to give back, so the record stays for it.
            say(
                out,
                format!(
                    "~/.zshrc changed since install, so it is left as it is; its old copy is {}",
                    backups.join("zshrc").display()
                ),
            );
        }
    }

    if let Some(record) = &state.zshrc_edits {
        let zshrc = home.join(".zshrc");
        let put_back = fs::read_to_string(&zshrc)
            .ok()
            .and_then(|now| with_removed_lines(&now, record));
        match put_back {
            Some(text) => match fs::write(&zshrc, text) {
                Ok(()) => {
                    say(out, "~/.zshrc: put back as it was");
                    state.zshrc_edits = None;
                }
                Err(e) => fail(out, format!("could not write ~/.zshrc: {e}")),
            },
            None => {
                // Not a failure: only these lines are left to give back, so the record stays for them.
                let lines: String = record
                    .edits
                    .iter()
                    .flat_map(|e| e.removed.iter().map(String::as_str))
                    .collect();
                say(
                    out,
                    format!(
                        "~/.zshrc changed since install, so it is left as it is; the lines install removed were:\n{}",
                        lines.trim_end()
                    ),
                );
            }
        }
    }

    if failed > 0 {
        // The hooks of a harness not undone still run the binaries, and a rerun needs the backups.
        save(home, &state)?;
        return Err(format!(
            "{failed} step(s) failed, said above; everything else was undone. Run `plotkeeper uninstall` again to retry them"
        ));
    }
    let mut gone = vec![bin, plugins_dir(home)];
    if state.zshrc.is_some() || state.zshrc_edits.is_some() {
        save(home, &state)?;
    } else {
        gone.push(record_dir(home));
    }
    for dir in gone {
        match fs::remove_dir_all(&dir) {
            Ok(()) => say(out, format!("removed {}", dir.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("could not remove {}: {e}", dir.display())),
        }
    }
    // A data folder install made and left empty goes too.
    let _ = fs::remove_dir(studio_hook::data_dir(home));
    Ok(())
}
