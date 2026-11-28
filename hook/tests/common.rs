//! What the hook's tests share: a scratch `HOME`, the hook binaries run on a payload, and the event log's lock held.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

/// Clears `XDG_DATA_HOME` and `XDG_CACHE_HOME` in this test's process, once, so on Linux the folders a test resolves
/// itself (`studio_hook::event_log` of its scratch home) are the ones its hook child, which runs without them, writes:
/// under the scratch home, never a real data folder.
fn no_xdg() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    // SAFETY: `remove_var` is sound only while no other thread reads or writes the environment. That holds because the
    // tests run under nextest, which runs each test in a process of its own, and a test makes its scratch home or its
    // first hook command, the two callers, before it starts a thread.
    ONCE.call_once(|| unsafe {
        std::env::remove_var("XDG_DATA_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
    });
}

/// An empty scratch `HOME`, named by `label` and this process.
pub(crate) fn scratch(label: &str) -> PathBuf {
    no_xdg();
    let home = std::env::temp_dir().join(format!("studio-{label}-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&home).ok();
    fs::create_dir_all(&home).unwrap();
    home
}

/// `plotkeeper-hook` with `home` as its `HOME` and no `XDG_DATA_HOME` (so on Linux its data folder is under `home` too),
/// not yet started.
pub(crate) fn hook(home: &Path) -> Command {
    no_xdg();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_plotkeeper-hook"));
    cmd.env("HOME", home).env_remove("XDG_DATA_HOME");
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

/// Holds the event log's lock, `events.lock` beside `log`, as another writer or a prune would, until dropped.
pub(crate) fn hold(log: &Path) -> fs::File {
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(log.with_file_name("events.lock"))
        .unwrap();
    lock.lock().unwrap();
    lock
}
