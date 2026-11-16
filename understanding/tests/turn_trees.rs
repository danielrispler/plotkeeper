//! Seam 2 (danielrispler/plotkeeper#102, Testing Decisions) for a turn's files
//! (danielrispler/plotkeeper#111): real temporary git repositories, with the hook's own trees.

use serde_json::json;
use std::{fs, path::Path, process::Command, sync::Mutex};
use studio_core::Studio;
use studio_understanding::{hunk, numstat, turn_diffs};

const SESSION: &str = "5b1c7e30-9d2a-4f8b-8c63-1e0a7d4b9f52";
const AT: u64 = 1_800_000_000_000;

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8(output.stdout).unwrap().trim_end().into()
}

/// A repository with `a.rs`, `b.rs` and `old.rs` committed; its root and the tree of its files now.
fn repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q"]);
    for (name, text) in [
        ("a.rs", "fn a() {}\n"),
        ("b.rs", "fn b() {}\n"),
        ("old.rs", "fn old() {\n    1\n}\n"),
    ] {
        fs::write(dir.path().join(name), text).unwrap();
    }
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-qm", "base"]);
    let root = fs::canonicalize(dir.path()).unwrap().display().to_string();
    (dir, root)
}

/// Hook trees are written through a scratch index named by the process, which the tests share.
static HOOK: Mutex<()> = Mutex::new(());

/// The tree of `root`'s files as the hook records it.
fn tree(root: &str) -> String {
    let _one = HOOK.lock().unwrap_or_else(|e| e.into_inner());
    studio_hook::record_tree(Path::new(root)).unwrap().1
}

fn line(name: &str, ms: u64, extra: serde_json::Value) -> String {
    let mut event = json!({
        "session_id": SESSION, "hook_event_name": name, "cwd": "/", "prompt_id": "p1",
        "studio_received_ms": AT + ms,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
}

#[test]
fn a_turns_files_equal_numstat_between_its_trees_for_what_its_agent_edited() {
    let (_dir, root) = repo();
    let start = tree(&root);
    // The agent edits a.rs and renames old.rs; Daniel edits b.rs; a binary file arrives too.
    fs::write(format!("{root}/a.rs"), "fn a() {}\nfn a2() {}\n").unwrap();
    fs::rename(format!("{root}/old.rs"), format!("{root}/new.rs")).unwrap();
    fs::write(format!("{root}/b.rs"), "fn b() {}\nfn b2() {}\n").unwrap();
    fs::write(format!("{root}/logo.png"), [0u8, 159, 146, 150, 0, 1]).unwrap();
    let end = tree(&root);

    let stats = numstat(Path::new(&root), &start, &end).unwrap();
    let find = |path: &str| stats.iter().find(|s| s.path == path).unwrap();
    assert_eq!(find("a.rs").added, Some(1));
    assert_eq!(find("new.rs").from.as_deref(), Some("old.rs"));
    assert_eq!(find("logo.png").added, None);

    let mut studio = Studio::replayed([
        line(
            "UserPromptSubmit",
            0,
            json!({ "prompt": "go", "studio_turn_start": { &root: &start } }),
        ),
        line(
            "PostToolUse",
            1_000,
            json!({
                "tool_name": "Edit", "tool_input": { "file_path": format!("{root}/a.rs") },
                "tool_use_id": "e1", "tool_response": {},
            }),
        ),
        line(
            "PostToolUse",
            2_000,
            json!({
                "tool_name": "Write", "tool_input": { "file_path": format!("{root}/new.rs") },
                "tool_use_id": "e2", "tool_response": {},
            }),
        ),
        line(
            "PostToolUse",
            3_000,
            json!({
                "tool_name": "Write", "tool_input": { "file_path": format!("{root}/logo.png") },
                "tool_use_id": "e3", "tool_response": {},
            }),
        ),
        line("Stop", 9_000, json!({ "studio_turn_end": { &root: &end } })),
    ]);
    let asks = studio.turn_diffs(AT + 20_000, 10);
    studio.set_turn_files(&asks[0], turn_diffs(&asks[0]));
    let files = studio.session(SESSION).unwrap().turns[0]
        .files
        .clone()
        .unwrap();
    let paths: Vec<&str> = files.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths.len(), 3);
    assert!(paths.contains(&"a.rs") && paths.contains(&"new.rs") && paths.contains(&"logo.png"));
    assert_eq!(files.external, 1, "b.rs is Daniel's");
    let renamed = files.files.iter().find(|f| f.path == "new.rs").unwrap();
    assert_eq!(renamed.from.as_deref(), Some("old.rs"));
    let binary = files.files.iter().find(|f| f.path == "logo.png").unwrap();
    assert_eq!((binary.added, binary.removed), (None, None));
}

#[test]
fn a_turns_hunks_are_still_readable_after_git_gc() {
    let (_dir, root) = repo();
    let start = tree(&root);
    fs::write(format!("{root}/a.rs"), "fn a() {}\nfn a2() {}\n").unwrap();
    let end = tree(&root);
    let mut studio = Studio::replayed([
        line(
            "UserPromptSubmit",
            0,
            json!({ "prompt": "go", "studio_turn_start": { &root: &start } }),
        ),
        line("Stop", 9_000, json!({ "studio_turn_end": { &root: &end } })),
    ]);
    let asks = studio.turn_diffs(AT + 20_000, 10);
    // Asking anchors the ended turn's trees.
    let _one = HOOK.lock().unwrap_or_else(|e| e.into_inner());
    assert!(turn_diffs(&asks[0]).is_some());
    drop(_one);

    git(Path::new(&root), &["gc", "--prune=now", "-q"]);

    let hunk = hunk(Path::new(&root), &start, &end, "a.rs", None).unwrap();
    assert!(hunk.contains("+fn a2() {}"), "{hunk}");
}

#[test]
fn trees_that_were_never_anchored_do_not_survive_git_gc() {
    // The reason for the anchors: the hook's trees alone are loose objects, pruned at once.
    let (_dir, root) = repo();
    let start = tree(&root);
    fs::write(format!("{root}/a.rs"), "fn a() {}\nfn a2() {}\n").unwrap();
    let end = tree(&root);
    git(Path::new(&root), &["gc", "--prune=now", "-q"]);
    assert!(hunk(Path::new(&root), &start, &end, "a.rs", None).is_none());
}
