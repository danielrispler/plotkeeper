//! The event log (#328, module 2): one append-only JSON Lines file, the only source of session state.

use std::path::{Path, PathBuf};

/// The log in the studio's data directory, which is Tauri's app data directory for the identifier
/// in `src-tauri/tauri.conf.json`. The hook writes it; the app reads it.
pub fn event_log(home: &Path) -> PathBuf {
    home.join("Library/Application Support/dev.falinks.studio/events.jsonl")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn the_log_lives_in_the_data_directory_of_the_apps_identifier() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
        let identifier = conf["identifier"].as_str().unwrap();
        let data = Path::new("/h/Library/Application Support").join(identifier);
        assert_eq!(super::event_log(Path::new("/h")), data.join("events.jsonl"));
    }
}
