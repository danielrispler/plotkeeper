//! The card writer (#328, Testing Decisions): the real `claude -p` command is checked without
//! running it, and the cards come from its reply.

use std::{ffi::OsStr, fs, os::unix::fs::PermissionsExt, path::Path};
use studio_core::{CardRequest, Check, Step};
use studio_understanding::{Card, ClaudeWriter, find_claude};

const CLAUDE: &str = "/opt/homebrew/bin/claude";

fn edit() -> CardRequest {
    CardRequest {
        session_id: "2c7a9f14-6e3b-4d58-a1c9-0f8e2b7d4a63".into(),
        tool_use_id: "toolu_01Hc4Xe7Lm2Qp9Rs5Tv8Wy3Z".into(),
        step: Step {
            tool: "Edit".into(),
            detail: Some("apps/studio/ui/style.css".into()),
        },
        change: concat!(
            "@@ -9,1 +9,1 @@\n",
            "-.unchecked { font-size:12px; }\n",
            "+.unchecked { font-size:15px; }\n",
        )
        .into(),
        check: None,
    }
}

#[test]
fn the_real_writer_runs_claude_on_the_subscription_in_its_neutral_folder() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let command = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&edit());

    assert_eq!(command.get_program(), CLAUDE);
    // Both keys would take precedence over the subscription login, and without the plugin the
    // writer's session sends no hook events; nothing else is passed.
    let mut env: Vec<_> = command.get_envs().collect();
    env.sort();
    assert_eq!(
        env,
        [
            (OsStr::new("ANTHROPIC_API_KEY"), None),
            (OsStr::new("ANTHROPIC_AUTH_TOKEN"), None),
            (OsStr::new("CLAUDE_CODE_PLUGIN_DIRS"), None),
        ]
    );
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_str().unwrap())
        .collect();
    // The change is untrusted text: no tools and no MCP servers.
    assert_eq!(args[..4], ["--tools", "", "--strict-mcp-config", "-p"]);
    assert!(!args.contains(&"--bare"));
    let prompt = args[4];
    assert!(prompt.contains("why the agent took this step"));
    assert!(prompt.contains("Edit apps/studio/ui/style.css"));
    assert!(prompt.contains(&edit().change));
    assert_eq!(command.get_current_dir(), Some(folder));
}

#[test]
fn a_huge_change_is_cut_so_the_prompt_stays_small() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    // A new file of 100 000 two-byte letters, as added lines.
    let request = CardRequest {
        change: format!("+{}\n", "א".repeat(100_000)),
        ..edit()
    };
    let command = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&request);

    let prompt = command.get_args().last().unwrap().to_str().unwrap();
    assert!(prompt.len() < 70 << 10, "{}", prompt.len());
    assert!(prompt.ends_with(" more bytes cut]"));
}

/// A stand-in login shell named `name`, which runs `script` and never `claude`.
fn shell(dir: &Path, name: &str, script: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_login_shell_finds_claude_and_without_its_answer_the_path_does() {
    let dir = tempfile::tempdir().unwrap();
    // A profile may print before the answer.
    let answers = shell(
        dir.path(),
        "answers",
        &format!(r#"[ "$1|$2" = "-lc|command -v claude" ] || exit 2; echo Welcome; echo {CLAUDE}"#),
    );
    let fails = shell(dir.path(), "fails", "exit 1");

    assert_eq!(find_claude(&answers), Path::new(CLAUDE));
    assert_eq!(find_claude(&fails), Path::new("claude"));
    assert_eq!(
        find_claude(&dir.path().join("missing")),
        Path::new("claude")
    );
}

#[test]
fn a_reply_with_one_json_object_gives_the_card_even_in_a_code_fence() {
    let reply = concat!(
        "```json\n",
        r#"{"question": "למה הסוכן הגדיל את `font-size` של `.unchecked`?", "#,
        r#""answer": "כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ.", "#,
        r#""paths": ["apps/studio/ui/style.css"], "#,
        r#""code": "-.unchecked { font-size:12px; }\n+.unchecked { font-size:15px; }", "#,
        r#""why_it_matters": "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.", "#,
        r#""not_checked": "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה."}"#,
        "\n```\n",
    );

    assert_eq!(
        Card::from_reply(reply),
        Ok(Card {
            question: "למה הסוכן הגדיל את `font-size` של `.unchecked`?".into(),
            answer: "כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ.".into(),
            paths: vec!["apps/studio/ui/style.css".into()],
            code: "-.unchecked { font-size:12px; }\n+.unchecked { font-size:15px; }".into(),
            why_it_matters: "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.".into(),
            not_checked: "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה.".into(),
            // A reply without `passed`, like an edit's, still parses.
            passed: None,
        })
    );
}

fn check() -> CardRequest {
    CardRequest {
        session_id: "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18".into(),
        tool_use_id: "toolu_01Uy4Ka9Lo3Pv6Bs1Dc8Fh5R".into(),
        step: Step {
            tool: "Bash".into(),
            detail: Some("cargo test 2>&1 | tail -5".into()),
        },
        change: "test result: FAILED. 11 passed; 1 failed; 0 ignored".into(),
        check: Some(Check {
            guess: Some(true),
            exit_passed: None,
        }),
    }
}

#[test]
fn a_checks_card_writer_gets_its_command_and_result_and_says_whether_it_passed() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let command = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&check());

    let prompt = command.get_args().last().unwrap().to_str().unwrap();
    assert!(prompt.contains("Bash cargo test 2>&1 | tail -5\nResult:\n"));
    assert!(prompt.contains(&check().change));
    assert!(prompt.contains(r#""passed""#));
    // Daniel's guess is never part of the writer's input.
    assert!(!prompt.contains("guess"));
}

#[test]
fn a_checks_card_says_passed_failed_or_unknown() {
    let passed = |value: &str| {
        let reply = format!(r#"{{"question": "למה?", "answer": "כי.", "passed": {value}}}"#);
        Card::from_reply(&reply).expect(value).passed
    };

    assert_eq!(passed("true"), Some(true));
    assert_eq!(passed("false"), Some(false));
    assert_eq!(passed(r#""unknown""#), None);
    assert_eq!(passed("null"), None);
}

#[test]
fn a_reply_that_does_not_parse_gives_no_card_and_one_log_line() {
    let replies = [
        "",
        "I can't write a card without the file.\nShall I read it?",
        r#"{"question": "למה?", "why_it_matters": "כי."}"#,
        // A `}` before the first `{`.
        "Sorry :} I need {more context",
    ];

    for reply in replies {
        let line = Card::from_reply(reply).expect_err(reply);
        assert_eq!(line.lines().count(), 1, "{line}");
    }
}
