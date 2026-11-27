//! The `plotkeeper` binary as Daniel runs it (danielrispler/plotkeeper#325): it serves on a free port, a second start for the
//! same data folder finds the first and exits, and Ctrl+C stops the first cleanly. The browser stays closed.

mod isolated_home;

use studio_test_support::git;

use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

/// A start with no harness on the PATH and no terminal, so it neither asks nor wires (#434) and its first line is the
/// address. It starts in the home folder, which is in no git repository, so it adds no project.
fn plotkeeper(home: &std::path::Path) -> Child {
    plotkeeper_in(home, home)
}

/// [`plotkeeper`] started in `folder`.
fn plotkeeper_in(home: &std::path::Path, folder: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .current_dir(folder)
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
        .current_dir(home.path())
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
        .current_dir(home.path())
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

/// The saved projects of the plotkeeper serving `url`, read through its API with the token the address carries.
fn saved_projects(url: &str) -> Vec<String> {
    let rest = url.strip_prefix("http://127.0.0.1:").unwrap();
    let (port, rest) = rest.split_once('/').unwrap();
    let token = rest.strip_prefix("?token=").unwrap();
    let token = token.split('#').next().unwrap();
    let mut stream = std::net::TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    let request = format!(
        "POST /api/projects HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: plotkeeper={token}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).unwrap();
    let body = raw.split_once("\r\n\r\n").unwrap().1;
    let list: Vec<serde_json::Value> = serde_json::from_str(body).unwrap();
    list.iter()
        .map(|p| p["path"].as_str().unwrap().to_string())
        .collect()
}

/// A repository with one commit in a new folder under `base`, spelled as `git` spells it.
fn repo_in(base: &std::path::Path, name: &str) -> std::path::PathBuf {
    let dir = base.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "Start"]);
    dir.canonicalize().unwrap()
}

/// Start-up registration (spec #753, "Projects"): a start in a repository adds it and opens the page on its map; a second start,
/// in a task's worktree of another, adds that one's main checkout through the running server and opens the page on it, not on the
/// one shown last. The repositories are under `/var/tmp`, as the system's temporary folders are never saved.
#[test]
fn a_start_in_a_repository_adds_it_and_opens_the_page_on_its_map() {
    isolated_home::set();
    let home = tempfile::tempdir().unwrap();
    let base = tempfile::tempdir_in("/var/tmp").unwrap();
    let first_repo = repo_in(base.path(), "first");
    let second_repo = repo_in(base.path(), "second");
    git(
        &second_repo,
        &[
            "worktree",
            "add",
            "-q",
            ".claude/worktrees/task",
            "-b",
            "task",
        ],
    );
    let opens_on = |url: &str, repo: &std::path::Path| {
        let (_, project) = url
            .split_once("#project=")
            .unwrap_or_else(|| panic!("{url}"));
        assert_eq!(project, repo.display().to_string(), "{url}");
    };

    let mut first = Stopped(plotkeeper_in(home.path(), &first_repo));
    let url = address(&mut first.0);
    opens_on(&url, &first_repo);
    assert_eq!(saved_projects(&url), [first_repo.display().to_string()]);

    let mut second = plotkeeper_in(home.path(), &second_repo.join(".claude/worktrees/task"));
    let second_url = address(&mut second);
    assert!(wait(&mut second).success());
    assert!(
        second_url.starts_with(url.split('#').next().unwrap()),
        "{second_url}"
    );
    opens_on(&second_url, &second_repo);
    assert_eq!(
        saved_projects(&url),
        [
            first_repo.display().to_string(),
            second_repo.display().to_string()
        ]
    );
}

/// A running plotkeeper, stopped by Ctrl+C when the test ends, passed or failed.
struct Stopped(Child);

impl Drop for Stopped {
    fn drop(&mut self) {
        Command::new("kill")
            .args(["-INT", &self.0.id().to_string()])
            .status()
            .ok();
        wait(&mut self.0);
    }
}
