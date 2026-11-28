//! A fake program for tests: the running test binary, hard-linked (or copied, where a link
//! cannot be made) under another name, with a behaviour file beside it. Started under that
//! name, the binary finds the behaviour file before the test harness's `main`, acts it out and
//! exits. It needs no shell, Python or Node, so it runs the same on every platform.

use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// What a fake does when it is called.
#[derive(Clone, Debug, Default)]
pub struct Behaviour {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
    /// How long it waits, after printing, before it exits.
    pub wait: Duration,
    /// Whether it appends the call to its record, read back with [`calls`].
    pub record: bool,
}

/// One recorded call of a fake.
#[derive(Debug, PartialEq)]
pub struct Call {
    pub args: Vec<String>,
    pub cwd: String,
    pub stdin: String,
    pub env: BTreeMap<String, String>,
}

fn beside(fake: &Path, suffix: &str) -> PathBuf {
    let mut name = fake.as_os_str().to_owned();
    name.push(suffix);
    name.into()
}

fn behaviour_file(fake: &Path) -> PathBuf {
    beside(fake, ".fake.json")
}

fn record_file(fake: &Path) -> PathBuf {
    beside(fake, ".record.json")
}

/// A name for a temporary file beside `file`, unique to this process, thread and call.
fn temp_beside(file: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    beside(file, &format!(".tmp{}-{n}", std::process::id()))
}

/// Writes `text` to `file` so a reader sees all of it or the file as it was.
fn write_whole(file: &Path, text: &str) {
    let temp = temp_beside(file);
    fs::write(&temp, text).unwrap();
    fs::rename(&temp, file).unwrap();
}

/// Puts the running test binary at `file`: a hard link, or a copy where a link cannot cross
/// filesystems. Never a symbolic link: on Linux the process's own path would read as the test
/// binary's.
fn place(file: &Path) {
    let exe = std::env::current_exe().unwrap();
    match fs::hard_link(&exe, file) {
        Ok(()) => {}
        // Another call made it first. Copying onto it would write onto the test binary itself.
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            // Copy under a temporary name, close it, and rename it into place: a file still open
            // for writing makes starting it fail with "Text file busy" on Linux.
            let temp = temp_beside(file);
            fs::copy(&exe, &temp).unwrap();
            fs::rename(&temp, file).unwrap();
        }
        Err(e) => panic!("cannot place the fake at {}: {e}", file.display()),
    }
}

/// Makes the fake `name` in `dir` behave as `behaviour` and returns its path (`name.exe` on
/// Windows). Calling it again for the same name rewrites the behaviour and keeps the record.
pub fn fake(dir: &Path, name: &str, behaviour: &Behaviour) -> PathBuf {
    let file = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    if !file.exists() {
        place(&file);
    }
    let spec = json!({
        "stdout": behaviour.stdout,
        "stderr": behaviour.stderr,
        "code": behaviour.code,
        "wait_ms": behaviour.wait.as_millis() as u64,
        "record": behaviour.record,
    });
    write_whole(&behaviour_file(&file), &spec.to_string());
    file
}

/// The calls a fake recorded, oldest first.
pub fn calls(fake: &Path) -> Vec<Call> {
    let Ok(text) = fs::read_to_string(record_file(fake)) else {
        return Vec::new();
    };
    let entries: Vec<Value> = serde_json::from_str(&text).unwrap();
    entries
        .iter()
        .map(|e| Call {
            args: serde_json::from_value(e["args"].clone()).unwrap(),
            cwd: e["cwd"].as_str().unwrap().into(),
            stdin: e["stdin"].as_str().unwrap().into(),
            env: serde_json::from_value(e["env"].clone()).unwrap(),
        })
        .collect()
}

/// Runs before the test harness's `main` in every binary that links this crate, and does
/// nothing unless a behaviour file sits beside the running executable.
#[ctor::ctor(unsafe)]
fn act_as_a_fake_when_started_as_one() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Ok(text) = fs::read_to_string(behaviour_file(&exe)) else {
        return;
    };
    let spec: Value = serde_json::from_str(&text).unwrap();
    if spec["record"].as_bool() == Some(true) {
        let mut stdin = String::new();
        std::io::stdin().read_to_string(&mut stdin).ok();
        let entry = json!({
            "args": std::env::args().skip(1).collect::<Vec<_>>(),
            "cwd": std::env::current_dir().unwrap().canonicalize().unwrap().display().to_string(),
            "stdin": stdin,
            "env": std::env::vars().collect::<BTreeMap<_, _>>(),
        });
        let file = record_file(&exe);
        // One call at a time reads and rewrites the record, so no entry is lost.
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(beside(&exe, ".record.lock"))
            .unwrap();
        lock.lock().unwrap();
        let mut all: Vec<Value> = fs::read_to_string(&file)
            .ok()
            .map(|t| serde_json::from_str(&t).unwrap())
            .unwrap_or_default();
        all.push(entry);
        write_whole(&file, &serde_json::to_string(&all).unwrap());
        drop(lock);
    }
    std::io::stdout()
        .write_all(spec["stdout"].as_str().unwrap().as_bytes())
        .ok();
    std::io::stdout().flush().ok();
    std::io::stderr()
        .write_all(spec["stderr"].as_str().unwrap().as_bytes())
        .ok();
    std::thread::sleep(Duration::from_millis(spec["wait_ms"].as_u64().unwrap()));
    std::process::exit(spec["code"].as_i64().unwrap() as i32);
}
