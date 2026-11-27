//! Background runs (spec danielrispler/plotkeeper#378, ticket #390): a headless agent run with scoped tools, started by
//! Daniel's click and never a session. This is the runner; back a law (`back_a_law.rs`), micro-worlds (`micro_worlds.rs`),
//! the map text review (`map_review.rs`, which also starts by itself for Daniel's own PRs) and First map (`map_draft.rs`) start
//! runs.
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
//! - **The limits.** One run at a time per project, or one batch of runs that hold the project's slot together
//!   ([`Runner::submit_batch`]), the others queued in the order they were asked for; a time limit the launcher keeps; a
//!   third of the cores for builds; and a failure on the login or a rate limit pauses every queue until
//!   [`Runner::resume`].

use crate::records::Notice;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions, TryLockError},
    io::{self, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use studio_understanding::{FailureKind, login_or_quota, millis};
use studio_workbench::run::RunError;

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
/// How long a Stop waits for a launcher to end its child before it kills the launcher outright.
const STOP_GRACE: Duration = Duration::from_secs(5);
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
    /// Checks a PR's changed map text against the PR's code, reading only (`map_review.rs`).
    MapReview,
    /// Writes or reviews a part group's text for a repository's first map, reading only and answering in its result.
    FirstMap,
}

impl Kind {
    /// The tools the agent has, and no other.
    pub fn tools(self) -> &'static [&'static str] {
        match self {
            Kind::BackLaw => &["Bash", "Read", "Edit", "Write", "Glob", "Grep"],
            Kind::MicroWorld => &["Read", "Edit", "Write", "Glob", "Grep"],
            Kind::MapReview | Kind::FirstMap => &["Read", "Glob", "Grep"],
        }
    }

    /// Whether the project's `setup` runs before the agent. Never for a map text review: its folder holds a PR's head, whose
    /// code a setup such as `npm ci` would run outside the sandbox, and with no shell it needs none. Nor for a first map,
    /// which only reads.
    fn takes_setup(self) -> bool {
        !matches!(self, Kind::MapReview | Kind::FirstMap)
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
    /// 30 minutes, and 10 for a micro-world, a map text review or a first map.
    pub fn of(kind: Kind) -> Limits {
        let minutes = match kind {
            Kind::BackLaw => 30,
            Kind::MicroWorld | Kind::MapReview | Kind::FirstMap => 10,
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
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HarnessCommand {
    pub program: PathBuf,
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_remove: Vec<String>,
    /// A command the launcher runs first, which must exit 0: whether the sandbox can start. When it fails, the run fails
    /// with [`SANDBOX_DOWN`] and the harness never starts. Empty: no check.
    #[serde(default)]
    pub check: Vec<String>,
    /// A folder the launcher makes before the harness, new and private (`0700`), and removes when the harness exits: the
    /// run's own temp folder.
    #[serde(default)]
    pub private_tmp: Option<PathBuf>,
}

/// What a run that found no working sandbox ends with.
pub const SANDBOX_DOWN: &str = "The sandbox could not start";

/// Whether Seatbelt can apply a profile here: it cannot inside another sandbox, such as a plotkeeper started from a
/// sandboxed shell. Claude Code keeps such a run going with every command refused and exits 0 (sandbox gate, #395). The
/// profile must deny something: inside another sandbox, an `(allow default)` profile still applies, and only one that
/// restricts fails, as Claude Code's does.
pub const SANDBOX_CHECK: [&str; 4] = [
    "/usr/bin/sandbox-exec",
    "-p",
    "(version 1)(allow default)(deny network-outbound)",
    "/usr/bin/true",
];

/// Whether Claude Code's Linux sandbox can start here (#704): bubblewrap and socat on `PATH`, where Claude Code looks
/// them up, and bubblewrap starting a sandbox in the shape Claude Code's sandbox runtime gives it, with its own network
/// namespace and a fresh `/proc`. A missing tool gets plotkeeper's message naming what to install; a container or
/// kernel that refuses gets bubblewrap's own words ("No permissions to create new namespace", "Can't mount proc").
/// Without the check, Claude Code runs commands unsandboxed when either tool is missing. The shape and the texts were
/// measured in Docker (#704).
pub const BWRAP_CHECK: [&str; 3] = [
    "/bin/sh",
    "-c",
    "command -v bwrap >/dev/null 2>&1 || { echo 'bubblewrap (bwrap) is not installed: install bubblewrap' >&2; exit 1; }
command -v socat >/dev/null 2>&1 || { echo 'socat is not installed: install socat' >&2; exit 1; }
exec bwrap --new-session --die-with-parent --unshare-net --ro-bind / / --dev /dev --unshare-pid --unshare-user \
--cap-drop ALL --proc /proc -- /bin/sh -c true",
];

/// The sandbox check of this platform: Seatbelt's on macOS, bubblewrap's elsewhere.
fn sandbox_check() -> Vec<String> {
    #[cfg(target_os = "macos")]
    let check = SANDBOX_CHECK.as_slice();
    #[cfg(not(target_os = "macos"))]
    let check = BWRAP_CHECK.as_slice();
    check.iter().map(|word| (*word).to_string()).collect()
}

/// What Claude Code and Seatbelt print when the sandbox fails after the start: a run that shows one failed, whatever its
/// exit. Whole phrases, so a result that only names the setting does not match.
const SANDBOX_FAILURES: [&str; 2] = [
    "sandbox_apply: Operation not permitted",
    "Sandbox is required but",
];

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
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
        let program = studio_understanding::find(shell.as_ref(), "claude")
            .ok_or("`claude` was not found, so the run cannot start")?;
        Ok(claude_command(&program, plan))
    }
}

/// The sandbox block a run's `--settings` carries, as spec #378 gives it and the sandbox gate (#395) settled: the user's
/// hooks off, Claude Code's OS sandbox on and required, no way out of it, and no reading of plotkeeper's data folder
/// (`data`, which holds its token) or of a credential folder. `autoAllowBashIfSandboxed` must stay `false`: with `true`,
/// commands outside the allow list ran. `allowLocalBinding` lets a project's tests serve on loopback, which plotkeeper's
/// own need; it also lets a run's tools reach any server on the Mac's loopback, a limit v1 accepts.
pub fn sandbox_settings(data: &Path) -> serde_json::Value {
    serde_json::json!({
        "disableAllHooks": true,
        "sandbox": {
            "enabled": true,
            "failIfUnavailable": true,
            "allowUnsandboxedCommands": false,
            "autoAllowBashIfSandboxed": false,
            "network": { "allowLocalBinding": true },
            "filesystem": { "denyRead": [
                data.to_string_lossy(), "~/.claude", "~/.claude.json", "~/.ssh", "~/.aws", "~/.config/gh", "~/.npmrc",
                "~/.netrc", "~/.cargo/credentials.toml", "~/.gnupg", "~/.kube",
            ] },
        },
    })
}

/// The git commands a run's shell may use: reading, and staging inside its own clone. Never a commit, a push or `gh`.
const GIT: [&str; 4] = ["git status", "git diff", "git log", "git add"];

/// The folder inside a micro-world's working folder that the agent writes its page into, and the only place it may write: the
/// rest of the working folder is the project's files at the commit the page is built at, to read (#399).
pub const MICRO_WORLD_OUT: &str = ".micro-world";

/// What `--allowedTools` lists for a run: edits inside its working `folder` (a micro-world's only inside
/// [`MICRO_WORLD_OUT`] there, and none for a map text review or a first map, which only read), and for a kind with a
/// shell the read-only git commands and one entry per command of the project's `checks`, each bare and with arguments. An
/// entry of `checks` that has `git` or `gh` as any of its words is left out, so no list ever allows a push. This list is
/// not a security boundary: an allowed check runs code the agent wrote. The OS sandbox and trusted input are.
pub fn allowed_tools(kind: Kind, folder: &Path, checks: &[String]) -> Vec<String> {
    // The rule's own form for an absolute path: `//` and the path.
    let writes = match kind {
        Kind::MapReview | Kind::FirstMap => return Vec::new(),
        Kind::MicroWorld => folder.join(MICRO_WORLD_OUT),
        Kind::BackLaw => folder.to_path_buf(),
    };
    let mut allowed = vec![format!("Edit(/{}/**)", writes.display())];
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
pub(crate) fn cores() -> usize {
    (thread::available_parallelism().map_or(1, std::num::NonZero::get) / 3).max(1)
}

/// The `claude` command for `plan`, built and never run here: no user settings, hooks, plugins, skills or MCP servers
/// (`--restricted --safe-mode --strict-mcp-config`), nothing asked and nothing allowed that the list does not name
/// (`dontAsk`), the kind's tools, the sandbox block, and the prompt as its last argument since standard input is closed.
/// It runs on the CLI's own login, as the writer does: the API key variables are removed, and `PLOTKEEPER_INTERNAL=1`
/// keeps the hook from recording it. The launcher first checks that the sandbox can start ([`SANDBOX_CHECK`] on macOS,
/// [`BWRAP_CHECK`] on Linux), and gives the run a private temp folder, [`private_tmp`], as `CLAUDE_CODE_TMPDIR`: without
/// it, the sandbox lets the tools write into `/tmp/claude-<uid>`, where every session keeps its scratchpad.
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
    // `--allowedTools` takes several values, so an option must follow it. An empty list is no flag at all.
    let allowed = allowed_tools(plan.kind, plan.folder, plan.checks);
    if !allowed.is_empty() {
        args.push("--allowedTools".into());
        args.extend(allowed);
    }
    args.extend(["--settings".into(), sandbox_settings(plan.data).to_string()]);
    args.extend(["--output-format".into(), "json".into()]);
    // NUL bytes, which no argument can hold, are dropped.
    args.extend(["-p".into(), plan.prompt.replace('\0', "")]);
    let tmp = private_tmp(plan.folder);
    HarnessCommand {
        program: program.into(),
        args,
        env: [
            ("PLOTKEEPER_INTERNAL".to_string(), "1".to_string()),
            (
                "CLAUDE_CODE_TMPDIR".to_string(),
                tmp.to_string_lossy().into_owned(),
            ),
        ]
        .into(),
        env_remove: [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "CLAUDE_CODE_PLUGIN_DIRS",
        ]
        .map(String::from)
        .into(),
        check: sandbox_check(),
        private_tmp: Some(tmp),
    }
}

/// The run's own temp folder, `/tmp/pk-<16 hex>` named from its working `folder`. Claude Code gives the sandboxed tools
/// `<CLAUDE_CODE_TMPDIR>/claude-<uid>` as their `TMPDIR` only while that path is at most 44 bytes, and otherwise falls
/// back to the shared `/tmp/claude-<uid>` (2.1.325; research `sandbox-loopback-preflight-temp.md`), so the name is of
/// fixed length, 42 bytes with the longest uid, and long enough (64 bits) that two runs never share it.
pub fn private_tmp(folder: &Path) -> PathBuf {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    folder.hash(&mut hasher);
    PathBuf::from(format!("/tmp/pk-{:016x}", hasher.finish()))
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

impl Config {
    /// The runner of the plotkeeper whose event log is `log`: the data folder is the log's, the launcher sits beside this
    /// executable, and the harness is Claude Code.
    pub fn beside(log: &Path) -> Config {
        let data = log.parent().unwrap_or(Path::new("")).to_path_buf();
        let exe = std::env::current_exe().and_then(|exe| exe.canonicalize());
        let beside = exe.ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
        Config {
            runs: std::env::home_dir().map_or_else(
                || data.join("../plotkeeper-runs"),
                |h| studio_hook::runs_dir(&h),
            ),
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
    /// The batch the run is part of, named by its first run's id ([`Runner::submit_batch`]); none for a run on its own.
    #[serde(default)]
    batch: Option<String>,
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

/// Writes `value` as `file` whole, by the workbench's atomic write.
fn write_json(file: &Path, value: &impl Serialize) -> io::Result<()> {
    studio_workbench::write_atomic(file, &serde_json::to_vec(value)?)
}

/// Whether nobody holds the lock of the run in `dir`: its launcher is gone, or never started.
fn lock_free(dir: &Path) -> bool {
    let Ok(lock) = File::open(dir.join(LOCK)) else {
        return true;
    };
    !matches!(lock.try_lock(), Err(TryLockError::WouldBlock))
}

/// Whether the lock of the run in `dir` is free within `grace`.
fn free_within(dir: &Path, grace: Duration) -> bool {
    let end = Instant::now() + grace;
    loop {
        if lock_free(dir) {
            return true;
        }
        if Instant::now() >= end {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
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
    listen_for_stop();
    let stored: Launched = read_json(&dir.join(RUN)).ok_or("the folder holds no run")?;
    let started_ms = millis();
    let deadline = Instant::now() + Duration::from_secs(stored.timeout_secs);
    let tmp = stored.command.private_tmp.as_deref();
    // Only a folder this launcher made is removed: never one that was there before, which fails the run.
    let mut made = None;
    let status = match run_check(&stored, deadline)
        .and_then(|()| run_setup(dir, &stored, deadline))
        .and_then(|()| make_private(tmp).map(|()| made = tmp))
        .and_then(|()| run_harness(dir, &stored, deadline))
    {
        Ok(exit) => Status {
            exit,
            ..Status::default()
        },
        Err(status) => status,
    };
    if let Some(tmp) = made {
        // Best effort: a file already gone is fine.
        fs::remove_dir_all(tmp).ok();
    }
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

/// Runs `command` until `deadline`, through the one process runner: its exit code, none when a signal ended it. The
/// run's time limit passing, or a Stop ([`stopped`]), ends it as timed out, with nothing it started left running.
fn run_step(command: &mut Command, deadline: Instant) -> Result<Option<i32>, RunOutcome> {
    let left = deadline.saturating_duration_since(Instant::now());
    match studio_workbench::run::run_logged(command, left, &stopped) {
        Ok(status) => Ok(status.code()),
        Err(RunError::TimedOut(_)) => Err(RunOutcome::TimedOut),
        Err(RunError::Start(e)) => Err(RunOutcome::NotStarted(e.to_string())),
        // A logged run gives any exit as its status.
        Err(error @ RunError::Failed(_)) => Err(RunOutcome::NotStarted(error.to_string())),
    }
}

/// Why [`run_step`] gave no exit code.
enum RunOutcome {
    TimedOut,
    NotStarted(String),
}

fn timed_out() -> Status {
    Status {
        timed_out: true,
        ..Status::default()
    }
}

/// Runs the command's sandbox check within the run's time limit: a failure is [`SANDBOX_DOWN`], with what the check
/// printed, or its exit code when it printed nothing.
fn run_check(stored: &Launched, deadline: Instant) -> Result<(), Status> {
    let Some((program, args)) = stored.command.check.split_first() else {
        return Ok(());
    };
    let down = |why: String| Status {
        error: Some(format!("{SANDBOX_DOWN}: {why}")),
        ..Status::default()
    };
    let mut check = Command::new(program);
    check.args(args);
    let left = deadline.saturating_duration_since(Instant::now());
    let output = match studio_workbench::run::run_until_unless(&mut check, left, &stopped) {
        Ok(_) => return Ok(()),
        Err(RunError::TimedOut(_)) => return Err(timed_out()),
        Err(RunError::Start(e)) => return Err(down(e.to_string())),
        Err(RunError::Failed(output)) => output,
    };
    Err(down(match String::from_utf8_lossy(&output.stderr).trim() {
        "" => output
            .status
            .code()
            .map_or("the check was stopped by a signal".into(), |code| {
                format!("the check exited {code}")
            }),
        said => said.to_string(),
    }))
}

/// Makes the run's private temp folder, which must be new: a folder already there is never shared with a run.
fn make_private(tmp: Option<&Path>) -> Result<(), Status> {
    use std::os::unix::fs::DirBuilderExt;
    let Some(tmp) = tmp else {
        return Ok(());
    };
    fs::DirBuilder::new()
        .mode(0o700)
        .create(tmp)
        .map_err(|e| Status {
            error: Some(format!("the run's temp folder {}: {e}", tmp.display())),
            ..Status::default()
        })
}

/// Runs the project's setup command, outside any sandbox and with the network, before the agent starts.
fn run_setup(dir: &Path, stored: &Launched, deadline: Instant) -> Result<(), Status> {
    let Some(setup) = &stored.setup else {
        return Ok(());
    };
    let log = output(dir, SETUP_LOG)?;
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", setup])
        .current_dir(&stored.folder)
        .envs(&stored.command.env)
        .stderr(log.try_clone().map_err(|e| Status {
            error: Some(e.to_string()),
            ..Status::default()
        })?)
        .stdout(log);
    let failed = |setup_exit| Status {
        setup_exit: Some(setup_exit),
        ..Status::default()
    };
    match run_step(&mut command, deadline) {
        Ok(Some(0)) => Ok(()),
        Ok(code) => Err(failed(code.unwrap_or(-1))),
        Err(RunOutcome::TimedOut) => Err(timed_out()),
        Err(RunOutcome::NotStarted(_)) => Err(failed(-1)),
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
        .stdout(output(dir, STDOUT)?)
        .stderr(output(dir, STDERR)?);
    for name in &command.env_remove {
        harness.env_remove(name);
    }
    run_step(&mut harness, deadline).map_err(|outcome| match outcome {
        RunOutcome::TimedOut => timed_out(),
        RunOutcome::NotStarted(e) => Status {
            error: Some(format!(
                "`{}` did not start: {e}",
                command.program.display()
            )),
            ..Status::default()
        },
    })
}

/// Set once plotkeeper's Stop has asked this launcher to end ([`Runner::stop`]).
static STOP: AtomicBool = AtomicBool::new(false);

/// Whether a Stop has asked this launcher to end. Its child runs in a process group of its own, which a kill of the
/// launcher's group would miss, so the runner kills that group when this turns true.
fn stopped() -> bool {
    STOP.load(Ordering::Relaxed)
}

extern "C" fn on_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

/// Has a `SIGTERM`, the Stop's ask, set [`STOP`] rather than end the launcher at once.
fn listen_for_stop() {
    // SAFETY: `action` is a zeroed `sigaction` that outlives the call; the handler only stores to an atomic, which is
    // safe in a signal handler.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_stop as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);
        libc::sigaction(libc::SIGTERM, &action, std::ptr::null_mut());
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
        let sandbox_failed = [&result, &errors]
            .iter()
            .any(|said| SANDBOX_FAILURES.iter().any(|word| said.contains(word)));
        let end = if sandbox_failed {
            End::Failed
        } else if status.exit != Some(0) || is_error {
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
        let result = if result.is_empty() {
            tail(&errors, 20)
        } else {
            result
        };
        Outcome {
            end,
            result: if sandbox_failed {
                format!("{SANDBOX_DOWN}\n{result}")
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
    /// The ended runs whose folder a module is still reading, by id, with how many holds each has ([`Runner::hold`]).
    held: BTreeMap<String, usize>,
    /// The batches whose run's end is being told, by project and batch, once per end: each holds its project's slot as a
    /// running run does, so a run the module joins then (a First map reviewer) starts before a request queued behind.
    telling: Vec<(String, String)>,
}

/// A module's hold on an ended run's folder: Dismiss is refused until it is dropped.
pub struct Hold {
    runner: Weak<Runner>,
    id: String,
}

impl Drop for Hold {
    fn drop(&mut self) {
        if let Some(runner) = self.runner.upgrade() {
            let mut state = runner.state();
            if let Some(count) = state.held.get_mut(&self.id) {
                *count -= 1;
                if *count == 0 {
                    state.held.remove(&self.id);
                }
            }
        }
    }
}

/// A run that ended, as told to the module that asked for it ([`Runner::on_end`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Ended {
    pub id: String,
    pub kind: Kind,
    pub project: String,
    /// Its working folder, which stays until the run is dismissed or plotkeeper starts again.
    pub folder: PathBuf,
    pub end: End,
}

type EndTold = Arc<dyn Fn(Ended) + Send + Sync>;

/// The background runs of every project: their queue, their processes and their outcomes.
pub struct Runner {
    config: Config,
    state: Mutex<State>,
    changed: Notice,
    /// What is told of a run's end, one per module that listens ([`Runner::on_end`]).
    ended: Mutex<Vec<EndTold>>,
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
            changed: Notice::default(),
            ended: Mutex::default(),
            me: me.clone(),
            ids: AtomicU64::new(0),
        })
    }

    /// Sets what is told the project whose runs changed; the first call wins.
    pub fn on_change(&self, changed: impl Fn(&str) + Send + Sync + 'static) {
        self.changed.set(changed);
    }

    /// Adds what is told when a run ends on its own, as it ends or when [`Runner::pick_up`] finds it ended; never for a
    /// cancelled run. Each module that listens adds its own, before `pick_up`, and each is told every end, in the order
    /// they were added. While the end of a batch's run is told, the batch keeps its project's slot, so a run joined to it
    /// then ([`Runner::join`]) starts at once, ahead of a request queued behind the batch.
    pub fn on_end(&self, ended: impl Fn(Ended) + Send + Sync + 'static) {
        self.ended
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Arc::new(ended));
    }

    fn tell(&self, project: &str) {
        self.changed.tell(project);
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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

    /// Queues `request` and starts it when its project has no run or batch going: the run's id. Reads `settings.json` now, so a
    /// saved change counts from the next run. A run that cannot start is refused with the prompt to copy instead, and its
    /// folder is removed: a harness other than `claude`, a kind with a shell in a project with no `checks`, or a harness
    /// that builds no command.
    pub fn submit(&self, request: Request) -> Result<String, Refusal> {
        let stored = self.prepare(&request, None)?;
        Ok(self.queue(vec![stored]).remove(0))
    }

    /// Queues `requests` as one batch, which holds its project's slot together: its runs start together, each with its own
    /// folder, limit and status, and the slot frees when the last of them ends. The ids of its runs, in order; the first
    /// names the batch, for [`Runner::join`] and [`Runner::cancel_batch`]. One request refused refuses the whole batch, as
    /// [`Runner::submit`] refuses a run, and removes every new folder of it.
    pub fn submit_batch(&self, requests: Vec<Request>) -> Result<Vec<String>, Refusal> {
        let batch = requests
            .first()
            .and_then(|first| self.run_dir_of(&first.folder))
            .and_then(|dir| {
                dir.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            });
        let mut prepared = Vec::new();
        for (at, request) in requests.iter().enumerate() {
            match self.prepare(request, batch.clone()) {
                Ok(stored) => prepared.push(stored),
                Err(refusal) => {
                    let others = requests.iter().enumerate().filter(|(each, _)| *each != at);
                    for (_, other) in others {
                        // The new runs' own folders, and the ones already prepared here: never a submitted run's.
                        let new = self.run_dir_of(&other.folder).filter(|dir| {
                            !dir.join(RUN).exists()
                                || prepared.iter().any(|stored| other.folder == stored.folder)
                        });
                        if let Some(dir) = new {
                            // Best effort: a file already gone is fine.
                            fs::remove_dir_all(dir).ok();
                        }
                    }
                    return Err(refusal);
                }
            }
        }
        Ok(self.queue(prepared))
    }

    /// Queues `request` in `batch`: while a run of that batch is running it starts at once, as a reviewer or a retry of a
    /// batch does; otherwise it waits its turn, and starts with the batch's other queued runs. Refused as
    /// [`Runner::submit`] refuses.
    pub fn join(&self, batch: &str, request: Request) -> Result<String, Refusal> {
        let stored = self.prepare(&request, Some(batch.to_string()))?;
        Ok(self.queue(vec![stored]).remove(0))
    }

    /// Queues the prepared runs and starts what may start: their ids.
    fn queue(&self, runs: Vec<Stored>) -> Vec<String> {
        let ids: Vec<String> = runs.iter().map(|stored| stored.id.clone()).collect();
        let mut projects: Vec<String> = runs.iter().map(|stored| stored.project.clone()).collect();
        projects.dedup();
        let mut state = self.state();
        state.runs.extend(runs.into_iter().map(|stored| Run {
            stored,
            phase: Phase::Queued,
        }));
        self.advance(&mut state);
        drop(state);
        projects.iter().for_each(|project| self.tell(project));
        ids
    }

    /// Checks `request` and saves it as a run of `batch` in its folder, or refuses it and removes that folder.
    fn prepare(&self, request: &Request, batch: Option<String>) -> Result<Stored, Refusal> {
        // The folder of the new run; none for a folder that is no run's, or that a run was already submitted with.
        let new = self
            .run_dir_of(&request.folder)
            .filter(|dir| !dir.join(RUN).exists());
        let refuse = |line: String| {
            // Only a new run's own folder is ever removed: never a submitted run's, never a folder outside the runs folder.
            if let Some(dir) = &new {
                // Best effort: a file already gone is fine.
                fs::remove_dir_all(dir).ok();
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
            setup: settings.setup.filter(|_| request.kind.takes_setup()),
            command,
            batch,
        };
        write_json(&dir.join(RUN), &stored)
            .map_err(|e| refuse(format!("the run could not be saved: {e}")))?;
        Ok(stored)
    }

    /// The folder of the run whose working folder is `folder`: `<runs>/<id>` for `<runs>/<id>/work`, and no other path.
    fn run_dir_of(&self, folder: &Path) -> Option<PathBuf> {
        let dir = folder.parent()?;
        // A plain name under the runs folder: never `..`, whose parent would read as the runs folder too.
        dir.file_name()?;
        (folder.file_name()? == WORK && dir.parent()? == self.config.runs)
            .then(|| dir.to_path_buf())
    }

    /// Starts the first queued run of each project that has none going, unless the queues are paused, and with it every
    /// other queued run of its batch. A project is busy while any of its runs is running, wherever that run sits in the
    /// order, or while the end of a batch's run is told; while that run is a batch's, a queued run of the same batch joins
    /// it and starts.
    fn advance(&self, state: &mut State) {
        if state.paused.is_some() {
            return;
        }
        // Each project that is busy, with the batch that holds it: none for a run on its own.
        let running = state
            .runs
            .iter()
            .filter(|run| matches!(run.phase, Phase::Running { .. }));
        let mut busy: Vec<(String, Option<String>)> = running
            .map(|run| (run.stored.project.clone(), run.stored.batch.clone()))
            .collect();
        let telling = state.telling.iter().cloned();
        busy.extend(telling.map(|(project, batch)| (project, Some(batch))));
        for run in &mut state.runs {
            let stored = &run.stored;
            let free = !busy.iter().any(|(project, _)| *project == stored.project);
            let joins = stored.batch.is_some()
                && busy.contains(&(stored.project.clone(), stored.batch.clone()));
            if !matches!(run.phase, Phase::Queued) || !(free || joins) {
                continue;
            }
            run.phase = self.start(&run.stored);
            // A run that could not start ended at once, and the next in its queue has its turn.
            if matches!(run.phase, Phase::Running { .. }) {
                busy.push((run.stored.project.clone(), run.stored.batch.clone()));
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
            lock.try_lock().map_err(|e| match e {
                TryLockError::WouldBlock => io::Error::other("the run's lock is held"),
                TryLockError::Error(e) => e,
            })?;
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
                if let Err(e) = fs::write(dir.join(RECORDED), "") {
                    log::warn!(
                        "run {}: its `recorded` mark was not written: {e}",
                        stored.id
                    );
                }
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
                // Only reaps the child; its exit status is not used.
                child.wait().ok();
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
        let end = self.finish(run);
        let ended = Self::ended_of(&run.stored, end);
        let telling = Self::telling(&mut state, &ended.id);
        let pause = end.pause();
        if pause.is_some() {
            self.pause(&mut state, pause);
        }
        self.advance(&mut state);
        // A pause holds every project's queue, so each project with runs draws again.
        let told = Self::told(&state, pause, &project);
        drop(state);
        self.tell_ends(vec![ended], telling);
        told.iter().for_each(|project| self.tell(project));
    }

    /// Marks the batch of ended run `id`, if it has one, as having its end told, which holds its project's slot: the mark,
    /// for [`Runner::tell_ends`] to take off.
    fn telling(state: &mut State, id: &str) -> Option<(String, String)> {
        let run = state.runs.iter().find(|run| run.stored.id == id)?;
        let mark = (run.stored.project.clone(), run.stored.batch.clone()?);
        state.telling.push(mark.clone());
        Some(mark)
    }

    fn ended_of(stored: &Stored, end: End) -> Ended {
        Ended {
            id: stored.id.clone(),
            kind: stored.kind,
            project: stored.project.clone(),
            folder: stored.folder.clone(),
            end,
        }
    }

    /// Tells every listener each of `ends`, with the runner's lock let go, then takes off the marks `telling` of their
    /// batches, so a batch with no run left frees its project's slot, and starts what may start.
    fn tell_ends(&self, ends: Vec<Ended>, telling: impl IntoIterator<Item = (String, String)>) {
        let listeners = self
            .ended
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        for ended in ends {
            for tell in &listeners {
                tell(ended.clone());
            }
        }
        let mut state = self.state();
        let mut freed = Vec::new();
        for mark in telling {
            if let Some(at) = state.telling.iter().position(|each| *each == mark) {
                freed.push(state.telling.remove(at).0);
            }
        }
        if freed.is_empty() {
            return;
        }
        self.advance(&mut state);
        drop(state);
        freed.dedup();
        freed.iter().for_each(|project| self.tell(project));
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
        // A launcher that ended with a status removed its own temp folder, if it made one. One that died wrote none, so
        // the folder is removed here, best effort, as Cancel removes it. A folder that was there before is never removed.
        let died = status.is_none();
        // Removed before the run is recorded: a crash in between leaves the run unrecorded, so the next start records it
        // and the folder is already gone.
        if let (true, Some(tmp)) = (died, &run.stored.command.private_tmp) {
            fs::remove_dir_all(tmp).ok();
        }
        let launched = read_json::<Launch>(&dir.join(LAUNCH)).map_or(0, |launch| launch.at_ms);
        let (from, to) = status.map_or((launched, millis()), |s| (s.started_ms, s.ended_ms));
        let duration = to.saturating_sub(from);
        self.record(
            &run.stored,
            serde_json::json!(outcome.end),
            Some(&outcome),
            duration,
        );
        if let Err(e) = fs::write(dir.join(RECORDED), "") {
            log::warn!(
                "run {}: its `recorded` mark was not written: {e}",
                run.stored.id
            );
        }
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

    /// Holds run `id`'s folder for a module that reads what the run made, such as a proof or a host check: Dismiss is
    /// refused while the hold lasts. None when the run is gone, its folder with it.
    pub fn hold(&self, id: &str) -> Option<Hold> {
        let mut state = self.state();
        if !state.runs.iter().any(|run| run.stored.id == id) {
            return None;
        }
        *state.held.entry(id.to_string()).or_default() += 1;
        drop(state);
        Some(Hold {
            runner: self.me.clone(),
            id: id.to_string(),
        })
    }

    /// Cancel: a queued run is dropped, a running one is stopped ([`Runner::stop`]), and either way the run's folder is
    /// removed and the next run of its project starts. An ended run is only removed, and so is a run that ended on its own
    /// at that moment, which is recorded as it ended. A running run is killed only when its lock is held and the process
    /// its folder names still has the start time written down, so a process id that was reused is never signalled;
    /// otherwise nothing is stopped or removed. A run a module holds ([`Runner::hold`]) is refused.
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        let mut state = self.state();
        let at = state
            .runs
            .iter()
            .position(|run| run.stored.id == id)
            .ok_or("no such run")?;
        if state.held.contains_key(id) {
            return Err(
                "plotkeeper is still checking what this run made: dismiss it once that is done"
                    .into(),
            );
        }
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
        // A launcher killed here never removed its temp folder. Its name is the run's own, so no other run holds it.
        if matches!(run.phase, Phase::Running { .. })
            && let Some(tmp) = &run.stored.command.private_tmp
        {
            // Best effort: a file already gone is fine.
            fs::remove_dir_all(tmp).ok();
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

    /// Discard of a batch: every run of it is cancelled ([`Runner::cancel`]), the queued ones first, so none of them starts
    /// while the running ones are stopped; once the last is gone its project's slot is free. Every run is tried, and the
    /// first refusal is told.
    pub fn cancel_batch(&self, batch: &str) -> Result<(), String> {
        let state = self.state();
        let mut runs: Vec<(bool, String)> = state
            .runs
            .iter()
            .filter(|run| run.stored.batch.as_deref() == Some(batch))
            .map(|run| (!matches!(run.phase, Phase::Queued), run.stored.id.clone()))
            .collect();
        drop(state);
        // `false` sorts first: the queued runs.
        runs.sort_by_key(|(started, _)| *started);
        let mut refused = None;
        for (_, id) in runs {
            if let Err(e) = self.cancel(&id) {
                refused.get_or_insert(e);
            }
        }
        refused.map_or(Ok(()), Err)
    }

    /// Stops the running run in `dir`, a `SIGTERM` to its launcher's process group and a `SIGKILL` past [`STOP_GRACE`],
    /// and waits until its lock is free: true when
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
                let group = i32::try_from(pid)
                    .map_err(|e| format!("the run's process id is out of range: {e}"))?;
                // Asked first: the launcher then kills its child's process group, which is not its own, and ends. One that
                // does not end within the grace is killed with its group.
                // SAFETY: signals to the process group the launcher leads, which the lock and the start time just named;
                // the lock still held says the launcher is there to be signalled again.
                if unsafe { libc::killpg(group, libc::SIGTERM) } == 0 {
                    if !free_within(dir, STOP_GRACE) {
                        // SAFETY: as above.
                        unsafe { libc::killpg(group, libc::SIGKILL) };
                        wait_free(dir);
                    }
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
        if free_within(dir, Duration::from_secs(1)) {
            return Ok(false);
        }
        Err(refused)
    }

    /// The projects to tell of a change in `project`: itself, and every project with runs when the change paused the
    /// queues. `project` is always told, also once its last run is gone.
    fn told(state: &State, pause: Option<Pause>, project: &str) -> Vec<String> {
        let mut told = match pause {
            Some(_) => Self::projects(state),
            None => Vec::new(),
        };
        if !told.iter().any(|each| each == project) {
            told.push(project.to_string());
        }
        told
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
        let mut ended = Vec::new();
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
                    // Best effort: a file already gone is fine.
                    fs::remove_dir_all(&dir).ok();
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
                    let end = self.finish(&mut run);
                    ended.push(Self::ended_of(&run.stored, end));
                    paused = end.pause().or(paused);
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
        let telling: Vec<(String, String)> = ended
            .iter()
            .filter_map(|ended| Self::telling(&mut state, &ended.id))
            .collect();
        self.advance(&mut state);
        drop(state);
        self.tell_ends(ended, telling);
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
        let outcome = match &run.phase {
            Phase::Ended { outcome, .. } => Some(outcome.clone()),
            _ => None,
        };
        drop(state);
        outcome
    }
}
