//! The one process runner (#548, "One process runner in `studio-workbench`"): every `git` and `gh` the workspace
//! starts is built here, so none waits on a prompt nobody sees, and every wait on a child with a deadline goes through
//! `run_until`, which leaves no helper of a stopped command behind.

use std::{
    ffi::OsStr,
    fmt,
    io::{self, Read, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

/// Why a run gave no output to use.
#[derive(Debug)]
pub enum RunError {
    /// The program did not start, or its exit could not be read.
    Start(io::Error),
    /// It ran past this deadline, and its process group was killed.
    TimedOut(Duration),
    /// It exited non-zero: its status, and what it printed.
    Failed(Output),
}

/// What the command said went wrong: for a non-zero exit its trimmed stderr, or its status when it printed none.
impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Start(error) => write!(f, "did not start: {error}"),
            RunError::TimedOut(deadline) => write!(f, "ran past {deadline:?}"),
            RunError::Failed(output) => match String::from_utf8_lossy(&output.stderr).trim() {
                "" => write!(f, "{}", output.status),
                said => f.write_str(said),
            },
        }
    }
}

impl std::error::Error for RunError {}

/// `git` in `dir`, to add arguments to. It takes no optional locks, so a read never holds `index.lock` against an
/// agent's commit (#27), and it fails where it would ask for a password or a passphrase.
pub fn git(dir: &Path) -> Command {
    let mut command = quiet(Command::new("git"), dir);
    command.arg("--no-optional-locks");
    command
}

/// `gh` in `dir`, to add arguments to: [`crate::gh_path`]'s, which never prompts, nor does the `git` it starts.
pub fn gh(dir: &Path) -> Command {
    gh_at(crate::gh_path(), dir)
}

/// [`gh`] with the `gh` at `program`.
pub fn gh_at(program: impl AsRef<OsStr>, dir: &Path) -> Command {
    quiet(Command::new(program), dir)
}

/// `command` in `dir` with no input and every prompt off. Both variables go to both programs: `gh` starts `git`.
fn quiet(mut command: Command, dir: &Path) -> Command {
    command
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null());
    command
}

/// Runs `command` with no input and returns its output when it exits 0 within `deadline`. It runs in a process group
/// of its own, and past the deadline the whole group is killed: the command and whatever it started.
pub fn run_until(command: &mut Command, deadline: Duration) -> Result<Output, RunError> {
    run(command, None, deadline, &|| false)
}

/// [`run_until`], with `input` on the command's stdin.
pub fn run_until_with_input(
    command: &mut Command,
    input: &[u8],
    deadline: Duration,
) -> Result<Output, RunError> {
    run(command, Some(input), deadline, &|| false)
}

/// [`run_until`], for a caller that may stop waiting first: once `given_up` is true the group is killed as at the
/// deadline, and the run ends as [`RunError::TimedOut`].
pub fn run_until_unless(
    command: &mut Command,
    deadline: Duration,
    given_up: &dyn Fn() -> bool,
) -> Result<Output, RunError> {
    run(command, None, deadline, given_up)
}

fn run(
    command: &mut Command,
    input: Option<&[u8]>,
    deadline: Duration,
    given_up: &dyn Fn() -> bool,
) -> Result<Output, RunError> {
    let end = Instant::now() + deadline;
    let mut child = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(RunError::Start)?;
    let group = libc::pid_t::try_from(child.id()).ok();
    let kill_group = || {
        if let Some(group) = group {
            // SAFETY: `kill` takes no pointers. The group is the one made for this child above; its id stays taken
            // while the unreaped child or a process holding its output lives, so this signals no one else.
            unsafe { libc::kill(-group, libc::SIGKILL) };
        }
    };
    // Threads, so a command that reads no input cannot hold back the deadline, and one that fills one pipe while the
    // other is read cannot stall.
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        let input = input.to_vec();
        // A command that exits without reading its input closes the pipe: its exit status says what happened.
        thread::spawn(move || stdin.write_all(&input).ok());
    }
    let (send, read) = mpsc::channel();
    let pipes: [(bool, Option<Box<dyn Read + Send>>); 2] = [
        (true, child.stdout.take().map(|p| Box::new(p) as _)),
        (false, child.stderr.take().map(|p| Box::new(p) as _)),
    ];
    for (is_stdout, pipe) in pipes {
        let send = send.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                // What was read before a failed read is still what the command printed.
                pipe.read_to_end(&mut bytes).ok();
            }
            // The receiver is gone only when the run has already ended.
            send.send((is_stdout, bytes)).ok();
        });
    }
    let mut nap = Duration::from_millis(1);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                kill_group();
                return Err(RunError::Start(error));
            }
        }
        if Instant::now() >= end || given_up() {
            kill_group();
            // Reaped, so it is no zombie; a killed child's wait cannot fail in a way to act on.
            child.wait().ok();
            return Err(RunError::TimedOut(deadline));
        }
        thread::sleep(nap);
        nap = (nap * 2).min(Duration::from_millis(5));
    };
    let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
    for _ in 0..2 {
        // A helper the command left behind may hold the pipes open: it gets what is left of the deadline, and at
        // least a moment for the output of a command that exited right at it.
        let left = end.saturating_duration_since(Instant::now());
        let Ok((is_stdout, bytes)) = read.recv_timeout(left.max(Duration::from_millis(100))) else {
            kill_group();
            return Err(RunError::TimedOut(deadline));
        };
        if is_stdout {
            stdout = bytes;
        } else {
            stderr = bytes;
        }
    }
    let output = Output {
        status,
        stdout,
        stderr,
    };
    if output.status.success() {
        Ok(output)
    } else {
        Err(RunError::Failed(output))
    }
}
