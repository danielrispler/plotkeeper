// Removed from core/tests/turn_files.rs in #415 with `Studio::summary_facts`, `turn_workers` and `turn_merged`, which only these tests called. One `turn_workers` assert was dropped from `a_worktree_the_session_edits_through_bash_with_no_worker_in_it_counts_for_its_turn`.

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
        moved: vec![],
        files: vec![stat("src/a.rs", 3, 1), stat("src/b.rs", 5, 0)],
    }];
    let facts = studio.summary_facts(&ask, Some(diff));

    assert_eq!(facts.files.len(), 2);
    assert_eq!(facts.files[0].file.path, "src/a.rs");
    assert_eq!(facts.files[1].file.path, "src/b.rs");
    assert_eq!(studio.summary_ask("no-such-turn"), None);
}

#[test]
fn the_turns_workers_homes_are_the_worktrees_its_offer_leaves_out() {
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
    ask(&mut studio, AT + 20_000);
    assert_eq!(studio.turn_workers(&turn_id(&studio)), [WORKER_ROOT]);
}

#[test]
fn a_subagent_that_stayed_in_the_sessions_worktree_has_no_home_of_its_own() {
    let stayed = line(
        "SubagentStart",
        500,
        json!({ "agent_id": "agent-1", "cwd": ROOT }),
    );
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        stayed,
        edit(1_000, "e1", "/w/repo/src/a.rs", Some("agent-1")),
        stop(json!({ ROOT: "e1" })),
    ]);
    ask(&mut studio, AT + 20_000);
    assert!(studio.turn_workers(&turn_id(&studio)).is_empty());
}

#[test]
fn a_turn_that_ran_only_git_merge_base_has_no_merge_flag() {
    let ran = |command: &str| {
        let lines = [
            prompt(json!({ ROOT: "s1" })),
            line(
                "PostToolUse",
                1_000,
                json!({ "tool_name": "Bash", "tool_input": { "command": command },
                        "tool_use_id": "b1", "tool_response": {} }),
            ),
            stop(json!({ ROOT: "e1" })),
        ];
        let studio = Studio::replayed(lines);
        let id = studio.session(SESSION).unwrap().turns[0]
            .id
            .clone()
            .unwrap();
        studio.turn_merged(&id)
    };
    assert!(!ran("git merge-base HEAD origin/main"));
    assert!(!ran("cd /w/repo && git merge-base --is-ancestor a b"));
    assert!(ran("git merge feature"));
    assert!(ran("git fetch && git merge origin/main"));
    assert!(ran("gh pr merge 12 --squash"));
}

#[test]
fn a_worker_spawned_in_an_earlier_turn_and_still_working_is_a_home_of_a_later_turn() {
    let later = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        edit(
            1_000,
            "e1",
            "/w/repo/.claude/worktrees/w1/x.rs",
            Some("agent-1"),
        ),
        line(
            "Stop",
            9_000,
            json!({ "background_tasks": [{ "id": "agent-1", "type": "subagent" }],
                    "studio_turn_end": { ROOT: "e1" } }),
        ),
        later(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        later("Stop", 20_000, json!({ "studio_turn_end": { ROOT: "e2" } })),
    ]);
    ask(&mut studio, AT + 40_000);
    let id = |turn: usize| {
        studio.session(SESSION).unwrap().turns[turn]
            .id
            .clone()
            .unwrap()
    };
    assert_eq!(studio.turn_workers(&id(0)), [WORKER_ROOT]);
    assert_eq!(studio.turn_workers(&id(1)), [WORKER_ROOT]);
}

#[test]
fn a_worker_that_ended_before_a_turn_began_is_no_home_of_it() {
    let later = |name: &str, ms: u64, extra: Value| {
        let mut extra = extra;
        extra["prompt_id"] = "p2".into();
        line(name, ms, extra)
    };
    let mut studio = Studio::replayed([
        prompt(json!({ ROOT: "s1" })),
        worker_starts(500, "agent-1", "w1"),
        edit(
            1_000,
            "e1",
            "/w/repo/.claude/worktrees/w1/x.rs",
            Some("agent-1"),
        ),
        line(
            "SubagentStop",
            2_000,
            json!({ "agent_id": "agent-1", "studio_turn_end": { WORKER_ROOT: "w2" } }),
        ),
        stop(json!({ ROOT: "e1" })),
        later(
            "UserPromptSubmit",
            10_000,
            json!({ "prompt": "next", "studio_turn_start": { ROOT: "s2" } }),
        ),
        later("Stop", 20_000, json!({ "studio_turn_end": { ROOT: "e2" } })),
    ]);
    ask(&mut studio, AT + 40_000);
    let second = studio.session(SESSION).unwrap().turns[1]
        .id
        .clone()
        .unwrap();
    assert!(studio.turn_workers(&second).is_empty());
}
// And the helper only those tests used:


fn turn_id(studio: &Studio) -> String {
    studio.session(SESSION).unwrap().turns[0]
        .id
        .clone()
        .expect("turn id")
}
