//! The one writer contract (#327): each writer's command is built and canned replies and failures are read into the
//! contract. No model runs.

use std::{
    ffi::OsStr,
    os::unix::process::ExitStatusExt,
    path::Path,
    process::{Command, ExitStatus},
    time::Duration,
};
use studio_understanding::{
    CardWriter, ClaudeWriter, FailureKind, SummarySetup, WriterKind, WriterRequest, build_command,
    read_reply,
};

fn exit(code: i32) -> ExitStatus {
    ExitStatus::from_raw(code << 8)
}

fn request() -> WriterRequest {
    WriterRequest {
        system: Some("You write cards.".into()),
        user: "The change.\0".into(),
        schema: Some(serde_json::json!({ "type": "object" })),
        model: Some("m1".into()),
        effort: Some("low".into()),
        timeout: Duration::from_secs(150),
    }
}

fn args(command: &Command) -> Vec<&str> {
    command.get_args().map(|a| a.to_str().unwrap()).collect()
}

fn env(command: &Command) -> Vec<(&OsStr, Option<&OsStr>)> {
    let mut env: Vec<_> = command.get_envs().collect();
    env.sort();
    env
}

// @law: writer-has-no-tools
#[test]
fn the_claude_command_has_no_tools_a_schema_and_the_internal_mark() {
    let (command, stdin) = build_command(
        WriterKind::Claude,
        Path::new("/bin/claude"),
        Path::new("/w/card-writer"),
        &request(),
    );
    assert_eq!(command.get_program(), "/bin/claude");
    assert_eq!(
        args(&command),
        [
            "--tools",
            "",
            "--strict-mcp-config",
            "--safe-mode",
            "--model",
            "m1",
            "--effort",
            "low",
            "--system-prompt",
            "You write cards.",
            "--output-format",
            "json",
            "--json-schema",
            r#"{"type":"object"}"#,
            "-p"
        ]
    );
    // The prompt goes on standard input, without its NUL bytes.
    assert_eq!(stdin, "The change.");
    assert!(env(&command).contains(&(OsStr::new("PLOTKEEPER_INTERNAL"), Some(OsStr::new("1")))));
    assert_eq!(command.get_current_dir(), Some(Path::new("/w/card-writer")));
}

#[test]
fn agy_writes_with_its_own_model_when_the_setting_names_another_writers() {
    let asked = WriterRequest {
        model: Some("haiku".into()),
        ..request()
    };
    let (command, _) = build_command(
        WriterKind::Agy,
        Path::new("/bin/agy"),
        Path::new("/w/card-writer-agy"),
        &asked,
    );
    let args = args(&command);
    let at = args.iter().position(|a| *a == "--model").unwrap();
    assert_eq!(args[at + 1], "gemini-4-neon-low");
    assert!(!args.contains(&"--effort"));
    // A model it knows is kept, and so is a `claude` model of any id.
    assert_eq!(
        WriterKind::Agy.model("gemini-4-neon-high"),
        "gemini-4-neon-high"
    );
    assert_eq!(WriterKind::Claude.model("claude-x-9"), "claude-x-9");
}

// agy has no flag that turns its tools off, nor a read-only mode that answers plainly (`--mode plan` writes a plan
// instead): its lockout is the sandboxed terminal and no slash commands or skills, in an empty folder.
// @law: writer-has-no-tools
#[test]
fn the_agy_command_takes_the_schema_runs_in_its_own_folder_and_is_marked_internal() {
    let (command, stdin) = build_command(
        WriterKind::Agy,
        Path::new("/bin/agy"),
        Path::new("/w/card-writer-agy"),
        &WriterRequest {
            model: Some("gemini-4-neon-medium".into()),
            ..request()
        },
    );
    assert_eq!(command.get_program(), "/bin/agy");
    assert_eq!(
        args(&command),
        [
            "--output-format",
            "json",
            "--json-schema",
            r#"{"type":"object"}"#,
            "--model",
            "gemini-4-neon-medium",
            "--effort",
            "low",
            "--sandbox",
            "--disable-slash-commands",
            "--print-timeout",
            "3m",
            "-p",
            // No system prompt flag: the system prompt leads the prompt, which is an argument.
            "You write cards.\n\nThe change."
        ]
    );
    assert_eq!(stdin, "");
    assert_eq!(
        env(&command),
        [(OsStr::new("PLOTKEEPER_INTERNAL"), Some(OsStr::new("1")))]
    );
    assert_eq!(
        command.get_current_dir(),
        Some(Path::new("/w/card-writer-agy"))
    );
}

// Codex cannot turn its tools off: its lockout is the read-only sandbox, without the user's config (and so without
// their MCP servers), in an empty folder.
// @law: writer-has-no-tools
#[test]
fn the_codex_command_is_read_only_takes_the_schema_from_a_file_and_is_marked_internal() {
    let folder = tempfile::tempdir().unwrap();
    let (command, stdin) = build_command(
        WriterKind::Codex,
        Path::new("/bin/codex"),
        folder.path(),
        &WriterRequest {
            model: Some("gpt-5.6-luna".into()),
            ..request()
        },
    );
    assert_eq!(command.get_program(), "/bin/codex");
    let all = args(&command);
    let (schema, head) = (all[all.len() - 2], &all[..all.len() - 2]);
    assert_eq!(
        head,
        [
            "exec",
            "--skip-git-repo-check",
            "-s",
            "read-only",
            "--ephemeral",
            "--ignore-user-config",
            "--json",
            "--model",
            "gpt-5.6-luna",
            "-c",
            r#"model_reasoning_effort="low""#,
            "--output-schema",
        ]
    );
    // The schema is a file in the neutral folder, and the prompt comes on standard input (`-`).
    assert!(Path::new(schema).starts_with(folder.path()), "{schema}");
    assert_eq!(
        std::fs::read_to_string(schema).unwrap(),
        r#"{"type":"object"}"#
    );
    assert_eq!(command.get_args().last().unwrap(), "-");
    // No system prompt flag: the system prompt leads the prompt.
    assert_eq!(stdin, "You write cards.\n\nThe change.");
    assert_eq!(
        env(&command),
        [(OsStr::new("PLOTKEEPER_INTERNAL"), Some(OsStr::new("1")))]
    );
    assert_eq!(command.get_current_dir(), Some(folder.path()));

    // Without a schema there is no schema file; a model it does not list (the default `haiku`) is its first.
    let (command, _) = build_command(
        WriterKind::Codex,
        Path::new("/bin/codex"),
        Path::new("/w/card-writer-codex"),
        &WriterRequest {
            schema: None,
            model: Some("haiku".into()),
            effort: Some(String::new()),
            ..request()
        },
    );
    let args = args(&command);
    assert!(
        !args.contains(&"--output-schema") && !args.contains(&"-c"),
        "{args:?}"
    );
    let at = args.iter().position(|a| *a == "--model").unwrap();
    assert_eq!(args[at + 1], WriterKind::Codex.models()[0]);
}

// Copilot's tools are turned off by an empty `--available-tools` allow-list (its docs; that an empty list turns all of
// them off is not verified), it never stops to ask, and it runs in an empty folder.
// @law: writer-has-no-tools
#[test]
fn the_copilot_command_allows_no_tools_takes_the_prompt_as_an_argument_and_is_marked_internal() {
    let (command, stdin) = build_command(
        WriterKind::Copilot,
        Path::new("/bin/copilot"),
        Path::new("/w/card-writer-copilot"),
        &WriterRequest {
            model: Some("gpt-6-astra".into()),
            ..request()
        },
    );
    assert_eq!(command.get_program(), "/bin/copilot");
    assert_eq!(
        args(&command),
        [
            "-s",
            "--output-format=json",
            "--available-tools=",
            "--no-ask-user",
            "--model=gpt-6-astra",
            "-p",
            // No system prompt flag, and no schema flag: the system prompt leads the prompt, an argument.
            "You write cards.\n\nThe change."
        ]
    );
    assert_eq!(stdin, "");
    assert_eq!(
        env(&command),
        [(OsStr::new("PLOTKEEPER_INTERNAL"), Some(OsStr::new("1")))]
    );
    assert_eq!(
        command.get_current_dir(),
        Some(Path::new("/w/card-writer-copilot"))
    );
    // A model it does not list (the default `haiku`) is its first, and it takes no effort.
    let (command, _) = build_command(
        WriterKind::Copilot,
        Path::new("/bin/copilot"),
        Path::new("/w/card-writer-copilot"),
        &WriterRequest {
            model: Some("haiku".into()),
            ..request()
        },
    );
    let model = format!("--model={}", WriterKind::Copilot.models()[0]);
    assert!(
        args(&command).contains(&model.as_str()),
        "{:?}",
        args(&command)
    );
    assert!(WriterKind::Copilot.efforts().is_empty());
}

// Cursor's `agent` cannot turn its tools off: its lockout is ask mode, read-only, in an empty folder it trusts so that
// no prompt waits.
// @law: writer-has-no-tools
#[test]
fn the_cursor_command_is_in_ask_mode_takes_the_prompt_as_an_argument_and_is_marked_internal() {
    let (command, stdin) = build_command(
        WriterKind::Cursor,
        Path::new("/bin/agent"),
        Path::new("/w/card-writer-cursor"),
        &WriterRequest {
            model: Some("gpt-5.6-luna".into()),
            ..request()
        },
    );
    assert_eq!(command.get_program(), "/bin/agent");
    assert_eq!(
        args(&command),
        [
            "-p",
            "--output-format",
            "json",
            "--mode",
            "ask",
            "--trust",
            "--model",
            "gpt-5.6-luna",
            "You write cards.\n\nThe change."
        ]
    );
    assert_eq!(stdin, "");
    assert_eq!(
        env(&command),
        [(OsStr::new("PLOTKEEPER_INTERNAL"), Some(OsStr::new("1")))]
    );
    assert_eq!(
        command.get_current_dir(),
        Some(Path::new("/w/card-writer-cursor"))
    );
    let (command, _) = build_command(
        WriterKind::Cursor,
        Path::new("/bin/agent"),
        Path::new("/w/card-writer-cursor"),
        &WriterRequest {
            model: Some("haiku".into()),
            ..request()
        },
    );
    let args = args(&command);
    let at = args.iter().position(|a| *a == "--model").unwrap();
    assert_eq!(args[at + 1], WriterKind::Cursor.models()[0]);
    assert!(WriterKind::Cursor.efforts().is_empty());
}

#[test]
fn agys_own_timeout_covers_a_logged_out_agys_wait() {
    let writer =
        ClaudeWriter::new("/bin/claude".into(), "/w/card-writer".into()).with_kind(WriterKind::Agy);
    assert!(writer.timeout() > studio_understanding::AGY_LOGIN_WAIT);
    assert!(WriterKind::Agy.floor() > studio_understanding::AGY_LOGIN_WAIT);
    // Claude keeps its own.
    assert_eq!(WriterKind::Claude.timeout(), Duration::from_secs(300));
}

#[test]
fn the_summary_setting_chooses_the_writer_and_claude_is_the_default() {
    let setup: SummarySetup = serde_json::from_str(r#"{"model":"m","effort":""}"#).unwrap();
    assert_eq!(setup.writer, WriterKind::Claude);
    let setup: SummarySetup =
        serde_json::from_str(r#"{"model":"m","effort":"","writer":"agy"}"#).unwrap();
    assert_eq!(setup.writer, WriterKind::Agy);
    assert_eq!(SummarySetup::default().writer, WriterKind::Claude);
}

#[test]
fn a_writer_set_to_agy_builds_agy_commands_for_every_call_and_back() {
    let dir = Path::new("/w/card-writer");
    let writer = ClaudeWriter::new("/bin/claude".into(), dir.into());
    writer.use_writer(WriterKind::Agy);
    let (command, _) = writer.goal_command(&["a prompt"]);
    assert_eq!(command.get_program(), "agy");
    assert_eq!(
        command.get_current_dir(),
        Some(Path::new("/w/card-writer-agy"))
    );
    assert!(args(&command).contains(&"-p"));
    writer.use_writer(WriterKind::Claude);
    let (command, _) = writer.goal_command(&["a prompt"]);
    assert_eq!(command.get_program(), "/bin/claude");
}

fn read(kind: WriterKind, schema: bool, code: i32, out: &str) -> Result<String, FailureKind> {
    read_reply(kind, schema, &exit(code), out)
        .map(|r| r.text)
        .map_err(|e| e.kind)
}

#[test]
fn a_claude_reply_is_its_text_and_with_a_schema_its_structured_output_and_usage() {
    assert_eq!(
        read(WriterKind::Claude, false, 0, "hello\n"),
        Ok("hello\n".into())
    );

    let envelope = r#"{"type":"result","is_error":false,"result":"pong","structured_output":{"answer":"pong"},"usage":{"input_tokens":12,"output_tokens":3}}"#;
    let reply = read_reply(WriterKind::Claude, true, &exit(0), envelope).unwrap();
    assert_eq!(reply.json, Some(serde_json::json!({ "answer": "pong" })));
    assert_eq!(reply.text, r#"{"answer":"pong"}"#);
    let usage = reply.usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (12, 3));
}

#[test]
fn claudes_failures_are_told_apart() {
    let kind = |out: &str, code| read(WriterKind::Claude, false, code, out).unwrap_err();
    assert_eq!(
        kind("Not logged in · Please run /login", 1),
        FailureKind::NotLoggedIn
    );
    assert_eq!(kind("Claude usage limit reached", 1), FailureKind::Quota);
    assert_eq!(kind("API Error: 429 rate limit", 1), FailureKind::Quota);
    assert_eq!(kind("", 0), FailureKind::Empty);
    assert_eq!(kind("   \n", 0), FailureKind::Empty);
    assert_eq!(kind("boom", 3), FailureKind::Other);

    // With a schema the failure is in the envelope, whose subtype stays "success".
    let envelope = r#"{"subtype":"success","is_error":true,"result":"Not logged in · Please run /login","terminal_reason":"api_error"}"#;
    assert_eq!(
        read(WriterKind::Claude, true, 1, envelope),
        Err(FailureKind::NotLoggedIn)
    );
}

#[test]
fn an_agy_reply_is_its_structured_output_with_usage() {
    let out = r#"{"conversation_id":"c","status":"SUCCESS","response":"pong\n{\"answer\":\"pong\"}\n","structured_output":{"answer":"pong"},"usage":{"input_tokens":20,"output_tokens":5,"total_tokens":25}}"#;
    let reply = read_reply(WriterKind::Agy, true, &exit(0), out).unwrap();
    assert_eq!(reply.json, Some(serde_json::json!({ "answer": "pong" })));
    assert_eq!(reply.text, r#"{"answer":"pong"}"#);
    assert_eq!(reply.usage.map(|u| u.output_tokens), Some(5));

    // With no schema its `response` is the text.
    let out = r#"{"status":"SUCCESS","response":"plain words"}"#;
    assert_eq!(
        read(WriterKind::Agy, false, 0, out),
        Ok("plain words".into())
    );
}

#[test]
fn agys_failures_are_told_apart() {
    let kind = |out: &str, code| read(WriterKind::Agy, true, code, out).unwrap_err();
    // Logged out: an OAuth URL, then JSON with an error, which the exit code may not show.
    let logged_out = "Open https://accounts.example/oauth to sign in\n{\"status\":\"ERROR\",\"error\":\"authentication failed or timed out\",\"finished_reason\":\"error\"}";
    assert_eq!(kind(logged_out, 0), FailureKind::NotLoggedIn);
    assert_eq!(kind(logged_out, 1), FailureKind::NotLoggedIn);
    assert_eq!(
        kind(
            r#"{"status":"ERROR","error":"RESOURCE_EXHAUSTED: quota"}"#,
            1
        ),
        FailureKind::Quota
    );
    assert_eq!(kind("429 Too Many Requests", 1), FailureKind::Quota);
    // `--print-timeout` hit.
    assert_eq!(kind("", 3), FailureKind::Timeout);
    assert_eq!(kind("", 0), FailureKind::Empty);
    assert_eq!(
        kind(r#"{"status":"SUCCESS","response":""}"#, 0),
        FailureKind::Empty
    );
    assert_eq!(kind("segfault", 139), FailureKind::Other);
}

// What `codex exec --json` printed, as a real run printed it (codex-cli 0.171.0).
const CODEX_OK: &str = r#"{"type":"thread.started","thread_id":"t"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{\"answer\":\"pong\"}"}}
{"type":"turn.completed","usage":{"input_tokens":14574,"cached_input_tokens":0,"output_tokens":15,"reasoning_output_tokens":0}}
"#;

#[test]
fn a_codex_reply_is_its_last_agent_message_with_usage() {
    let reply = read_reply(WriterKind::Codex, true, &exit(0), CODEX_OK).unwrap();
    assert_eq!(reply.text, r#"{"answer":"pong"}"#);
    assert_eq!(reply.json, Some(serde_json::json!({ "answer": "pong" })));
    let usage = reply.usage.unwrap();
    assert_eq!((usage.input_tokens, usage.output_tokens), (14574, 15));

    // With no schema the message is the text, and a warning item before it is no reply.
    let out = r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Model metadata not found."}}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"first"}}
{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"plain words"}}
{"type":"turn.completed","usage":{"input_tokens":3,"output_tokens":2}}"#;
    let reply = read_reply(WriterKind::Codex, false, &exit(0), out).unwrap();
    assert_eq!((reply.text.as_str(), reply.json), ("plain words", None));
}

#[test]
fn codexs_failures_are_told_apart() {
    let kind = |out: &str, code| read(WriterKind::Codex, true, code, out).unwrap_err();
    // Logged out: it retries, then the turn fails with a 401.
    let logged_out = r#"{"type":"thread.started","thread_id":"t"}
{"type":"error","message":"Reconnecting... 5/5 (unexpected status 401 Unauthorized: Missing bearer or basic authentication in header)"}
{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized: Missing bearer or basic authentication in header"}}"#;
    assert_eq!(kind(logged_out, 1), FailureKind::NotLoggedIn);
    let limit = r#"{"type":"turn.failed","error":{"message":"You've hit your usage limit. Try again later."}}"#;
    assert_eq!(kind(limit, 1), FailureKind::Quota);
    // A failed turn is a failure whatever the exit code says.
    assert_eq!(kind(limit, 0), FailureKind::Quota);
    let unsupported = r#"{"type":"error","message":"The 'x' model is not supported when using Codex with a ChatGPT account."}
{"type":"turn.failed","error":{"message":"The 'x' model is not supported when using Codex with a ChatGPT account."}}"#;
    assert_eq!(kind(unsupported, 1), FailureKind::Other);
    // A bad `CODEX_HOME` prints its error on standard error only.
    assert_eq!(kind("", 1), FailureKind::Other);
    assert_eq!(kind("", 0), FailureKind::Empty);
    let silent = r#"{"type":"turn.started"}
{"type":"turn.completed","usage":{"input_tokens":3,"output_tokens":0}}"#;
    assert_eq!(kind(silent, 0), FailureKind::Empty);
    // What it said reaches the log line, not the page.
    let error = read_reply(WriterKind::Codex, true, &exit(1), limit).unwrap_err();
    assert!(error.to_string().contains("usage limit"), "{error}");
    assert!(
        !error.reason().contains("usage limit"),
        "{}",
        error.reason()
    );
}

// What `copilot -p -s --output-format=json` prints is JSONL whose records its docs do not describe: these are guesses,
// for wave 1's trial to confirm or fix. The reply is the last `assistant.message`'s content; output with no JSON line at
// all (`-s` printing the reply alone) is the reply.
const COPILOT_OK: &str = r#"{"type":"session.start","data":{"model":"claude-haiku-4.5"}}
{"type":"assistant.message","data":{"content":"{\"answer\":\"pong\"}"}}
{"type":"session.idle","data":{}}
"#;

#[test]
fn a_copilot_reply_is_its_last_assistant_message_and_the_object_is_read_from_its_text() {
    let reply = read_reply(WriterKind::Copilot, true, &exit(0), COPILOT_OK).unwrap();
    assert_eq!(reply.text, r#"{"answer":"pong"}"#);
    // No schema flag: the object is taken from the text, and the caller checks it.
    assert_eq!(reply.json, Some(serde_json::json!({ "answer": "pong" })));
    assert_eq!(reply.usage, None);

    let out = r#"{"type":"assistant.message","data":{"content":"first"}}
{"type":"assistant.message","data":{"content":"Here it is: {\"answer\":\"pong\"}"}}"#;
    let reply = read_reply(WriterKind::Copilot, true, &exit(0), out).unwrap();
    assert_eq!(reply.json, Some(serde_json::json!({ "answer": "pong" })));
    let reply = read_reply(WriterKind::Copilot, false, &exit(0), "plain words\n").unwrap();
    assert_eq!((reply.text.as_str(), reply.json), ("plain words\n", None));
}

#[test]
fn copilots_failures_are_told_apart() {
    let kind = |out: &str, code| read(WriterKind::Copilot, true, code, out).unwrap_err();
    let error =
        |message: &str| format!(r#"{{"type":"session.error","data":{{"message":"{message}"}}}}"#);
    assert_eq!(
        kind(&error("Not authenticated. Run copilot login."), 1),
        FailureKind::NotLoggedIn
    );
    assert_eq!(
        kind(&error("You have exceeded your rate limit."), 1),
        FailureKind::Quota
    );
    // An error record is a failure whatever the exit code says.
    assert_eq!(kind(&error("quota exceeded"), 0), FailureKind::Quota);
    assert_eq!(kind("Error: Not logged in", 1), FailureKind::NotLoggedIn);
    // Its docs: non-zero when interrupted or the final response is empty.
    assert_eq!(kind("", 1), FailureKind::Other);
    assert_eq!(kind("", 0), FailureKind::Empty);
    let silent = r#"{"type":"assistant.message","data":{"content":"  "}}
{"type":"session.idle","data":{}}"#;
    assert_eq!(kind(silent, 0), FailureKind::Empty);
    let error = read_reply(
        WriterKind::Copilot,
        true,
        &exit(1),
        &error("rate limit for gpt-x"),
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("rate limit for gpt-x"),
        "{error}"
    );
    assert!(!error.reason().contains("gpt-x"), "{}", error.reason());
}

// `agent -p --output-format json`'s one object, as Cursor's output-format page documents it.
const CURSOR_OK: &str = r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":1234,"duration_api_ms":1234,"result":"{\"answer\":\"pong\"}","session_id":"s","request_id":"r"}"#;

#[test]
fn a_cursor_reply_is_its_result_and_the_object_is_read_from_its_text() {
    let reply = read_reply(WriterKind::Cursor, true, &exit(0), CURSOR_OK).unwrap();
    assert_eq!(reply.text, r#"{"answer":"pong"}"#);
    assert_eq!(reply.json, Some(serde_json::json!({ "answer": "pong" })));
    // Its object has no usage.
    assert_eq!(reply.usage, None);
    let out = r#"{"type":"result","subtype":"success","is_error":false,"result":"plain words"}"#;
    assert_eq!(
        read(WriterKind::Cursor, false, 0, out),
        Ok("plain words".into())
    );
}

#[test]
fn cursors_failures_are_told_apart() {
    // On failure it exits non-zero with a message on standard error and no JSON; the writer classifies that failure by
    // standard error (read here as the output, as `read_reply` takes one text).
    let kind = |out: &str, code| read(WriterKind::Cursor, true, code, out).unwrap_err();
    assert_eq!(
        kind("Error: Not authenticated", 1),
        FailureKind::NotLoggedIn
    );
    assert_eq!(
        kind("Error: You've hit your usage limit", 1),
        FailureKind::Quota
    );
    assert_eq!(
        kind("Error: headless mode is disabled by your team", 1),
        FailureKind::Other
    );
    assert_eq!(kind("", 1), FailureKind::Other);
    assert_eq!(kind("", 0), FailureKind::Empty);
    let empty = r#"{"type":"result","subtype":"success","is_error":false,"result":""}"#;
    assert_eq!(kind(empty, 0), FailureKind::Empty);
    let error =
        r#"{"type":"result","subtype":"error","is_error":true,"result":"Not authenticated"}"#;
    assert_eq!(kind(error, 1), FailureKind::NotLoggedIn);
}

#[test]
fn a_program_that_is_not_installed_and_one_that_hangs_are_typed_failures() {
    let dir = tempfile::tempdir().unwrap();
    let writer = ClaudeWriter::new(dir.path().join("none"), dir.path().into())
        .with_agy(dir.path().join("no-agy"), dir.path().into())
        .with_codex(dir.path().join("no-codex"), dir.path().into())
        .with_copilot(dir.path().join("no-copilot"), dir.path().into())
        .with_cursor(dir.path().join("no-agent"), dir.path().into());
    let request = WriterRequest {
        user: "x".into(),
        timeout: Duration::from_secs(5),
        ..WriterRequest::default()
    };
    // An HTTP writer runs no program: its failures are read in `http_writer.rs`.
    for kind in WriterKind::ALL.into_iter().filter(|kind| !kind.http()) {
        let writer_kind = kind;
        writer.use_writer(writer_kind);
        let error = writer.complete(&request).unwrap_err();
        assert_eq!(error.kind, FailureKind::NotInstalled, "{kind:?}");
        assert_eq!(error.program, kind.program());
        // One line, whose program output stays after the first quote.
        assert_eq!(error.to_string().lines().count(), 1);
    }

    let hang = dir.path().join("hang");
    std::fs::write(&hang, "#!/bin/sh\nsleep 600\n").unwrap();
    std::fs::set_permissions(&hang, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let writer = ClaudeWriter::new(hang, dir.path().into());
    let request = WriterRequest {
        timeout: Duration::from_millis(200),
        ..request
    };
    let error = writer.complete(&request).unwrap_err();
    assert_eq!(error.kind, FailureKind::Timeout);
}

#[test]
fn the_failure_notice_says_summaries_are_off_and_gives_each_writer_its_own_fix() {
    let notice = |kind, program| {
        studio_understanding::WriterError {
            kind,
            program,
            detail: String::new(),
        }
        .notice()
    };
    for kind in [FailureKind::NotLoggedIn, FailureKind::NotInstalled] {
        let agy = notice(kind, "agy");
        assert!(agy.starts_with("Summaries are off: `agy` "), "{agy}");
        assert!(!agy.contains("Claude"), "{agy}");
        let claude = notice(kind, "claude");
        assert!(
            claude.starts_with("Summaries are off: `claude` "),
            "{claude}"
        );
        assert!(
            claude.contains("claude") && !claude.contains("agy"),
            "{claude}"
        );
    }
    assert!(notice(FailureKind::NotLoggedIn, "agy").contains("Log in to `agy`"));
    assert!(notice(FailureKind::NotInstalled, "agy").contains("Antigravity CLI"));
    assert!(notice(FailureKind::NotLoggedIn, "claude").contains("/login"));
    assert!(notice(FailureKind::NotInstalled, "claude").contains("Install Claude Code"));
    for kind in [FailureKind::NotLoggedIn, FailureKind::NotInstalled] {
        let codex = notice(kind, "codex");
        assert!(codex.starts_with("Summaries are off: `codex` "), "{codex}");
        assert!(
            !codex.contains("Claude") && !codex.contains("agy"),
            "{codex}"
        );
    }
    assert!(notice(FailureKind::NotLoggedIn, "codex").contains("`codex login`"));
    assert!(notice(FailureKind::NotInstalled, "codex").contains("Install the Codex CLI"));
    for (program, login, install) in [
        (
            "copilot",
            "`copilot login`",
            "Install the GitHub Copilot CLI",
        ),
        ("agent", "`agent login`", "Install the Cursor CLI"),
    ] {
        for kind in [FailureKind::NotLoggedIn, FailureKind::NotInstalled] {
            let line = notice(kind, program);
            assert!(
                line.starts_with(&format!("Summaries are off: `{program}` ")),
                "{line}"
            );
            assert!(!line.contains("Claude") && !line.contains("agy"), "{line}");
        }
        assert!(notice(FailureKind::NotLoggedIn, program).contains(login));
        assert!(notice(FailureKind::NotInstalled, program).contains(install));
    }
    // The quota fix points at the page, where the writer is picked now, not at a hand-edited file.
    let quota = notice(FailureKind::Quota, "claude");
    assert!(
        !quota.contains("settings.json") && quota.contains("Writer"),
        "{quota}"
    );
}

/// A stand-in login shell that answers `-lc <command>` with only `bin` on its `PATH`, and an executable `name` in `bin`.
fn faked_path(dir: &Path, programs: &[&str]) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for name in programs {
        std::fs::write(bin.join(name), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(bin.join(name), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let shell = dir.join("login-shell");
    std::fs::write(
        &shell,
        format!(
            "#!/bin/sh\nPATH='{}'\nexport PATH\nexec /bin/sh -c \"$2\"\n",
            bin.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    shell
}

#[test]
fn the_writers_found_are_the_ones_command_v_finds_in_their_order() {
    let dir = tempfile::tempdir().unwrap();
    let found = |programs: &[&str]| {
        let dir = dir.path().join(programs.join("-"));
        let shell = faked_path(&dir, programs);
        studio_understanding::found_writers(&shell)
            .into_iter()
            .map(|w| (w.kind, w.found))
            .collect::<Vec<_>>()
    };
    use WriterKind::{Agy, Anthropic, Claude, Codex, Copilot, Cursor, OpenAI};
    assert_eq!(
        found(&["agy"]),
        [
            (Anthropic, false),
            (OpenAI, false),
            (Claude, false),
            (Codex, false),
            (Agy, true),
            (Copilot, false),
            (Cursor, false)
        ]
    );
    assert_eq!(
        found(&["claude", "agy"]),
        [
            (Anthropic, false),
            (OpenAI, false),
            (Claude, true),
            (Codex, false),
            (Agy, true),
            (Copilot, false),
            (Cursor, false)
        ]
    );
    assert_eq!(
        found(&["codex"]),
        [
            (Anthropic, false),
            (OpenAI, false),
            (Claude, false),
            (Codex, true),
            (Agy, false),
            (Copilot, false),
            (Cursor, false)
        ]
    );
    // Cursor's program is `agent`.
    assert_eq!(
        found(&["copilot", "agent"]),
        [
            (Anthropic, false),
            (OpenAI, false),
            (Claude, false),
            (Codex, false),
            (Agy, false),
            (Copilot, true),
            (Cursor, true)
        ]
    );
    assert_eq!(
        found(&[]),
        [
            (Anthropic, false),
            (OpenAI, false),
            (Claude, false),
            (Codex, false),
            (Agy, false),
            (Copilot, false),
            (Cursor, false)
        ]
    );
    // An API-key writer runs no program, so none named `anthropic` makes it found: the server says whether its key is set.
    assert_eq!(found(&["anthropic"])[0], (Anthropic, false));
    assert_eq!(found(&["openai"])[1], (OpenAI, false));
    // Each writer brings its name, models and efforts for the page.
    let shell = faked_path(&dir.path().join("lists"), &["claude"]);
    let writers = studio_understanding::found_writers(&shell);
    assert_eq!(writers[0].name, "Anthropic API key");
    assert_eq!(writers[0].models, WriterKind::Anthropic.models());
    assert_eq!(writers[1].name, "OpenAI-compatible API");
    assert_eq!(writers[1].efforts, ["", "low", "medium", "high"]);
    assert_eq!(writers[2].name, "Claude");
    assert_eq!(writers[2].models, WriterKind::Claude.models());
    assert!(writers[2].efforts.contains(&"low"));
    assert_eq!(writers[3].name, "Codex");
    assert!(writers[3].models.contains(&"gpt-6-luna"));
    assert_eq!(writers[3].efforts, ["", "low", "medium", "high"]);
    assert_eq!(writers[4].name, "agy");
    assert!(
        writers[4].efforts.is_empty(),
        "agy's effort is part of its model id"
    );
    assert_eq!((writers[5].name, writers[6].name), ("Copilot", "Cursor"));
    assert!(writers[5].models.contains(&"claude-haiku-4.5"));
    assert!(writers[6].models.contains(&"auto"));
    assert!(writers[5].efforts.is_empty() && writers[6].efforts.is_empty());
}

#[test]
fn a_test_of_codex_passes_or_fails_with_a_typed_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let codex = dir.path().join("codex");
    let script = |body: &str| {
        std::fs::write(&codex, format!("#!/bin/sh\ncat > /dev/null\n{body}\n")).unwrap();
        std::fs::set_permissions(&codex, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    };
    let writer = ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into())
        .with_codex(codex.clone(), dir.path().into());
    script(&format!("cat <<'EOF'\n{CODEX_OK}EOF"));
    assert_eq!(writer.test(WriterKind::Codex, "gpt-6-luna", "low"), Ok(()));
    script(
        r#"echo '{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized"}}'; exit 1"#,
    );
    let error = writer
        .test(WriterKind::Codex, "gpt-6-luna", "")
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotLoggedIn);
    assert!(
        error.reason().contains("`codex login`"),
        "{}",
        error.reason()
    );
}

/// An executable `name` in `dir` that reads its input and runs `body`.
fn stub_program(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let program = dir.join(name);
    std::fs::write(&program, format!("#!/bin/sh\ncat > /dev/null\n{body}\n")).unwrap();
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    program
}

#[test]
fn a_test_of_copilot_passes_or_fails_with_a_typed_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let writer = |body: &str| {
        ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into())
            .with_copilot(stub_program(dir.path(), "copilot", body), dir.path().into())
    };
    let ok = writer(&format!("cat <<'EOF'\n{COPILOT_OK}EOF"));
    assert_eq!(ok.test(WriterKind::Copilot, "claude-haiku-4.5", ""), Ok(()));
    let error =
        writer(r#"echo '{"type":"session.error","data":{"message":"Not authenticated"}}'; exit 1"#)
            .test(WriterKind::Copilot, "claude-haiku-4.5", "")
            .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotLoggedIn);
    assert!(
        error.reason().contains("`copilot login`"),
        "{}",
        error.reason()
    );
}

#[test]
fn a_test_of_cursor_passes_or_fails_with_a_typed_outcome_read_from_standard_error() {
    let dir = tempfile::tempdir().unwrap();
    let writer = |body: &str| {
        ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into())
            .with_cursor(stub_program(dir.path(), "agent", body), dir.path().into())
    };
    let ok = writer(&format!("cat <<'EOF'\n{CURSOR_OK}\nEOF"));
    assert_eq!(ok.test(WriterKind::Cursor, "auto", ""), Ok(()));
    // Cursor writes its failure on standard error only.
    let error = writer("echo 'Error: Not authenticated' >&2; exit 1")
        .test(WriterKind::Cursor, "auto", "")
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::NotLoggedIn);
    assert!(
        error.reason().contains("`agent login`"),
        "{}",
        error.reason()
    );
    let error = writer("echo 'Error: You hit your usage limit' >&2; exit 1")
        .test(WriterKind::Cursor, "auto", "")
        .unwrap_err();
    assert_eq!(error.kind, FailureKind::Quota);
}

#[test]
fn a_cursor_reply_is_read_from_standard_output_whatever_standard_error_says() {
    let dir = tempfile::tempdir().unwrap();
    let reply =
        r#"{"type":"result","subtype":"success","is_error":false,"result":"Ship the writer."}"#;
    let body = format!(
        "echo 'warning: {{x}} is deprecated' >&2\ncat <<'EOF'\n{reply}\nEOF\necho 'note: {{y}}' >&2"
    );
    let writer = ClaudeWriter::new(dir.path().join("no-claude"), dir.path().into())
        .with_cursor(stub_program(dir.path(), "agent", &body), dir.path().into());
    writer.use_writer(WriterKind::Cursor);
    assert_eq!(writer.goal(&["Ship it."]), Ok("Ship the writer.".into()));
}

#[test]
fn a_writer_beside_the_log_knows_whether_the_login_shell_found_claude() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("events.jsonl");
    let without = faked_path(&dir.path().join("without"), &["agy"]);
    let with = faked_path(&dir.path().join("with"), &["claude"]);
    // SAFETY: nextest runs each test in its own process, so no other thread reads the environment.
    unsafe { std::env::set_var("SHELL", &without) };
    assert!(!ClaudeWriter::beside(&log).unwrap().claude_found());
    unsafe { std::env::set_var("SHELL", &with) };
    assert!(ClaudeWriter::beside(&log).unwrap().claude_found());
}
