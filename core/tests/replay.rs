//! Seam 1 (#328, Testing Decisions): replay JSON Lines fixtures of hook events through the core and
//! check what the pane would show. The fixtures follow the real v0 payload sequences
//! (https://github.com/danielrispler/Falinks/issues/321#issuecomment-6492980442, "Hook payloads").

use studio_core::{Step, Studio};

fn replay(log: &str) -> Studio {
    let mut studio = Studio::default();
    for line in log.lines() {
        studio.apply(line);
    }
    studio
}

fn step(tool: &str, detail: Option<&str>) -> Option<Step> {
    Some(Step {
        tool: tool.into(),
        detail: detail.map(Into::into),
    })
}

#[test]
fn a_failed_command_is_still_the_last_step() {
    let studio = replay(include_str!("fixtures/failing-bash.jsonl"));

    let now = studio.prompted_last().expect("a session was prompted");
    assert_eq!(now.id, "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31");
    assert_eq!(now.last_step, step("Bash", Some("ls does-not-exist")));
}

#[test]
fn a_permission_prompt_keeps_the_step_it_asks_about() {
    let studio = replay(include_str!("fixtures/permission-write.jsonl"));

    let now = studio.prompted_last().expect("a session was prompted");
    assert_eq!(now.last_step, step("Write", Some("probe.txt")));
}

#[test]
fn the_pane_follows_the_session_prompted_last_while_another_keeps_working() {
    let studio = replay(include_str!("fixtures/two-sessions.jsonl"));

    let now = studio.prompted_last().expect("a session was prompted");
    assert_eq!(now.id, "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807");
    assert_eq!(
        now.cwd,
        "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-322-terminals"
    );
    assert_eq!(now.last_step, step("Grep", Some("STUDIO_TERMINAL_ID")));

    let earlier = studio
        .session("c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64")
        .expect("first session");
    assert_eq!(earlier.last_step, step("Edit", Some("src/lib.rs")));
}

#[test]
fn questions_and_plan_approvals_inside_a_wait_leave_the_steps_in_order() {
    let log = include_str!("fixtures/ask-then-plan.jsonl");
    let lines: Vec<&str> = log.lines().collect();
    let at_question = replay(&lines[..5].join("\n"));
    let at_end = replay(log);

    let asked = at_question.prompted_last().expect("a session was prompted");
    assert_eq!(
        asked.last_step,
        step(
            "AskUserQuestion",
            Some("Which README section should I fix?")
        )
    );
    let done = at_end.prompted_last().expect("a session was prompted");
    assert_eq!(done.last_step, step("Edit", Some("README.md")));
}

#[test]
fn lines_that_are_not_hook_events_are_skipped() {
    let mut log = String::from("not json\n{\"hook_event_name\":\"Stop\"}\n\n");
    log += include_str!("fixtures/failing-bash.jsonl");
    log += "{\"session_id\":\"7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31\",\"hook_event_name\":\"PreToolUse\",\"tool_na";

    let studio = replay(&log);

    let now = studio.prompted_last().expect("a session was prompted");
    assert_eq!(now.last_step, step("Bash", Some("ls does-not-exist")));
}

#[test]
fn a_log_without_prompts_has_no_session_to_follow() {
    let first_line = include_str!("fixtures/failing-bash.jsonl")
        .lines()
        .next()
        .unwrap();
    assert_eq!(replay(first_line).prompted_last(), None);
    assert_eq!(replay("").prompted_last(), None);
}
