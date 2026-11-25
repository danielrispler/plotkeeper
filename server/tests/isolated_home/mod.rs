//! Points the test process's home at an empty temporary folder, so nothing a test runs reads or writes Daniel's real
//! `~/Library/Application Support/plotkeeper` (the event log, `usage`, the answer files). Call `set` before anything
//! reads the home; it is safe from every test thread and does its work once.

use std::sync::OnceLock;

static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();

pub(crate) fn set() {
    HOME.get_or_init(|| {
        let home = tempfile::tempdir().unwrap();
        // SAFETY: `set_var` is sound only while no other thread reads or writes the environment. That holds because
        // the tests run under nextest, which runs each test in a process of its own, and the test calls `set` before
        // it starts a thread; `get_or_init` runs this once per process.
        unsafe { std::env::set_var("HOME", home.path()) };
        home
    });
}
