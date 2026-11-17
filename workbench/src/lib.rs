//! Workbench git (danielrispler/Falinks#328, module 7; danielrispler/Falinks#461, decision 8): the workspaces a session can start in, read live
//! from `git worktree list --porcelain` with no stored copy, and the `claude` command a studio
//! terminal runs. Beside them, creating and removing tasks (danielrispler/Falinks#468; danielrispler/Falinks#461, decision 9): no call passes
//! git's force option, so git refuses whatever would lose work, and its message is shown as it is.

mod usage;
pub use usage::{
    AgentUsage, LimitGroup, UsageWindow, claude_usage, cli_usages, parse_agy, parse_codex,
    read_agy, read_codex,
};

use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};

/// A main checkout or a task's worktree: its folder, and its branch, `None` at a detached `HEAD`.
#[derive(Debug, PartialEq, Serialize)]
pub struct Workspace {
    pub path: String,
    pub branch: Option<String>,
}

/// The repository's main checkout, its tasks, the worktrees under `.claude/worktrees/` in `git`'s
/// order, and how many `agent-*` worktrees there were hidden.
#[derive(Debug, PartialEq, Serialize)]
pub struct Workspaces {
    pub main: Workspace,
    pub tasks: Vec<Workspace>,
    pub hidden: usize,
}

/// The studio's projects (#93), from `file`: an ordered list of main-checkout paths. With no file
/// it is created from `seed`, one entry or none, and from then on the file decides. Written by a
/// temporary file and a rename, as the card store does.
pub fn projects(file: &Path, seed: impl FnOnce() -> Option<String>) -> Result<Vec<String>, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| format!("{} does not parse: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let list: Vec<String> = seed().into_iter().collect();
            write_projects(file, &list)?;
            Ok(list)
        }
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

fn write_projects(file: &Path, list: &[String]) -> Result<(), String> {
    let dir = file.parent().ok_or("no folder for projects.json")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, list).map_err(|e| e.to_string())?;
    temp.persist(file).map_err(|e| e.to_string())?;
    Ok(())
}

/// The main checkout of the repository that holds `folder`, which may be any of its worktrees: the parent
/// of git's common dir. A folder git does not recognise is refused with a message.
pub fn main_checkout(folder: &Path) -> Result<String, String> {
    let common = git(
        folder,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .map_err(|_| format!("{} is not in a git repository", folder.display()))?;
    Path::new(&common)
        .parent()
        .map(|main| main.display().to_string())
        .ok_or_else(|| format!("{} is not in a git repository", folder.display()))
}

/// Adds the repository holding `folder` to the projects in `file`, at the end, as its main checkout, and
/// returns the new list. `seed` is as for `projects`, for a file not made yet. A folder git does not recognise and a repository already listed are refused.
pub fn add_project(
    file: &Path,
    seed: impl FnOnce() -> Option<String>,
    folder: &Path,
) -> Result<Vec<String>, String> {
    let mut list = projects(file, seed)?;
    let main = main_checkout(folder)?;
    if list.contains(&main) {
        return Err(format!("{main} is already a project"));
    }
    list.push(main);
    write_projects(file, &list)?;
    Ok(list)
}

/// Takes `path` out of the projects in `file` and returns the new list. Only the entry goes: the
/// repository's worktrees, branches and sessions are not touched.
pub fn remove_project(
    file: &Path,
    seed: impl FnOnce() -> Option<String>,
    path: &str,
) -> Result<Vec<String>, String> {
    let mut list = projects(file, seed)?;
    list.retain(|project| project != path);
    write_projects(file, &list)?;
    Ok(list)
}

/// Writes the projects of `file` in `order`, which must hold the same projects, and returns the list.
pub fn reorder_projects(
    file: &Path,
    seed: impl FnOnce() -> Option<String>,
    order: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut have = projects(file, seed)?;
    let mut want = order.clone();
    have.sort();
    want.sort();
    if have != want {
        return Err("the new order does not hold the same projects".into());
    }
    write_projects(file, &order)?;
    Ok(order)
}

/// The repositories to suggest as projects: the main checkout of each of `cwds`, newest first as given,
/// each once, without those already in the projects of `file`. A folder that is gone or not in a
/// repository is skipped.
pub fn suggest_projects(
    file: &Path,
    seed: impl FnOnce() -> Option<String>,
    cwds: &[String],
) -> Result<Vec<String>, String> {
    let listed = projects(file, seed)?;
    let mut found: Vec<String> = Vec::new();
    for cwd in cwds {
        if let Ok(main) = main_checkout(Path::new(cwd))
            && !listed.contains(&main)
            && !found.contains(&main)
        {
            found.push(main);
        }
    }
    Ok(found)
}

/// The workspaces of the repository that holds `repo`, which may be any of its worktrees.
pub fn workspaces(repo: &Path) -> Result<Workspaces, String> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git worktree list: {e}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("git worktree list: {}", error.trim()));
    }
    // Each worktree is a block of lines; the main checkout's comes first.
    let text = String::from_utf8_lossy(&output.stdout);
    let mut listed = text.split("\n\n").filter_map(|block| {
        let mut workspace = Workspace {
            path: String::new(),
            branch: None,
        };
        for line in block.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                workspace.path = path.into();
            } else if let Some(branch) = line.strip_prefix("branch ") {
                workspace.branch = Some(branch.trim_start_matches("refs/heads/").into());
            } else if line == "bare" || line.starts_with("prunable") {
                // A bare repository has no files, and a prunable worktree's folder is gone.
                return None;
            }
        }
        (!workspace.path.is_empty()).then_some(workspace)
    });
    let main = listed.next().ok_or("git listed no main checkout")?;
    let under = format!("{}/.claude/worktrees/", main.path);
    let (agents, tasks): (Vec<Workspace>, Vec<Workspace>) = listed
        .filter(|workspace| workspace.path.starts_with(&under))
        .partition(|workspace| workspace.path[under.len()..].starts_with("agent-"));
    Ok(Workspaces {
        main,
        tasks,
        hidden: agents.len(),
    })
}

/// A workspace's git state (#26, #29): its branch, `None` at a detached `HEAD`; its distance from its
/// upstream, commits ahead and behind, `None` with no upstream (never pushed, or detached); and how
/// many files `git status` lists as changed; how far it is behind `origin`'s default branch and
/// whether a rebase onto it would conflict; and the operation in progress.
#[derive(Debug, PartialEq, Serialize)]
pub struct GitState {
    pub branch: Option<String>,
    pub upstream: Option<(u32, u32)>,
    pub changed: usize,
    /// What a half-done operation is: "rebase", "merge", "cherry-pick", "revert" or "conflicts"
    /// (files unmerged with none of those); `None` when nothing is in progress (#31).
    pub operation: Option<String>,
    /// Commits `origin`'s default branch has that this branch lacks; `None` when level with it or
    /// when `origin/HEAD` is not set (#30).
    pub main_behind: Option<u32>,
    /// Whether the branch is the default branch itself, where main distance is the upstream
    /// distance and shows once.
    pub on_default: bool,
    /// Whether rebasing the branch onto `origin`'s default branch would conflict; `false` when it
    /// is not behind it (#32).
    pub would_conflict: bool,
    /// Whether pulling `origin`'s default branch into this checkout would overwrite its files: a
    /// changed or untracked file the incoming commits also touch (#164). Only on the default branch.
    pub would_overwrite: bool,
}

/// The git state of the workspace at `path`, one of the workspaces of `repo`; any other path is refused, so
/// the window cannot run git in an arbitrary folder. Read-only, and with no optional locks.
pub fn git_state(repo: &Path, path: &str) -> Result<GitState, String> {
    let known = workspaces(repo)?;
    if known.main.path != path && !known.tasks.iter().any(|task| task.path == path) {
        return Err(format!("not a workspace of this repository: {path}"));
    }
    let dir = Path::new(path);
    let branch = git(dir, &["symbolic-ref", "--short", "-q", "HEAD"]).ok();
    // git fails when there is no upstream, as at a detached `HEAD` or on a branch never pushed.
    let upstream = git(
        dir,
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )
    .ok()
    .and_then(|line| {
        let (ahead, behind) = line.split_once('\t')?;
        Some((ahead.parse().ok()?, behind.parse().ok()?))
    });
    let changed = git(dir, &["status", "--porcelain"])?.lines().count();
    let operation = operation(dir)?;
    let default = default_branch(dir);
    let on_default =
        matches!((&branch, &default), (Some(b), Some(d)) if d.strip_prefix("origin/") == Some(b));
    let main_behind = default
        .as_ref()
        .and_then(|d| git(dir, &["rev-list", "--count", &format!("HEAD..{d}")]).ok())
        .and_then(|n| n.parse().ok())
        .filter(|n| *n > 0);
    let would_conflict = match (&default, on_default, main_behind) {
        (Some(d), false, Some(_)) => would_conflict(dir, d),
        _ => false,
    };
    let would_overwrite = match (&default, on_default) {
        (Some(d), true) => would_overwrite(dir, d),
        _ => false,
    };
    Ok(GitState {
        branch,
        upstream,
        changed,
        operation,
        main_behind,
        on_default,
        would_conflict,
        would_overwrite,
    })
}

/// What a workspace's would-conflict check last said, with the tips it said it for and how often the
/// check ran.
struct Checked {
    head: String,
    default: String,
    conflicts: bool,
    runs: usize,
}

static CHECKED: Mutex<BTreeMap<String, Checked>> = Mutex::new(BTreeMap::new());

/// How many times the would-conflict check ran for the workspace at `path` (#32); a test reads it.
pub fn merge_tree_runs(path: &str) -> usize {
    let checked = CHECKED.lock().unwrap();
    checked.get(path).map_or(0, |c| c.runs)
}

/// Whether rebasing `HEAD` onto `default` would conflict, predicted by `git merge-tree
/// --write-tree`, which touches no worktree, index or ref. Cached per workspace on the pair (`HEAD`
/// sha, default branch sha), so it runs again only when either moves (#32).
fn would_conflict(dir: &Path, default: &str) -> bool {
    let (Ok(head), Ok(tip)) = (
        git(dir, &["rev-parse", "HEAD"]),
        git(dir, &["rev-parse", default]),
    ) else {
        return false;
    };
    let mut checked = CHECKED.lock().unwrap();
    let key = dir.display().to_string();
    if let Some(c) = checked.get(&key)
        && c.head == head
        && c.default == tip
    {
        return c.conflicts;
    }
    // Exit status 1 is a conflict; any other failure is not a prediction.
    let conflicts = run(dir, &["merge-tree", "--write-tree", "--quiet", &head, &tip])
        .is_ok_and(|output| output.status.code() == Some(1));
    let runs = checked.get(&key).map_or(0, |c| c.runs) + 1;
    checked.insert(
        key,
        Checked {
            head,
            default: tip,
            conflicts,
            runs,
        },
    );
    conflicts
}

/// Whether fast-forwarding `dir`'s checked-out branch to `default` would overwrite a file: one that is
/// changed or untracked and that the incoming commits `HEAD..default` also touch (#164). Unrelated
/// changes and untracked files elsewhere do not count.
fn would_overwrite(dir: &Path, default: &str) -> bool {
    // `-z` on both: raw, unquoted paths, so a non-ASCII name compares equal on both sides.
    let Ok(incoming) = run(
        dir,
        &["diff", "--name-only", "-z", &format!("HEAD..{default}")],
    ) else {
        return false;
    };
    let Ok(status) = run(dir, &["status", "--porcelain", "-z", "-uall"]) else {
        return false;
    };
    if !incoming.status.success() || !status.status.success() {
        return false;
    }
    let status = String::from_utf8_lossy(&status.stdout);
    // Entries are "XY path"; a rename or copy is followed by its other path as an entry of its own.
    let mut changed: Vec<&str> = Vec::new();
    let mut entries = status.split('\0');
    while let Some(entry) = entries.next() {
        let Some(path) = entry.get(3..) else { continue };
        changed.push(path);
        if entry.starts_with(['R', 'C']) || entry[1..2].starts_with(['R', 'C']) {
            changed.extend(entries.next());
        }
    }
    String::from_utf8_lossy(&incoming.stdout)
        .split('\0')
        .any(|path| !path.is_empty() && changed.contains(&path))
}

/// The operation in progress in the workspace at `dir`, from the files its repository keeps while one is stopped.
fn operation(dir: &Path) -> Result<Option<String>, String> {
    let marked = |name: &str| -> Result<bool, String> {
        let path = git(dir, &["rev-parse", "--git-path", name])?;
        Ok(dir.join(path).exists())
    };
    for (name, operation) in [
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("MERGE_HEAD", "merge"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
    ] {
        if marked(name)? {
            return Ok(Some(operation.into()));
        }
    }
    let unmerged = !git(dir, &["ls-files", "--unmerged"])?.is_empty();
    Ok(unmerged.then(|| "conflicts".into()))
}

/// Fetches `origin` once for the repository that holds `repo` (#33), whichever of its worktrees that
/// is: the refs are shared. A failure returns git's message and leaves the refs as they were. It
/// writes `refs/remotes/*` and `FETCH_HEAD` only, never a branch, the index or a worktree's files.
pub fn fetch(repo: &Path) -> Result<(), String> {
    git(repo, &["fetch", "--quiet", "origin"]).map(drop)
}

/// The remote's default branch as a ref, such as `origin/main`; none when origin's HEAD is not known.
fn default_branch(dir: &Path) -> Option<String> {
    git(
        dir,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
    )
    .ok()
}

/// Brings the default branch of the repository that holds `repo` up to `origin`'s (#164), fetching
/// first. With the default branch checked out in the main checkout it is a fast-forward-only merge;
/// otherwise the local branch moves without a checkout, as `git fetch origin main:main` does. It never
/// merges, rebases or stashes, and no other branch moves. Git's message is the error when the branch
/// has diverged or uncommitted changes are in the way.
pub fn pull(repo: &Path) -> Result<(), String> {
    let main = workspaces(repo)?.main.path;
    let main = Path::new(&main);
    fetch(main)?;
    let remote = default_branch(main).ok_or("origin's default branch is not known")?;
    let name = remote.strip_prefix("origin/").unwrap_or(&remote);
    if git(main, &["symbolic-ref", "--short", "-q", "HEAD"]).is_ok_and(|b| b == name) {
        git(main, &["merge", "--ff-only", "--quiet", &remote]).map(drop)
    } else {
        git(
            main,
            &["fetch", "--quiet", "origin", &format!("{name}:{name}")],
        )
        .map(drop)
    }
}

/// The command a studio terminal runs for a new session, or for `resume`, an ended session's id. It
/// sets `CLAUDE_CODE_PLUGIN_DIRS` to the studio plugin at `plugin` alone, as the README loads it,
/// so a profile that sets it too never loads the hooks twice, and `CLAUDE_CLIENT_PRESENCE_FILE` to
/// `presence`, which the shell keeps while the window is focused, so Remote Control skips a phone
/// push then (danielrispler/Falinks#522). The daemon runs it under a login shell with the terminal's `STUDIO_TERMINAL_ID`.
/// `auto` runs it in Claude Code's auto mode (danielrispler/Falinks#541), which still asks when its classifier blocks a
/// step; bypass mode is never offered.
pub fn claude(plugin: &Path, presence: &Path, resume: Option<&str>, auto: bool) -> Vec<String> {
    let mut argv = vec![
        "env".into(),
        format!("CLAUDE_CODE_PLUGIN_DIRS={}", plugin.display()),
        format!("CLAUDE_CLIENT_PRESENCE_FILE={}", presence.display()),
        "claude".into(),
        "--settings".into(),
        status_line(plugin),
    ];
    if auto {
        argv.extend(["--permission-mode".into(), "auto".into()]);
    }
    if let Some(id) = resume {
        argv.extend(["--resume".into(), id.into()]);
    }
    argv
}

/// Points `checkout` in the studio's data folder `data` at `checkout` (#21), so a profile that names
/// the plugin through it survives a moved checkout; returns that plugin path. The hooks'
/// `"${CLAUDE_PLUGIN_ROOT}"/../target` resolves through the link into the real checkout. A real
/// folder there is never replaced.
pub fn link_checkout(data: &Path, checkout: &Path) -> std::io::Result<PathBuf> {
    let link = data.join("checkout");
    std::fs::create_dir_all(data)?;
    if link
        .symlink_metadata()
        .is_ok_and(|m| !m.file_type().is_symlink())
    {
        return Err(std::io::Error::other(format!(
            "{} is no link",
            link.display()
        )));
    }
    // A new link renamed over the old, so a session starting meanwhile never finds none.
    let next = data.join("checkout.next");
    let _ = std::fs::remove_file(&next);
    std::os::unix::fs::symlink(checkout, &next)?;
    std::fs::rename(&next, &link)?;
    Ok(link.join("plugin"))
}

/// Whether `CLAUDE_CODE_PLUGIN_DIRS`, colon-separated as Claude Code reads it, has an entry whose
/// real folder is `plugin`'s.
pub fn loads_plugin(dirs: &str, plugin: &Path) -> bool {
    let Ok(ours) = std::fs::canonicalize(plugin) else {
        return false;
    };
    dirs.split(':')
        .filter(|d| !d.is_empty())
        .any(|d| std::fs::canonicalize(d).is_ok_and(|d| d == ours))
}

/// `profile` with each `export CLAUDE_CODE_PLUGIN_DIRS=` line ending in `link` (#21). It drops the
/// entries that are another studio plugin, with `~` or `$HOME` read as `home`, so the hooks never
/// load twice, and keeps every other entry as written, a missing one too, since Claude Code skips it.
/// With no such line, one is appended. `None` when such a line uses `${`, `$(` or a backtick, which a
/// split on `:` could break.
// shortcut: reads `export NAME=value` lines only, quoted or bare, with no trailing comment, and
// writes LF line endings; parse more when a profile sets the variable another way.
pub fn fixed_profile(profile: &str, link: &str, home: &Path) -> Option<String> {
    const EXPORT: &str = "export CLAUDE_CODE_PLUGIN_DIRS=";
    let studio = |dir: &str| {
        let dir = ["$HOME/", "${HOME}/", "~/"]
            .iter()
            .find_map(|prefix| dir.strip_prefix(prefix))
            .map_or_else(|| PathBuf::from(dir), |rest| home.join(rest));
        std::fs::read_to_string(dir.join(".claude-plugin/plugin.json"))
            .is_ok_and(|m| m.contains(r#""falinks-studio""#))
    };
    let mut found = false;
    let mut lines = Vec::new();
    for line in profile.lines() {
        let indent = &line[..line.len() - line.trim_start().len()];
        let Some(value) = line.trim_start().strip_prefix(EXPORT) else {
            lines.push(line.to_string());
            continue;
        };
        if ["${", "$(", "`"].iter().any(|s| value.contains(s)) {
            return None;
        }
        found = true;
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        let kept = value
            .split(':')
            .filter(|d| !d.is_empty() && *d != link && !studio(d));
        let dirs: Vec<&str> = kept.chain([link]).collect();
        lines.push(format!("{indent}{EXPORT}\"{}\"", dirs.join(":")));
    }
    if !found {
        lines.push("# The studio plugin's hooks, for every claude session (#21).".into());
        lines.push(format!("{EXPORT}\"{link}\""));
    }
    Some(lines.join("\n") + "\n")
}

/// The command that brings back an agent other than Claude Code (danielrispler/Falinks#561), by the name of the program a
/// revived terminal had in its foreground: conversation `id` when it is known, else the latest one
/// in the terminal's folder. `None` for any other program, which revives as a plain shell. Checked
/// from each tool's `--help`: `agy` 1.4.7 and `codex` 0.167.0 (2026-11-11). Cursor's `agent` was not
/// installed; its `--resume [id]` is from its docs (2026-11-09), unchecked.
pub fn resume(program: &str, id: Option<&str>) -> Option<Vec<String>> {
    let argv: &[&str] = match (program, id) {
        ("agy", Some(_)) => &["agy", "--conversation"],
        ("agy", None) => &["agy", "--continue"],
        ("agent" | "cursor-agent", _) => &[program, "--resume"],
        ("codex", Some(_)) => &["codex", "resume"],
        // `codex resume` lists the folder's sessions only, unless given `--all`.
        ("codex", None) => &["codex", "resume", "--last"],
        _ => return None,
    };
    Some(argv.iter().copied().chain(id).map(String::from).collect())
}

/// The agents the picker offers, by command, Claude Code first.
pub const AGENTS: [&str; 3] = ["claude", "agy", "codex"];

/// The command that starts a new conversation of an agent other than Claude Code, with `prompt` as
/// its first prompt when not empty. `None` for an agent it does not know. From each tool's `--help`:
/// `agy` 1.4.7's `--prompt-interactive`, `codex` 0.167.0's `[PROMPT]` (2026-11-15).
pub fn start(program: &str, prompt: &str) -> Option<Vec<String>> {
    let mut argv = vec![program.to_string()];
    match program {
        "agy" if !prompt.is_empty() => argv.push(format!("--prompt-interactive={prompt}")),
        // `--`: a prompt like "-p x" is no flag.
        "codex" if !prompt.is_empty() => argv.extend(["--".into(), prompt.into()]),
        "agy" | "codex" => {}
        _ => return None,
    }
    Some(argv)
}

/// Whether `argv` is `resume`'s command for an agent's latest conversation, not a known one.
pub fn resumes_latest(argv: &[String]) -> bool {
    argv.first()
        .is_some_and(|program| resume(program, None).as_deref() == Some(argv))
}

/// `--settings` that make the session's status line the studio's wrapper, `workbench/statusline.sh`
/// beside the plugin (danielrispler/Falinks#521; danielrispler/Falinks#517, decision 5): it saves the usage snapshot, then runs the user's own
/// status line. The path is single-quoted, as Claude Code runs the command in a shell.
fn status_line(plugin: &Path) -> String {
    let wrapper = plugin.with_file_name("workbench").join("statusline.sh");
    let quoted = format!("'{}'", wrapper.display().to_string().replace('\'', r"'\''"));
    serde_json::json!({ "statusLine": { "type": "command", "command": quoted } }).to_string()
}

/// The `context_window` of session `id`'s status line snapshot in `dir`: its window size, current
/// usage and percentages (danielrispler/Falinks#517, decision 1; danielrispler/Falinks#519). `None` when there is no snapshot, it has no window
/// size, or `id` is no plain session id, as the status line saves only those.
pub fn context_window(dir: &Path, id: &str) -> Option<serde_json::Value> {
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    if id.is_empty() || !id.chars().all(plain) {
        return None;
    }
    let snapshot = std::fs::read(dir.join(format!("{id}.json"))).ok()?;
    let snapshot: serde_json::Value = serde_json::from_slice(&snapshot).ok()?;
    let window = snapshot.get("context_window")?;
    window["context_window_size"].as_u64()?;
    Some(window.clone())
}

/// Creates task `name`: after a `git fetch`, branch `name` from `origin`'s default branch, checked
/// out in `.claude/worktrees/<name>` of the main checkout. A name already taken, by a branch here or
/// at `origin` or by a folder, is refused and nothing is created.
pub fn create_task(repo: &Path, name: &str) -> Result<Workspace, String> {
    // A valid branch name has no `..` part, so the folder stays under `.claude/worktrees/`.
    git(repo, &["check-ref-format", "--branch", name])?;
    let main = workspaces(repo)?.main.path;
    let path = format!("{main}/.claude/worktrees/{name}");
    git(repo, &["fetch", "--quiet", "origin"])?;
    // `origin/HEAD` names the default branch; a clone sets it, `set-head` asks `origin` otherwise.
    let head = ["symbolic-ref", "--short", "refs/remotes/origin/HEAD"];
    let base = match git(repo, &head) {
        Ok(base) => base,
        Err(_) => {
            git(repo, &["remote", "set-head", "origin", "--auto"])?;
            git(repo, &head)?
        }
    };
    if branch_taken(repo, name) {
        return Err(format!("a branch named '{name}' already exists"));
    }
    if Path::new(&path).exists() {
        return Err(format!("'{path}' already exists"));
    }
    // `--no-track`: the task's branch does not follow the default branch it starts from.
    let add = [
        "worktree",
        "add",
        "--quiet",
        "--no-track",
        "-b",
        name,
        &path,
        &base,
    ];
    git(repo, &add)?;
    Ok(Workspace {
        path,
        branch: Some(name.into()),
    })
}

/// Whether branch `name` exists here or at `origin`.
fn branch_taken(repo: &Path, name: &str) -> bool {
    let taken =
        |reference: String| git(repo, &["rev-parse", "--verify", "--quiet", &reference]).is_ok();
    taken(format!("refs/heads/{name}")) || taken(format!("refs/remotes/origin/{name}"))
}

/// Creates a task named after `request`, what Daniel asks a new session for: issue `danielrispler/Falinks#123` (or
/// `123`) gives `123-<its title>`, or `issue-123` when `gh` cannot read the title; a file of the
/// main checkout, such as a spec, gives its file name; any other text, a prompt, gives its first
/// words. A taken name gets the next free number, `-2` and on.
pub fn create_named_task(repo: &Path, request: &str) -> Result<Workspace, String> {
    let request = request.trim();
    let main = workspaces(repo)?.main.path;
    let spec = Path::new(&main).join(request);
    let number = request.strip_prefix('#').unwrap_or(request);
    let words = if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
        match issue_title(repo, number) {
            Some(title) => format!("{number} {title}"),
            None => format!("issue {number}"),
        }
    } else if !request.is_empty() && spec.is_file() {
        let stem = spec.file_stem().unwrap_or_default();
        stem.to_string_lossy().into_owned()
    } else {
        request.into()
    };
    let base = slug(&words);
    let free = |name: &String| {
        !branch_taken(repo, name)
            && !Path::new(&format!("{main}/.claude/worktrees/{name}")).exists()
    };
    let name = std::iter::once(base.clone())
        .chain((2..).map(|n| format!("{base}-{n}")))
        .find(free)
        .unwrap();
    create_task(repo, &name)
}

/// Issue `number`'s title, from `gh` in `repo`.
fn issue_title(repo: &Path, number: &str) -> Option<String> {
    let output = Command::new("gh")
        .args(["issue", "view", number, "--json", "title", "--jq", ".title"])
        .current_dir(repo)
        .output()
        .ok()?;
    let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (output.status.success() && !title.is_empty()).then_some(title)
}

/// A branch name from `words`: its first five words, in lowercase ASCII letters and digits joined
/// by `-`, with apostrophes dropped; `session` when none is left.
fn slug(words: &str) -> String {
    let words: Vec<String> = words
        .replace(['\'', '\u{2019}'], "")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(5)
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() {
        "session".into()
    } else {
        words.join("-")
    }
}

/// What removing a task's worktree would lose: its uncommitted changes as `git status --short`
/// lines, and its commits that no branch at `origin` holds, as `<short hash> <subject>`.
#[derive(Debug, PartialEq, Serialize)]
pub struct Losses {
    pub uncommitted: Vec<String>,
    pub unpushed: Vec<String>,
}

/// What removing the worktree `task` would lose: its uncommitted changes and its unpushed commits.
/// Read-only; the same two checks tell removal and **Landed** that nothing is at risk.
pub fn losses(task: &Path) -> Result<Losses, String> {
    let lines = |text: String| text.lines().map(String::from).collect::<Vec<_>>();
    Ok(Losses {
        uncommitted: lines(git(task, &["status", "--short"])?),
        unpushed: lines(git(
            task,
            &["log", "--format=%h %s", "HEAD", "--not", "--remotes"],
        )?),
    })
}

/// Removes the worktree at `path` with `git worktree remove`, never forced; its branch stays.
/// Unless `confirmed`, a worktree with uncommitted changes or unpushed commits is left as it is and
/// they are returned, for Daniel to confirm. Confirmed, git still refuses to drop uncommitted
/// changes, and its message is the error.
pub fn remove_task(repo: &Path, path: &str, confirmed: bool) -> Result<Option<Losses>, String> {
    if !confirmed {
        let losses = losses(Path::new(path))?;
        if !losses.uncommitted.is_empty() || !losses.unpushed.is_empty() {
            return Ok(Some(losses));
        }
    }
    git(repo, &["worktree", "remove", path])?;
    Ok(None)
}

/// Deletes branch `branch` after its task's worktree is gone, never forced: with `git branch -d`, or,
/// when git refuses one merged by squash, when its tip is the head of a merged pull request from it,
/// as `gh` lists them. Otherwise git's message is the error and the branch stays.
pub fn delete_merged_branch(repo: &Path, branch: &str) -> Result<(), String> {
    delete_branch_merged_at(repo, branch, &merged_heads(repo, branch))
}

/// The head commits of the merged pull requests from `branch`; none when `gh` cannot tell.
fn merged_heads(repo: &Path, branch: &str) -> Vec<String> {
    let args = ["pr", "list", "--head", branch, "--state", "merged"];
    let jq = ["--json", "headRefOid", "--jq", ".[].headRefOid"];
    Command::new("gh")
        .args(args)
        .args(jq)
        .current_dir(repo)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// [`delete_merged_branch`] with the merged pull requests' `heads` given. A squash merge leaves the
/// branch's commits off `main` (danielrispler/Falinks#461, decision 9's one exception): a tip a merged pull request had
/// loses nothing, and `update-ref` deletes it only while the branch still points there.
pub fn delete_branch_merged_at(repo: &Path, branch: &str, heads: &[String]) -> Result<(), String> {
    let refused = match git(repo, &["branch", "-d", branch]) {
        Ok(_) => return Ok(()),
        Err(refused) => refused,
    };
    let tip = git(
        repo,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )?;
    if !heads.contains(&tip) {
        return Err(refused);
    }
    git(
        repo,
        &["update-ref", "-d", &format!("refs/heads/{branch}"), &tip],
    )
    .map(drop)
}

/// A branch the main checkout can switch to: a local branch, or one at `origin` with no local branch
/// of its name (`remote`). `task` is the folder name of the task's worktree that holds it, which
/// git would refuse; `current` marks the main checkout's own branch.
#[derive(Debug, PartialEq, Serialize)]
pub struct Branch {
    pub name: String,
    pub remote: bool,
    pub task: Option<String>,
    pub current: bool,
}

/// The branches the main checkout of the repository that holds `repo` can switch to: local branches
/// in name order, then the remote-only ones.
pub fn branches(repo: &Path) -> Result<Vec<Branch>, String> {
    let found = workspaces(repo)?;
    let main = Path::new(&found.main.path);
    let names = |prefix: &str| -> Result<Vec<String>, String> {
        let listed = git(main, &["for-each-ref", "--format=%(refname)", prefix])?;
        Ok(listed
            .lines()
            .filter_map(|line| line.strip_prefix(prefix))
            .map(String::from)
            .collect())
    };
    let local = names("refs/heads/")?;
    // `origin/HEAD` is a pointer, not a branch.
    let remote: Vec<String> = names("refs/remotes/origin/")?
        .into_iter()
        .filter(|name| name != "HEAD" && !local.contains(name))
        .collect();
    let held = |name: &str| {
        let task = found
            .tasks
            .iter()
            .find(|task| task.branch.as_deref() == Some(name))?;
        Path::new(&task.path)
            .file_name()
            .map(|folder| folder.to_string_lossy().into_owned())
    };
    Ok(local
        .iter()
        .map(|name| Branch {
            name: name.clone(),
            remote: false,
            task: held(name),
            current: found.main.branch.as_deref() == Some(name),
        })
        .chain(remote.into_iter().map(|name| Branch {
            name,
            remote: true,
            task: None,
            current: false,
        }))
        .collect())
}

/// Switches the main checkout of the repository that holds `repo` to `branch` with `git switch`: a
/// local branch, or, with `remote`, a new local branch tracking `origin/<branch>`. No stash and no
/// force or discard option, so git refuses a switch that would lose uncommitted work, and its
/// message is the error.
pub fn switch_branch(repo: &Path, branch: &str, remote: bool) -> Result<(), String> {
    // A valid branch name never starts with `-`, so git reads it as a branch.
    git(repo, &["check-ref-format", "--branch", branch])?;
    let main = workspaces(repo)?.main.path;
    let target = format!("origin/{branch}");
    let args: &[&str] = if remote {
        &["switch", "--track", &target]
    } else {
        &["switch", branch]
    };
    git(Path::new(&main), args).map(drop)
}

/// Runs `git` in `dir` and returns its output, or git's own error message.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = run(dir, args)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    // Only the end: a status line may start with a space.
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().into())
}

/// Runs `git` in `dir` and returns its output whatever its exit status, for a caller that reads the status.
fn run(dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    // No optional locks: a read never holds `index.lock` against an agent's commit (#27).
    Command::new("git")
        .arg("--no-optional-locks")
        // A fetch that wants a password fails, rather than waiting on a prompt nobody sees.
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git {}: {e}", args[0]))
}

/// The studio's plugin for agy (#43, decisions 8 and 9; #49). `agy plugin install` copies the
/// folder, so the app renders the shipped `plugin-agy/` with `studio-hook`'s path, compares it with
/// the installed copy and installs again when they differ.
pub mod agy_plugin {
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};

    /// The plugin's name in `plugin.json`, and its folder under `~/.gemini/config/plugins/`.
    const NAME: &str = "studio";
    const PLACEHOLDER: &str = "__STUDIO_HOOK__";

    #[derive(Debug, PartialEq, Eq)]
    pub enum Status {
        Current,
        Missing,
        Stale,
    }

    /// The plugin's two files as installed: `source`'s, with `hook` where the hooks name it.
    fn rendered(source: &Path, hook: &Path) -> Result<[(&'static str, String); 2], String> {
        let read = |name: &str| {
            std::fs::read_to_string(source.join(name))
                .map_err(|e| format!("could not read {}/{name}: {e}", source.display()))
        };
        // The path sits in a JSON string, so it is escaped as one.
        let path = serde_json::to_string(&hook.display().to_string()).map_err(|e| e.to_string())?;
        let path = &path[1..path.len() - 1];
        Ok([
            ("plugin.json", read("plugin.json")?),
            ("hooks.json", read("hooks.json")?.replace(PLACEHOLDER, path)),
        ])
    }

    fn installed(home: &Path) -> PathBuf {
        home.join(".gemini/config/plugins").join(NAME)
    }

    /// Whether the copy in `home`'s agy config is the plugin of `source` for `hook`.
    pub fn status(source: &Path, hook: &Path, home: &Path) -> Result<Status, String> {
        let dir = installed(home);
        if !dir.join("hooks.json").exists() {
            return Ok(Status::Missing);
        }
        let same = rendered(source, hook)?
            .iter()
            .all(|(name, text)| std::fs::read_to_string(dir.join(name)).is_ok_and(|t| &t == text));
        Ok(if same { Status::Current } else { Status::Stale })
    }

    /// Whether agy accepts the installed copy: it skips an invalid `hooks.json` silently, so a copy
    /// equal to the source can still read as current while doing nothing.
    pub fn installed_is_valid(agy: &Path, home: &Path) -> bool {
        Command::new(agy)
            .args(["plugin", "validate"])
            .arg(installed(home))
            .env("HOME", home)
            .stdin(Stdio::null())
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// Installs the plugin of `source` for `hook` with `agy` (the command), for `home`: validates a
    /// rendered copy first, as install does not and skips an invalid `hooks.json` silently, then
    /// replaces any installed copy.
    pub fn install(agy: &Path, source: &Path, hook: &Path, home: &Path) -> Result<(), String> {
        let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
        let dir = scratch.path().join(NAME);
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        for (name, text) in rendered(source, hook)? {
            std::fs::write(dir.join(name), text).map_err(|e| e.to_string())?;
        }
        let run = |args: &[&str]| {
            Command::new(agy)
                .args(["plugin"])
                .args(args)
                .arg(&dir)
                .env("HOME", home)
                .stdin(Stdio::null())
                .output()
                .map_err(|e| format!("could not run agy: {e}"))
        };
        let valid = run(&["validate"])?;
        if !valid.status.success() {
            return Err(format!(
                "agy did not validate the studio plugin: {}",
                String::from_utf8_lossy(&valid.stdout)
            ));
        }
        // An uninstall of what is not there fails, and that is fine.
        let _ = Command::new(agy)
            .args(["plugin", "uninstall", NAME])
            .env("HOME", home)
            .stdin(Stdio::null())
            .output();
        let done = run(&["install"])?;
        if !done.status.success() {
            return Err(format!(
                "agy plugin install failed: {}",
                String::from_utf8_lossy(&done.stderr)
            ));
        }
        Ok(())
    }
}
