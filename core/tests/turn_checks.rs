//! Seam 1 (danielrispler/plotkeeper#102, Testing Decisions) for a turn's checks
//! (danielrispler/plotkeeper#112): hook events replayed through the core, and how each turn's checks
//! ended, from the step ledger's rows.

use serde_json::{Value, json};
use studio_core::{Check, CheckStatus, Studio, check_output};

const SESSION: &str = "7d3e9a51-2b8c-4f6e-a0d7-5c1b9e3f8a24";
const CWD: &str = "/Users/danielrispler/work/plotkeeper";

fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut line = json!({
        "session_id": SESSION,
        "hook_event_name": name,
        "cwd": CWD,
        "prompt_id": "p1",
        "studio_received_ms": 1_793_265_000_000u64 + ms,
    });
    line.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    line.to_string()
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
    let dir = std::env::temp_dir().join(format!("turn-checks-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("events.jsonl");
    std::fs::write(&log, text).unwrap();
    let studio = Studio::replayed(&lines);
    let ledger = &studio.session(SESSION).unwrap().ledger;

    let failed = check_output(ledger.row(None, "t1").unwrap(), &log).unwrap();
    let passed = check_output(ledger.row(None, "t2").unwrap(), &log).unwrap();

    assert_eq!(failed.exit_code, Some(2));
    assert!(failed.tail.ends_with("error: it broke"), "{}", failed.tail);
    assert_eq!(passed.exit_code, Some(0));
    assert_eq!(passed.tail, "ok\nall passed");
    std::fs::remove_dir_all(dir).unwrap();
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

#[test]
fn a_command_with_two_checks_gives_a_check_of_each_with_its_own_statement_not_counted() {
    for command in [
        "cargo test -p core; cargo clippy --all 2>&1",
        "cargo test -p core && cargo clippy --all",
    ] {
        let found = checks(&[passes(1_000, "t1", command, None)]);
        let statements: Vec<(&str, &str, CheckStatus)> = found
            .iter()
            .map(|c| (c.suite.as_str(), c.runs[0].statement.as_str(), c.status))
            .collect();
        assert_eq!(
            statements,
            [
                ("cargo test", "cargo test -p core", CheckStatus::Ran),
                ("cargo clippy", "cargo clippy --all", CheckStatus::Ran),
            ],
            "{command}"
        );
    }
}

#[test]
fn a_heredoc_edit_followed_by_cargo_test_gives_a_run_whose_statement_is_the_cargo_test() {
    let command = "cd core && python3 - <<'EOF'\nprint('cargo test')\nEOF\ncargo test -q";

    let found = checks(&[passes(1_000, "t1", command, None)]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Passed, 0)]);
    assert_eq!(found[0].runs[0].statement, "cargo test -q");
}
