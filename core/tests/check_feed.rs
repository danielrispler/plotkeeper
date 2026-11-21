//! Seam 1 (spec #254, ticket #268) for the checks the rail's feed lists: hook events replayed through
//! the core, read back from each turn as the window receives it.

use crate::common;
use serde_json::{Value, json};
use studio_core::{CheckStatus, Listed, Studio};

const SESSION: &str = "5b1d7f20-3c9a-4e8b-b6d4-0a2f8c1e7d39";
const CWD: &str = "/Users/danielrispler/work/plotkeeper";
const OTHER: &str = "/Users/danielrispler/work/plotkeeper/.claude/worktrees/other";
const T0: u64 = 1_793_265_000_000;
const MIN: u64 = 60_000;

fn line(name: &str, ms: u64, extra: Value) -> String {
    let mut base = common::base(SESSION, name, CWD, T0 + ms);
    base["prompt_id"] = json!("p1");
    common::with(base, extra)
}

fn prompt() -> String {
    line("UserPromptSubmit", 0, json!({ "prompt": "Check it" }))
}

fn call(id: &str, command: &str, agent: Option<&str>) -> Value {
    let mut call =
        json!({ "tool_name": "Bash", "tool_input": { "command": command }, "tool_use_id": id });
    if let Some(agent) = agent {
        call["agent_id"] = agent.into();
    }
    call
}

fn starts(ms: u64, id: &str, command: &str) -> String {
    line("PreToolUse", ms, call(id, command, None))
}

/// A call of `agent` that starts at `ms` and ends at `ms + took`, passing or with exit code 1.
fn runs(
    ms: u64,
    took: u64,
    id: &str,
    command: &str,
    agent: Option<&str>,
    pass: bool,
) -> Vec<String> {
    let start = call(id, command, agent);
    let mut end = start.clone();
    let name = if pass {
        end["tool_response"] = json!({ "stdout": "ok", "stderr": "" });
        "PostToolUse"
    } else {
        end["error"] = "Exit code 1\nfailures".into();
        "PostToolUseFailure"
    };
    vec![line("PreToolUse", ms, start), line(name, ms + took, end)]
}

fn listed(lines: Vec<String>) -> Vec<Listed> {
    let mut all = vec![prompt()];
    all.extend(lines);
    let studio = Studio::replayed(&all);
    studio.session(SESSION).unwrap().turns[0].listed.clone()
}

fn shown(listed: &[Listed]) -> Vec<(&str, CheckStatus)> {
    listed
        .iter()
        .map(|l| (l.statement.as_str(), l.status))
        .collect()
}

#[test]
fn a_running_check_is_listed_with_its_own_statement_and_start() {
    let found = listed(vec![starts(
        1_000,
        "t1",
        "cd /x && python3 - <<'EOF'\nprint(1)\nEOF\ncargo test",
    )]);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Running)]);
    assert_eq!(found[0].started_at, T0 + 1_000);
    assert_eq!(found[0].ended_at, None);
}

#[test]
fn a_failed_check_is_listed_and_a_passed_one_or_one_that_passed_on_a_retry_is_not() {
    let mut lines = runs(1_000, 100, "t1", "cargo test", None, false);
    lines.extend(runs(2_000, 100, "t2", "cargo clippy", None, false));
    lines.extend(runs(3_000, 100, "t3", "cargo clippy", None, true));
    lines.extend(runs(4_000, 100, "t4", "npm run check", None, true));
    let found = listed(lines);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Failed)]);
    assert_eq!(found[0].ended_at, Some(T0 + 1_100));
}

#[test]
fn the_listed_checks_are_newest_first() {
    let mut lines = runs(1_000, 100, "t1", "cargo test", None, false);
    lines.extend(runs(2_000, 100, "t2", "cargo clippy", None, false));
    let found = listed(lines);

    assert_eq!(
        shown(&found),
        [
            ("cargo clippy", CheckStatus::Failed),
            ("cargo test", CheckStatus::Failed)
        ]
    );
}

#[test]
fn a_workers_run_is_not_listed() {
    let found = listed(runs(1_000, 100, "t1", "cargo test", Some("w1"), false));

    assert_eq!(found, []);
}

#[test]
fn a_running_check_shows_the_typical_duration_of_its_statement_across_worktrees() {
    let mut lines = runs(1_000, 6 * MIN, "t1", "cargo test", None, true);
    let mut other = runs(
        7 * MIN,
        8 * MIN,
        "t2",
        &format!("cd {OTHER} && cargo test"),
        None,
        true,
    );
    for l in &mut other {
        let mut event: Value = serde_json::from_str(l).unwrap();
        event["studio_touched_roots"] = json!([OTHER]);
        *l = event.to_string();
    }
    lines.extend(other);
    // Behind a heredoc edit, in a command of its own.
    lines.push(starts(
        20 * MIN,
        "t3",
        "python3 - <<'EOF'\nprint(1)\nEOF\ncargo test",
    ));
    let found = listed(lines);

    assert_eq!(shown(&found), [("cargo test", CheckStatus::Running)]);
    assert_eq!(found[0].typical_ms, Some(6 * MIN));
}

#[test]
fn a_command_of_two_checks_is_timed_by_both_statements_joined() {
    let mut lines = runs(
        1_000,
        3 * MIN,
        "t1",
        "cargo test && cargo clippy",
        None,
        true,
    );
    lines.push(starts(10 * MIN, "t2", "cargo test && cargo clippy"));
    lines.push(starts(11 * MIN, "t3", "cargo test"));
    let mut all = vec![prompt()];
    all.extend(lines);
    let studio = Studio::replayed(&all);
    let found = &studio.session(SESSION).unwrap().turns[0].listed;

    let typical: Vec<(&str, Option<u64>)> = found
        .iter()
        .map(|l| (l.statement.as_str(), l.typical_ms))
        .collect();
    // The lone `cargo test` is another key, with no history.
    assert_eq!(
        typical,
        [("cargo test", None), ("cargo clippy", Some(3 * MIN))]
    );
}

#[test]
fn a_check_a_worker_later_passed_is_not_listed_though_the_agents_run_failed() {
    let mut lines = runs(1_000, 1_000, "t1", "cargo test", None, false);
    lines.extend(runs(5_000, 1_000, "t2", "cargo test", Some("w1"), true));

    assert_eq!(shown(&listed(lines)), []);
}
