//! Background runs (spec danielrispler/plotkeeper#378, ticket #390): a headless agent run with scoped tools, started by
//! Daniel's click and never a session. This is the runner; no module starts a run yet.
//!
//! - **The seam.** A [`Request`] (run kind, working folder, prompt, limits) goes in through [`Runner::submit`]; an
//!   [`Outcome`] (result text, denials, cost, exit) comes out through [`Runner::outcome`]. The [`Harness`] builds the
//!   command a run starts: [`Claude`] is the only real one, and the tests' fake is a shell script.
//! - **A run's folder** is `<runs>/<id>/`, outside plotkeeper's data folder, which a run may not read. It holds what
//!   plotkeeper and the launcher keep (`run.json`, `lock`, `launch.json`, `status.json`, the harness's output) and
//!   `work/`, the working folder, the only place the agent writes. [`Runner::new_folder`] makes both.
//! - **The lifecycle.** A run starts through the launcher (`plotkeeper-run`, [`launch`]) in its own process group. The
//!   launcher holds an exclusive lock on the folder's `lock` for as long as it lives and writes `status.json` whole when the
//!   child exits. A run is alive while that lock is held: a free lock with no status is a dead run. So a run outlives
//!   plotkeeper, and [`Runner::pick_up`] finds it again at the next start.
//! - **The limits.** One run at a time per project, the others queued in the order they were asked for; a time limit the
//!   launcher keeps; a third of the cores for builds; and a failure on the login or a rate limit pauses every queue until
//!   [`Runner::resume`].

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use studio_understanding::{FailureKind, login_or_quota, millis};

const RUN: &str = "run.json";
const LOCK: &str = "lock";
const LAUNCH: &str = "launch.json";
const STATUS: &str = "status.json";
const RECORDED: &str = "recorded";
const WORK: &str = "work";
const STDOUT: &str = "stdout";
const STDERR: &str = "stderr";
const SETUP_LOG: &str = "setup.log";
const PAUSED: &str = "paused.json";
/// The record of every run, one line each, in the data folder.
const RECORDS: &str = "background-runs.jsonl";

/// What a run is for, which decides its tools and its time limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Writes a test that backs a law, in a clone of the project.
    BackLaw,
    /// Builds a small simulator page from the project's code, with no shell.
    MicroWorld,
}

impl Kind {
    /// The tools the agent has, and no other.
    pub fn tools(self) -> &'static [&'static str] {
        match self {
            Kind::BackLaw => &["Bash", "Read", "Edit", "Write", "Glob", "Grep"],
            Kind::MicroWorld => &["Read", "Edit", "Write", "Glob", "Grep"],
        }
    }

    /// Whether the agent has a shell, and so needs the project's checks to allow in it.
    fn shell(self) -> bool {
        self.tools().contains(&"Bash")
    }
}

/// How long a run may take before the launcher stops it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub timeout: Duration,
}

impl Limits {
    /// 30 minutes, and 10 for a micro-world.
    pub fn of(kind: Kind) -> Limits {
        let minutes = match kind {
            Kind::BackLaw => 30,
            Kind::MicroWorld => 10,
        };
        Limits {
            timeout: Duration::from_secs(minutes * 60),
        }
    }
}

/// A run to start. `folder` is the working folder [`Runner::new_folder`] gave, filled by the caller (a clone, or left
/// empty for the agent's output). `prompt` holds only text plotkeeper wrote or Daniel confirmed, never an issue's, a PR's
/// or a comment's; it is also what the fallback copies, so it stands alone.
pub struct Request {
    pub kind: Kind,
    pub project: String,
    pub folder: PathBuf,
    pub prompt: String,
    pub limits: Limits,
}

/// The command the launcher starts for a run, with what it adds to and takes from plotkeeper's own environment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HarnessCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_remove: Vec<String>,
}

/// What a harness builds its command from.
pub struct Plan<'a> {
    pub kind: Kind,
    pub project: &'a str,
    /// The working folder: the agent's current folder, and the only place it writes.
    pub folder: &'a Path,
    pub prompt: &'a str,
    /// `backgroundRuns.model`; none: the harness's own default.
    pub model: Option<&'a str>,
    /// The project's `checks`, as `settings.json` holds them now.
    pub checks: &'a [String],
    /// plotkeeper's data folder, which a run may not read.
    pub data: &'a Path,
}

/// The agent program a run starts: the runner's one seam. It only builds the command; the launcher runs it.
pub trait Harness: Send + Sync {
    /// The command for `plan`, or why there is none, such as a program that is not installed.
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String>;
}

/// Claude Code, headless: `claude -p`, found as the login shell's `command -v` finds it.
pub struct Claude;

impl Harness for Claude {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        let program = studio_understanding::find(shell.as_ref(), "claude")
            .ok_or("`claude` was not found, so the run cannot start")?;
        Ok(claude_command(&program, plan))
    }
}

/// The sandbox block a run's `--settings` carries, exactly as spec #378 gives it: the user's hooks off, Claude Code's OS
/// sandbox on and required, no way out of it, and no reading of plotkeeper's data folder (`data`, which holds its token) or
/// of a credential folder. `autoAllowBashIfSandboxed` must stay `false`: with `true`, commands outside the allow list ran.
pub fn sandbox_settings(data: &Path) -> serde_json::Value {
    serde_json::json!({
        "disableAllHooks": true,
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "autoAllowBashIfSandboxed": false,
            "filesystem": { "denyRead": [
                data.to_string_lossy(), "~/.claude", "~/.claude.json", "~/.ssh", "~/.aws", "~/.config/gh", "~/.npmrc",
                "~/.netrc", "~/.cargo/credentials.toml", "~/.gnupg", "~/.kube",
            ] },
        },
    })
}

/// The git commands a run's shell may use: reading, and staging inside its own clone. Never a commit, a push or `gh`.
const GIT: [&str; 4] = ["git status", "git diff", "git log", "git add"];

/// What `--allowedTools` lists for a run: edits inside its working `folder`, and for a kind with a shell the read-only
/// git commands and one entry per command of the project's `checks`, each bare and with arguments. An entry of `checks`
/// that has `git` or `gh` as any of its words is left out, so no list ever allows a push. This list is not a security
/// boundary: an allowed check runs code the agent wrote. The OS sandbox and trusted input are.
pub fn allowed_tools(kind: Kind, folder: &Path, checks: &[String]) -> Vec<String> {
    // The rule's own form for an absolute path: `//` and the path.
    let mut allowed = vec![format!("Edit(/{}/**)", folder.display())];
    if kind.shell() {
        let checks = checks.iter().map(|check| check.trim()).filter(|check| {
            !check.is_empty()
                && !check
                    .split_whitespace()
                    .any(|word| word == "git" || word == "gh")
        });
        for command in GIT.into_iter().chain(checks) {
            allowed.push(format!("Bash({command})"));
            allowed.push(format!("Bash({command} *)"));
        }
    }
    allowed
}

/// How many cores a run's builds and tests get: a third, so a run leaves room for the sessions.
fn cores() -> usize {
    (thread::available_parallelism().map_or(1, |n| n.get()) / 3).max(1)
}

/// The `claude` command for `plan`, built and never run here: no user settings, hooks, plugins, skills or MCP servers
/// (`--restricted --safe-mode --strict-mcp-config`), nothing asked and nothing allowed that the list does not name
/// (`dontAsk`), the kind's tools, the sandbox block, and the prompt as its last argument since standard input is closed.
/// It runs on the CLI's own login, as the writer does: the API key variables are removed, and `PLOTKEEPER_INTERNAL=1`
/// keeps the hook from recording it.
pub fn claude_command(program: &Path, plan: &Plan) -> HarnessCommand {
    let mut args: Vec<String> = [
        "--restricted",
        "--safe-mode",
        "--strict-mcp-config",
        "--permission-mode",
        "dontAsk",
        "--permission-prompts",
        "none",
    ]
    .map(String::from)
    .into();
    if let Some(model) = plan.model {
        args.extend(["--model".into(), model.into()]);
    }
    args.extend(["--tools".into(), plan.kind.tools().join(",")]);
    // `--allowedTools` takes several values, so an option must follow it.
    args.push("--allowedTools".into());
    args.extend(allowed_tools(plan.kind, plan.folder, plan.checks));
    args.extend(["--settings".into(), sandbox_settings(plan.data).to_string()]);
    args.extend(["--output-format".into(), "json".into()]);
    // NUL bytes, which no argument can hold, are dropped.
    args.extend(["-p".into(), plan.prompt.replace('\0', "")]);
    HarnessCommand {
        program: program.into(),
        args,
        env: [("PLOTKEEPER_INTERNAL".to_string(), "1".to_string())].into(),
        env_remove: [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_PLUGIN_DIRS",
        ]
        .map(String::from)
        .into(),
    }
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum End {
    /// It exited 0 and no tool call was denied.
    Done,
    /// It exited 0, and tool calls were denied on the way: never a clean success.
    Denied,
    Failed,
    /// The launcher stopped it at its time limit.
    TimedOut,
    /// Its process went away with no final status.
    Died,
    /// The project's `setup` command failed, so the agent never started.
    SetupFailed,
    /// The harness is not logged in. Pauses the queues.
    NotLoggedIn,
    /// The harness hit a quota or a rate limit. Pauses the queues.
    RateLimited,
}

impl End {
    /// The pause this end puts on every queue; none for an end that pauses nothing.
    fn pause(self) -> Option<Pause> {
        match self {
            End::NotLoggedIn => Some(Pause::NotLoggedIn),
            End::RateLimited => Some(Pause::RateLimited),
            _ => None,
        }
    }
}

/// Why every queue waits until Daniel resumes them, as `paused.json` keeps it across a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Pause {
    NotLoggedIn,
    RateLimited,
    /// A reason another build wrote: still a pause.
    #[serde(other)]
    Other,
}

impl Pause {
    /// The words of the one indicator.
    fn line(self) -> &'static str {
        match self {
            Pause::NotLoggedIn => {
                "Claude is not logged in: run `claude` in a terminal and log in with /login"
            }
            Pause::RateLimited => "Claude hit its usage or rate limit",
            Pause::Other => "A run failed on the login or a limit",
        }
    }
}

/// What a run gave back.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub end: End,
    /// The agent's last words, or what a failure printed.
    pub result: String,
    /// The tool calls it was denied, as `Tool: what it asked for`.
    pub denials: Vec<String>,
    /// In US dollars, as the harness reports it.
    pub cost: Option<f64>,
    /// The harness's exit code; none when it was stopped, died or never started.
    pub exit: Option<i32>,
}

impl Outcome {
    /// An end with no harness result behind it: `result` says why.
    fn bare(end: End, result: String) -> Outcome {
        Outcome {
            end,
            result,
            denials: Vec::new(),
            cost: None,
            exit: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    Queued,
    Running,
    Ended,
}

/// A run as the map shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunView {
    pub id: String,
    pub kind: Kind,
    pub state: RunState,
    /// When it was asked for, started or ended, by its state; Unix milliseconds.
    pub at: u64,
    /// How an ended run ended, and one line on it.
    pub outcome: Option<End>,
    pub line: Option<String>,
    pub denials: Vec<String>,
    /// The fallback: the run's prompt, for Daniel to paste into his own agent, on every end but a clean one.
    pub prompt: Option<String>,
}

/// A project's runs, in the order they were asked for, and the one line that says why every queue is paused.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Runs {
    pub paused: Option<String>,
    pub runs: Vec<RunView>,
}

/// Why a run did not start, and the prompt to copy instead.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Refusal {
    pub line: String,
    pub prompt: String,
}

/// Where the runner keeps its things and what it starts.
pub struct Config {
    /// plotkeeper's data folder: a run may not read it, and the record of runs is kept in it.
    pub data: PathBuf,
    /// The runs folder, outside the data folder.
    pub runs: PathBuf,
    /// `settings.json`, read anew for each run.
    pub settings: PathBuf,
    /// The launcher's executable, `plotkeeper-run`.
    pub launcher: PathBuf,
    pub harness: Arc<dyn Harness>,
}

/// The runs folder: `~/Library/Caches/plotkeeper/runs`, outside the data folder and with no space in its path.
pub fn runs_dir(home: &Path) -> PathBuf {
    home.join("Library/Caches/plotkeeper/runs")
}

impl Config {
    /// The runner of the plotkeeper whose event log is `log`: the data folder is the log's, the launcher sits beside this
    /// executable, and the harness is Claude Code.
    pub fn beside(log: &Path) -> Config {
        let data = log.parent().unwrap_or(Path::new("")).to_path_buf();
        let exe = std::env::current_exe().and_then(|exe| exe.canonicalize());
        let beside = exe.ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        Config {
            runs: std::env::home_dir()
                .map_or_else(|| data.join("../plotkeeper-runs"), |h| runs_dir(&h)),
            settings: data.join("settings.json"),
            launcher: beside.unwrap_or_default().join("plotkeeper-run"),
            harness: Arc::new(Claude),
            data,
        }
    }
}

/// A run as its folder's `run.json` keeps it: what plotkeeper needs to list it after a restart, and what the launcher
/// needs to run it. A reader ignores a field it does not know.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    id: String,
    kind: Kind,
    project: String,
    /// The working folder.
    folder: PathBuf,
    prompt: String,
    created_ms: u64,
    timeout_secs: u64,
    /// The project's `setup` command, which the launcher runs in the working folder before the harness.
    #[serde(default)]
    setup: Option<String>,
    command: HarnessCommand,
}

/// What the launcher reads of `run.json`: only what it runs, so a run of a kind this build does not know still keeps its
/// limit and writes its status.
#[derive(Debug, Deserialize)]
struct Launched {
    folder: PathBuf,
    timeout_secs: u64,
    #[serde(default)]
    setup: Option<String>,
    command: HarnessCommand,
}

/// Who runs a started run: the launcher's process id and when that process started, which together name one process even
/// once the id is reused. `pid` is none between the moment a start is written down and the moment the launcher runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Launch {
    #[serde(default)]
    pid: Option<u32>,
    #[serde(default)]
    start: Option<String>,
    #[serde(default)]
    at_ms: u64,
}

/// The launcher's last word on a run, written whole.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Status {
    /// The harness's exit code.
    #[serde(default)]
    exit: Option<i32>,
    #[serde(default)]
    timed_out: bool,
    /// The setup command's exit code, when it failed.
    #[serde(default)]
    setup_exit: Option<i32>,
    /// Why the harness never started.
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    started_ms: u64,
    #[serde(default)]
    ended_ms: u64,
}

fn read_json<T: serde::de::DeserializeOwned>(file: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(file).ok()?).ok()
}

/// Writes `value` as `file` whole: beside it, then renamed over it.
fn write_json(file: &Path, value: &impl Serialize) -> io::Result<()> {
    let dir = file.parent().unwrap_or(Path::new(""));
    let mut temp = tempfile::NamedTempFile::new_in(dir)?;
    serde_json::to_writer(&mut temp, value)?;
    temp.persist(file)?;
    Ok(())
}

/// Whether nobody holds the lock of the run in `dir`: its launcher is gone, or never started.
fn lock_free(dir: &Path) -> bool {
    let Ok(lock) = File::open(dir.join(LOCK)) else {
        return true;
    };
    !matches!(lock.try_lock(), Err(TryLockError::WouldBlock))
}

/// Waits until the lock of the run in `dir` is free.
fn wait_free(dir: &Path) {
    if let Ok(lock) = File::open(dir.join(LOCK)) {
        while lock
            .lock()
            .is_err_and(|e| e.kind() == io::ErrorKind::Interrupted)
        {}
    }
}

/// When process `pid` started, as `ps` prints it, to the second; none when there is no such process.
fn process_start(pid: u32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    let start = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !start.is_empty()).then_some(start)
}

// The launcher.

/// The launcher, `plotkeeper-run <run's folder>`: runs the project's setup command, then the harness, in the working
/// folder, with standard input closed and the output kept in the run's folder, and writes `status.json` when the harness
/// exits or the time limit passes. plotkeeper starts it in a process group of its own with the run's lock as its standard
/// input, already held; the launcher gives that to no child, so the lock is free exactly when the launcher is gone. Its
/// last act is to kill that group, itself included, so nothing a run started outlives it: not at the limit, and not a
/// stray child of a run that finished.
pub fn launch(dir: &Path) -> Result<(), String> {
    let stored: Launched = read_json(&dir.join(RUN)).ok_or("the folder holds no run")?;
    let started_ms = millis();
    let deadline = Instant::now() + Duration::from_secs(stored.timeout_secs);
    let status = match run_setup(dir, &stored, deadline)
        .and_then(|()| run_harness(dir, &stored, deadline))
    {
        Ok(exit) => Status {
            exit,
            ..Status::default()
        },
        Err(status) => status,
    };
    let status = Status {
        started_ms,
        ended_ms: millis(),
        ..status
    };
    let written = write_json(&dir.join(STATUS), &status);
    // The status is written, or cannot be: either way the group goes now, this launcher with it.
    kill_own_group();
    written.map_err(|e| format!("write the status: {e}"))
}

fn output(dir: &Path, name: &str) -> Result<File, Status> {
    File::create(dir.join(name)).map_err(|e| Status {
        error: Some(format!("{name}: {e}")),
        ..Status::default()
    })
}

/// What ended a wait on a child.
enum Waited {
    Exited(Option<i32>),
    TimedOut,
}

/// Waits for `child` until `deadline`, on a thread of its own so the wait needs no polling.
fn wait_until(mut child: Child, deadline: Instant) -> Waited {
    let (done, waited) = mpsc::channel();
    thread::spawn(move || drop(done.send(child.wait())));
    match waited.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(status) => Waited::Exited(status.ok().and_then(|status| status.code())),
        Err(_) => Waited::TimedOut,
    }
}

/// Runs the project's setup command, outside any sandbox and with the network, before the agent starts.
fn run_setup(dir: &Path, stored: &Launched, deadline: Instant) -> Result<(), Status> {
    let Some(setup) = &stored.setup else {
        return Ok(());
    };
    let log = output(dir, SETUP_LOG)?;
    let child = Command::new("/bin/sh")
        .args(["-c", setup])
        .current_dir(&stored.folder)
        .envs(&stored.command.env)
        .stdin(Stdio::null())
        .stderr(log.try_clone().map_err(|e| Status {
            error: Some(e.to_string()),
            ..Status::default()
        })?)
        .stdout(log)
        .spawn();
    let failed = |setup_exit| Status {
        setup_exit: Some(setup_exit),
        ..Status::default()
    };
    match child.map(|child| wait_until(child, deadline)) {
        Ok(Waited::Exited(Some(0))) => Ok(()),
        Ok(Waited::Exited(code)) => Err(failed(code.unwrap_or(-1))),
        Ok(Waited::TimedOut) => Err(Status {
            timed_out: true,
            ..Status::default()
        }),
        Err(_) => Err(failed(-1)),
    }
}

/// Runs the harness; its exit code, none when a signal ended it.
fn run_harness(dir: &Path, stored: &Launched, deadline: Instant) -> Result<Option<i32>, Status> {
    let command = &stored.command;
    let mut harness = Command::new(&command.program);
    harness
        .args(&command.args)
        .current_dir(&stored.folder)
        .envs(&command.env)
        .stdin(Stdio::null())
        .stdout(output(dir, STDOUT)?)
        .stderr(output(dir, STDERR)?);
    for name in &command.env_remove {
        harness.env_remove(name);
    }
    let child = harness.spawn().map_err(|e| Status {
        error: Some(format!(
            "`{}` did not start: {e}",
            command.program.display()
        )),
        ..Status::default()
    })?;
    match wait_until(child, deadline) {
        Waited::Exited(exit) => Ok(exit),
        Waited::TimedOut => Err(Status {
            timed_out: true,
            ..Status::default()
        }),
    }
}

/// Kills the launcher's process group, itself included, when it leads one; a launcher started by hand inside another
/// group kills nothing.
fn kill_own_group() {
    // SAFETY: plain system calls on this process's own ids; the signal goes only to the group this process leads.
    unsafe {
        if libc::getpgrp() == libc::getpid() {
            libc::killpg(libc::getpgrp(), libc::SIGKILL);
        }
    }
}

// Reading a run's end.

/// The last `lines` lines of `text`.
fn tail(text: &str, lines: usize) -> String {
    let all: Vec<&str> = text.trim().lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

/// What the harness printed.
struct Printed {
    result: String,
    /// It says the run is an error, by `is_error` or by a `subtype` that names one, such as `error_max_turns`.
    is_error: bool,
    cost: Option<f64>,
    denials: Vec<String>,
}

/// What the harness printed, read as Claude Code's `--output-format json` result: its `result`, whether it says it is an
/// error, its cost and the tool calls it was denied. Output that is no such JSON is the result as it is.
fn read_result(stdout: &str) -> Printed {
    let parsed = serde_json::from_str::<serde_json::Value>(stdout.trim()).ok();
    // With `--verbose` the output is every message, the result last.
    let result = parsed.and_then(|value| match value {
        serde_json::Value::Array(messages) => messages.into_iter().rfind(|m| m["type"] == "result"),
        value => value.is_object().then_some(value),
    });
    let Some(result) = result else {
        return Printed {
            result: stdout.trim().to_string(),
            is_error: false,
            cost: None,
            denials: Vec::new(),
        };
    };
    let denials = result["permission_denials"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|denial| {
            let input = &denial["tool_input"];
            let asked = ["command", "file_path", "path", "pattern"]
                .iter()
                .find_map(|key| input[key].as_str());
            let asked = asked.map_or_else(|| input.to_string(), str::to_string);
            format!(
                "{}: {asked}",
                denial["tool_name"].as_str().unwrap_or("A tool")
            )
        });
    let subtype = result["subtype"].as_str().unwrap_or_default();
    Printed {
        result: result["result"]
            .as_str()
            .unwrap_or_default()
            .trim()
            .to_string(),
        is_error: result["is_error"].as_bool().unwrap_or(false) || subtype.starts_with("error"),
        cost: result["total_cost_usd"].as_f64(),
        denials: denials.collect(),
    }
}

/// The outcome of the ended run in `dir`, from the launcher's status and the harness's output.
fn read_outcome(dir: &Path) -> (Outcome, Option<Status>) {
    let text = |name: &str| {
        String::from_utf8_lossy(&fs::read(dir.join(name)).unwrap_or_default()).into_owned()
    };
    let bare = Outcome::bare;
    let Some(status) = read_json::<Status>(&dir.join(STATUS)) else {
        return (bare(End::Died, String::new()), None);
    };
    let outcome = if status.timed_out {
        bare(End::TimedOut, String::new())
    } else if status.setup_exit.is_some() {
        bare(End::SetupFailed, tail(&text(SETUP_LOG), 20))
    } else if let Some(error) = &status.error {
        bare(End::Failed, error.clone())
    } else {
        let Printed {
            result,
            is_error,
            cost,
            denials,
        } = read_result(&text(STDOUT));
        let errors = text(STDERR);
        let end = if status.exit != Some(0) || is_error {
            // Only a failure's words are read for a login or a limit: a good result may talk about either.
            match login_or_quota(&format!("{result}\n{errors}")) {
                Some(FailureKind::NotLoggedIn) => End::NotLoggedIn,
                Some(FailureKind::Quota) => End::RateLimited,
                _ => End::Failed,
            }
        } else if denials.is_empty() {
            End::Done
        } else {
            End::Denied
        };
        Outcome {
            end,
            result: if result.is_empty() {
                tail(&errors, 20)
            } else {
                result
            },
            denials,
            cost,
            exit: status.exit,
        }
    };
    (outcome, Some(status))
}

/// The one line the map shows for an ended run.
fn line(stored: &Stored, outcome: &Outcome) -> String {
    let first = outcome
        .result
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    let why: String = first.trim().chars().take(200).collect();
    let with = |lead: &str| match why.as_str() {
        "" => lead.to_string(),
        why => format!("{lead}: {why}"),
    };
    match outcome.end {
        End::Done => "Finished".into(),
        End::Denied => match outcome.denials.len() {
            1 => "Finished, but 1 tool call was denied".into(),
            n => format!("Finished, but {n} tool calls were denied"),
        },
        End::Failed => with("Failed"),
        End::TimedOut => format!(
            "Stopped at its limit of {} minutes",
            stored.timeout_secs.div_ceil(60)
        ),
        End::Died => "Its process went away before it finished".into(),
        End::SetupFailed => with("The project's setup command failed"),
        End::NotLoggedIn => Pause::NotLoggedIn.line().into(),
        End::RateLimited => Pause::RateLimited.line().into(),
    }
}

// The runner.

enum Phase {
    Queued,
    /// Its launcher was started at `since_ms`.
    Running {
        since_ms: u64,
    },
    Ended {
        outcome: Outcome,
        at_ms: u64,
    },
}

struct Run {
    stored: Stored,
    phase: Phase,
}

#[derive(Default)]
struct State {
    /// Every known run, in the order they were asked for.
    runs: Vec<Run>,
    /// Why the queues are paused, until Daniel resumes them.
    paused: Option<Pause>,
}

type Changed = Box<dyn Fn(&str) + Send + Sync>;

/// The background runs of every project: their queue, their processes and their outcomes.
pub struct Runner {
    config: Config,
    state: Mutex<State>,
    changed: OnceLock<Changed>,
    /// For the threads that wait on a run's end: a runner that is gone is told nothing.
    me: Weak<Runner>,
    ids: AtomicU64,
}

impl Runner {
    /// A runner that knows no run yet; [`Runner::pick_up`] reads the runs folder.
    pub fn new(config: Config) -> Arc<Runner> {
        Arc::new_cyclic(|me| Runner {
            config,
            state: Mutex::default(),
            changed: OnceLock::new(),
            me: me.clone(),
            ids: AtomicU64::new(0),
        })
    }

    /// Sets what is told the project whose runs changed; the first call wins.
    pub fn on_change(&self, changed: impl Fn(&str) + Send + Sync + 'static) {
        let _ = self.changed.set(Box::new(changed));
    }

    fn tell(&self, project: &str) {
        if let Some(changed) = self.changed.get() {
            changed(project);
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.config.runs.join(id)
    }

    /// Makes the folder of a new run and gives its working folder, empty, for the caller to fill and name in its
    /// [`Request`]. A folder never submitted is removed at the next start.
    pub fn new_folder(&self) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.config.runs).map_err(|e| e.to_string())?;
        loop {
            let n = self.ids.fetch_add(1, Ordering::Relaxed);
            let dir =
                self.config
                    .runs
                    .join(format!("{:x}-{:x}-{n:x}", millis(), std::process::id()));
            match fs::create_dir(&dir) {
                Ok(()) => {
                    let work = dir.join(WORK);
                    return fs::create_dir(&work)
                        .map(|()| work)
                        .map_err(|e| e.to_string());
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    /// Queues `request` and starts it when its project has no run going: the run's id. Reads `settings.json` now, so a
    /// saved change counts from the next run. A run that cannot start is refused with the prompt to copy instead, and its
    /// folder is removed: a harness other than `claude`, a kind with a shell in a project with no `checks`, or a harness
    /// that builds no command.
    pub fn submit(&self, request: Request) -> Result<String, Refusal> {
        // The folder of the new run; none for a folder that is no run's, or that a run was already submitted with.
        let new = self
            .run_dir_of(&request.folder)
            .filter(|dir| !dir.join(RUN).exists());
        let refuse = |line: String| {
            // Only a new run's own folder is ever removed: never a submitted run's, never a folder outside the runs folder.
            if let Some(dir) = &new {
                let _ = fs::remove_dir_all(dir);
            }
            Refusal {
                line,
                prompt: request.prompt.clone(),
            }
        };
        let Some(dir) = &new else {
            return Err(refuse("the working folder is not a new run's".into()));
        };
        let id = dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let text = fs::read_to_string(&self.config.settings).unwrap_or_default();
        let settings = crate::settings::run_settings_in(&text, &request.project);
        if settings.harness != "claude" {
            let harness = settings.harness;
            return Err(refuse(format!(
                "backgroundRuns.harness is `{harness}`: only `claude` runs"
            )));
        }
        if request.kind.shell() && settings.checks.is_empty() {
            return Err(refuse("Set this project's checks".into()));
        }
        let plan = Plan {
            kind: request.kind,
            project: &request.project,
            folder: &request.folder,
            prompt: &request.prompt,
            model: settings.model.as_deref(),
            checks: &settings.checks,
            data: &self.config.data,
        };
        let mut command = self.config.harness.command(&plan).map_err(refuse)?;
        let cores = cores().to_string();
        for name in ["CARGO_BUILD_JOBS", "NEXTEST_TEST_THREADS"] {
            command.env.insert(name.into(), cores.clone());
        }
        let stored = Stored {
            id: id.clone(),
            kind: request.kind,
            project: request.project.clone(),
            folder: request.folder.clone(),
            prompt: request.prompt.clone(),
            created_ms: millis(),
            timeout_secs: request.limits.timeout.as_secs().max(1),
            setup: settings.setup,
            command,
        };
        write_json(&dir.join(RUN), &stored)
            .map_err(|e| refuse(format!("the run could not be saved: {e}")))?;
        let mut state = self.state();
        state.runs.push(Run {
            stored,
            phase: Phase::Queued,
        });
        self.advance(&mut state);
        drop(state);
        self.tell(&request.project);
        Ok(id)
    }

    /// The folder of the run whose working folder is `folder`: `<runs>/<id>` for `<runs>/<id>/work`, and no other path.
    fn run_dir_of(&self, folder: &Path) -> Option<PathBuf> {
        let dir = folder.parent()?;
        // A plain name under the runs folder: never `..`, whose parent would read as the runs folder too.
        dir.file_name()?;
        (folder.file_name()? == WORK && dir.parent()? == self.config.runs)
            .then(|| dir.to_path_buf())
    }

    /// Starts the first queued run of each project that has none going, unless the queues are paused. A project is busy
    /// while any of its runs is running, wherever that run sits in the order.
    fn advance(&self, state: &mut State) {
        if state.paused.is_some() {
            return;
        }
        let running = state
            .runs
            .iter()
            .filter(|run| matches!(run.phase, Phase::Running { .. }));
        let mut busy: Vec<String> = running.map(|run| run.stored.project.clone()).collect();
        for run in &mut state.runs {
            if !matches!(run.phase, Phase::Queued) || busy.contains(&run.stored.project) {
                continue;
            }
            run.phase = self.start(&run.stored);
            // A run that could not start ended at once, and the next in its queue has its turn.
            if matches!(run.phase, Phase::Running { .. }) {
                busy.push(run.stored.project.clone());
            }
        }
    }

    /// Starts the launcher of `stored`, in a process group of its own, holding the run's lock from before it runs: the
    /// lock is taken here and handed over as the launcher's standard input, so there is no moment a started run looks dead.
    fn start(&self, stored: &Stored) -> Phase {
        let dir = self.dir(&stored.id);
        let since_ms = millis();
        let started = (|| -> io::Result<Child> {
            // Written first: a start that got no further is then found as a dead run, never as a queued one.
            write_json(
                &dir.join(LAUNCH),
                &Launch {
                    at_ms: since_ms,
                    ..Launch::default()
                },
            )?;
            let lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(dir.join(LOCK))?;
            lock.try_lock()
                .map_err(|_| io::Error::other("the run's lock is held"))?;
            Command::new(&self.config.launcher)
                .arg(&dir)
                .stdin(lock)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .process_group(0)
                .spawn()
        })();
        match started {
            Ok(child) => {
                let pid = child.id();
                let who = Launch {
                    pid: Some(pid),
                    start: process_start(pid),
                    at_ms: since_ms,
                };
                if let Err(e) = write_json(&dir.join(LAUNCH), &who) {
                    log::warn!(
                        "background run {}: its process was not written down: {e}",
                        stored.id
                    );
                }
                self.watch(stored.id.clone(), Some(child));
                Phase::Running { since_ms }
            }
            Err(e) => {
                let launcher = self.config.launcher.display();
                let outcome = Outcome::bare(
                    End::Failed,
                    format!("the launcher `{launcher}` did not start: {e}"),
                );
                self.record(stored, serde_json::json!(outcome.end), Some(&outcome), 0);
                let _ = fs::write(dir.join(RECORDED), "");
                Phase::Ended {
                    outcome,
                    at_ms: millis(),
                }
            }
        }
    }

    /// Waits for run `id`'s end on a thread of its own, by waiting for its lock, and then reads how it ended. `child` is the
    /// launcher when this plotkeeper started it, to reap.
    fn watch(&self, id: String, child: Option<Child>) {
        let (dir, me) = (self.dir(&id), self.me.clone());
        thread::spawn(move || {
            wait_free(&dir);
            if let Some(mut child) = child {
                let _ = child.wait();
            }
            if let Some(runner) = me.upgrade() {
                runner.ended(&id);
            }
        });
    }

    /// Run `id`'s launcher is gone: its outcome is read and recorded, a login or limit failure pauses the queues, and the
    /// next run of its project starts.
    fn ended(&self, id: &str) {
        let mut state = self.state();
        let Some(run) = state.runs.iter_mut().find(|run| run.stored.id == id) else {
            return;
        };
        if !matches!(run.phase, Phase::Running { .. }) {
            return;
        }
        let project = run.stored.project.clone();
        let pause = self.finish(run).pause();
        if pause.is_some() {
            self.pause(&mut state, pause);
        }
        self.advance(&mut state);
        // A pause holds every project's queue, so each project with runs draws again.
        let told = Self::told(&state, pause, &project);
        drop(state);
        told.iter().for_each(|project| self.tell(project));
    }

    /// The projects that have a run, each once.
    fn projects(state: &State) -> Vec<String> {
        let mut projects: Vec<String> = state
            .runs
            .iter()
            .map(|run| run.stored.project.clone())
            .collect();
        projects.sort();
        projects.dedup();
        projects
    }

    /// Reads the ended `run`'s outcome from its folder, records it and marks the run ended.
    fn finish(&self, run: &mut Run) -> End {
        let dir = self.dir(&run.stored.id);
        let (outcome, status) = read_outcome(&dir);
        let launched = read_json::<Launch>(&dir.join(LAUNCH)).map_or(0, |launch| launch.at_ms);
        let (from, to) = status.map_or((launched, millis()), |s| (s.started_ms, s.ended_ms));
        let duration = to.saturating_sub(from);
        self.record(
            &run.stored,
            serde_json::json!(outcome.end),
            Some(&outcome),
            duration,
        );
        let _ = fs::write(dir.join(RECORDED), "");
        let end = outcome.end;
        run.phase = Phase::Ended { outcome, at_ms: to };
        end
    }

    /// Appends one line to the record of runs: kind, project, outcome, denials, cost and duration. `outcome` is how the
    /// run ended, or `cancelled`; `of` is what an ended run gave back.
    fn record(
        &self,
        stored: &Stored,
        outcome: serde_json::Value,
        of: Option<&Outcome>,
        duration_ms: u64,
    ) {
        let line = serde_json::json!({
            "at": millis() / 1000,
            "id": stored.id,
            "kind": stored.kind,
            "project": stored.project,
            "outcome": outcome,
            "denials": of.map_or(&[][..], |of| &of.denials),
            "cost": of.and_then(|of| of.cost),
            "duration_ms": duration_ms,
        });
        let appended = fs::create_dir_all(&self.config.data).and_then(|()| {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.config.data.join(RECORDS))?;
            file.write_all(format!("{line}\n").as_bytes())
        });
        if let Err(e) = appended {
            log::error!("save the record of background run {}: {e}", stored.id);
        }
    }

    /// Pauses the queues for `why`, or resumes them with none, and keeps that for the next start.
    fn pause(&self, state: &mut State, why: Option<Pause>) {
        state.paused = why;
        let file = self.config.runs.join(PAUSED);
        let saved = match why {
            Some(why) => write_json(&file, &serde_json::json!({ "end": why })),
            None => fs::remove_file(&file).or_else(|e| match e.kind() {
                io::ErrorKind::NotFound => Ok(()),
                _ => Err(e),
            }),
        };
        if let Err(e) = saved {
            log::warn!("background runs: the pause was not saved: {e}");
        }
    }

    /// Daniel resumed the queues after a login or limit failure.
    pub fn resume(&self) {
        let mut state = self.state();
        self.pause(&mut state, None);
        self.advance(&mut state);
        let projects = Self::projects(&state);
        drop(state);
        projects.iter().for_each(|project| self.tell(project));
    }

    /// Cancel: a queued run is dropped, a running one has its process group killed, and either way the run's folder is
    /// removed and the next run of its project starts. An ended run is only removed, and so is a run that ended on its own
    /// at that moment, which is recorded as it ended. A running run is killed only when its lock is held and the process
    /// its folder names still has the start time written down, so a process id that was reused is never signalled;
    /// otherwise nothing is stopped or removed.
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let mut state = self.state();
        let at = state
            .runs
            .iter()
            .position(|run| run.stored.id == id)
            .ok_or("no such run")?;
        let dir = self.dir(id);
        let mut pause = None;
        if matches!(state.runs[at].phase, Phase::Running { .. }) && !Self::stop(&dir)? {
            // It ended on its own: a login or limit failure still pauses.
            pause = self.finish(&mut state.runs[at]).pause();
            if pause.is_some() {
                self.pause(&mut state, pause);
            }
        }
        let run = state.runs.remove(at);
        if !matches!(run.phase, Phase::Ended { .. }) {
            let since = match run.phase {
                Phase::Running { since_ms } => millis().saturating_sub(since_ms),
                _ => 0,
            };
            self.record(&run.stored, "cancelled".into(), None, since);
        }
        if let Err(e) = fs::remove_dir_all(&dir) {
            log::warn!("background run {id}: its folder was not removed: {e}");
        }
        self.advance(&mut state);
        let told = Self::told(&state, pause, &run.stored.project);
        drop(state);
        told.iter().for_each(|project| self.tell(project));
        Ok(())
    }

    /// Stops the running run in `dir` by killing its launcher's process group, and waits until its lock is free: true when
    /// it was killed here, false when its launcher had already gone, or was going at this very moment, so there was
    /// nothing to stop. Refuses, with the lock still held, when the process the folder names is not the one that was
    /// started.
    fn stop(dir: &Path) -> Result<bool, String> {
        if lock_free(dir) {
            return Ok(false);
        }
        let not_ours =
            "the run's process is not the one plotkeeper started, so nothing was stopped";
        let launch: Launch = read_json(&dir.join(LAUNCH)).unwrap_or_default();
        let Some((pid, start)) = launch.pid.zip(launch.start) else {
            return Err(not_ours.into());
        };
        let refused = match process_start(pid) {
            Some(now) if now == start => {
                let group =
                    i32::try_from(pid).map_err(|_| "the run's process id is out of range")?;
                // SAFETY: a signal to the process group the launcher leads, which the lock and the start time just named.
                if unsafe { libc::killpg(group, libc::SIGKILL) } == 0 {
                    wait_free(dir);
                    return Ok(true);
                }
                let why = io::Error::last_os_error();
                format!("the run's processes could not be stopped: {why}")
            }
            // Another process has the id: never signalled.
            Some(_) => return Err(not_ours.into()),
            None => not_ours.to_string(),
        };
        // A launcher that is exiting answers to neither `ps` nor a signal, and frees its lock a moment later.
        for _ in 0..100 {
            if lock_free(dir) {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(10));
        }
        Err(refused)
    }

    /// The projects to tell of a change in `project`: every project with runs when the change paused the queues.
    fn told(state: &State, pause: Option<Pause>, project: &str) -> Vec<String> {
        match pause {
            Some(_) => Self::projects(state),
            None => vec![project.to_string()],
        }
    }

    /// Picks the runs back up at a start, from the runs folder: a queued run waits its turn again, a run whose lock is
    /// held is still running and is waited for, and a run whose lock is free ended meanwhile: with no status it died. An
    /// ended run not yet recorded is recorded and listed; one already recorded is an orphan, and its folder is removed,
    /// as is a folder that holds no run.
    pub fn pick_up(&self) {
        let mut state = self.state();
        // The file is the pause: one whose reason does not read still pauses.
        state.paused = fs::read(self.config.runs.join(PAUSED)).ok().map(|text| {
            let paused = serde_json::from_slice::<serde_json::Value>(&text).unwrap_or_default();
            serde_json::from_value(paused["end"].clone()).unwrap_or(Pause::Other)
        });
        let folders = fs::read_dir(&self.config.runs)
            .into_iter()
            .flatten()
            .flatten();
        let mut runs = Vec::new();
        let mut paused = None;
        for dir in folders
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
        {
            let stored =
                read_json::<Stored>(&dir.join(RUN)).filter(|stored| self.dir(&stored.id) == dir);
            let orphan = dir.join(RECORDED).exists();
            let Some(stored) = stored.filter(|_| !orphan) else {
                // A folder this build cannot read is removed only once nothing runs in it: a newer build's run goes on.
                if lock_free(&dir) {
                    let _ = fs::remove_dir_all(&dir);
                } else {
                    log::warn!(
                        "background runs: {} holds a running run this build cannot read",
                        dir.display()
                    );
                }
                continue;
            };
            let mut run = Run {
                stored,
                phase: Phase::Queued,
            };
            if let Some(launch) = read_json::<Launch>(&dir.join(LAUNCH)) {
                if lock_free(&dir) {
                    paused = self.finish(&mut run).pause().or(paused);
                } else {
                    run.phase = Phase::Running {
                        since_ms: launch.at_ms,
                    };
                    self.watch(run.stored.id.clone(), None);
                }
            }
            runs.push(run);
        }
        runs.sort_by(|a, b| {
            (a.stored.created_ms, &a.stored.id).cmp(&(b.stored.created_ms, &b.stored.id))
        });
        state.runs = runs;
        if paused.is_some() {
            self.pause(&mut state, paused);
        }
        self.advance(&mut state);
    }

    /// `project`'s runs as the map shows them, and the pause line.
    pub fn runs(&self, project: &str) -> Runs {
        let state = self.state();
        let runs = state
            .runs
            .iter()
            .filter(|run| run.stored.project == project)
            .map(|run| {
                let stored = &run.stored;
                let (state, at, outcome) = match &run.phase {
                    Phase::Queued => (RunState::Queued, stored.created_ms, None),
                    Phase::Running { since_ms } => (RunState::Running, *since_ms, None),
                    Phase::Ended { outcome, at_ms } => (RunState::Ended, *at_ms, Some(outcome)),
                };
                RunView {
                    id: stored.id.clone(),
                    kind: stored.kind,
                    state,
                    at,
                    outcome: outcome.map(|outcome| outcome.end),
                    line: outcome.map(|outcome| line(stored, outcome)),
                    denials: outcome
                        .map(|outcome| outcome.denials.clone())
                        .unwrap_or_default(),
                    prompt: outcome
                        .filter(|outcome| outcome.end != End::Done)
                        .map(|_| stored.prompt.clone()),
                }
            });
        Runs {
            paused: state.paused.map(|why| why.line().to_string()),
            runs: runs.collect(),
        }
    }

    /// What ended run `id` gave back; none while it is queued or running, and once it is removed.
    pub fn outcome(&self, id: &str) -> Option<Outcome> {
        let state = self.state();
        let run = state.runs.iter().find(|run| run.stored.id == id)?;
        match &run.phase {
            Phase::Ended { outcome, .. } => Some(outcome.clone()),
            _ => None,
        }
    }
}
