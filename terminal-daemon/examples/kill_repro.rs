//! Can macOS kill the terminal daemon when the app ends (danielrispler/Falinks#543)? A reproduction that never touches the
//! live daemon, socket or data folder. It launches a separately built app, with its own bundle
//! identifier, through LaunchServices (`open`), so the app is in its own coalition as Daniel's is; a
//! plain child process would prove nothing. The app gets its own `HOME` in `/tmp`, so its data
//! folder and daemon socket are its own, and the fake card writer, so it makes no model call.
//!
//! For each way the app can end (Quit, `kill -9` as a force quit, `kill -SEGV` as a crash), it
//! launches the app, which starts the daemon and its login shell; spawns a second terminal; ends
//! the app; then checks that the daemon and both terminals' programs are alive. It prints JSON on
//! standard output and its progress on standard error, and copies the daemon's log into `--out`.
//! Memory pressure cannot be forced reliably, so it is not tried. It refuses to run against the
//! live home or the live bundle identifier. Run it outside auto mode, from the repository's root, as
//! `real-calls.sh` does:
//!
//! `cargo run -q -p studio-terminal-daemon --example kill_repro -- --app <Repro.app> --out <dir>`

use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};
use studio_terminal_daemon::Client;

/// The live app's bundle identifier. A build with it shares the live app's single-instance lock, so
/// launching it would hand over to Daniel's app.
const LIVE_ID: &str = "dev.falinks.studio";

/// Refuses a run whose data folder and socket would be the live ones, or whose app is the live app.
fn guard(live_home: &Path, repro_home: &Path, identifier: &str) -> Result<(), String> {
    let canonical = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    if canonical(live_home) == canonical(repro_home) {
        return Err(format!(
            "refusing: {} is the live home, so its socket and data folder are the live ones",
            repro_home.display()
        ));
    }
    if identifier == LIVE_ID {
        return Err(format!(
            "refusing: the app's identifier is the live {LIVE_ID}"
        ));
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("kill_repro: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str| {
        let at = args.iter().position(|a| a == name);
        at.and_then(|at| args.get(at + 1)).map(PathBuf::from)
    };
    let usage = "usage: kill_repro --app <Repro.app> --out <dir>";
    let (app, out) = (arg("--app").ok_or(usage)?, arg("--out").ok_or(usage)?);
    let plist = app.join("Contents/Info.plist");
    let identifier = plist_key(&plist, "CFBundleIdentifier")?;
    let exe = app
        .join("Contents/MacOS")
        .join(plist_key(&plist, "CFBundleExecutable")?);
    // In `/tmp`: a socket path must stay under 104 bytes.
    let home = PathBuf::from(format!("/tmp/studio-repro-{}", std::process::id()));
    let live = PathBuf::from(std::env::var_os("HOME").ok_or("no HOME")?);
    guard(&live, &home, &identifier)?;
    std::fs::create_dir_all(&home).map_err(|e| e.to_string())?;
    // An empty `.zshrc` keeps zsh's first-run prompt out of the login shells.
    std::fs::write(home.join(".zshrc"), "").map_err(|e| e.to_string())?;
    let data = home.join("Library/Application Support/dev.falinks.studio");
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    // The configured repository is the sandbox home, as the live one is the live `settings.json`'s.
    let settings = json!({ "repository": home });
    std::fs::write(data.join("settings.json"), settings.to_string()).map_err(|e| e.to_string())?;
    let socket = data.join("terminal-daemon.sock");

    let mut cases = Vec::new();
    for (case, signal) in [
        ("quit", None),
        ("force quit (kill -9)", Some(libc::SIGKILL)),
        ("crash (kill -SEGV)", Some(libc::SIGSEGV)),
    ] {
        eprintln!("== {case}");
        let result = one_case(&app, &exe, &identifier, &home, &socket, signal);
        let result = result.unwrap_or_else(|e| json!({ "error": e }));
        eprintln!("   {result}");
        cases.push(json!({ "case": case, "result": result }));
    }

    // Clean up only what this run made: its app, its daemon and its terminals, then its home.
    if let Some(pid) = app_pid(&exe) {
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
    if let Err(e) = studio_terminal_daemon::stop(&socket) {
        eprintln!("stop the reproduction's daemon: {e}");
    }
    let _ = std::fs::copy(socket.with_extension("log"), out.join("543-daemon.log"));
    let _ = std::fs::remove_dir_all(&home);
    let report = json!({
        "app": app, "identifier": identifier, "cases": cases,
        "untested": "memory pressure: it cannot be forced reliably",
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
    Ok(())
}

/// Launches the app, ends it with `signal` (Quit when `None`) and reports what survived.
fn one_case(
    app: &Path,
    exe: &Path,
    identifier: &str,
    home: &Path,
    socket: &Path,
    signal: Option<i32>,
) -> Result<Value, String> {
    let env = |k: &str, v: &Path| format!("{k}={}", v.display());
    let opened = Command::new("open")
        .arg("-n")
        .arg("-a")
        .arg(app)
        .args(["--env", &env("HOME", home)])
        .args(["--env", "STUDIO_CARD_WRITER=fake"])
        .status()
        .map_err(|e| e.to_string())?;
    if !opened.success() {
        return Err(format!("open {} failed", app.display()));
    }
    let app_pid = wait(20, || app_pid(exe)).ok_or("the app did not start")?;
    // The app starts the daemon and its login shell, `shell`, once its page loads.
    let Some(_) = wait(30, || list(socket).filter(|l| alive_term(l, "shell"))) else {
        unsafe { libc::kill(app_pid, libc::SIGKILL) };
        return Err(format!("no daemon on {} with a shell", socket.display()));
    };
    let mut client = Client::connect(socket, 0).map_err(|e| e.to_string())?;
    let spawn =
        json!({ "op": "spawn", "id": "repro", "argv": [], "cwd": home, "cols": 80, "rows": 24 });
    client.send(&spawn).map_err(|e| e.to_string())?;
    let before = wait(10, || list(socket).filter(|l| alive_term(l, "repro")))
        .ok_or("the second terminal did not start")?;

    match signal {
        None => {
            let quit = format!("tell application id \"{identifier}\" to quit");
            let _ = Command::new("osascript").args(["-e", &quit]).status();
        }
        Some(sig) => unsafe {
            libc::kill(app_pid, sig);
        },
    }
    let ended = wait(15, || (!alive(app_pid)).then_some(())).is_some();
    if !ended {
        unsafe { libc::kill(app_pid, libc::SIGKILL) };
    }
    // Time for whatever macOS does to the app's coalition to happen.
    thread::sleep(Duration::from_secs(3));

    let daemon_pid = before["daemon_pid"].as_i64().unwrap_or(0) as i32;
    let terms: Vec<Value> = ["shell", "repro"]
        .iter()
        .map(|id| {
            let pid = term(&before, id)["pid"].as_i64().unwrap_or(0) as i32;
            json!({ "id": id, "pid": pid, "alive": alive(pid) })
        })
        .collect();
    let survived = alive(daemon_pid) && terms.iter().all(|t| t["alive"] == true);
    Ok(json!({
        "app_pid": app_pid, "app_ended_by_itself": ended,
        "daemon_pid": daemon_pid, "daemon_alive": alive(daemon_pid),
        "daemon_answers": list(socket).is_some(),
        "terminals": terms, "survived": survived,
    }))
}

fn plist_key(plist: &Path, key: &str) -> Result<String, String> {
    let out = Command::new("plutil")
        .args(["-extract", key, "raw"])
        .arg(plist)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("no {key} in {}", plist.display()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The pid of the process running exactly `exe`.
fn app_pid(exe: &Path) -> Option<i32> {
    let out = Command::new("ps")
        .args(["-axo", "pid=,comm="])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|line| {
            let (pid, comm) = line.trim().split_once(' ')?;
            (Path::new(comm.trim()) == exe).then(|| pid.parse().ok())?
        })
}

fn alive(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) } == 0
}

/// The daemon's `list`, or `None` when no daemon answers on `socket`.
fn list(socket: &Path) -> Option<Value> {
    let mut client = Client::connect(socket, 0).ok()?;
    client.send(&json!({ "op": "list" })).ok()?;
    client.recv_until(|v| v["ev"] == "list").ok()
}

fn term<'a>(list: &'a Value, id: &str) -> &'a Value {
    let terms = list["terms"].as_array().into_iter().flatten();
    terms
        .into_iter()
        .find(|t| t["id"] == id)
        .unwrap_or(&Value::Null)
}

fn alive_term(list: &Value, id: &str) -> bool {
    let t = term(list, id);
    !t.is_null() && t["exited"] != true
}

/// Polls `f` every 200 ms for up to `secs` seconds.
fn wait<T>(secs: u64, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if Instant::now() > deadline {
            return None;
        }
        thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refuses_the_live_home_or_the_live_identifier() {
        let live = Path::new("/Users/d");
        assert!(guard(live, Path::new("/Users/d"), "dev.falinks.studio-repro").is_err());
        assert!(guard(live, Path::new("/Users/d/"), "dev.falinks.studio-repro").is_err());
        assert!(guard(live, Path::new("/tmp/studio-repro-1"), LIVE_ID).is_err());
        assert!(
            guard(
                live,
                Path::new("/tmp/studio-repro-1"),
                "dev.falinks.studio-repro"
            )
            .is_ok()
        );
    }
}
