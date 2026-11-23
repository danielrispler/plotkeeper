// The pid the hook records as `studio_pid` (#330): the agent's process, so the server can end a
// session whose agent is gone. A fake process table stands in for `ps`; no process is signalled.
use crate::common::{feed, hook, scratch};
use std::{fs, process::Stdio};
use studio_hook::agent_pid;

/// A table of `(pid, command, parent)`.
fn table(rows: &'static [(u32, &'static str, u32)]) -> impl Fn(u32) -> Option<(String, u32)> {
    move |pid| {
        let row = rows.iter().find(|r| r.0 == pid)?;
        Some((row.1.to_string(), row.2))
    }
}

#[test]
fn the_parent_is_the_agent_when_it_is_not_a_shell() {
    let t = table(&[(500, "/opt/homebrew/bin/claude", 100)]);
    assert_eq!(agent_pid(None, 500, t), Some(500));
}

#[test]
fn a_shell_between_the_agent_and_the_hook_is_skipped() {
    let t = table(&[(501, "/bin/sh", 500), (500, "claude", 100)]);
    assert_eq!(agent_pid(None, 501, t), Some(500));
}

#[test]
fn the_wrapper_pid_wins_and_a_bad_one_is_ignored() {
    let t = || table(&[(501, "/bin/sh", 500)]);
    assert_eq!(agent_pid(Some("777"), 501, t()), Some(777));
    assert_eq!(agent_pid(Some("nope"), 501, t()), Some(500));
    assert_eq!(agent_pid(Some("0"), 501, t()), Some(500));
}

#[test]
fn no_pid_when_the_hook_has_no_real_parent() {
    // Reparented to init: its agent is gone, and init is never an agent.
    assert_eq!(agent_pid(None, 1, table(&[])), None);
    assert_eq!(agent_pid(None, 0, table(&[])), None);
    // A shell that is a child of init has no agent above it.
    assert_eq!(agent_pid(None, 501, table(&[(501, "zsh", 1)])), None);
}

/// VS Code runs Copilot's agent in its own processes, so the pid the hook would record is the IDE's,
/// which outlives every session: none is recorded, and the session takes the "idle?" path (#361; #320,
/// "IDE-hosted pids").
#[test]
fn no_pid_when_the_agent_is_vs_code() {
    const HELPER: &str = "/Applications/Visual Studio Code.app/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)";
    const INSIDERS: &str =
        "/Applications/Visual Studio Code - Insiders.app/Contents/MacOS/Electron";
    for t in [
        || table(&[(600, "/bin/zsh", 700), (700, HELPER, 1)]),
        || table(&[(600, "/bin/zsh", 700), (700, INSIDERS, 1)]),
    ] {
        assert_eq!(agent_pid(None, 600, t()), None);
        assert_eq!(agent_pid(None, 700, t()), None);
    }
    // Copilot's CLI in VS Code's terminal is the agent, as in any terminal.
    let t = table(&[(800, "/opt/homebrew/bin/copilot", 700), (700, HELPER, 1)]);
    assert_eq!(agent_pid(None, 800, t), Some(800));
}

fn recorded(wrapper: Option<&str>, name: &str) -> serde_json::Value {
    let home = scratch(&format!("pid-{name}"));
    let mut cmd = hook(&home);
    cmd.env_remove("PLOTKEEPER_AGENT_PID")
        .stdout(Stdio::piped());
    if let Some(pid) = wrapper {
        cmd.env("PLOTKEEPER_AGENT_PID", pid);
    }
    let payload = serde_json::json!({ "session_id": "s-1", "hook_event_name": "Stop", "cwd": "/" });
    assert!(
        feed(cmd, &payload)
            .wait_with_output()
            .unwrap()
            .status
            .success()
    );
    let log = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    serde_json::from_str(log.lines().next().unwrap()).unwrap()
}

#[test]
fn the_logged_line_carries_the_parent_pid_or_the_wrappers() {
    // The test binary is the hook's parent here, and it is no shell.
    assert_eq!(recorded(None, "parent")["studio_pid"], std::process::id());
    assert_eq!(recorded(Some("4242"), "wrapper")["studio_pid"], 4242);
}

/// A hook run by an IDE app reports the IDE's pid, which lives on past the session, so none is
/// recorded and the session takes the "idle?" path (#320, "IDE-hosted pids"; #362).
#[test]
fn no_pid_when_the_agent_is_an_ide_app() {
    const HELPER: &str = "/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Plugin).app/Contents/MacOS/Cursor Helper (Plugin)";
    let t = table(&[(600, HELPER, 100)]);
    assert_eq!(agent_pid(None, 600, t), None);
    // Through a shell too.
    let t = table(&[(601, "/bin/zsh", 600), (600, HELPER, 100)]);
    assert_eq!(agent_pid(None, 601, t), None);
    // Cursor's CLI is a program of its own, whose pid ends with the session.
    let t = table(&[(
        700,
        "/Users/me/.local/share/cursor-agent/versions/2025.11.25/node",
        100,
    )]);
    assert_eq!(agent_pid(None, 700, t), Some(700));
}
