//! The card writer (#328 and #369, Testing Decisions): the real `claude -p` command is checked
//! without running it, the cards and their quizzes come from its reply, and a fake that fails is
//! retried.

use std::{cell::Cell, ffi::OsStr, fs, os::unix::fs::PermissionsExt, path::Path};
use studio_core::{Base, CardRequest};
use studio_understanding::{
    Card, CardWriter, Choice, ClaudeWriter, FakeWriter, Quiz, find_claude, write_card,
};

const CLAUDE: &str = "/opt/homebrew/bin/claude";

fn edit() -> CardRequest {
    CardRequest {
        session_id: "2c7a9f14-6e3b-4d58-a1c9-0f8e2b7d4a63".into(),
        id: "2c7a9f14-6e3b-4d58-a1c9-0f8e2b7d4a63@1793263024310".into(),
        base: Base::default(),
        change: concat!(
            "--- a/apps/studio/ui/style.css\n",
            "+++ b/apps/studio/ui/style.css\n",
            "@@ -9,1 +9,1 @@\n",
            "-.unchecked { font-size:12px; }\n",
            "+.unchecked { font-size:15px; }\n",
        )
        .into(),
    }
}

/// A fake writer whose first `fails` calls reply with no card.
struct Failing {
    fails: usize,
    calls: Cell<usize>,
}

impl CardWriter for Failing {
    fn reply(&self, request: &CardRequest) -> Result<String, String> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() <= self.fails {
            return Ok("Shall I read the file?".into());
        }
        FakeWriter.reply(request)
    }
}

/// The card `writer` writes on `edit()`, and the lines it logs.
fn written(writer: &impl CardWriter) -> (Option<Card>, Vec<String>) {
    let mut lines = Vec::new();
    let card = write_card(writer, &edit(), |line| lines.push(line));
    (card, lines)
}

#[test]
fn a_writer_that_fails_once_gives_the_card_on_its_retry() {
    let writer = Failing {
        fails: 1,
        calls: Cell::new(0),
    };

    let (card, lines) = written(&writer);

    assert!(card.is_some());
    assert_eq!(lines, [] as [String; 0]);
    assert_eq!(writer.calls.get(), 2);
}

#[test]
fn a_writer_that_fails_twice_gives_no_card_and_one_log_line() {
    let writer = Failing {
        fails: usize::MAX,
        calls: Cell::new(0),
    };

    let (card, lines) = written(&writer);

    assert_eq!(card, None);
    assert_eq!(writer.calls.get(), 2);
    let [line] = &lines[..] else {
        panic!("one log line: {lines:?}");
    };
    assert_eq!(line.lines().count(), 1, "{line}");
    assert!(line.contains(&edit().id), "{line}");
}

/// A fake writer that always gives `reply`.
struct Replying(String);

impl CardWriter for Replying {
    fn reply(&self, _: &CardRequest) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

/// A good reply on `edit()`, in a code fence: the explanation and a quiz on its design.
const GOOD: &str = concat!(
    "```json\n",
    r#"{"explanation": "הסוכן הגדיל את `font-size` של `.unchecked` כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ.", "#,
    r#""paths": ["apps/studio/ui/style.css"], "#,
    r#""code": "-.unchecked { font-size:12px; }\n+.unchecked { font-size:15px; }", "#,
    r#""why_it_matters": "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.", "#,
    r#""not_checked": "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה.", "#,
    r#""quiz": {"text": "למה השינוי נוגע רק ב־`.unchecked`?", "options": ["#,
    r#"{"text": "כי רק השורה הזו הייתה קטנה מדי לקריאה.", "why": "נכון: שאר הכרטיס כבר היה ב־`15px`."}, "#,
    r#"{"text": "כי `.unchecked` קובע את גודל הגופן של כל הכרטיס.", "why": "לא: הכלל חל רק על השורה עצמה."}, "#,
    r#"{"text": "כי גודל הגופן של הכרטיס בא מהגדרות המערכת.", "why": "לא: הגודל קבוע ב־`style.css`."}"#,
    r#"], "key": 0}}"#,
    "\n```\n",
);

fn good_card() -> Card {
    let choice = |text: &str, why: &str| Choice {
        text: text.into(),
        why: why.into(),
    };
    Card {
        explanation:
            "הסוכן הגדיל את `font-size` של `.unchecked` כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ."
                .into(),
        paths: vec!["apps/studio/ui/style.css".into()],
        code: "-.unchecked { font-size:12px; }\n+.unchecked { font-size:15px; }".into(),
        why_it_matters: "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.".into(),
        not_checked: "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה.".into(),
        quiz: Some(Quiz {
            text: "למה השינוי נוגע רק ב־`.unchecked`?".into(),
            options: vec![
                choice(
                    "כי רק השורה הזו הייתה קטנה מדי לקריאה.",
                    "נכון: שאר הכרטיס כבר היה ב־`15px`.",
                ),
                choice(
                    "כי `.unchecked` קובע את גודל הגופן של כל הכרטיס.",
                    "לא: הכלל חל רק על השורה עצמה.",
                ),
                choice(
                    "כי גודל הגופן של הכרטיס בא מהגדרות המערכת.",
                    "לא: הגודל קבוע ב־`style.css`.",
                ),
            ],
            key: 0,
        }),
    }
}

#[test]
fn a_good_reply_gives_the_card_with_its_quiz_options_key_and_reasons() {
    assert_eq!(written(&Replying(GOOD.into())), (Some(good_card()), vec![]));
}

#[test]
fn a_quiz_with_no_key_or_one_that_does_not_parse_logs_one_line_and_the_card_has_no_quiz() {
    let broken = [
        GOOD.replace(r#", "key": 0"#, ""),
        // Options with no reasons.
        GOOD.replace(r#", "why": ""#, r#", "note": ""#),
        // A key past the last option.
        GOOD.replace(r#""key": 0"#, r#""key": 3"#),
        GOOD.replace(r#""key": 0"#, r#""key": "א""#),
        // No quiz at all.
        GOOD.replace(r#""quiz": {"#, r#""other": {"#),
    ];

    for reply in broken {
        let (card, lines) = written(&Replying(reply.clone()));

        let no_quiz = Card {
            quiz: None,
            ..good_card()
        };
        assert_eq!(card, Some(no_quiz), "{reply}");
        let [line] = &lines[..] else {
            panic!("one log line on {reply}: {lines:?}");
        };
        assert_eq!(line.lines().count(), 1, "{line}");
        assert!(line.contains(&edit().id), "{line}");
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
    assert!(prompt.contains("multiple-choice question on the change's logic or design"));
    assert!(prompt.ends_with(&format!("\nChange:\n{}", edit().change)));
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
fn a_reply_that_does_not_parse_gives_no_card_and_one_log_line() {
    let replies = [
        "",
        "I can't write a card without the file.\nShall I read it?",
        // No explanation.
        r#"{"quiz": {"text": "למה?"}, "why_it_matters": "כי."}"#,
        // A `}` before the first `{`.
        "Sorry :} I need {more context",
    ];

    for reply in replies {
        let line = Card::from_reply(reply).expect_err(reply);
        assert_eq!(line.lines().count(), 1, "{line}");
    }
}
