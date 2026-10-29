//! Seam 1 (#328, Testing Decisions): replay JSON Lines fixtures of hook events through the core and
//! check what the pane would show. The fixtures follow the real v0 payload sequences
//! (https://github.com/danielrispler/Falinks/issues/321#issuecomment-6492980442, "Hook payloads").

use studio_core::{CardRequest, Output, Step, Studio};

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

/// The card requests the core makes when every line arrives live.
fn live(log: &str) -> Vec<CardRequest> {
    restart_after(log, 0)
}

#[test]
fn a_waits_first_edit_gets_a_card_and_its_later_edits_do_not() {
    let cards = live(include_str!("fixtures/first-edit.jsonl"));

    assert_eq!(
        cards,
        [CardRequest {
            session_id: "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092".into(),
            tool_use_id: "toolu_01Gw2Ec4Hs6Lj8Mx0Pz2Br4D".into(),
            step: Step {
                tool: "Edit".into(),
                detail: Some("apps/studio/core/src/lib.rs".into()),
            },
            change: concat!(
                "@@ -57,3 +57,3 @@\n",
                "         return Some(relative.unwrap_or(path).into());\n",
                "     }\n",
                "-    [\"pattern\", \"url\", \"query\", \"description\"]\n",
                "+    [\"pattern\", \"url\", \"query\", \"description\", \"prompt\"]\n",
            )
            .into(),
            check: None,
        }]
    );
}

#[test]
fn plan_files_plan_approvals_questions_and_read_only_commands_get_no_card() {
    let log = include_str!("fixtures/ask-then-plan.jsonl");
    let lines: Vec<&str> = log.lines().collect();
    let before_the_edit = lines[..15].join("\n");

    assert_eq!(live(&before_the_edit), []);
    let steps: Vec<Step> = live(log).into_iter().map(|card| card.step).collect();
    assert_eq!(steps, [step("Edit", Some("README.md")).unwrap()]);
}

#[test]
fn answering_a_permission_keeps_the_wait_so_the_next_edit_gets_no_card() {
    let cards = live(include_str!("fixtures/permission-write-then-edit.jsonl"));

    let steps: Vec<(Step, String)> = cards.into_iter().map(|c| (c.step, c.change)).collect();
    let new_file = (step("Write", Some("probe.txt")).unwrap(), "+probe\n".into());
    assert_eq!(steps, [new_file]);
}

#[test]
fn a_new_prompt_after_an_interrupt_opens_a_new_wait_with_its_own_card() {
    let cards = live(include_str!("fixtures/interrupt-then-prompt.jsonl"));

    let steps: Vec<Step> = cards.into_iter().map(|card| card.step).collect();
    let lib = step("Edit", Some("apps/studio/core/src/lib.rs")).unwrap();
    let tests = step("Edit", Some("apps/studio/core/tests/replay.rs")).unwrap();
    assert_eq!(steps, [lib, tests]);
}

/// The card requests after an app start that replays the first `replayed` lines of the log and then
/// gets the rest live.
fn restart_after(log: &str, replayed: usize) -> Vec<CardRequest> {
    let lines: Vec<&str> = log.lines().collect();
    let mut studio = replay(&lines[..replayed].join("\n"));
    lines[replayed..]
        .iter()
        .flat_map(|line| studio.apply(line))
        .filter_map(|output| match output {
            Output::Card(request) => Some(request),
            _ => None,
        })
        .collect()
}

#[test]
fn a_restart_mid_wait_gives_no_second_card_and_a_replay_gives_none() {
    let log = include_str!("fixtures/first-edit.jsonl");
    let edit = step("Edit", Some("apps/studio/core/src/lib.rs")).unwrap();
    let carded = |replayed| -> Vec<Step> {
        let cards = restart_after(log, replayed);
        cards.into_iter().map(|card| card.step).collect()
    };

    assert_eq!(carded(log.lines().count()), []);
    assert_eq!(carded(7), []);
    assert_eq!(carded(5), [edit]);
}
