//! `plotkeeper install` and `plotkeeper uninstall` (#357; spec #320, "Install and distribution").
//!
//! Install copies the hook binaries into the data folder's `bin/`, renders each harness's plugin with that path into
//! the data folder, and wires the harnesses it finds on the PATH at user level, each through its own CLI: Claude Code
//! with `claude plugin`, agy with `agy plugin`. It also removes the archived app's setup, so no event is recorded twice:
//! the `CLAUDE_CODE_PLUGIN_DIRS` entries in `~/.zshrc` that name a data folder's `checkout/plugin`, and agy's plugin
//! `studio`. What it changes it backs up first, and `uninstall` puts it back.
//!
//! plotkeeper never opens Claude Code's settings files (AGENTS.md, Hard rules; spec #320, "Install obeys the
//! credentials rule"): `claude plugin` writes them.

use serde::{Deserialize, Serialize};
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
/// The archived app's plugin for agy, by its folder's name.
const OLD_AGY_PLUGIN: &str = "studio";

/// What install did, kept in the data folder so uninstall undoes that and nothing else.
#[derive(Default, Serialize, Deserialize)]
struct State {
    /// The marketplace was added with `claude plugin`.
    claude: bool,
    /// plotkeeper's agy plugin was installed with `agy plugin`.
    agy: bool,
    /// `~/.zshrc` was rewritten to this text; its old bytes are in `backups/zshrc`.
    zshrc: Option<String>,
    /// agy plugins whose folders were backed up under `backups/agy/<name>` before install replaced or removed them.
    agy_backups: Vec<String>,
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

/// `profile` without the old plugin entries of its `export CLAUDE_CODE_PLUGIN_DIRS=` lines: a line left with no entry
/// goes, one with others keeps them, and the archived app's comment above a changed line goes too.
// shortcut: reads `export NAME=value` lines only, quoted or bare, as the archived app wrote them.
pub fn without_old_plugin_dirs(profile: &str) -> String {
    const EXPORT: &str = "export CLAUDE_CODE_PLUGIN_DIRS=";
    let mut lines: Vec<String> = Vec::new();
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
        if lines.last().is_some_and(|l| {
            l.trim_start().starts_with('#')
                && l.trim_end().ends_with("for every claude session (#21).")
        }) {
            lines.pop();
        }
        let kept: Vec<_> = value
            .split(':')
            .filter(|d| !d.is_empty() && !old_plugin_dir(d))
            .collect();
        if !kept.is_empty() {
            lines.push(format!("{indent}{EXPORT}\"{}\"{ending}", kept.join(":")));
        }
    }
    lines.concat()
}

/// Writes Claude Code's plugin for the binaries in `bin` into `dir`, as a marketplace of one plugin, which is how
/// `claude plugin install` takes a local folder.
fn render_claude(dir: &Path, bin: &Path) -> Result<(), String> {
    // The path sits in a JSON string, inside the shell's double quotes, so it is escaped as one.
    let path = serde_json::to_string(&bin.display().to_string()).map_err(|e| e.to_string())?;
    let path = &path[1..path.len() - 1];
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
        (
            format!("{NAME}/hooks/hooks.json"),
            HOOKS_JSON.replace(PLACEHOLDER, path),
        ),
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

    // The archived app's line in `~/.zshrc`, once: a second install keeps the first backup.
    let zshrc = home.join(".zshrc");
    if state.zshrc.is_none()
        && let Ok(text) = fs::read_to_string(&zshrc)
    {
        let cleaned = without_old_plugin_dirs(&text);
        if cleaned != text {
            fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
            fs::write(backups.join("zshrc"), &text)
                .map_err(|e| format!("could not back up ~/.zshrc: {e}"))?;
            state.zshrc = Some(cleaned.clone());
            save(home, &state)?;
            fs::write(&zshrc, cleaned).map_err(|e| format!("could not write ~/.zshrc: {e}"))?;
            say(
                out,
                format!(
                    "~/.zshrc: removed the old plugin path from CLAUDE_CODE_PLUGIN_DIRS (backup in {})",
                    backups.display()
                ),
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
    Ok(())
}

/// `plotkeeper uninstall` for `home`, with the harnesses found on `path`: undoes what install recorded, restores its
/// backups, and removes the data folder's `bin/`, the rendered plugins and the record. Prints each step to `out`.
pub fn uninstall(home: &Path, path: Option<&OsStr>, out: &mut dyn Write) -> Result<(), String> {
    let Some(state) = load(home)? else {
        say(out, "plotkeeper install has not run here; nothing to undo");
        return Ok(());
    };
    let backups = record_dir(home).join("backups");
    let mut keep_record = false;

    if state.agy || !state.agy_backups.is_empty() {
        let agy = on_path("agy", path)
            .ok_or("agy: `agy` not found, so its plugin cannot be uninstalled")?;
        if state.agy {
            agy_plugin::uninstall(&agy, agy_plugin::NAME, home)?;
            say(
                out,
                format!("agy: agy plugin uninstall {}", agy_plugin::NAME),
            );
        }
        for name in &state.agy_backups {
            agy_plugin::install(&agy, &backups.join("agy").join(name), home)?;
            say(out, format!("agy: put back the plugin `{name}`"));
        }
    }

    if state.claude {
        let claude = on_path("claude", path)
            .ok_or("Claude Code: `claude` not found, so its plugin cannot be uninstalled")?;
        run(
            &claude,
            &["plugin".as_ref(), "uninstall".as_ref(), CLAUDE_ID.as_ref()],
        )?;
        say(
            out,
            format!("Claude Code: claude plugin uninstall {CLAUDE_ID}"),
        );
        run(
            &claude,
            &[
                "plugin".as_ref(),
                "marketplace".as_ref(),
                "remove".as_ref(),
                NAME.as_ref(),
            ],
        )?;
        say(
            out,
            format!("Claude Code: claude plugin marketplace remove {NAME}"),
        );
    }

    if let Some(written) = &state.zshrc {
        let zshrc = home.join(".zshrc");
        if fs::read_to_string(&zshrc).is_ok_and(|now| &now == written) {
            let old = fs::read(backups.join("zshrc"))
                .map_err(|e| format!("could not read the backup of ~/.zshrc: {e}"))?;
            fs::write(&zshrc, old).map_err(|e| format!("could not write ~/.zshrc: {e}"))?;
            say(out, "~/.zshrc: put back as it was");
        } else {
            keep_record = true;
            say(
                out,
                format!(
                    "~/.zshrc changed since install, so it is left as it is; its old copy is {}",
                    backups.join("zshrc").display()
                ),
            );
        }
    }

    let mut gone = vec![studio_hook::bin_dir(home), plugins_dir(home)];
    if keep_record {
        // Only the backup of `~/.zshrc` is left to give back.
        let zshrc = state.zshrc.clone();
        save(
            home,
            &State {
                zshrc,
                ..State::default()
            },
        )?;
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
