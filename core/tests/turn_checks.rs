//! Seam 1 (danielrispler/plotkeeper#102, Testing Decisions) for a turn's checks
//! (danielrispler/plotkeeper#112): hook events replayed through the core, and how each turn's checks
//! ended, from the step ledger's rows.

use crate::common;
use serde_json::{Value, json};
use studio_core::{Check, CheckStatus, Studio, check_output};

const SESSION: &str = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
const CWD: &str = "/Users/danielrispler/work/plotkeeper";

fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut base = common::base(SESSION, name, CWD, 1_793_265_000_000u64 + ms);
    base["prompt_id"] = json!("p1");
    common::with(base, extra)
}

fn prompt() -> String {
    line("UserPromptSubmit", 0, json!({ "prompt": "Check it" }))
}

/// The start and the passing result of a Bash call of `agent` (the session's own when `None`).
fn passes(ms: u64, id: &str, command: &str, agent: Option<&str>) -> Vec<String> {
    let call =
        json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": id });
    let mut end = call.clone();
    end["tool_response"] = json!({ "stdout": "ok\nall passed", "stderr": "" });
    if let Some(agent) = agent {
        end["agent_id"] = agent.into();
    }
    let mut start = call;
    if let Some(agent) = agent {
        start["agent_id"] = agent.into();
    }
    vec![
        line("PreToolUse", ms, start),
        line("PostToolUse", ms + 100, end),
    ]
}

/// The start and the failing result of a Bash call, with `error` as the agent reports it.
fn fails(ms: u64, id: &str, command: &str, error: &str) -> Vec<String> {
    let call =
        json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": id });
    let mut end = call.clone();
    end["error"] = error.into();
    vec![
        line("PreToolUse", ms, call),
        line("PostToolUseFailure", ms + 100, end),
    ]
}

fn checks(calls: &[Vec<String>]) -> Vec<Check> {
    let mut lines = vec![prompt()];
    lines.extend(calls.concat());
    let studio = Studio::replayed(&lines);
    studio.session(SESSION).unwrap().turns[0].checks.clone()
}

fn shown(checks: &[Check]) -> Vec<(&str, CheckStatus, u32)> {
    checks
        .iter()
        .map(|c| (c.suite.as_str(), c.status, c.retries))
        .collect()
}

#[test]
fn npm_run_check_is_a_check_by_default() {
    let found = checks(&[passes(1_000, "t1", "npm run check", None)]);

    assert_eq!(shown(&found), [("npm run check", CheckStatus::Passed, 0)]);
    assert_eq!(found[0].runs[0].exit_code, Some(0));
}

#[test]
fn a_project_with_its_own_list_uses_that_list() {
    let mut lines = vec![prompt()];
    lines.extend(passes(1_000, "t1", "npm run check", None));
    lines.extend(passes(2_000, "t2", "make verify -j4", None));
    let mut studio = Studio::default();
    studio.set_check_lists([(CWD.to_string(), vec!["make verify".to_string()])]);
    lines.iter().for_each(|l| studio.replay(l));

    let found = &studio.session(SESSION).unwrap().turns[0].checks;

    assert_eq!(shown(found), [("make verify", CheckStatus::Passed, 0)]);

    // A list set after the replay applies to the turns already read.
    studio.set_check_lists([]);
    let found = &studio.session(SESSION).unwrap().turns[0].checks;
    assert_eq!(shown(found), [("npm run check", CheckStatus::Passed, 0)]);
}

#[test]
fn a_failed_run_then_a_passed_one_is_passed_after_one_retry() {
    let found = checks(&[
        fails(1_000, "t1", "cargo test", "Exit code 101\nfailures"),
        passes(2_000, "t2", "cargo test", None),
    ]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Passed, 1)]);
    let runs: Vec<(CheckStatus, Option<i32>)> = found[0]
        .runs
        .iter()
        .map(|r| (r.status, r.exit_code))
        .collect();
    assert_eq!(
        runs,
        [
            (CheckStatus::Failed, Some(101)),
            (CheckStatus::Passed, Some(0))
        ]
    );
}

#[test]
fn a_passed_run_then_a_failed_one_is_failed() {
    let found = checks(&[
        passes(1_000, "t1", "cargo test", None),
        fails(2_000, "t2", "cargo test", "Exit code 1\nboom"),
    ]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Failed, 0)]);
}

#[test]
fn a_piped_check_with_no_telling_exit_code_only_ran() {
    let found = checks(&[passes(1_000, "t1", "cargo test 2>&1 | tail -5", None)]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Ran, 0)]);
    assert_eq!(found[0].runs[0].exit_code, None);
}

#[test]
fn a_check_that_was_denied_is_no_run_and_each_suite_has_its_own_status() {
    let found = checks(&[
        fails(
            1_000,
            "t1",
            "cargo test",
            "The user doesn't want to proceed with this tool use.",
        ),
        passes(2_000, "t2", "cargo clippy", None),
        passes(3_000, "t3", "echo hello", None),
    ]);

    assert_eq!(shown(&found), [("cargo clippy", CheckStatus::Passed, 0)]);
}

#[test]
fn a_workers_check_counts_in_the_turn_that_spawned_it() {
    let mut lines = vec![prompt()];
    lines.push(line(
        "SubagentStart",
        500,
        json!({ "agent_id": "w1", "agent_type": "general-purpose" }),
    ));
    lines.extend(passes(1_000, "t1", "cargo test", Some("w1")));
    lines.push(line(
        "Stop",
        5_000,
        json!({ "last_assistant_message": "done" }),
    ));
    lines.push(line(
        "UserPromptSubmit",
        6_000,
        json!({ "prompt": "More", "prompt_id": "p2" }),
    ));
    let studio = Studio::replayed(&lines);

    let turns = &studio.session(SESSION).unwrap().turns;

    assert_eq!(
        shown(&turns[0].checks),
        [("cargo test", CheckStatus::Passed, 0)]
    );
    assert_eq!(turns[0].checks[0].runs[0].agent.as_deref(), Some("w1"));
    assert_eq!(turns[1].checks, []);
}

#[test]
fn a_checks_exit_code_and_output_tail_are_read_through_its_offset() {
    let calls = [
        fails(
            1_000,
            "t1",
            "npm run check",
            "Exit code 2\nline one\nline two\nerror: it broke",
        ),
        passes(2_000, "t2", "cargo test", None),
    ];
    let mut lines = vec![prompt()];
    lines.extend(calls.concat());
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let log = common::temp_file(&text);
    let log = log.path();
    let studio = Studio::replayed(&lines);
    let ledger = &studio.session(SESSION).unwrap().ledger;

    let failed = check_output(ledger.row(None, "t1").unwrap(), log).unwrap();
    let passed = check_output(ledger.row(None, "t2").unwrap(), log).unwrap();

    assert_eq!(failed.exit_code, Some(2));
    assert!(failed.tail.ends_with("error: it broke"), "{}", failed.tail);
    assert_eq!(passed.exit_code, Some(0));
    assert_eq!(passed.tail, "ok\nall passed");
}

/// The suites and statuses of the checks a turn with the one command `command` shows.
fn of_command(command: &str) -> Vec<(String, CheckStatus)> {
    checks(&[passes(1_000, "t1", command, None)])
        .iter()
        .map(|c| (c.suite.clone(), c.status))
        .collect()
}

#[test]
fn a_word_that_is_only_an_argument_or_in_a_heredocs_body_or_quoted_is_no_check() {
    for command in [
        "strings target/debug/ci-checks",
        "echo cargo test",
        "cat > notes.md <<'EOF'\ncargo test\nEOF",
        "gh pr create --body \"run cargo test first\"",
        "echo \"say \\\" cargo test\"",
        "perl -e 'exec @ARGV' cargo test",
    ] {
        assert_eq!(of_command(command), [], "{command}");
    }
}

#[test]
fn a_backslash_escaped_quote_does_not_open_the_rest_of_the_command() {
    assert_eq!(
        of_command("echo \\\" && cargo test"),
        [("cargo test".to_string(), CheckStatus::Passed)]
    );
}

#[test]
fn a_check_still_matches_behind_each_allowed_wrapper() {
    for command in [
        "time cargo test",
        "timeout 60 cargo test",
        "env RUST_LOG=1 cargo test",
        "RUST_LOG=1 cargo test",
        "nice cargo test",
        "exec cargo test",
        "command cargo test",
        "for x in a; do cargo test; done",
        "if true; then cargo test; fi",
        "cd x && cargo test",
        "(cargo test)",
        "cargo +nightly test -p core",
        "cargo test 2>&1",
    ] {
        let found = of_command(command);
        assert_eq!(found.len(), 1, "{command}: {found:?}");
        assert_eq!(found[0].0, "cargo test", "{command}");
    }
}

#[test]
fn cargo_run_bin_is_a_run_of_a_one_word_entry() {
    assert_eq!(
        of_command("cargo run -q --bin ci-checks"),
        [("ci-checks".to_string(), CheckStatus::Passed)]
    );
    assert_eq!(of_command("cargo run -q --bin other"), []);
}

/// The statuses of the runs of a turn with the one command `command`, which exits as `exit` says:
/// `None` for 0, else a failure with that code.
fn runs_of(command: &str, exit: Option<i32>) -> Vec<(String, CheckStatus, Option<i32>)> {
    let call = match exit {
        None => passes(1_000, "t1", command, None),
        Some(code) => fails(1_000, "t1", command, &format!("Exit code {code}\nfailures")),
    };
    checks(&[call])
        .iter()
        .map(|c| {
            (
                c.runs[0].statement.clone(),
                c.runs[0].status,
                c.runs[0].exit_code,
            )
        })
        .collect()
}

#[test]
fn a_chain_that_exits_zero_passes_each_check_in_its_last_group() {
    let statuses =
        |command| -> Vec<CheckStatus> { runs_of(command, None).into_iter().map(|r| r.1).collect() };

    assert_eq!(
        statuses("cargo fmt --check && cargo clippy"),
        [CheckStatus::Passed]
    );
    assert_eq!(
        statuses("cargo test && cargo clippy"),
        [CheckStatus::Passed, CheckStatus::Passed]
    );
    // Each run keeps its own statement.
    let statements: Vec<String> = runs_of("cargo test -p core && cargo clippy -q", None)
        .into_iter()
        .map(|r| r.0)
        .collect();
    assert_eq!(statements, ["cargo test -p core", "cargo clippy -q"]);
    // An earlier group is never counted.
    assert_eq!(
        statuses("cargo test; cargo clippy"),
        [CheckStatus::Ran, CheckStatus::Passed]
    );
}

#[test]
fn a_chain_that_exits_non_zero_blames_a_check_only_when_it_is_the_one_statement() {
    let ran = |statement: &str, code| (statement.to_string(), CheckStatus::Ran, Some(code));

    // Two checks: neither can be named.
    assert_eq!(
        runs_of("cargo test && cargo clippy", Some(101)),
        [ran("cargo test", 101), ran("cargo clippy", 101)]
    );
    // A `cd` does not hide it.
    assert_eq!(
        runs_of("cd x && cargo test", Some(101)),
        [("cargo test".to_string(), CheckStatus::Failed, Some(101))]
    );
    // Another statement may be the one that failed.
    assert_eq!(
        runs_of("git add -A && cargo test", Some(1)),
        [ran("cargo test", 1)]
    );
    // The earlier group is never counted; the last follows the rule.
    assert_eq!(
        runs_of("cargo test; cargo clippy", Some(101)),
        [
            ran("cargo test", 101),
            ("cargo clippy".to_string(), CheckStatus::Failed, Some(101))
        ]
    );
}

#[test]
fn a_masked_check_is_not_counted_whatever_the_exit_code() {
    for command in [
        "cargo test | tail",
        "cargo test || true",
        "! cargo test",
        "cargo test & wait",
    ] {
        for exit in [None, Some(1)] {
            let runs = runs_of(command, exit);
            assert_eq!(runs.len(), 1, "{command}");
            assert_eq!(runs[0].1, CheckStatus::Ran, "{command} {exit:?}");
        }
    }
}

#[test]
fn a_heredoc_edit_followed_by_cargo_test_gives_a_run_whose_statement_is_the_cargo_test() {
    let command = "cd core && python3 - <<'EOF'\nprint('cargo test')\nEOF\ncargo test -q";

    let found = checks(&[passes(1_000, "t1", command, None)]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Passed, 0)]);
    assert_eq!(found[0].runs[0].statement, "cargo test -q");
}

/// `calls` as one worktree-aware replay: every line of `calls[i]` gets `extra[i]`'s fields.
fn with(calls: Vec<String>, extra: Value) -> Vec<String> {
    calls
        .into_iter()
        .map(|l| {
            let mut v: Value = serde_json::from_str(&l).unwrap();
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            v.to_string()
        })
        .collect()
}

/// The worktree and status of each check of the turn.
fn trees(checks: &[Check]) -> Vec<(&str, &str, CheckStatus, u32)> {
    checks
        .iter()
        .map(|c| (c.suite.as_str(), c.worktree.as_str(), c.status, c.retries))
        .collect()
}

const OTHER: &str = "/Users/danielrispler/work/other";

#[test]
fn the_same_suite_in_two_worktrees_is_two_checks_and_one_pass_does_not_cover_the_other() {
    let found = checks(&[
        fails(1_000, "t1", "cargo test", "Exit code 101\nfailures"),
        with(
            passes(2_000, "t2", &format!("cd {OTHER} && cargo test"), None),
            json!({ "studio_touched_roots": [OTHER] }),
        ),
    ]);

    assert_eq!(
        trees(&found),
        [
            ("cargo test", CWD, CheckStatus::Failed, 0),
            ("cargo test", OTHER, CheckStatus::Passed, 0),
        ]
    );
    assert!(!found[0].foreign);
    assert!(found[1].foreign);
}

#[test]
fn a_failed_run_then_a_passing_run_in_the_same_worktree_is_one_passed_check_with_one_retry() {
    let found = checks(&[
        with(
            fails(1_000, "t1", "cargo test", "Exit code 101\nfailures"),
            json!({ "studio_touched_roots": [OTHER] }),
        ),
        with(
            passes(2_000, "t2", "cargo test", None),
            json!({ "studio_touched_roots": [OTHER] }),
        ),
    ]);

    assert_eq!(trees(&found), [("cargo test", CWD, CheckStatus::Passed, 1)]);
}

#[test]
fn a_run_is_in_the_worktree_of_the_last_absolute_cd_that_a_recorded_root_holds() {
    let roots = json!({ "studio_touched_roots": [OTHER, "/w/third"] });
    let command = format!("cd /w/third && cd {OTHER}/core && cargo test");

    let found = checks(&[with(passes(1_000, "t1", &command, None), roots)]);

    assert_eq!(
        trees(&found),
        [("cargo test", OTHER, CheckStatus::Passed, 0)]
    );
}

#[test]
fn a_run_with_no_cd_a_relative_cd_or_a_cd_no_root_holds_is_in_the_calls_first_root() {
    let roots = json!({ "studio_touched_roots": [OTHER] });
    for command in [
        "cargo test",
        "cd core && cargo test",
        "cd /elsewhere && cargo test",
        &format!("cd {OTHER} && cd core && cargo test"),
    ] {
        let found = checks(&[with(passes(1_000, "t1", command, None), roots.clone())]);
        let want = if command.starts_with("cd /Users") {
            OTHER
        } else {
            CWD
        };
        assert_eq!(
            trees(&found),
            [("cargo test", want, CheckStatus::Passed, 0)],
            "{command}"
        );
    }
}

#[test]
fn a_call_with_no_recorded_roots_is_in_the_sessions_worktree_or_its_workers_own() {
    let mut lines = vec![prompt()];
    lines.push(line(
        "SubagentStart",
        500,
        json!({ "agent_id": "w1", "agent_type": "general-purpose", "cwd": OTHER }),
    ));
    lines.extend(with(
        passes(1_000, "t1", "cargo test", None),
        json!({ "cwd": "" }),
    ));
    lines.extend(with(
        passes(2_000, "t2", "cargo clippy", Some("w1")),
        json!({ "cwd": "" }),
    ));
    let studio = Studio::replayed(&lines);

    let found = &studio.session(SESSION).unwrap().turns[0].checks;

    assert_eq!(
        trees(found),
        [
            ("cargo test", CWD, CheckStatus::Passed, 0),
            ("cargo clippy", OTHER, CheckStatus::Passed, 0),
        ]
    );
}

#[test]
fn the_banners_counts_keep_a_failure_in_one_worktree_apart_from_a_pass_in_another() {
    let mut lines = vec![prompt()];
    lines.extend(fails(1_000, "t1", "cargo test", "Exit code 101\nfailures"));
    lines.extend(with(
        passes(2_000, "t2", &format!("cd {OTHER} && cargo test"), None),
        json!({ "studio_touched_roots": [OTHER] }),
    ));
    lines.push(line(
        "Stop",
        5_000,
        json!({ "last_assistant_message": "done" }),
    ));
    let studio = Studio::replayed(&lines);

    let since = studio.session(SESSION).unwrap().since();

    assert_eq!((since.passed, since.failed, since.ran), (1, 1, 0));
}

/// The start of a Bash call of the session's own agent, with no result yet.
fn starts(ms: u64, id: &str, command: &str) -> String {
    let call =
        json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": id });
    line("PreToolUse", ms, call)
}

/// The result of a Bash call that started a background task `task`.
fn launches(ms: u64, id: &str, command: &str, task: &str) -> String {
    let call = json!({
        "tool_name": "Bash",
        "tool_input": { "command": command, "run_in_background": true },
        "tool_use_id": id,
        "tool_response": { "stdout": "", "stderr": "", "backgroundTaskId": task, "interrupted": false },
    });
    line("PostToolUse", ms, call)
}

/// The turn's end, then a wake-up prompt that reports `task` as `status`: the next turn.
fn notified(ms: u64, task: &str, status: &str) -> Vec<String> {
    let note = format!(
        "<task-notification><task-id>{task}</task-id><tool-use-id>x</tool-use-id><output-file>o</output-file><status>{status}</status><summary>s</summary></task-notification>"
    );
    vec![
        line("Stop", ms, json!({ "last_assistant_message": "waiting" })),
        line(
            "UserPromptSubmit",
            ms + 10,
            json!({ "prompt": note, "prompt_id": "p2" }),
        ),
    ]
}

fn turns(lines: &[String], of: usize) -> Vec<Check> {
    let mut all = vec![prompt()];
    all.extend(lines.iter().cloned());
    let studio = Studio::replayed(&all);
    studio.session(SESSION).unwrap().turns[of].checks.clone()
}

#[test]
fn a_check_that_started_is_running_with_its_start_until_its_result_ends_it() {
    let start = starts(1_000, "t1", "cargo test");
    let found = turns(std::slice::from_ref(&start), 0);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Running, 0)]);
    assert_eq!(found[0].runs[0].started_at, Some(1_793_265_001_000));

    let mut lines = vec![start];
    lines.push(passes(1_000, "t1", "cargo test", None).pop().unwrap());
    let found = turns(&lines, 0);
    assert_eq!(shown(&found), [("cargo test", CheckStatus::Passed, 0)]);
}

#[test]
fn a_result_that_landed_before_its_start_is_one_check() {
    let [start, end] = &passes(1_000, "t1", "cargo test", None)[..] else {
        panic!("two lines");
    };
    let found = turns(&[end.clone(), start.clone()], 0);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Passed, 0)]);
    assert_eq!(found[0].runs.len(), 1);
}

#[test]
fn a_check_that_did_not_run_is_not_in_the_turn_even_after_it_started() {
    let found = turns(
        &fails(
            1_000,
            "t1",
            "cargo test",
            "The user doesn't want to proceed with this tool use.",
        ),
        0,
    );

    assert_eq!(shown(&found), []);
}

#[test]
fn a_background_check_runs_until_its_notification_then_holds_what_it_reports() {
    let report = |status: &str| {
        let mut lines = vec![
            starts(1_000, "t1", "cargo test"),
            launches(1_100, "t1", "cargo test", "bk1"),
        ];
        let running = turns(&lines, 0);
        assert_eq!(shown(&running), [("cargo test", CheckStatus::Running, 0)]);
        lines.extend(notified(5_000, "bk1", status));
        // The turn that started it shows the result, though the notification landed in the next.
        shown_owned(turns(&lines, 0))
    };

    assert_eq!(
        report("completed"),
        [("cargo test".into(), CheckStatus::Passed, 0)]
    );
    assert_eq!(
        report("failed"),
        [("cargo test".into(), CheckStatus::Failed, 0)]
    );
    assert_eq!(
        report("killed"),
        [("cargo test".into(), CheckStatus::Ran, 0)]
    );
}

fn shown_owned(checks: Vec<Check>) -> Vec<(String, CheckStatus, u32)> {
    checks
        .into_iter()
        .map(|c| (c.suite, c.status, c.retries))
        .collect()
}

#[test]
fn a_shell_background_check_is_not_counted() {
    let found = checks(&[passes(1_000, "t1", "cargo test &", None)]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Ran, 0)]);
}

#[test]
fn a_turn_with_only_a_running_check_has_nothing_passed_or_failed_to_count() {
    let mut all = vec![prompt(), starts(1_000, "t1", "cargo test")];
    all.push(line(
        "Stop",
        2_000,
        json!({ "last_assistant_message": "x" }),
    ));
    let studio = Studio::replayed(&all);
    let session = studio.session(SESSION).unwrap();

    let since = session.since();
    assert_eq!((since.passed, since.failed, since.ran), (0, 0, 0));
}

#[test]
fn a_running_or_background_check_of_a_worker_with_no_roots_keeps_its_workers_worktree() {
    let worker = line(
        "SubagentStart",
        500,
        json!({ "agent_id": "w1", "agent_type": "general-purpose", "cwd": OTHER }),
    );
    let call = |ms, id, extra: &str| {
        let call = json!({ "tool_name": "Bash", "tool_input": { "command": "cargo test", "run_in_background": extra == "end" }, "tool_use_id": id, "agent_id": "w1", "cwd": "" });
        let mut call = call;
        if extra == "end" {
            call["tool_response"] = json!({ "stdout": "", "stderr": "", "backgroundTaskId": "bk1", "interrupted": false });
        }
        line(
            if extra == "end" {
                "PostToolUse"
            } else {
                "PreToolUse"
            },
            ms,
            call,
        )
    };

    let running = turns(&[worker.clone(), call(1_000, "t1", "start")], 0);
    assert_eq!(
        trees(&running),
        [("cargo test", OTHER, CheckStatus::Running, 0)]
    );

    let mut lines = vec![worker, call(1_000, "t1", "start"), call(1_100, "t1", "end")];
    lines.extend(notified(5_000, "bk1", "failed"));
    let reported = turns(&lines, 0);
    assert_eq!(
        trees(&reported),
        [("cargo test", OTHER, CheckStatus::Failed, 0)]
    );
}

#[test]
fn a_here_string_does_not_hide_the_commands_after_it() {
    let found = turns(
        &passes(1_000, "t1", "grep x <<< \"$v\"\ncargo test", None),
        0,
    );

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Passed, 0)]);
}

#[test]
fn a_result_that_lands_after_a_new_turn_opened_ends_the_check_of_its_own_turn() {
    let lines = vec![
        starts(1_000, "t1", "cargo test"),
        line(
            "UserPromptSubmit",
            2_000,
            json!({ "prompt": "Next", "prompt_id": "p2" }),
        ),
        passes(1_000, "t1", "cargo test", None).pop().unwrap(),
    ];

    assert_eq!(
        shown(&turns(&lines, 0)),
        [("cargo test", CheckStatus::Passed, 0)]
    );
}

#[test]
fn a_repeated_notification_is_not_applied_again() {
    let mut lines = vec![
        starts(1_000, "t1", "cargo test"),
        launches(1_100, "t1", "cargo test", "bk1"),
    ];
    lines.extend(notified(5_000, "bk1", "completed"));
    let once = {
        let mut all = vec![prompt()];
        all.extend(lines.iter().cloned());
        Studio::replayed(&all).typical("", "cargo test")
    };
    assert!(once.is_some());
    let mut again = notified(9_000, "bk1", "failed");
    again[1] = again[1].replace("\"p2\"", "\"p3\"");
    lines.extend(again);
    let mut all = vec![prompt()];
    all.extend(lines.iter().cloned());
    let studio = Studio::replayed(&all);

    assert_eq!(studio.typical("", "cargo test"), once);
    assert_eq!(
        shown(&studio.session(SESSION).unwrap().turns[0].checks),
        [("cargo test", CheckStatus::Passed, 0)]
    );
}

#[test]
fn a_quoted_assignment_or_a_command_substitution_before_a_check_does_not_hide_it() {
    for command in [
        "RUSTFLAGS=\"-D warnings\" cargo test",
        "FOO='bar' npm run check",
        "CARGO_BUILD_JOBS=$(( $(sysctl -n hw.ncpu) / 3 )) NEXTEST_TEST_THREADS=$(( $(sysctl -n hw.ncpu) / 3 )) npm run check",
        "X=$(echo \"a ) b\") cargo test",
        "echo $((1<<2))\ncargo test",
    ] {
        let found = of_command(command);
        assert_eq!(found.len(), 1, "{command}");
        assert_eq!(found[0].1, CheckStatus::Passed, "{command}");
    }
    // What a substitution runs is no statement of the command.
    assert_eq!(of_command("echo $(cargo test)"), []);
}

#[test]
fn a_negation_anywhere_in_the_last_group_masks_the_exit_code() {
    for exit in [None, Some(1)] {
        let runs = runs_of("cd /repo && ! cargo test", exit);
        assert_eq!(runs[0].1, CheckStatus::Ran, "{exit:?}");
    }
}

#[test]
fn pipefail_counts_only_when_an_earlier_statement_turns_it_on() {
    let status = |command: &str, exit| runs_of(command, exit)[0].1;

    for on in ["set -o pipefail", "set -euo pipefail", "set -eo pipefail"] {
        let command = format!("{on}; cargo test 2>&1 | tail -20");
        assert_eq!(status(&command, None), CheckStatus::Passed, "{command}");
        assert_eq!(
            status(&command, Some(101)),
            CheckStatus::Failed,
            "{command}"
        );
    }
    for command in [
        "set +o pipefail; cargo test | tail",
        "set -o pipefail; set +o pipefail; cargo test | tail",
        "cargo test | tail; set -o pipefail",
        "cargo test | tail",
    ] {
        for exit in [None, Some(101)] {
            assert_eq!(
                status(command, exit),
                CheckStatus::Ran,
                "{command} {exit:?}"
            );
        }
    }
}

#[test]
fn a_foreground_check_after_a_background_statement_is_counted() {
    for command in ["server &\ncargo test", "server & cargo test"] {
        assert_eq!(
            runs_of(command, None)[0].1,
            CheckStatus::Passed,
            "{command}"
        );
        assert_eq!(
            runs_of(command, Some(101))[0].1,
            CheckStatus::Failed,
            "{command}"
        );
    }
    for exit in [None, Some(1)] {
        assert_eq!(runs_of("cargo test &", exit)[0].1, CheckStatus::Ran);
    }
}

#[test]
fn a_tool_that_is_no_command_is_no_check_whatever_its_input_says() {
    let call = |name: &str| {
        let call = json!({ "tool_name": name, "tool_input": { "command": "cargo test" }, "tool_use_id": "t1" });
        let mut end = call.clone();
        end["tool_response"] = json!({ "stdout": "ok" });
        vec![
            line("PreToolUse", 1_000, call),
            line("PostToolUse", 1_100, end),
        ]
    };
    // A later check of the turn reads the whole ledger again.
    let later = passes(2_000, "t2", "cargo clippy", None);
    for name in ["mcp__shell__run", "Task"] {
        let mut lines = vec![prompt()];
        lines.extend(call(name));
        lines.extend(later.clone());
        let studio = Studio::replayed(&lines);
        let found = &studio.session(SESSION).unwrap().turns[0].checks;
        assert_eq!(
            shown(found),
            [("cargo clippy", CheckStatus::Passed, 0)],
            "{name}"
        );
        assert_eq!(studio.typical(CWD, "cargo test"), None, "{name}");
    }
}
