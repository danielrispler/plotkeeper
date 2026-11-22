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
fn the_agy_command_takes_the_schema_runs_in_its_own_folder_and_is_marked_internal() {
    let (command, stdin) = build_command(
        WriterKind::Agy,
        Path::new("/bin/agy"),
        Path::new("/w/card-writer-agy"),
        &request(),
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
            "m1",
            "--effort",
            "low",
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

#[test]
fn a_program_that_is_not_installed_and_one_that_hangs_are_typed_failures() {
    let dir = tempfile::tempdir().unwrap();
    let writer = ClaudeWriter::new(dir.path().join("none"), dir.path().into())
        .with_agy(dir.path().join("no-agy"), dir.path().into());
    let request = WriterRequest {
        user: "x".into(),
        timeout: Duration::from_secs(5),
        ..WriterRequest::default()
    };
    for kind in [WriterKind::Claude, WriterKind::Agy] {
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
