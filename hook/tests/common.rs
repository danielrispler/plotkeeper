//! What the hook's tests share: a scratch `HOME`, and the hook binaries run on a payload.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

/// An empty scratch `HOME`, named by `label` and this process.
pub(crate) fn scratch(label: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("studio-{label}-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&home).ok();
    fs::create_dir_all(&home).unwrap();
    home
}

/// `plotkeeper-hook` with `home` as its `HOME`, no `XDG_DATA_HOME` (so on Linux its data folder is under `home` too) and
/// outside a studio terminal, not yet started.
pub(crate) fn hook(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_plotkeeper-hook"));
    cmd.env("HOME", home)
        .env_remove("XDG_DATA_HOME")
        .env_remove("PLOTKEEPER_TERMINAL_ID");
    cmd
}

/// Starts `cmd` with `payload` as its stdin, which it closes, and returns the child.
pub(crate) fn feed(mut cmd: Command, payload: &serde_json::Value) -> Child {
    let mut child = cmd.stdin(Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child
}
