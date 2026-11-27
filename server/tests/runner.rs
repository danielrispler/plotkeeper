//! The background runner (spec danielrispler/plotkeeper#378, ticket #390) with a fake harness that is a real child process: a
//! run's prompt is the shell script the launcher runs, so a run sleeps, prints a result or dies as the test says. No test
//! runs `claude`: its command is built and read, never started.

mod isolated_home;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use studio_server::runner::{
    BWRAP_CHECK, Config, End, Harness, HarnessCommand, Kind, Limits, Plan, Request, RunState,
    Runner, SANDBOX_DOWN, allowed_tools, claude_command, private_tmp, sandbox_settings,
};
use studio_test_support::LauncherGuard;

const PROJECT: &str = "/p/app";

/// The fake harness: `sh -c <the run's prompt>`, with the checks the plan holds in `CHECKS`.
struct Script;

impl Harness for Script {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        Ok(HarnessCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), plan.prompt.to_string()],
            env: [("CHECKS".to_string(), plan.checks.join("|"))].into(),
            ..HarnessCommand::default()
        })
    }
}

/// [`Script`], with the private temp folder a real run has: the launcher makes it, named from the working folder.
struct WithTmp;

impl Harness for WithTmp {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let mut command = Script.command(plan)?;
        command.private_tmp = Some(private_tmp(plan.folder));
        Ok(command)
    }
}

/// A harness that is not installed.
struct Missing;

impl Harness for Missing {
    fn command(&self, _: &Plan) -> Result<HarnessCommand, String> {
        Err("`claude` was not found".into())
    }
}

struct Bench {
    runner: Arc<Runner>,
    changes: mpsc::Receiver<String>,
    /// Stops the launchers the test left running. Declared after the runner, so it drops once nothing can start another,
    /// and before `dir`, so it drops while their folders are still there.
    guard: LauncherGuard,
    dir: tempfile::TempDir,
}

fn config(dir: &Path, harness: Arc<dyn Harness>) -> Config {
    Config {
        data: dir.join("data"),
        runs: dir.join("runs"),
        settings: dir.join("data/settings.json"),
        launcher: env!("CARGO_BIN_EXE_plotkeeper-run").into(),
        harness,
    }
}

/// A runner over `dir`, as a start of plotkeeper makes it: the runs there are picked up.
fn open(dir: &Path, harness: Arc<dyn Harness>) -> (Arc<Runner>, mpsc::Receiver<String>) {
    fs::create_dir_all(dir.join("data")).unwrap();
    let runner = Runner::new(config(dir, harness));
    let (tell, changes) = mpsc::channel();
    runner.on_change(move |project| drop(tell.send(project.to_string())));
    runner.pick_up();
    (runner, changes)
}

/// A bench whose project has `cargo test` as its one check.
fn bench() -> Bench {
    bench_with(Arc::new(Script))
}

fn bench_with(harness: Arc<dyn Harness>) -> Bench {
    let dir = tempfile::tempdir().unwrap();
    settings(
        dir.path(),
        r#"{ "checks": { "/p/app": ["cargo test"], "/p/other": ["make"] } }"#,
    );
    let (runner, changes) = open(dir.path(), harness);
    Bench {
        guard: LauncherGuard::new(dir.path().join("runs")),
        dir,
        runner,
        changes,
    }
}

fn settings(dir: &Path, text: &str) {
    fs::create_dir_all(dir.join("data")).unwrap();
    fs::write(dir.join("data/settings.json"), text).unwrap();
}

impl Bench {
    /// Submits a run of `project` whose fake harness runs `script`; its id.
    fn run_in(&self, project: &str, script: &str) -> String {
        self.runner.submit(self.request(project, script)).unwrap()
    }

    fn run(&self, script: &str) -> String {
        self.run_in(PROJECT, script)
    }

    fn request(&self, project: &str, script: &str) -> Request {
        Request {
            kind: Kind::BackLaw,
            project: project.into(),
            folder: self.runner.new_folder().unwrap(),
            prompt: script.into(),
            limits: Limits::of(Kind::BackLaw),
        }
    }

    /// Waits, on the runner's own change notices, until `what` holds.
    fn until(&self, what: impl Fn(&Runner) -> bool) {
        until(&self.runner, &self.changes, what);
    }

    fn state(&self, id: &str) -> Option<RunState> {
        state(&self.runner, id)
    }

    fn folder(&self, id: &str) -> PathBuf {
        self.dir.path().join("runs").join(id)
    }

    fn records(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.dir.path().join("data/background-runs.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

fn until(runner: &Runner, changes: &mpsc::Receiver<String>, what: impl Fn(&Runner) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !what(runner) {
        let left = deadline.saturating_duration_since(Instant::now());
        changes
            .recv_timeout(left)
            .expect("the runner never reached the state the test waits for");
    }
}

fn state(runner: &Runner, id: &str) -> Option<RunState> {
    [PROJECT, "/p/other"]
        .iter()
        .flat_map(|project| runner.runs(project).runs)
        .find(|run| run.id == id)
        .map(|run| run.state)
}

fn ended(id: &str) -> impl Fn(&Runner) -> bool + '_ {
    move |runner| state(runner, id) == Some(RunState::Ended)
}

/// The script that prints `json` as the harness's result.
fn says(json: &str) -> String {
    format!("cat <<'EOF'\n{json}\nEOF")
}

/// A script that waits until the file `go` is in its working folder, then prints `result`.
fn waits(result: &str) -> String {
    format!("while [ ! -e go ]; do sleep 0.05; done; echo '{result}'")
}

fn alive(pid: u64) -> bool {
    studio_hook::sys::pid_alive(pid)
}

/// Waits until process `pid` is gone; a killed process takes a moment to be reaped.
fn gone(pid: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

fn launch(folder: &Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(folder.join("launch.json")).unwrap()).unwrap()
}

fn plan<'a>(kind: Kind, checks: &'a [String]) -> Plan<'a> {
    Plan {
        kind,
        project: "/p/app",
        folder: Path::new("/runs/r1/work"),
        prompt: "Back the law",
        model: Some("sonnet"),
        checks,
        data: Path::new("/home/d/Library/Application Support/plotkeeper"),
    }
}

fn after<'a>(args: &'a [String], flag: &str) -> &'a str {
    let at = args
        .iter()
        .position(|a| a == flag)
        .unwrap_or_else(|| panic!("no {flag}"));
    &args[at + 1]
}

#[test]
fn the_claude_command_holds_the_flags_the_tools_and_the_sandbox_block() {
    let checks = vec!["cargo test".to_string()];
    let command = claude_command(Path::new("/bin/claude"), &plan(Kind::BackLaw, &checks));
    let args = &command.args;
    for flag in ["--restricted", "--safe-mode", "--strict-mcp-config"] {
        assert!(args.iter().any(|a| a == flag), "{flag} is missing");
    }
    assert_eq!(after(args, "--permission-mode"), "dontAsk");
    assert_eq!(after(args, "--permission-prompts"), "none");
    assert_eq!(after(args, "--model"), "sonnet");
    assert_eq!(after(args, "--tools"), "Bash,Read,Edit,Write,Glob,Grep");
    assert_eq!(after(args, "-p"), "Back the law");
    assert_eq!(args.last().map(String::as_str), Some("Back the law"));

    let block: serde_json::Value = serde_json::from_str(after(args, "--settings")).unwrap();
    assert_eq!(
        block,
        serde_json::json!({
            "disableAllHooks": true,
            "sandbox": {
                "enabled": true,
                "failIfUnavailable": true,
                "allowUnsandboxedCommands": false,
                "autoAllowBashIfSandboxed": false,
                "network": { "allowLocalBinding": true },
                "filesystem": { "denyRead": [
                    "/home/d/Library/Application Support/plotkeeper", "~/.claude", "~/.claude.json", "~/.ssh", "~/.aws",
                    "~/.config/gh", "~/.npmrc", "~/.netrc", "~/.cargo/credentials.toml", "~/.gnupg", "~/.kube",
                ] },
            },
        })
    );
    assert_eq!(block, sandbox_settings(plan(Kind::BackLaw, &checks).data));

    // As the writer runs: the login, never an API key, and the hook records nothing.
    assert_eq!(command.program, Path::new("/bin/claude"));
    assert_eq!(
        command.env.get("PLOTKEEPER_INTERNAL").map(String::as_str),
        Some("1")
    );
    for name in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"] {
        assert!(command.env_remove.iter().any(|n| n == name), "{name} stays");
    }
    // The launcher checks the sandbox first, and the run's tools get a temp folder of its own.
    #[cfg(target_os = "macos")]
    {
        use studio_server::runner::SANDBOX_CHECK;
        assert_eq!(command.check, SANDBOX_CHECK.map(String::from));
        // Inside another sandbox only a profile that denies something fails to apply (#395's gate, probe `unavailable`).
        assert!(SANDBOX_CHECK[2].contains("(deny "), "{}", SANDBOX_CHECK[2]);
    }
    // On Linux, bubblewrap's (#704).
    #[cfg(target_os = "linux")]
    assert_eq!(command.check, BWRAP_CHECK.map(String::from));
    let tmp = command.private_tmp.clone().expect("no private temp folder");
    assert_eq!(
        command.env.get("CLAUDE_CODE_TMPDIR").map(String::as_str),
        Some(tmp.to_str().unwrap())
    );
}

#[test]
fn a_runs_temp_folder_is_its_own_and_short_enough_for_claude_code_to_use() {
    let one = private_tmp(Path::new("/runs/r1/work"));
    let two = private_tmp(Path::new("/runs/r2/work"));
    assert_ne!(one, two);
    assert_eq!(one, private_tmp(Path::new("/runs/r1/work")));
    assert!(one.starts_with("/tmp"), "{}", one.display());
    // Claude Code falls back to the shared `/tmp/claude-<uid>` when `<dir>/claude-<uid>` is over 44 bytes.
    let longest = format!("{}/claude-{}", one.display(), u32::MAX);
    assert!(longest.len() <= 44, "{longest} is {} bytes", longest.len());
}

/// The fake harness, with a sandbox check of `check` (a shell script) and, under `tmp`, a private temp folder named in
/// `PK_TMP`.
struct Checked {
    check: String,
    tmp: Option<PathBuf>,
}

impl Harness for Checked {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let mut command = Script.command(plan)?;
        command.check = ["/bin/sh", "-c", &self.check].map(String::from).into();
        if let Some(tmp) = &self.tmp {
            command
                .env
                .insert("PK_TMP".into(), tmp.to_string_lossy().into_owned());
            command.private_tmp = Some(tmp.clone());
        }
        Ok(command)
    }
}

#[test]
fn a_run_whose_sandbox_cannot_start_fails_before_its_harness_runs() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let harness = Checked {
        check: "echo 'sandbox_apply: Operation not permitted' >&2; exit 71".into(),
        tmp: None,
    };
    let (runner, changes) = open(dir.path(), Arc::new(harness));
    let id = submit(&runner, PROJECT, "touch ran");
    until(&runner, &changes, ended(&id));
    let outcome = runner.outcome(&id).unwrap();
    assert_eq!(outcome.end, End::Failed);
    assert!(
        outcome.result.starts_with(SANDBOX_DOWN),
        "{}",
        outcome.result
    );
    assert!(
        outcome.result.contains("sandbox_apply"),
        "{}",
        outcome.result
    );
    assert!(
        !dir.path().join("runs").join(&id).join("work/ran").exists(),
        "the harness ran"
    );
    // A failed run keeps its prompt to copy.
    let view = runner
        .runs(PROJECT)
        .runs
        .into_iter()
        .find(|run| run.id == id)
        .unwrap();
    assert_eq!(view.prompt.as_deref(), Some("touch ran"));

    // On Linux the check is bubblewrap's (#704), run here with stand-in `bwrap` and `socat` on its `PATH`. Each failure
    // names its cause; bubblewrap's are its own words, as the probe in Docker measured them (local evidence
    // `2026-11-27-issue-704-linux-sandbox-probe`): a default container, then one with seccomp and AppArmor unconfined.
    const NO_NAMESPACE: &str = "bwrap: No permissions to create new namespace, likely because the kernel does not allow \
        non-privileged user namespaces. See <https://deb.li/bubblewrap> or \
        <file:///usr/share/doc/bubblewrap/README.Debian.gz>.";
    const NO_PROC: &str = "bwrap: Can't mount proc on /newroot/proc: Operation not permitted";
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let bin = dir.path().join("bin");
    let (runner, changes) = open(dir.path(), Arc::new(Bwrapped(bin.clone())));
    for (bwrap, socat, said) in [
        (
            None,
            true,
            "bubblewrap (bwrap) is not installed: install bubblewrap",
        ),
        (Some(0), false, "socat is not installed: install socat"),
        (Some(1), true, NO_NAMESPACE),
        (Some(1), true, NO_PROC),
    ] {
        stand_ins(&bin, bwrap.map(|exit| (said, exit)), socat);
        let id = submit(&runner, PROJECT, "touch ran");
        until(&runner, &changes, ended(&id));
        let outcome = runner.outcome(&id).unwrap();
        assert_eq!(outcome.end, End::Failed, "{said}");
        assert_eq!(outcome.result, format!("{SANDBOX_DOWN}: {said}"));
        assert!(
            !dir.path().join("runs").join(&id).join("work/ran").exists(),
            "the harness ran: {said}"
        );
    }

    // With both there and a sandbox that starts, the harness runs; bubblewrap was asked for Claude Code's shape, with
    // the sandbox's own network namespace and a fresh /proc.
    stand_ins(&bin, Some(("", 0)), true);
    let id = submit(&runner, PROJECT, "touch ran");
    until(&runner, &changes, ended(&id));
    assert_eq!(runner.outcome(&id).unwrap().end, End::Done);
    assert!(dir.path().join("runs").join(&id).join("work/ran").exists());
    assert_eq!(
        fs::read_to_string(bin.join("bwrap.args")).unwrap().trim(),
        "--new-session --die-with-parent --unshare-net --ro-bind / / --dev /dev --unshare-pid --unshare-user \
         --cap-drop ALL --proc /proc -- /bin/sh -c true"
    );
}

/// The fake harness with the Linux sandbox check, run with `PATH` set to the folder of stand-ins.
struct Bwrapped(PathBuf);

impl Harness for Bwrapped {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let mut command = Script.command(plan)?;
        command.check = ["/usr/bin/env".into(), format!("PATH={}", self.0.display())]
            .into_iter()
            .chain(BWRAP_CHECK.map(String::from))
            .collect();
        Ok(command)
    }
}

/// Fills `bin` with a stand-in `bwrap`, if any, that saves its arguments to `bwrap.args`, prints the text on standard
/// error and exits with the code, and a `socat` if asked.
fn stand_ins(bin: &Path, bwrap: Option<(&str, i32)>, socat: bool) {
    use std::os::unix::fs::PermissionsExt;
    drop(fs::remove_dir_all(bin));
    fs::create_dir_all(bin).unwrap();
    let mut tools = Vec::new();
    if let Some((said, exit)) = bwrap {
        let script = format!(
            "#!/bin/sh\necho \"$*\" > '{}'\nprintf '%s\\n' '{}' >&2\nexit {exit}\n",
            bin.join("bwrap.args").display(),
            said.replace('\'', r"'\''"),
        );
        tools.push(("bwrap", script));
    }
    if socat {
        tools.push(("socat", "#!/bin/sh\nexit 0\n".into()));
    }
    for (name, script) in tools {
        fs::write(bin.join(name), script).unwrap();
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn a_runs_private_temp_folder_is_made_new_and_private_and_removed_after() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let tmp = dir.path().join("pk-tmp");
    let harness = Checked {
        check: "true".into(),
        tmp: Some(tmp.clone()),
    };
    let (runner, changes) = open(dir.path(), Arc::new(harness));
    // The folder's mode, by GNU `stat` on Linux or BSD `stat` on macOS.
    let id = submit(
        &runner,
        PROJECT,
        "stat -c %a \"$PK_TMP\" 2>/dev/null || stat -f %Lp \"$PK_TMP\"",
    );
    until(&runner, &changes, ended(&id));
    assert_eq!(runner.outcome(&id).unwrap().result, "700");
    assert!(!tmp.exists(), "the temp folder outlived the run");

    // A folder already there is never handed to a run, and never removed by it.
    fs::create_dir(&tmp).unwrap();
    fs::write(tmp.join("theirs"), "").unwrap();
    let id = submit(&runner, PROJECT, "touch \"$PK_TMP/ran\"");
    until(&runner, &changes, ended(&id));
    assert_eq!(runner.outcome(&id).unwrap().end, End::Failed);
    assert!(
        !tmp.join("ran").exists(),
        "the harness ran in a shared folder"
    );
    assert!(
        tmp.join("theirs").exists(),
        "the run removed a folder it did not make"
    );
}

#[test]
fn a_run_whose_check_fails_leaves_a_folder_of_its_temp_folders_name_alone() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let tmp = dir.path().join("pk-tmp");
    fs::create_dir(&tmp).unwrap();
    fs::write(tmp.join("theirs"), "").unwrap();
    let harness = Checked {
        check: "exit 3".into(),
        tmp: Some(tmp.clone()),
    };
    let (runner, changes) = open(dir.path(), Arc::new(harness));
    let id = submit(&runner, PROJECT, "true");
    until(&runner, &changes, ended(&id));
    let outcome = runner.outcome(&id).unwrap();
    assert_eq!(outcome.end, End::Failed);
    // A check that prints nothing still says why.
    assert_eq!(
        outcome.result,
        format!("{SANDBOX_DOWN}: the check exited 3")
    );
    assert!(
        tmp.join("theirs").exists(),
        "the run removed a folder it did not make"
    );
}

#[test]
fn cancel_removes_the_temp_folder_of_the_run_it_kills() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let tmp = dir.path().join("pk-tmp");
    let harness = Checked {
        check: "true".into(),
        tmp: Some(tmp.clone()),
    };
    let (runner, changes) = open(dir.path(), Arc::new(harness));
    let id = submit(&runner, PROJECT, "touch \"$PK_TMP/made\"; sleep 60");
    until(&runner, &changes, |r| {
        state(r, &id) == Some(RunState::Running)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tmp.join("made").exists() {
        assert!(
            Instant::now() < deadline,
            "the run never wrote its temp folder"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    runner.cancel(&id).unwrap();
    assert!(
        !tmp.exists(),
        "a cancelled run's temp folder was left behind"
    );
}

#[test]
fn a_result_that_only_names_the_sandbox_settings_is_no_failure() {
    let bench = bench();
    let id = bench.run(&says(
        r#"{"type":"result","subtype":"success","is_error":false,"result":"The runner reads sandbox_apply: and Sandbox is required from a run's output."}"#,
    ));
    bench.until(ended(&id));
    assert_eq!(bench.runner.outcome(&id).unwrap().end, End::Done);
}

#[test]
fn a_run_that_shows_a_failed_sandbox_is_a_failure_even_at_exit_zero() {
    let bench = bench();
    let id = bench.run(&says(
        r#"{"type":"result","subtype":"success","is_error":false,"result":"Every command failed: sandbox_apply: Operation not permitted"}"#,
    ));
    bench.until(ended(&id));
    let outcome = bench.runner.outcome(&id).unwrap();
    assert_eq!(outcome.end, End::Failed);
    assert!(
        outcome.result.starts_with(SANDBOX_DOWN),
        "{}",
        outcome.result
    );
}

#[test]
fn the_allow_list_is_read_only_git_and_the_checks_and_never_a_push_or_gh() {
    let checks: Vec<String> = [
        "cargo test",
        "npm run check",
        "gh pr checks",
        "git push origin",
        "make verify && git push",
        "npx gh pr list",
    ]
    .map(String::from)
    .into();
    let allowed = allowed_tools(Kind::BackLaw, Path::new("/runs/r1/work"), &checks);
    for entry in [
        "Bash(git status)",
        "Bash(git status *)",
        "Bash(git diff *)",
        "Bash(git log *)",
        "Bash(git add *)",
        "Bash(cargo test)",
        "Bash(cargo test *)",
        "Bash(npm run check)",
        "Bash(npm run check *)",
        "Edit(//runs/r1/work/**)",
    ] {
        assert!(
            allowed.iter().any(|a| a == entry),
            "{entry} is missing from {allowed:?}"
        );
    }
    assert!(
        !allowed
            .iter()
            .any(|a| a.contains("push") || a.contains("gh ") || a.contains("commit")),
        "{allowed:?}"
    );
    // The built command carries the same list.
    let command = claude_command(Path::new("/bin/claude"), &plan(Kind::BackLaw, &checks));
    let at = command
        .args
        .iter()
        .position(|a| a == "--allowedTools")
        .unwrap();
    let listed: Vec<&String> = command.args[at + 1..]
        .iter()
        .take_while(|a| !a.starts_with("--"))
        .collect();
    assert_eq!(listed, allowed.iter().collect::<Vec<_>>());
    assert!(
        !command
            .args
            .iter()
            .any(|a| a.contains("push") || a.contains("gh "))
    );
}

#[test]
fn a_kind_with_no_shell_reads_and_writes_only_where_it_may() {
    let checks = vec!["cargo test".to_string()];
    // The kind, its tools, its allow list and its limit in minutes.
    let rows: [(Kind, &str, &[&str], u64); 2] = [
        // The working folder holds the project's files to read; the agent writes only into its output folder there (#399).
        (
            Kind::MicroWorld,
            "Read,Edit,Write,Glob,Grep",
            &["Edit(//runs/r1/work/.micro-world/**)"],
            10,
        ),
        // A first map's writer or reviewer only reads, and answers in its result (#753).
        (Kind::FirstMap, "Read,Glob,Grep", &[], 10),
    ];
    for (kind, tools, allows, minutes) in rows {
        let command = claude_command(Path::new("/bin/claude"), &plan(kind, &checks));
        assert_eq!(after(&command.args, "--tools"), tools, "{kind:?}");
        assert!(!command.args.iter().any(|a| a == "--add-dir"), "{kind:?}");
        let allowed = allowed_tools(kind, Path::new("/runs/r1/work"), &checks);
        assert_eq!(allowed, allows, "{kind:?}");
        // An empty list is no flag at all.
        assert_eq!(
            command.args.iter().any(|a| a == "--allowedTools"),
            !allows.is_empty(),
            "{kind:?}"
        );
        assert_eq!(
            Limits::of(kind).timeout,
            Duration::from_secs(minutes * 60),
            "{kind:?}"
        );
    }
    assert_eq!(
        Limits::of(Kind::BackLaw).timeout,
        Duration::from_secs(30 * 60)
    );
}

#[test]
fn a_run_ends_with_its_result_cost_and_exit_and_appends_a_record() {
    let bench = bench();
    let id = bench.run(&says(
        r#"{"type":"result","subtype":"success","is_error":false,"result":"The law is backed.","total_cost_usd":0.42,"permission_denials":[]}"#,
    ));
    bench.until(ended(&id));
    let outcome = bench.runner.outcome(&id).unwrap();
    assert_eq!(outcome.end, End::Done);
    assert_eq!(outcome.result, "The law is backed.");
    assert_eq!(outcome.cost, Some(0.42));
    assert_eq!(outcome.exit, Some(0));
    assert!(outcome.denials.is_empty());
    // A clean success offers no prompt to copy.
    let view = bench.runner.runs(PROJECT).runs.remove(0);
    assert_eq!((view.outcome, view.prompt), (Some(End::Done), None));

    let records = bench.records();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record["id"], id.as_str());
    assert_eq!(record["kind"], "back-law");
    assert_eq!(record["project"], PROJECT);
    assert_eq!(record["outcome"], "done");
    assert_eq!(record["denials"], serde_json::json!([]));
    assert_eq!(record["cost"], 0.42);
    assert!(
        record["duration_ms"].is_u64() && record["at"].is_u64(),
        "{record}"
    );
}

#[test]
fn denials_in_a_zero_exit_are_reported_never_clean_success() {
    let bench = bench();
    let id = bench.run(&says(
        r#"{"type":"result","subtype":"success","is_error":false,"result":"Done.","total_cost_usd":0.1,"permission_denials":[{"tool_name":"Bash","tool_use_id":"t1","tool_input":{"command":"git push origin HEAD"}},{"tool_name":"Write","tool_use_id":"t2","tool_input":{"file_path":"/etc/hosts"}}]}"#,
    ));
    bench.until(ended(&id));
    let outcome = bench.runner.outcome(&id).unwrap();
    assert_eq!(outcome.exit, Some(0));
    assert_eq!(outcome.end, End::Denied);
    assert_eq!(
        outcome.denials,
        ["Bash: git push origin HEAD", "Write: /etc/hosts"]
    );
    let view = bench.runner.runs(PROJECT).runs.remove(0);
    assert_eq!(view.denials, outcome.denials);
    assert!(
        view.line
            .as_deref()
            .is_some_and(|l| l.contains("2 tool calls were denied")),
        "{:?}",
        view.line
    );
    assert_eq!(bench.records()[0]["outcome"], "denied");
    assert_eq!(
        bench.records()[0]["denials"][0],
        "Bash: git push origin HEAD"
    );
}

#[test]
fn a_failed_run_offers_its_prompt_to_copy() {
    let bench = bench();
    let script = "echo 'the build broke' >&2; exit 3";
    let id = bench.run(script);
    bench.until(ended(&id));
    let outcome = bench.runner.outcome(&id).unwrap();
    assert_eq!((outcome.end, outcome.exit), (End::Failed, Some(3)));
    let view = bench.runner.runs(PROJECT).runs.remove(0);
    assert_eq!(view.prompt.as_deref(), Some(script));
    assert!(
        view.line
            .as_deref()
            .is_some_and(|l| l.contains("the build broke")),
        "{:?}",
        view.line
    );
    assert_eq!(bench.records()[0]["outcome"], "failed");
}

#[test]
fn a_run_that_cannot_start_is_refused_with_the_prompt_to_copy() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let (runner, _changes) = open(dir.path(), Arc::new(Missing));
    let folder = runner.new_folder().unwrap();
    let refused = runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: folder.clone(),
            prompt: "Back the law".into(),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap_err();
    assert!(
        refused.line.contains("`claude` was not found"),
        "{}",
        refused.line
    );
    assert_eq!(refused.prompt, "Back the law");
    assert!(runner.runs(PROJECT).runs.is_empty());
    assert!(!folder.exists(), "a refused run leaves no folder");

    // Only `claude` is a harness.
    settings(
        dir.path(),
        r#"{ "checks": { "/p/app": ["cargo test"] }, "backgroundRuns": { "harness": "codex" } }"#,
    );
    let (runner, _changes) = open(dir.path(), Arc::new(Script));
    let refused = runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: runner.new_folder().unwrap(),
            prompt: "Back the law".into(),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap_err();
    assert!(refused.line.contains("codex"), "{}", refused.line);
}

#[test]
fn checks_and_setup_are_read_fresh_for_each_run_and_a_project_with_none_is_refused() {
    let bench = bench();
    // The fake harness prints the checks its plan held.
    let first = bench.run("echo \"$CHECKS\"");
    bench.until(ended(&first));
    assert_eq!(bench.runner.outcome(&first).unwrap().result, "cargo test");

    settings(
        bench.dir.path(),
        "{ // edited while plotkeeper runs\n \"checks\": { \"/p/app\": [\"make verify\", \"npm run check\"], },\n \"setup\": { \"/p/app\": \"touch setup-ran\" } }",
    );
    let second = bench.run("test -e setup-ran && echo \"$CHECKS\"");
    bench.until(ended(&second));
    let outcome = bench.runner.outcome(&second).unwrap();
    assert_eq!(
        (outcome.end, outcome.result.as_str()),
        (End::Done, "make verify|npm run check")
    );

    // A setup that fails ends the run before the agent starts.
    settings(
        bench.dir.path(),
        r#"{ "checks": { "/p/app": ["make"] }, "setup": { "/p/app": "echo no network >&2; exit 4" } }"#,
    );
    let third = bench.run("touch agent-ran");
    bench.until(ended(&third));
    let outcome = bench.runner.outcome(&third).unwrap();
    assert_eq!(outcome.end, End::SetupFailed);
    assert!(outcome.result.contains("no network"), "{}", outcome.result);
    assert!(!bench.folder(&third).join("work/agent-ran").exists());

    // No `checks` entry: nothing is guessed.
    settings(
        bench.dir.path(),
        r#"{ "checks": { "/p/elsewhere": ["make"] } }"#,
    );
    let request = bench.request(PROJECT, "echo never");
    let folder = request.folder.clone();
    let refused = bench.runner.submit(request).unwrap_err();
    assert_eq!(refused.line, "Set this project's checks");
    assert_eq!(refused.prompt, "echo never");
    assert!(!folder.exists());

    // A first map has no shell, so the checks gate never applies, and it gets no setup.
    settings(
        bench.dir.path(),
        r#"{ "checks": { "/p/elsewhere": ["make"] }, "setup": { "/p/app": "touch setup-ran" } }"#,
    );
    let mut request = bench.request(
        PROJECT,
        "test -e setup-ran && echo setup ran || echo no setup",
    );
    request.kind = Kind::FirstMap;
    let first_map = bench.runner.submit(request).unwrap();
    bench.until(ended(&first_map));
    let outcome = bench.runner.outcome(&first_map).unwrap();
    assert_eq!(
        (outcome.end, outcome.result.as_str()),
        (End::Done, "no setup")
    );
}

#[test]
fn one_run_at_a_time_per_project_and_the_queue_runs_in_order() {
    let bench = bench();
    let first = bench.run(&waits("first"));
    let second = bench.run(&waits("second"));
    let third = bench.run("echo third");
    let other = bench.run_in("/p/other", &waits("other"));
    bench.until(|_| {
        bench.state(&first) == Some(RunState::Running)
            && bench.state(&other) == Some(RunState::Running)
    });
    assert_eq!(bench.state(&second), Some(RunState::Queued));
    assert_eq!(bench.state(&third), Some(RunState::Queued));
    let listed: Vec<String> = bench
        .runner
        .runs(PROJECT)
        .runs
        .into_iter()
        .map(|run| run.id)
        .collect();
    assert_eq!(listed, [first.clone(), second.clone(), third.clone()]);

    fs::write(bench.folder(&first).join("work/go"), "").unwrap();
    bench.until(|_| bench.state(&second) == Some(RunState::Running));
    assert_eq!(bench.state(&first), Some(RunState::Ended));
    assert_eq!(bench.state(&third), Some(RunState::Queued));
    fs::write(bench.folder(&second).join("work/go"), "").unwrap();
    bench.until(ended(&third));
    fs::write(bench.folder(&other).join("work/go"), "").unwrap();
    bench.until(ended(&other));
    assert_eq!(bench.runner.outcome(&second).unwrap().result, "second");
}

#[test]
fn a_batch_starts_together_and_holds_its_projects_slot_until_its_last_run_ends() {
    let bench = bench();
    let before = bench.run(&waits("before"));
    let mut short = bench.request(PROJECT, "sleep 60");
    short.limits.timeout = Duration::from_secs(1);
    let batch = bench
        .runner
        .submit_batch(vec![
            bench.request(PROJECT, &waits("a")),
            bench.request(PROJECT, &waits("b")),
            short,
        ])
        .unwrap();
    let [a, b, short] = <[String; 3]>::try_from(batch).unwrap();
    let later = bench.run("echo later");
    bench.until(|_| bench.state(&before) == Some(RunState::Running));
    for id in [&a, &b, &short, &later] {
        assert_eq!(bench.state(id), Some(RunState::Queued), "{id}");
    }

    // The batch takes the slot when it frees, all of it at once, and the later request waits behind it.
    fs::write(bench.folder(&before).join("work/go"), "").unwrap();
    bench.until(|_| {
        [&a, &b, &short]
            .iter()
            .all(|id| bench.state(id) != Some(RunState::Queued))
    });
    assert_eq!(bench.state(&a), Some(RunState::Running));
    assert_eq!(bench.state(&b), Some(RunState::Running));
    // Each run keeps its own limit.
    bench.until(ended(&short));
    assert_eq!(bench.runner.outcome(&short).unwrap().end, End::TimedOut);
    assert_eq!(bench.state(&later), Some(RunState::Queued));

    fs::write(bench.folder(&a).join("work/go"), "").unwrap();
    bench.until(ended(&a));
    assert_eq!(bench.state(&b), Some(RunState::Running));
    assert_eq!(bench.state(&later), Some(RunState::Queued));

    // The slot frees when the last of them ends.
    fs::write(bench.folder(&b).join("work/go"), "").unwrap();
    bench.until(ended(&later));
    for (id, said) in [(&a, "a"), (&b, "b"), (&later, "later")] {
        assert_eq!(bench.runner.outcome(id).unwrap().result, said);
    }
    // Each has its own folder and status.
    for id in [&a, &b, &short] {
        assert!(bench.folder(id).join("status.json").exists(), "{id}");
    }
}

#[test]
fn a_run_that_joins_an_active_batch_starts_at_once() {
    let bench = bench();
    let ids = bench
        .runner
        .submit_batch(vec![bench.request(PROJECT, &waits("writer"))])
        .unwrap();
    let writer = &ids[0];
    let later = bench.run("echo later");
    bench.until(|_| bench.state(writer) == Some(RunState::Running));
    // A reviewer, a retry or a redraft joins the batch ahead of the request that waits behind it.
    let reviewer = bench
        .runner
        .join(writer, bench.request(PROJECT, &waits("reviewer")))
        .unwrap();
    assert_eq!(bench.state(&reviewer), Some(RunState::Running));
    assert_eq!(bench.state(&later), Some(RunState::Queued));
    fs::write(bench.folder(writer).join("work/go"), "").unwrap();
    bench.until(ended(writer));
    assert_eq!(bench.state(&later), Some(RunState::Queued));
    fs::write(bench.folder(&reviewer).join("work/go"), "").unwrap();
    bench.until(ended(&later));
}

#[test]
fn a_run_its_batchs_last_end_joins_starts_before_the_request_waiting_behind_the_batch() {
    let bench = bench();
    let ids = bench
        .runner
        .submit_batch(vec![bench.request(PROJECT, &waits("writer"))])
        .unwrap();
    let writer = ids[0].clone();
    bench.until(|_| bench.state(&writer) == Some(RunState::Running));
    let later = bench.run("echo later");
    // The module told of the writer's end joins its reviewer then, as First map does, before the slot frees.
    let (tell, joined) = mpsc::channel();
    let runner = Arc::downgrade(&bench.runner);
    let reviewer = bench.request(PROJECT, &waits("reviewer"));
    let reviewer = std::sync::Mutex::new(Some(reviewer));
    let batch = writer.clone();
    bench.runner.on_end(move |end| {
        if end.id != batch {
            return;
        }
        let request = reviewer.lock().unwrap().take();
        if let (Some(runner), Some(request)) = (runner.upgrade(), request) {
            drop(tell.send(runner.join(&batch, request).unwrap()));
        }
    });
    fs::write(bench.folder(&writer).join("work/go"), "").unwrap();
    let reviewer = joined.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(bench.state(&reviewer), Some(RunState::Running));
    assert_eq!(bench.state(&later), Some(RunState::Queued));
    fs::write(bench.folder(&reviewer).join("work/go"), "").unwrap();
    bench.until(ended(&later));
}

#[test]
fn a_batch_with_a_request_refused_is_refused_whole_and_leaves_no_folder() {
    let bench = bench();
    let first = bench.request(PROJECT, "echo first");
    let third = bench.request(PROJECT, "echo third");
    let folders = [first.folder.clone(), third.folder.clone()];
    let mut refused = bench.request(PROJECT, "echo refused");
    refused.project = "/p/no-checks".into();
    let refusal = bench
        .runner
        .submit_batch(vec![first, refused, third])
        .unwrap_err();
    assert_eq!(refusal.line, "Set this project's checks");
    assert_eq!(refusal.prompt, "echo refused");
    assert!(bench.runner.runs(PROJECT).runs.is_empty());
    for folder in folders {
        assert!(!folder.exists(), "{}", folder.display());
    }
    // Nothing of it is picked up at the next start.
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench;
    drop((runner, changes));
    let (runner, _changes) = open(dir.path(), Arc::new(Script));
    assert!(runner.runs(PROJECT).runs.is_empty());
    assert_eq!(fs::read_dir(dir.path().join("runs")).unwrap().count(), 0);
}

#[test]
fn discarding_a_batch_cancels_every_run_of_it_and_frees_the_slot() {
    let bench = bench();
    // A module that listens is told each end: a cancelled run's as `Cancelled`, a run that ended on its own as it ended, and once.
    let (tell, told) = mpsc::channel();
    bench
        .runner
        .on_end(move |end| drop(tell.send((end.id, end.end))));
    // Queued behind another run: its runs are dropped and never start.
    let before = bench.run(&waits("before"));
    let queued = bench
        .runner
        .submit_batch(vec![
            bench.request(PROJECT, "touch started"),
            bench.request(PROJECT, "touch started"),
        ])
        .unwrap();
    bench.until(|_| bench.state(&before) == Some(RunState::Running));
    bench.runner.cancel_batch(&queued[0]).unwrap();
    for id in &queued {
        assert_eq!(bench.state(id), None, "{id}");
        assert!(!bench.folder(id).exists(), "{id}");
    }
    assert_eq!(bench.state(&before), Some(RunState::Running));
    fs::write(bench.folder(&before).join("work/go"), "").unwrap();
    bench.until(ended(&before));

    // Running: its running runs are stopped and its ended ones removed, and the request behind it starts.
    let batch = bench
        .runner
        .submit_batch(vec![
            bench.request(PROJECT, &waits("never")),
            bench.request(PROJECT, "echo quick"),
        ])
        .unwrap();
    let later = bench.run("echo later");
    bench.until(|_| {
        bench.state(&batch[0]) == Some(RunState::Running)
            && bench.state(&batch[1]) == Some(RunState::Ended)
    });
    let pid = launch(&bench.folder(&batch[0]))["pid"].as_u64().unwrap();
    assert_eq!(bench.state(&later), Some(RunState::Queued));
    bench.runner.cancel_batch(&batch[0]).unwrap();
    for id in &batch {
        assert_eq!(bench.state(id), None, "{id}");
        assert!(!bench.folder(id).exists(), "{id}");
    }
    assert!(gone(pid), "a running run of the batch outlived its discard");
    bench.until(ended(&later));
    let cancelled = bench
        .records()
        .iter()
        .filter(|record| record["outcome"] == "cancelled")
        .count();
    assert_eq!(cancelled, 3);
    let mut ends: Vec<(String, End)> = (0..6)
        .map(|_| told.recv_timeout(Duration::from_secs(30)).unwrap())
        .collect();
    ends.sort_by(|a, b| a.0.cmp(&b.0));
    let mut want = vec![
        (before, End::Done),
        (queued[0].clone(), End::Cancelled),
        (queued[1].clone(), End::Cancelled),
        (batch[0].clone(), End::Cancelled),
        (batch[1].clone(), End::Done),
        (later, End::Done),
    ];
    want.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(ends, want);
    assert!(told.recv_timeout(Duration::from_millis(300)).is_err());
}

/// The script of a harness that fails on a usage limit.
fn limited() -> String {
    format!(
        "{}\nexit 1",
        says(
            r#"{"type":"result","subtype":"success","is_error":true,"result":"Claude usage limit reached. Your limit will reset at 5pm.","total_cost_usd":0}"#
        )
    )
}

#[test]
fn a_limit_failure_mid_batch_pauses_the_queues_and_keeps_the_batchs_ended_runs() {
    let bench = bench();
    let batch = bench
        .runner
        .submit_batch(vec![
            bench.request(PROJECT, "echo landed"),
            bench.request(PROJECT, &limited()),
            bench.request(PROJECT, &waits("still running")),
        ])
        .unwrap();
    let [landed, failed, running] = <[String; 3]>::try_from(batch).unwrap();
    bench.until(|r| ended(&landed)(r) && ended(&failed)(r));
    assert!(bench.runner.runs(PROJECT).paused.is_some());
    assert_eq!(bench.runner.outcome(&landed).unwrap().result, "landed");
    assert_eq!(bench.runner.outcome(&failed).unwrap().end, End::RateLimited);
    let mut outcomes: Vec<String> = bench
        .records()
        .iter()
        .map(|record| record["outcome"].as_str().unwrap().to_string())
        .collect();
    outcomes.sort();
    assert_eq!(outcomes, ["done", "rate-limited"]);
    // A run already going goes on; a retry waits for Resume, as the request behind the batch does.
    assert_eq!(bench.state(&running), Some(RunState::Running));
    let retry = bench
        .runner
        .join(&landed, bench.request(PROJECT, "echo retried"))
        .unwrap();
    let later = bench.run("echo later");
    assert_eq!(bench.state(&retry), Some(RunState::Queued));

    // After Resume the retry joins the batch, which still holds the slot.
    bench.runner.resume();
    bench.until(ended(&retry));
    assert_eq!(bench.runner.outcome(&retry).unwrap().result, "retried");
    assert_eq!(bench.state(&later), Some(RunState::Queued));
    fs::write(bench.folder(&running).join("work/go"), "").unwrap();
    bench.until(ended(&later));
}

#[test]
fn a_restart_mid_batch_finds_every_run_of_it_and_leaves_no_slot_held() {
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench();
    let request = |script: &str| Request {
        kind: Kind::BackLaw,
        project: PROJECT.into(),
        folder: runner.new_folder().unwrap(),
        prompt: script.into(),
        limits: Limits::of(Kind::BackLaw),
    };
    let batch = runner
        .submit_batch(vec![
            request(&waits("live")),
            request("sleep 60"),
            request(&waits("ended")),
        ])
        .unwrap();
    let [live, dead, finished] = <[String; 3]>::try_from(batch).unwrap();
    let later = submit(&runner, PROJECT, "echo later");
    until(&runner, &changes, |r| {
        [&live, &dead, &finished]
            .iter()
            .all(|id| state(r, id) == Some(RunState::Running))
    });
    let runs = dir.path().join("runs");
    let pid = launch(&runs.join(&dead))["pid"].as_u64().unwrap();
    // plotkeeper stops; meanwhile one run dies and another ends.
    drop((runner, changes));
    kill_group(pid);
    assert!(gone(pid));
    fs::write(runs.join(&finished).join("work/go"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !runs.join(&finished).join("status.json").exists() {
        assert!(Instant::now() < deadline, "the run never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));

    let (runner, changes) = open(dir.path(), Arc::new(Script));
    assert_eq!(state(&runner, &live), Some(RunState::Running));
    assert_eq!(runner.outcome(&dead).unwrap().end, End::Died);
    assert_eq!(runner.outcome(&finished).unwrap().result, "ended");
    assert_eq!(state(&runner, &later), Some(RunState::Queued));
    // The live run still holds the batch's slot: a retry joins it at once.
    let retry = runner
        .join(&live, request_in(&runner, "echo retried"))
        .unwrap();
    assert_eq!(state(&runner, &retry), Some(RunState::Running));
    until(&runner, &changes, ended(&retry));
    assert_eq!(state(&runner, &later), Some(RunState::Queued));
    // The slot frees when the last live run ends.
    fs::write(runs.join(&live).join("work/go"), "").unwrap();
    until(&runner, &changes, ended(&later));
    assert_eq!(runner.outcome(&live).unwrap().result, "live");
}

/// A request of `PROJECT` on `runner` whose fake harness runs `script`.
fn request_in(runner: &Runner, script: &str) -> Request {
    Request {
        kind: Kind::BackLaw,
        project: PROJECT.into(),
        folder: runner.new_folder().unwrap(),
        prompt: script.into(),
        limits: Limits::of(Kind::BackLaw),
    }
}

#[test]
fn a_bench_dropped_while_a_run_waits_stops_its_launcher() {
    let bench = bench();
    let id = bench.run(&waits("x"));
    let folder = bench.folder(&id);
    let deadline = Instant::now() + Duration::from_secs(30);
    let pid = loop {
        if let Some(pid) = fs::read_to_string(folder.join("launch.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|launch| launch["pid"].as_u64())
        {
            break pid;
        }
        assert!(Instant::now() < deadline, "the launcher never started");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(alive(pid));
    drop(bench);
    assert!(gone(pid), "the launcher outlived its bench");
}

/// The pids of the launchers whose command line names a folder under `dir`.
fn launchers_under(dir: &Path) -> Vec<String> {
    let out = std::process::Command::new("ps")
        .args(["-Ao", "pid=,command="])
        .output()
        .unwrap();
    let launcher = env!("CARGO_BIN_EXE_plotkeeper-run");
    let dir = dir.display().to_string();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| line.contains(launcher) && line.contains(&dir))
        .map(|line| line.trim().to_string())
        .collect()
}

#[test]
fn a_bench_dropped_while_a_run_waits_behind_another_stops_both_launchers() {
    let bench = bench();
    let first = bench.run(&waits("first"));
    let second = bench.run(&waits("second"));
    bench.until(|_| bench.state(&first) == Some(RunState::Running));
    assert_eq!(bench.state(&second), Some(RunState::Queued));
    let dir = bench.dir.path().to_path_buf();
    drop(bench);
    // A launcher started as the guard stopped the first would keep polling for its limit; give it time to show.
    std::thread::sleep(Duration::from_millis(500));
    let left = launchers_under(&dir);
    for line in &left {
        if let Some(pid) = line
            .split_whitespace()
            .next()
            .and_then(|p| p.parse::<i32>().ok())
        {
            // SAFETY: a launcher this test's bench started, which leads its own group.
            unsafe { libc::killpg(pid, libc::SIGKILL) };
        }
    }
    assert!(left.is_empty(), "launchers outlived their bench: {left:?}");
}

#[test]
fn a_run_survives_a_restart_and_is_picked_up() {
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench();
    let folder = runner.new_folder().unwrap();
    let id = runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: folder.clone(),
            prompt: waits("picked up"),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap();
    let queued = runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: runner.new_folder().unwrap(),
            prompt: "echo queued ran".into(),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap();
    until(&runner, &changes, |r| {
        state(r, &id) == Some(RunState::Running)
    });
    // plotkeeper stops: its runner is gone, the run goes on.
    drop((runner, changes));

    let (runner, changes) = open(dir.path(), Arc::new(Script));
    assert_eq!(state(&runner, &id), Some(RunState::Running));
    assert_eq!(state(&runner, &queued), Some(RunState::Queued));
    fs::write(folder.join("go"), "").unwrap();
    until(&runner, &changes, ended(&queued));
    assert_eq!(runner.outcome(&id).unwrap().result, "picked up");
    assert_eq!(runner.outcome(&queued).unwrap().result, "queued ran");

    // The next start removes the ended runs' folders: nothing waits on them.
    drop((runner, changes));
    let (runner, _changes) = open(dir.path(), Arc::new(Script));
    assert!(runner.runs(PROJECT).runs.is_empty());
    assert_eq!(fs::read_dir(dir.path().join("runs")).unwrap().count(), 0);
}

#[test]
fn a_dead_run_is_failed_and_frees_the_queue() {
    let bench = bench_with(Arc::new(WithTmp));
    let dead = bench.run("sleep 60");
    let next = bench.run("echo next ran");
    bench.until(|_| bench.state(&dead) == Some(RunState::Running));
    let tmp = private_tmp(&bench.folder(&dead).join("work"));
    // The launcher makes the folder after its check and setup: wait for it, then a running run keeps it.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tmp.exists() {
        assert!(
            Instant::now() < deadline,
            "the run never made its temp folder"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let pid = launch(&bench.folder(&dead))["pid"].as_u64().unwrap();
    // The launcher and its child die with no final status.
    kill_group(pid);
    bench.until(ended(&next));
    let outcome = bench.runner.outcome(&dead).unwrap();
    assert_eq!((outcome.end, outcome.exit), (End::Died, None));
    assert_eq!(bench.runner.outcome(&next).unwrap().result, "next ran");
    assert_eq!(bench.records()[0]["outcome"], "died");
    assert!(!tmp.exists(), "a dead run's temp folder was left behind");
}

#[test]
fn a_run_found_dead_at_start_is_failed_and_frees_the_queue() {
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench_with(Arc::new(WithTmp));
    let submit = |runner: &Runner, script: &str| {
        runner
            .submit(Request {
                kind: Kind::BackLaw,
                project: PROJECT.into(),
                folder: runner.new_folder().unwrap(),
                prompt: script.into(),
                limits: Limits::of(Kind::BackLaw),
            })
            .unwrap()
    };
    let dead = submit(&runner, "sleep 60");
    let next = submit(&runner, "echo next ran");
    until(&runner, &changes, |r| {
        state(r, &dead) == Some(RunState::Running)
    });
    let pid = launch(&dir.path().join("runs").join(&dead))["pid"]
        .as_u64()
        .unwrap();
    let tmp = private_tmp(&dir.path().join("runs").join(&dead).join("work"));
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tmp.exists() {
        assert!(
            Instant::now() < deadline,
            "the run never made its temp folder"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    drop((runner, changes));
    kill_group(pid);
    assert!(gone(pid));

    let (runner, changes) = open(dir.path(), Arc::new(WithTmp));
    until(&runner, &changes, ended(&next));
    assert_eq!(runner.outcome(&dead).unwrap().end, End::Died);
    assert_eq!(runner.outcome(&next).unwrap().result, "next ran");
    assert!(!tmp.exists(), "a dead run's temp folder was left behind");
}

#[test]
fn cancel_kills_the_group_and_removes_the_folder() {
    let bench = bench();
    let id = bench.run("sleep 60 & echo $! > child.pid; wait");
    let queued = bench.run("echo after the cancel");
    bench.until(|_| bench.state(&id) == Some(RunState::Running));
    let folder = bench.folder(&id);
    let deadline = Instant::now() + Duration::from_secs(10);
    let child = loop {
        match fs::read_to_string(folder.join("work/child.pid"))
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
        {
            Some(pid) => break pid,
            None => assert!(
                Instant::now() < deadline,
                "the fake harness never wrote its child's pid"
            ),
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let launcher = launch(&folder)["pid"].as_u64().unwrap();
    assert!(alive(child) && alive(launcher));

    bench.runner.cancel(&id).unwrap();
    assert!(!folder.exists(), "Cancel removes the run's folder");
    assert_eq!(bench.state(&id), None);
    assert!(gone(launcher), "the launcher outlived Cancel");
    assert!(gone(child), "a child of the run outlived Cancel");
    assert_eq!(bench.records()[0]["outcome"], "cancelled");
    // The queue moves on.
    bench.until(ended(&queued));

    // A queued run is cancelled before it ever starts.
    let running = bench.run(&waits("x"));
    let waiting = bench.run("touch started");
    bench.until(|_| bench.state(&running) == Some(RunState::Running));
    bench.runner.cancel(&waiting).unwrap();
    assert!(!bench.folder(&waiting).exists());
    assert_eq!(bench.state(&waiting), None);
    bench.runner.cancel(&running).unwrap();
}

#[test]
fn cancel_never_touches_a_process_whose_start_time_differs() {
    let bench = bench();
    let id = bench.run("sleep 60");
    bench.until(|_| bench.state(&id) == Some(RunState::Running));
    let folder = bench.folder(&id);
    let recorded = launch(&folder);
    let pid = recorded["pid"].as_u64().unwrap();
    // The folder names this pid with another start time, as when the id was reused by a later process.
    let mut other = recorded.clone();
    other["start"] = "Thu Jan  1 00:00:00 1970".into();
    fs::write(folder.join("launch.json"), other.to_string()).unwrap();

    let refused = bench.runner.cancel(&id).unwrap_err();
    assert!(refused.contains("nothing was stopped"), "{refused}");
    assert!(alive(pid), "Cancel killed a process it did not start");
    assert!(folder.exists());
    assert_eq!(bench.state(&id), Some(RunState::Running));

    fs::write(folder.join("launch.json"), recorded.to_string()).unwrap();
    bench.runner.cancel(&id).unwrap();
    assert!(gone(pid));
}

#[test]
fn an_auth_or_rate_limit_failure_pauses_the_queue_until_resumed() {
    for (said, end) in [
        ("Invalid API key · Please run /login", End::NotLoggedIn),
        (
            "Claude usage limit reached. Your limit will reset at 5pm.",
            End::RateLimited,
        ),
    ] {
        let bench = bench();
        let failed = bench.run(&format!(
            "{}\nexit 1",
            says(&format!(r#"{{"type":"result","subtype":"success","is_error":true,"result":"{said}","total_cost_usd":0}}"#))
        ));
        let waiting = bench.run("echo resumed");
        bench.until(ended(&failed));
        assert_eq!(bench.runner.outcome(&failed).unwrap().end, end);
        assert!(bench.runner.runs(PROJECT).paused.is_some());
        // One indicator, whatever the project; nothing starts while it shows.
        assert_eq!(
            bench.runner.runs("/p/other").paused,
            bench.runner.runs(PROJECT).paused
        );
        let other = bench.run_in("/p/other", "echo other");
        assert_eq!(bench.state(&waiting), Some(RunState::Queued));
        assert_eq!(bench.state(&other), Some(RunState::Queued));

        // The pause outlives a restart.
        let Bench {
            dir,
            // Before `runner`, so it drops after it and before `dir`.
            guard: _guard,
            runner,
            changes,
        } = bench;
        drop((runner, changes));
        let (runner, changes) = open(dir.path(), Arc::new(Script));
        assert!(runner.runs(PROJECT).paused.is_some());
        assert_eq!(state(&runner, &waiting), Some(RunState::Queued));

        runner.resume();
        assert_eq!(runner.runs(PROJECT).paused, None);
        until(&runner, &changes, |r| {
            ended(&waiting)(r) && ended(&other)(r)
        });
        assert_eq!(runner.outcome(&waiting).unwrap().result, "resumed");
    }
}

#[test]
fn a_result_that_only_mentions_a_rate_limit_pauses_nothing() {
    let bench = bench();
    let id = bench.run(&says(
        r#"{"type":"result","is_error":false,"result":"The test covers the rate limit law."}"#,
    ));
    bench.until(ended(&id));
    assert_eq!(bench.runner.outcome(&id).unwrap().end, End::Done);
    assert_eq!(bench.runner.runs(PROJECT).paused, None);
}

#[test]
fn a_run_past_its_limit_is_stopped() {
    let bench = bench();
    let mut request = bench.request(PROJECT, "sleep 60 & echo $! > child.pid; wait");
    request.limits.timeout = Duration::from_secs(1);
    let folder = request.folder.clone();
    let id = bench.runner.submit(request).unwrap();
    bench.until(ended(&id));
    let outcome = bench.runner.outcome(&id).unwrap();
    assert_eq!((outcome.end, outcome.exit), (End::TimedOut, None));
    let child: u64 = fs::read_to_string(folder.join("child.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(gone(child), "the run's child outlived its limit");
    assert_eq!(bench.records()[0]["outcome"], "timed-out");
}

/// The pid a script wrote to `file`, waited for.
fn pid_in(file: &Path) -> u64 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(pid) = fs::read_to_string(file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "nothing wrote {}",
            file.display()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A script that starts a helper, writes its pid to `file`, and sleeps.
fn hangs(file: &Path) -> String {
    format!("sleep 60 & echo $! > '{}'; sleep 60", file.display())
}

#[test]
fn a_check_or_setup_past_the_runs_limit_leaves_no_process_behind() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    let check_pid = dir.path().join("check.pid");
    let harness = Checked {
        check: hangs(&check_pid),
        tmp: None,
    };
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let (runner, changes) = open(dir.path(), Arc::new(harness));
    let mut request = Request {
        kind: Kind::BackLaw,
        project: PROJECT.into(),
        folder: runner.new_folder().unwrap(),
        prompt: "touch ran".into(),
        limits: Limits::of(Kind::BackLaw),
    };
    request.limits.timeout = Duration::from_secs(1);
    let id = runner.submit(request).unwrap();
    until(&runner, &changes, ended(&id));
    assert_eq!(runner.outcome(&id).unwrap().end, End::TimedOut);
    assert!(
        gone(pid_in(&check_pid)),
        "the check's helper outlived the limit"
    );

    let bench = bench();
    let setup_pid = bench.dir.path().join("setup.pid");
    settings(
        bench.dir.path(),
        &serde_json::json!({ "checks": { PROJECT: ["make"] }, "setup": { PROJECT: hangs(&setup_pid) } })
            .to_string(),
    );
    let mut request = bench.request(PROJECT, "touch ran");
    request.limits.timeout = Duration::from_secs(1);
    let folder = request.folder.clone();
    let id = bench.runner.submit(request).unwrap();
    bench.until(ended(&id));
    assert_eq!(bench.runner.outcome(&id).unwrap().end, End::TimedOut);
    assert!(
        gone(pid_in(&setup_pid)),
        "the setup's helper outlived the limit"
    );
    assert!(
        !folder.join("ran").exists(),
        "the harness ran after a setup that timed out"
    );
}

#[test]
fn cancel_ends_a_running_setup_and_what_it_started() {
    let bench = bench();
    let setup_pid = bench.dir.path().join("setup.pid");
    settings(
        bench.dir.path(),
        &serde_json::json!({ "checks": { PROJECT: ["make"] }, "setup": { PROJECT: hangs(&setup_pid) } })
            .to_string(),
    );
    let id = bench.run("touch ran");
    let helper = pid_in(&setup_pid);
    let launcher = launch(&bench.folder(&id))["pid"].as_u64().unwrap();

    bench.runner.cancel(&id).unwrap();
    assert!(
        !bench.folder(&id).exists(),
        "Cancel removes the run's folder"
    );
    assert!(gone(launcher), "the launcher outlived Cancel");
    assert!(gone(helper), "the setup's helper outlived Cancel");
}

#[test]
fn a_runs_builds_get_a_third_of_the_cores() {
    let bench = bench();
    let id = bench.run("echo \"$CARGO_BUILD_JOBS $NEXTEST_TEST_THREADS\"");
    bench.until(ended(&id));
    let third = (std::thread::available_parallelism().unwrap().get() / 3).max(1);
    assert_eq!(
        bench.runner.outcome(&id).unwrap().result,
        format!("{third} {third}")
    );
}

#[test]
fn a_folder_with_no_run_is_removed_at_start() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), "{}");
    let (runner, _changes) = open(dir.path(), Arc::new(Script));
    // A working folder was made and plotkeeper stopped before its run was submitted.
    let orphan = runner.new_folder().unwrap();
    assert!(orphan.starts_with(dir.path().join("runs")) && orphan.ends_with("work"));
    drop(runner);
    let (_runner, _changes) = open(dir.path(), Arc::new(Script));
    assert!(!orphan.exists());
}

struct NoEffects;

impl studio_understanding::Effects for NoEffects {
    fn list(&self) {}
    fn name(&self, _: &str, _: &str) {}
    fn log(&self, _: String) {}
}

struct NoPage;

impl studio_server::api::Page for NoPage {
    fn focus(&self, _: bool) {}
}

#[test]
fn the_pages_commands_cancel_a_run_and_resume_the_queues() {
    use studio_server::{App, api};
    use studio_understanding::{CardStore, FakeWriter, RiskNotes};
    isolated_home::set();
    let bench = bench();
    let log = bench.dir.path().join("data/events.jsonl");
    fs::write(&log, "").unwrap();
    let cards = Arc::new(std::sync::Mutex::new(CardStore::open_beside(&log).unwrap()));
    let notes = Arc::new(std::sync::Mutex::new(RiskNotes::open_beside(&log).unwrap()));
    let mut app = App::open_with(
        &log,
        cards,
        notes,
        Arc::new(FakeWriter),
        Arc::new(NoEffects),
    )
    .unwrap();
    app.runner = bench.runner.clone();
    let app = Arc::new(app);
    let call = |name: &str, args: serde_json::Value| api::call(&app, &NoPage, name, args);

    let id = bench.run("sleep 60");
    bench.until(|_| bench.state(&id) == Some(RunState::Running));
    assert_eq!(
        call("run_cancel", serde_json::json!({ "id": id })).unwrap(),
        serde_json::Value::Null
    );
    assert!(!bench.folder(&id).exists());
    let missing = call("run_cancel", serde_json::json!({ "id": id })).unwrap_err();
    assert!(matches!(missing, api::Failure::Failed(why) if why == "no such run"));

    let failed = bench.run(&format!(
        "{}\nexit 1",
        says(r#"{"is_error":true,"result":"Please run /login"}"#)
    ));
    bench.until(ended(&failed));
    assert!(bench.runner.runs(PROJECT).paused.is_some());
    assert_eq!(
        call("runs_resume", serde_json::Value::Null).unwrap(),
        serde_json::Value::Null
    );
    assert_eq!(bench.runner.runs(PROJECT).paused, None);
}

#[test]
fn submitting_a_runs_folder_twice_is_refused_and_leaves_the_run_alone() {
    let bench = bench();
    let first = bench.request(PROJECT, &waits("still here"));
    let folder = first.folder.clone();
    let id = bench.runner.submit(first).unwrap();
    bench.until(|_| bench.state(&id) == Some(RunState::Running));
    let again = Request {
        kind: Kind::BackLaw,
        project: PROJECT.into(),
        folder: folder.clone(),
        prompt: "echo twice".into(),
        limits: Limits::of(Kind::BackLaw),
    };
    assert!(bench.runner.submit(again).is_err());
    assert!(
        folder.exists(),
        "the refusal removed a running run's folder"
    );
    fs::write(folder.join("go"), "").unwrap();
    bench.until(ended(&id));
    assert_eq!(bench.runner.outcome(&id).unwrap().result, "still here");

    // A folder outside the runs folder is refused and never removed.
    let outside = bench.dir.path().join("elsewhere/work");
    fs::create_dir_all(&outside).unwrap();
    let stray = Request {
        kind: Kind::BackLaw,
        project: PROJECT.into(),
        folder: outside.clone(),
        prompt: "echo stray".into(),
        limits: Limits::of(Kind::BackLaw),
    };
    assert!(bench.runner.submit(stray).is_err());
    assert!(outside.exists());
}

#[test]
fn a_launcher_that_does_not_start_fails_the_run_once_and_frees_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let open_with = |launcher: &str| {
        let runner = Runner::new(Config {
            launcher: launcher.into(),
            ..config(dir.path(), Arc::new(Script))
        });
        runner.pick_up();
        runner
    };
    let runner = open_with("/no/such/plotkeeper-run");
    // Its end is told as any end is, so the module that asked for the run is not left waiting on it.
    let (tell, told) = mpsc::channel();
    runner.on_end(move |end| drop(tell.send((end.id, end.end))));
    let submit = |script: &str| {
        runner
            .submit(Request {
                kind: Kind::BackLaw,
                project: PROJECT.into(),
                folder: runner.new_folder().unwrap(),
                prompt: script.into(),
                limits: Limits::of(Kind::BackLaw),
            })
            .unwrap()
    };
    let (first, second) = (submit("echo one"), submit("echo two"));
    for id in [&first, &second] {
        let outcome = runner.outcome(id).unwrap();
        assert_eq!(outcome.end, End::Failed);
        assert!(
            outcome.result.contains("did not start"),
            "{}",
            outcome.result
        );
    }
    assert_eq!(
        runner.runs(PROJECT).runs[0].prompt.as_deref(),
        Some("echo one")
    );
    let records = || fs::read_to_string(dir.path().join("data/background-runs.jsonl")).unwrap();
    assert_eq!(records().lines().count(), 2);
    for id in [&first, &second] {
        let end = told.recv_timeout(Duration::from_secs(30));
        assert_eq!(end, Ok((id.clone(), End::Failed)));
    }
    assert!(told.recv_timeout(Duration::from_millis(300)).is_err());

    // The next start records nothing again and removes what is left.
    drop(runner);
    let runner = open_with(env!("CARGO_BIN_EXE_plotkeeper-run"));
    assert!(runner.runs(PROJECT).runs.is_empty());
    assert_eq!(records().lines().count(), 2);
}

/// Submits `script` for `project` on `runner`.
fn submit(runner: &Runner, project: &str, script: &str) -> String {
    runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: project.into(),
            folder: runner.new_folder().unwrap(),
            prompt: script.into(),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap()
}

/// Kills the process group `pid` leads with SIGKILL. By `killpg`, not `kill -9 -<pid>`: Linux's procps `kill` reads
/// `-<pid>` there as an option, where macOS's takes it as the group.
fn kill_group(pid: u64) {
    let group = libc::pid_t::try_from(pid).unwrap();
    // SAFETY: killpg takes plain integers and touches no memory of this process.
    let killed = unsafe { libc::killpg(group, libc::SIGKILL) };
    assert_eq!(killed, 0, "{}", std::io::Error::last_os_error());
}

#[test]
fn a_start_leaves_a_running_run_it_cannot_read_alone() {
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench();
    let id = submit(&runner, PROJECT, "sleep 60");
    until(&runner, &changes, |r| {
        state(r, &id) == Some(RunState::Running)
    });
    let folder = dir.path().join("runs").join(&id);
    let pid = launch(&folder)["pid"].as_u64().unwrap();
    // As a newer build would write it: a kind this build does not know.
    let run = fs::read_to_string(folder.join("run.json")).unwrap();
    fs::write(
        folder.join("run.json"),
        run.replace("\"back-law\"", "\"future-kind\""),
    )
    .unwrap();
    drop((runner, changes));

    let (_runner, _changes) = open(dir.path(), Arc::new(Script));
    assert!(
        folder.exists(),
        "a start removed the folder of a run that is still running"
    );
    assert!(alive(pid));
    kill_group(pid);
    assert!(gone(pid));
    // Once its launcher is gone it is an orphan like any other.
    let (_runner, _changes) = open(dir.path(), Arc::new(Script));
    assert!(!folder.exists());
}

#[test]
fn a_queued_run_never_starts_beside_a_running_one_whatever_their_order_on_disk() {
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench();
    let running = submit(&runner, PROJECT, &waits("first"));
    let queued = submit(&runner, PROJECT, "echo second");
    until(&runner, &changes, |r| {
        state(r, &running) == Some(RunState::Running)
    });
    drop((runner, changes));
    // The queued run sorts first, as after a clock that stepped back.
    let file = dir.path().join("runs").join(&queued).join("run.json");
    let mut run: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    run["created_ms"] = 0.into();
    fs::write(&file, run.to_string()).unwrap();

    let (runner, changes) = open(dir.path(), Arc::new(Script));
    assert_eq!(state(&runner, &running), Some(RunState::Running));
    assert_eq!(state(&runner, &queued), Some(RunState::Queued));
    fs::write(dir.path().join("runs").join(&running).join("work/go"), "").unwrap();
    until(&runner, &changes, ended(&queued));
}

#[test]
fn a_refusal_never_removes_a_folder_that_is_not_a_new_runs() {
    let bench = bench();
    // `<runs>/../work`: its parent's parent is the runs folder, and it is no run's.
    let runs = bench.dir.path().join("runs");
    fs::create_dir_all(&runs).unwrap();
    let beside = runs.join("..").join("work");
    fs::create_dir_all(&beside).unwrap();
    settings(bench.dir.path(), "{}");
    let refused = bench.runner.submit(Request {
        kind: Kind::BackLaw,
        project: PROJECT.into(),
        folder: beside.clone(),
        prompt: "echo no".into(),
        limits: Limits::of(Kind::BackLaw),
    });
    assert!(refused.is_err());
    assert!(beside.exists() && runs.exists());
    assert!(bench.dir.path().join("data/settings.json").exists());
}

#[test]
fn a_finished_runs_stray_children_are_stopped() {
    let bench = bench();
    let id = bench.run("sleep 60 & echo $! > child.pid; echo finished");
    bench.until(ended(&id));
    assert_eq!(bench.runner.outcome(&id).unwrap().result, "finished");
    let child: u64 = fs::read_to_string(bench.folder(&id).join("work/child.pid"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(gone(child), "a child of a finished run kept running");
}

/// A harness whose command removes `HOME` from the environment.
struct NoHome;

impl Harness for NoHome {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let mut command = Script.command(plan)?;
        command.env_remove = vec!["HOME".into()];
        Ok(command)
    }
}

#[test]
fn the_launcher_closes_standard_input_and_removes_what_the_command_says() {
    assert!(
        std::env::var_os("HOME").is_some(),
        "the test needs HOME set to see it removed"
    );
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(
        dir.path(),
        r#"{ "checks": { "/p/app": ["cargo test"] }, "setup": { "/p/app": "echo \"setup ${HOME:+has HOME}\" > setup-said" } }"#,
    );
    let (runner, changes) = open(dir.path(), Arc::new(NoHome));
    let id = submit(
        &runner,
        PROJECT,
        "[ /dev/stdin -ef /dev/null ] && echo \"stdin closed, HOME ${HOME-unset}, $(cat setup-said)\"",
    );
    until(&runner, &changes, ended(&id));
    // The harness has no HOME and no input; the setup command keeps plotkeeper's own environment.
    assert_eq!(
        runner.outcome(&id).unwrap().result,
        "stdin closed, HOME unset, setup has HOME"
    );
}

#[test]
fn a_result_whose_subtype_is_an_error_is_a_failure_even_at_exit_zero() {
    let bench = bench();
    let id = bench.run(&says(
        r#"{"type":"result","subtype":"error_max_turns","is_error":false,"result":""}"#,
    ));
    bench.until(ended(&id));
    assert_eq!(bench.runner.outcome(&id).unwrap().end, End::Failed);
}

#[test]
fn a_pause_is_told_to_every_project_with_runs_and_a_pause_file_of_another_build_still_pauses() {
    let Bench {
        dir,
        // Before `runner`, so it drops after it and before `dir`.
        guard: _guard,
        runner,
        changes,
    } = bench();
    let other = submit(&runner, "/p/other", &waits("other"));
    until(&runner, &changes, |r| {
        state(r, &other) == Some(RunState::Running)
    });
    while changes.try_recv().is_ok() {}
    let failed = submit(
        &runner,
        PROJECT,
        &format!(
            "{}\nexit 1",
            says(r#"{"is_error":true,"result":"Please run /login"}"#)
        ),
    );
    until(&runner, &changes, ended(&failed));
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut told = Vec::new();
    while !told.contains(&"/p/other".to_string()) {
        let left = deadline.saturating_duration_since(Instant::now());
        told.push(
            changes
                .recv_timeout(left)
                .expect("the other project was never told of the pause"),
        );
    }
    runner.cancel(&other).unwrap();
    drop((runner, changes));

    // A reason this build does not know is still a pause.
    fs::write(
        dir.path().join("runs/paused.json"),
        r#"{"end":"a-future-reason"}"#,
    )
    .unwrap();
    let (runner, _changes) = open(dir.path(), Arc::new(Script));
    assert!(runner.runs(PROJECT).paused.is_some());
}

#[test]
fn cancel_of_a_run_that_ends_at_that_moment_never_fails() {
    let bench = bench();
    // Cancels timed around the end of a short run: whichever wins, Cancel removes the run and says nothing went wrong.
    for round in 0..40u64 {
        let id = bench.run("sleep 0.05");
        // Started by `submit`; a busy machine can see it only once it has ended, which never comes back to running.
        bench.until(|_| matches!(bench.state(&id), Some(RunState::Running | RunState::Ended)));
        std::thread::sleep(Duration::from_millis(35 + round));
        assert_eq!(bench.runner.cancel(&id), Ok(()), "round {round}");
        assert!(!bench.folder(&id).exists());
    }
    // Each run is recorded once, as cancelled or as it ended.
    assert_eq!(bench.records().len(), 40);
}

#[test]
fn every_module_that_listens_is_told_a_runs_end_once_also_at_a_pick_up_and_past_one_that_panics() {
    let dir = tempfile::tempdir().unwrap();
    let _guard = LauncherGuard::new(dir.path().join("runs"));
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    fs::create_dir_all(dir.path().join("data")).unwrap();
    let ends = |runner: &Runner| {
        let (tell, told) = mpsc::channel();
        runner.on_end(move |end| drop(tell.send(end)));
        told
    };
    let runner = Runner::new(config(dir.path(), Arc::new(Script)));
    // A module whose listener panics is logged, and keeps no other from being told.
    runner.on_end(|_| panic!("a listener that panics"));
    // Back a law and First map each listen, and each is told.
    let told = ends(&runner);
    let other = ends(&runner);
    runner.pick_up();
    let folder = runner.new_folder().unwrap();
    let id = runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: folder.clone(),
            prompt: "echo done".into(),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap();
    let end = told.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!(
        (
            end.id.as_str(),
            end.kind,
            end.project.as_str(),
            &end.folder,
            end.end
        ),
        (id.as_str(), Kind::BackLaw, PROJECT, &folder, End::Done)
    );
    assert!(told.recv_timeout(Duration::from_millis(300)).is_err());
    assert_eq!(other.recv_timeout(Duration::from_secs(30)).ok(), Some(end));
    assert!(other.recv_timeout(Duration::from_millis(300)).is_err());
    // Nor does it leave a batch holding its project's slot: the request behind the batch runs.
    let batch = runner
        .submit_batch(vec![Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: runner.new_folder().unwrap(),
            prompt: "echo batch".into(),
            limits: Limits::of(Kind::BackLaw),
        }])
        .unwrap();
    let later = submit(&runner, PROJECT, "echo later");
    for id in [&batch[0], &later] {
        assert_eq!(
            told.recv_timeout(Duration::from_secs(30)).map(|end| end.id),
            Ok(id.clone())
        );
    }

    // A run that ends while plotkeeper is stopped is told at the next start, when it is picked up.
    let waiting = runner.new_folder().unwrap();
    let id = runner
        .submit(Request {
            kind: Kind::BackLaw,
            project: PROJECT.into(),
            folder: waiting.clone(),
            prompt: waits("later"),
            limits: Limits::of(Kind::BackLaw),
        })
        .unwrap();
    drop((runner, told, other));
    fs::write(waiting.join("go"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !dir
        .path()
        .join("runs")
        .join(&id)
        .join("status.json")
        .exists()
    {
        assert!(Instant::now() < deadline, "the run never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));
    let runner = Runner::new(config(dir.path(), Arc::new(Script)));
    let told = ends(&runner);
    let other = ends(&runner);
    runner.pick_up();
    let end = told.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!((end.id.as_str(), end.end), (id.as_str(), End::Done));
    assert_eq!(other.recv_timeout(Duration::from_secs(30)).ok(), Some(end));
}
