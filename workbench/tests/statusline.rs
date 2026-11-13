//! danielrispler/Falinks#481: the one line added to Daniel's status line script, right after `input=$(cat)`, saves the
//! status line's stdin as the session's usage snapshot and leaves what the status line prints as it
//! was. Needs `bash` and `jq`, as the script does.

use std::{
    fs, io::Write, os::unix::fs::PermissionsExt, path::Path, process::Command, process::Stdio,
};
use studio_workbench::plan_usage;

const LINE: &str = include_str!("../statusline-usage.sh");

const STDIN: &str = r#"{"session_id":"9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807","workspace":{"current_dir":"/x"},"rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":1738425600},"seven_day":{"used_percentage":41.2,"resets_at":1738857600}}}"#;

/// A status line script shaped like Daniel's: read stdin, then print a line built from it.
fn script(with_line: bool) -> String {
    let line = if with_line { LINE } else { "" };
    format!(
        "input=$(cat)\n{line}\nprintf '%s' \"$(echo \"$input\" | jq -r .workspace.current_dir)\"\n"
    )
}

/// Runs `script` with `stdin` and `home` as `HOME`; returns stdout, stderr and the exit code.
fn run(script: &str, stdin: &str, home: &Path) -> (String, String, Option<i32>) {
    let mut child = Command::new("bash")
        .args(["-c", script])
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let text = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (text(out.stdout), text(out.stderr), out.status.code())
}

fn usage_dir(home: &Path) -> std::path::PathBuf {
    home.join("Library/Application Support/dev.falinks.studio/usage")
}

#[test]
fn the_line_saves_the_snapshot_and_leaves_the_output_unchanged() {
    let home = tempfile::tempdir().unwrap();

    let before = run(&script(false), STDIN, home.path());
    let after = run(&script(true), STDIN, home.path());

    assert_eq!(after, before);
    let dir = usage_dir(home.path());
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, ["9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807.json"]);
    assert_eq!(fs::read_to_string(dir.join(&names[0])).unwrap(), STDIN);
    // Private to its user, like the studio's other files.
    let mode = fs::metadata(dir.join(&names[0]))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(
        plan_usage(&dir).unwrap()["five_hour"]["used_percentage"],
        23.5
    );
}

#[test]
fn a_usage_folder_open_to_others_is_made_private() {
    let home = tempfile::tempdir().unwrap();
    let dir = usage_dir(home.path());
    fs::create_dir_all(&dir).unwrap();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

    run(&script(true), STDIN, home.path());

    let mode = fs::metadata(&dir).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn a_bad_session_id_or_an_unwritable_folder_changes_nothing_either() {
    let home = tempfile::tempdir().unwrap();
    let escape = STDIN.replace("9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807", "../../x");
    assert_eq!(
        run(&script(true), &escape, home.path()),
        run(&script(false), &escape, home.path())
    );
    assert!(!usage_dir(home.path()).exists());

    // A file where the folder should be: mkdir fails, quietly.
    let blocked = home
        .path()
        .join("Library/Application Support/dev.falinks.studio");
    fs::create_dir_all(blocked.parent().unwrap()).unwrap();
    fs::write(&blocked, "").unwrap();
    assert_eq!(
        run(&script(true), STDIN, home.path()),
        run(&script(false), STDIN, home.path())
    );
}
