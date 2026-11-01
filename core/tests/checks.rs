//! Seam 1 (#328, Testing Decisions) for checks and guesses (#343): replay hook events through the core
//! and check the guess prompts and the session's check lines (#369, decision 14). The fixture
//! follows the real payload shapes of the other fixtures; a check that fails comes as
//! `PostToolUseFailure` with its exit code in `error`.

use studio_core::{Check, CheckLine, GuessPrompt, Output, Step, Studio};

const CHECKS: &str = include_str!("fixtures/checks.jsonl");
const SESSION: &str = "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18";
const COMMAND: &str = "cd apps/studio && cargo test";

/// The outputs of each line when the log arrives live.
fn live(log: &str) -> Vec<Vec<Output>> {
    let mut studio = Studio::default();
    log.lines().map(|line| studio.apply(line)).collect()
}

fn prompt(tool_use_id: &str) -> GuessPrompt {
    GuessPrompt {
        session_id: SESSION.into(),
        tool_use_id: tool_use_id.into(),
        step: Step {
            tool: "Bash".into(),
            detail: Some(COMMAND.into()),
        },
        guess: None,
    }
}

/// Before #374 an edit before the check took the wait's card step, and with it the guess.
#[test]
fn the_waits_first_check_raises_the_guess_prompt_even_after_an_edit() {
    let prompts: Vec<(usize, GuessPrompt)> = live(CHECKS)
        .into_iter()
        .enumerate()
        .flat_map(|(line, outputs)| outputs.into_iter().map(move |output| (line, output)))
        .filter_map(|(line, output)| match output {
            Output::Guess(prompt) => Some((line, prompt)),
            _ => None,
        })
        .collect();

    // Lines 2, 8 and 12 are the checks' `PreToolUse`; the second wait's check follows its Edit.
    assert_eq!(
        prompts,
        [
            (2, prompt("toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W")),
            (8, prompt("toolu_01Zp7Xc4Fr1Mg8Hn5Qw2Ej6T")),
            (12, prompt("toolu_01Uy4Ka9Lo3Pv6Bs1Dc8Fh5R")),
        ]
    );
}

/// The session's check lines once `log` arrived live.
fn lines(log: &str) -> Vec<CheckLine> {
    let mut studio = Studio::default();
    log.lines().for_each(|line| drop(studio.apply(line)));
    studio
        .session(SESSION)
        .map_or_else(Vec::new, |s| s.checks.clone())
}

#[test]
fn a_check_that_passes_or_fails_shows_as_a_line_and_gets_no_card() {
    let line = |exit_passed| CheckLine {
        command: COMMAND.into(),
        check: Check {
            guess: None,
            exit_passed,
        },
    };

    assert_eq!(
        lines(CHECKS),
        [line(Some(true)), line(Some(true)), line(Some(false))]
    );
    let cards = live(CHECKS).into_iter().flatten();
    assert_eq!(cards.filter(|o| matches!(o, Output::Card(_))).count(), 0);
}

#[test]
fn a_tap_before_the_result_is_the_guess_and_none_is_recorded_without_one() {
    let check = "toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W";
    // Daniel taps yes after line `tap_after`, or never; the guess on the first check's line.
    let guess = |tap_after: Option<usize>| {
        let mut studio = Studio::default();
        for (line, event) in CHECKS.lines().take(5).enumerate() {
            studio.apply(event);
            if tap_after == Some(line) {
                studio.guess(check, true);
            }
        }
        let [line] = &studio.session(SESSION).unwrap().checks[..] else {
            panic!("one check line");
        };
        line.check.guess
    };

    assert_eq!(guess(Some(2)), Some(true));
    assert_eq!(guess(None), None);
    assert_eq!(guess(Some(3)), None);
}

#[test]
fn the_session_shows_the_prompt_and_its_tap_until_the_result_arrives() {
    let lines: Vec<&str> = CHECKS.lines().collect();
    let mut studio = Studio::default();
    let open = |studio: &Studio| studio.session(SESSION).unwrap().guess.clone();
    lines[..3].iter().for_each(|line| studio.replay(line));
    assert_eq!(
        open(&studio),
        Some(prompt("toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W"))
    );

    studio.guess("toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W", false);
    let tapped = open(&studio).expect("still open");
    assert_eq!(tapped.guess, Some(false));

    // A restart mid-check keeps the prompt; the result, live, closes it and keeps the tap.
    studio.apply(lines[3]);
    assert_eq!(open(&studio), None);
    let checks = &studio.session(SESSION).unwrap().checks;
    assert_eq!(checks[0].check.guess, Some(false));
}

#[test]
fn a_prompt_whose_result_never_comes_closes_when_the_turn_ends() {
    let lines: Vec<&str> = CHECKS.lines().collect();
    let mut studio = Studio::default();

    for line in [lines[1], lines[2], lines[4]] {
        studio.replay(line);
    }

    assert_eq!(studio.session(SESSION).unwrap().guess, None);
}

/// The first wait, a passing check, with its command replaced by `command`, as JSON spells it.
fn first_wait(command: &str) -> String {
    let lines: Vec<&str> = CHECKS.lines().take(5).collect();
    lines.join("\n").replace(COMMAND, command)
}

#[test]
fn the_exit_code_counts_only_when_nothing_after_the_check_can_mask_it() {
    let exit_passed = |command| {
        let [line] = &lines(&first_wait(command))[..] else {
            panic!("one check line on `{command}`");
        };
        line.check.exit_passed
    };

    assert_eq!(exit_passed("cargo test"), Some(true));
    assert_eq!(
        exit_passed("set -o pipefail; cargo test | tail"),
        Some(true)
    );
    // `tail` exits 0 even when the tests fail, so only the card writer can tell.
    assert_eq!(exit_passed("cargo test 2>&1 | tail -5"), None);
    // So do `true` after `||`, `echo` after `;` and a later line (JSON-escaped here).
    assert_eq!(exit_passed("cargo test || true"), None);
    assert_eq!(exit_passed("cargo test; echo $?"), None);
    assert_eq!(exit_passed(r"cargo test\necho done"), None);
    // `pipefail` covers the pipe, not a later statement, and only when a `set` turns it on.
    assert_eq!(
        exit_passed(r"set -euo pipefail\ncargo test 2>&1 | tail -20"),
        Some(true)
    );
    assert_eq!(
        exit_passed("set -o pipefail && cargo test | tail"),
        Some(true)
    );
    assert_eq!(
        exit_passed("set -o pipefail; cargo test | tail; echo done"),
        None
    );
    assert_eq!(exit_passed("cargo test pipefail_cases | tail"), None);
    // `!` flips the code, and `&` runs the check in the background.
    assert_eq!(exit_passed("! cargo test"), None);
    assert_eq!(exit_passed("cargo test & wait"), None);
}

/// The first wait's lines, with its check's result changed by `result`.
fn first_wait_lines(result: impl Fn(&mut serde_json::Value)) -> Vec<String> {
    let mut lines: Vec<String> = CHECKS.lines().take(5).map(Into::into).collect();
    let mut post: serde_json::Value = serde_json::from_str(&lines[3]).unwrap();
    result(&mut post);
    lines[3] = post.to_string();
    lines
}

#[test]
fn a_check_that_did_not_run_gets_no_line_and_leaves_the_guess_open() {
    let wait = first_wait_lines(|post| {
        post["hook_event_name"] = "PostToolUseFailure".into();
        post["error"] = "The user doesn't want to proceed with this tool use.".into();
    });
    let passed = CHECKS.lines().nth(3).unwrap();
    // The prompt, the denied check, then the agent runs the same check again and it passes.
    let [prompt, start, denied] = [&wait[1], &wait[2], &wait[3]].map(String::as_str);
    let log = [prompt, start, denied, start, passed].join("\n");
    let outputs: Vec<Output> = live(&log).into_iter().flatten().collect();

    let [Output::Guess(_), Output::Guess(_)] = &outputs[..] else {
        panic!("two prompts: {outputs:?}");
    };
    let [line] = &lines(&log)[..] else {
        panic!("one check line");
    };
    assert_eq!(line.check.exit_passed, Some(true));
}

#[test]
fn a_check_whose_result_lands_before_its_start_gets_its_line_and_no_prompt() {
    let wait: Vec<&str> = CHECKS.lines().collect();
    // The hooks are async: the result, then the start.
    let log = [wait[1], wait[3], wait[2], wait[4]].join("\n");

    assert_eq!(live(&log).concat(), []);
    let untapped = Check {
        guess: None,
        exit_passed: Some(true),
    };
    let checks: Vec<Check> = lines(&log).into_iter().map(|l| l.check).collect();
    assert_eq!(checks, [untapped]);
}

#[test]
fn a_check_run_in_the_background_is_no_check() {
    let log = first_wait(COMMAND).replace(
        &format!(r#""command":"{COMMAND}""#),
        &format!(r#""command":"{COMMAND}","run_in_background":true"#),
    );

    assert_eq!(log.matches("run_in_background").count(), 2);
    assert_eq!(live(&log).concat(), []);
    assert_eq!(lines(&log), []);
}

#[test]
fn the_check_list_names_which_commands_are_checks() {
    let prompts = |command| {
        let outputs = live(&first_wait(command)).into_iter().flatten();
        outputs.filter(|o| matches!(o, Output::Guess(_))).count()
    };
    let checks = [
        "cargo nextest run --workspace",
        "cargo clippy --workspace --all-targets",
        "cargo check",
        "cargo build --release",
        "cargo run --bin ci-checks -- --all",
        "cargo +nightly test",
        "cargo -q clippy --workspace",
        "cargo --locked build",
        "cd tests/go && go vet ./...",
        "RUST_BACKTRACE=1 go test ./... 2>&1 | tail -20",
    ];
    let not_checks = [
        "cargo fmt",
        "cargo run --bin wayfinder-startup -- 266",
        "git log --oneline -3",
        r#"gh pr create --title \"Guess\" --body \"Tests: cargo test passes.\""#,
    ];

    for command in checks {
        assert_eq!(prompts(command), 1, "{command}");
    }
    for command in not_checks {
        assert_eq!(prompts(command), 0, "{command}");
    }
}
