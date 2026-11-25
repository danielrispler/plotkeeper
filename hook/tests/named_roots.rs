// Seam 1 (danielrispler/plotkeeper#202, Testing Decisions): the hook binary with a scratch `HOME`
// and a temporary repository with a second worktree. A Bash call touches every worktree whose
// path its command names.
use crate::common::{feed, hook};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::{Value, json};
use studio_test_support::git;

/// A scratch home, a repository `main` with a commit, and the git-spelled root of each.
struct Scene {
    home: PathBuf,
    main: PathBuf,
    /// A folder put first on the hook's `PATH`.
    bin: Option<PathBuf>,
}

impl Scene {
    fn new(name: &str) -> Scene {
        let base = std::env::temp_dir().join(format!("studio-named-{name}-{}", std::process::id()));
        // Cleanup: a leftover temp file fails no test.
        fs::remove_dir_all(&base).ok();
        let home = base.join("home");
        let main = base.join("main");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&main).unwrap();
        // `git` spells a temp dir through its symlinks.
        let main = main.canonicalize().unwrap();
        let home = home.canonicalize().unwrap();
        git(&main, &["init", "-q"]);
        fs::write(main.join("a.txt"), "a\n").unwrap();
        git(&main, &["add", "."]);
        git(&main, &["commit", "-qm", "init"]);
        Scene {
            home,
            main,
            bin: None,
        }
    }

    /// A worktree at `path` below the main checkout or beside it.
    fn worktree(&self, path: &Path) -> PathBuf {
        git(
            &self.main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                &format!("b{}", path.display().to_string().len()),
                path.to_str().unwrap(),
            ],
        );
        path.canonicalize().unwrap()
    }

    /// Runs the hook on `event` for session `s-1` in the main checkout, and returns the line it logged.
    fn hook(&self, name: &str, extra: Value) -> Value {
        let mut event = json!({
            "session_id": "s-1", "hook_event_name": name, "cwd": self.main, "prompt_id": "p-1",
        });
        event
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut cmd = hook(&self.home);
        cmd.env("PATH", self.path());
        assert!(feed(cmd, &event).wait().unwrap().success());
        let log = fs::read_to_string(studio_hook::event_log(&self.home)).unwrap();
        serde_json::from_str(log.lines().last().unwrap()).unwrap()
    }

    fn path(&self) -> String {
        let path = std::env::var("PATH").unwrap_or_default();
        match &self.bin {
            Some(bin) => format!("{}:{path}", bin.display()),
            None => path,
        }
    }

    fn bash(&self, command: &str, extra: Value) -> Value {
        let mut event = json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": "b1" });
        event
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.hook("PreToolUse", event)
    }

    fn tree(&self, root: &Path) -> String {
        git(root, &["write-tree"])
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        // Cleanup: a leftover temp file fails no test.
        fs::remove_dir_all(self.home.parent().unwrap()).ok();
    }
}

fn roots(line: &Value) -> Vec<String> {
    let list = line["studio_touched_roots"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    list.iter()
        .map(|r| r.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_session_bash_naming_a_worktree_records_its_start_and_end_trees() {
    let scene = Scene::new("session");
    let second = scene.worktree(&scene.main.with_file_name("second"));
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));

    let call = scene.bash(&format!("cd {} && touch x", second.display()), json!({}));

    let main = scene.main.to_str().unwrap();
    let second_root = second.to_str().unwrap();
    assert_eq!(call["studio_turn_start"][second_root], scene.tree(&second));
    assert_eq!(roots(&call), [main, second_root]);
    let stop = scene.hook("Stop", json!({}));
    assert!(stop["studio_turn_end"][second_root].is_string(), "{stop}");
    assert!(stop["studio_turn_end"][main].is_string(), "{stop}");
}

#[test]
fn a_subagent_bash_naming_a_worktree_puts_it_in_the_workers_record_with_its_start_tree() {
    let scene = Scene::new("worker");
    let second = scene.worktree(&scene.main.with_file_name("second"));
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    scene.hook("Stop", json!({}));

    let call = scene.bash(
        &format!("git -C {} status", second.display()),
        json!({ "agent_id": "w1" }),
    );

    let second_root = second.to_str().unwrap();
    assert_eq!(call["studio_turn_start"][second_root], scene.tree(&second));
    let state = studio_hook::event_log(&scene.home).with_file_name("turns/s-1.json");
    let state: Value = serde_json::from_str(&fs::read_to_string(state).unwrap()).unwrap();
    let kept = state["workers"]["w1"]["roots"].as_array().unwrap().clone();
    assert!(kept.contains(&json!(second_root)), "{state}");
}

#[test]
fn a_path_touches_the_deepest_root_that_holds_it_on_a_path_boundary() {
    let scene = Scene::new("boundary");
    let one = scene.worktree(&scene.main.with_file_name("issue-1"));
    let inner = scene.worktree(&scene.main.join("inner"));
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));

    let call = scene.bash(
        &format!(
            "cat {}/../issue-12/x {}/f.txt",
            scene.main.display(),
            inner.display()
        ),
        json!({}),
    );

    let main = scene.main.to_str().unwrap();
    let one = one.to_str().unwrap();
    let got = roots(&call);
    assert!(got.contains(&inner.to_str().unwrap().to_owned()), "{got:?}");
    assert!(got.contains(&main.to_owned()), "{got:?}");
    assert!(!got.contains(&one.to_owned()), "{got:?}");
    let named = scene.bash(
        &format!("ls {}/f.txt", inner.display()),
        json!({ "tool_use_id": "b2" }),
    );
    assert_eq!(roots(&named), [main, inner.to_str().unwrap()]);
}

#[test]
fn a_worktree_made_by_an_earlier_command_is_touched_by_a_later_one() {
    let scene = Scene::new("later");
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    let made = scene.main.with_file_name("made");
    let first = scene.bash(
        &format!("git worktree add -q -b made {}", made.display()),
        json!({}),
    );
    assert_eq!(roots(&first), [scene.main.to_str().unwrap()]);
    git(
        &scene.main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "made",
            made.to_str().unwrap(),
        ],
    );

    let later = scene.bash(
        &format!("ls {}", made.display()),
        json!({ "tool_use_id": "b2" }),
    );

    let made = made.canonicalize().unwrap();
    assert!(
        roots(&later).contains(&made.to_str().unwrap().to_owned()),
        "{later}"
    );
}

#[test]
fn a_path_in_a_removed_worktree_touches_nothing_but_a_new_folder_in_a_listed_one_touches_it() {
    let scene = Scene::new("removed");
    let second = scene.worktree(&scene.main.join(".claude/worktrees/second"));
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    scene.bash(&format!("ls {}", second.display()), json!({}));
    git(
        &scene.main,
        &["worktree", "remove", "--force", second.to_str().unwrap()],
    );

    let gone = scene.bash(
        &format!("cat {}/src/a.rs", second.display()),
        json!({ "tool_use_id": "b2" }),
    );

    let main = scene.main.to_str().unwrap();
    assert_eq!(roots(&gone), [main], "only the folder's own root: {gone}");

    // Never seen by the hook, and never a root of its own.
    let unseen = scene.main.join(".claude/worktrees/unseen/src/a.rs");
    let gone = scene.bash(
        &format!("cat {}", unseen.display()),
        json!({ "tool_use_id": "b3" }),
    );
    assert_eq!(roots(&gone), [main]);

    let other = scene.worktree(&scene.main.with_file_name("other"));
    let write = scene.hook(
        "PreToolUse",
        json!({ "tool_name": "Write", "tool_input": { "file_path": other.join("new/dir/f.rs") }, "tool_use_id": "w1" }),
    );
    assert!(
        roots(&write).contains(&other.to_str().unwrap().to_owned()),
        "{write}"
    );
}

// A worktree made mid-turn (danielrispler/plotkeeper#204): its start is the commit it was created
// at, which its reflog's first line gives.

/// The reflog resolves to whole seconds, and a worktree counts as made later only in a later second.
fn next_second() {
    std::thread::sleep(std::time::Duration::from_millis(1100));
}

/// Runs `command` as the agent would, outside the hook.
fn run(dir: &Path, command: &str) {
    let status = Command::new("sh")
        .args(["-c", command])
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "{command}");
}

/// The files that differ between trees `from` and `to`.
fn changed(root: &Path, from: &str, to: &str) -> String {
    git(root, &["diff", "--name-only", from, to])
}

/// The files in tree `tree`.
fn files(root: &Path, tree: &str) -> String {
    git(root, &["ls-tree", "-r", "--name-only", tree])
}

#[test]
fn a_worktree_made_and_edited_by_one_command_starts_from_its_creation_commit() {
    let scene = Scene::new("made-edited");
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    next_second();
    let made = scene.main.with_file_name("made");
    let command = format!(
        "git worktree add -q -b made {0} && echo x > {0}/new.txt",
        made.display()
    );
    scene.bash(&command, json!({}));
    run(&scene.main, &command);

    let next = scene.bash(
        &format!("ls {}", made.display()),
        json!({ "tool_use_id": "b2" }),
    );

    let made = made.canonicalize().unwrap();
    let root = made.to_str().unwrap();
    let start = next["studio_turn_start"][root].as_str().unwrap();
    assert_eq!(start, git(&made, &["rev-parse", "HEAD^{tree}"]));
    let stop = scene.hook("Stop", json!({}));
    let end = stop["studio_turn_end"][root].as_str().unwrap();
    assert_eq!(changed(&made, start, end), "new.txt");
}

#[test]
fn a_worktree_that_existed_before_the_turn_starts_from_its_files_at_first_touch() {
    let scene = Scene::new("older");
    let older = scene.worktree(&scene.main.with_file_name("older"));
    fs::write(older.join("there.txt"), "t\n").unwrap();
    next_second();
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));

    let call = scene.bash(&format!("ls {}", older.display()), json!({}));

    let root = older.to_str().unwrap();
    let start = call["studio_turn_start"][root].as_str().unwrap();
    assert!(files(&older, start).contains("there.txt"));
    fs::write(older.join("new.txt"), "n\n").unwrap();
    let stop = scene.hook("Stop", json!({}));
    let end = stop["studio_turn_end"][root].as_str().unwrap();
    assert_eq!(changed(&older, start, end), "new.txt");
}

#[test]
fn a_worker_is_measured_from_its_own_first_event() {
    let scene = Scene::new("worker-made");
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    next_second();
    // Made after the turn began, before the worker's first event: its files at first touch.
    let before = scene.worktree(&scene.main.with_file_name("before"));
    fs::write(before.join("there.txt"), "t\n").unwrap();
    let first = scene.bash(
        &format!("ls {}", before.display()),
        json!({ "agent_id": "w1" }),
    );
    let before_root = before.to_str().unwrap();
    let start = first["studio_turn_start"][before_root].as_str().unwrap();
    assert!(files(&before, start).contains("there.txt"));
    next_second();
    // Made after the worker's first event.
    let made = scene.main.with_file_name("made");
    let command = format!(
        "git worktree add -q -b made {0} && echo x > {0}/new.txt",
        made.display()
    );
    scene.bash(&command, json!({ "agent_id": "w1", "tool_use_id": "b2" }));
    run(&scene.main, &command);

    let next = scene.bash(
        &format!("ls {}", made.display()),
        json!({ "agent_id": "w1", "tool_use_id": "b3" }),
    );

    let made = made.canonicalize().unwrap();
    let root = made.to_str().unwrap();
    let start = next["studio_turn_start"][root].as_str().unwrap();
    assert_eq!(start, git(&made, &["rev-parse", "HEAD^{tree}"]));
    let stop = scene.hook("Stop", json!({}));
    let end = stop["studio_turn_end"][root].as_str().unwrap();
    assert_eq!(changed(&made, start, end), "new.txt");
}

#[test]
fn a_worktree_with_no_first_reflog_line_starts_from_its_files_at_first_touch() {
    let scene = Scene::new("no-reflog");
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    next_second();
    let made = scene.worktree(&scene.main.with_file_name("made"));
    fs::write(made.join("new.txt"), "n\n").unwrap();
    let log = git(&made, &["rev-parse", "--absolute-git-dir"]);
    fs::write(Path::new(&log).join("logs/HEAD"), "").unwrap();

    let call = scene.bash(&format!("ls {}", made.display()), json!({}));

    let start = call["studio_turn_start"][made.to_str().unwrap()]
        .as_str()
        .unwrap();
    assert!(files(&made, start).contains("new.txt"));
}

#[test]
fn a_late_event_whose_folder_is_a_removed_worktrees_touches_no_root() {
    let scene = Scene::new("removed-cwd");
    let second = scene.worktree(&scene.main.join(".claude/worktrees/second"));
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    scene.bash(&format!("ls {}", second.display()), json!({}));
    git(
        &scene.main,
        &["worktree", "remove", "--force", second.to_str().unwrap()],
    );

    let late = scene.hook(
        "PostToolUse",
        json!({ "tool_name": "Read", "cwd": second, "tool_use_id": "r1" }),
    );
    assert_eq!(roots(&late), Vec::<String>::new(), "{late}");
}

/// A `git` on the path that counts its `worktree list` runs in `count`, then runs the real one.
fn counting_git(scene: &Scene) -> (PathBuf, PathBuf) {
    let bin = scene.home.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let count = scene.home.join("lists");
    let real =
        String::from_utf8(Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in *'worktree list'*) echo x >> '{}';; esac\nexec {} \"$@\"\n",
        count.display(),
        real.trim()
    );
    fs::write(bin.join("git"), script).unwrap();
    Command::new("chmod")
        .arg("+x")
        .arg(bin.join("git"))
        .status()
        .unwrap();
    (bin, count)
}

#[test]
fn a_path_under_a_root_the_hook_knows_lists_no_worktrees_again() {
    let mut scene = Scene::new("lists");
    let second = scene.worktree(&scene.main.with_file_name("second"));
    scene.hook("UserPromptSubmit", json!({ "prompt": "go" }));
    scene.bash(&format!("ls {}", second.display()), json!({}));
    let (bin, count) = counting_git(&scene);
    scene.bin = Some(bin);

    let call = scene.bash(
        &format!(
            "cat {}/a.txt {}/b.txt",
            second.display(),
            scene.main.display()
        ),
        json!({ "tool_use_id": "b2" }),
    );
    let second_root = second.to_str().unwrap().to_owned();
    assert!(roots(&call).contains(&second_root), "{call}");
    let lists = fs::read_to_string(&count)
        .unwrap_or_default()
        .lines()
        .count();
    assert_eq!(lists, 0, "listed again for a root it knew");
}

// A worktree about to be made is no touch of the root above it (danielrispler/plotkeeper#202): the
// known root `/repo` must not hold `/repo/.claude/worktrees/w1` before that folder exists.
#[test]
fn a_worktree_about_to_be_made_does_not_touch_a_known_root_above_it() {
    let scene = Scene::new("about");
    let other = scene.worktree(&scene.main.with_file_name("other"));
    fs::create_dir_all(scene.main.join(".claude/worktrees")).unwrap();
    scene.hook("UserPromptSubmit", json!({ "prompt": "go", "cwd": other }));
    let known = scene.bash(
        &format!("cat {}/a.txt", scene.main.display()),
        json!({ "cwd": other }),
    );
    assert!(roots(&known).contains(&scene.main.to_str().unwrap().to_owned()));

    let made = scene.bash(
        &format!(
            "git worktree add {}/.claude/worktrees/w1",
            scene.main.display()
        ),
        json!({ "cwd": other, "tool_use_id": "b2" }),
    );
    assert_eq!(roots(&made), [other.to_str().unwrap()], "{made}");
}
