//! Points the test process's home at an empty temporary folder, so nothing a test runs reads or writes Daniel's real
//! `~/Library/Application Support/plotkeeper` (the event log, `usage`, the answer files). Call `set` before anything
//! reads the home; it is safe from every test thread and does its work once.

use std::sync::OnceLock;

static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();

pub fn set() {
    HOME.get_or_init(|| {
        let home = tempfile::tempdir().unwrap();
        // SAFETY: `get_or_init` runs this once, before any test thread reads the home, since each test calls `set` first.
        unsafe { std::env::set_var("HOME", home.path()) };
        home
    });
}
