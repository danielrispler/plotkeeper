//! The card writer (#328 and #369, Testing Decisions): the real `claude -p` command is checked
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
use studio_core::{Base, CardRequest, SummaryRequest};
use studio_understanding::{
    Card, CardStore, CardWriter, Choice, ClaudeWriter, FakeWriter, Quiz, SummarySetup, TurnSummary,
    explain_more, find_claude, write_agent_summary, write_card, write_summary,
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
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        self.calls.set(self.calls.get() + 1);
        if self.calls.get() <= self.fails {
            return Ok("Shall I read the file?".into());
        }
        FakeWriter.reply(request, earlier, brief)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }

    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String> {
        FakeWriter.rewrite(prompts, summaries, diffstat)
    }

    fn update(&self, brief: &str, workers: &str) -> Result<String, String> {
        FakeWriter.update(brief, workers)
    }
}

/// The card `writer` writes on `edit()`, with no earlier card and no brief yet, and the lines it
/// logs.
fn written(writer: &impl CardWriter) -> (Option<Card>, Vec<String>) {
    let mut lines = Vec::new();
    let card = write_card(writer, &edit(), &[], "", |line| lines.push(line));
    (card.map(|(card, _)| sorted(card)), lines)
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
    fn reply(&self, _: &CardRequest, _: &[String], _: &str) -> Result<String, String> {
        Ok(self.0.clone())
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Ok(self.0.clone())
    }

    fn rewrite(&self, _: &[&str], _: &[String], _: &str) -> Result<String, String> {
        Ok(self.0.clone())
    }

    fn update(&self, _: &str, _: &str) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

/// A good reply on `edit()`, in a code fence: the explanation, a quiz on its design, no
/// architecture section, and the brief updated.
const GOOD: &str = concat!(
    "```json\n",
    r#"{"summary": "הגופן של שורת „לא נבדק” גדל ל־`15px`.", "#,
    r#""explanation": "הסוכן הגדיל את `font-size` של `.unchecked` כדי שאפשר יהיה לקרוא את שורת „לא נבדק” בלי מאמץ.", "#,
    r#""paths": ["apps/studio/ui/style.css"], "#,
    r#""code": "-.unchecked { font-size:12px; }\n+.unchecked { font-size:15px; }", "#,
    r#""why_it_matters": "טקסט משני ב־`12px` היה קטן מדי לקריאה בזמן המתנה.", "#,
    r#""not_checked": "לא נבדק אם הכרטיס עדיין נכנס בחלונית צרה.", "#,
    r#""architecture": "structural: none", "#,
    r#""quiz": {"text": "למה השינוי נוגע רק ב־`.unchecked`?", "options": ["#,
    r#"{"text": "כי רק השורה הזו הייתה קטנה מדי לקריאה.", "why": "נכון: שאר הכרטיס כבר היה ב־`15px`."}, "#,
    r#"{"text": "כי `.unchecked` קובע את גודל הגופן של כל הכרטיס.", "why": "לא: הכלל חל רק על השורה עצמה."}, "#,
    r#"{"text": "כי גודל הגופן של הכרטיס בא מהגדרות המערכת.", "why": "לא: הגודל קבוע ב־`style.css`."}"#,
    r#"], "key": 0}, "#,
    r#""brief": "הסשן מתקן את עיצוב הכרטיס.\nשורת „לא נבדק” גדלה ל־`15px`."}"#,
    "\n```\n",
);

/// `GOOD`'s brief.
const BRIEF: &str = "הסשן מתקן את עיצוב הכרטיס.\nשורת „לא נבדק” גדלה ל־`15px`.";

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
        code: "-.unchecked { font-size:12px; }\n+.unchecked { font-size:15px; }".into(),
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
fn a_good_reply_gives_the_card_with_its_quiz_options_key_and_reasons() {
    assert_eq!(written(&Replying(GOOD.into())), (Some(good_card()), vec![]));
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
    let (_, prompt) = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&edit(), &[], "");

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
fn the_card_call_returns_the_brief_updated_and_one_with_none_logs_one_line() {
    let updated = write_card(&Replying(GOOD.into()), &edit(), &[], "", |l| panic!("{l}"));
    let updated = updated.map(|(card, brief)| (sorted(card), brief));
    assert_eq!(updated, Some((good_card(), Some(BRIEF.into()))));

    let no_brief = GOOD.replace(r#""brief""#, r#""other""#);
    let mut lines = Vec::new();
    let written = write_card(&Replying(no_brief), &edit(), &[], "", |l| lines.push(l));

    let written = written.map(|(card, brief)| (sorted(card), brief));
    assert_eq!(written, Some((good_card(), None)));
    let [line] = &lines[..] else {
        panic!("one log line: {lines:?}");
    };
    assert!(line.contains(&edit().id), "{line}");
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
    let earlier = ["`Card` קיבל שדה `summary`.".to_string()];
    let brief = "הסשן מוסיף תקציר לכל סשן.\nהכותב מחזיר את התקציר.";
    let (command, prompt) =
        ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&edit(), &earlier, brief);

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
    // plugins, hooks, skills and CLAUDE.md out (#517, decision 9). The prompt goes on standard input.
    assert_eq!(
        args,
        ["--tools", "", "--strict-mcp-config", "--safe-mode", "-p"]
    );
    assert!(prompt.contains("multiple-choice question on the change's logic or design"));
    // The architecture section reads the prompt's earlier cards; the brief is updated.
    assert!(prompt.contains(&format!("earlier cards, oldest first:\n- {}\n", earlier[0])));
    assert!(prompt.contains(&format!("The brief so far:\n{brief}\n")));
    assert!(prompt.ends_with(&format!("\nChange:\n{}", edit().diff())));
    assert_eq!(command.get_current_dir(), Some(folder));
}

#[test]
fn every_writer_run_is_in_safe_mode() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/card-writer".into());
    let reply = FakeWriter.reply(&edit(), &[], "").unwrap();
    let (card, _) = Card::from_reply(&reply).unwrap();
    let runs = [
        ("card", writer.command(&edit(), &[], "")),
        ("rewrite", writer.rewrite_command(&["a prompt"], &[], "")),
        ("update", writer.update_command("", "workers")),
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
fn an_oversized_rewrite_cuts_the_oldest_prompts_first_and_keeps_every_summary() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    // Three prompts of 30 KiB each, 90 KiB in all, and 200 summaries.
    let prompts: Vec<String> = ["first", "second", "third"]
        .iter()
        .map(|name| format!("{name} {}", "x".repeat(30 << 10)))
        .collect();
    let prompts: Vec<&str> = prompts.iter().map(String::as_str).collect();
    let summaries: Vec<String> = (0..200).map(|i| format!("summary {i}")).collect();
    let diffstat = " core/src/lib.rs | 12 ++++++------\n 1 file changed\n";

    let (_, prompt) = writer.rewrite_command(&prompts, &summaries, diffstat);

    assert!(
        prompt.contains("rewrite from scratch the brief"),
        "{prompt}"
    );
    assert!(!prompt.contains("first x"));
    assert!(prompt.contains("second x") && prompt.contains("third x"));
    assert!(
        summaries
            .iter()
            .all(|s| prompt.contains(&format!("- {s}\n")))
    );
    assert!(prompt.ends_with(diffstat));
    assert!(prompt.len() < 70 << 10, "{}", prompt.len());
}

#[test]
fn a_rewrite_whose_newest_prompt_alone_is_over_the_budget_keeps_its_start() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    // A pasted log of 100 KiB.
    let pasted = format!("Why does this fail?\n{}", "x".repeat(100 << 10));

    let (_, prompt) = writer.rewrite_command(&["Add a", &pasted], &["summary".into()], "");

    assert!(!prompt.contains("Add a"), "{prompt}");
    assert!(
        prompt.contains("<prompt>\nWhy does this fail?\nxxx"),
        "{prompt}"
    );
    assert!(prompt.contains("- summary\n"));
    assert!(prompt.len() < 70 << 10, "{}", prompt.len());
}

#[test]
fn a_rewrite_whose_summaries_fill_the_budget_still_keeps_the_newest_prompts_start() {
    let folder = Path::new("/h/Library/Application Support/dev.falinks.studio/card-writer");
    let writer = ClaudeWriter::new(CLAUDE.into(), folder.into());
    // 2,000 summaries of about 40 bytes each, 80 KiB in all.
    let summaries: Vec<String> = (0..2_000)
        .map(|i| format!("summary {i:04} of a long session's card"))
        .collect();

    let (_, prompt) = writer.rewrite_command(&["Why does this fail?"], &summaries, "");

    assert!(
        prompt.contains("<prompt>\nWhy does this fail?\n"),
        "{}",
        &prompt[..500]
    );
}

#[test]
fn every_prompt_asks_for_english_and_none_for_hebrew() {
    let writer = ClaudeWriter::new(CLAUDE.into(), "/f".into());
    let prompts = [
        writer.command(&edit(), &[], "").1,
        writer.more_command(&edit(), &good_card()).1,
        writer.rewrite_command(&["Add a"], &[], "").1,
        writer.update_command("brief", "workers").1,
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

    let (card_run, _) = writer.command(&edit(), &[], "");
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

    let (_, card) = writer.command(&workers, &[], "");
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
    let (_, prompt) = ClaudeWriter::new(CLAUDE.into(), folder.into()).command(&request, &[], "");

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

/// The writer on a stub `claude` in `dir` that runs `script`, stopped after one second.
fn stub(dir: &Path, script: &str) -> ClaudeWriter {
    let claude = shell(dir, "claude", script);
    ClaudeWriter::new(claude, dir.into()).with_timeout(Duration::from_secs(1))
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
    let scripts = [
        "echo 'Not logged in'; exit 1",
        "cat > /dev/null",
        // A hung call whose own child also hangs.
        "sleep 600 & echo $! > child; sleep 600",
    ];
    for script in scripts {
        let dir = tempfile::tempdir().unwrap();
        let store = stored(dir.path());
        let mut lines = Vec::new();

        let more = explain_more(&store, &stub(dir.path(), script), &edit().id, |l| {
            lines.push(l)
        });

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

    let card = write_card(&writer, &request, &[], "", |l| panic!("{l}"));

    assert!(card.is_some());
    let input = fs::read_to_string(dir.path().join("input")).unwrap();
    assert!(
        input.ends_with("\nChange:\n+++ b/data.bin\n+ab\n"),
        "{input}"
    );
}

// The turn summary (#526; #517, decisions 13 to 15).

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
    }
}

fn setup() -> SummarySetup {
    SummarySetup {
        model: "claude-haiku-4-5".into(),
        effort: "medium".into(),
    }
}

/// A summary reply whose learning section's options are `options`, the first the key.
fn summary_reply(summary: &str, options: [&str; 3]) -> String {
    let option =
        |text: &str| serde_json::json!({ "text": text, "why": format!("Because of {text}.") });
    serde_json::json!({
        "summary": summary,
        "learning": {
            "idea": "A stack matches nested brackets.",
            "question": {
                "text": "Why a stack?",
                "options": options.map(option),
                "key": 0,
            },
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
    // The answer stays out of the options (#517, decision 14).
    assert!(s.contains("Never mark the right option"), "{s}");
    // The question tests understanding, and only when the turn has something to teach (#583).
    for rule in [
        "why something works or how a mechanism behaves",
        "a mistake a developer could really make",
        "keeps its bound or condition",
        r#""learning": null"#,
    ] {
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
    fn reply(&self, r: &CardRequest, e: &[String], b: &str) -> Result<String, String> {
        FakeWriter.reply(r, e, b)
    }
    fn more(&self, r: &CardRequest, c: &Card) -> Result<String, String> {
        FakeWriter.more(r, c)
    }
    fn rewrite(&self, p: &[&str], s: &[String], d: &str) -> Result<String, String> {
        FakeWriter.rewrite(p, s, d)
    }
    fn update(&self, b: &str, w: &str) -> Result<String, String> {
        FakeWriter.update(b, w)
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

#[test]
fn the_summary_input_holds_the_sessions_last_3_summaries_and_their_questions() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    for n in 1..=4 {
        let writer = recording(summary_reply(&format!("Summary {n}."), ["a", "b", "c"]));
        write_summary(&store, &writer, &setup(), turn("s", n), |_| {}).unwrap();
    }
    // Another session's summary is not this session's.
    let other = recording(summary_reply("Other session.", ["a", "b", "c"]));
    write_summary(&store, &other, &setup(), turn("t", 5), |_| {}).unwrap();

    let writer = recording(summary_reply("Summary 6.", ["a", "b", "c"]));
    write_summary(&store, &writer, &setup(), turn("s", 6), |_| {}).unwrap();

    let input = writer.inputs.lock().unwrap().pop().unwrap();
    assert!(!input.contains("Summary 1."), "{input}");
    assert!(!input.contains("Other session."), "{input}");
    for n in 2..=4 {
        assert!(input.contains(&format!("Summary {n}.")), "{input}");
    }
    assert_eq!(input.matches("Why a stack?").count(), 3, "{input}");
    let kept = store.lock().unwrap().turn_summary("s@6").cloned();
    assert_eq!(kept.unwrap().summary.summary, "Summary 6.");
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
fn a_summary_reply_gives_the_summary_and_the_learning_section() {
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
    let learning = summary.learning.expect("a learning section");
    assert_eq!(learning.idea, "A stack matches nested brackets.");
    assert_eq!(learning.question.text, "Why a stack?");
    let key = &learning.question.options[learning.question.key];
    assert_eq!(key.text, "It matches nesting.");
}

#[test]
fn a_reply_with_a_null_learning_section_gives_the_summary_alone_and_logs_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(r#"{"summary": "Merged the lanes.", "learning": null}"#.into());
    let mut lines = Vec::new();

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

    assert_eq!(summary.unwrap().learning, None);
    assert_eq!(lines, Vec::<String>::new());
    assert_eq!(writer.inputs.lock().unwrap().len(), 1, "no retry");
}

#[test]
fn a_reply_whose_key_is_much_longer_than_its_wrong_options_is_asked_once_more() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    // The lengths of a real guessable reply (#583): a key of 100 characters, the longest wrong 77.
    let writer = recording(summary_reply(
        "Rewrote the parser.",
        [&"k".repeat(100), &"w".repeat(77), &"w".repeat(66)],
    ));
    let mut lines = Vec::new();

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

    let summary = summary.unwrap();
    assert_eq!(summary.summary, "Rewrote the parser.");
    assert_eq!(summary.learning, None);
    assert_eq!(writer.inputs.lock().unwrap().len(), 2, "asked once more");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("s@1"), "{lines:?}");
}

#[test]
fn a_reply_whose_options_are_of_similar_length_passes_on_the_first_call() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    // A real reply whose key is the shortest (#583): 97 characters, the wrong ones up to 114.
    let writer = recording(summary_reply(
        "Rewrote the parser.",
        [&"k".repeat(97), &"w".repeat(114), &"w".repeat(101)],
    ));

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |_| {}).unwrap();

    assert!(summary.learning.is_some());
    assert_eq!(writer.inputs.lock().unwrap().len(), 1);
}

#[test]
fn a_reply_with_no_learning_field_gives_the_summary_alone_and_logs_why() {
    let dir = tempfile::tempdir().unwrap();
    let store = summaries(dir.path());
    let writer = recording(r#"{"summary": "Rewrote the parser."}"#.into());
    let mut lines = Vec::new();

    let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |l| lines.push(l));

    assert_eq!(summary.unwrap().learning, None);
    assert_eq!(lines.len(), 1, "only an explicit null is silent: {lines:?}");
    assert_eq!(writer.inputs.lock().unwrap().len(), 1, "no retry");
}

#[test]
fn a_short_key_a_little_longer_than_its_wrong_options_passes_on_the_first_call() {
    for options in [
        ["O(n log n)", "O(n)", "O(1)"],
        ["Yes, it blocks", "No", "Only on Linux"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let store = summaries(dir.path());
        let writer = recording(summary_reply("Rewrote the parser.", options));

        let summary = write_summary(&store, &writer, &setup(), turn("s", 1), |_| {}).unwrap();

        assert!(summary.learning.is_some(), "{options:?}");
        assert_eq!(writer.inputs.lock().unwrap().len(), 1, "{options:?}");
    }
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

    let options = summary.learning.unwrap().question.options.into_iter();
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
    let request = SummaryRequest {
        card: Some("s@1".into()),
        ..turn("s", 1)
    };

    let summary = write_summary(&store, &writer, &setup(), request, |_| {}).unwrap();

    assert_eq!(summary.summary, "Rewrote the parser.");
    assert_eq!(summary.learning, None);
    let input = writer.inputs.lock().unwrap().pop().unwrap();
    assert!(input.contains("has a card"), "{input}");
}

// The subagent summary, written when Daniel opens a subagent (#535).

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
