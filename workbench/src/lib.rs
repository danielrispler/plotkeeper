//! Workbench git (danielrispler/Falinks#328, module 7; danielrispler/Falinks#461, decision 8): a folder's main
//! checkout, `origin`'s fetch and what removing a task's worktree would lose, with no call passing git's force
//! option; the `gh` to run; and the agy plugin. What
//! only its own tests used is in `archive/workbench/` (#415), with the `claude` command and status line wrapper
//! a studio terminal ran and the context window its snapshots held (#416).

// Spec #548, Group 3: these two crates document their public API.
#![warn(missing_docs)]

pub mod clock;
pub mod run;

use serde::Serialize;
use std::{path::Path, process::Command, sync::OnceLock, time::Duration};

/// The main checkout of the repository that holds `folder`, which may be any of its worktrees: the parent
/// of git's common dir. A folder git does not recognise is refused with a message.
pub fn main_checkout(folder: &Path) -> Result<String, String> {
    let common = git(
        folder,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        GIT_READ_LIMIT,
    )
    .map_err(|e| format!("{} is not in a git repository: {e}", folder.display()))?;
    Path::new(&common)
        .parent()
        .map(|main| main.display().to_string())
        .ok_or_else(|| format!("{} is not in a git repository", folder.display()))
}

/// Fetches `origin` once for the repository that holds `repo` (#33), whichever of its worktrees that
/// is: the refs are shared. A failure returns git's message and leaves the refs as they were. It
/// writes `refs/remotes/*` and `FETCH_HEAD` only, never a branch, the index or a worktree's files.
pub fn fetch(repo: &Path) -> Result<(), String> {
    git(repo, &["fetch", "--quiet", "origin"], FETCH_LIMIT).map(drop)
}

/// The `gh` to run: the first on Daniel's login shell's `PATH`, found once; bare `gh` when it
/// finds none.
pub fn gh_path() -> &'static str {
    static GH: OnceLock<String> = OnceLock::new();
    GH.get_or_init(|| {
        // An app started from the Dock gets launchd's `PATH`, which lacks Homebrew's folder.
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        let found = Command::new(shell).args(["-lc", "command -v gh"]).output();
        let found = found.ok().filter(|o| o.status.success()).map(|o| o.stdout);
        found
            .as_deref()
            .map(String::from_utf8_lossy)
            .and_then(|out| out.lines().last().map(str::to_owned))
            .filter(|path| path.starts_with('/'))
            .unwrap_or_else(|| "gh".into())
    })
}

/// What removing a task's worktree would lose: its uncommitted changes as `git status --short`
/// lines, and its commits that no branch at `origin` holds, as `<short hash> <subject>`.
#[derive(Debug, PartialEq, Serialize)]
pub struct Losses {
    /// Its uncommitted changes, as `git status --short` lines.
    pub uncommitted: Vec<String>,
    /// Its commits that no branch at `origin` holds, as `<short hash> <subject>`.
    pub unpushed: Vec<String>,
}

/// What removing the worktree `task` would lose: its uncommitted changes and its unpushed commits.
/// Read-only; the same two checks tell removal and **Landed** that nothing is at risk.
pub fn losses(task: &Path) -> Result<Losses, String> {
    let lines = |text: String| text.lines().map(String::from).collect::<Vec<_>>();
    Ok(Losses {
        uncommitted: lines(git(task, &["status", "--short"], GIT_READ_LIMIT)?),
        unpushed: lines(git(
            task,
            &["log", "--format=%h %s", "HEAD", "--not", "--remotes"],
            GIT_READ_LIMIT,
        )?),
    })
}

/// The longest a read of the repository on this machine may take.
const GIT_READ_LIMIT: Duration = Duration::from_secs(30);
/// The longest a fetch may take: it waits on the network.
const FETCH_LIMIT: Duration = Duration::from_secs(120);

/// Runs `git` in `dir` within `limit` and returns its output, or what went wrong: for a failed git, its own message.
fn git(dir: &Path, args: &[&str], limit: Duration) -> Result<String, String> {
    let output = run::run_until(run::git(dir).args(args), limit).map_err(|e| match e {
        run::RunError::Failed(_) => e.to_string(),
        e => format!("git {}: {e}", args[0]),
    })?;
    // Only the end: a status line may start with a space.
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().into())
}

/// plotkeeper's plugin for agy (#43, decisions 8 and 9; #49). `agy plugin install` copies the
/// folder, so `plotkeeper install` renders the shipped `plugin-agy/`, embedded here, with `plotkeeper-hook`'s path into
/// a folder of its own (#357), and installs that folder.
pub mod agy_plugin {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    /// The plugin's name in `plugin.json`, and its folder under `~/.gemini/config/plugins/`.
    pub const NAME: &str = "plotkeeper";
    const PLACEHOLDER: &str = "__PLOTKEEPER_HOOK__";
    const PLUGIN_JSON: &str = include_str!("../../plugin-agy/plugin.json");
    const HOOKS_JSON: &str = include_str!("../../plugin-agy/hooks.json");

    /// Whether the plugin's copy in agy's config is the one for this hook.
    #[derive(Debug, PartialEq, Eq)]
    pub enum Status {
        /// Installed, with the same files.
        Current,
        /// Not installed.
        Missing,
        /// Installed, with files that differ.
        Stale,
    }

    /// The plugin's two files as installed: the shipped ones, with `hook` where the hooks name it.
    pub fn rendered(hook: &Path) -> Result<[(&'static str, String); 2], String> {
        // The path sits in a JSON string, so it is escaped as one.
        let path = serde_json::to_string(&hook.display().to_string()).map_err(|e| e.to_string())?;
        let path = &path[1..path.len() - 1];
        Ok([
            ("plugin.json", PLUGIN_JSON.to_owned()),
            ("hooks.json", HOOKS_JSON.replace(PLACEHOLDER, path)),
        ])
    }

    /// Writes the plugin for `hook` into `dir`, which is created.
    pub fn render(dir: &Path, hook: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("could not make {}: {e}", dir.display()))?;
        for (name, text) in rendered(hook)? {
            std::fs::write(dir.join(name), text)
                .map_err(|e| format!("could not write {}/{name}: {e}", dir.display()))?;
        }
        Ok(())
    }

    /// The folder agy keeps its copy of the plugin `name` in, under `home`.
    pub fn installed(home: &Path, name: &str) -> PathBuf {
        home.join(".gemini/config/plugins").join(name)
    }

    /// Whether the copy in `home`'s agy config is the plugin for `hook`.
    pub fn status(hook: &Path, home: &Path) -> Result<Status, String> {
        let dir = installed(home, NAME);
        if !dir.join("hooks.json").exists() {
            return Ok(Status::Missing);
        }
        let same = rendered(hook)?
            .iter()
            .all(|(name, text)| std::fs::read_to_string(dir.join(name)).is_ok_and(|t| &t == text));
        Ok(if same { Status::Current } else { Status::Stale })
    }

    /// Installs the plugin folder `dir` with `agy` (the command), for `home`: validates it first, as install does
    /// not and skips an invalid `hooks.json` silently, then replaces any installed copy of the plugin of `dir`'s name.
    pub fn install(agy: &Path, dir: &Path, home: &Path) -> Result<(), String> {
        let run = |args: &[&str]| {
            Command::new(agy)
                .args(["plugin"])
                .args(args)
                .arg(dir)
                .env("HOME", home)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("could not run agy: {e}"))
        };
        let valid = run(&["validate"])?;
        if !valid.status.success() {
            return Err(format!(
                "agy did not validate the plugin in {}: {}",
                dir.display(),
                String::from_utf8_lossy(&valid.stdout)
            ));
        }
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            // An uninstall of what is not there fails, and that is fine.
            uninstall(agy, name, home).ok();
        }
        let done = run(&["install"])?;
        if !done.status.success() {
            return Err(format!(
                "agy plugin install failed: {}",
                String::from_utf8_lossy(&done.stderr)
            ));
        }
        Ok(())
    }

    /// Uninstalls the plugin `name` with `agy`, for `home`.
    pub fn uninstall(agy: &Path, name: &str, home: &Path) -> Result<(), String> {
        let out = Command::new(agy)
            .args(["plugin", "uninstall", name])
            .env("HOME", home)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("could not run agy: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "agy plugin uninstall {name} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(())
    }
}
