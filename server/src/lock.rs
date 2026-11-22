//! One plotkeeper per data folder (danielrispler/plotkeeper#325): a lock file the running one holds, and a file beside it with
//! the port and token it serves on, so a second start can open the browser on the first.

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
            let _ = fs::remove_file(&running);
            Ok(Instance::First(Held {
                _lock: lock,
                running,
            }))
        }
        Err(TryLockError::WouldBlock) => {
            // The first may still be starting: its address is written once it listens.
            for _ in 0..40 {
                if let Some(running) = read_running(&dir.join(RUNNING)) {
                    return Ok(Instance::Second(running));
                }
                thread::sleep(Duration::from_millis(50));
            }
            Err(io::Error::other(
                "another plotkeeper holds the data folder but does not say where it serves",
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
    })
}

impl Held {
    /// Says where this plotkeeper serves, for a second start. The file is for its owner only: it holds the token.
    pub fn publish(&self, port: u16, token: &str) -> io::Result<()> {
        let text = serde_json::json!({ "port": port, "token": token }).to_string();
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
        let _ = fs::remove_file(&self.running);
    }
}
