//! Points the test process's home at an empty temporary folder and clears `XDG_DATA_HOME` and `XDG_CACHE_HOME`, so
//! nothing a test runs reads or writes Daniel's real data folder (the event log, `usage`, the answer files) or runs
//! folder, and on Linux both folders, and a child's, sit under a home the test names. Call `set` before anything reads
//! the home; it is safe from every test thread and does its work once.

use std::sync::OnceLock;

static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();

pub(crate) fn set() {
    HOME.get_or_init(|| {
        let home = tempfile::tempdir().unwrap();
        // SAFETY: `set_var` is sound only while no other thread reads or writes the environment. That holds because
        // the tests run under nextest, which runs each test in a process of its own, and the test calls `set` before
        // it starts a thread; `get_or_init` runs this once per process.
        unsafe {
            std::env::set_var("HOME", home.path());
            std::env::remove_var("XDG_DATA_HOME");
            std::env::remove_var("XDG_CACHE_HOME");
        }
        home
    });
}
