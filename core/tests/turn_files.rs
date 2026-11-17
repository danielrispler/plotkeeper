//! Seam 1 (danielrispler/plotkeeper#102, Testing Decisions) for a turn's files
//! (danielrispler/plotkeeper#111): hook events with the hook's trees replayed through the core, and
//! the numstat the shell hands back, as `git diff --numstat` would give it.

use serde_json::{Value, json};
use studio_core::{Numstat, RootDiff, Studio, TurnDiff};

const SESSION: &str = "5b1c7e30-9d2a-4f8b-8c63-1e0a7d4b9f52";
const ROOT: &str = "/w/repo";
const WORKER_ROOT: &str = "/w/repo/.claude/worktrees/w1";
const AT: u64 = 1_800_000_000_000;

fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut event = json!({
        "session_id": SESSION,
        "hook_event_name": name,
        "cwd": ROOT,
        "prompt_id": "p1",
        "studio_received_ms": AT + ms,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
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
fn an_ended_turns_files_are_the_diff_kept_to_what_its_agent_edited_and_the_rest_is_external() {
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
        files: vec![stat("src/a.rs", 3, 1), stat("src/b.rs", 5, 0)],
    }];
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).expect("files");
    assert_eq!(found.files.len(), 1);
    assert_eq!(found.files[0].path, "src/a.rs");
    assert_eq!(
        (found.files[0].added, found.files[0].removed),
        (Some(3), Some(1))
    );
    assert_eq!(found.external, 1);
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
            files: vec![],
        },
        RootDiff {
            root: WORKER_ROOT.into(),
            files: vec![stat("x.rs", 2, 0)],
        },
    ];
    studio.set_turn_files(&asks[0], Some(diff));
    let found = files(&studio).unwrap();
    assert_eq!(found.files.len(), 1);
    assert_eq!(found.files[0].root, WORKER_ROOT);
    assert_eq!(found.external, 0);
}

/// The diff of each worktree of `ask`, from `files`: the files that differ in each root, by root.
fn diffs(ask: &TurnDiff, files: &[(&str, &str)]) -> Vec<RootDiff> {
    let diff = |root: &str| RootDiff {
        root: root.into(),
        files: (files.iter())
            .filter(|(r, _)| *r == root)
            .map(|(_, path)| stat(path, 1, 0))
            .collect(),
    };
    ask.trees.iter().map(|t| diff(&t.root)).collect()
}

fn turn_files(studio: &Studio, turn: usize) -> (Vec<String>, u64) {
    let found = studio.session(SESSION).unwrap().turns[turn].files.clone();
    let found = found.expect("files");
    let paths = found.files.iter().map(|f| f.path.clone()).collect();
    (paths, found.external)
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
    assert_eq!(turn_files(&studio, 0), (vec!["late.rs".into()], 0));
    let second = asks.iter().find(|a| a.turn == 1).expect("the next turn");
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "src/c.rs")])));
    assert_eq!(turn_files(&studio, 1), (vec!["src/c.rs".into()], 0));
}

#[test]
fn a_late_edit_of_a_worker_in_the_sessions_worktree_is_not_external_in_the_next_turn() {
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
    assert_eq!(turn_files(&studio, 0), (vec!["late.rs".into()], 0));
    let second = asks.iter().find(|a| a.turn == 1).unwrap();
    studio.set_turn_files(second, Some(diffs(second, &[(ROOT, "late.rs")])));
    assert_eq!(turn_files(&studio, 1), (vec![], 0));
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

#[test]
fn a_turns_summary_facts_are_its_files_with_counts_and_its_checks() {
    let studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        edit(1_000, "e1", "/w/repo/src/a.rs", None),
        stop(json!({ ROOT: "e1" })),
    ]);
    let id = studio.session(SESSION).unwrap().turns[0]
        .id
        .clone()
        .unwrap();

    let ask = studio
        .summary_ask(&id)
        .expect("an ask for a turn with trees");
    assert!(ask.ended);
    assert_eq!(ask.trees[0].end.as_deref(), Some("e1"));
    let diff = vec![RootDiff {
        root: ROOT.into(),
        files: vec![stat("src/a.rs", 3, 1), stat("src/b.rs", 5, 0)],
    }];
    let facts = studio.summary_facts(&ask, Some(diff));

    assert_eq!(facts.files.len(), 1);
    assert_eq!(facts.files[0].file.path, "src/a.rs");
    assert_eq!(facts.external, 1);
    assert_eq!(studio.summary_ask("no-such-turn"), None);
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
