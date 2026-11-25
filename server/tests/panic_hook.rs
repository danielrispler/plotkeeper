//! A panic stops plotkeeper (danielrispler/plotkeeper#551, spec #548): the hook the binary installs logs what panicked and
//! exits non-zero. Its own test binary with one test, as the hook and the logger are the whole process's.

use std::sync::{Arc, Mutex};

static LOGGED: Mutex<Vec<(log::Level, String)>> = Mutex::new(Vec::new());

struct Recording;

impl log::Log for Recording {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        studio_server::logged(metadata)
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let line = record.args().to_string();
            LOGGED.lock().unwrap().push((record.level(), line));
        }
    }
    fn flush(&self) {}
}

#[test]
fn a_panic_on_any_thread_is_logged_and_exits_non_zero() {
    log::set_logger(&Recording).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let exits = Arc::new(Mutex::new(Vec::new()));
    let recorded = exits.clone();
    studio_server::exit_on_panic(move |code| recorded.lock().unwrap().push(code));

    let thread = std::thread::Builder::new().name("follower".into());
    let panicked = thread.spawn(|| panic!("the log is gone")).unwrap().join();

    assert!(panicked.is_err());
    let exits = exits.lock().unwrap();
    assert_eq!(exits.len(), 1, "{exits:?}");
    assert_ne!(exits[0], 0);
    let logged = LOGGED.lock().unwrap().clone();
    let [(level, line)] = &logged[..] else {
        panic!("one line: {logged:?}")
    };
    assert_eq!(*level, log::Level::Error);
    assert!(line.contains("follower"), "{line}");
    assert!(line.contains("the log is gone"), "{line}");
    assert!(line.contains("panic_hook.rs"), "{line}");
}
