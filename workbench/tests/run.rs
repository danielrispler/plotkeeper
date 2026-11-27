//! Seam 4 (#548, Testing Decisions): the process runner, with real children. A fake `git` on `PATH` prints what it was
//! started with; `sh` scripts stand in for a command that hangs, fails or leaves a helper behind.

use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use studio_workbench::run::{
    RunError, gh_at, git, run_logged, run_until, run_until_unless, run_until_with_input,
};

/// Long enough for a loaded machine to start a script; far short of the scripts' own sleeps.
const SOON: Duration = Duration::from_secs(2);
const AMPLE: Duration = Duration::from_secs(30);

/// A program `name` in `dir` that prints its arguments, its environment and whether its stdin holds a line.
fn fake(dir: &Path, name: &str) -> std::path::PathBuf {
    let file = dir.join(name);
    let script = "#!/bin/sh\necho \"args: $*\"\necho \"cwd: $(pwd -P)\"\nenv\n\
                  if read line; then echo \"stdin: $line\"; else echo 'stdin: none'; fi\n";
    std::fs::write(&file, script).unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    file
}

fn printed(out: std::process::Output) -> Vec<String> {
    let text = String::from_utf8(out.stdout).unwrap();
    text.lines().map(String::from).collect()
}

fn sh(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

/// Whether process `pid` is still there.
fn alive(pid: &str) -> bool {
    let probe = Command::new("kill").args(["-0", pid]).output().unwrap();
    probe.status.success()
}

#[test]
fn a_git_never_prompts_takes_no_optional_locks_and_reads_no_input() {
    let tools = tempfile::tempdir().unwrap();
    fake(tools.path(), "git");
    let dir = tempfile::tempdir().unwrap();
    let path = std::env::join_paths(
        std::iter::once(tools.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let out = run_until(
        git(dir.path())
            .env("PATH", path)
            .args(["status", "--short"]),
        AMPLE,
    )
    .unwrap();

    let lines = printed(out);
    let has = |line: &str| lines.iter().any(|l| l == line);
    assert!(has("args: --no-optional-locks status --short"), "{lines:?}");
    assert!(has("GIT_TERMINAL_PROMPT=0"), "{lines:?}");
    assert!(has("GH_PROMPT_DISABLED=1"), "{lines:?}");
    assert!(has("stdin: none"), "{lines:?}");
    let here = dir.path().canonicalize().unwrap();
    assert!(has(&format!("cwd: {}", here.display())), "{lines:?}");
}

#[test]
fn a_gh_never_prompts_and_neither_does_the_git_it_starts() {
    let tools = tempfile::tempdir().unwrap();
    let gh = fake(tools.path(), "gh");
    let dir = tempfile::tempdir().unwrap();

    let out = run_until(gh_at(&gh, dir.path()).args(["pr", "list"]), AMPLE).unwrap();

    let lines = printed(out);
    let has = |line: &str| lines.iter().any(|l| l == line);
    assert!(has("args: pr list"), "{lines:?}");
    assert!(has("GH_PROMPT_DISABLED=1"), "{lines:?}");
    assert!(has("GIT_TERMINAL_PROMPT=0"), "{lines:?}");
    assert!(has("stdin: none"), "{lines:?}");
}

#[test]
fn input_reaches_the_commands_stdin() {
    let tools = tempfile::tempdir().unwrap();
    let gh = fake(tools.path(), "gh");

    let out = run_until_with_input(gh_at(&gh, tools.path()).arg("api"), b"the body\n", AMPLE);

    let lines = printed(out.unwrap());
    assert!(lines.iter().any(|l| l == "stdin: the body"), "{lines:?}");
}

#[test]
fn a_deadline_kills_the_whole_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("helper.pid");
    // The helper holds the command's output open, as a credential helper or a filter would.
    let script = format!("sleep 20 & echo $! > {}; sleep 20", pid_file.display());
    let started = Instant::now();

    let ended = run_until(&mut sh(&script), SOON);

    assert!(
        matches!(ended, Err(RunError::TimedOut(limit)) if limit == SOON),
        "{ended:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    let helper = std::fs::read_to_string(&pid_file).unwrap();
    let helper = helper.trim();
    assert!(!helper.is_empty());
    // A killed process is gone once its parent reaps it: `launchd` or `init` here, in a moment.
    let gone = (0..200).any(|_| {
        std::thread::sleep(Duration::from_millis(10));
        !alive(helper)
    });
    assert!(gone, "the helper {helper} outlived the deadline");
}

#[test]
fn a_helper_that_outlives_the_command_cannot_hold_the_run_past_its_deadline() {
    let started = Instant::now();

    let ended = run_until(&mut sh("sleep 20 & exit 0"), SOON);

    assert!(matches!(ended, Err(RunError::TimedOut(_))), "{ended:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_run_given_up_ends_at_once_as_timed_out() {
    let started = Instant::now();

    let ended = run_until_unless(&mut sh("sleep 20"), AMPLE, &|| true);

    assert!(matches!(ended, Err(RunError::TimedOut(_))), "{ended:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_run_given_up_while_a_helper_holds_its_output_ends_before_its_deadline() {
    let started = Instant::now();
    // The command exits at once; its helper holds the pipes until it is killed.
    let given_up = || started.elapsed() > Duration::from_millis(300);

    let ended = run_until_unless(&mut sh("sleep 20 & exit 0"), AMPLE, &given_up);

    assert!(matches!(ended, Err(RunError::TimedOut(_))), "{ended:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_program_that_is_not_there_failed_to_start() {
    let ended = run_until(&mut Command::new("/no/such/program"), AMPLE);

    let Err(RunError::Start(error)) = &ended else {
        panic!("{ended:?}");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    let said = ended.unwrap_err().to_string();
    assert!(said.starts_with("did not start: "), "{said}");
}

#[test]
fn a_run_past_its_deadline_timed_out() {
    let limit = Duration::from_millis(200);

    let ended = run_until(&mut sh("sleep 20"), limit);

    assert!(
        matches!(ended, Err(RunError::TimedOut(l)) if l == limit),
        "{ended:?}"
    );
    assert_eq!(ended.unwrap_err().to_string(), "ran past 200ms");
}

#[test]
fn a_non_zero_exit_failed_with_its_status_and_what_it_printed() {
    let ended = run_until(
        &mut sh("echo partial; echo ' the reason ' >&2; exit 3"),
        AMPLE,
    );

    let Err(RunError::Failed(output)) = &ended else {
        panic!("{ended:?}");
    };
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, b"partial\n");
    assert_eq!(output.stderr, b" the reason \n");
    assert_eq!(ended.unwrap_err().to_string(), "the reason");
}

#[test]
fn a_silent_non_zero_exit_says_its_status() {
    let ended = run_until(&mut sh("exit 3"), AMPLE);

    assert_eq!(ended.unwrap_err().to_string(), "exit status: 3");
}

#[test]
fn a_zero_exit_gives_both_streams_however_much_they_hold() {
    // More than a pipe holds on each stream: neither stalls the other.
    let script = "head -c 300000 /dev/zero; head -c 300000 /dev/zero >&2";

    let out = run_until(&mut sh(script), AMPLE).unwrap();

    assert_eq!((out.stdout.len(), out.stderr.len()), (300_000, 300_000));
}

/// `sh -c script` with both streams in `log`.
fn logged(script: &str, log: &Path) -> Command {
    let out = std::fs::File::create(log).unwrap();
    let mut command = sh(script);
    command.stderr(out.try_clone().unwrap()).stdout(out);
    command
}

/// Whether process `pid` is gone within two seconds: a killed process is gone once its parent reaps it.
fn goes(pid: &str) -> bool {
    (0..200).any(|_| {
        std::thread::sleep(Duration::from_millis(10));
        !alive(pid)
    })
}

#[test]
fn a_logged_run_gives_its_exit_status_and_leaves_its_output_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("out.log");

    let status = run_logged(
        &mut logged("echo said; echo why >&2; exit 3", &log),
        AMPLE,
        &|| false,
    );

    assert_eq!(status.unwrap().code(), Some(3));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "said\nwhy\n");
}

#[test]
fn a_logged_run_past_its_deadline_leaves_no_process_behind() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("helper.pid");
    let script = format!("sleep 20 & echo $! > {}; sleep 20", pid_file.display());
    let started = Instant::now();

    let ended = run_logged(&mut logged(&script, &dir.path().join("log")), SOON, &|| {
        false
    });

    assert!(
        matches!(ended, Err(RunError::TimedOut(limit)) if limit == SOON),
        "{ended:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    let helper = std::fs::read_to_string(&pid_file).unwrap();
    assert!(
        goes(helper.trim()),
        "the helper {helper} outlived the deadline"
    );
}

#[test]
fn a_logged_run_given_up_ends_at_once_and_leaves_no_process_behind() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("helper.pid");
    let script = format!("sleep 20 & echo $! > {}; sleep 20", pid_file.display());
    let started = Instant::now();
    let given_up = || pid_file.exists() && started.elapsed() > Duration::from_millis(300);

    let ended = run_logged(
        &mut logged(&script, &dir.path().join("log")),
        AMPLE,
        &given_up,
    );

    assert!(matches!(ended, Err(RunError::TimedOut(_))), "{ended:?}");
    assert!(started.elapsed() < Duration::from_secs(10));
    let helper = std::fs::read_to_string(&pid_file).unwrap();
    assert!(goes(helper.trim()), "the helper {helper} outlived the run");
}

#[test]
fn a_helper_a_logged_run_left_behind_is_killed_when_it_exits() {
    let dir = tempfile::tempdir().unwrap();
    let pid_file = dir.path().join("helper.pid");
    let script = format!("sleep 20 & echo $! > {}; exit 0", pid_file.display());
    let started = Instant::now();

    let status = run_logged(
        &mut logged(&script, &dir.path().join("log")),
        AMPLE,
        &|| false,
    );

    assert!(status.unwrap().success());
    assert!(started.elapsed() < Duration::from_secs(10));
    let helper = std::fs::read_to_string(&pid_file).unwrap();
    assert!(goes(helper.trim()), "the helper {helper} outlived the run");
}

#[test]
fn a_logged_program_that_is_not_there_failed_to_start() {
    let ended = run_logged(&mut Command::new("/no/such/program"), AMPLE, &|| false);

    assert!(matches!(ended, Err(RunError::Start(_))), "{ended:?}");
}

/// Every `git` and `gh` is built by the runner: outside it, `archive/` and the tests' own helpers, no source file
/// names either program to `Command::new`.
// @law: git-and-gh-go-through-the-runner
#[test]
fn no_git_or_gh_is_built_outside_the_runner() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let listed = Command::new("git")
        .args(["ls-files", "-z", "--", "*.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed = String::from_utf8(listed.stdout).unwrap();
    let ours = listed.split('\0').filter(|file| {
        let helper = file.starts_with("test-support/")
            || file.contains("/tests/")
            || file.ends_with("tests.rs");
        !file.is_empty()
            && !helper
            && !file.starts_with("archive/")
            && *file != "workbench/src/run.rs"
    });
    let built = [
        "Command::new(\"git\")",
        "Command::new(\"gh\")",
        "Command::new(studio_workbench::gh_path())",
        "Command::new(gh_path())",
        "Command::new(crate::gh_path())",
    ];
    let mut found = Vec::new();
    for file in ours {
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        // A file's own test module, inline or in its `tests.rs`, is a test helper.
        for (number, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            if built.iter().any(|b| line.contains(b)) {
                found.push(format!("{file}:{}", number + 1));
            }
        }
    }
    assert!(found.is_empty(), "built without the runner: {found:#?}");
}
