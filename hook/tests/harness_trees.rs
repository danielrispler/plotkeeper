// Seam 1 (#426): the hook binary with a scratch `HOME` and a temporary repository, fed the lines
// wave 1's trial recorded of one Codex and one Cursor turn that fixed `calc.py` (#369;
// `core/tests/fixtures/{codex,cursor}/trial.jsonl`, the trial's folder written `/w/trial`). Each
// harness's turn records the start and end trees of the worktree it worked in, as Claude Code's does.
use crate::common::{feed, hook};
use serde_json::Value;
use std::{fs, path::Path};
use studio_test_support::git;

const CODEX: &str = include_str!("../../core/tests/fixtures/codex/trial.jsonl");
const CURSOR: &str = include_str!("../../core/tests/fixtures/cursor/trial.jsonl");

const BUGGY: &str = "def add(a, b):\n    return a - b\n\nprint(\"2+3 =\", add(2, 3))\n";
const FIXED: &str = "def add(a, b):\n    return a + b\n\nprint(\"2+3 =\", add(2, 3))\n";

/// Feeds the hook entry of `agent` each of the trial's lines its hooks sent, with the trial's folder
/// a fresh repository holding the buggy `calc.py`, which is fixed before the call that edits it
/// reports. Returns the repository's root and the lines the hook logged.
fn replay(agent: &str, recorded: &str, edited: impl Fn(&Value) -> bool) -> (String, Vec<Value>) {
    let base = std::env::temp_dir().join(format!("studio-harness-{agent}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (home, trial) = (base.join("home"), base.join("trial"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&trial).unwrap();
    // `git` spells a temp dir through its symlinks.
    let trial = trial.canonicalize().unwrap();
    git(&trial, &["init", "-q"]);
    fs::write(trial.join("calc.py"), BUGGY).unwrap();
    git(&trial, &["add", "."]);
    git(&trial, &["commit", "-qm", "init"]);
    let root = trial.to_str().unwrap().to_string();
    for line in recorded.lines() {
        let line: Value = serde_json::from_str(&line.replace("/w/trial", &root)).unwrap();
        // The server's own lines never reach the hook.
        if line.get("studio_agent").is_none() {
            continue;
        }
        let mut payload = line.clone();
        let fields = payload.as_object_mut().unwrap();
        fields.retain(|key, _| !key.starts_with("studio_"));
        if edited(&line) {
            fs::write(trial.join("calc.py"), FIXED).unwrap();
        }
        let mut cmd = hook(&home);
        cmd.args(["--agent", agent]);
        assert!(feed(cmd, &payload).wait().unwrap().success());
    }
    let log = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    let logged = log
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (root, logged)
}

/// The tree the line named `name` recorded under `key` for `root`.
fn tree(lines: &[Value], name: &str, key: &str, root: &str) -> String {
    let line = lines.iter().find(|l| l["hook_event_name"] == name).unwrap();
    let tree = line[key][root].as_str();
    tree.unwrap_or_else(|| panic!("{name} has no {key} for {root}: {line}"))
        .to_string()
}

/// The files that differ between trees `from` and `to` of repository `root`, which goes with the call.
fn changed(root: &str, from: &str, to: &str) -> String {
    let changed = git(Path::new(root), &["diff", "--name-only", from, to]);
    let _ = fs::remove_dir_all(Path::new(root).parent().unwrap());
    changed
}

#[test]
fn a_codex_turn_that_patches_a_file_records_its_worktrees_start_and_end_trees() {
    let patched =
        |l: &Value| l["hook_event_name"] == "PostToolUse" && l["tool_name"] == "apply_patch";
    let (root, lines) = replay("codex", CODEX, patched);
    let start = tree(&lines, "UserPromptSubmit", "studio_turn_start", &root);
    let end = tree(&lines, "Stop", "studio_turn_end", &root);
    assert_eq!(changed(&root, &start, &end), "calc.py");
}

#[test]
fn a_cursor_turn_that_writes_a_file_records_its_worktrees_start_and_end_trees() {
    let written = |l: &Value| l["hook_event_name"] == "postToolUse" && l["tool_name"] == "Write";
    let (root, lines) = replay("cursor", CURSOR, written);
    let start = tree(&lines, "beforeSubmitPrompt", "studio_turn_start", &root);
    let end = tree(&lines, "stop", "studio_turn_end", &root);
    assert_eq!(changed(&root, &start, &end), "calc.py");
}

/// A fresh repository at `base`/`name` with one commit, spelled as `git` spells it.
fn repository(base: &Path, name: &str) -> String {
    let root = base.join(name);
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("README"), "hi\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    root.to_str().unwrap().to_string()
}

/// Feeds Cursor's hook entry `payload`, in Cursor's names, with `home` as `HOME`, and returns the line it logged.
fn cursor(home: &Path, payload: Value) -> Value {
    let mut cmd = hook(home);
    cmd.args(["--agent", "cursor"]);
    assert!(feed(cmd, &payload).wait().unwrap().success());
    let log = fs::read_to_string(studio_hook::event_log(home)).unwrap();
    serde_json::from_str(log.lines().last().unwrap()).unwrap()
}

/// Cursor sends no event before a call, only `postToolUse` after it: a turn's start trees are taken
/// at its prompt for every workspace root, so a file written in a root other than the first is
/// still the turn's (#426 review).
#[test]
fn a_cursor_turn_that_writes_in_its_second_workspace_root_lists_the_file() {
    let base = std::env::temp_dir().join(format!("studio-cursor-roots-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let (a, b) = (repository(&base, "a"), repository(&base, "b"));
    let event = |name: &str| {
        serde_json::json!({
            "conversation_id": "c-1", "generation_id": "g-1", "hook_event_name": name,
            "cursor_version": "3.31.15", "workspace_roots": [a, b],
        })
    };
    let prompt = cursor(&home, event("beforeSubmitPrompt"));
    let file = format!("{b}/f.py");
    fs::write(&file, "print(1)\n").unwrap();
    let mut write = event("postToolUse");
    write["tool_name"] = "Write".into();
    write["tool_input"] = serde_json::json!({ "file_path": file });
    let written = cursor(&home, write);
    let stop = cursor(&home, event("stop"));

    let start = prompt["studio_turn_start"][&b]
        .as_str()
        .or(written["studio_turn_start"][&b].as_str())
        .unwrap();
    let end = stop["studio_turn_end"][&b].as_str().unwrap();
    let files = git(Path::new(&b), &["diff", "--name-only", start, end]);
    let _ = fs::remove_dir_all(&base);
    assert_eq!(files, "f.py");
}

/// A `Shell` call names the folder it ran in, which it touches, though it lies outside every
/// workspace root.
#[test]
fn a_cursor_shell_call_touches_the_folder_it_ran_in() {
    let base = std::env::temp_dir().join(format!("studio-cursor-shell-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let (a, c) = (repository(&base, "a"), repository(&base, "c"));
    let event = |name: &str| {
        serde_json::json!({
            "conversation_id": "c-1", "generation_id": "g-1", "hook_event_name": name,
            "cursor_version": "3.31.15", "workspace_roots": [a], "cwd": "",
        })
    };
    cursor(&home, event("beforeSubmitPrompt"));
    let mut shell = event("postToolUse");
    shell["tool_name"] = "Shell".into();
    shell["tool_input"] = serde_json::json!({ "command": "python3 calc.py", "cwd": c });
    let ran = cursor(&home, shell);
    let _ = fs::remove_dir_all(&base);

    assert_eq!(ran["studio_touched_roots"], serde_json::json!([c]));
}

/// Codex's and Cursor's entries give the hook 5 seconds, and one `git` call may take 20: a turn's
/// trees that take too long are left out, and the line is still logged in time.
#[test]
fn a_cursor_line_is_logged_in_time_when_git_hangs() {
    let base = std::env::temp_dir().join(format!("studio-cursor-hang-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (home, bin) = (base.join("home"), base.join("bin"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let a = repository(&base, "a");
    // A `git` that hangs, as on a huge or locked repository.
    let fake = bin.join("git");
    fs::write(&fake, "#!/bin/sh\nexec sleep 30\n").unwrap();
    fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let payload = serde_json::json!({
        "conversation_id": "c-1", "generation_id": "g-1", "hook_event_name": "beforeSubmitPrompt",
        "cursor_version": "3.31.15", "workspace_roots": [a], "prompt": "hi",
    });
    let mut cmd = hook(&home);
    cmd.args(["--agent", "cursor"]).env("PATH", path);
    let began = std::time::Instant::now();
    assert!(feed(cmd, &payload).wait().unwrap().success());
    let took = began.elapsed();
    let log = fs::read_to_string(studio_hook::event_log(&home)).unwrap_or_default();
    let _ = fs::remove_dir_all(&base);

    assert!(took < std::time::Duration::from_secs(5), "took {took:?}");
    assert_eq!(log.lines().count(), 1, "{log}");
}
