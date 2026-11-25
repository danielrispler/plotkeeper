//! One plotkeeper per data folder (danielrispler/plotkeeper#325): a lock file the running one holds, and a file beside it with
//! the port and token it serves on, so a second start can open the browser on the first, and its version, so a second
//! start of another version can say so (#439).

use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

const LOCK: &str = "plotkeeper.lock";
const RUNNING: &str = "plotkeeper.running.json";

/// The lock of the data folder, held while this value lives.
pub struct Held {
    _lock: File,
    running: PathBuf,
}

/// Where the one that holds the lock serves.
#[derive(Debug)]
pub struct Running {
    pub port: u16,
    pub token: String,
    /// Its version; `None` in a running file from before #439.
    pub version: Option<String>,
}

impl Running {
    /// What a start of version `mine` says when it finds this one running at another version: that the older one keeps
    /// serving until it is stopped. `None` when the versions match or the running file does not say.
    pub fn version_notice(&self, mine: &str) -> Option<String> {
        let theirs = self.version.as_deref().filter(|v| *v != mine)?;
        Some(format!(
            "plotkeeper {theirs} is still running; stop it (Ctrl+C) and run again to use {mine}"
        ))
    }
}

pub enum Instance {
    /// No other plotkeeper holds the folder.
    First(Held),
    /// Another does, and serves here.
    Second(Running),
}

/// Takes the folder's lock, or finds who holds it.
pub fn acquire(dir: &Path) -> io::Result<Instance> {
    fs::create_dir_all(dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(dir.join(LOCK))?;
    match lock.try_lock() {
        Ok(()) => {
            let running = dir.join(RUNNING);
            // Best effort: a file already gone is fine.
            fs::remove_file(&running).ok();
            Ok(Instance::First(Held {
                _lock: lock,
                running,
            }))
        }
        Err(TryLockError::WouldBlock) => {
            // The first may still be starting: its address is written once it listens, after its wiring step, which
            // may wait on its question (#434).
            for _ in 0..40 {
                if let Some(running) = read_running(&dir.join(RUNNING)) {
                    return Ok(Instance::Second(running));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(io::Error::other(
                "another plotkeeper holds the data folder but does not say where it serves yet; it may still be starting, or waiting for an answer in its terminal",
            ))
        }
        Err(TryLockError::Error(e)) => Err(e),
    }
}

fn read_running(file: &Path) -> Option<Running> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(file).ok()?).ok()?;
    Some(Running {
        port: u16::try_from(value["port"].as_u64()?).ok()?,
        token: value["token"].as_str()?.to_string(),
        version: value["version"].as_str().map(str::to_string),
    })
}

impl Held {
    /// Says where this plotkeeper serves and its version, for a second start. The file is for its owner only: it holds
    /// the token.
    pub fn publish(&self, port: u16, token: &str) -> io::Result<()> {
        let version = env!("CARGO_PKG_VERSION");
        let text =
            serde_json::json!({ "port": port, "token": token, "version": version }).to_string();
        let temp = self.running.with_extension("tmp");
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp)?;
        io::Write::write_all(&mut file, text.as_bytes())?;
        fs::rename(&temp, &self.running)
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        // Best effort: a file already gone is fine.
        fs::remove_file(&self.running).ok();
    }
}
