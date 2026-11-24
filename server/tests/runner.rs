//! The background runner (spec danielrispler/plotkeeper#378, ticket #390) with a fake harness that is a real child process: a
//! run's prompt is the shell script the launcher runs, so a run sleeps, prints a result or dies as the test says. No test
//! runs `claude`: its command is built and read, never started.

mod isolated_home;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use studio_server::runner::{
    Config, End, Harness, HarnessCommand, Kind, Limits, Plan, Request, RunState, Runner,
    SANDBOX_CHECK, SANDBOX_DOWN, allowed_tools, claude_command, private_tmp, sandbox_settings,
};

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

/// A harness that is not installed.
struct Missing;

impl Harness for Missing {
    fn command(&self, _: &Plan) -> Result<HarnessCommand, String> {
        Err("`claude` was not found".into())
    }
}

struct Bench {
    dir: tempfile::TempDir,
    runner: Arc<Runner>,
    changes: mpsc::Receiver<String>,
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
    let dir = tempfile::tempdir().unwrap();
    settings(
        dir.path(),
        r#"{ "checks": { "/p/app": ["cargo test"], "/p/other": ["make"] } }"#,
    );
    let (runner, changes) = open(dir.path(), Arc::new(Script));
    Bench {
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
        until(&self.runner, &self.changes, what)
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
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .unwrap()
        .status
        .success()
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
    assert_eq!(command.check, SANDBOX_CHECK.map(String::from));
    // Inside another sandbox only a profile that denies something fails to apply (#395's gate, probe `unavailable`).
    assert!(SANDBOX_CHECK[2].contains("(deny "), "{}", SANDBOX_CHECK[2]);
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
    check: &'static str,
    tmp: Option<PathBuf>,
}

impl Harness for Checked {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let mut command = Script.command(plan)?;
        command.check = ["/bin/sh", "-c", self.check].map(String::from).into();
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
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let harness = Checked {
        check: "echo 'sandbox_apply: Operation not permitted' >&2; exit 71",
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
}

#[test]
fn a_runs_private_temp_folder_is_made_new_and_private_and_removed_after() {
    let dir = tempfile::tempdir().unwrap();
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let tmp = dir.path().join("pk-tmp");
    let harness = Checked {
        check: "true",
        tmp: Some(tmp.clone()),
    };
    let (runner, changes) = open(dir.path(), Arc::new(harness));
    let id = submit(&runner, PROJECT, "stat -f %Lp \"$PK_TMP\"");
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
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let tmp = dir.path().join("pk-tmp");
    fs::create_dir(&tmp).unwrap();
    fs::write(tmp.join("theirs"), "").unwrap();
    let harness = Checked {
        check: "exit 3",
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
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    let tmp = dir.path().join("pk-tmp");
    let harness = Checked {
        check: "true",
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
fn a_micro_world_has_no_bash_and_writes_only_its_own_folder() {
    let checks = vec!["cargo test".to_string()];
    let command = claude_command(Path::new("/bin/claude"), &plan(Kind::MicroWorld, &checks));
    assert_eq!(after(&command.args, "--tools"), "Read,Edit,Write,Glob,Grep");
    // Only the flags spec #378 gives: how a micro-world reads the project is its own ticket's.
    assert!(!command.args.iter().any(|a| a == "--add-dir"));
    let allowed = allowed_tools(Kind::MicroWorld, Path::new("/runs/r1/work"), &checks);
    assert_eq!(allowed, ["Edit(//runs/r1/work/**)"]);
    assert_eq!(
        Limits::of(Kind::MicroWorld).timeout,
        Duration::from_secs(10 * 60)
    );
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
fn a_run_survives_a_restart_and_is_picked_up() {
    let Bench {
        dir,
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
    let bench = bench();
    let dead = bench.run("sleep 60");
    let next = bench.run("echo next ran");
    bench.until(|_| bench.state(&dead) == Some(RunState::Running));
    let pid = launch(&bench.folder(&dead))["pid"].as_u64().unwrap();
    // The launcher and its child die with no final status.
    assert!(
        Command::new("kill")
            .args(["-9", &format!("-{pid}")])
            .status()
            .unwrap()
            .success()
    );
    bench.until(ended(&next));
    let outcome = bench.runner.outcome(&dead).unwrap();
    assert_eq!((outcome.end, outcome.exit), (End::Died, None));
    assert_eq!(bench.runner.outcome(&next).unwrap().result, "next ran");
    assert_eq!(bench.records()[0]["outcome"], "died");
}

#[test]
fn a_run_found_dead_at_start_is_failed_and_frees_the_queue() {
    let Bench {
        dir,
        runner,
        changes,
    } = bench();
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
    drop((runner, changes));
    assert!(
        Command::new("kill")
            .args(["-9", &format!("-{pid}")])
            .status()
            .unwrap()
            .success()
    );
    assert!(gone(pid));

    let (runner, changes) = open(dir.path(), Arc::new(Script));
    until(&runner, &changes, ended(&next));
    assert_eq!(runner.outcome(&dead).unwrap().end, End::Died);
    assert_eq!(runner.outcome(&next).unwrap().result, "next ran");
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

fn kill_group(pid: u64) {
    assert!(
        Command::new("kill")
            .args(["-9", &format!("-{pid}")])
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn a_start_leaves_a_running_run_it_cannot_read_alone() {
    let Bench {
        dir,
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
        bench.until(|_| bench.state(&id) == Some(RunState::Running));
        std::thread::sleep(Duration::from_millis(35 + round));
        assert_eq!(bench.runner.cancel(&id), Ok(()), "round {round}");
        assert!(!bench.folder(&id).exists());
    }
    // Each run is recorded once, as cancelled or as it ended.
    assert_eq!(bench.records().len(), 40);
}

#[test]
fn the_module_that_asked_for_a_run_is_told_its_end_once_also_at_a_pick_up() {
    let dir = tempfile::tempdir().unwrap();
    settings(dir.path(), r#"{ "checks": { "/p/app": ["cargo test"] } }"#);
    fs::create_dir_all(dir.path().join("data")).unwrap();
    let ends = |runner: &Runner| {
        let (tell, told) = mpsc::channel();
        runner.on_end(move |end| drop(tell.send(end)));
        told
    };
    let runner = Runner::new(config(dir.path(), Arc::new(Script)));
    let told = ends(&runner);
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
    drop((runner, told));
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
    runner.pick_up();
    let end = told.recv_timeout(Duration::from_secs(30)).unwrap();
    assert_eq!((end.id.as_str(), end.end), (id.as_str(), End::Done));
}
