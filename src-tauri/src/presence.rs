//! The presence file for Remote Control (danielrispler/Falinks#522). Claude Code skips a phone push while the file named
//! by `CLAUDE_CLIENT_PRESENCE_FILE` exists, and reads nothing from it. The studio's terminals set
//! it to `path()`; the shell keeps the file while the window is focused, and deletes it on blur, on
//! quit and at start, for a file a crash left. After a crash the daemon deletes it once the app's
//! connection drops (danielrispler/Falinks#555), unless
//! the daemon dropped it for falling behind.

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

/// At the app's start: deletes a file a crash left.
pub fn start(path: &Path) {
    set(path, false);
}

/// On a window event: focus creates the file, blur deletes it.
pub fn window_event(path: &Path, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::Focused(focused) = event {
        set(path, *focused);
    }
}

/// On a run event: the app's exit deletes the file.
pub fn run_event(path: &Path, event: &tauri::RunEvent) {
    if let tauri::RunEvent::Exit = event {
        set(path, false);
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

    /// The shell's wiring: start deletes a file a crash left, focus creates it, blur and the app's
    /// exit delete it, and other events leave it.
    #[test]
    fn start_focus_blur_and_exit_set_the_file() {
        use tauri::{RunEvent, WindowEvent};
        let path = std::env::temp_dir().join(format!("studio-wired-{}", std::process::id()));
        std::fs::write(&path, "").unwrap();

        super::start(&path);
        assert!(!path.exists());
        super::window_event(&path, &WindowEvent::Focused(true));
        assert!(path.exists());
        super::window_event(&path, &WindowEvent::Destroyed);
        assert!(path.exists());
        super::window_event(&path, &WindowEvent::Focused(false));
        assert!(!path.exists());
        super::window_event(&path, &WindowEvent::Focused(true));
        super::run_event(&path, &RunEvent::Ready);
        assert!(path.exists());
        super::run_event(&path, &RunEvent::Exit);
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
