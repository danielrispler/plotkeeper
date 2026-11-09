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
    // AskUserQuestion, ExitPlanMode; the Stop that asks in text gives none (#540).
    assert_eq!(live(ask_then_plan), [asker, asker].map(notice));
    assert_eq!(live(include_str!("fixtures/failing-bash.jsonl")), []);
}

#[test]
fn a_stop_clears_every_block_even_when_its_message_asks() {
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
        [Running, Running, NeedsYou, YourTurn].map(Some)
    );
    assert_eq!(live(&asked), [notice(PROBE)]);
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
fn a_question_and_a_plan_need_you_until_answered_and_each_turn_ends_on_your_turn() {
    let log = include_str!("fixtures/ask-then-plan.jsonl");

    let expected = [
        // AskUserQuestion: UserPromptSubmit, PreToolUse, PermissionRequest, Notification, PostToolUse.
        Running, NeedsYou, NeedsYou, NeedsYou, Running,
        // Stop: "Should I plan the fix first, or edit it directly?" A text question blocks nothing.
        YourTurn,
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

// A reply that asks Daniel in text blocks nothing, so it is your turn (#540, reversing #328's story
// 36). The messages follow the shapes in #321's evidence ("Transcripts").

#[test]
fn a_reply_that_ends_with_a_question_is_your_turn() {
    let end = "I ran `cargo test` in `apps/studio`: 14 tests pass.

Two ways to go on:
- merge now;
- wait for the heavy run first.

Which do you want?";
    let mid = "One question from the review is still open. Should the core drop closed sessions now? If yes, it needs its own ticket.";
    let issue = "#341 is done and its PR is green. Should I open the next ticket?";
    assert_eq!(after_stop(end), YourTurn);
    assert_eq!(after_stop(mid), YourTurn);
    assert_eq!(after_stop(issue), YourTurn);
}

// Needs you across agents (#434, decisions 24 to 26): the four cases of #434's first version (B4) and
// the dead call (B3), hand-written in the day log's shapes.

/// Session `s1`'s event `name` at `at`, by subagent `agent` (`None` for its own agent), with `fields`.
fn ev(name: &str, at: u64, agent: Option<&str>, fields: serde_json::Value) -> String {
    let mut event = serde_json::json!({
        "session_id": "s1",
        "hook_event_name": name,
        "cwd": "/w",
        "prompt_id": "p1",
        "studio_received_ms": at,
    });
    if let Some(agent) = agent {
        event["agent_id"] = agent.into();
    }
    let fields = fields.as_object().unwrap().clone();
    event.as_object_mut().unwrap().extend(fields);
    event.to_string()
}

/// A call `id` of `tool` with `command`, by `agent`: its `name` event at `at`.
fn call(name: &str, at: u64, agent: Option<&str>, id: &str, tool: &str, command: &str) -> String {
    let fields = serde_json::json!({"tool_name": tool, "tool_use_id": id,
        "tool_input": {"command": command}});
    ev(name, at, agent, fields)
}

/// A permission notice, which names no agent.
fn notification(at: u64) -> String {
    let fields = serde_json::json!({"notification_type": "permission_prompt"});
    ev("Notification", at, None, fields)
}

/// The permission request of `agent` for `tool` with `command`.
fn request(at: u64, agent: Option<&str>, tool: &str, command: &str) -> String {
    let fields = serde_json::json!({"tool_name": tool, "tool_input": {"command": command}});
    ev("PermissionRequest", at, agent, fields)
}

/// `s1`'s state and who blocks it, as its row names them, after each of `lines`.
fn blocks(studio: &mut Studio, lines: &[String]) -> Vec<(State, Option<Option<String>>)> {
    let blocks = lines.iter().map(|line| {
        studio.apply(line);
        let session = studio.session("s1").unwrap();
        (
            session.state,
            session.asker.as_ref().map(|a| a.agent.clone()),
        )
    });
    blocks.collect()
}

/// `s1` in a turn, with subagent `w1` started.
fn started() -> Studio {
    let mut studio = Studio::default();
    studio.apply(&ev(
        "UserPromptSubmit",
        1,
        None,
        serde_json::json!({"prompt": "Go"}),
    ));
    studio
}

const W1: Option<&str> = Some("w1");

#[test]
fn a_notice_that_could_be_several_agents_names_none_and_clears_when_all_their_calls_resolve() {
    let mut studio = started();
    let lines = [
        call("PreToolUse", 2, None, "own", "Bash", "cargo fetch"),
        call("PreToolUse", 3, W1, "w1-test", "Bash", "cargo test"),
        notification(4),
        call("PostToolUse", 5, W1, "w1-test", "Bash", "cargo test"),
        call("PostToolUse", 6, None, "own", "Bash", "cargo fetch"),
    ];

    assert_eq!(
        blocks(&mut studio, &lines),
        [
            (Running, None),
            (Running, None),
            (NeedsYou, Some(None)),
            (NeedsYou, Some(None)),
            (Running, None),
        ]
    );
}

#[test]
fn a_second_agents_notice_is_kept_while_another_agent_asks() {
    let mut studio = started();
    let lines = [
        call("PreToolUse", 2, None, "own", "Write", "probe.txt"),
        request(3, None, "Write", "probe.txt"),
        // The request's own notice.
        notification(4),
        call("PreToolUse", 5, W1, "w1-test", "Bash", "cargo test"),
        // `w1`'s prompt, which fires only a notice.
        notification(6),
        call("PostToolUse", 7, None, "own", "Write", "probe.txt"),
        call("PostToolUse", 8, W1, "w1-test", "Bash", "cargo test"),
    ];

    let states: Vec<State> = blocks(&mut studio, &lines)
        .into_iter()
        .map(|w| w.0)
        .collect();
    assert_eq!(
        states,
        [
            Running, NeedsYou, NeedsYou, NeedsYou, NeedsYou, NeedsYou, Running
        ]
    );
}

/// #434, decision 26: the session's open spawn of a foreground subagent waits on no permission.
#[test]
fn a_notice_does_not_wait_on_the_open_spawn_of_the_subagent_that_asks() {
    let mut studio = started();
    let w1 = Some(Some("w1".to_string()));
    let lines = [
        call("PreToolUse", 2, None, "spawn", "Agent", ""),
        call("PreToolUse", 3, W1, "w1-test", "Bash", "cargo test"),
        notification(4),
        call("PostToolUse", 5, W1, "w1-test", "Bash", "cargo test"),
    ];

    assert_eq!(
        blocks(&mut studio, &lines),
        [
            (Running, None),
            (Running, None),
            (NeedsYou, w1),
            (Running, None),
        ]
    );
}

/// #434, story 29: a notice that names its agent echoes only that agent's request.
#[test]
fn a_notice_naming_one_agent_is_not_taken_as_another_agents_echo() {
    let mut studio = started();
    let (w1, w2) = (Some(Some("w1".to_string())), Some(Some("w2".to_string())));
    let named = serde_json::json!({"notification_type": "permission_prompt"});
    let lines = [
        call("PreToolUse", 2, W1, "w1-test", "Bash", "cargo test"),
        call("PreToolUse", 3, Some("w2"), "w2-test", "Bash", "cargo test"),
        request(4, Some("w2"), "Bash", "cargo test"),
        ev("Notification", 5, W1, named),
        call(
            "PostToolUse",
            6,
            Some("w2"),
            "w2-test",
            "Bash",
            "cargo test",
        ),
        call("PostToolUse", 7, W1, "w1-test", "Bash", "cargo test"),
    ];

    assert_eq!(
        blocks(&mut studio, &lines),
        [
            (Running, None),
            (Running, None),
            (NeedsYou, w2.clone()),
            (NeedsYou, w2),
            (NeedsYou, w1),
            (Running, None),
        ]
    );
}

#[test]
fn a_permission_request_leaves_another_agents_notice_in_place() {
    let mut studio = started();
    let w1 = Some(Some("w1".to_string()));
    let lines = [
        call("PreToolUse", 2, W1, "w1-test", "Bash", "cargo test"),
        notification(3),
        call("PreToolUse", 4, None, "own", "Write", "probe.txt"),
        request(5, None, "Write", "probe.txt"),
        call("PostToolUse", 6, None, "own", "Write", "probe.txt"),
        call("PostToolUse", 7, W1, "w1-test", "Bash", "cargo test"),
    ];

    assert_eq!(
        blocks(&mut studio, &lines),
        [
            (Running, None),
            (NeedsYou, w1.clone()),
            (NeedsYou, w1.clone()),
            (NeedsYou, w1.clone()),
            (NeedsYou, w1),
            (Running, None),
        ]
    );
}

#[test]
fn a_permission_request_binds_only_to_its_own_agents_call() {
    let mut studio = started();
    let w1 = Some(Some("w1".to_string()));
    let lines = [
        call("PreToolUse", 2, W1, "w1-test", "Bash", "cargo test"),
        call("PreToolUse", 3, Some("w2"), "w2-test", "Bash", "cargo test"),
        request(4, W1, "Bash", "cargo test"),
        call(
            "PostToolUse",
            5,
            Some("w2"),
            "w2-test",
            "Bash",
            "cargo test",
        ),
        call("PostToolUse", 6, W1, "w1-test", "Bash", "cargo test"),
    ];

    assert_eq!(
        blocks(&mut studio, &lines),
        [
            (Running, None),
            (Running, None),
            (NeedsYou, w1.clone()),
            (NeedsYou, w1),
            (Running, None),
        ]
    );
}

#[test]
fn a_call_that_never_reported_back_holds_no_wait_past_its_sessions_turn() {
    let mut studio = started();
    let dead = Some(Some("d1".to_string()));
    let stop = |at| {
        ev(
            "Stop",
            at,
            None,
            serde_json::json!({"last_assistant_message": "Done."}),
        )
    };
    let lines = [
        // Subagent `d1`'s command, which never reports back, as `a4376`'s in the day log.
        call("PreToolUse", 2, Some("d1"), "d1-ls", "Bash", "ls"),
        // A sandboxed command's network notice, which lands before its own `PreToolUse`.
        notification(3),
        call("PreToolUse", 4, None, "own", "Bash", "cargo fetch"),
        call("PostToolUse", 5, None, "own", "Bash", "cargo fetch"),
        stop(6),
    ];

    assert_eq!(
        blocks(&mut studio, &lines),
        [
            (Running, None),
            (NeedsYou, dead.clone()),
            (NeedsYou, dead.clone()),
            (NeedsYou, dead),
            (YourTurn, None),
        ]
    );
}

/// The answer hook's `StudioAnswer` line for `line`'s call: Daniel's `answer` from the studio card
/// (#478).
fn card_answer(line: &str, answer: serde_json::Value) -> String {
    let mut event: serde_json::Value = serde_json::from_str(line).unwrap();
    event["studio_asked_by"] = event["hook_event_name"].clone();
    event["hook_event_name"] = "StudioAnswer".into();
    event["studio_answer"] = answer;
    event["studio_received_ms"] = (event["studio_received_ms"].as_u64().unwrap() + 1).into();
    event.to_string()
}

fn decision(decision: &str) -> serde_json::Value {
    serde_json::json!({ "decision": decision })
}

#[test]
fn a_permission_decided_on_the_card_stops_needing_you_at_once() {
    let lines: Vec<&str> = include_str!("fixtures/permission-write.jsonl")
        .lines()
        .collect();
    let allowed = card_answer(lines[2], decision("allow"));
    let log = [lines[0], lines[1], lines[2], lines[3], &allowed, lines[4]].join("\n");

    // UserPromptSubmit, PreToolUse, PermissionRequest, Notification, the card's answer, PostToolUse.
    let expected = [Running, Running, NeedsYou, NeedsYou, Running, Running].map(Some);
    assert_eq!(states(&log, PROBE), expected);

    let lines: Vec<&str> = include_str!("fixtures/permission-denied.jsonl")
        .lines()
        .collect();
    let denied = card_answer(lines[2], decision("deny"));
    let log = [lines[0], lines[1], lines[2], &denied].join("\n");
    let expected = [Running, Running, NeedsYou, Running].map(Some);
    assert_eq!(states(&log, PROBE), expected);
}

#[test]
fn a_plan_rejected_on_the_card_stops_needing_you_with_no_result() {
    let asker = "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95";
    let lines: Vec<&str> = include_str!("fixtures/ask-then-plan.jsonl")
        .lines()
        .collect();
    // Up to ExitPlanMode's PreToolUse, then the card's rejection: a denied call may get no result.
    let rejected = card_answer(lines[11], decision("deny"));
    let mut log = lines[..12].to_vec();
    log.push(&rejected);
    let states = states(&log.join("\n"), asker);
    assert_eq!(states[11..], [Some(NeedsYou), Some(Running)]);
}

#[test]
fn a_question_answered_on_the_card_waits_for_its_result() {
    let asker = "2d8a6f41-c95e-4b37-a1d0-7e3f8b2c6a95";
    let lines: Vec<&str> = include_str!("fixtures/ask-then-plan.jsonl")
        .lines()
        .collect();
    let answer = card_answer(
        lines[1],
        serde_json::json!({ "answers": { "Which?": "A" } }),
    );
    let log = [lines[0], lines[1], &answer].join("\n");
    // Its PostToolUse carries the answer, so it stays open until then.
    assert_eq!(states(&log, asker)[2], Some(NeedsYou));
}
