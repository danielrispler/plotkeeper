//! A fake program for tests: the running test binary, hard-linked (or copied, where a link
//! cannot be made) under another name, with a behaviour file beside it. Started under that
//! name, the binary finds the behaviour file before the test harness's `main`, acts it out and
//! exits. It needs no shell, Python or Node, so it runs the same on every platform.

use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
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

/// Makes the fake `name` in `dir` behave as `behaviour` and returns its path (`name.exe` on
/// Windows). Calling it again for the same name rewrites the behaviour, and clears the record.
pub fn fake(dir: &Path, name: &str, behaviour: &Behaviour) -> PathBuf {
    let file = dir.join(if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    if !behaviour_file(&file).exists() {
        fs::remove_file(&file).ok();
        let exe = std::env::current_exe().unwrap();
        // Never a symbolic link: on Linux the process's own path would read as the test binary's.
        if fs::hard_link(&exe, &file).is_err() {
            fs::copy(&exe, &file).unwrap();
        }
    }
    let spec = json!({
        "stdout": behaviour.stdout,
        "stderr": behaviour.stderr,
        "code": behaviour.code,
        "wait_ms": behaviour.wait.as_millis() as u64,
        "record": behaviour.record,
    });
    fs::write(behaviour_file(&file), spec.to_string()).unwrap();
    fs::remove_file(record_file(&file)).ok();
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
        let mut all: Vec<Value> = fs::read_to_string(&file)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        all.push(entry);
        fs::write(&file, serde_json::to_string(&all).unwrap()).unwrap();
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
