//! danielrispler/Falinks#521 (danielrispler/Falinks#517, decision 5): the status line wrapper of sessions the studio starts, as a black box.
//! It gets the status line's JSON on stdin and a fake `HOME` holding a fake Claude Code settings
//! file; it saves the snapshot, then runs the user's own `statusLine.command`. Needs `bash` and
//! `jq`, as the wrapper does. Beside it, the `claude` command that set it and the snapshot's context window (#416).

use serde_json::json;
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use studio_workbench::{claude, context_window};

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
    home.join("Library/Application Support/plotkeeper/usage")
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

/// danielrispler/Falinks#517, decision 1; danielrispler/Falinks#519: a session's context is its own snapshot's `context_window`.
#[test]
fn a_sessions_snapshot_gives_its_context_window() {
    let dir = tempfile::tempdir().unwrap();
    let window = |size: &str| {
        format!(
            r#"{{"session_id":"a-1","context_window":{{"context_window_size":{size},"used_percentage":13}}}}"#
        )
    };
    fs::write(dir.path().join("a-1.json"), window("1000000")).unwrap();
    fs::write(dir.path().join("b-2.json"), window("200000")).unwrap();

    let a = json!({"context_window_size": 1_000_000, "used_percentage": 13});
    assert_eq!(context_window(dir.path(), "a-1"), Some(a));
    let b = context_window(dir.path(), "b-2").unwrap();
    assert_eq!(b["context_window_size"], json!(200_000));
    // No snapshot, one without the field, or an id that is no file name: none.
    assert_eq!(context_window(dir.path(), "c-3"), None);
    fs::write(dir.path().join("d-4.json"), r#"{"session_id":"d-4"}"#).unwrap();
    assert_eq!(context_window(dir.path(), "d-4"), None);
    let name = dir.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(context_window(dir.path(), &format!("../{name}/a-1")), None);
}

/// The studio's status line wrapper beside the plugin (danielrispler/Falinks#521), quoted for the shell that runs it.
const STATUS_LINE: &str =
    r#"{"statusLine":{"command":"'/r/apps/studio/workbench/statusline.sh'","type":"command"}}"#;

/// `claude` runs with the studio plugin alone, set the way the README loads it, so a profile that
/// also sets `CLAUDE_CODE_PLUGIN_DIRS` never loads its hooks twice. It names the presence file, so
/// Remote Control skips a phone push while the studio is focused (danielrispler/Falinks#522).
#[test]
fn claude_starts_with_the_studio_plugin_and_resumes_by_session_id() {
    let plugin = Path::new("/r/apps/studio/plugin");
    let presence = Path::new("/d/present");

    assert_eq!(
        claude(plugin, presence, None, false),
        [
            "env",
            "CLAUDE_CODE_PLUGIN_DIRS=/r/apps/studio/plugin",
            "CLAUDE_CLIENT_PRESENCE_FILE=/d/present",
            "claude",
            "--settings",
            STATUS_LINE,
        ]
    );
    assert_eq!(
        claude(plugin, presence, Some("9e0b5f13"), false),
        [
            "env",
            "CLAUDE_CODE_PLUGIN_DIRS=/r/apps/studio/plugin",
            "CLAUDE_CLIENT_PRESENCE_FILE=/d/present",
            "claude",
            "--settings",
            STATUS_LINE,
            "--resume",
            "9e0b5f13"
        ]
    );
}

/// Auto mode (danielrispler/Falinks#541) is Claude Code's `--permission-mode auto`, added only when asked, for a new
/// session and for a resumed one, which keeps the mode it was started with. Never bypass.
#[test]
fn claude_runs_in_auto_mode_only_when_asked() {
    let plugin = Path::new("/r/apps/studio/plugin");
    let presence = Path::new("/d/present");
    let tail = |argv: Vec<String>| argv[6..].to_vec();

    assert_eq!(
        tail(claude(plugin, presence, None, true)),
        ["--permission-mode", "auto"]
    );
    assert_eq!(
        tail(claude(plugin, presence, Some("9e0b5f13"), true)),
        ["--permission-mode", "auto", "--resume", "9e0b5f13"]
    );
    assert!(tail(claude(plugin, presence, None, false)).is_empty());
}
