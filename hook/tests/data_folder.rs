//! #322: the data folder moves once from `dev.falinks.studio` to `plotkeeper`, with a link at the old path, on macOS.
//! #700: on Linux the data and runs folders follow XDG, and nothing moves.
use crate::common::{feed, hook, scratch};
use std::{fs, path::Path};

#[cfg(target_os = "macos")]
fn support(home: &Path) -> std::path::PathBuf {
    home.join("Library/Application Support")
}

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
fn the_move_renames_the_old_folder_and_leaves_a_link_once() {
    let home = scratch("move-once");
    let old = support(&home).join("dev.falinks.studio");
    fs::create_dir_all(old.join("checkout")).unwrap();
    fs::write(old.join("events.jsonl"), "a\n").unwrap();

    assert!(studio_hook::move_old_data_dir(&home).unwrap());

    let new = support(&home).join("plotkeeper");
    assert_eq!(fs::read_to_string(new.join("events.jsonl")).unwrap(), "a\n");
    assert!(fs::symlink_metadata(&old).unwrap().is_symlink());
    assert_eq!(fs::read_to_string(old.join("events.jsonl")).unwrap(), "a\n");
    assert!(old.join("checkout").is_dir());
    // A second start moves nothing and leaves the link as it was.
    assert!(!studio_hook::move_old_data_dir(&home).unwrap());
    assert!(fs::symlink_metadata(&old).unwrap().is_symlink());
}

#[cfg(target_os = "macos")]
#[test]
fn the_move_never_goes_over_an_existing_new_folder() {
    let home = scratch("move-over");
    let old = support(&home).join("dev.falinks.studio");
    let new = support(&home).join("plotkeeper");
    fs::create_dir_all(&old).unwrap();
    fs::create_dir_all(&new).unwrap();
    fs::write(old.join("events.jsonl"), "old\n").unwrap();
    fs::write(new.join("events.jsonl"), "new\n").unwrap();

    assert!(!studio_hook::move_old_data_dir(&home).unwrap());

    assert!(!fs::symlink_metadata(&old).unwrap().is_symlink());
    assert_eq!(
        fs::read_to_string(old.join("events.jsonl")).unwrap(),
        "old\n"
    );
    assert_eq!(
        fs::read_to_string(new.join("events.jsonl")).unwrap(),
        "new\n"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn nothing_moves_without_an_old_folder() {
    let home = scratch("move-none");
    assert!(!studio_hook::move_old_data_dir(&home).unwrap());
    assert!(!support(&home).join("dev.falinks.studio").exists());
}

#[cfg(target_os = "macos")]
#[test]
fn a_hook_built_with_the_old_path_still_appends_to_the_same_log() {
    let home = scratch("old-hook");
    let old = support(&home).join("dev.falinks.studio");
    fs::create_dir_all(&old).unwrap();
    studio_hook::move_old_data_dir(&home).unwrap();

    // What a hook built before the change does: append to the log under the old path.
    let event = serde_json::json!({ "hook_event_name": "Stop" });
    assert!(studio_hook::append(&old.join("events.jsonl"), &event, studio_hook::PATIENCE).unwrap());

    let log = fs::read_to_string(support(&home).join("plotkeeper/events.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 1);
}

#[cfg(target_os = "macos")]
#[test]
fn the_hook_moves_the_old_folder_before_it_writes() {
    let home = scratch("hook-moves");
    let old = support(&home).join("dev.falinks.studio");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("events.jsonl"), "{\"before\":1}\n").unwrap();

    let payload = serde_json::json!({ "hook_event_name": "Stop", "session_id": "s" });
    assert!(feed(hook(&home), &payload).wait().unwrap().success());

    let log = fs::read_to_string(support(&home).join("plotkeeper/events.jsonl")).unwrap();
    assert_eq!(log.lines().count(), 2, "{log}");
    assert!(log.starts_with("{\"before\":1}"));
    assert!(fs::symlink_metadata(&old).unwrap().is_symlink());
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

#[cfg(target_os = "linux")]
#[test]
fn on_linux_the_old_folder_is_never_moved() {
    let home = scratch("no-move");
    let old = home.join("Library/Application Support/dev.falinks.studio");
    fs::create_dir_all(&old).unwrap();
    assert!(!studio_hook::move_old_data_dir(&home).unwrap());
    assert!(!fs::symlink_metadata(&old).unwrap().is_symlink());
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
        cmd.env("HOME", &home).env_remove("PLOTKEEPER_TERMINAL_ID");
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
