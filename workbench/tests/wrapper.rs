//! danielrispler/Falinks#521 (danielrispler/Falinks#517, decision 5): the status line wrapper of sessions the studio starts, as a black box.
//! It gets the status line's JSON on stdin and a fake `HOME` holding a fake Claude Code settings
//! file; it saves the snapshot, then runs the user's own `statusLine.command`. Needs `bash` and
//! `jq`, as the wrapper does.

use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const WRAPPER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/statusline.sh");
const ID: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";
const STDIN: &str = r#"{"session_id":"9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807","workspace":{"current_dir":"/x"},"context_window":{"context_window_size":1000000}}"#;

/// Writes the user's Claude Code settings in `home`, with `command` as its status line, if any.
fn settings(home: &Path, command: Option<&str>) {
    let dir = home.join(".claude");
    fs::create_dir_all(&dir).unwrap();
    let settings = match command {
        Some(c) => serde_json::json!({ "statusLine": { "type": "command", "command": c } }),
        None => serde_json::json!({ "theme": "dark" }),
    };
    fs::write(dir.join("settings.json"), settings.to_string()).unwrap();
}

fn start(home: &Path) -> Child {
    let mut child = Command::new("bash")
        .arg(WRAPPER)
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(STDIN.as_bytes()).unwrap();
    child
}

/// Runs the wrapper with `home` as `HOME`; returns its stdout, stderr and exit code.
fn run(home: &Path) -> (String, String, Option<i32>) {
    let out = start(home).wait_with_output().unwrap();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (text(out.stdout), text(out.stderr), out.status.code())
}

fn snapshot(home: &Path) -> PathBuf {
    home.join("Library/Application Support/dev.falinks.studio/usage")
        .join(format!("{ID}.json"))
}

fn saved_privately(home: &Path) {
    let path = snapshot(home);
    assert_eq!(fs::read_to_string(&path).unwrap(), STDIN);
    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn the_snapshot_is_saved_privately_and_the_users_status_line_passes_through() {
    let home = tempfile::tempdir().unwrap();
    // Reads the same stdin, and prints to both streams and fails, all of which pass through.
    settings(
        home.path(),
        Some("jq -j .workspace.current_dir; echo ' oops' >&2; exit 3"),
    );

    assert_eq!(run(home.path()), ("/x".into(), " oops\n".into(), Some(3)));
    saved_privately(home.path());
}

#[test]
fn a_changed_command_is_used_on_the_next_run() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), Some("printf one"));
    assert_eq!(run(home.path()).0, "one");

    settings(home.path(), Some("printf two"));
    assert_eq!(run(home.path()).0, "two");
}

#[test]
fn with_no_status_line_it_prints_nothing_and_saves_the_snapshot() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(run(home.path()), (String::new(), String::new(), Some(0)));
    saved_privately(home.path());

    let home = tempfile::tempdir().unwrap();
    settings(home.path(), None);
    assert_eq!(run(home.path()), (String::new(), String::new(), Some(0)));
    saved_privately(home.path());
}

#[test]
fn a_failing_command_still_leaves_the_snapshot() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), Some("no-such-command-521"));

    let (out, _, code) = run(home.path());
    assert_eq!((out.as_str(), code), ("", Some(127)));
    saved_privately(home.path());
}

#[test]
fn a_hanging_command_still_leaves_the_snapshot() {
    let home = tempfile::tempdir().unwrap();
    settings(home.path(), Some("sleep 30"));

    let mut child = start(home.path());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !snapshot(home.path()).exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let hung = child.try_wait().unwrap().is_none();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(hung);
    saved_privately(home.path());
}
