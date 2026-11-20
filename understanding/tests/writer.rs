//! The card writer (danielrispler/Falinks#328 and danielrispler/Falinks#369, Testing Decisions): the real `claude -p` command is checked
//! without running it, the cards, their quizzes and the updated brief come from its reply, and a
//! fake that fails is retried.

use std::{
    cell::Cell,
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use studio_core::{
    Base, CardRequest, Check, CheckRun, CheckStatus, FactFile, Facts, SummaryRequest, TurnFile,
};
use studio_understanding::{
    Card, CardStore, CardWriter, Choice, ClaudeWriter, FakeWriter, LineKind, Quiz, SummarySetup,
    TurnSummary, explain_more, find_claude, retest_quiz, write_agent_summary, write_card,
    write_retests, write_summary,
};

const CLAUDE: &str = "/opt/homebrew/bin/claude";

fn edit() -> CardRequest {
    CardRequest {
        session_id: "2c7a9f14-6e3b-4d58-a1c9-0f8e2b7d4a63".into(),
        id: "2c7a9f14-6e3b-4d58-a1c9-0f8e2b7d4a63@1793263024310".into(),
        base: Base::default(),
        change: Some(
            concat!(
                "--- a/apps/studio/ui/style.css\n",
                "+++ b/apps/studio/ui/style.css\n",
                "@@ -9,1 +9,1 @@\n",
                "-.unchecked { font-size:12px; }\n",
                "+.unchecked { font-size:15px; }\n",
            )
            .into(),
        ),
        prompt: None,
        ..CardRequest::default()
    }
}

/// A fake writer whose first `fails` calls reply with no card.
struct Failing {
    fails: usize,
    calls: Cell<usize>,
}

impl CardWriter for Failing {
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() <= self.fails {
            return Ok("Shall I read the file?".into());
        }
        FakeWriter.reply(request, earlier)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }
}

/// The card `writer` writes on `edit()`, with no earlier card, and the lines it
/// logs.
fn written(writer: &impl CardWriter) -> (Option<Card>, Vec<String>) {
    let mut lines = Vec::new();
    let card = write_card(writer, &edit(), &[], |line| lines.push(line));
    (card.map(sorted), lines)
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
    fn reply(&self, _: &CardRequest, _: &[String]) -> Result<String, String> {
        Ok(self.0.clone())
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

/// A good reply on `edit()`, in a code fence: the explanation, a quiz on its design, no
/// architecture section.
const GOOD: &str = concat!(
    "```json\n",
    r#"{"summary": "הגופן של שורת „לא נבדק” גדל ל־`15px`.", "#,
    r#""explanation": "הסוכן הגדיל את `font-size` של `.unchecked` כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ.", "#,
    r#""paths": ["apps/studio/ui/style.css"], "#,
    r#""ranges": [], "#,
    r#""why_it_matters": "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.", "#,
    r#""not_checked": "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה.", "#,
    r#""architecture": "structural: none", "#,
    r#""quiz": {"text": "למה השינוי נוגע רק ב־`.unchecked`?", "options": ["#,
    r#"{"text": "כי רק השורה הזו הייתה קטנה מדי לקריאה.", "why": "נכון: שאר הכרטיס כבר היה ב־`15px`.", "right": true}, "#,
    r#"{"text": "כי `.unchecked` קובע את גודל הגופן של כל הכרטיס.", "why": "לא: הכלל חל רק על השורה עצמה.", "right": false}, "#,
    r#"{"text": "כי גודל הגופן של הכרטיס בא מהגדרות המערכת.", "why": "לא: הגודל קבוע ב־`style.css`.", "right": false}"#,
    r#"], "key": 0}}"#,
    "\n```\n",
);

/// `card` with its quiz's options in the order of their text, the key moved with them, so cards
/// compare whatever order the shuffle gave.
fn sorted(mut card: Card) -> Card {
    if let Some(quiz) = &mut card.quiz {
        let right = quiz.options[quiz.key].clone();
        quiz.options.sort_by(|a, b| a.text.cmp(&b.text));
        quiz.key = quiz.options.iter().position(|o| *o == right).unwrap();
    }
    card
}

/// The card in `GOOD`, its options sorted.
fn good_card() -> Card {
    let choice = |text: &str, why: &str| Choice {
        text: text.into(),
        why: why.into(),
    };
    let card = Card {
        summary: "הגופן של שורת „לא נבדק” גדל ל־`15px`.".into(),
        explanation:
            "הסוכן הגדיל את `font-size` של `.unchecked` כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ."
                .into(),
        paths: vec!["apps/studio/ui/style.css".into()],
        code: String::new(),
        ranges: Vec::new(),
        why_it_matters: "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.".into(),
        not_checked: "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה.".into(),
        architecture: None,
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
    };
    sorted(card)
}

#[test]
fn the_app_shuffles_the_quizs_options_and_the_key_and_reasons_move_with_them() {
    let right = "כי רק השורה הזו הייתה קטנה מדי לקריאה.";
    let mut keys = std::collections::BTreeSet::new();
    for _ in 0..60 {
        let (card, _) = Card::from_reply(GOOD).unwrap();
        let quiz = card.quiz.clone().unwrap();

        assert_eq!(quiz.options[quiz.key].text, right);
        assert_eq!(sorted(card), good_card());
        keys.insert(quiz.key);
    }
    assert_eq!(keys.len(), 3, "{keys:?}");
}

#[test]
fn the_writer_places_no_option_and_never_names_options_by_letter() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let (_, prompt) = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&edit(), &[]);

    assert!(!prompt.contains("random place"), "{prompt}");
    assert!(
        prompt.contains("never refer to an option by its letter"),
        "{prompt}"
    );
}

#[test]
fn a_reply_with_an_architecture_section_gives_the_card_that_section() {
    let section = "הקוד עבר מ־`lib.rs` למודול חדש, `brief.rs`, ש־`lib.rs` קורא לו.";
    let reply = GOOD.replace("structural: none", section);

    let (card, lines) = written(&Replying(reply));

    let structural = Card {
        architecture: Some(section.into()),
        ..good_card()
    };
    assert_eq!((card, lines), (Some(structural), vec![]));
}

#[test]
fn the_card_call_returns_the_card_and_asks_for_no_brief() {
    let card = write_card(&Replying(GOOD.into()), &edit(), &[], |l| panic!("{l}"));
    assert_eq!(card.map(sorted), Some(good_card()));

    let (_, prompt) = ClaudeWriter::new(CLAUDE.into(), "/f".into()).command(&edit(), &[]);
    assert!(!prompt.contains("brief"), "{prompt}");
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

/// `GOOD` with its three options' texts, reasons and marks replaced by `options`, the key its first.
fn good_with(options: [(&str, &str, bool); 3]) -> String {
    let quiz: Vec<_> = (options.iter())
        .map(|(text, why, right)| serde_json::json!({ "text": text, "why": why, "right": right }))
        .collect();
    let start = GOOD.find(r#""options": ["#).unwrap();
    let end = GOOD.find(r#"], "key": 0}"#).unwrap();
    let options = serde_json::to_string(&quiz).unwrap();
    format!(
        "{}\"options\": {options}{}",
        &GOOD[..start],
        &GOOD[end + 1..]
    )
}

#[test]
fn a_quiz_that_breaks_a_rule_is_rejected_and_the_card_keeps_no_quiz() {
    let ten = "one two three four five six seven eight nine ten";
    let eleven = "one two three four five six seven eight nine ten eleven";
    // An option of 10 words passes.
    assert!(
        Card::from_reply(&good_with([
            ("b", "Why.", true),
            (ten, "No.", false),
            ("c", "No.", false)
        ]))
        .unwrap()
        .1
        .is_none()
    );
    let broken = [
        // An option of 11 words.
        good_with([
            ("short", "Why.", true),
            (eleven, "No.", false),
            ("b", "No.", false),
        ]),
        // The key is the longest option.
        good_with([
            ("a long right option", "Why.", true),
            ("b", "No.", false),
            ("c", "No.", false),
        ]),
        // The key is not the option marked right.
        good_with([
            ("a", "Why.", false),
            ("bbb", "No.", true),
            ("cc", "No.", false),
        ]),
        // Two options marked right.
        good_with([
            ("a", "Why.", true),
            ("bbb", "Yes.", true),
            ("cc", "No.", false),
        ]),
        // No option marked right.
        good_with([
            ("a", "Why.", false),
            ("bbb", "No.", false),
            ("cc", "No.", false),
        ]),
        // A missing reason.
        good_with([
            ("a", "Why.", true),
            ("bbb", "", false),
            ("cc", "No.", false),
        ]),
        // A missing mark.
        GOOD.replace(r#", "right": false"#, ""),
    ];
    for reply in broken {
        let (card, problem) = Card::from_reply(&reply).unwrap();
        assert_eq!(card.quiz, None, "{reply}");
        assert!(problem.is_some(), "{reply}");
    }
}

/// A fake writer that gives `replies` in turn, the last again and again, and counts its calls.
struct CardSequence {
    replies: Vec<String>,
    calls: Cell<usize>,
}

impl CardWriter for CardSequence {
    fn reply(&self, _: &CardRequest, _: &[String]) -> Result<String, String> {
        let at = self.calls.get().min(self.replies.len() - 1);
        self.calls.set(self.calls.get() + 1);
        Ok(self.replies[at].clone())
    }
    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }
}

#[test]
fn a_rejected_quiz_is_asked_for_once_more_and_the_second_rejection_leaves_the_card_with_none() {
    let longest = good_with([
        ("a long right option", "Why.", true),
        ("b", "No.", false),
        ("c", "No.", false),
    ]);

    // The second reply is good: its quiz stands.
    let writer = CardSequence {
        replies: vec![longest.clone(), GOOD.into()],
        calls: Cell::new(0),
    };
    let card = write_card(&writer, &edit(), &[], |l| panic!("{l}")).unwrap();
    assert_eq!(sorted(card), good_card());
    assert_eq!(writer.calls.get(), 2);

    // Both are rejected: the card is kept with no quiz, after two calls and no more.
    let writer = CardSequence {
        replies: vec![longest],
        calls: Cell::new(0),
    };
    let mut lines = Vec::new();
    let card = write_card(&writer, &edit(), &[], |l| lines.push(l)).unwrap();
    assert_eq!(card.quiz, None);
    assert_eq!(writer.calls.get(), 2);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("longest"), "{lines:?}");
}

#[test]
fn the_card_prompt_carries_the_quiz_rules() {
    let folder = Path::new("/card-writer");
    let (_, prompt) = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&edit(), &[]);
    for rule in RULES {
        assert!(prompt.contains(rule), "{rule}: {prompt}");
    }
}

#[test]
fn the_card_prompts_name_ranges_and_never_ask_for_copied_code() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
    let (_, card) = writer.command(&edit(), &[]);
    assert!(
        card.contains("\"ranges\"") && !card.contains("copied from it"),
        "{card}"
    );
    let check = CardRequest {
        check: Some("cargo test".into()),
        ..edit()
    };
    let (_, check) = writer.command(&check, &[]);
    assert!(
        !check.contains("copied from it") && !check.contains("\"code\""),
        "{check}"
    );
}

/// What both writers' prompts say of the Quiz (`docs/studio/question-rules.md`).
const RULES: [&str; 6] = [
    "why this design over the alternative",
    "cannot be answered from",
    "the alternative that was considered, the previous behaviour, or a common wrong model",
    "at most 10 words",
    "the right option is never the longest",
    "exactly one option is right",
];

#[test]
fn the_real_writer_runs_claude_on_the_subscription_in_its_neutral_folder() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let earlier = ["`Card` קיבל שדה `summary`.".to_string()];
    let (command, prompt) =
        ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&edit(), &earlier);

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
    // The change is untrusted text: no tools and no MCP servers, and safe mode keeps Daniel's
    // plugins, hooks, skills and CLAUDE.md out (danielrispler/Falinks#517, decision 9). The prompt goes on standard input.
    assert_eq!(
        args,
        ["--tools", "", "--strict-mcp-config", "--safe-mode", "-p"]
    );
    assert!(prompt.contains("multiple-choice question on the change's logic or design"));
    // The architecture section reads the prompt's earlier cards.
    assert!(prompt.contains(&format!("earlier cards, oldest first:\n- {}\n", earlier[0])));
    assert!(prompt.ends_with(&format!("\nChange:\n{}", edit().diff())));
    assert_eq!(command.get_current_dir(), Some(folder));
}

#[test]
fn every_writer_run_is_in_safe_mode() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
    let reply = FakeWriter.reply(&edit(), &[]).unwrap();
    let (card, _) = Card::from_reply(&reply).unwrap();
    let runs = [
        ("card", writer.command(&edit(), &[])),
        ("goal", writer.goal_command(&["a prompt"])),
        ("explain more", writer.more_command(&edit(), &card)),
        ("ask", writer.ask_command("an item", &[], "why?")),
        (
            "summary",
            writer.summary_command(&setup(), &turn("s", 1), &[]),
        ),
        (
            "subagent summary",
            writer.agent_summary_command(&setup(), "a prompt", "a message"),
        ),
    ];
    for (name, (command, _)) in runs {
        let args: Vec<_> = command.get_args().collect();
        assert!(
            args.contains(&OsStr::new("--safe-mode")),
            "{name}: {args:?}"
        );
    }
}

#[test]
fn an_oversized_goal_input_cuts_the_oldest_prompts_first() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    // Three prompts of 30 KiB each, 90 KiB in all.
    let prompts: Vec<String> = ["first", "second", "third"]
        .iter()
        .map(|name| format!("{name} {}", "x".repeat(30 << 10)))
        .collect();
    let prompts: Vec<&str> = prompts.iter().map(String::as_str).collect();

    let (_, prompt) = writer.goal_command(&prompts);

    assert!(
        prompt.contains("Goal of a coding agent's session"),
        "{prompt}"
    );
    assert!(!prompt.contains("first x"));
    assert!(prompt.contains("second x") && prompt.contains("third x"));
    assert!(prompt.len() < 70 << 10, "{}", prompt.len());
}

#[test]
fn a_goal_whose_newest_prompt_alone_is_over_the_budget_keeps_its_start() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    // A pasted log of 100 KiB.
    let pasted = format!("Why does this fail?\n{}", "x".repeat(100 << 10));

    let (_, prompt) = writer.goal_command(&["Add a", &pasted]);

    assert!(!prompt.contains("Add a"), "{prompt}");
    assert!(
        prompt.contains("<prompt>\nWhy does this fail?\nxxx"),
        "{prompt}"
    );
    assert!(prompt.len() < 70 << 10, "{}", prompt.len());
}

#[test]
fn every_prompt_asks_for_english_and_none_for_hebrew() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/f".into());
    let prompts = [
        writer.command(&edit(), &[]).1,
        writer.more_command(&edit(), &good_card()).1,
        writer.goal_command(&["Add a"]).1,
        writer.ask_command("item", &[], "Why?").1,
    ];
    for prompt in prompts {
        assert!(
            prompt.contains("Write in English, in plain words"),
            "{prompt}"
        );
        assert!(!prompt.contains("Hebrew"), "{prompt}");
    }
}

#[test]
fn explain_more_runs_the_same_writer_on_the_cards_explanation_and_its_change() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    let card = good_card();

    let (more, prompt) = writer.more_command(&edit(), &card);

    let (card_run, _) = writer.command(&edit(), &[]);
    assert_eq!(more.get_program(), card_run.get_program());
    assert!(more.get_envs().eq(card_run.get_envs()));
    assert!(more.get_args().eq(card_run.get_args()));
    assert_eq!(more.get_current_dir(), Some(folder));
    assert!(prompt.contains("in more depth than the card"), "{prompt}");
    assert!(prompt.contains(&card.explanation), "{prompt}");
    assert!(prompt.ends_with(&format!("\nChange:\n{}", edit().diff())));
}

#[test]
fn explain_more_on_a_card_built_from_workers_tells_the_writer_there_is_no_diff() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    let workers = CardRequest {
        change: Some("No diff: the agent changed nothing itself since its last card. Its workers, each with its status and the summaries of its cards since then:\n- `Implement #378` (completed): `a.rs` נוסף.\n".into()),
        ..edit()
    };

    let (_, prompt) = writer.more_command(&workers, &good_card());

    let (_, card) = writer.command(&workers, &[]);
    let clause = card
        .lines()
        .find(|line| line.starts_with("When the change is not a diff"))
        .unwrap();
    assert!(clause.contains("how the work was split"), "{clause}");
    assert!(prompt.contains("When the change is not a diff"), "{prompt}");
    assert!(
        prompt.contains("how the work was split and what came back"),
        "{prompt}"
    );
    assert!(prompt.ends_with(workers.diff()));
}

#[test]
fn a_huge_change_is_cut_so_the_prompt_stays_small() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    // A new file of 100 000 two-byte letters, as added lines.
    let request = CardRequest {
        change: Some(format!("+{}\n", "א".repeat(100_000))),
        ..edit()
    };
    let (_, prompt) = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&request, &[]);

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

/// The writer on a stub `claude` in `dir` that runs `script`. Its timeout is long: a stub that has
/// to start and finish is not racing a clock, however loaded the machine is; a test of the timeout
/// itself sets a short one.
fn stub(dir: &Path, script: &str) -> ClaudeWriter {
    let claude = shell(dir, "claude", script);
    ClaudeWriter::new(claude, dir.into()).with_timeout(Duration::from_secs(30))
}

/// A store in `dir` that holds `good_card()` on `edit()`.
fn stored(dir: &Path) -> Mutex<CardStore> {
    let mut store = CardStore::open(dir.join("cards.json")).unwrap();
    store.add(edit(), good_card(), 1).unwrap();
    Mutex::new(store)
}

/// Whether process `pid` still runs, given a moment to go once it was stopped.
fn running(pid: &str) -> bool {
    (0..50).all(|_| {
        let kill = Command::new("kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status();
        let alive = kill.unwrap().success();
        alive && {
            std::thread::sleep(Duration::from_millis(20));
            true
        }
    })
}

#[test]
fn a_claude_that_fails_prints_nothing_or_hangs_gives_nothing_keeps_nothing_and_leaves_nothing_running()
 {
    // Stubs that start `sh` and reply keep a second; only the hang is cut short.
    let scripts = [
        ("echo 'Not logged in'; exit 1", Duration::from_secs(1)),
        ("cat > /dev/null", Duration::from_secs(1)),
        // A hung call whose own child also hangs.
        (
            "sleep 600 & echo $! > child; sleep 600",
            Duration::from_millis(200),
        ),
    ];
    for (script, timeout) in scripts {
        let dir = tempfile::tempdir().unwrap();
        let store = stored(dir.path());
        let mut lines = Vec::new();

        let writer = stub(dir.path(), script).with_timeout(timeout);
        let more = explain_more(&store, &writer, &edit().id, |l| lines.push(l));

        assert_eq!(more, None, "{script}");
        assert_eq!(store.lock().unwrap().cards()[0].more, None, "{script}");
        let [line] = &lines[..] else {
            panic!("one log line on {script}: {lines:?}");
        };
        assert_eq!(line.lines().count(), 1, "{line}");
        if let Ok(child) = fs::read_to_string(dir.path().join("child")) {
            assert!(!running(child.trim()), "{script}");
        }
    }
}

#[test]
fn explain_more_after_a_failed_call_asks_the_writer_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = stored(dir.path());
    // Fails on its first call, then explains.
    let writer = stub(
        dir.path(),
        "cat > /dev/null; [ -e called ] || { touch called; exit 1; }; echo 'הסבר ארוך'",
    );
    let log = |_| {};

    assert_eq!(explain_more(&store, &writer, &edit().id, log), None);
    assert_eq!(
        explain_more(&store, &writer, &edit().id, log).as_deref(),
        Some("הסבר ארוך")
    );
}

#[test]
fn a_change_holding_nul_bytes_reaches_the_writer_on_standard_input_and_gets_a_card() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("reply"), GOOD).unwrap();
    let writer = stub(dir.path(), "cat > input; cat reply");
    let request = CardRequest {
        change: Some("+++ b/data.bin\n+a\0b\n".into()),
        ..edit()
    };

    let card = write_card(&writer, &request, &[], |l| panic!("{l}"));

    assert!(card.is_some());
    let input = fs::read_to_string(dir.path().join("input")).unwrap();
    assert!(
        input.ends_with("\nChange:\n+++ b/data.bin\n+ab\n"),
        "{input}"
    );
}

// The turn summary (danielrispler/Falinks#526; danielrispler/Falinks#517, decisions 13 to 15).

/// A turn of session `session`, named by `n`, that gets a summary.
fn turn(session: &str, n: u64) -> SummaryRequest {
    SummaryRequest {
        session_id: session.into(),
        id: format!("{session}@{n}"),
        prompt: "Explain the parser".into(),
        responses: vec![
            "Reading the parser first.".into(),
            "Rewrote the parser.\n\nIt keeps a bracket stack.".into(),
        ],
        intentions: vec![
            "Reading the parser first.".into(),
            "Rewrote the parser.".into(),
        ],
        card: None,
        facts: Facts::default(),
        ..SummaryRequest::default()
    }
}

fn setup() -> SummarySetup {
    SummarySetup {
        model: "claude-haiku-4-5".into(),
        effort: "medium".into(),
    }
}

/// A summary reply whose Quiz's options are `options`, the first the key and right.
fn summary_reply(summary: &str, options: [&str; 3]) -> String {
    let option = |(i, text): (usize, &str)| serde_json::json!({ "text": text, "why": format!("Because of {text}."), "right": i == 0 });
    let mut lines = summary.lines();
    let headline = lines.next().unwrap_or_default();
    let lines: Vec<_> = lines
        .map(|l| serde_json::json!({ "kind": "reported", "text": l }))
        .collect();
    serde_json::json!({
        "headline": headline,
        "lines": lines,
        "quiz": {
            "text": "Why a stack?",
            "options": options.into_iter().enumerate().map(option).collect::<Vec<_>>(),
            "key": 0,
        },
    })
    .to_string()
}

#[test]
fn the_summary_call_runs_in_safe_mode_with_the_settings_model_effort_and_its_own_system_prompt() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
    let (command, input) = writer.summary_command(&setup(), &turn("s", 1), &[]);

    let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
    let [tools, none, mcp, safe, model, m, effort, e, system, s, p] = args[..] else {
        panic!("{args:?}");
    };
    assert_eq!(
        [tools, none, mcp, safe, model, m, effort, e, system, p],
        [
            "--tools",
            "",
            "--strict-mcp-config",
            "--safe-mode",
            "--model",
            "claude-haiku-4-5",
            "--effort",
            "medium",
            "--system-prompt",
            "-p"
        ]
    );
    assert!(s.contains("turn summary"), "{s}");
    // The answer stays out of the options (danielrispler/Falinks#517, decision 14).
    assert!(s.contains("Never mark the right option"), "{s}");
    // The question tests understanding, and only when the turn has something to teach (danielrispler/Falinks#583).
    for rule in ["keeps its bound or condition", r#""quiz": null"#] {
        assert!(s.contains(rule), "{rule}: {s}");
    }
    // The same rules as the card's (`docs/studio/question-rules.md`).
    for rule in RULES {
        assert!(s.contains(rule), "{rule}: {s}");
    }
    assert!(input.contains("Explain the parser"));
    assert!(input.contains("It keeps a bracket stack."));
    assert!(input.contains("- Rewrote the parser.\n"));
}

#[test]
fn an_empty_effort_leaves_the_models_default() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
    let setup = SummarySetup {
        effort: String::new(),
        ..setup()
    };
    let (command, _) = writer.summary_command(&setup, &turn("s", 1), &[]);
    assert!(!command.get_args().any(|a| a == "--effort"));
}

/// The real summary command's input, recorded, with a fake reply.
struct Recording {
    inputs: Mutex<Vec<String>>,
    reply: String,
}

impl CardWriter for Recording {
    fn reply(&self, r: &CardRequest, e: &[String]) -> Result<String, String> {
        FakeWriter.reply(r, e)
    }
    fn more(&self, r: &CardRequest, c: &Card) -> Result<String, String> {
        FakeWriter.more(r, c)
    }
    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
        let (_, input) = writer.summary_command(setup, request, earlier);
        self.inputs.lock().unwrap().push(input);
        Ok(self.reply.clone())
    }
    fn summarize_agent(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        message: &str,
    ) -> Result<String, String> {
        let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
        let (_, input) = writer.agent_summary_command(setup, prompt, message);
        self.inputs.lock().unwrap().push(input);
        Ok(self.reply.clone())
    }
}

fn recording(reply: String) -> Recording {
    Recording {
        inputs: Mutex::default(),
        reply,
    }
}

fn summaries(dir: &Path) -> Mutex<CardStore> {
    Mutex::new(CardStore::open(dir.join("cards.json")).unwrap())
}

/// Summary names, in words: a digit in a headline must be in the turn's material.
const NAMES: [&str; 5] = ["zero", "one", "two", "three", "four"];

#[test]
fn the_summary_input_holds_the_sessions_last_3_summaries_and_their_questions() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    for n in 1..=4 {
        let writer = recording(summary_reply(
            &format!("Summary {}.", NAMES[n as usize]),
            ["a", "b", "c"],
        ));
        write_summary(&store, &writer, &setup(), turn("s", n), |_| {}).unwrap();
    }
    // Another session's summary is not this session's.
    let other = recording(summary_reply("Other session.", ["a", "b", "c"]));
    write_summary(&store, &other, &setup(), turn("t", 5), |_| {}).unwrap();

    let writer = recording(summary_reply("Summary six.", ["a", "b", "c"]));
    write_summary(&store, &writer, &setup(), turn("s", 6), |_| {}).unwrap();

    let input = writer.inputs.lock().unwrap().pop().unwrap();
    assert!(!input.contains("Summary one."), "{input}");
    assert!(!input.contains("Other session."), "{input}");
    for n in 2..=4 {
        assert!(
            input.contains(&format!("Summary {}.", NAMES[n as usize])),
            "{input}"
        );
    }
    // The fourth Quiz within the hour was not offered (danielrispler/plotkeeper#119), so the summary of 4 has none.
    assert_eq!(input.matches("Why a stack?").count(), 2, "{input}");
    let kept = store.lock().unwrap().turn_summary("s@6").cloned();
    assert_eq!(kept.unwrap().summary.summary, "Summary six.");
    // The store keeps them across restarts.
    assert!(
        summaries(dir.path())
            .lock()
            .unwrap()
            .turn_summary("s@6")
            .is_some()
    );
}

#[test]
fn a_summary_reply_gives_the_summary_and_the_quiz() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(summary_reply(
        "Rewrote the parser.\nIt keeps a stack.",
        [
            "It matches nesting.",
            "It is faster.",
            "It uses less memory.",
        ],
    ));

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |_| {}).unwrap();

    assert_eq!(summary.summary, "Rewrote the parser.\nIt keeps a stack.");
    let quiz = summary.quiz.expect("a quiz");
    assert_eq!(quiz.text, "Why a stack?");
    assert_eq!(quiz.options[quiz.key].text, "It matches nesting.");
}

#[test]
fn a_reply_with_a_null_quiz_gives_the_summary_alone_and_logs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(r#"{"headline": "Merged the lanes.", "quiz": null}"#.into());
    let mut lines = Vec::new();

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

    assert_eq!(summary.unwrap().quiz, None);
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(writer.inputs.lock().unwrap().len(), 1, "no retry");
}

/// A summary reply whose Quiz is `options`, each `(text, why, right)`, with key `key`.
fn quiz_reply(options: &[(&str, &str, bool)], key: usize) -> String {
    let options: Vec<_> = (options.iter())
        .map(|(text, why, right)| serde_json::json!({ "text": text, "why": why, "right": right }))
        .collect();
    serde_json::json!({
        "headline": "Rewrote the parser.",
        "lines": [],
        "quiz": { "text": "Why a stack?", "options": options, "key": key },
    })
    .to_string()
}

const ELEVEN: &str = "one two three four five six seven eight nine ten eleven";

#[test]
fn a_reply_that_breaks_a_quiz_rule_is_asked_once_more_and_the_second_rejection_leaves_no_quiz() {
    let broken = [
        // An option of 11 words.
        quiz_reply(
            &[
                ("a", "Why.", true),
                (ELEVEN, "No.", false),
                ("c", "No.", false),
            ],
            0,
        ),
        // The key is the longest option.
        quiz_reply(
            &[
                ("The longest", "Why.", true),
                ("b", "No.", false),
                ("c", "No.", false),
            ],
            0,
        ),
        // The key is not the option marked right.
        quiz_reply(
            &[
                ("a", "Why.", false),
                ("bbb", "No.", true),
                ("cc", "No.", false),
            ],
            0,
        ),
        // Two options marked right.
        quiz_reply(
            &[
                ("a", "Why.", true),
                ("bbb", "Yes.", true),
                ("cc", "No.", false),
            ],
            0,
        ),
        // A missing reason.
        quiz_reply(
            &[
                ("a", "Why.", true),
                ("bbb", " ", false),
                ("cc", "No.", false),
            ],
            0,
        ),
        // A missing mark.
        quiz_reply(
            &[
                ("a", "Why.", true),
                ("bbb", "No.", false),
                ("cc", "No.", false),
            ],
            0,
        )
        .replace(r#""right":false,"#, ""),
    ];
    for reply in broken {
        let dir = tempfile::tempdir().unwrap();
        let store = summaries(dir.path());
        let writer = recording(reply.clone());
        let mut lines = Vec::new();

        let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

        let summary = summary.unwrap();
        assert_eq!(summary.summary, "Rewrote the parser.", "{reply}");
        assert_eq!(summary.quiz, None, "{reply}");
        assert_eq!(
            writer.inputs.lock().unwrap().len(),
            2,
            "asked once more: {reply}"
        );
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("s@1"), "{lines:?}");
    }
}

#[test]
fn a_rejected_quiz_gives_one_on_the_second_reply_when_that_one_is_good() {
    let longest = quiz_reply(
        &[
            ("The longest", "Why.", true),
            ("b", "No.", false),
            ("c", "No.", false),
        ],
        0,
    );
    let good = summary_reply("Rewrote the parser.", ["a", "bb", "cc"]);

    let (summary, inputs) = written_by(turn("s", 1), &[longest, good]);

    assert!(summary.quiz.is_some());
    assert_eq!(inputs.len(), 2);
}

#[test]
fn an_option_of_ten_words_and_a_key_that_is_not_the_longest_pass_on_the_first_call() {
    let ten = "one two three four five six seven eight nine ten";
    for options in [
        [
            ("a", "Why.", true),
            (ten, "No.", false),
            ("c", "No.", false),
        ],
        [
            ("O(n)", "Why.", true),
            ("O(n log n)", "No.", false),
            ("O(1)", "No.", false),
        ],
        // Equal lengths: the key is not longer than the others.
        [
            ("aa", "Why.", true),
            ("bb", "No.", false),
            ("cc", "No.", false),
        ],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = summaries(dir.path());
        let writer = recording(quiz_reply(&options, 0));

        let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |_| {}).unwrap();

        assert!(summary.quiz.is_some(), "{options:?}");
        assert_eq!(writer.inputs.lock().unwrap().len(), 1, "{options:?}");
    }
}

#[test]
fn a_reply_with_no_quiz_field_gives_the_summary_alone_and_logs_why() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(r#"{"headline": "Rewrote the parser."}"#.into());
    let mut lines = Vec::new();

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

    assert_eq!(summary.unwrap().quiz, None);
    assert_eq!(lines.len(), 1, "only an explicit null is silent: {lines:?}");
    assert_eq!(writer.inputs.lock().unwrap().len(), 1, "no retry");
}

#[test]
fn a_learning_section_stored_before_the_quiz_reads_as_a_quiz() {
    let old = r#"{"summary": "Rewrote the parser.", "learning": {"idea": "A stack.", "question": {"text": "Why a stack?", "options": [{"text": "It nests.", "why": "Yes."}, {"text": "It is fast.", "why": "No."}], "key": 0}}}"#;

    let summary: TurnSummary = serde_json::from_str(old).unwrap();
    let quiz = summary.quiz.clone().expect("the question reads as a Quiz");
    assert_eq!((quiz.text.as_str(), quiz.key), ("Why a stack?", 0));
    // Stored again, it is a Quiz, which reads back the same.
    let again: TurnSummary =
        serde_json::from_str(&serde_json::to_string(&summary).unwrap()).unwrap();
    assert_eq!(again, summary);

    let none: TurnSummary = serde_json::from_str(r#"{"summary": "x", "learning": null}"#).unwrap();
    assert_eq!(none.quiz, None);
}

#[test]
fn a_reply_that_marks_the_answer_inside_the_options_has_the_mark_removed() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(summary_reply(
        "Rewrote the parser.",
        [
            "✅ It matches nesting.",
            "It is faster.",
            "It uses less memory (correct)",
        ],
    ));

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |_| {}).unwrap();

    let options = summary.quiz.unwrap().options.into_iter();
    let mut options: Vec<String> = options.map(|o| o.text).collect();
    options.sort();
    assert_eq!(
        options,
        [
            "It is faster.",
            "It matches nesting.",
            "It uses less memory"
        ]
    );
}

#[test]
fn a_malformed_summary_reply_gives_none_and_one_log_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording("Shall I read the transcript?".into());
    let mut lines = Vec::new();

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

    assert_eq!(summary, None);
    assert_eq!(writer.inputs.lock().unwrap().len(), 2, "asked once more");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("s@1"), "{lines:?}");
    assert_eq!(store.lock().unwrap().turn_summary("s@1"), None);
}

#[test]
fn a_turn_with_a_card_gets_its_summary_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(summary_reply("Rewrote the parser.", ["a", "b", "c"]));
    let on = edit();
    let card = write_card(&FakeWriter, &on, &[], |_| {}).unwrap();
    store
        .lock()
        .unwrap()
        .add(
            CardRequest {
                id: "s@1".into(),
                ..on
            },
            card,
            1,
        )
        .unwrap();
    let request = SummaryRequest {
        card: Some("s@1".into()),
        ..turn("s", 1)
    };

    let summary = write_summary(&store, &writer, &setup(), request, |_| {}).unwrap();

    assert_eq!(summary.summary, "Rewrote the parser.");
    assert_eq!(summary.quiz, None);
    let input = writer.inputs.lock().unwrap().pop().unwrap();
    assert!(input.contains("has a card"), "{input}");
}

#[test]
fn a_card_the_offer_did_not_write_is_no_card_and_the_summary_carries_the_quiz() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(summary_reply("Rewrote the parser.", ["a", "b", "c"]));
    let request = SummaryRequest {
        card: Some("s@1".into()),
        ..turn("s", 1)
    };

    let summary = write_summary(&store, &writer, &setup(), request, |_| {}).unwrap();

    assert!(summary.quiz.is_some());
    let input = writer.inputs.lock().unwrap().pop().unwrap();
    assert!(!input.contains("has a card"), "{input}");
}

// The subagent summary, written when Daniel opens a subagent (danielrispler/Falinks#535).

const SPAWNED: &str = "Review the parser change.\nLook for off-by-ones.";
const LAST: &str = "Done.\nThe parser change is fine: the bounds hold.";

#[test]
fn the_subagent_summary_call_runs_in_safe_mode_with_the_settings_model_and_effort() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
    let (command, input) = writer.agent_summary_command(&setup(), SPAWNED, LAST);

    let args: Vec<_> = command.get_args().map(|a| a.to_str().unwrap()).collect();
    let [tools, none, mcp, safe, model, m, effort, e, system, s, p] = args[..] else {
        panic!("{args:?}");
    };
    assert_eq!(
        [tools, none, mcp, safe, model, m, effort, e, system, p],
        [
            "--tools",
            "",
            "--strict-mcp-config",
            "--safe-mode",
            "--model",
            "claude-haiku-4-5",
            "--effort",
            "medium",
            "--system-prompt",
            "-p"
        ]
    );
    assert!(s.contains("subagent"), "{s}");
    assert!(input.contains(SPAWNED) && input.contains(LAST), "{input}");
}

#[test]
fn opening_a_subagent_writes_its_summary_once_and_opening_it_again_makes_no_call() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(r#"{"summary": "Reviewed the parser change: the bounds hold."}"#.into());

    let first = write_agent_summary(&store, &writer, &setup(), "c1", SPAWNED, LAST, |_| {});
    let again = write_agent_summary(
        &summaries(dir.path()),
        &writer,
        &setup(),
        "c1",
        SPAWNED,
        LAST,
        |_| {},
    );

    assert_eq!(
        first.as_deref(),
        Some("Reviewed the parser change: the bounds hold.")
    );
    assert_eq!(again, first, "kept across restarts");
    let inputs = writer.inputs.lock().unwrap();
    assert_eq!(inputs.len(), 1, "one call");
    assert!(inputs[0].contains(SPAWNED) && inputs[0].contains(LAST));
}

#[test]
fn a_failed_subagent_summary_keeps_nothing_and_logs_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording("Shall I read the transcript?".into());
    let mut lines = Vec::new();

    let summary = write_agent_summary(&store, &writer, &setup(), "c1", SPAWNED, LAST, |l| {
        lines.push(l)
    });

    assert_eq!(summary, None);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("c1"), "{lines:?}");
    assert_eq!(store.lock().unwrap().agent_summary("c1"), None);
    let failing = Failing {
        fails: 0,
        calls: Cell::new(0),
    };
    assert_eq!(
        write_agent_summary(&store, &failing, &setup(), "c1", SPAWNED, LAST, |_| {}),
        None
    );
}

/// A writer on `program` whose availability notices go to the returned list: `Some(text)` when no
/// call can start, `None` when one works again (danielrispler/plotkeeper#51).
fn watched(
    program: &Path,
    dir: &Path,
) -> (ClaudeWriter, std::sync::Arc<Mutex<Vec<Option<String>>>>) {
    let heard = std::sync::Arc::new(Mutex::new(Vec::new()));
    let sink = heard.clone();
    let writer = ClaudeWriter::new(program.into(), dir.into())
        .with_timeout(Duration::from_secs(30))
        .on_availability(move |notice| sink.lock().unwrap().push(notice));
    (writer, heard)
}

#[test]
fn a_writer_that_cannot_start_gives_one_notice_until_a_call_works() {
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("claude");
    let (writer, heard) = watched(&program, dir.path());

    for _ in 0..3 {
        assert!(writer.reply(&edit(), &[]).is_err());
    }
    {
        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 1, "{heard:?}");
        let text = heard[0].as_deref().unwrap();
        assert!(text.contains("Cards and briefs are off"), "{text}");
    }

    shell(dir.path(), "claude", "echo ok");
    assert_eq!(writer.reply(&edit(), &[]).unwrap().trim(), "ok");
    assert_eq!(heard.lock().unwrap().last(), Some(&None));
    assert_eq!(heard.lock().unwrap().len(), 2);
}

#[test]
fn a_writer_that_starts_and_fails_raises_no_notice() {
    let dir = tempfile::tempdir().unwrap();
    let claude = shell(dir.path(), "claude", "exit 3");
    let (writer, heard) = watched(&claude, dir.path());

    assert!(writer.reply(&edit(), &[]).is_err());
    assert!(heard.lock().unwrap().is_empty());
}

// The grounded turn writer (danielrispler/plotkeeper#115).

fn file(path: &str, added: u64, removed: u64, hunk: &str) -> FactFile {
    FactFile {
        file: TurnFile {
            root: "/w/repo".into(),
            path: path.into(),
            from: None,
            added: Some(added),
            removed: Some(removed),
            agent: None,
        },
        hunk: hunk.into(),
    }
}

fn check(status: CheckStatus, retries: u32, exit_code: i32) -> Check {
    let run = CheckRun {
        command: "npm run check".into(),
        status,
        exit_code: Some(exit_code),
        call: "c1".into(),
        agent: None,
    };
    Check {
        suite: "npm run check".into(),
        status,
        retries,
        runs: vec![run],
    }
}

/// A turn whose facts hold one file with a 47-line-added hunk and `checks`.
fn with_facts(checks: Vec<Check>) -> SummaryRequest {
    let hunk = "@@ -1,1 +1,1 @@\n-let width = 30;\n+let width = 4096;\n";
    SummaryRequest {
        facts: Facts {
            files: vec![file("src/parser2.rs", 47, 3, hunk)],
            checks,
            ..Facts::default()
        },
        ..turn("s", 1)
    }
}

fn reply(headline: &str, lines: serde_json::Value) -> String {
    serde_json::json!({ "headline": headline, "lines": lines, "quiz": null }).to_string()
}

/// The summary `writer` replies for `request`, and the inputs it was given.
fn written_by(request: SummaryRequest, replies: &[String]) -> (TurnSummary, Vec<String>) {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = Sequence {
        replies: Mutex::new(replies.to_vec()),
        inputs: Mutex::default(),
    };
    let summary = write_summary(&store, &writer, &setup(), request, |_| {}).unwrap();
    let inputs = writer.inputs.lock().unwrap().clone();
    (summary, inputs)
}

/// A writer that gives each of its replies in turn, the last again after that.
struct Sequence {
    replies: Mutex<Vec<String>>,
    inputs: Mutex<Vec<String>>,
}

impl CardWriter for Sequence {
    fn reply(&self, r: &CardRequest, e: &[String]) -> Result<String, String> {
        FakeWriter.reply(r, e)
    }
    fn more(&self, r: &CardRequest, c: &Card) -> Result<String, String> {
        FakeWriter.more(r, c)
    }
    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
        let (_, input) = writer.summary_command(setup, request, earlier);
        self.inputs.lock().unwrap().push(input);
        let mut replies = self.replies.lock().unwrap();
        Ok(match replies.len() {
            1 => replies[0].clone(),
            _ => replies.remove(0),
        })
    }
}

#[test]
fn the_writers_input_holds_the_turns_files_counts_capped_hunks_and_check_facts() {
    let long = (1..=100).map(|n| format!("+line{n}\n")).collect::<String>();
    let mut request = with_facts(vec![check(CheckStatus::Passed, 1, 0)]);
    request.facts.files.push(file("src/long.rs", 100, 0, &long));

    let (_, inputs) = written_by(
        request,
        &[reply("Widened the parser.", serde_json::json!([]))],
    );

    let input = &inputs[0];
    assert!(input.contains("src/parser2.rs +47 -3"), "{input}");
    assert!(input.contains("+let width = 4096;"), "{input}");
    assert!(
        input.contains("npm run check: Passed, after 1 earlier failed runs"),
        "{input}"
    );
    assert!(
        input.contains("+line40") && !input.contains("+line41"),
        "{input}"
    );
    assert!(
        !input.contains("other files") && !input.contains("external"),
        "{input}"
    );
}

#[test]
fn typed_lines_parse_and_a_line_of_an_unknown_type_is_dropped() {
    let lines = serde_json::json!([
        { "kind": "why", "text": "A bracket stack keeps nesting." },
        { "kind": "opinion", "text": "I like it." },
        { "kind": "open", "text": "Nobody looked at it in a real window." },
        { "kind": "ask", "text": "Merge it?" },
    ]);

    let (summary, _) = written_by(turn("s", 1), &[reply("Rewrote the parser.", lines)]);

    assert_eq!(summary.headline.as_deref(), Some("Rewrote the parser."));
    let kinds: Vec<LineKind> = summary.lines.iter().map(|l| l.kind).collect();
    assert_eq!(kinds, [LineKind::Why, LineKind::Open, LineKind::Ask]);
    assert_eq!(
        summary.summary,
        "Rewrote the parser.\nA bracket stack keeps nesting.\nNobody looked at it in a real window.\nMerge it?"
    );
}

#[test]
fn a_fix_with_no_passed_check_is_the_agents_claim_and_one_with_a_passed_check_is_not() {
    let said = |checks: Vec<Check>| {
        let request = with_facts(checks);
        written_by(
            request,
            &[reply("Fixed the overlay", serde_json::json!([]))],
        )
        .0
        .headline
    };

    assert_eq!(
        said(vec![]).as_deref(),
        Some("Agent says: Fixed the overlay")
    );
    assert_eq!(
        said(vec![check(CheckStatus::Failed, 0, 1)]).as_deref(),
        Some("Agent says: Fixed the overlay")
    );
    assert_eq!(
        said(vec![check(CheckStatus::Ran, 0, 0)]).as_deref(),
        Some("Agent says: Fixed the overlay")
    );
    assert_eq!(
        said(vec![check(CheckStatus::Passed, 0, 0)]).as_deref(),
        Some("Fixed the overlay")
    );
}

#[test]
fn a_line_with_a_number_or_id_the_turn_lacks_is_stripped_and_true_numbers_stay() {
    let request = with_facts(vec![check(CheckStatus::Passed, 2, 0)]);
    let texts = [
        "Made 12 changes.",                // an invented count
        "Cut the cost by 40%.",            // a percentage
        "Closes #123.",                    // an issue id
        "Reverted in abcdef1.",            // a SHA
        "Used one stack and two passes.",  // number words
        "Raised the width to 4096.",       // in the hunk
        "Changed 47 lines and removed 3.", // in the line counts
        "Passed after 2 retries.",         // in the check facts
        "Edited src/parser2.rs.",          // digits in a known path
        "Edited src/parser3.rs.",          // digits in an unknown path
    ];
    let kept: Vec<&str> = texts
        .into_iter()
        .filter(|text| {
            let lines = serde_json::json!([{ "kind": "why", "text": text }]);
            let (summary, _) = written_by(request.clone(), &[reply("Widened the parser.", lines)]);
            !summary.lines.is_empty()
        })
        .collect();

    assert_eq!(kept, [texts[4], texts[5], texts[6], texts[7], texts[8]]);
}

#[test]
fn a_headline_with_an_invented_number_is_asked_for_once_more_and_a_second_failure_is_stripped() {
    let bad = reply("Cut the cost by 40%.", serde_json::json!([]));
    let good = reply("Cut the cost.", serde_json::json!([]));

    let (summary, inputs) = written_by(turn("s", 1), &[bad.clone(), good]);
    assert_eq!(summary.headline.as_deref(), Some("Cut the cost."));
    assert_eq!(inputs.len(), 2);

    let (summary, inputs) = written_by(turn("s", 1), &[bad.clone(), bad]);
    assert_eq!(summary.headline.as_deref(), Some(""));
    assert_eq!(inputs.len(), 2);
}

#[test]
fn a_headline_over_16_words_is_asked_for_once_more() {
    let long = reply(&"word ".repeat(17), serde_json::json!([]));
    let short = reply("Short.", serde_json::json!([]));

    let (summary, inputs) = written_by(turn("s", 1), &[long.clone(), short]);
    assert_eq!(summary.headline.as_deref(), Some("Short."));
    assert_eq!(inputs.len(), 2);

    // A second long one stands, as the Quiz's does.
    let (summary, _) = written_by(turn("s", 1), &[long.clone(), long]);
    assert_eq!(summary.headline.unwrap().split_whitespace().count(), 17);
}

#[test]
fn a_summary_stored_before_the_headline_still_reads() {
    let old = r#"{"summary": "Rewrote the parser.\nIt keeps a stack.", "quiz": null}"#;

    let summary: TurnSummary = serde_json::from_str(old).unwrap();

    assert_eq!(summary.summary, "Rewrote the parser.\nIt keeps a stack.");
    assert_eq!(summary.headline, None);
    assert_eq!(summary.lines, []);
}

/// A store holding one Quiz on turn `t1` with its hunk, and when it was offered.
fn week_old_quiz(dir: &Path) -> (CardStore, u64) {
    let mut store = CardStore::open(dir.join("cards.json")).unwrap();
    add_quiz(&mut store, "t1", T0);
    (store, T0)
}

/// A fixed clock for the retests' tests.
const T0: u64 = 1_790_000_000_000;
const DAY: u64 = 24 * 3600 * 1000;

/// Keeps a Quiz on turn `id` with its hunk, offered at `at`.
fn add_quiz(store: &mut CardStore, id: &str, at: u64) {
    let request = SummaryRequest {
        id: id.into(),
        session_id: "s1".into(),
        facts: Facts {
            files: vec![FactFile {
                file: TurnFile {
                    root: "/w".into(),
                    path: "src/gate.rs".into(),
                    from: None,
                    added: Some(1),
                    removed: Some(1),
                    agent: None,
                },
                hunk: "@@ -1 +1 @@\n-open\n+frozen\n".into(),
            }],
            ..Facts::default()
        },
        ..Default::default()
    };
    let summary: TurnSummary = serde_json::from_value(serde_json::json!({
        "summary": "Froze the gate.",
        "quiz": { "text": "Why freeze the gate?", "key": 0, "options": [
            { "text": "The pairs must not move", "why": "Scores come later, so the pairs set is fixed first." },
            { "text": "Speed", "why": "No." }] },
    }))
    .unwrap();
    store.add_summary_at(request, summary, at).unwrap();
}

#[test]
fn the_retests_writer_input_holds_the_stored_hunk_and_the_keys_reason() {
    let dir = tempfile::tempdir().unwrap();
    let (store, offered) = week_old_quiz(dir.path());
    let ask = store.due_retests(offered + 7 * 24 * 3600 * 1000).remove(0);
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());

    let (command, prompt) = writer.retest_command(&ask);

    assert!(prompt.contains("+frozen"), "{prompt}");
    assert!(
        prompt.contains("Scores come later, so the pairs set is fixed first."),
        "{prompt}"
    );
    assert!(prompt.contains("Why freeze the gate?"), "{prompt}");
    assert!(command.get_args().any(|a| a == "--safe-mode"));
    // The rules of any Quiz are in the instructions.
    assert!(prompt.contains("at most 10 words"), "{prompt}");
}

/// A retest reply on the gate, with the right option the longest or not.
fn retest_reply(longest_right: bool) -> String {
    let right = if longest_right {
        "Only the two sets stay fixed"
    } else {
        "Pairs"
    };
    serde_json::json!({ "text": "What must stay true after the freeze?", "key": 0, "options": [
        { "text": right, "why": "Yes.", "right": true },
        { "text": "Scores", "why": "No.", "right": false },
        { "text": "Order", "why": "No.", "right": false }] })
    .to_string()
}

/// A writer that gives the replies in order and counts its calls.
struct Retesting(Vec<String>, Cell<usize>);

impl CardWriter for Retesting {
    fn reply(&self, _: &CardRequest, _: &[String]) -> Result<String, String> {
        Err("no cards".into())
    }
    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Err("no cards".into())
    }
    fn retest(&self, _: &studio_understanding::RetestAsk) -> Result<String, String> {
        let at = self.1.get();
        self.1.set(at + 1);
        self.0.get(at).cloned().ok_or_else(|| "no reply".into())
    }
}

#[test]
fn a_retest_is_written_once_a_week_after_and_its_reply_passes_the_quiz_guards() {
    let dir = tempfile::tempdir().unwrap();
    let (store, offered) = week_old_quiz(dir.path());
    let store = Mutex::new(store);
    let week = 7 * 24 * 3600 * 1000;
    // The first reply breaks a guard (its right option is longest), the second is good: one retry.
    let writer = Retesting(vec![retest_reply(true), retest_reply(false)], Cell::new(0));

    // Six days after: none. Seven: one, written; and never again.
    assert_eq!(
        write_retests(&store, &writer, offered + week - 1, |l| panic!("{l}")),
        0
    );
    assert_eq!(writer.1.get(), 0);
    assert_eq!(
        write_retests(&store, &writer, offered + week, |l| panic!("{l}")),
        1
    );
    assert_eq!(
        write_retests(&store, &writer, offered + 2 * week, |l| panic!("{l}")),
        0
    );

    assert_eq!(writer.1.get(), 2);
    let store = store.lock().unwrap();
    let quiz = store.retests()[0].quiz.as_ref().unwrap();
    assert_eq!(quiz.options[quiz.key].text, "Pairs");
}

#[test]
fn a_retest_that_breaks_the_guards_twice_is_kept_as_none_and_not_asked_again() {
    let dir = tempfile::tempdir().unwrap();
    let (store, offered) = week_old_quiz(dir.path());
    let store = Mutex::new(store);
    let writer = Retesting(vec![retest_reply(true), retest_reply(true)], Cell::new(0));
    let mut lines = Vec::new();
    let now = offered + 7 * 24 * 3600 * 1000;

    assert_eq!(write_retests(&store, &writer, now, |l| lines.push(l)), 0);
    assert_eq!(write_retests(&store, &writer, now, |l| lines.push(l)), 0);

    assert_eq!(writer.1.get(), 2);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(store.lock().unwrap().retests()[0].quiz, None);
}

/// A store with a Quiz offered on each of `quizzes`, `(id, at)`, and a retest already written on each of `waiting`.
fn retests_store(dir: &Path, quizzes: &[(&str, u64)], waiting: &[&str]) -> Mutex<CardStore> {
    let mut store = CardStore::open(dir.join("cards.json")).unwrap();
    for (id, at) in quizzes {
        add_quiz(&mut store, id, *at);
    }
    let now = T0 + 8 * DAY;
    for ask in store.due_retests(now) {
        if waiting.contains(&ask.of.as_str()) {
            let quiz = retest_quiz(&retest_reply(false)).unwrap();
            store.add_retest(&ask, Some(quiz), now).unwrap();
        }
    }
    Mutex::new(store)
}

/// The Quizzes the store holds a retest of, in the order they were written.
fn retested(store: &Mutex<CardStore>) -> Vec<String> {
    let store = store.lock().unwrap();
    store.retests().iter().map(|r| r.of.clone()).collect()
}

#[test]
fn with_three_retests_waiting_a_run_writes_none_and_calls_no_writer() {
    let dir = tempfile::tempdir().unwrap();
    let quizzes = [("a", T0), ("b", T0), ("c", T0), ("d", T0)];
    let store = retests_store(dir.path(), &quizzes, &["a", "b", "c"]);
    let writer = Retesting(vec![retest_reply(false); 4], Cell::new(0));

    assert_eq!(
        write_retests(&store, &writer, T0 + 8 * DAY, |l| panic!("{l}")),
        0
    );

    assert_eq!(writer.1.get(), 0);
    assert_eq!(retested(&store), ["a", "b", "c"]);
}

#[test]
fn with_one_waiting_a_run_writes_two_picked_quizzes_first_and_the_latest_due_first() {
    use studio_understanding::Tap;
    let dir = tempfile::tempdir().unwrap();
    let quizzes = [
        ("waiting", T0),
        ("a", T0),
        ("b", T0 + DAY),
        ("c", T0 + 5 * DAY),
        ("d", T0 + 4 * DAY),
        ("e", T0 + 3 * DAY),
    ];
    let store = retests_store(dir.path(), &quizzes, &["waiting"]);
    for id in ["a", "b"] {
        let mut store = store.lock().unwrap();
        store
            .quiz_tap(id, Tap::Pick { option: 1 }, T0 + 2 * DAY)
            .unwrap();
    }
    let writer = Retesting(vec![retest_reply(false); 5], Cell::new(0));

    // c, d and e came due later than a and b, but were never picked.
    assert_eq!(
        write_retests(&store, &writer, T0 + 13 * DAY, |l| panic!("{l}")),
        2
    );

    assert_eq!(writer.1.get(), 2);
    assert_eq!(retested(&store), ["waiting", "b", "a"]);
}

#[test]
fn after_a_pick_on_one_of_three_waiting_the_next_run_writes_the_held_retest() {
    use studio_understanding::Tap;
    let dir = tempfile::tempdir().unwrap();
    let quizzes = [("a", T0), ("b", T0), ("c", T0), ("d", T0)];
    let store = retests_store(dir.path(), &quizzes, &["a", "b", "c"]);
    let writer = Retesting(vec![retest_reply(false)], Cell::new(0));
    let now = T0 + 9 * DAY;
    assert_eq!(write_retests(&store, &writer, now, |l| panic!("{l}")), 0);

    (store.lock().unwrap())
        .quiz_tap("retest a", Tap::Pick { option: 0 }, now)
        .unwrap();

    assert_eq!(
        write_retests(&store, &writer, now + 3600 * 1000, |l| panic!("{l}")),
        1
    );
    assert_eq!(writer.1.get(), 1);
    assert_eq!(retested(&store), ["a", "b", "c", "d"]);
}

#[test]
fn a_writer_that_gives_no_reply_ends_the_run_and_its_quiz_stays_due() {
    let dir = tempfile::tempdir().unwrap();
    let quizzes = [("waiting", T0), ("a", T0), ("b", T0), ("c", T0), ("d", T0)];
    let store = retests_store(dir.path(), &quizzes, &["waiting"]);
    let writer = Retesting(Vec::new(), Cell::new(0));
    let mut lines = Vec::new();

    assert_eq!(
        write_retests(&store, &writer, T0 + 9 * DAY, |l| lines.push(l)),
        0
    );

    // One ask, tried twice; the rest are not asked until the next run.
    assert_eq!(writer.1.get(), 2);
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(retested(&store), ["waiting"]);
}

#[test]
fn a_retest_rejected_twice_does_not_wait_so_the_run_writes_on_until_three_wait() {
    let dir = tempfile::tempdir().unwrap();
    let quizzes = [("waiting", T0), ("a", T0), ("b", T0), ("c", T0)];
    let store = retests_store(dir.path(), &quizzes, &["waiting"]);
    let (bad, good) = (retest_reply(true), retest_reply(false));
    let writer = Retesting(vec![bad.clone(), bad, good.clone(), good], Cell::new(0));
    let mut lines = Vec::new();

    assert_eq!(
        write_retests(&store, &writer, T0 + 9 * DAY, |l| lines.push(l)),
        2
    );

    assert_eq!(writer.1.get(), 4);
    assert_eq!(retested(&store), ["waiting", "a", "b", "c"]);
    assert_eq!(store.lock().unwrap().retests()[1].quiz, None);
}

/// A writer that flags the Quizzes `flag` Key wrong while it writes its first retest, as Daniel might meanwhile.
struct Flagging<'a> {
    store: &'a Mutex<CardStore>,
    flag: [&'a str; 2],
    calls: Cell<usize>,
}

impl CardWriter for Flagging<'_> {
    fn reply(&self, _: &CardRequest, _: &[String]) -> Result<String, String> {
        Err("no cards".into())
    }
    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Err("no cards".into())
    }
    fn retest(&self, _: &studio_understanding::RetestAsk) -> Result<String, String> {
        if self.calls.replace(self.calls.get() + 1) == 0 {
            let mut store = self.store.lock().unwrap();
            for id in self.flag {
                store
                    .quiz_tap(id, studio_understanding::Tap::KeyWrong, T0)
                    .unwrap();
            }
        }
        Ok(retest_reply(false))
    }
}

#[test]
fn a_quiz_flagged_key_wrong_while_the_writer_runs_keeps_no_retest() {
    let dir = tempfile::tempdir().unwrap();
    let store = retests_store(dir.path(), &[("a", T0), ("b", T0), ("c", T0)], &[]);
    // `a` is flagged while its own retest is written, `b` while an earlier one is.
    let writer = Flagging {
        store: &store,
        flag: ["a", "b"],
        calls: Cell::new(0),
    };

    assert_eq!(
        write_retests(&store, &writer, T0 + 9 * DAY, |l| panic!("{l}")),
        1
    );

    assert_eq!(retested(&store), ["c"]);
}
