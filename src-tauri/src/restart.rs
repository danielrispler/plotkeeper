//! Restarts (#470): one app instance at a time, a "restart into the new build" step, and the core's
//! answer on the daemon's terminals, idle and how each comes back (`Studio::revival`).

use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
};
use studio_core::{Revive, Studio, Terminal};
use tauri::{AppHandle, Emitter, Manager, State};

/// Set once the read loop has replayed the event log, so the core knows every session's terminal.
pub static REPLAYED: AtomicBool = AtomicBool::new(false);

/// How each terminal of the daemon's `list`, or of a dead daemon's record, comes back, and what runs
/// in it now; after the event log's replay.
#[tauri::command(async)]
pub fn revival(terms: Vec<Terminal>, studio: State<Mutex<Studio>>) -> Vec<Revive> {
    while !REPLAYED.load(Ordering::Acquire) {
        thread::sleep(Duration::from_millis(50));
    }
    studio.lock().unwrap().revival(&terms)
}

/// A second instance hands over and exits; this one comes forward and, when the second was another
/// build, offers to restart into it.
pub fn second_instance(app: &AppHandle, args: Vec<String>, cwd: String) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let Some(exe) = args.first().map(|a| Path::new(&cwd).join(a)) else {
        return;
    };
    let ours = std::env::current_exe().ok();
    let same = |a: &Path, b: &Path| a.canonicalize().ok() == b.canonicalize().ok();
    if ours.as_deref().is_none_or(|ours| !same(&exe, ours)) || modified(&exe) != *BUILT {
        let _ = app.emit("new-build", exe);
    }
}

/// This executable's modification time at start.
static BUILT: LazyLock<Option<SystemTime>> =
    LazyLock::new(|| modified(&std::env::current_exe().ok()?));

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Offers the new build when a rebuild replaces this executable, as `npm run tauri build` does to
/// the bundle, which `open` would only bring forward.
// shortcut: polls every 2 s; switch to file-system events if it ever matters.
pub fn watch_build(app: AppHandle) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    // Read now, before any rebuild.
    LazyLock::force(&BUILT);
    thread::spawn(move || {
        let mut seen = *BUILT;
        loop {
            thread::sleep(Duration::from_secs(2));
            let now = modified(&exe);
            if now.is_some() && now != seen {
                seen = now;
                let _ = app.emit("new-build", &exe);
            }
        }
    });
}

/// Quits and starts `exe` once this process is gone, so its single-instance check finds none.
#[tauri::command]
pub fn restart_into(exe: PathBuf, app: AppHandle) -> Result<(), String> {
    let pid = std::process::id().to_string();
    let wait = r#"while kill -0 "$1" 2>/dev/null; do sleep 0.1; done; exec "$2""#;
    Command::new("/bin/sh")
        .args(["-c", wait, "sh", &pid])
        .arg(&exe)
        .spawn()
        .map_err(|e| format!("start {}: {e}", exe.display()))?;
    app.exit(0);
    Ok(())
}
