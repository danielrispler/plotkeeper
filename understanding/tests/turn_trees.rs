//! Seam 2 (danielrispler/plotkeeper#102, Testing Decisions) for a turn's files
//! (danielrispler/plotkeeper#111): real temporary git repositories, with the hook's own trees.

use serde_json::json;
use std::{fs, path::Path, process::Command, sync::Mutex};
use studio_core::Studio;
use studio_understanding::{hunk, numstat, turn_diffs, turn_hunk};

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

#[test]
fn a_file_hunk_equals_git_diff_between_the_trees_and_is_capped() {
    let (_dir, root) = repo();
    let start = tree(&root);
    let long: String = (0..60).map(|i| format!("line {i}\n")).collect();
    fs::write(format!("{root}/a.rs"), format!("fn a() {{}}\n{long}")).unwrap();
    fs::write(format!("{root}/b.rs"), "fn b() {}\nfn b2() {}\n").unwrap();
    let end = tree(&root);
    let root_path = Path::new(&root);

    let small = turn_hunk(root_path, &start, Some(&end), "b.rs", None, false).unwrap();
    let diff = git(
        root_path,
        &["diff", "--no-color", &start, &end, "--", "b.rs"],
    );
    assert_eq!(small.text.trim_end(), diff);
    assert_eq!(
        (small.left_out, small.binary, small.first_line),
        (0, false, 2)
    );

    let big = turn_hunk(root_path, &start, Some(&end), "a.rs", None, false).unwrap();
    let all = git(
        root_path,
        &["diff", "--no-color", &start, &end, "--", "a.rs"],
    );
    let lines: Vec<&str> = all.lines().collect();
    assert_eq!(big.text.lines().count(), 40);
    assert_eq!(big.text.lines().collect::<Vec<_>>(), lines[..40]);
    assert_eq!(big.left_out, lines.len() - 40);
}

#[test]
fn a_renamed_file_hunk_reads_through_its_old_path_and_a_binary_file_has_no_lines() {
    let (_dir, root) = repo();
    fs::write(format!("{root}/img.bin"), [0u8, 1, 2, 0]).unwrap();
    let start = tree(&root);
    git(Path::new(&root), &["mv", "old.rs", "new.rs"]);
    fs::write(format!("{root}/new.rs"), "fn old() {\n    2\n}\n").unwrap();
    fs::write(format!("{root}/img.bin"), [0u8, 9, 9, 0]).unwrap();
    let end = tree(&root);
    let root_path = Path::new(&root);

    let renamed = turn_hunk(
        root_path,
        &start,
        Some(&end),
        "new.rs",
        Some("old.rs"),
        false,
    )
    .unwrap();
    assert!(
        renamed.text.contains("rename from old.rs"),
        "{}",
        renamed.text
    );
    assert!(renamed.text.contains("+    2"));
    let binary = turn_hunk(root_path, &start, Some(&end), "img.bin", None, false).unwrap();
    assert!(binary.binary && binary.text.is_empty());
}

#[test]
fn a_running_turns_hunk_ends_at_the_worktree_now() {
    let (_dir, root) = repo();
    let start = tree(&root);
    fs::write(format!("{root}/a.rs"), "fn a() {}\nfn live() {}\n").unwrap();
    let _one = HOOK.lock().unwrap_or_else(|e| e.into_inner());
    let hunk = turn_hunk(Path::new(&root), &start, None, "a.rs", None, false).unwrap();
    assert!(hunk.text.contains("+fn live() {}"));
}

#[test]
fn the_whole_hunk_is_git_diff_between_the_trees_with_nothing_left_out() {
    let (_dir, root) = repo();
    let start = tree(&root);
    let long: String = (0..60).map(|i| format!("line {i}\n")).collect();
    fs::write(format!("{root}/a.rs"), format!("fn a() {{}}\n{long}")).unwrap();
    let end = tree(&root);
    let root_path = Path::new(&root);

    let whole = turn_hunk(root_path, &start, Some(&end), "a.rs", None, true).unwrap();
    let diff = git(
        root_path,
        &["diff", "--no-color", &start, &end, "--", "a.rs"],
    );
    assert_eq!(whole.text.trim_end(), diff);
    assert_eq!(whole.left_out, 0);
}

/// Runs hook event `event` through the hook's tree recording, with its state in `data`, and
/// returns the line it logs.
fn hooked(data: &Path, ms: u64, mut event: serde_json::Value) -> String {
    event["session_id"] = SESSION.into();
    let event = event.as_object_mut().unwrap();
    {
        let _one = HOOK.lock().unwrap_or_else(|e| e.into_inner());
        studio_hook::trees(event, data).unwrap();
    }
    event.insert("studio_received_ms".into(), (AT + ms).into());
    serde_json::Value::from(event.clone()).to_string()
}

#[test]
fn a_workers_edits_in_its_own_worktree_count_in_its_turn_even_after_the_turn_ended() {
    let (_dir, root) = repo();
    let outside = tempfile::tempdir().unwrap();
    let path = outside.path().join("w1");
    git(
        Path::new(&root),
        &["worktree", "add", "-q", "-b", "w1", path.to_str().unwrap()],
    );
    let worker = fs::canonicalize(&path).unwrap().display().to_string();
    let data = tempfile::tempdir().unwrap();
    let data = data.path();
    let own = |name: &str, prompt: &str, extra: serde_json::Value| {
        let mut event = json!({ "hook_event_name": name, "cwd": &root, "prompt_id": prompt });
        event
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        event
    };
    let by_worker = |name: &str, prompt: &str, extra: serde_json::Value| {
        let mut event = json!({ "hook_event_name": name, "cwd": &worker, "prompt_id": prompt,
                                "agent_id": "agent-1" });
        event
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        event
    };
    let write = |id: &str, file: &str| {
        json!({ "tool_name": "Write", "tool_input": { "file_path": format!("{worker}/{file}") },
                "tool_use_id": id, "tool_response": {} })
    };
    let running = json!({ "background_tasks": [{ "id": "agent-1", "type": "subagent" }] });

    let mut lines = vec![
        hooked(
            data,
            0,
            own("UserPromptSubmit", "p1", json!({ "prompt": "go" })),
        ),
        hooked(data, 500, by_worker("SubagentStart", "p1", json!({}))),
    ];
    fs::write(format!("{worker}/a.rs"), "fn a() {}\nfn during() {}\n").unwrap();
    lines.push(hooked(
        data,
        1_000,
        by_worker("PostToolUse", "p1", write("w-1", "a.rs")),
    ));
    lines.push(hooked(data, 9_000, own("Stop", "p1", running)));
    // The worker runs on in the background into the next turn.
    lines.push(hooked(
        data,
        10_000,
        own("UserPromptSubmit", "p2", json!({ "prompt": "next" })),
    ));
    fs::write(format!("{worker}/late.rs"), "fn late() {}\n").unwrap();
    lines.push(hooked(
        data,
        12_000,
        by_worker("PostToolUse", "p2", write("w-2", "late.rs")),
    ));
    lines.push(hooked(
        data,
        15_000,
        by_worker("SubagentStop", "p2", json!({})),
    ));
    lines.push(hooked(
        data,
        20_000,
        own("Stop", "p2", json!({ "background_tasks": [] })),
    ));

    let mut studio = Studio::replayed(&lines);
    for ask in studio.turn_diffs(AT + 40_000, 10) {
        studio.set_turn_files(&ask, turn_diffs(&ask));
    }
    let turns = &studio.session(SESSION).unwrap().turns;
    let files = |turn: usize| {
        let found = turns[turn].files.clone().unwrap_or_default();
        let paths: Vec<(String, String)> = (found.files.iter())
            .map(|f| (f.root.clone(), f.path.clone()))
            .collect();
        (paths, found.external)
    };
    let theirs = |path: &str| (worker.clone(), path.to_owned());
    assert_eq!(files(0), (vec![theirs("a.rs"), theirs("late.rs")], 0));
    assert_eq!(files(1), (vec![], 0));
}

#[test]
fn a_workers_late_edit_in_the_sessions_worktree_is_its_turns_and_not_external_in_the_next() {
    let (_dir, root) = repo();
    let data = tempfile::tempdir().unwrap();
    let data = data.path();
    let event = |name: &str, prompt: &str, extra: serde_json::Value| {
        let mut event = json!({ "hook_event_name": name, "cwd": &root, "prompt_id": prompt });
        event
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        event
    };
    let worker = json!({ "agent_id": "agent-1" });
    let mut lines = vec![
        hooked(
            data,
            0,
            event("UserPromptSubmit", "p1", json!({ "prompt": "go" })),
        ),
        hooked(data, 500, event("SubagentStart", "p1", worker.clone())),
        hooked(
            data,
            9_000,
            event(
                "Stop",
                "p1",
                json!({ "background_tasks": [{ "id": "agent-1" }] }),
            ),
        ),
        hooked(
            data,
            10_000,
            event("UserPromptSubmit", "p2", json!({ "prompt": "next" })),
        ),
    ];
    fs::write(format!("{root}/late.rs"), "fn late() {}\n").unwrap();
    let write = json!({ "agent_id": "agent-1", "tool_name": "Write", "tool_use_id": "w-1",
                        "tool_input": { "file_path": format!("{root}/late.rs") }, "tool_response": {} });
    lines.push(hooked(data, 12_000, event("PostToolUse", "p2", write)));
    lines.push(hooked(data, 15_000, event("SubagentStop", "p2", worker)));
    lines.push(hooked(
        data,
        20_000,
        event("Stop", "p2", json!({ "background_tasks": [] })),
    ));

    let mut studio = Studio::replayed(&lines);
    for ask in studio.turn_diffs(AT + 40_000, 10) {
        studio.set_turn_files(&ask, turn_diffs(&ask));
    }
    let turns = &studio.session(SESSION).unwrap().turns;
    let files = |turn: usize| {
        let found = turns[turn].files.clone().unwrap_or_default();
        let paths: Vec<String> = found.files.iter().map(|f| f.path.clone()).collect();
        (paths, found.external)
    };
    assert_eq!(files(0), (vec!["late.rs".to_owned()], 0));
    assert_eq!(files(1), (vec![], 0));
}
