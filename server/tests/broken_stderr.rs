//! Logging never panics (spec #548): a stderr that cannot be written, a closed pipe as in `plotkeeper 2>&1 | head -n 1`,
//! loses the line, and an error still reaches the page. Its own test binary with one test, as the logger and fd 2 are the
//! whole process's. libtest's output capture takes `eprintln!`'s line before fd 2 does, so the test shows the broken pipe
//! as nextest runs it, with `--nocapture`.

use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Page(Mutex<Vec<(String, serde_json::Value)>>);

impl studio_server::Sink for Page {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        self.0.lock().unwrap().push((name.to_string(), payload));
    }
    fn focused(&self) -> bool {
        false
    }
    fn replayed(&self) {}
}

#[test]
fn a_line_logged_to_a_closed_stderr_is_lost_without_a_panic() {
    log::set_logger(&studio_server::LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let page = Arc::new(Page::default());
    studio_server::LOGGER.send_errors_to(page.clone());

    // fd 2 becomes a pipe whose reading end is closed, so a write to it fails with a broken pipe.
    let mut ends = [0; 2];
    // SAFETY: plain fd calls on fds this test owns; fd 2 is put back before any assertion can print.
    let saved = unsafe {
        assert_eq!(libc::pipe(ends.as_mut_ptr()), 0);
        libc::close(ends[0]);
        let saved = libc::dup(2);
        libc::dup2(ends[1], 2);
        libc::close(ends[1]);
        saved
    };
    let logged = std::panic::catch_unwind(|| {
        log::warn!("prune the event log: busy");
        log::error!("save the cards: disk full");
    });
    // SAFETY: as above.
    unsafe {
        libc::dup2(saved, 2);
        libc::close(saved);
    }

    assert!(logged.is_ok(), "logging panicked");
    let sent = page.0.lock().unwrap().clone();
    assert_eq!(
        sent,
        [(
            "server-error".to_string(),
            serde_json::json!("save the cards: disk full")
        )]
    );
}
