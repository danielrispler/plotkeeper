// The hook binary with a scratch `HOME`: what it keeps of each session's turn in `turns/` does not
// outlive the session.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

fn hook(home: &Path, event: serde_json::Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_studio-hook"))
        .env("HOME", home)
        .env_remove("STUDIO_TERMINAL_ID")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(event.to_string().as_bytes()).unwrap();
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

fn turns(home: &Path) -> PathBuf {
    studio_hook::event_log(home).with_file_name("turns")
}

#[test]
fn a_sessions_turn_file_goes_at_its_end_and_any_older_than_7_days_at_a_start() {
    let home = std::env::temp_dir().join(format!("studio-hook-turns-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    let event = |name: &str, session: &str| serde_json::json!({ "session_id": session, "hook_event_name": name, "cwd": "/", "prompt_id": "p-1" });

    hook(&home, event("UserPromptSubmit", "s-1"));
    assert!(turns(&home).join("s-1.json").exists());
    hook(&home, event("SessionEnd", "s-1"));
    assert!(!turns(&home).join("s-1.json").exists());

    // A session whose end never came: its file is pruned a week later.
    hook(&home, event("UserPromptSubmit", "s-old"));
    hook(&home, event("UserPromptSubmit", "s-new"));
    let old = fs::File::options()
        .write(true)
        .open(turns(&home).join("s-old.json"))
        .unwrap();
    old.set_modified(SystemTime::now() - Duration::from_secs(8 * 24 * 3600))
        .unwrap();
    hook(&home, event("SessionStart", "s-2"));
    let left: Vec<String> = fs::read_dir(turns(&home))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let _ = fs::remove_dir_all(&home);
    assert_eq!(left, ["s-new.json"]);
}
