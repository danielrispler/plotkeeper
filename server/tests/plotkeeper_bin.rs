//! The `plotkeeper` binary as Daniel runs it (danielrispler/plotkeeper#325): it serves on a free port, a second start for the
//! same data folder finds the first and exits, and Ctrl+C stops the first cleanly. The browser stays closed.

mod isolated_home;

use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// A start with no harness on the PATH and no terminal, so it neither asks nor wires (#434) and its first line is the
/// address.
fn plotkeeper(home: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .stdin(Stdio::null())
        .env("PLOTKEEPER_NO_BROWSER", "1")
        .env("PLOTKEEPER_CARD_WRITER", "fake")
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
    isolated_home::set();
    let home = tempfile::tempdir().unwrap();
    let mut first = plotkeeper(home.path());
    let url = address(&mut first);
    assert!(
        url.starts_with("http://127.0.0.1:") && url.contains("/?token="),
        "{url}"
    );

    // The running file says the version, for a second start of another version (#439).
    let data = studio_hook::data_dir(home.path());
    let running: serde_json::Value =
        serde_json::from_slice(&std::fs::read(data.join("plotkeeper.running.json")).unwrap())
            .unwrap();
    assert_eq!(running["version"], env!("CARGO_PKG_VERSION"));

    let mut second = plotkeeper(home.path());
    assert_eq!(address(&mut second), url);
    assert!(wait(&mut second).success());

    let kill = Command::new("kill")
        .args(["-INT", &first.id().to_string()])
        .status()
        .unwrap();
    assert!(kill.success());
    assert!(wait(&mut first).success(), "Ctrl+C is a clean stop");
    assert!(!data.join("plotkeeper.running.json").exists());
}

/// `plotkeeper --version` says the npm package's version (#368), the one `npm i -g plotkeeper` installed, and touches
/// nothing in the home folder.
#[test]
fn version_is_the_npm_packages() {
    isolated_home::set();
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .arg("--version")
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let package: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../npm/package.json"))
            .unwrap(),
    )
    .unwrap();
    let expected = format!("plotkeeper {}\n", package["version"].as_str().unwrap());
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}

/// Holds the data folder's lock as a running plotkeeper would, with `running` as its running file, and starts a second
/// plotkeeper against it.
fn second_start_against(running: serde_json::Value) -> std::process::Output {
    isolated_home::set();
    let home = tempfile::tempdir().unwrap();
    let data = studio_hook::data_dir(home.path());
    std::fs::create_dir_all(&data).unwrap();
    let lock = std::fs::File::create(data.join("plotkeeper.lock")).unwrap();
    lock.try_lock().unwrap();
    std::fs::write(data.join("plotkeeper.running.json"), running.to_string()).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .env("HOME", home.path())
        .env("PLOTKEEPER_NO_BROWSER", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "plotkeeper: http://127.0.0.1:4321/?token=abc\n",
        "the page is opened"
    );
    out
}

/// A second start against an older server says so and still opens its page (#439); it never stops it.
#[test]
fn a_second_start_against_another_version_says_so() {
    let out = second_start_against(serde_json::json!({
        "port": 4321, "token": "abc", "version": "0.0.1"
    }));
    let expected = format!(
        "plotkeeper 0.0.1 is still running; stop it (Ctrl+C) and run again to use {}\n",
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr), expected);
}

#[test]
fn a_second_start_against_the_same_version_says_nothing() {
    let out = second_start_against(serde_json::json!({
        "port": 4321, "token": "abc", "version": env!("CARGO_PKG_VERSION")
    }));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

/// A running file from before #439 has no version: the page opens as before, with no notice.
#[test]
fn a_second_start_against_a_running_file_without_a_version_opens_the_page() {
    let out = second_start_against(serde_json::json!({ "port": 4321, "token": "abc" }));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

/// A first start waiting on its question holds the lock but has no address yet (#434): a second start says the first
/// may be waiting for an answer in its terminal.
#[test]
fn a_second_start_while_the_first_has_no_address_says_it_may_be_waiting_for_an_answer() {
    isolated_home::set();
    let home = tempfile::tempdir().unwrap();
    let data = studio_hook::data_dir(home.path());
    std::fs::create_dir_all(&data).unwrap();
    let lock = std::fs::File::create(data.join("plotkeeper.lock")).unwrap();
    lock.try_lock().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .env("HOME", home.path())
        .env("PLOTKEEPER_NO_BROWSER", "1")
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("waiting for an answer in its terminal"),
        "{text}"
    );
}
