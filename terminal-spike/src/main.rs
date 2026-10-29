// Spike for #322: terminals inside the Tauri app that survive restarts. Throwaway.
// One binary: `--daemon` runs the background terminal process (Orca's model); anything else runs the app.
mod app;
mod daemon;

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};

pub struct Pty {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    pub killer: Box<dyn ChildKiller + Send + Sync>,
    pub pid: Option<u32>,
}

// Variables that leak in from whatever started the app (Ghostty, tmux, a Claude Code Bash tool) are dropped. Terminal
// ones make programs assume the wrong terminal. Claude Code's mark a nested session (transcripts off) and carry its
// session id and messaging token. Shells start as login shells, so anything set in Daniel's profile comes back.
const SCRUB: &[&str] =
    &["TERM_PROGRAM", "TERM_SESSION_ID", "LC_TERMINAL", "ITERM_", "GHOSTTY_", "KITTY_", "TMUX", "HERDR_", "STUDIO_", "CLAUDE"];

pub fn spawn(
    argv: &[String],
    cwd: &str,
    env: &[(String, String)],
    cols: u16,
    rows: u16,
    mut on_output: impl FnMut(&[u8]) + Send + 'static,
    on_exit: impl FnOnce(String) + Send + 'static,
) -> Result<Pty, String> {
    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.cwd(cwd);
    for (k, _) in std::env::vars() {
        if SCRUB.iter().any(|p| k.starts_with(p)) {
            cmd.env_remove(k);
        }
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let (killer, pid) = (child.clone_killer(), child.process_id());
    std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            on_output(&buf[..n]);
        }
    });
    // A separate waiter, because a background job can keep the PTY open after the shell itself exits.
    std::thread::spawn(move || {
        let status = match child.wait() {
            Ok(s) => s.signal().map(|sig| format!("signal {sig}")).unwrap_or(format!("code {}", s.exit_code())),
            Err(err) => err.to_string(),
        };
        on_exit(status);
    });
    Ok(Pty { master: pair.master, writer, killer, pid })
}

// The program in the terminal's foreground, such as `zsh` or `claude`.
pub fn foreground(pty: &Pty) -> Option<String> {
    let pid = pty.master.process_group_leader()?;
    let out = std::process::Command::new("ps").args(["-o", "comm=", "-p", &pid.to_string()]).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().rsplit('/').next()?.to_string())
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--daemon") {
        daemon::run();
    } else {
        app::run();
    }
}
