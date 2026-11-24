//! Seam 1 (danielrispler/plotkeeper#102, Testing Decisions) for a turn's files
//! (danielrispler/plotkeeper#111): hook events with the hook's trees replayed through the core, and
//! the numstat the shell hands back, as `git diff --numstat` would give it.

use crate::common;
use serde_json::{Value, json};
use studio_core::{CheckStatus, Numstat, RootDiff, Studio, TurnDiff};

const SESSION: &str = "5b1c7e30-9d2a-4f8b-8c63-1e0a7d4b9f52";
const ROOT: &str = "/w/repo";
const WORKER_ROOT: &str = "/w/repo/.claude/worktrees/w1";
const AT: u64 = 1_800_000_000_000;

fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut base = common::base(SESSION, name, ROOT, AT + ms);
    base["prompt_id"] = json!("p1");
    common::with(base, extra)
}

fn prompt(start: Value) -> String {
    line(
        "UserPromptSubmit",
        0,
        json!({ "prompt": "go", "studio_turn_start": start }),
    )
}

/// A finished edit of `file` by the session's agent, or by worker `agent`.
fn edit(ms: u64, id: &str, file: &str, agent: Option<&str>) -> String {
    let mut extra = json!({
        "tool_name": "Edit", "tool_input": { "file_path": file }, "tool_use_id": id,
        "tool_response": {},
    });
    if let Some(agent) = agent {
        extra["agent_id"] = agent.into();
    }
    line("PostToolUse", ms, extra)
}

fn stop(end: Value) -> String {
    line("Stop", 9_000, json!({ "studio_turn_end": end }))
}

fn stat(path: &str, added: u64, removed: u64) -> Numstat {
    Numstat {
        path: path.into(),
        from: None,
        added: Some(added),
        removed: Some(removed),
    }
}

fn files(studio: &Studio) -> Option<studio_core::TurnFiles> {
    studio.session(SESSION).unwrap().turns[0].files.clone()
}

fn ask(studio: &mut Studio, now: u64) -> Vec<TurnDiff> {
    studio.turn_diffs(now, 10)
}

#[test]
fn an_ended_turns_files_are_every_file_that_differs_between_its_trees() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        stop(json!({ ROOT: "e1" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    assert_eq!(asks.len(), 1);
    assert!(asks[0].ended);
    assert_eq!(asks[0].trees[0].start, "s1");
    assert_eq!(asks[0].trees[0].end.as_deref(), Some("e1"));
    // Daniel edited b.rs in the same worktree during the turn.
    let diff = vec![RootDiff {
        root: ROOT.into(),
        moved: vec![],
        files: vec![stat("src/a.rs", 3, 1), stat("src/b.rs", 5, 0)],
    }];
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).expect("files");
    assert_eq!(found.files.len(), 2);
    assert_eq!(found.files[0].path, "src/a.rs");
    assert_eq!(
        (found.files[0].added, found.files[0].removed),
        (Some(3), Some(1))
    );
    assert_eq!(found.files[1].path, "src/b.rs");
    // An ended turn is asked for once.
    assert!(ask(&mut studio, AT + 40_000).is_empty());
}

/// Worker `agent`'s first event, in its own worktree, with the start tree the hook recorded there.
fn worker_starts(ms: u64, agent: &str, tree: &str) -> String {
    line(
        "SubagentStart",
        ms,
        json!({ "agent_id": agent, "cwd": WORKER_ROOT, "studio_turn_start": { WORKER_ROOT: tree } }),
    )
}

#[test]
fn a_workers_edits_in_its_own_worktree_count_in_the_turn_that_spawned_it() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        edit(
            1_000,
            "e1",
            "/w/repo/.claude/worktrees/w1/x.rs",
            Some("agent-1"),
        ),
        stop(json!({ ROOT: "e1", WORKER_ROOT: "w2" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    assert_eq!(asks[0].trees.len(), 2);
    let diff = vec![
        RootDiff {
            root: ROOT.into(),
            moved: vec![],
            files: vec![],
        },
        RootDiff {
            root: WORKER_ROOT.into(),
            moved: vec![],
            files: vec![stat("x.rs", 2, 0)],
        },
    ];
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).unwrap();
    assert_eq!(found.files.len(), 1);
    assert_eq!(found.files[0].root, WORKER_ROOT);
}

/// The diff of each worktree of `ask`, from `files`: the files that differ in each root, by root.
fn diffs(ask: &TurnDiff, files: &[(&str, &str)]) -> Vec<RootDiff> {
    let diff = |root: &str| RootDiff {
        root: root.into(),
        moved: vec![],
        files: (files.iter())
            .filter(|(r, _)| *r == root)
            .map(|(_, path)| stat(path, 1, 0))
            .collect(),
    };
    ask.trees.iter().map(|t| diff(&t.root)).collect()
}

fn turn_files(studio: &Studio, turn: usize) -> Vec<String> {
    let found = studio.session(SESSION).unwrap().turns[turn].files.clone();
    let found = found.expect("files");
    found.files.iter().map(|f| f.path.clone()).collect()
}

#[test]
fn a_background_workers_late_edits_count_in_the_turn_that_spawned_it_and_not_the_next() {
    let late = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        // The worker runs on in the background past its turn's end, which takes its worktree's
        // tree too, as the hook does for a worktree its turn's worker touched.
        line(
            "Stop",
            9_000,
            json!({ "background_tasks": [{ "id": "agent-1", "type": "subagent" }],
                    "studio_turn_end": { ROOT: "e1", WORKER_ROOT: "w2" } }),
        ),
        late(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        edit(
            12_000,
            "e1",
            "/w/repo/.claude/worktrees/w1/late.rs",
            Some("agent-1"),
        ),
        late(
            "PostToolUse",
            13_000,
            json!({ "tool_name": "Edit", "tool_input": { "file_path": "/w/repo/src/c.rs" },
                    "tool_use_id": "e2", "tool_response": {} }),
        ),
    ]);
    // While the worker runs, its worktree's end is the worktree now.
    let asks = ask(&mut studio, AT + 14_000);
    let first = asks
        .iter()
        .find(|a| a.turn == 0)
        .expect("the spawning turn");
    assert!(!first.ended);
    let worker = first.trees.iter().find(|t| t.root == WORKER_ROOT).unwrap();
    assert_eq!(worker.end, None);
    // So does a hunk's.
    let trees = studio.turn_trees(SESSION, 0, WORKER_ROOT);
    assert_eq!(trees, Some(("w1".into(), None)));
    let root = first.trees.iter().find(|t| t.root == ROOT).unwrap();
    assert_eq!(root.end.as_deref(), Some("e1"));

    // Its stop takes its worktree's end tree.
    studio.replay(&late(
        "SubagentStop",
        15_000,
        json!({ "agent_id": "agent-1", "cwd": WORKER_ROOT,
                "studio_turn_end": { WORKER_ROOT: "w3" } }),
    ));
    studio.replay(&late(
        "Stop",
        20_000,
        json!({ "studio_turn_end": { ROOT: "e2" } }),
    ));
    let asks = ask(&mut studio, AT + 40_000);
    let first = asks
        .iter()
        .find(|a| a.turn == 0)
        .expect("the spawning turn");
    assert!(first.ended);
    let worker = first.trees.iter().find(|t| t.root == WORKER_ROOT).unwrap();
    assert_eq!(worker.end.as_deref(), Some("w3"));
    studio.set_turn_files(first, Some(diffs(first, &[(WORKER_ROOT, "late.rs")])));
    assert_eq!(turn_files(&studio, 0), vec!["late.rs".to_string()]);
    let second = asks.iter().find(|a| a.turn == 1).expect("the next turn");
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "src/c.rs")])));
    assert_eq!(turn_files(&studio, 1), vec!["src/c.rs".to_string()]);
}

/// Daniel edited b.rs during turn 0, and the agent edited it again in turn 1, after turn 0 and its
/// workers were done: turn 0's diff still counts it as its own.
#[test]
fn a_files_later_edit_by_another_turn_does_not_take_a_change_before_it() {
    let next = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        stop(json!({ ROOT: "e1" })),
        next(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        next(
            "PostToolUse",
            11_000,
            json!({ "tool_name": "Edit", "tool_input": { "file_path": "/w/repo/src/b.rs" },
                    "tool_use_id": "e2", "tool_response": {} }),
        ),
        next("Stop", 20_000, json!({ "studio_turn_end": { ROOT: "e2" } })),
    ]);
    let asks = ask(&mut studio, AT + 40_000);
    let first = asks.iter().find(|a| a.turn == 0).unwrap();
    let diff = diffs(first, &[(ROOT, "src/a.rs"), (ROOT, "src/b.rs")]);
    studio.set_turn_files(first, Some(diff));
    assert_eq!(
        turn_files(&studio, 0),
        vec!["src/a.rs".to_string(), "src/b.rs".to_string()]
    );
    // And turn 1's diff counts b.rs as its own, not as turn 0's.
    let second = asks.iter().find(|a| a.turn == 1).unwrap();
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "src/b.rs")])));
    assert_eq!(turn_files(&studio, 1), vec!["src/b.rs".to_string()]);
}

/// A call whose start never landed is placed by its result's time: turn 0's edit of a.rs does not
/// hide Daniel's change to a.rs during turn 1.
#[test]
fn an_earlier_turns_edit_with_no_start_does_not_take_a_change_in_a_later_turn() {
    let next = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        stop(json!({ ROOT: "e1" })),
        next(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        next("Stop", 20_000, json!({ "studio_turn_end": { ROOT: "e2" } })),
    ]);
    let asks = ask(&mut studio, AT + 40_000);
    let second = asks.iter().find(|a| a.turn == 1).unwrap();
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "src/a.rs")])));
    assert_eq!(turn_files(&studio, 1), vec!["src/a.rs".to_string()]);
}

#[test]
fn a_late_edit_of_a_worker_in_the_sessions_worktree_is_not_the_next_turns() {
    let late = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        line("SubagentStart", 500, json!({ "agent_id": "agent-1" })),
        line(
            "Stop",
            9_000,
            json!({ "background_tasks": [{ "id": "agent-1", "type": "subagent" }],
                    "studio_turn_end": { ROOT: "e1" } }),
        ),
        late(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        edit(12_000, "e1", "/w/repo/late.rs", Some("agent-1")),
        late(
            "SubagentStop",
            15_000,
            json!({ "agent_id": "agent-1", "studio_turn_end": { ROOT: "w3" } }),
        ),
        late("Stop", 20_000, json!({ "studio_turn_end": { ROOT: "e2" } })),
    ]);
    let asks = ask(&mut studio, AT + 40_000);
    let first = asks.iter().find(|a| a.turn == 0).unwrap();
    assert_eq!(first.trees[0].end.as_deref(), Some("w3"));
    studio.set_turn_files(first, Some(diffs(first, &[(ROOT, "late.rs")])));
    assert_eq!(turn_files(&studio, 0), vec!["late.rs".to_string()]);
    let second = asks.iter().find(|a| a.turn == 1).unwrap();
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "late.rs")])));
    assert_eq!(turn_files(&studio, 1), Vec::<String>::new());
}

#[test]
fn a_rename_is_one_row_and_a_binary_file_has_no_counts() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/new.rs", None),
        edit(2_000, "e2", "/w/repo/logo.png", None),
        stop(json!({ ROOT: "e1" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    let renamed = Numstat {
        path: "new.rs".into(),
        from: Some("old.rs".into()),
        added: Some(0),
        removed: Some(0),
    };
    let binary = Numstat {
        path: "logo.png".into(),
        from: None,
        added: None,
        removed: None,
    };
    let diff = vec![RootDiff {
        root: ROOT.into(),
        moved: vec![],
        files: vec![renamed, binary],
    }];
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).unwrap();
    assert_eq!(found.files.len(), 2);
    assert_eq!(found.files[0].from.as_deref(), Some("old.rs"));
    assert_eq!((found.files[1].added, found.files[1].removed), (None, None));
}

#[test]
fn a_running_turn_reports_its_files_so_far_and_is_asked_again_after_5_seconds() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
    ]);
    let asks = ask(&mut studio, AT + 2_000);
    assert!(!asks[0].ended);
    // No end tree: the shell takes the worktree's tree now.
    assert_eq!(asks[0].trees[0].end, None);
    let diff = vec![RootDiff {
        root: ROOT.into(),
        moved: vec![],
        files: vec![stat("src/a.rs", 1, 0)],
    }];
    studio.set_turn_files(&asks[0], Some(diff));
    assert_eq!(files(&studio).unwrap().files.len(), 1);
    assert!(ask(&mut studio, AT + 4_000).is_empty());
    // Due after 5 seconds once the session has had an event since the ask.
    studio.replay(&edit(3_000, "e2", "/w/repo/src/a.rs", None));
    assert_eq!(ask(&mut studio, AT + 7_500).len(), 1);
}

#[test]
fn a_turn_whose_trees_could_not_be_diffed_has_no_files() {
    let mut studio = Studio::replayed([prompt(json!({ ROOT: "s1" })), stop(json!({ ROOT: "e1" }))]);
    let asks = ask(&mut studio, AT + 20_000);
    studio.set_turn_files(&asks[0], None);
    assert_eq!(files(&studio), None);
}

#[test]
fn an_agy_session_reports_no_turn_files_and_its_capability_says_so() {
    assert_eq!(
        [
            studio_core::AGY_CAPABILITIES.turn_trees,
            studio_core::CLAUDE_CAPABILITIES.turn_trees
        ],
        [false, true]
    );
    let brain =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/agy/brain");
    let run =
        include_str!("fixtures/agy/run.jsonl").replace("@BRAIN@", &brain.display().to_string());
    // Trees on its first event are not taken: its adapter does not report them.
    let mut lines: Vec<String> = run.lines().map(Into::into).collect();
    let mut first: Value = serde_json::from_str(&lines[0]).unwrap();
    first["studio_turn_start"] = json!({ ROOT: "s1" });
    lines[0] = first.to_string();
    let mut studio = Studio::replayed(lines);
    assert!(ask(&mut studio, AT + 2_000).is_empty());
    assert!(
        studio
            .session("145c3c4a-3fac-4c6c-8ffc-b1bd95c0f78a")
            .unwrap()
            .turns
            .iter()
            .all(|t| t.files.is_none())
    );
}

/// `count` turns, each ended, from prompt ids `t0`, `t1`, ... one second apart.
fn ended_turns(count: u64) -> Vec<String> {
    let mut lines = Vec::new();
    for i in 0..count {
        let at = i * 10_000;
        let id = format!("t{i}");
        let mut start =
            json!({ "prompt_id": id, "prompt": "go", "studio_turn_start": { ROOT: "s1" } });
        start["studio_received_ms"] = (AT + at).into();
        lines.push(line("UserPromptSubmit", at, start));
        let mut end = json!({ "prompt_id": id, "studio_turn_end": { ROOT: "e1" } });
        end["studio_received_ms"] = (AT + at + 5_000).into();
        lines.push(line("Stop", at + 5_000, end));
    }
    lines
}

#[test]
fn ended_turns_past_a_passs_limit_are_asked_in_the_next_pass() {
    let mut studio = Studio::replayed(ended_turns(5));
    let now = AT + 100_000;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..3 {
        for ask in studio.turn_diffs(now, 2) {
            seen.insert(ask.turn);
            let diff = vec![RootDiff {
                root: ROOT.into(),
                moved: vec![],
                files: vec![],
            }];
            studio.set_turn_files(&ask, Some(diff));
        }
    }
    assert_eq!(seen.len(), 5, "{seen:?}");
}

#[test]
fn an_unended_turn_older_than_the_window_is_not_diffed_again() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
    ]);
    assert_eq!(studio.turn_diffs(AT + 20_000, 10).len(), 1);
    let eight_days = 8 * 24 * 60 * 60 * 1000;
    assert!(studio.turn_diffs(AT + eight_days, 10).is_empty());
}

#[test]
fn an_unended_turn_is_asked_again_only_after_its_session_has_an_event() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
    ]);
    assert_eq!(studio.turn_diffs(AT + 20_000, 10).len(), 1);
    // The session was killed: nothing has happened since the ask, so the last result stands.
    assert!(studio.turn_diffs(AT + 40_000, 10).is_empty());
    assert!(studio.turn_diffs(AT + 60_000, 10).is_empty());
    // A new edit makes it due again.
    studio.replay(&edit(50_000, "e2", "/w/repo/src/b.rs", None));
    assert_eq!(studio.turn_diffs(AT + 70_000, 10).len(), 1);
    assert!(studio.turn_diffs(AT + 90_000, 10).is_empty());
}

const OTHER: &str = "9d3f5a62-1c47-4e0b-a2d8-6b7e0c1f4a93";
const OTHER_ROOT: &str = "/w/other";

/// A finished call of the other session, whose turn starts at 500 ms and runs on: `name` is its
/// tool, `input` its input, `at` its time, and `cwd` its folder.
fn other_call(
    ms: u64,
    id: &str,
    tool: &str,
    input: Value,
    cwd: &str,
    roots: Option<Value>,
) -> String {
    let mut event = json!({
        "session_id": OTHER, "hook_event_name": "PostToolUse", "cwd": cwd, "prompt_id": "q1",
        "studio_received_ms": AT + ms, "tool_name": tool, "tool_input": input,
        "tool_use_id": id, "tool_response": {},
    });
    if let Some(roots) = roots {
        event["studio_touched_roots"] = roots;
    }
    event.to_string()
}

fn other_prompt() -> String {
    json!({
        "session_id": OTHER, "hook_event_name": "UserPromptSubmit", "cwd": ROOT, "prompt_id": "q1",
        "studio_received_ms": AT + 500, "prompt": "go", "studio_turn_start": { ROOT: "t1" },
    })
    .to_string()
}

fn bash(ms: u64, id: &str, command: &str, roots: Option<Value>) -> String {
    other_call(ms, id, "Bash", json!({ "command": command }), ROOT, roots)
}

/// The files of the session's turn 0 when the trees differ in `a.rs` and `b.rs` of the root, and
/// `others` happened in the turn.
fn kept(others: Vec<String>) -> Vec<String> {
    kept_of(&["src/a.rs", "src/b.rs"], others)
}

/// Like `kept`, when the trees differ in `paths` of the root.
fn kept_of(paths: &[&str], others: Vec<String>) -> Vec<String> {
    let mut lines = vec![prompt(json!({ ROOT: "s1" })), other_prompt()];
    lines.extend(others);
    lines.push(stop(json!({ ROOT: "e1" })));
    let mut studio = Studio::replayed(lines);
    let asks = ask(&mut studio, AT + 20_000);
    let mine = asks.iter().find(|a| a.session_id == SESSION).unwrap();
    studio.set_turn_files(
        mine,
        Some(diffs(
            mine,
            &paths.iter().map(|p| (ROOT, *p)).collect::<Vec<_>>(),
        )),
    );
    turn_files(&studio, 0)
}

#[test]
fn a_file_changed_only_through_bash_is_the_turns_with_its_lines() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        line(
            "PostToolUse",
            1_000,
            json!({ "tool_name": "Bash", "tool_input": { "command": "sed -i s/a/b/ src/a.rs" },
                    "tool_use_id": "b1", "tool_response": {} }),
        ),
        stop(json!({ ROOT: "e1" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    let diff = vec![RootDiff {
        root: ROOT.into(),
        moved: vec![],
        files: vec![stat("src/a.rs", 4, 2)],
    }];
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).unwrap();
    assert_eq!(found.files.len(), 1);
    assert_eq!(
        (found.files[0].added, found.files[0].removed),
        (Some(4), Some(2))
    );
}

#[test]
fn with_no_other_toucher_of_the_root_every_differing_file_is_the_turns() {
    assert_eq!(kept(vec![]), ["src/a.rs", "src/b.rs"]);
    // Another session's call in another root names neither.
    let elsewhere = other_call(
        2_000,
        "x1",
        "Edit",
        json!({ "file_path": "/w/other/src/a.rs" }),
        OTHER_ROOT,
        None,
    );
    assert_eq!(kept(vec![elsewhere]), ["src/a.rs", "src/b.rs"]);
}

#[test]
fn another_sessions_edit_of_the_absolute_path_takes_the_file() {
    let edit = other_call(
        2_000,
        "x1",
        "Edit",
        json!({ "file_path": "/w/repo/src/a.rs" }),
        ROOT,
        None,
    );
    assert_eq!(kept(vec![edit]), ["src/b.rs"]);
}

#[test]
fn another_sessions_bash_command_holding_the_absolute_path_takes_the_file() {
    let command = bash(2_000, "x1", "sed -i s/a/b/ /w/repo/src/b.rs", None);
    assert_eq!(kept(vec![command]), ["src/a.rs"]);
}

#[test]
fn another_sessions_bash_command_that_touched_the_root_takes_the_path_from_the_root() {
    let command = bash(2_000, "x1", "sed -i s/a/b/ src/a.rs", Some(json!([ROOT])));
    assert_eq!(kept(vec![command]), ["src/b.rs"]);
}

#[test]
fn another_sessions_command_with_only_the_bare_name_or_another_root_leaves_the_file() {
    let bare = bash(2_000, "x1", "touch a.rs && cat b.rs", Some(json!([ROOT])));
    assert_eq!(kept(vec![bare]), ["src/a.rs", "src/b.rs"]);
    // The same relative path, from a command that touched another root.
    let from_other = other_call(
        2_000,
        "x2",
        "Bash",
        json!({ "command": "sed -i s/a/b/ src/a.rs" }),
        OTHER_ROOT,
        Some(json!([OTHER_ROOT])),
    );
    assert_eq!(kept(vec![from_other]), ["src/a.rs", "src/b.rs"]);
}

#[test]
fn another_sessions_call_outside_the_turns_trees_leaves_the_file() {
    // Before the turn began, and after its end.
    let before = other_call(
        0,
        "x1",
        "Edit",
        json!({ "file_path": "/w/repo/src/a.rs" }),
        ROOT,
        None,
    );
    let after = other_call(
        9_500,
        "x2",
        "Edit",
        json!({ "file_path": "/w/repo/src/b.rs" }),
        ROOT,
        None,
    );
    let mut early: Value = serde_json::from_str(&before).unwrap();
    early["studio_received_ms"] = (AT - 5_000).into();
    assert_eq!(
        kept(vec![early.to_string(), after]),
        ["src/a.rs", "src/b.rs"]
    );
}

#[test]
fn a_later_turn_of_the_same_session_that_names_a_file_during_the_earlier_trees_takes_it() {
    let next = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    // Turn 0's worker runs on past its end, so turn 0's trees run until the worker stops; turn 1
    // edits b.rs meanwhile.
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        line("SubagentStart", 500, json!({ "agent_id": "agent-1" })),
        line(
            "Stop",
            9_000,
            json!({ "background_tasks": [{ "id": "agent-1", "type": "subagent" }],
                    "studio_turn_end": { ROOT: "e1" } }),
        ),
        next(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        next(
            "PostToolUse",
            12_000,
            json!({ "tool_name": "Edit", "tool_input": { "file_path": "/w/repo/src/b.rs" },
                    "tool_use_id": "e2", "tool_response": {} }),
        ),
        next(
            "SubagentStop",
            15_000,
            json!({ "agent_id": "agent-1", "studio_turn_end": { ROOT: "w3" } }),
        ),
        next("Stop", 20_000, json!({ "studio_turn_end": { ROOT: "e2" } })),
    ]);
    let asks = ask(&mut studio, AT + 40_000);
    let first = asks.iter().find(|a| a.turn == 0).unwrap();
    studio.set_turn_files(
        first,
        Some(diffs(first, &[(ROOT, "src/a.rs"), (ROOT, "src/b.rs")])),
    );
    assert_eq!(turn_files(&studio, 0), ["src/a.rs"]);
    let second = asks.iter().find(|a| a.turn == 1).unwrap();
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "src/b.rs")])));
    assert_eq!(turn_files(&studio, 1), ["src/b.rs"]);
}

#[test]
fn a_files_agent_is_the_one_whose_call_names_it_or_the_workers_whose_worktree_holds_it() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        edit(
            1_200,
            "e3",
            "/w/repo/.claude/worktrees/w1/y.rs",
            Some("agent-1"),
        ),
        line(
            "PostToolUse",
            1_500,
            json!({ "tool_name": "Bash", "tool_input": { "command": "make /w/repo/src/c.rs" },
                    "tool_use_id": "b1", "tool_response": {} }),
        ),
        stop(json!({ ROOT: "e1", WORKER_ROOT: "w2" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    let diff = diffs(
        &asks[0],
        &[
            (ROOT, "src/a.rs"),
            (ROOT, "src/b.rs"),
            (ROOT, "src/c.rs"),
            (WORKER_ROOT, "y.rs"),
            (WORKER_ROOT, "z.rs"),
        ],
    );
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).unwrap();
    let agents: Vec<(&str, Option<&str>)> = found
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.agent.as_deref()))
        .collect();
    // Daniel's b.rs names nobody; the worker's own call and its worktree name the worker.
    assert_eq!(
        agents,
        [
            ("src/a.rs", Some("claude")),
            ("src/b.rs", None),
            ("src/c.rs", Some("claude")),
            ("y.rs", Some("agent")),
            ("z.rs", Some("agent"))
        ]
    );
}

/// The files and merged count of turn 0 when the trees differ in `a.rs`, `b.rs` and `c.rs` of the
/// root, a move of `HEAD` brought in `b.rs` and `c.rs`, and `calls` happened in the turn.
fn merged_of(calls: Vec<String>) -> (Vec<String>, usize) {
    let mut lines = vec![prompt(json!({ ROOT: "s1" }))];
    lines.extend(calls);
    lines.push(stop(json!({ ROOT: "e1" })));
    let mut studio = Studio::replayed(lines);
    let asks = ask(&mut studio, AT + 20_000);
    let mut diff = diffs(
        &asks[0],
        &[(ROOT, "src/a.rs"), (ROOT, "src/b.rs"), (ROOT, "src/c.rs")],
    );
    diff[0].moved = vec!["src/b.rs".into(), "src/c.rs".into()];
    studio.set_turn_files(&asks[0], Some(diff));
    (turn_files(&studio, 0), files(&studio).unwrap().merged)
}

#[test]
fn files_a_move_of_head_brought_in_are_counted_as_merged_and_not_the_turns() {
    let calls = vec![edit(1_000, "e1", "/w/repo/src/a.rs", None)];
    assert_eq!(merged_of(calls), (vec!["src/a.rs".to_string()], 2));
}

#[test]
fn a_merged_file_that_an_edit_of_the_turn_names_stays_the_turns() {
    let calls = vec![
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        edit(2_000, "e2", "/w/repo/src/b.rs", None),
    ];
    assert_eq!(
        merged_of(calls),
        (vec!["src/a.rs".to_string(), "src/b.rs".to_string()], 1)
    );
}

#[test]
fn a_merged_file_that_a_bash_command_of_the_turn_names_by_its_path_stays_the_turns() {
    let command = line(
        "PostToolUse",
        1_000,
        json!({ "tool_name": "Bash", "tool_input": { "command": "sed -i s/<<<//g /w/repo/src/c.rs" },
                "tool_use_id": "b1", "tool_response": {} }),
    );
    assert_eq!(
        merged_of(vec![command]),
        (vec!["src/a.rs".to_string(), "src/c.rs".to_string()], 1)
    );
}

#[test]
fn another_sessions_command_names_a_root_level_file_only_by_its_absolute_path() {
    let paths = ["README.md", "Cargo.toml"];
    let bare = bash(
        2_000,
        "x1",
        "cat README.md ./Cargo.toml",
        Some(json!([ROOT])),
    );
    assert_eq!(kept_of(&paths, vec![bare]), paths);
    let absolute = bash(2_000, "x2", "cat /w/repo/README.md", Some(json!([ROOT])));
    assert_eq!(kept_of(&paths, vec![absolute]), ["Cargo.toml"]);
}

#[test]
fn another_sessions_command_names_a_path_on_its_boundaries_not_as_a_substring() {
    let paths = ["src/a.rs", "Cargo.toml", "core/Cargo.toml"];
    let command = bash(
        2_000,
        "x1",
        "cat src/a.rs.bak mysrc/a.rs core/Cargo.toml.lock",
        Some(json!([ROOT])),
    );
    assert_eq!(kept_of(&paths, vec![command]), paths);
    let right = bash(
        2_000,
        "x2",
        "cat ./src/a.rs; cat \"core/Cargo.toml\"",
        Some(json!([ROOT])),
    );
    assert_eq!(kept_of(&paths, vec![right]), ["Cargo.toml"]);
}

#[test]
fn a_worktree_the_session_edits_through_bash_with_no_worker_in_it_counts_for_its_turn() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1", WORKER_ROOT: "w1" })),
        line(
            "PostToolUse",
            1_000,
            json!({ "tool_name": "Bash", "tool_use_id": "b1", "tool_response": {},
                    "tool_input": { "command": "cd /w/repo/.claude/worktrees/w1 && sed -i s/a/b/ x.rs" },
                    "studio_touched_roots": [ROOT, WORKER_ROOT] }),
        ),
        stop(json!({ ROOT: "e1", WORKER_ROOT: "w2" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    studio.set_turn_files(&asks[0], Some(diffs(&asks[0], &[(WORKER_ROOT, "x.rs")])));
    assert_eq!(turn_files(&studio, 0), ["x.rs"]);
}

#[test]
fn a_file_the_turns_own_call_names_stays_the_turns_though_another_call_names_it_too() {
    let read = other_call(
        2_000,
        "x1",
        "Bash",
        json!({ "command": "git diff /w/repo/src/a.rs" }),
        ROOT,
        None,
    );
    let own = edit(1_000, "e1", "/w/repo/src/a.rs", None);
    assert_eq!(kept(vec![own, read]), ["src/a.rs", "src/b.rs"]);
}

#[test]
fn another_sessions_edit_spelled_through_a_symlink_takes_the_file_from_the_root_git_spells() {
    // Git spells the root through `/private`; the other session's tool names its file without.
    let root = "/private/w/repo";
    let mut studio = Studio::with_roots(|folder| Some(format!("/private{folder}")));
    let other_edit = other_call(
        1_000,
        "x1",
        "Edit",
        json!({ "file_path": "/w/repo/src/a.rs" }),
        ROOT,
        None,
    );
    let other_start = other_prompt();
    for line in [
        prompt(json!({ root: "s1" })),
        other_start,
        other_edit,
        stop(json!({ root: "e1" })),
    ] {
        studio.replay(&line);
    }
    let asks = ask(&mut studio, AT + 20_000);
    let mine = asks.iter().find(|a| a.session_id == SESSION).unwrap();
    let both = [(root, "src/a.rs"), (root, "src/b.rs")];
    studio.set_turn_files(mine, Some(diffs(mine, &both)));
    assert_eq!(turn_files(&studio, 0), ["src/b.rs"]);
}

/// A second prompt of the session at `ms`; a resume before it leaves the first turn with no end.
fn second_prompt(ms: u64, start: Value) -> String {
    line(
        "UserPromptSubmit",
        ms,
        json!({ "prompt": "next", "prompt_id": "p2", "studio_turn_start": start }),
    )
}

#[test]
fn a_deleted_sessions_turns_are_not_asked_until_it_starts_again() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        stop(json!({ ROOT: "e1" })),
        line("StudioDelete", 10_000, json!({})),
    ]);
    assert!(ask(&mut studio, AT + 20_000).is_empty());
    assert!(ask(&mut studio, AT + 25_000).is_empty());
    studio.replay(&line("SessionStart", 30_000, json!({ "source": "resume" })));
    assert_eq!(ask(&mut studio, AT + 40_000).len(), 1);
}

#[test]
fn an_unclosed_turn_of_an_ended_session_never_diffs_live_and_is_settled() {
    // The session's end lands before its delayed prompt, so the turn never closes.
    let mut studio = Studio::replayed([
        line("SessionEnd", 500, json!({})),
        prompt(json!({ ROOT: "s1" })),
    ]);
    assert!(studio.session(SESSION).unwrap().turns[0].ended_at.is_none());
    assert!(ask(&mut studio, AT + 20_000).is_empty());
    studio.replay(&edit(2_000, "e1", "/w/repo/src/a.rs", None));
    assert!(ask(&mut studio, AT + 40_000).is_empty());
}

#[test]
fn an_unclosed_turn_of_an_ended_session_caps_its_window_at_the_sessions_last_event() {
    let mut studio = Studio::replayed([
        line("SessionEnd", 500, json!({})),
        prompt(json!({ ROOT: "s1" })),
        worker_starts(600, "agent-1", "w1"),
        line(
            "SubagentStop",
            2_000,
            json!({ "agent_id": "agent-1", "studio_turn_end": { WORKER_ROOT: "w2" } }),
        ),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].window, (AT, AT + 2_000));
}

#[test]
fn a_non_final_unclosed_turn_never_diffs_live_and_caps_its_window_at_the_next_turns_start() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        line("SessionStart", 3_000, json!({ "source": "resume" })),
        second_prompt(5_000, json!({ ROOT: "s2" })),
    ]);
    let asks = ask(&mut studio, AT + 10_000);
    // Only the later turn, which runs, is asked.
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].turn, 1);
    assert_eq!(asks[0].window.1, u64::MAX);
    assert!(ask(&mut studio, AT + 30_000).is_empty());
}

#[test]
fn a_non_final_unclosed_turns_window_ends_where_the_next_turn_starts() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        line(
            "SubagentStop",
            2_000,
            json!({ "agent_id": "agent-1", "studio_turn_end": { WORKER_ROOT: "w2" } }),
        ),
        line("SessionStart", 3_000, json!({ "source": "resume" })),
        second_prompt(5_000, json!({ ROOT: "s2" })),
    ]);
    let asks = ask(&mut studio, AT + 10_000);
    let first = asks.iter().find(|a| a.turn == 0).unwrap();
    assert_eq!(first.window, (AT, AT + 5_000));
}

#[test]
fn a_stale_turn_with_a_running_worker_sorts_behind_the_running_turn() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        line("SessionStart", 3_000, json!({ "source": "resume" })),
        second_prompt(5_000, json!({ ROOT: "s2" })),
    ]);
    let asks = ask(&mut studio, AT + 10_000);
    let turns: Vec<usize> = asks.iter().map(|a| a.turn).collect();
    assert_eq!(turns, [1, 0]);
    // The worker still runs, so the window stays open.
    assert_eq!(asks[1].window.1, u64::MAX);
    assert_eq!(studio.turn_diffs(AT + 10_000, 1).len(), 0);
}

/// The matching runs on a snapshot, with the core free to change: a snapshot taken, the core
/// replayed on, the files computed from the snapshot alone and stored in a short write.
#[test]
fn a_turns_files_are_computed_from_a_snapshot_and_stored_afterwards() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        stop(json!({ ROOT: "e1" })),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    let diff = diffs(&asks[0], &[(ROOT, "src/a.rs"), (ROOT, "src/b.rs")]);
    let snapshot = studio.turn_files_snapshot(&asks[0], &diff);
    // Another session's edit of b.rs lands after the snapshot: the snapshot does not see it.
    studio.replay(&line(
        "PostToolUse",
        2_000,
        json!({ "session_id": "other", "tool_name": "Edit", "tool_use_id": "o1", "tool_response": {},
                "tool_input": { "file_path": "/w/repo/src/b.rs" } }),
    ));
    let found = studio_core::compute_turn_files(&snapshot, diff);
    let paths: Vec<_> = found.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["src/a.rs", "src/b.rs"]);
    assert!(found.files[0].agent.is_some());
    assert_eq!(found.files[1].agent, None);
    assert!(studio.session(SESSION).unwrap().turns[0].files.is_none());
    studio.store_turn_files(&asks[0], Some(found.clone()));
    assert_eq!(files(&studio), Some(found));
}

#[test]
fn a_running_turn_the_shell_settled_is_not_asked_again_while_it_runs() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    assert_eq!(asks.len(), 1);
    assert!(!asks[0].ended);
    studio.settle_turn_files(&asks[0]);
    studio.replay(&edit(25_000, "e2", "/w/repo/src/b.rs", None));
    assert!(ask(&mut studio, AT + 60_000).is_empty());
}

#[test]
fn a_running_turn_the_shell_settled_is_asked_again_once_its_stop_lands_end_trees() {
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
    ]);
    let asks = ask(&mut studio, AT + 20_000);
    studio.settle_turn_files(&asks[0]);
    studio.replay(&stop(json!({ ROOT: "e1" })));
    let again = ask(&mut studio, AT + 60_000);
    assert_eq!(again.len(), 1);
    assert!(again[0].ended);
    assert_eq!(again[0].trees[0].end.as_deref(), Some("e1"));
}

/// A Bash call of the session's own agent: its start, and its result unless `ends` is false.
fn check_call(ms: u64, id: &str, command: &str, ends: bool) -> Vec<String> {
    let call =
        json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": id });
    let mut lines = vec![line("PreToolUse", ms, call.clone())];
    if ends {
        let mut end = call;
        end["tool_response"] = json!({ "stdout": "ok", "stderr": "" });
        lines.push(line("PostToolUse", ms + 100, end));
    }
    lines
}

#[test]
fn a_summarys_facts_hold_only_the_checks_and_runs_that_ended() {
    let mut lines = vec![
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
    ];
    lines.extend(check_call(2_000, "t1", "cargo test", true));
    // The same check runs again, and another check has only started.
    lines.extend(check_call(3_000, "t2", "cargo test", false));
    lines.extend(check_call(3_500, "t3", "npm run check", false));
    lines.push(stop(json!({ ROOT: "e1" })));
    let studio = Studio::replayed(lines);
    let turn = &studio.session(SESSION).unwrap().turns[0];
    assert_eq!(turn.checks.len(), 2);
    let ask = studio.summary_ask(turn.id.as_deref().unwrap()).unwrap();

    let facts = studio.facts_of(&ask, None);

    let shown: Vec<_> = facts
        .checks
        .iter()
        .map(|c| (c.suite.as_str(), c.status, c.runs.len()))
        .collect();
    assert_eq!(shown, [("cargo test", CheckStatus::Passed, 1)]);
    assert!(!serde_json::to_string(&facts).unwrap().contains("running"));
}
