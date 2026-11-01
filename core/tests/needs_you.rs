//! Seam 1 (#328, Testing Decisions) for "needs you" (#349): replay hook events through the core and
//! check each session's state after every line, and the notices only live events give. The fixtures
//! follow the real v0 payload sequences
//! (https://github.com/danielrispler/Falinks/issues/321#issuecomment-6492980442, "Hook payloads").

use studio_core::{Output, State, Studio};

use State::{Ended, NeedsYou, Running, YourTurn};

/// The session's state after each line of the log, replayed.
fn states(log: &str, session: &str) -> Vec<Option<State>> {
    let mut studio = Studio::default();
    log.lines()
        .map(|line| {
            studio.replay(line);
            studio.session(session).map(|s| s.state)
        })
        .collect()
}

/// The "needs you" notices when the log's lines arrive live.
fn live(log: &str) -> Vec<Output> {
    let mut studio = Studio::default();
    let outputs = log.lines().flat_map(|line| studio.apply(line));
    outputs
        .filter(|output| matches!(output, Output::NeedsYou { .. }))
        .collect()
}

fn notice(session: &str) -> Output {
    Output::NeedsYou {
        session_id: session.into(),
        agent: None,
    }
}

const PROBE: &str = "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31";

#[test]
fn each_time_a_session_newly_needs_you_gives_one_notice() {
    let permission = include_str!("fixtures/permission-write.jsonl");
    let background = include_str!("fixtures/background-agent.jsonl");
    let ask_then_plan = include_str!("fixtures/ask-then-plan.jsonl");
    let asker = "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95";

    // The Notification that follows a PermissionRequest gives no second notice.
    assert_eq!(live(permission), [notice(PROBE)]);
    assert_eq!(
        live(background),
        [notice("c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64")]
    );
    // AskUserQuestion, the Stop that asks, ExitPlanMode.
    assert_eq!(live(ask_then_plan), [asker, asker, asker].map(notice));
    assert_eq!(live(include_str!("fixtures/failing-bash.jsonl")), []);
}

#[test]
fn a_stop_clears_every_block_before_it_asks() {
    let lines: Vec<&str> = include_str!("fixtures/sandbox-network.jsonl")
        .lines()
        .collect();
    // The network request's command never reports back, so only the turn's end clears it.
    let unasked = [lines[0], lines[1], lines[2], lines[4]].join("\n");
    let asking = lines[4].replace(
        "`cargo fetch` finished; the crates are in the local registry.",
        "`cargo fetch` finished. Should I run the tests now?",
    );
    let asked = [lines[0], lines[1], lines[2], &asking].join("\n");

    assert_eq!(
        states(&unasked, PROBE),
        [Running, Running, NeedsYou, YourTurn].map(Some)
    );
    assert_eq!(
        states(&asked, PROBE),
        [Running, Running, NeedsYou, NeedsYou].map(Some)
    );
    // The Stop's question is a new reason to need Daniel, so it gives its own notice.
    assert_eq!(live(&asked), [notice(PROBE), notice(PROBE)]);
}

#[test]
fn a_permission_prompt_needs_you_until_its_tool_runs() {
    let log = include_str!("fixtures/permission-write.jsonl");

    // UserPromptSubmit, PreToolUse, PermissionRequest, Notification, PostToolUse.
    let expected = [Running, Running, NeedsYou, NeedsYou, Running].map(Some);
    assert_eq!(states(log, PROBE), expected);
}

#[test]
fn a_denied_permission_stops_needing_you_when_its_tool_fails() {
    let log = include_str!("fixtures/permission-denied.jsonl");

    // UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUseFailure.
    let expected = [Running, Running, NeedsYou, Running].map(Some);
    assert_eq!(states(log, PROBE), expected);
}

#[test]
fn a_permission_prompt_that_lands_before_its_tool_call_still_ends_when_the_tool_runs() {
    // The hooks are async, so a PermissionRequest can land before its PreToolUse.
    let lines: Vec<&str> = include_str!("fixtures/permission-write.jsonl")
        .lines()
        .collect();
    let log = [lines[0], lines[2], lines[1], lines[3], lines[4]].join("\n");

    // UserPromptSubmit, PermissionRequest, PreToolUse, Notification, PostToolUse.
    let expected = [Running, NeedsYou, NeedsYou, NeedsYou, Running].map(Some);
    assert_eq!(states(&log, PROBE), expected);
}

#[test]
fn a_permission_prompts_notification_that_lands_before_its_request_ends_with_the_tool() {
    let lines: Vec<&str> = include_str!("fixtures/permission-write.jsonl")
        .lines()
        .collect();
    let log = [lines[0], lines[1], lines[3], lines[2], lines[4]].join("\n");

    // UserPromptSubmit, PreToolUse, Notification, PermissionRequest, PostToolUse.
    let expected = [Running, Running, NeedsYou, NeedsYou, Running].map(Some);
    assert_eq!(states(&log, PROBE), expected);
    assert_eq!(live(&log), [notice(PROBE)]);
}

#[test]
fn a_command_that_never_reported_back_holds_no_block_in_the_next_turn() {
    let lines: Vec<&str> = include_str!("fixtures/sandbox-network.jsonl")
        .lines()
        .collect();
    let next = |line: &str| line.replace("toolu_01Pq5Rs7Tu9Vw1Xy3Za5Bc7D", "toolu_01Next");
    // The first turn's command never reports back. In the next turn, the network request lands
    // before its own command's PreToolUse.
    let log = [
        lines[0].into(),
        lines[1].into(),
        lines[2].into(),
        lines[4].into(),
        lines[0].into(),
        lines[2].into(),
        next(lines[1]),
        next(lines[3]),
    ]
    .join("\n");

    assert_eq!(states(&log, PROBE).last(), Some(&Some(Running)));
}

#[test]
fn a_background_agents_events_leave_the_main_agents_block_in_place() {
    let log = include_str!("fixtures/background-agent.jsonl");

    let expected = [
        // UserPromptSubmit, a background Agent's PreToolUse and PostToolUse.
        Running, Running, Running,
        // The main agent's Write: PreToolUse, PermissionRequest.
        Running, NeedsYou,
        // The background agent's Glob and failing Bash, each PreToolUse and its result.
        NeedsYou, NeedsYou, NeedsYou, NeedsYou,
        // Notification, then the Write's PostToolUse.
        NeedsYou, Running,
    ]
    .map(Some);
    assert_eq!(
        states(log, "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64"),
        expected
    );
}

#[test]
fn a_sandboxed_commands_network_request_needs_you_until_the_command_ends() {
    let log = include_str!("fixtures/sandbox-network.jsonl");

    // UserPromptSubmit, PreToolUse, Notification `permission_prompt` with no PermissionRequest,
    // PostToolUse, Stop.
    let expected = [Running, Running, NeedsYou, Running, YourTurn].map(Some);
    assert_eq!(states(log, PROBE), expected);
}

#[test]
fn an_mcp_servers_request_for_input_needs_you_until_its_tool_ends() {
    let log = include_str!("fixtures/elicitation.jsonl");

    // UserPromptSubmit, PreToolUse, Elicitation, PostToolUse, Stop.
    let expected = [Running, Running, NeedsYou, Running, YourTurn].map(Some);
    assert_eq!(states(log, PROBE), expected);
}

#[test]
fn a_request_that_lands_before_its_tool_call_still_ends_when_the_tool_ends() {
    // The hooks are async, so an Elicitation or a network request can land before its PreToolUse.
    let logs = [
        ("elicitation", include_str!("fixtures/elicitation.jsonl")),
        (
            "sandbox-network",
            include_str!("fixtures/sandbox-network.jsonl"),
        ),
    ];
    for (name, log) in logs {
        let lines: Vec<&str> = log.lines().collect();
        let log = [lines[0], lines[2], lines[1], lines[3], lines[4]].join("\n");

        // UserPromptSubmit, the request, PreToolUse, PostToolUse, Stop.
        let expected = [Running, NeedsYou, NeedsYou, Running, YourTurn].map(Some);
        assert_eq!(states(&log, PROBE), expected, "{name}");
    }
}

#[test]
fn a_session_closed_at_a_plan_approval_stops_needing_you() {
    let log = include_str!("fixtures/closed-at-plan.jsonl");

    // UserPromptSubmit, PreToolUse, PermissionRequest, SessionEnd: no PostToolUse and no Stop.
    let expected = [Running, NeedsYou, NeedsYou, Ended].map(Some);
    assert_eq!(
        states(log, "5b9e1c7a-2f4d-4a6b-8e0c-3d5f7a9b1c24"),
        expected
    );
}

#[test]
fn a_question_and_a_plan_need_you_until_answered_and_a_turn_ends_on_your_turn_or_a_question() {
    let log = include_str!("fixtures/ask-then-plan.jsonl");

    let expected = [
        // AskUserQuestion: UserPromptSubmit, PreToolUse, PermissionRequest, Notification, PostToolUse.
        Running, NeedsYou, NeedsYou, NeedsYou, Running,
        // Stop: "Should I plan the fix first, or edit it directly?"
        NeedsYou,
        // UserPromptSubmit, then a Bash and a plan file, each PreToolUse and PostToolUse.
        Running, Running, Running, Running, Running,
        // ExitPlanMode: PreToolUse, PermissionRequest, Notification, PostToolUse.
        NeedsYou, NeedsYou, NeedsYou, Running,
        // An Edit, then Stop: "Fixed the install line in `README.md`."
        Running, Running, YourTurn,
    ]
    .map(Some);
    assert_eq!(
        states(log, "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95"),
        expected
    );
}

/// The session's state after a turn whose final message is `message`.
fn after_stop(message: &str) -> State {
    let log = include_str!("fixtures/failing-bash.jsonl");
    let (turn, stop) = log.trim_end().rsplit_once('\n').unwrap();
    let mut stop: serde_json::Value = serde_json::from_str(stop).unwrap();
    stop["last_assistant_message"] = message.into();
    states(&format!("{turn}\n{stop}"), PROBE)
        .pop()
        .flatten()
        .unwrap()
}

// The sentence rule. The messages follow the shapes in #321's evidence ("Transcripts"); the ones
// marked "evidence" are quoted from it.

#[test]
fn a_question_at_the_end_of_a_message_needs_you() {
    let message = "I ran `cargo test` in `apps/studio`: 14 tests pass.

Two ways to go on:
- merge now;
- wait for the heavy run first.

Which do you want?";
    assert_eq!(after_stop(message), NeedsYou);
}

#[test]
fn a_question_sentence_mid_paragraph_needs_you() {
    let message = "The checks pass and the branch is merged.

One question from the review is still open. Should the core drop closed sessions now? If yes, it needs its own ticket.";
    assert_eq!(after_stop(message), NeedsYou);
}

#[test]
fn an_ask_without_a_question_mark_is_your_turn() {
    let your_call = "One design gap blocks the fixtures, so I need your call before writing them.";
    // Evidence, "Hook payloads": a real Stop that asks without a `?`.
    let decide = "I also haven't checked `apps/studio/AGENTS.md` for a studio language rule, which could decide between the two.";
    assert_eq!(after_stop(your_call), YourTurn);
    assert_eq!(after_stop(decide), YourTurn);
}

#[test]
fn a_question_mark_in_fenced_code_is_your_turn() {
    let message = "I can't start that skill for you. Type it as its own message:

```
/ask-matt which ticket should come next? I'm finishing #349.
```

I can also read the open issues and order them myself.";
    assert_eq!(after_stop(message), YourTurn);
}

#[test]
fn a_question_mark_in_inline_code_is_your_turn() {
    let message =
        "Running two checks in parallel: the notification build and the `?` rule measurement.";
    assert_eq!(after_stop(message), YourTurn);
}

#[test]
fn a_question_in_a_table_row_is_your_turn() {
    // Evidence, "Transcripts": table cells such as `| … | Auto mode OK?` were false hits.
    let message = "| Ticket | Session | Auto mode OK?
|---|---|---|
| #341 | auto mode | Yes |
| #342 | outside auto mode | No |

Only #342 runs the real card writer.";
    assert_eq!(after_stop(message), YourTurn);
}

#[test]
fn a_question_in_a_blockquote_or_a_heading_is_your_turn() {
    let quote = "> Where do we read STUDIO_TERMINAL_ID?

Only in `hook/src/main.rs`: the hook copies it from its environment into each event log line.";
    let heading = "## Why are the hooks async?

So Claude Code never waits for them. Two events that fire within a few milliseconds can land in either order.";
    assert_eq!(after_stop(quote), YourTurn);
    assert_eq!(after_stop(heading), YourTurn);
}

#[test]
fn a_question_on_a_line_that_opens_with_an_issue_number_needs_you() {
    let message = "#341 is done and its PR is green. Should I open the next ticket?";
    assert_eq!(after_stop(message), NeedsYou);
}
