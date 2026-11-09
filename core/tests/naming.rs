//! Seam 1 (#461, Testing Decisions) for naming and context share (#480, decision 19; #517,
//! decisions 1 and 2): hook events through the core, with a made-up transcript fixture (generator in
//! the PR's notes).

use studio_core::{Context, Studio};

const NAMING: &str = include_str!("fixtures/transcripts/naming.jsonl");
const SESSION: &str = "7d2c9a4e-1f3b-4c6d-9e8a-5b0f2c7d1a36";
const TASK: &str = "/Users/danielrispler/work/Falinks/.claude/worktrees/agent-a1b2c3d4";
const MAIN: &str = "/Users/danielrispler/work/Falinks";

/// A hook event of the session's own agent in folder `cwd`, with transcript `path`.
fn event(name: &str, cwd: &str, path: &std::path::Path, extra: &str) -> String {
    format!(
        r#"{{"session_id":"{SESSION}","hook_event_name":"{name}","cwd":"{cwd}","transcript_path":"{}","studio_received_ms":1793300000000{extra}}}"#,
        path.display()
    )
}

/// A session in `cwd` at its first step, `command`, with a transcript of its own for test `name`
/// holding `lines`.
fn session(name: &str, cwd: &str, lines: &str, command: &str) -> (Studio, std::path::PathBuf) {
    let file = format!("studio-naming-{name}-{}.jsonl", std::process::id());
    let path = std::env::temp_dir().join(file);
    std::fs::write(&path, lines).expect("write the transcript");
    let step = format!(
        r#","tool_name":"Bash","tool_input":{{"command":{}}},"tool_use_id":"toolu_1""#,
        serde_json::to_string(command).unwrap()
    );
    let studio = Studio::replayed([
        event("UserPromptSubmit", cwd, &path, r#","prompt":"(scrubbed)""#),
        event("PreToolUse", cwd, &path, &step),
    ]);
    (studio, path)
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
fn the_context_share_follows_the_last_assistant_messages_token_counts() {
    let (mut studio, path) = session("context", TASK, &first(3), "gh issue view 480");
    let window = Some(1_000_000);

    follow(&mut studio);
    assert!(studio.set_window(SESSION, window));
    // 2 + 30,000 + 70,000 input tokens.
    let tokens = |studio: &Studio| context(studio).map(|c| c.tokens);
    assert_eq!(tokens(&studio), Some(100_002));

    // A helper's line is its own context; the agent's next message counts, in the same window.
    std::fs::write(&path, first(6)).unwrap();
    follow(&mut studio);
    let latest = Some(Context {
        tokens: 250_004,
        window,
        share: Some(26),
    });
    assert_eq!(context(&studio), latest);

    // A new prompt starts a turn, not a context.
    std::fs::write(&path, first(7)).unwrap();
    follow(&mut studio);
    assert_eq!(context(&studio), latest);
}

#[test]
fn the_window_comes_from_the_status_line_snapshot_not_the_model() {
    // #517, measurement 1: 84.8k tokens, and a model without the `[1m]` mark.
    let lines = first(3)
        .replace("claude-opus-5-5[1m]", "claude-opus-5-5")
        .replace(
            "\"cache_read_input_tokens\":70000",
            "\"cache_read_input_tokens\":54798",
        );
    let (mut studio, _) = session("snapshot", TASK, &lines, "gh issue view 480");

    follow(&mut studio);
    assert!(studio.set_window(SESSION, Some(1_000_000)));
    let share = context(&studio).and_then(|c| c.share);
    assert_eq!(share, Some(9));
    // The same window again changes nothing.
    assert!(!studio.set_window(SESSION, Some(1_000_000)));
}

#[test]
fn with_no_snapshot_a_session_has_a_token_count_and_no_share() {
    let (mut studio, _) = session("no-snapshot", TASK, &first(3), "gh issue view 480");

    follow(&mut studio);
    assert!(!studio.set_window(SESSION, None));
    let context = context(&studio).unwrap();
    assert_eq!(
        (context.tokens, context.window, context.share),
        (100_002, None, None)
    );
}

fn name(studio: &Studio) -> String {
    studio.session(SESSION).unwrap().identity.name.clone()
}

#[test]
fn an_issue_branch_names_its_issue_its_pr_and_its_worktree() {
    let (mut studio, _) = session("issue", TASK, &first(3), "gh issue view 480");
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
    let (mut studio, _) = session("branch", TASK, &lines, "cargo test");
    follow(&mut studio);
    studio.set_pr("fix-typo", Some(12));
    assert_eq!(name(&studio), "fix-typo · PR #12 · agent-a1b2c3d4");
}

#[test]
fn a_worktree_named_after_its_branch_is_not_named_twice() {
    let task = "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-480-naming";
    let lines = first(3).replace(TASK, task);
    let (mut studio, _) = session("same", task, &lines, "cargo test");
    follow(&mut studio);
    assert_eq!(name(&studio), "#480 · issue-480-naming");

    let task = "/Users/danielrispler/work/Falinks/.claude/worktrees/fix-typo";
    let lines = lines.replace("issue-480-naming", "fix-typo");
    let (mut studio, _) = session("same-branch", task, &lines, "cargo test");
    follow(&mut studio);
    assert_eq!(name(&studio), "fix-typo");
}

#[test]
fn the_main_checkout_shows_its_branch_and_folder_and_a_claimed_issue() {
    let lines = first(3)
        .replace("issue-480-naming", "main")
        .replace(TASK, MAIN);
    let (mut studio, _) = session("main", MAIN, &lines, "cargo test");
    follow(&mut studio);
    assert_eq!(name(&studio), "main · Falinks");

    let (mut studio, _) = session(
        "claim",
        MAIN,
        &lines,
        "cargo run --bin wayfinder-startup -- 266 --claim 480",
    );
    follow(&mut studio);
    assert_eq!(name(&studio), "#480 · main · Falinks");
}

#[test]
fn before_its_transcript_is_read_a_session_is_named_by_its_worktree() {
    let (studio, _) = session("unread", TASK, &first(3), "cargo test");
    assert_eq!(name(&studio), "agent-a1b2c3d4");
}
