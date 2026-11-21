//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for naming and context share (danielrispler/Falinks#480, decision 19; danielrispler/Falinks#517,
//! decisions 1 and 2): hook events through the core, with a made-up transcript fixture (generator in
//! the PR's notes).

use crate::common;
use serde_json::{Value, json};
use studio_core::{Context, Studio};

const NAMING: &str = include_str!("fixtures/transcripts/naming.jsonl");
const SESSION: &str = "7d2c9a4e-1f3b-4c6d-9e8a-5b0f2c7d1a36";
const TASK: &str = "/Users/danielrispler/work/Falinks/.claude/worktrees/agent-a1b2c3d4";
const MAIN: &str = "/Users/danielrispler/work/Falinks";

/// A hook event of the session's own agent in folder `cwd`, with transcript `path`.
fn event(name: &str, cwd: &str, path: &std::path::Path, extra: Value) -> String {
    let mut base = common::base(SESSION, name, cwd, 1793300000000);
    base["transcript_path"] = json!(path);
    common::with(base, extra)
}

/// A session in `cwd` at its first step, `command`, with a transcript of its own holding `lines`.
fn session(cwd: &str, lines: &str, command: &str) -> (Studio, tempfile::NamedTempFile) {
    let file = common::temp_file(lines);
    let path = file.path();
    let step =
        json!({"tool_name": "Bash", "tool_input": {"command": command}, "tool_use_id": "toolu_1"});
    let studio = Studio::replayed([
        event(
            "UserPromptSubmit",
            cwd,
            path,
            json!({"prompt": "(scrubbed)"}),
        ),
        event("PreToolUse", cwd, path, step),
    ]);
    (studio, file)
}

fn first(lines: usize) -> String {
    NAMING
        .lines()
        .take(lines)
        .map(|l| format!("{l}\n"))
        .collect()
}

fn follow(studio: &mut Studio) {
    studio.follow_transcripts(|_| {});
}

fn context(studio: &Studio) -> Option<Context> {
    studio.session(SESSION).unwrap().transcript.context.clone()
}

/// The context's tokens, window, share and what is left of the window.
fn measured(studio: &Studio) -> (u64, Option<u64>, Option<u64>, Option<u64>) {
    let c = context(studio).unwrap();
    (c.tokens, c.window, c.share, c.left)
}

/// A status line snapshot's `context_window` of a 1M window, its `current_usage` holding `cached`
/// cache-read tokens, with Claude's own percentages when `percentages` gives them.
fn snapshot(cached: u64, percentages: Option<(u64, u64)>) -> Option<Value> {
    let mut window = json!({
        "total_input_tokens": 133_671,
        "context_window_size": 1_000_000,
        "current_usage": {"input_tokens": 2, "output_tokens": 8, "cache_creation_input_tokens": 4_901, "cache_read_input_tokens": cached},
    });
    if let Some((used, left)) = percentages {
        window["used_percentage"] = json!(used);
        window["remaining_percentage"] = json!(left);
    }
    Some(window)
}

#[test]
fn the_context_follows_the_last_assistant_messages_token_counts() {
    let (mut studio, file) = session(TASK, &first(3), "gh issue view 480");

    follow(&mut studio);
    // 2 + 30,000 + 70,000 input tokens.
    let tokens = |studio: &Studio| context(studio).map(|c| c.tokens);
    assert_eq!(tokens(&studio), Some(100_002));

    // A helper's line is its own context; the agent's next message counts.
    std::fs::write(file.path(), first(6)).unwrap();
    follow(&mut studio);
    assert_eq!(tokens(&studio), Some(250_004));

    // A new prompt starts a turn, not a context.
    std::fs::write(file.path(), first(7)).unwrap();
    follow(&mut studio);
    assert_eq!(tokens(&studio), Some(250_004));
}

#[test]
fn with_a_snapshot_the_context_is_claudes_status_line_view() {
    // danielrispler/Falinks#517, measurement 1's turn: the snapshot counts 2 + 4,901 + 84,899 = 89,802 tokens, its 9% used
    // and 91% left, whatever the transcript's latest message says.
    let (mut studio, file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);
    let claudes = snapshot(84_899, Some((9, 91)));

    assert!(studio.set_snapshot(SESSION, claudes.clone()));
    assert_eq!(
        measured(&studio),
        (89_802, Some(1_000_000), Some(9), Some(91))
    );
    // The same snapshot again changes nothing.
    assert!(!studio.set_snapshot(SESSION, claudes));
    // The transcript's next message keeps the snapshot's view.
    std::fs::write(file.path(), first(6)).unwrap();
    follow(&mut studio);
    assert_eq!(
        measured(&studio),
        (89_802, Some(1_000_000), Some(9), Some(91))
    );
}

#[test]
fn a_snapshot_without_percentages_rounds_to_nearest() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);

    // 2 + 4,901 + 197,797 = 202,700 tokens, 20.27% of the window: 20 used, 80 left, not 21.
    assert!(studio.set_snapshot(SESSION, snapshot(197_797, None)));
    assert_eq!(
        measured(&studio),
        (202_700, Some(1_000_000), Some(20), Some(80))
    );
}

#[test]
fn before_the_first_api_call_the_transcript_counts_in_the_snapshots_window() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);

    let mut start = snapshot(0, None).unwrap();
    start["current_usage"] = Value::Null;
    assert!(studio.set_snapshot(SESSION, Some(start)));
    // The transcript's 100,002 tokens of 1M: 10% used, 90% left.
    assert_eq!(
        measured(&studio),
        (100_002, Some(1_000_000), Some(10), Some(90))
    );
}

#[test]
fn with_no_snapshot_a_session_has_a_token_count_and_no_share() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");

    follow(&mut studio);
    assert!(!studio.set_snapshot(SESSION, None));
    assert_eq!(measured(&studio), (100_002, None, None, None));
}

fn name(studio: &Studio) -> String {
    studio.session(SESSION).unwrap().identity.name.clone()
}

#[test]
fn an_issue_branch_names_its_issue_its_pr_and_its_worktree() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);
    assert_eq!(name(&studio), "#480 · agent-a1b2c3d4");

    // The shell looks the branch's PR up off the poll.
    assert_eq!(
        studio.branches(),
        vec![(TASK.to_string(), "issue-480-naming".to_string())]
    );
    studio.set_pr("issue-480-naming", Some(490));
    assert_eq!(name(&studio), "#480 · PR #490 · agent-a1b2c3d4");
    let identity = &studio.session(SESSION).unwrap().identity;
    assert_eq!((identity.issue, identity.pr), (Some(480), Some(490)));
}

#[test]
fn a_branch_that_names_no_issue_shows_itself() {
    let lines = first(3).replace("issue-480-naming", "fix-typo");
    let (mut studio, _file) = session(TASK, &lines, "cargo test");
    follow(&mut studio);
    studio.set_pr("fix-typo", Some(12));
    assert_eq!(name(&studio), "fix-typo · PR #12 · agent-a1b2c3d4");
}

#[test]
fn a_worktree_named_after_its_branch_is_not_named_twice() {
    let task = "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-480-naming";
    let lines = first(3).replace(TASK, task);
    let (mut studio, _file) = session(task, &lines, "cargo test");
    follow(&mut studio);
    assert_eq!(name(&studio), "#480 · issue-480-naming");

    let task = "/Users/danielrispler/work/Falinks/.claude/worktrees/fix-typo";
    let lines = lines.replace("issue-480-naming", "fix-typo");
    let (mut studio, _file) = session(task, &lines, "cargo test");
    follow(&mut studio);
    assert_eq!(name(&studio), "fix-typo");
}

#[test]
fn the_main_checkout_shows_its_branch_and_folder_and_a_claimed_issue() {
    let lines = first(3)
        .replace("issue-480-naming", "main")
        .replace(TASK, MAIN);
    let (mut studio, _file) = session(MAIN, &lines, "cargo test");
    follow(&mut studio);
    assert_eq!(name(&studio), "main · Falinks");

    let (mut studio, _file) = session(
        MAIN,
        &lines,
        "cargo run --bin wayfinder-startup -- 266 --claim 480",
    );
    follow(&mut studio);
    assert_eq!(name(&studio), "#480 · main · Falinks");
}

#[test]
fn before_its_transcript_is_read_a_session_is_named_by_its_worktree() {
    let (studio, _file) = session(TASK, &first(3), "cargo test");
    assert_eq!(name(&studio), "agent-a1b2c3d4");
}
