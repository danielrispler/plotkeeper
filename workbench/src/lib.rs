//! Workbench git (danielrispler/Falinks#328, module 7; danielrispler/Falinks#461, decision 8): the places a session can start in, read live
//! from `git worktree list --porcelain` with no stored copy, and the `claude` command a studio
//! terminal runs. Beside them, creating and removing tasks (danielrispler/Falinks#468; danielrispler/Falinks#461, decision 9): no call passes
//! git's force option, so git refuses whatever would lose work, and its message is shown as it is.

use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// A main checkout or a task's worktree: its folder, and its branch, `None` at a detached `HEAD`.
#[derive(Debug, PartialEq, Serialize)]
pub struct Place {
    pub path: String,
    pub branch: Option<String>,
}

/// The repository's main checkout, its tasks, the worktrees under `.claude/worktrees/` in `git`'s
/// order, and how many `agent-*` worktrees there were hidden.
#[derive(Debug, PartialEq, Serialize)]
pub struct Places {
    pub main: Place,
    pub tasks: Vec<Place>,
    pub hidden: usize,
}

/// The places of the repository that holds `repo`, which may be any of its worktrees.
pub fn places(repo: &Path) -> Result<Places, String> {
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
        let mut place = Place {
            path: String::new(),
            branch: None,
        };
        for line in block.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                place.path = path.into();
            } else if let Some(branch) = line.strip_prefix("branch ") {
                place.branch = Some(branch.trim_start_matches("refs/heads/").into());
            } else if line == "bare" || line.starts_with("prunable") {
                // A bare repository has no files, and a prunable worktree's folder is gone.
                return None;
            }
        }
        (!place.path.is_empty()).then_some(place)
    });
    let main = listed.next().ok_or("git listed no main checkout")?;
    let under = format!("{}/.claude/worktrees/", main.path);
    let (agents, tasks): (Vec<Place>, Vec<Place>) = listed
        .filter(|place| place.path.starts_with(&under))
        .partition(|place| place.path[under.len()..].starts_with("agent-"));
    Ok(Places {
        main,
        tasks,
        hidden: agents.len(),
    })
}

/// A place's git state (#26, #29): its branch, `None` at a detached `HEAD`; its distance from its
/// upstream, commits ahead and behind, `None` with no upstream (never pushed, or detached); and how
/// many files `git status` lists as changed. Later tickets add main distance and the operation in
/// progress here.
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
}

/// The git state of the place at `path`, one of the places of `repo`; any other path is refused, so
/// the window cannot run git in an arbitrary folder. Read-only, and with no optional locks.
pub fn git_state(repo: &Path, path: &str) -> Result<GitState, String> {
    let known = places(repo)?;
    if known.main.path != path && !known.tasks.iter().any(|task| task.path == path) {
        return Err(format!("not a place of this repository: {path}"));
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
    let default = git(
        dir,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
    )
    .ok();
    let on_default =
        matches!((&branch, &default), (Some(b), Some(d)) if d.strip_prefix("origin/") == Some(b));
    let main_behind = default
        .and_then(|d| git(dir, &["rev-list", "--count", &format!("HEAD..{d}")]).ok())
        .and_then(|n| n.parse().ok())
        .filter(|n| *n > 0);
    Ok(GitState {
        branch,
        upstream,
        changed,
        operation,
        main_behind,
        on_default,
    })
}

/// The operation in progress in the place at `dir`, from the files its repository keeps while one is stopped.
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

/// The plan's usage (danielrispler/Falinks#481; danielrispler/Falinks#461, decision 19): the `rate_limits` of the newest `*.json` snapshot in
/// `dir`, where Daniel's status line command saves its stdin, one file per session, by temp file and
/// rename. `None` when there is no snapshot, it has no limits yet, or it does not parse.
pub fn plan_usage(dir: &Path) -> Option<serde_json::Value> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .max_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok())?;
    let snapshot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(newest.path()).ok()?).ok()?;
    snapshot.get("rate_limits").cloned()
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
pub fn create_task(repo: &Path, name: &str) -> Result<Place, String> {
    // A valid branch name has no `..` part, so the folder stays under `.claude/worktrees/`.
    git(repo, &["check-ref-format", "--branch", name])?;
    let main = places(repo)?.main.path;
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
    Ok(Place {
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
pub fn create_named_task(repo: &Path, request: &str) -> Result<Place, String> {
    let request = request.trim();
    let main = places(repo)?.main.path;
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

/// Removes the worktree at `path` with `git worktree remove`, never forced; its branch stays.
/// Unless `confirmed`, a worktree with uncommitted changes or unpushed commits is left as it is and
/// they are returned, for Daniel to confirm. Confirmed, git still refuses to drop uncommitted
/// changes, and its message is the error.
pub fn remove_task(repo: &Path, path: &str, confirmed: bool) -> Result<Option<Losses>, String> {
    if !confirmed {
        let task = Path::new(path);
        let lines = |text: String| text.lines().map(String::from).collect::<Vec<_>>();
        let losses = Losses {
            uncommitted: lines(git(task, &["status", "--short"])?),
            unpushed: lines(git(
                task,
                &["log", "--format=%h %s", "HEAD", "--not", "--remotes"],
            )?),
        };
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

/// Runs `git` in `dir` and returns its output, or git's own error message.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    // No optional locks: a read never holds `index.lock` against an agent's commit (#27).
    let output = Command::new("git")
        .arg("--no-optional-locks")
        // A fetch that wants a password fails, rather than waiting on a prompt nobody sees.
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("git {}: {e}", args[0]))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    // Only the end: a status line may start with a space.
    Ok(String::from_utf8_lossy(&output.stdout).trim_end().into())
}
