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
