//! The `plotkeeper` binary as Daniel runs it (danielrispler/plotkeeper#325): it serves on a free port, a second start for the
//! same data folder finds the first and exits, and Ctrl+C stops the first cleanly. The browser stays closed.

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

fn plotkeeper(home: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .env("HOME", home)
        .env("PLOTKEEPER_NO_BROWSER", "1")
        .env("STUDIO_CARD_WRITER", "fake")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
}

/// The address the process printed.
fn address(child: &mut Child) -> String {
    let mut line = String::new();
    BufReader::new(child.stdout.as_mut().unwrap())
        .read_line(&mut line)
        .unwrap();
    line.trim()
        .strip_prefix("plotkeeper: ")
        .unwrap_or_else(|| panic!("no address in {line:?}"))
        .to_string()
}

fn wait(child: &mut Child) -> std::process::ExitStatus {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < until, "the process did not exit");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_second_start_opens_the_first_and_ctrl_c_stops_it() {
    let home = tempfile::tempdir().unwrap();
    let mut first = plotkeeper(home.path());
    let url = address(&mut first);
    assert!(
        url.starts_with("http://127.0.0.1:") && url.contains("/?token="),
        "{url}"
    );

    let mut second = plotkeeper(home.path());
    assert_eq!(address(&mut second), url);
    assert!(wait(&mut second).success());

    let kill = Command::new("kill")
        .args(["-INT", &first.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    assert!(wait(&mut first).success(), "Ctrl+C is a clean stop");
    let data = home.path().join("Library/Application Support/plotkeeper");
    assert!(!data.join("plotkeeper.running.json").exists());
}
