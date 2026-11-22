//! #322: the data folder moves once from `dev.falinks.studio` to `plotkeeper`, with a link at the old path.
use crate::common::{feed, hook, scratch};
use std::{fs, path::Path};

fn support(home: &Path) -> std::path::PathBuf {
    home.join("Library/Application Support")
}

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

#[test]
fn nothing_moves_without_an_old_folder() {
    let home = scratch("move-none");
    assert!(!studio_hook::move_old_data_dir(&home).unwrap());
    assert!(!support(&home).join("dev.falinks.studio").exists());
}

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
