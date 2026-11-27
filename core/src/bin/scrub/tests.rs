use serde_json::{Value, json};

fn scrub(event: Value) -> Value {
    let Value::Object(event) = event else {
        panic!("an event is an object")
    };
    Value::Object(super::scrub(event))
}

#[test]
fn a_wake_up_keeps_its_markup_and_ids_and_loses_its_text() {
    let prompt = "<task-notification>\n<task-id>a1191df87fb730aa0</task-id>\n\
        <tool-use-id>toolu_01Bg1Ag2Ct3Dv4Ew5Fx6Gy7H</tool-use-id>\n\
        <output-file>/private/tmp/x/tasks/a1191df87fb730aa0.output</output-file>\n\
        <status>killed</status>\n<summary>Agent \"Fix the parser\" was stopped</summary>\n\
        <result>The parser now rejects empty input.</result>\n</task-notification>";
    let message = "<agent-message from=\"a2526f0d7353298ee\">Done, see Vec<String> in \
        <a href=\"https://example.com/a b\">the diff</a></agent-message>";

    let scrubbed = |prompt: &str| {
        let event = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt});
        scrub(event)["prompt"].as_str().unwrap().to_string()
    };

    assert_eq!(
        scrubbed(prompt),
        "<task-notification><task-id>a1191df87fb730aa0</task-id>\
        <tool-use-id>toolu_01Bg1Ag2Ct3Dv4Ew5Fx6Gy7H</tool-use-id><output-file>…</output-file>\
        <status>killed</status><summary>…</summary><result>…</result></task-notification>"
    );
    assert_eq!(
        scrubbed(message),
        "<agent-message from=\"a2526f0d7353298ee\">…<…>…<…>…</…></agent-message>"
    );
    assert_eq!(scrubbed("continue"), "…");
    assert_eq!(
        scrubbed(
            "Use <sk-ant-api03-AbCdEf> and <input value=\"hunter2\"> then a<b pass:word=\"Sekret99\" c>d"
        ),
        "…<…>…<…>…<…>…"
    );
    assert_eq!(
        scrubbed("Is 3 < 4? Read <pasted_content id=\"fa32\">hunter2</pasted_content id=\"fa32\">"),
        "…<pasted_content id=\"fa32\">…</pasted_content id=\"fa32\">"
    );
}

#[test]
fn a_spawn_keeps_its_ids_description_and_isolation_and_loses_its_prompt() {
    let spawn = json!({
        "session_id": "5d7f69ab", "hook_event_name": "PostToolUse", "cwd": "/w/Falinks",
        "transcript_path": "/h/.claude/projects/x.jsonl", "effort": {"level": "high"},
        "tool_name": "Agent", "tool_use_id": "toolu_01", "duration_ms": 12,
        "tool_input": {"description": "Implement #355", "isolation": "worktree", "prompt": "Read #355."},
        "tool_response": {"agentId": "a1191df87fb730aa0", "status": "async_launched", "isAsync": true,
            "prompt": "Read #355.", "outputFile": "/private/tmp/x.output"},
        "studio_received_ms": 1793264003290u64,
    });

    assert_eq!(
        scrub(spawn),
        json!({
            "session_id": "5d7f69ab", "hook_event_name": "PostToolUse", "cwd": "/w/Falinks",
            "transcript_path": "…", "effort": {},
            "tool_name": "Agent", "tool_use_id": "toolu_01", "duration_ms": 12,
            "tool_input": {"description": "Implement #355", "isolation": "worktree", "prompt": "…"},
            "tool_response": {"agentId": "a1191df87fb730aa0", "status": "async_launched", "isAsync": true,
                "prompt": "…", "outputFile": "…"},
            "studio_received_ms": 1793264003290u64,
        })
    );
}

#[test]
fn steps_lose_their_inputs_results_and_file_contents() {
    let edit = json!({
        "hook_event_name": "PostToolUse", "agent_id": "a1191df87fb730aa0", "agent_type": "general-purpose",
        "tool_name": "Edit",
        "tool_input": {"file_path": "/w/src/lib.rs", "old_string": "a", "new_string": "b", "replace_all": false},
        "tool_response": {"filePath": "/w/src/lib.rs", "originalFile": "a\n",
            "structuredPatch": [{"lines": ["-a", "+b"]}]},
    });
    let failed = json!({"hook_event_name": "PostToolUseFailure", "tool_name": "Bash",
        "tool_input": {"command": "cargo test", "description": "Run the tests\nsecond line"},
        "error": "Exit code 101\ntest result: FAILED"});
    let refused =
        json!({"hook_event_name": "PostToolUseFailure", "error": "This agent is isolated in /w"});
    let masked = json!({"hook_event_name": "PostToolUseFailure", "error": "Exit code 1 TOKEN=abc"});
    let event = json!({"hook_event_name": "PreToolUse", "tool_name": "mcp__cal__create_event",
        "tool_input": {"name": "Lawyer", "description": "Meet Dr Cohen", "status": "confirmed"}});
    let task = json!({"hook_event_name": "PreToolUse", "tool_name": "TaskCreate",
        "tool_input": {"subject": "Call the bank", "description": "Ask about the loan"}});

    assert_eq!(
        scrub(edit),
        json!({
            "hook_event_name": "PostToolUse", "agent_id": "a1191df87fb730aa0", "agent_type": "general-purpose",
            "tool_name": "Edit",
            "tool_input": {"file_path": "/w/src/lib.rs", "old_string": "…", "new_string": "…", "replace_all": false},
            "tool_response": {"filePath": "/w/src/lib.rs", "originalFile": "…", "structuredPatch": []},
        })
    );
    assert_eq!(
        scrub(failed),
        json!({"hook_event_name": "PostToolUseFailure", "tool_name": "Bash",
            "tool_input": {"command": "…", "description": "…"}, "error": "Exit code 101"})
    );
    assert_eq!(scrub(refused)["error"], "…");
    assert_eq!(scrub(masked)["error"], "…");
    let input = |event| scrub(event)["tool_input"].clone();
    assert_eq!(
        input(event),
        json!({"name": "…", "description": "…", "status": "…"})
    );
    assert_eq!(input(task), json!({"subject": "…", "description": "…"}));
}

#[test]
fn a_recorded_blocked_run_keeps_its_marked_command_and_the_cards_decision() {
    let denied = |command: &str| {
        json!({"hook_event_name": "PermissionDenied", "tool_name": "Bash", "reason": "Auto mode blocked it",
            "tool_input": {"command": command}})
    };
    let answered = json!({"hook_event_name": "StudioAnswer", "studio_asked_by": "PermissionDenied",
        "tool_name": "Bash", "studio_answer": {"decision": "allow", "note": "x"}});
    let wake = json!({"hook_event_name": "UserPromptSubmit", "prompt":
        "<task-notification><status>completed</status><output-file>/private/tmp/t/a.output</output-file>\
        <summary>Background command \"claude -p # studio-fixture-background\" completed (exit code 0)</summary>\
        <result>Daniel's text</result></task-notification>"});

    assert_eq!(
        scrub(denied("claude -p ok # studio-fixture-foreground"))["tool_input"]["command"],
        "claude -p ok # studio-fixture-foreground"
    );
    assert_eq!(
        scrub(denied("rm -rf ~/secret"))["tool_input"]["command"],
        "…"
    );
    let kept = scrub(answered);
    assert_eq!(kept["studio_asked_by"], "PermissionDenied");
    assert_eq!(kept["studio_answer"], json!({"decision": "allow"}));
    assert_eq!(
        scrub(wake)["prompt"],
        "<task-notification><status>completed</status><output-file>…</output-file>\
        <summary>Background command \"…\" completed (exit code 0)</summary>\
        <result>…</result></task-notification>"
    );
    let summary = |text: &str| {
        let prompt = format!("<task-notification><summary>{text}</summary></task-notification>");
        let event = json!({"hook_event_name": "UserPromptSubmit", "prompt": prompt});
        scrub(event)["prompt"].as_str().unwrap().to_string()
    };
    assert_eq!(
        summary("Background command \"grep x\" failed with exit code 144"),
        "<task-notification><summary>Background command \"…\" failed with exit code 144</summary></task-notification>"
    );
    assert_eq!(
        summary(
            "Background command \"grep x\" completed (exit code 1: No matches found in ~/secret)"
        ),
        "<task-notification><summary>Background command \"…\" completed (exit code 1: …)</summary></task-notification>"
    );
    assert_eq!(
        summary("Background command \"grep x\" failed with Daniel's text"),
        "<task-notification><summary>…</summary></task-notification>"
    );
}

#[test]
fn a_stop_keeps_its_background_tasks_without_their_commands() {
    let stop = |message: &str| {
        json!({"hook_event_name": "Stop", "last_assistant_message": message,
            "background_tasks": [
                {"id": "bbr1524wy", "type": "shell", "status": "running", "description": "Run the gate",
                    "command": "TOKEN=x ./gate"},
                {"id": "a32aaf17ca17f12b2", "type": "subagent", "status": "running",
                    "description": "Standards review", "agent_type": "general-purpose"}],
            "session_crons": [{"id": "9d12fe2c", "prompt": "Continue #364"}]})
    };

    assert_eq!(
        scrub(stop("Merged. Shall I close #370?")),
        json!({"hook_event_name": "Stop", "last_assistant_message": "…",
            "background_tasks": [
                {"id": "bbr1524wy", "type": "shell", "status": "running", "description": "Run the gate",
                    "command": "…"},
                {"id": "a32aaf17ca17f12b2", "type": "subagent", "status": "running",
                    "description": "Standards review", "agent_type": "general-purpose"}],
            "session_crons": []})
    );
}
