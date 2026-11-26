//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for naming and the context's tokens (danielrispler/Falinks#480, decision 19;
//! the share's tests are in `archive/core/tests/naming.rs`, #416): hook events through the core, with a made-up transcript fixture (generator in
//! the PR's notes).

use crate::common;
use serde_json::{Value, json};
use studio_core::{BranchQuery, Context, Studio};

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
        [BranchQuery {
            project: MAIN.into(),
            cwd: TASK.into(),
            branch: "issue-480-naming".into(),
        }]
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
