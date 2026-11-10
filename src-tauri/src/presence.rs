//! The presence file for Remote Control (#522). Claude Code skips a phone push while the file named
//! by `CLAUDE_CLIENT_PRESENCE_FILE` exists, and reads nothing from it. The studio's terminals set
//! it to `path()`; the shell keeps the file while the window is focused, and deletes it on blur, on
//! quit and at start, for a file a crash left. After a crash the daemon deletes it once the app's
//! connection drops (#555),
//! unless the daemon dropped it for falling behind.

use std::{
    fs,
    path::{Path, PathBuf},
};

/// `present` in the studio's data folder, the one `studio_terminal_daemon::presence` names.
pub fn path() -> Option<PathBuf> {
    Some(studio_hook::data_dir(&std::env::home_dir()?).join("present"))
}

/// Creates the file at `path` while the window is `focused`, else deletes it.
pub fn set(path: &Path, focused: bool) {
    let done = if focused {
        fs::write(path, "")
    } else {
        fs::remove_file(path).or_else(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(e),
        })
    };
    if let Err(e) = done {
        eprintln!("studio: presence file {}: {e}", path.display());
    }
}

/// Sets the file from the window's focus, or deletes it with no window.
pub fn focus(focused: bool) {
    if let Some(path) = path() {
        set(&path, focused);
    }
}

#[cfg(test)]
mod tests {
    use super::set;

    #[test]
    fn focus_creates_the_file_and_blur_deletes_it() {
        let path = std::env::temp_dir().join(format!("studio-present-{}", std::process::id()));

        set(&path, true);
        assert!(path.exists());
        set(&path, true);
        assert!(path.exists());
        set(&path, false);
        assert!(!path.exists());
        // At start, with no file left, deleting it is no error.
        set(&path, false);
        assert!(!path.exists());
    }

    #[test]
    fn the_daemon_deletes_this_same_file() {
        let socket = studio_terminal_daemon::default_socket();
        assert_eq!(
            super::path(),
            Some(studio_terminal_daemon::presence(&socket))
        );
    }
}
