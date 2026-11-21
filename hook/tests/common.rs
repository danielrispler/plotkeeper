//! What the hook's tests share: a scratch `HOME`, and the hook binaries run on a payload.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

/// An empty scratch `HOME`, named by `label` and this process.
pub fn scratch(label: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("studio-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    home
}

/// `studio-hook` with `home` as its `HOME` and outside a studio terminal, not yet started.
pub fn hook(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_studio-hook"));
    cmd.env("HOME", home).env_remove("STUDIO_TERMINAL_ID");
    cmd
}

/// Starts `cmd` with `payload` as its stdin, which it closes, and returns the child.
pub fn feed(mut cmd: Command, payload: &serde_json::Value) -> Child {
    let mut child = cmd.stdin(Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child
}
