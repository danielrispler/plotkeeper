// Seam 1 (#426): the hook binary with a scratch `HOME` and a temporary repository, fed the lines
// wave 1's trial recorded of one Codex and one Cursor turn that fixed `calc.py` (#369;
// `core/tests/fixtures/{codex,cursor}/trial.jsonl`, the trial's folder written `/w/trial`), and
// the lines of a real OpenCode turn that wrote `hello.txt` (#717; the first turn of
// `core/tests/fixtures/opencode/run.jsonl`, in `/Users/me/work/hello`). Each harness's turn records
// the start and end trees of the worktree it worked in, as Claude Code's does.
use crate::common::{feed, hold, hook};
use serde_json::Value;
use std::{fs, path::Path, time::Duration};
use studio_test_support::git;

const CODEX: &str = include_str!("../../core/tests/fixtures/codex/trial.jsonl");
const CURSOR: &str = include_str!("../../core/tests/fixtures/cursor/trial.jsonl");
const OPENCODE: &str = include_str!("../../core/tests/fixtures/opencode/run.jsonl");

const BUGGY: &str = "def add(a, b):\n    return a - b\n\nprint(\"2+3 =\", add(2, 3))\n";
const FIXED: &str = "def add(a, b):\n    return a + b\n\nprint(\"2+3 =\", add(2, 3))\n";

/// Feeds the hook entry of `agent` each of the lines its hooks sent in `recorded`, with their
/// `folder` a fresh repository holding the buggy `file`, which is fixed before the call that edits it
/// reports. Returns the repository's root and the lines the hook logged.
fn replay(
    agent: &str,
    recorded: &str,
    (folder, file): (&str, &str),
    edited: impl Fn(&Value) -> bool,
) -> (String, Vec<Value>) {
    let base = std::env::temp_dir().join(format!("studio-harness-{agent}-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
    let (home, trial) = (base.join("home"), base.join("trial"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&trial).unwrap();
    // `git` spells a temp dir through its symlinks.
    let trial = trial.canonicalize().unwrap();
    git(&trial, &["init", "-q"]);
    fs::write(trial.join(file), BUGGY).unwrap();
    git(&trial, &["add", "."]);
    git(&trial, &["commit", "-qm", "init"]);
    let root = trial.to_str().unwrap().to_string();
    for line in recorded.lines() {
        let line: Value = serde_json::from_str(&line.replace(folder, &root)).unwrap();
        // The server's own lines never reach the hook.
        if line.get("studio_agent").is_none() {
            continue;
        }
        let mut payload = line.clone();
        let fields = payload.as_object_mut().unwrap();
        fields.retain(|key, _| !key.starts_with("studio_"));
        if edited(&line) {
            fs::write(trial.join(file), FIXED).unwrap();
        }
        let mut cmd = untimed(hook(&home));
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
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(Path::new(root).parent().unwrap()).ok();
    changed
}

/// The first turn of `session` in `recorded`: its lines up to the first that `ends`.
fn first_turn(recorded: &str, session: &str, ends: &str) -> String {
    let lines = recorded.lines().filter(|l| l.contains(session));
    let mut turn = Vec::new();
    for line in lines {
        turn.push(line);
        if line.contains(ends) {
            break;
        }
    }
    turn.join("\n")
}

#[test]
fn a_harness_turn_that_edits_a_file_records_its_worktrees_start_and_end_trees() {
    let opencode = first_turn(
        OPENCODE,
        "ses_de5fa80f2ffeXThZwnlgn61UPk",
        "session.execution.succeeded",
    );
    let trial = ("/w/trial", "calc.py");
    // Each harness, its lines, their folder and the file its turn edits, the call that edits it, and
    // the names of the lines its start and end trees land on.
    let cases = [
        (
            "codex",
            CODEX,
            trial,
            ("PostToolUse", "apply_patch"),
            ("UserPromptSubmit", "Stop"),
        ),
        (
            "cursor",
            CURSOR,
            trial,
            ("postToolUse", "Write"),
            ("beforeSubmitPrompt", "stop"),
        ),
        (
            "opencode",
            opencode.as_str(),
            ("/Users/me/work/hello", "hello.txt"),
            ("tool.execute.after", "write"),
            ("session.prompt", "session.execution.succeeded"),
        ),
    ];
    for (agent, recorded, at, (call, tool), (starts, ends)) in cases {
        let edits = |l: &Value| l["hook_event_name"] == call && l["tool_name"] == tool;
        let (root, lines) = replay(agent, recorded, at, edits);
        let start = tree(&lines, starts, "studio_turn_start", &root);
        let end = tree(&lines, ends, "studio_turn_end", &root);
        assert_eq!(changed(&root, &start, &end), at.1, "{agent}");
    }
}

/// `cmd` with a trees budget a loaded machine cannot run out of: these tests check what the trees
/// hold; the budget has its own tests below. Under parallel builds two roots' trees took over the
/// real 1.2 s.
fn untimed(mut cmd: std::process::Command) -> std::process::Command {
    cmd.env("PLOTKEEPER_HOOK_TREES_BUDGET_MS", "30000");
    cmd
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
    let mut cmd = untimed(hook(home));
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
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
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
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
    assert_eq!(files, "f.py");
}

/// The prompt's start trees of every workspace root are no touch of the roots it did not work in:
/// it touches only its folder's, so another session's change in a root this turn never touched is
/// not fingerprinted as this turn's (#426 review).
#[test]
fn a_cursor_prompt_starts_every_workspace_root_but_touches_only_its_folders() {
    let base = std::env::temp_dir().join(format!("studio-cursor-touch-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let (a, b) = (repository(&base, "a"), repository(&base, "b"));
    let prompt = cursor(
        &home,
        serde_json::json!({
            "conversation_id": "c-1", "generation_id": "g-1", "hook_event_name": "beforeSubmitPrompt",
            "cursor_version": "3.31.15", "workspace_roots": [a, b],
        }),
    );
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();

    assert_eq!(prompt["studio_touched_roots"], serde_json::json!([a]));
    let started = prompt["studio_turn_start"].as_object().unwrap();
    assert!(
        started.contains_key(&a) && started.contains_key(&b),
        "{prompt}"
    );
}

/// A `Shell` call names the folder it ran in, which it touches, though it lies outside every
/// workspace root.
#[test]
fn a_cursor_shell_call_touches_the_folder_it_ran_in() {
    let base = std::env::temp_dir().join(format!("studio-cursor-shell-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
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
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();

    assert_eq!(ran["studio_touched_roots"], serde_json::json!([c]));
}

/// Codex's and Cursor's entries give the hook 5 seconds, and one `git` call may take 20: a turn's
/// trees that take too long are left out, and the line is still logged in time.
#[test]
fn a_cursor_line_is_logged_in_time_when_git_hangs() {
    let base = std::env::temp_dir().join(format!("studio-cursor-hang-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
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
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();

    assert!(took < std::time::Duration::from_secs(5), "took {took:?}");
    assert_eq!(log.lines().count(), 1, "{log}");
}

/// A turn's start trees that finish past the budget, while the line waits for the log's lock, are
/// not kept for a line logged without them: the turn's next touch of the worktree records its start
/// tree, so its end tree has a start to go with (#426 review).
#[test]
fn a_cursor_prompts_late_trees_leave_the_worktrees_start_to_its_next_touch() {
    let base = std::env::temp_dir().join(format!("studio-cursor-late-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
    let (home, bin) = (base.join("home"), base.join("bin"));
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let a = repository(&base, "a");
    // A `git` whose `write-tree` is slow, past the budget but before the hook gives up the lock.
    let real = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    let real = String::from_utf8(real.stdout).unwrap();
    let fake = bin.join("git");
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do [ \"$a\" = write-tree ] && sleep 3.5; done\nexec {} \"$@\"\n",
        real.trim()
    );
    fs::write(&fake, script).unwrap();
    fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
    let event = |name: &str| {
        serde_json::json!({
            "conversation_id": "c-1", "generation_id": "g-1", "hook_event_name": name,
            "cursor_version": "3.31.15", "workspace_roots": [a],
        })
    };
    // Another hook holds the log's lock meanwhile.
    let log = studio_hook::event_log(&home);
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let held = hold(&log);
    let mut cmd = hook(&home);
    cmd.args(["--agent", "cursor"])
        .env("PATH", path)
        .env("PLOTKEEPER_HOOK_PATIENCE_MS", "2000");
    assert!(
        feed(cmd, &event("beforeSubmitPrompt"))
            .wait()
            .unwrap()
            .success()
    );
    drop(held);
    let file = format!("{a}/f.py");
    fs::write(&file, "print(1)\n").unwrap();
    let mut write = event("postToolUse");
    write["tool_name"] = "Write".into();
    write["tool_input"] = serde_json::json!({ "file_path": file });
    let written = cursor(&home, write);
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();

    assert!(written["studio_turn_start"][&a].is_string(), "{written}");
}

/// OpenCode's plugin spawns one hook an event and never waits for it, so the hooks of a turn's last
/// calls and its end run at once with the next turn's start, and reach the turns file in any order
/// (#717). Here each such group is spawned at once, last line first, after the turn's edit, while the
/// log's lock is held a moment, so that their appends wait on it together; the third turn is a
/// wake-up, which has no prompt id. The log they wrote, replayed through the core, still has each
/// turn list its own edit (#744): the log holds a session's lines in the order the hook took them,
/// and the core takes no turn end from a line of a turn already past, and puts its call in the turn
/// before.
#[test]
fn opencode_turns_whose_hooks_run_at_once_out_of_order_each_list_their_own_edit() {
    use serde_json::json;
    let base = std::env::temp_dir().join(format!("studio-opencode-order-{}", std::process::id()));
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
    let home = base.join("home");
    fs::create_dir_all(&home).unwrap();
    let root = repository(&base, "hello");
    let line = |name: &str, sent: u64, extra: Value| {
        let mut line = json!({
            "session_id": "ses_1", "cwd": root, "hook_event_name": name, "sent_ms": sent,
        });
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        line
    };
    const TURNS: u64 = 4;
    let start = |k: u64| match k {
        2 => line("session.execution.started", 100 * k + 10, json!({})),
        _ => line(
            "session.prompt",
            100 * k + 10,
            json!({ "prompt_id": format!("msg_{k}") }),
        ),
    };
    let call = |name: &str, k: u64, sent: u64| {
        let input = json!({ "path": format!("f{k}.txt"), "content": "x" });
        let extra = json!({ "tool_name": "write", "tool_use_id": format!("call_{k}"), "tool_input": input });
        line(name, 100 * k + sent, extra)
    };
    let end = |k: u64| line("session.execution.succeeded", 100 * k + 40, json!({}));
    let log = studio_hook::event_log(&home);
    fs::create_dir_all(log.parent().unwrap()).unwrap();
    let run = |lines: Vec<Value>| {
        let held = studio_hook::locked(&log, Duration::from_secs(1)).unwrap();
        let children: Vec<_> = (lines.iter().rev())
            .map(|payload| {
                let mut cmd = hook(&home);
                cmd.args(["--agent", "opencode"]);
                feed(cmd, payload)
            })
            .collect();
        // Long enough for every hook of the group to reach the log's lock, or the turns file's.
        std::thread::sleep(Duration::from_millis(300));
        drop(held);
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
    };

    run(vec![start(0)]);
    for k in 0..TURNS {
        fs::write(Path::new(&root).join(format!("f{k}.txt")), "x\n").unwrap();
        let mut group = vec![
            call("tool.execute.before", k, 20),
            call("tool.execute.after", k, 30),
            end(k),
        ];
        if k + 1 < TURNS {
            group.push(start(k + 1));
        }
        run(group);
    }

    let log = fs::read_to_string(log).unwrap();
    let listed = core_listed(&log, &root);
    // Cleanup: a leftover temp file fails no test.
    fs::remove_dir_all(&base).ok();
    assert_eq!(
        listed,
        [["f0.txt"], ["f1.txt"], ["f2.txt"], ["f3.txt"]],
        "{log}"
    );
}

/// The files each turn of the one session in `log` lists once the core has read the log and diffed
/// its turns' trees in repository `root`, as the shell does.
fn core_listed(log: &str, root: &str) -> Vec<Vec<String>> {
    use studio_core::{Numstat, RootDiff, Studio};
    let mut studio = Studio::default();
    let mut now = 0;
    for line in log.lines() {
        let at = serde_json::from_str::<Value>(line).unwrap()["studio_received_ms"].as_u64();
        now = now.max(at.unwrap_or_default());
        studio.apply(line);
    }
    for ask in studio.turn_diffs(now + 20_000, 100) {
        let files = (ask.trees.iter())
            .filter_map(|trees| {
                let diff = ["diff", "--numstat", &trees.start, trees.end.as_ref()?];
                Some(git(Path::new(root), &diff))
            })
            .flat_map(|numstat| {
                let lines: Vec<String> = numstat.lines().map(Into::into).collect();
                lines.into_iter().map(|line| {
                    let path = line.split('\t').nth(2).unwrap_or_default().to_owned();
                    Numstat {
                        path,
                        from: None,
                        added: Some(1),
                        removed: Some(0),
                    }
                })
            })
            .collect();
        let diff = RootDiff {
            root: root.into(),
            moved: vec![],
            files,
        };
        studio.set_turn_files(&ask, Some(vec![diff]));
    }
    let session = studio.sessions()[0];
    (session.turns.iter())
        .map(|turn| {
            let files = turn.files.iter().flat_map(|f| f.files.iter());
            files.map(|f| f.path.clone()).collect()
        })
        .collect()
}
