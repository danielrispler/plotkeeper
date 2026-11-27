//! Where the data and runs folders are: on macOS under `Library/`, on Linux following XDG (#700).
#[cfg(target_os = "linux")]
use crate::common::{feed, hook, scratch};
#[cfg(target_os = "linux")]
use std::fs;
use std::path::Path;

#[cfg(target_os = "macos")]
#[test]
fn the_data_folder_is_plotkeeper() {
    assert_eq!(
        studio_hook::data_dir(Path::new("/h")),
        Path::new("/h/Library/Application Support/plotkeeper")
    );
    assert_eq!(
        studio_hook::event_log(Path::new("/h")),
        Path::new("/h/Library/Application Support/plotkeeper/events.jsonl")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_runs_folder_is_in_the_caches() {
    assert_eq!(
        studio_hook::runs_dir(Path::new("/h")),
        Path::new("/h/Library/Caches/plotkeeper/runs")
    );
}

/// Sets or clears `var` in this test's process.
#[cfg(target_os = "linux")]
fn set_env(var: &str, value: Option<&str>) {
    // SAFETY: `set_var` and `remove_var` are sound only while no other thread reads or writes the environment. That
    // holds because the tests run under nextest, which runs each test in a process of its own, and the tests that call
    // this start no thread.
    unsafe {
        match value {
            Some(v) => std::env::set_var(var, v),
            None => std::env::remove_var(var),
        }
    }
}

/// The event log `plotkeeper-hook` writes on a Stop, with `home` as its `HOME` and `XDG_DATA_HOME` as given.
#[cfg(target_os = "linux")]
fn log_the_hook_writes(home: &Path, xdg_data_home: Option<&Path>) -> std::path::PathBuf {
    let mut cmd = hook(home);
    match xdg_data_home {
        Some(dir) => cmd.env("XDG_DATA_HOME", dir),
        None => cmd.env_remove("XDG_DATA_HOME"),
    };
    let payload = serde_json::json!({ "hook_event_name": "Stop", "session_id": "s" });
    assert!(feed(cmd, &payload).wait().unwrap().success());
    let logs: Vec<_> = walk(home)
        .into_iter()
        .filter(|p| p.ends_with("events.jsonl"))
        .collect();
    assert_eq!(logs.len(), 1, "{logs:?}");
    logs[0].clone()
}

#[cfg(target_os = "linux")]
fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(dir)
        .unwrap()
        .flat_map(|e| {
            let path = e.unwrap().path();
            if path.is_dir() {
                walk(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn on_linux_the_event_log_lands_in_the_xdg_data_folder() {
    let home = scratch("xdg-data");
    assert_eq!(
        log_the_hook_writes(&home, Some(&home.join("data"))),
        home.join("data/plotkeeper/events.jsonl")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn on_linux_without_xdg_data_home_the_event_log_lands_in_local_share() {
    let home = scratch("xdg-data-unset");
    assert_eq!(
        log_the_hook_writes(&home, None),
        home.join(".local/share/plotkeeper/events.jsonl")
    );
    let empty = scratch("xdg-data-empty");
    assert_eq!(
        log_the_hook_writes(&empty, Some(Path::new(""))),
        empty.join(".local/share/plotkeeper/events.jsonl")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn on_linux_the_data_folder_follows_xdg_data_home() {
    set_env("XDG_DATA_HOME", Some("/x/data"));
    assert_eq!(
        studio_hook::data_dir(Path::new("/h")),
        Path::new("/x/data/plotkeeper")
    );
    set_env("XDG_DATA_HOME", Some(""));
    assert_eq!(
        studio_hook::data_dir(Path::new("/h")),
        Path::new("/h/.local/share/plotkeeper")
    );
    set_env("XDG_DATA_HOME", None);
    assert_eq!(
        studio_hook::event_log(Path::new("/h")),
        Path::new("/h/.local/share/plotkeeper/events.jsonl")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn on_linux_the_runs_folder_follows_xdg_cache_home() {
    set_env("XDG_CACHE_HOME", Some("/x/cache"));
    assert_eq!(
        studio_hook::runs_dir(Path::new("/h")),
        Path::new("/x/cache/plotkeeper/runs")
    );
    set_env("XDG_CACHE_HOME", Some(""));
    assert_eq!(
        studio_hook::runs_dir(Path::new("/h")),
        Path::new("/h/.cache/plotkeeper/runs")
    );
    set_env("XDG_CACHE_HOME", None);
    assert_eq!(
        studio_hook::runs_dir(Path::new("/h")),
        Path::new("/h/.cache/plotkeeper/runs")
    );
}

/// On Linux a hook run from its copy in a data folder's `bin/` writes to that data folder, whatever `XDG_DATA_HOME` its
/// agent was started with, as an IDE started from the desktop has none while `plotkeeper` started from a terminal had
/// one: the `plotkeeper` that copied it there reads that folder (#510, "Folders").
#[cfg(target_os = "linux")]
#[test]
fn on_linux_a_hook_copy_writes_to_the_data_folder_it_sits_in() {
    let home = scratch("hook-copy");
    let data = home.join("terminal-data/plotkeeper");
    fs::create_dir_all(data.join("bin")).unwrap();
    let copy = data.join("bin/plotkeeper-hook");
    fs::copy(env!("CARGO_BIN_EXE_plotkeeper-hook"), &copy).unwrap();
    for xdg_data_home in [None, Some(home.join("desktop-data"))] {
        let mut cmd = std::process::Command::new(&copy);
        cmd.env("HOME", &home);
        match &xdg_data_home {
            Some(dir) => cmd.env("XDG_DATA_HOME", dir),
            None => cmd.env_remove("XDG_DATA_HOME"),
        };
        let payload = serde_json::json!({ "hook_event_name": "Stop", "session_id": "s" });
        assert!(feed(cmd, &payload).wait().unwrap().success());
    }
    let logs: Vec<_> = walk(&home)
        .into_iter()
        .filter(|p| p.ends_with("events.jsonl"))
        .collect();
    assert_eq!(logs, [data.join("events.jsonl")]);
    assert_eq!(fs::read_to_string(&logs[0]).unwrap().lines().count(), 2);
}
