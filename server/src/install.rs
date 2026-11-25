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
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::{BufRead, Write},
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
    /// Each harness the user answered for, by its id (`claude`, `agy`, `codex`, `cursor`, `copilot`, `qwen`); a harness
    /// with none has not been asked (#434).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    harnesses: BTreeMap<String, HarnessRecord>,
    /// Written by a build that keeps answers (#434), on every save, and kept through uninstall, so a harness with no
    /// answer here is unasked even when an uninstall that failed partway left its old-style flag set. Without it, the
    /// record is from before answers, and reading it gives each flagged harness a wired answer (`State::with_answers`).
    #[serde(default)]
    answers_kept: bool,
}

/// Install's record of one harness.
#[derive(Serialize, Deserialize)]
struct HarnessRecord {
    answer: Answer,
    /// For a wired harness, the fingerprint of what install rendered for it (#438), which a start compares with what
    /// it would render now. A wired record with none, from a build before #438 or rewritten by one, is rewired once by
    /// the next start, which then stores it. An older build ignores the field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fingerprint: Option<String>,
}

/// A fingerprint of `files`, (name, text) pairs: their 64-bit FNV-1a hash in hex, which, unlike std's hasher, stays the
/// same across builds.
fn fingerprint(files: &[(String, String)]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, text) in files {
        for byte in name.bytes().chain([0]).chain(text.bytes()).chain([0]) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

impl State {
    /// A record from before answers were kept (no `answers_kept`) gets an answer, wired with no fingerprint, for each
    /// harness its flags say install wired, so the first save keeps them and the next start rewires each once. A later
    /// record is taken as it is: a harness with no answer there is unasked, whatever an uninstall that failed partway
    /// left flagged.
    fn with_answers(mut self) -> State {
        if !self.answers_kept {
            for harness in HARNESSES {
                if harness.flagged(&self) {
                    self.harnesses
                        .entry(harness.id().to_owned())
                        .or_insert(HarnessRecord {
                            answer: Answer::Wired,
                            fingerprint: None,
                        });
                }
            }
        }
        self
    }
}

impl HarnessRecord {
    /// The record of `harness` just wired to the hook binaries in `bin`.
    fn wired(harness: Harness, home: &Path, bin: &Path) -> Result<HarnessRecord, String> {
        Ok(HarnessRecord {
            answer: Answer::Wired,
            fingerprint: Some(harness.fingerprint(home, bin)?),
        })
    }
}

/// The user's answer for a harness: `plotkeeper install` and a yes record it wired, a no declined.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Answer {
    Wired,
    Declined,
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
            .map(|state: State| Some(state.with_answers()))
            .map_err(|e| format!("could not read {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("could not read {}: {e}", path.display())),
    }
}

fn save(home: &Path, state: &State) -> Result<(), String> {
    let dir = record_dir(home);
    fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    let mut value = serde_json::to_value(state).map_err(|e| e.to_string())?;
    value["answers_kept"] = Value::Bool(true);
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    write_whole(&dir.join("state.json"), text)
        .map_err(|e| format!("could not record the install: {e}"))
}

/// Writes `text` as the whole of `file`, or leaves `file` as it was: the text goes to a temp file in the same folder,
/// which is then renamed over it, so a crash mid-write never leaves half a `~/.zshrc`. A symlink is resolved first,
/// so the file it points at is replaced and the link stays a link. The file keeps its permissions.
fn write_whole(file: &Path, text: impl AsRef<[u8]>) -> std::io::Result<()> {
    let target = match fs::canonicalize(file) {
        Ok(target) => target,
        // A link to a file that is not there: writing through it makes the file and keeps the link.
        Err(_) if file.is_symlink() => return fs::write(file, text),
        Err(_) => file.to_path_buf(),
    };
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let temp = target.with_file_name(format!(".{name}.plotkeeper-{}.tmp", std::process::id()));
    let written = (|| {
        let mut out = fs::File::create(&temp)?;
        out.write_all(text.as_ref())?;
        if let Ok(old) = fs::metadata(&target) {
            out.set_permissions(old.permissions())?;
        }
        out.sync_all()?;
        fs::rename(&temp, &target)
    })();
    if written.is_err() {
        // Best effort: the write's own error is the one returned.
        fs::remove_file(&temp).ok();
    }
    written
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
    // Output only: a closed terminal is no error.
    writeln!(out, "  {}", line.as_ref()).ok();
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

/// Claude Code's plugin for the binaries in `bin`, as a marketplace of one plugin, which is how `claude plugin
/// install` takes a local folder: each file by its path in that folder.
fn claude_files(bin: &Path) -> Result<Vec<(String, String)>, String> {
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
    Ok(files.into())
}

/// Writes Claude Code's plugin for the binaries in `bin` into `dir`.
fn render_claude(dir: &Path, bin: &Path) -> Result<(), String> {
    for (name, text) in claude_files(bin)? {
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
    wire(home, beside, path, None, out)
}

/// Wires the harnesses found on `path`, or of those only the ones in `only`, and records each as wired. With `only`,
/// the harnesses not found are not said.
fn wire(
    home: &Path,
    beside: &Path,
    path: Option<&OsStr>,
    only: Option<&[Harness]>,
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
            write_whole(&zshrc, cleaned).map_err(|e| format!("could not write ~/.zshrc: {e}"))?;
            say(
                out,
                "~/.zshrc: removed the old plugin path from CLAUDE_CODE_PLUGIN_DIRS (the removed lines are kept for uninstall)",
            );
        }
    }
    save(home, &state)?;

    for harness in HARNESSES {
        if !harness.found(home, path) {
            if only.is_none() {
                say(out, harness.skipped());
            }
            continue;
        }
        if only.is_some_and(|only| !only.contains(&harness)) {
            continue;
        }
        wire_one(harness, home, &bin, path, &backups, &mut state, out)?;
        state.harnesses.insert(
            harness.id().to_owned(),
            HarnessRecord::wired(harness, home, &bin)?,
        );
        save(home, &state)?;
    }
    Ok(())
}

/// Wires one harness found on `path` to the hook binaries in `bin`, recording each step in `state` for uninstall.
fn wire_one(
    harness: Harness,
    home: &Path,
    bin: &Path,
    path: Option<&OsStr>,
    backups: &Path,
    state: &mut State,
    out: &mut dyn Write,
) -> Result<(), String> {
    let found = |name: &str| on_path(name, path).ok_or_else(|| format!("`{name}` not found"));
    match harness {
        Harness::Claude => {
            let claude = found("claude")?;
            let dir = plugins_dir(home).join("claude");
            render_claude(&dir, bin)?;
            if state.claude {
                // A second install starts from none; either may already be gone.
                run(
                    &claude,
                    &["plugin".as_ref(), "uninstall".as_ref(), CLAUDE_ID.as_ref()],
                )
                .ok();
                run(
                    &claude,
                    &[
                        "plugin".as_ref(),
                        "marketplace".as_ref(),
                        "remove".as_ref(),
                        NAME.as_ref(),
                    ],
                )
                .ok();
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
            save(home, state)?;
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
            save(home, state)?;
            say(
                out,
                format!("Claude Code: claude plugin install {CLAUDE_ID}"),
            );
        }
        Harness::Agy => {
            let agy = found("agy")?;
            for name in [OLD_AGY_PLUGIN, agy_plugin::NAME] {
                let installed = agy_plugin::installed(home, name);
                let ours = name == agy_plugin::NAME && state.agy;
                if ours || state.agy_backups.iter().any(|n| n == name) || !installed.is_dir() {
                    continue;
                }
                copy_dir(&installed, &backups.join("agy").join(name))
                    .map_err(|e| format!("could not back up {}: {e}", installed.display()))?;
                state.agy_backups.push(name.to_owned());
                save(home, state)?;
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
            save(home, state)?;
            agy_plugin::install(&agy, &dir, home)?;
            say(out, format!("agy: agy plugin install {}", dir.display()));
        }
        Harness::Codex => {
            codex_file(home).install(home, bin, state, |s| &mut s.codex, out)?;
            say(out, CODEX_TRUST);
        }
        Harness::Cursor => {
            cursor_file(home).install(home, bin, state, |s| &mut s.cursor, out)?;
        }
        Harness::Copilot => install_copilot(home, bin, &found("copilot")?, state, out)?,
        Harness::Qwen => install_qwen(home, bin, &found("qwen")?, state, out)?,
    }
    Ok(())
}

/// The harnesses plotkeeper wires, in the order install goes through them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Harness {
    Claude,
    Agy,
    Codex,
    Cursor,
    Copilot,
    Qwen,
}

const HARNESSES: [Harness; 6] = [
    Harness::Claude,
    Harness::Agy,
    Harness::Codex,
    Harness::Cursor,
    Harness::Copilot,
    Harness::Qwen,
];

const CODEX_RETRUST: &str = "Codex: once, run `/hooks` in Codex and trust plotkeeper's changed hooks; Codex skips a changed hook until it is trusted again";

const CODEX_TRUST: &str = "Codex: once, run `/hooks` in Codex and trust plotkeeper's hooks; Codex skips a hook until it is trusted";

impl Harness {
    /// Its key in install's record.
    fn id(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Agy => "agy",
            Harness::Codex => "codex",
            Harness::Cursor => "cursor",
            Harness::Copilot => "copilot",
            Harness::Qwen => "qwen",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Harness::Claude => "Claude Code",
            Harness::Agy => "agy",
            Harness::Codex => "Codex",
            Harness::Cursor => "Cursor",
            Harness::Copilot => "Copilot",
            Harness::Qwen => "Qwen Code",
        }
    }

    /// Whether it is on this machine: its program on `path`, or for Cursor its folder, which its IDE and its CLI
    /// make, or its programs, the IDE's `cursor` and the CLI's `agent`.
    fn found(self, home: &Path, path: Option<&OsStr>) -> bool {
        match self {
            Harness::Cursor => {
                home.join(".cursor").is_dir()
                    || on_path("cursor", path).is_some()
                    || on_path("agent", path).is_some()
            }
            _ => on_path(self.id(), path).is_some(),
        }
    }

    /// The line install prints when it is not found.
    fn skipped(self) -> String {
        match self {
            Harness::Cursor => {
                "Cursor: `~/.cursor`, `cursor` and `agent` not found, skipped".into()
            }
            _ => format!("{}: `{}` not found, skipped", self.label(), self.id()),
        }
    }

    /// What wiring it would change, in the words install prints when it does.
    fn plan(self, home: &Path) -> Vec<String> {
        let plugins = plugins_dir(home);
        match self {
            Harness::Claude => vec![
                format!(
                    "Claude Code: claude plugin marketplace add {}",
                    plugins.join("claude").display()
                ),
                format!("Claude Code: claude plugin install {CLAUDE_ID}"),
            ],
            Harness::Agy => vec![format!(
                "agy: agy plugin install {}",
                plugins.join("agy").join(agy_plugin::NAME).display()
            )],
            Harness::Codex => vec![
                format!(
                    "Codex: add plotkeeper's hooks to {} (a backup is kept)",
                    codex_file(home).shown()
                ),
                CODEX_TRUST.to_owned(),
            ],
            Harness::Cursor => vec![format!(
                "Cursor: add plotkeeper's hooks to {} (a backup is kept)",
                cursor_file(home).shown()
            )],
            Harness::Copilot => vec![format!(
                "Copilot: copilot plugin install {}",
                plugins.join("copilot").join(NAME).display()
            )],
            Harness::Qwen => vec![format!(
                "Qwen Code: qwen extensions install {}",
                plugins.join("qwen").join(NAME).display()
            )],
        }
    }

    /// The fingerprint of what install renders for it with the hook binaries in `bin`: Claude Code's, agy's and
    /// Copilot's plugin folders, Codex's and Cursor's entries, Qwen Code's extension. It changes when a template does,
    /// and when the binaries' path does.
    fn fingerprint(self, home: &Path, bin: &Path) -> Result<String, String> {
        let entries = |file: HookFile| -> Result<Vec<(String, String)>, String> {
            let ours = Value::Object(file.ours(bin)?);
            Ok(vec![("hooks".into(), ours.to_string())])
        };
        let files = match self {
            Harness::Claude => claude_files(bin)?,
            Harness::Agy => agy_plugin::rendered(&bin.join("plotkeeper-hook"))?
                .map(|(name, text)| (name.to_owned(), text))
                .into(),
            Harness::Codex => entries(codex_file(home))?,
            Harness::Cursor => entries(cursor_file(home))?,
            Harness::Copilot => copilot_files(bin)?,
            Harness::Qwen => vec![(
                "qwen-extension.json".into(),
                render(QWEN_EXTENSION_JSON, bin)?,
            )],
        };
        Ok(fingerprint(&files))
    }

    /// Its answer in `state`; none when it has not been asked.
    fn answer(self, state: &State) -> Option<Answer> {
        state.harnesses.get(self.id()).map(|record| record.answer)
    }

    /// Whether install's flags in `state` say it wired this harness.
    fn flagged(self, state: &State) -> bool {
        match self {
            Harness::Claude => state.claude,
            Harness::Agy => state.agy,
            Harness::Codex => state.codex.is_some(),
            Harness::Cursor => state.cursor.is_some(),
            Harness::Copilot => state.copilot,
            Harness::Qwen => state.qwen,
        }
    }
}

/// Whether install's record says any harness is wired, read afresh, so the page's "not recording" notice (#437) goes as
/// soon as `plotkeeper install` wires one. A record that cannot be read counts as nothing wired.
pub fn any_wired(home: &Path) -> bool {
    load(home).ok().flatten().is_some_and(|state| {
        HARNESSES
            .into_iter()
            .any(|h| h.answer(&state) == Some(Answer::Wired))
    })
}

/// The wiring step of a bare `plotkeeper` start (#434). First it rewires, without asking, each wired harness whose
/// fingerprint changed (#438), with or without a terminal. Then, for the harnesses found on `path` that have no answer yet, it
/// prints what wiring them would change and asks `Wire these? [Y/n]` on `terminal`, then wires them on yes, with the
/// hook binaries from `beside`, or records them as declined on no. With no terminal it asks nothing, wires nothing,
/// records nothing and prints the command that wires. When every harness found has an answer and an equal fingerprint, it
/// runs no harness CLI.
pub fn wire_on_start(
    home: &Path,
    beside: &Path,
    path: Option<&OsStr>,
    terminal: Option<&mut dyn BufRead>,
    out: &mut dyn Write,
) -> Result<(), String> {
    rewire_changed(home, path, out)?;
    let mut state = load(home)?.unwrap_or_default();
    let unasked: Vec<Harness> = HARNESSES
        .into_iter()
        .filter(|h| h.found(home, path) && h.answer(&state).is_none())
        .collect();
    if unasked.is_empty() {
        return Ok(());
    }
    let names: Vec<_> = unasked.iter().map(|h| h.label()).collect();
    let not_asked = |out: &mut dyn Write| {
        // Output only: a closed terminal is no error.
        writeln!(
            out,
            "plotkeeper: {} not wired, as there is no terminal to ask in; to wire, run: plotkeeper install --yes",
            names.join(", ")
        ).ok();
    };
    let Some(terminal) = terminal else {
        not_asked(out);
        return Ok(());
    };
    // Output only: a closed terminal is no error.
    writeln!(
        out,
        "plotkeeper can record these agents by wiring its hooks into them:"
    )
    .ok();
    for harness in &unasked {
        for line in harness.plan(home) {
            say(out, line);
        }
    }
    for harness in HARNESSES {
        if !harness.found(home, path) {
            say(out, harness.skipped());
        }
    }
    say(
        out,
        format!(
            "hook binaries: copied into {}",
            studio_hook::bin_dir(home).display()
        ),
    );
    say(
        out,
        "events stay on this machine, in plotkeeper's data folder",
    );
    say(out, "to undo it: plotkeeper uninstall");
    // Output only: a closed terminal is no error.
    write!(out, "\nWire these? [Y/n] ").ok();
    // Output only: a closed terminal is no error.
    out.flush().ok();
    let mut answer = String::new();
    if terminal.read_line(&mut answer).unwrap_or(0) == 0 {
        // The terminal closed before an answer.
        writeln!(out).ok();
        not_asked(out);
        return Ok(());
    }
    if matches!(answer.trim().to_lowercase().as_str(), "" | "y" | "yes") {
        // The summary above already said each step, so they are not said again as they run.
        wire(home, beside, path, Some(&unasked), &mut std::io::sink())?;
        // Output only: a closed terminal is no error.
        writeln!(out, "plotkeeper: wired {}", names.join(", ")).ok();
        return Ok(());
    }
    for harness in &unasked {
        state.harnesses.insert(
            harness.id().to_owned(),
            HarnessRecord {
                answer: Answer::Declined,
                fingerprint: None,
            },
        );
    }
    save(home, &state)?;
    // Output only: a closed terminal is no error.
    writeln!(
        out,
        "plotkeeper: not wired; plotkeeper will not ask again. To wire later, run: plotkeeper install"
    ).ok();
    Ok(())
}

/// Rewires, without asking, each harness found on `path` that is wired and whose fingerprint differs from what install
/// would render for it now, or that has none (#438), saying so in one line each; for Codex, also that its hooks must
/// be trusted again. Runs no harness CLI when every fingerprint is equal. A declined harness is never rewired. One
/// that fails to rewire is said, and the rest are still rewired.
fn rewire_changed(home: &Path, path: Option<&OsStr>, out: &mut dyn Write) -> Result<(), String> {
    let Some(mut state) = load(home)? else {
        return Ok(());
    };
    let bin = studio_hook::bin_dir(home);
    let backups = record_dir(home).join("backups");
    for harness in HARNESSES {
        if harness.answer(&state) != Some(Answer::Wired) || !harness.found(home, path) {
            continue;
        }
        let rewired = harness.fingerprint(home, &bin).and_then(|now| {
            let stored = state
                .harnesses
                .get(harness.id())
                .and_then(|r| r.fingerprint.as_deref());
            if stored == Some(now.as_str()) {
                return Ok(false);
            }
            // Its steps are install's; the start says it in one line.
            wire_one(
                harness,
                home,
                &bin,
                path,
                &backups,
                &mut state,
                &mut std::io::sink(),
            )?;
            state.harnesses.insert(
                harness.id().to_owned(),
                HarnessRecord::wired(harness, home, &bin)?,
            );
            save(home, &state)?;
            Ok(true)
        });
        match rewired {
            Ok(true) => {}
            Ok(false) => continue,
            Err(e) => {
                // The others are still rewired, and the asking step still runs.
                writeln!(
                    out,
                    "plotkeeper: could not rewire {}: {e}; run plotkeeper install to retry",
                    harness.label()
                )
                .ok();
                continue;
            }
        }
        // Output only: a closed terminal is no error.
        writeln!(
            out,
            "plotkeeper: rewired {}, as plotkeeper's hooks for it changed",
            harness.label()
        )
        .ok();
        if harness == Harness::Codex {
            // Output only: a closed terminal is no error.
            writeln!(out, "  {CODEX_RETRUST}").ok();
        }
    }
    Ok(())
}

/// Renders `plugin-qwen/` for the binaries in `bin` into the data folder and installs it with `qwen extensions install
/// --consent`, which copies it into Qwen Code's own extension folder without a prompt; a second install replaces the
/// first, as Qwen Code refuses an extension already installed ("Please uninstall it first", `extensionManager.ts`).
/// plotkeeper opens no file of Qwen Code's.
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
        // Once it is out, or already gone ("Extension not found.", Qwen Code's `extensionManager.ts`), nothing is
        // installed until the install below, so one that fails leaves uninstall nothing to undo for Qwen Code. An
        // uninstall that failed otherwise may have left it in, so the record stays.
        let gone = run(
            qwen,
            &["extensions".as_ref(), "uninstall".as_ref(), NAME.as_ref()],
        )
        .map_or_else(|e| e.contains("Extension not found"), |()| true);
        if gone {
            state.qwen = false;
            save(home, state)?;
        }
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
    for (name, text) in copilot_files(bin)? {
        let file = dir.join(name);
        fs::write(&file, text).map_err(|e| format!("could not write {}: {e}", file.display()))?;
    }
    if state.copilot {
        // Already gone is fine: the install below puts it back.
        run(
            copilot,
            &["plugin".as_ref(), "uninstall".as_ref(), NAME.as_ref()],
        )
        .ok();
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

/// Copilot's plugin for the binaries in `bin`, each file by its name in the plugin's folder.
fn copilot_files(bin: &Path) -> Result<Vec<(String, String)>, String> {
    Ok(vec![
        ("plugin.json".into(), COPILOT_PLUGIN_JSON.to_owned()),
        ("hooks.json".into(), render(COPILOT_HOOKS_JSON, bin)?),
    ])
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

    /// `text` without plotkeeper's entries, any template's (#435); `text` itself, byte for byte, when it has none.
    // shortcut: a file that had some is written back with its keys sorted, as serde_json writes them.
    fn without_ours(&self, text: &str, bin: &Path) -> Result<String, String> {
        let mut hooks = self.parse(text)?;
        if !self.remove_ours(&mut hooks, bin)? {
            return Ok(text.to_owned());
        }
        Ok(serde_json::to_string_pretty(&hooks).map_err(|e| e.to_string())? + "\n")
    }

    /// Takes plotkeeper's entries out of `hooks`: those whose command names `bin`, whatever template wrote them, so a
    /// rewire leaves no stale entry (#434, "The fingerprint"). An event left empty goes. Whether any was taken out.
    fn remove_ours(&self, hooks: &mut Value, bin: &Path) -> Result<bool, String> {
        let bin = format!("{}/", bin.display());
        let names_bin = |handler: &Value| {
            handler
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|c| c.contains(&bin))
        };
        // Cursor's entry is a handler; Codex's is a group of handlers.
        let ours = |entry: &Value| {
            names_bin(entry)
                || entry
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_some_and(|handlers| handlers.iter().any(names_bin))
        };
        let events = self.events(hooks)?;
        let mut removed = false;
        events.retain(|_, entries| {
            let entries = entries.as_array_mut().expect("checked to be an array");
            let before = entries.len();
            entries.retain(|entry| !ours(entry));
            removed |= entries.len() != before;
            entries.len() == before || !entries.is_empty()
        });
        Ok(removed)
    }

    /// Puts plotkeeper's current entries beside the user's own, in place of any it had there. Before the first install, and before one that finds
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
        self.remove_ours(&mut hooks, bin)?;
        let events = self.events(&mut hooks)?;
        for (event, ours) in self.ours(bin)? {
            let entries = events
                .entry(event)
                .or_insert_with(|| Value::Array(Vec::new()));
            let entries = entries.as_array_mut().expect("checked to be an array");
            entries.extend(ours.as_array().into_iter().flatten().cloned());
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
        write_whole(&self.file, &text)
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
                write_whole(&self.file, old)
                    .map_err(|e| format!("could not write {}: {e}", self.shown()))?;
            } else {
                fs::remove_file(&self.file)
                    .map_err(|e| format!("could not remove {}: {e}", self.shown()))?;
                if record.made_dir {
                    // Best effort: a folder that is not empty stays.
                    fs::remove_dir(self.file.parent().unwrap_or(home)).ok();
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
            write_whole(&self.file, text)
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
    // Every answer is forgotten, so the next start asks again as on a first run.
    state.harnesses.clear();
    let mut undo = Undo {
        home,
        path,
        out,
        backups: record_dir(home).join("backups"),
        bin: studio_hook::bin_dir(home),
        state,
        failed: 0,
    };
    undo.hook_files();
    undo.copilot();
    undo.qwen();
    undo.agy();
    undo.claude();
    undo.zshrc();
    undo.zshrc_edits();
    let Undo {
        out,
        bin,
        state,
        failed,
        ..
    } = undo;
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
    let data = studio_hook::data_dir(home);
    fs::remove_dir(&data).ok();
    // Uninstall keeps what was recorded, and cannot remove the package it runs from: it says both.
    if data.exists() {
        say(
            out,
            format!(
                "your recorded history stays in {}; delete that folder to remove it too",
                data.display()
            ),
        );
    }
    say(
        out,
        "if you installed plotkeeper with npm i -g, remove it with: npm uninstall -g plotkeeper",
    );
    Ok(())
}

/// One uninstall's steps: each undoes one thing the record holds and clears it from `state` once
/// done, or says why it failed and counts it, leaving it recorded for a rerun.
struct Undo<'a> {
    home: &'a Path,
    path: Option<&'a OsStr>,
    out: &'a mut dyn Write,
    /// Where install kept the files it replaced.
    backups: PathBuf,
    bin: PathBuf,
    state: State,
    failed: usize,
}

impl Undo<'_> {
    fn fail(&mut self, e: String) {
        say(self.out, format!("failed: {e}"));
        self.failed += 1;
    }

    /// Codex's and Cursor's hooks files.
    fn hook_files(&mut self) {
        if let Some(codex) = &self.state.codex {
            match codex_file(self.home).uninstall(self.home, codex, &self.bin, self.out) {
                Ok(()) => self.state.codex = None,
                Err(e) => self.fail(e),
            }
        }
        if let Some(cursor) = &self.state.cursor {
            match cursor_file(self.home).uninstall(self.home, cursor, &self.bin, self.out) {
                Ok(()) => self.state.cursor = None,
                Err(e) => self.fail(e),
            }
        }
    }

    fn copilot(&mut self) {
        if !self.state.copilot {
            return;
        }
        let done = on_path("copilot", self.path)
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
                say(
                    self.out,
                    format!("Copilot: copilot plugin uninstall {NAME}"),
                );
                self.state.copilot = false;
            }
            Err(e) => self.fail(e),
        }
    }

    fn qwen(&mut self) {
        if !self.state.qwen {
            return;
        }
        let done = on_path("qwen", self.path)
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
                say(
                    self.out,
                    format!("Qwen Code: qwen extensions uninstall {NAME}"),
                );
                self.state.qwen = false;
            }
            Err(e) => self.fail(e),
        }
    }

    /// agy's plugin, and the plugins of the same name install moved aside.
    fn agy(&mut self) {
        if !self.state.agy && self.state.agy_backups.is_empty() {
            return;
        }
        let Some(agy) = on_path("agy", self.path) else {
            self.fail("agy: `agy` not found, so its plugin cannot be uninstalled".into());
            return;
        };
        if self.state.agy {
            match agy_plugin::uninstall(&agy, agy_plugin::NAME, self.home) {
                Ok(()) => {
                    say(
                        self.out,
                        format!("agy: agy plugin uninstall {}", agy_plugin::NAME),
                    );
                    self.state.agy = false;
                }
                Err(e) => self.fail(e),
            }
        }
        let mut left = Vec::new();
        for name in std::mem::take(&mut self.state.agy_backups) {
            match agy_plugin::install(&agy, &self.backups.join("agy").join(&name), self.home) {
                Ok(()) => say(self.out, format!("agy: put back the plugin `{name}`")),
                Err(e) => {
                    self.fail(e);
                    left.push(name);
                }
            }
        }
        self.state.agy_backups = left;
    }

    /// Claude Code's plugin, then its marketplace.
    fn claude(&mut self) {
        if !self.state.claude {
            return;
        }
        let Some(claude) = on_path("claude", self.path) else {
            self.fail(
                "Claude Code: `claude` not found, so its plugin cannot be uninstalled".into(),
            );
            return;
        };
        if !self.state.claude_plugin_gone {
            match run(
                &claude,
                &["plugin".as_ref(), "uninstall".as_ref(), CLAUDE_ID.as_ref()],
            ) {
                Ok(()) => {
                    say(
                        self.out,
                        format!("Claude Code: claude plugin uninstall {CLAUDE_ID}"),
                    );
                    self.state.claude_plugin_gone = true;
                }
                Err(e) => self.fail(e),
            }
        }
        if self.state.claude_plugin_gone {
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
                        self.out,
                        format!("Claude Code: claude plugin marketplace remove {NAME}"),
                    );
                    self.state.claude = false;
                    self.state.claude_plugin_gone = false;
                }
                Err(e) => self.fail(e),
            }
        }
    }

    /// The `~/.zshrc` install wrote whole, put back from its backup while it is as install left it.
    fn zshrc(&mut self) {
        let Some(written) = &self.state.zshrc else {
            return;
        };
        let zshrc = self.home.join(".zshrc");
        let backup = self.backups.join("zshrc");
        if fs::read_to_string(&zshrc).is_ok_and(|now| &now == written) {
            let put_back = fs::read(&backup)
                .map_err(|e| format!("could not read the backup of ~/.zshrc: {e}"))
                .and_then(|old| {
                    write_whole(&zshrc, old).map_err(|e| format!("could not write ~/.zshrc: {e}"))
                });
            match put_back.and_then(|()| {
                fs::remove_file(&backup)
                    .map_err(|e| format!("could not delete the backup of ~/.zshrc: {e}"))
            }) {
                Ok(()) => {
                    say(self.out, "~/.zshrc: put back as it was");
                    self.state.zshrc = None;
                }
                Err(e) => self.fail(e),
            }
        } else {
            // Not a failure: only the backup of `~/.zshrc` is left to give back, so the record stays for it.
            say(
                self.out,
                format!(
                    "~/.zshrc changed since install, so it is left as it is; its old copy is {}",
                    backup.display()
                ),
            );
        }
    }

    /// The lines install removed from `~/.zshrc`, put back where they were.
    fn zshrc_edits(&mut self) {
        let Some(record) = &self.state.zshrc_edits else {
            return;
        };
        let zshrc = self.home.join(".zshrc");
        let put_back = fs::read_to_string(&zshrc)
            .ok()
            .and_then(|now| with_removed_lines(&now, record));
        match put_back {
            Some(text) => match write_whole(&zshrc, text) {
                Ok(()) => {
                    say(self.out, "~/.zshrc: put back as it was");
                    self.state.zshrc_edits = None;
                }
                Err(e) => self.fail(format!("could not write ~/.zshrc: {e}")),
            },
            None => {
                // Not a failure: only these lines are left to give back, so the record stays for them.
                let lines: String = record
                    .edits
                    .iter()
                    .flat_map(|e| e.removed.iter().map(String::as_str))
                    .collect();
                say(
                    self.out,
                    format!(
                        "~/.zshrc changed since install, so it is left as it is; the lines install removed were:\n{}",
                        lines.trim_end()
                    ),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record of `harnesses` as #436's build reads and writes it: an answer, nothing else.
    #[derive(Deserialize)]
    struct Before438 {
        harnesses: BTreeMap<String, Before438Record>,
    }

    #[derive(Deserialize)]
    struct Before438Record {
        answer: Answer,
    }

    #[test]
    fn a_record_with_fingerprints_reads_in_a_build_before_them_and_one_without_reads_here() {
        let mut state = State::default();
        state.harnesses.insert(
            "codex".into(),
            HarnessRecord {
                answer: Answer::Wired,
                fingerprint: Some("0123456789abcdef".into()),
            },
        );
        let text = serde_json::to_string(&state).unwrap();
        assert!(
            text.contains("\"fingerprint\":\"0123456789abcdef\""),
            "{text}"
        );
        let old: Before438 = serde_json::from_str(&text).unwrap();
        assert_eq!(old.harnesses["codex"].answer, Answer::Wired);

        let text = r#"{"claude":false,"agy":false,"agy_backups":[],"codex":null,"cursor":null,"harnesses":{"codex":{"answer":"wired"},"agy":{"answer":"declined"}}}"#;
        let state: State = serde_json::from_str(text).unwrap();
        assert_eq!(state.harnesses["codex"].answer, Answer::Wired);
        assert_eq!(state.harnesses["codex"].fingerprint, None);
        assert_eq!(state.harnesses["agy"].answer, Answer::Declined);
    }

    #[test]
    fn a_record_from_before_answers_counts_its_flags_as_wired_and_a_later_one_does_not() {
        let before = r#"{"claude":true,"agy":false,"agy_backups":[],"codex":null,"cursor":null}"#;
        let state = serde_json::from_str::<State>(before)
            .unwrap()
            .with_answers();
        assert_eq!(Harness::Claude.answer(&state), Some(Answer::Wired));
        assert_eq!(Harness::Agy.answer(&state), None);

        let after = r#"{"claude":true,"claude_plugin_gone":true,"agy":false,"agy_backups":[],"codex":null,"cursor":null,"answers_kept":true}"#;
        let state = serde_json::from_str::<State>(after).unwrap().with_answers();
        assert_eq!(Harness::Claude.answer(&state), None);
    }
}
