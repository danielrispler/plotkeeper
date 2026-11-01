//! Seam 1 (#328, Testing Decisions): replay JSON Lines fixtures of hook events through the core and
//! check what the pane would show. The fixtures follow the real v0 payload sequences
//! (https://github.com/danielrispler/Falinks/issues/321#issuecomment-6492980442, "Hook payloads").

use studio_core::{Step, Studio};

fn replay(log: &str) -> Studio {
    let mut studio = Studio::default();
    for line in log.lines() {
        studio.replay(line);
    }
    studio
}

fn step(tool: &str, detail: Option<&str>) -> Option<Step> {
    Some(Step {
        tool: tool.into(),
        detail: detail.map(Into::into),
    })
}

/// The session of `failing-bash.jsonl`, `permission-write.jsonl` and others.
const PROBE: &str = "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31";

#[test]
fn a_failed_command_is_still_the_last_step() {
    let studio = replay(include_str!("fixtures/failing-bash.jsonl"));

    let probe = studio.session(PROBE).expect("the session");
    assert_eq!(probe.last_step, step("Bash", Some("ls does-not-exist")));
}

#[test]
fn a_permission_prompt_keeps_the_step_it_asks_about() {
    let studio = replay(include_str!("fixtures/permission-write.jsonl"));

    let probe = studio.session(PROBE).expect("the session");
    assert_eq!(probe.last_step, step("Write", Some("probe.txt")));
}

/// Before #371 the pane followed the session prompted last (#328, user story 33). Now only Daniel's
/// click changes it (#369, decision 20).
#[test]
fn the_selection_holds_when_another_session_is_prompted() {
    let log = include_str!("fixtures/two-sessions.jsonl");
    let lines: Vec<&str> = log.lines().collect();
    let first = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
    let mut studio = replay(&lines[..3].join("\n"));
    studio.select(first);

    for line in &lines[3..] {
        studio.replay(line);
    }

    let selected = studio.selected().expect("a selected session");
    assert_eq!(selected.id, first);
    assert_eq!(selected.last_step, step("Edit", Some("src/lib.rs")));
    let other = studio
        .session("9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807")
        .expect("the session prompted last");
    assert_eq!(
        other.cwd,
        "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-322-terminals"
    );
    assert_eq!(other.last_step, step("Grep", Some("STUDIO_TERMINAL_ID")));
}

#[test]
fn questions_and_plan_approvals_inside_a_wait_leave_the_steps_in_order() {
    let log = include_str!("fixtures/ask-then-plan.jsonl");
    let lines: Vec<&str> = log.lines().collect();
    let at_question = replay(&lines[..5].join("\n"));
    let at_end = replay(log);

    let asker = "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95";
    let asked = at_question.session(asker).expect("the session");
    assert_eq!(
        asked.last_step,
        step(
            "AskUserQuestion",
            Some("Which README section should I fix?")
        )
    );
    let done = at_end.session(asker).expect("the session");
    assert_eq!(done.last_step, step("Edit", Some("README.md")));
}

#[test]
fn lines_that_are_not_hook_events_are_skipped() {
    let mut log = String::from("not json\n{\"hook_event_name\":\"Stop\"}\n\n");
    log += include_str!("fixtures/failing-bash.jsonl");
    log += "{\"session_id\":\"7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31\",\"hook_event_name\":\"PreToolUse\",\"tool_na";

    let studio = replay(&log);

    let probe = studio.session(PROBE).expect("the session");
    assert_eq!(probe.last_step, step("Bash", Some("ls does-not-exist")));
}

/// The two recorded logs (#370), scrubbed of their content: the checkpoint day's events, and the
/// scripted session that spawns a worker and a helper (`record-subagent-run.sh`). Each line names a
/// session, and the core holds every session named.
#[test]
fn the_recorded_logs_replay_through_the_core() {
    for log in [
        include_str!("fixtures/checkpoint-day.jsonl"),
        include_str!("fixtures/subagent-run.jsonl"),
    ] {
        let studio = replay(log);

        assert_ne!(log, "");
        for line in log.lines() {
            let event: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
            let id = event["session_id"]
                .as_str()
                .expect("an event names its session");
            assert!(studio.session(id).is_some(), "session {id} was dropped");
        }
    }
}

#[test]
fn nothing_is_selected_until_daniel_clicks() {
    let mut studio = replay(include_str!("fixtures/failing-bash.jsonl"));
    assert_eq!(studio.selected(), None);

    studio.select(PROBE);

    assert_eq!(studio.selected().map(|s| s.id.as_str()), Some(PROBE));
}
