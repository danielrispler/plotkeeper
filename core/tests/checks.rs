//! Seam 1 (#328, Testing Decisions) for checks and guesses (#343): replay hook events through the core
//! and check the guess prompts and card requests. The fixture follows the real payload shapes of the
//! other fixtures; a check that fails comes as `PostToolUseFailure` with its exit code in `error`.

use studio_core::{CardRequest, Check, GuessPrompt, Output, Step, Studio};

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

#[test]
fn a_check_as_the_waits_first_card_step_raises_the_guess_prompt_and_one_after_an_edit_does_not() {
    let prompts: Vec<(usize, GuessPrompt)> = live(CHECKS)
        .into_iter()
        .enumerate()
        .flat_map(|(line, outputs)| outputs.into_iter().map(move |output| (line, output)))
        .filter_map(|(line, output)| match output {
            Output::Guess(prompt) => Some((line, prompt)),
            _ => None,
        })
        .collect();

    // Line 2 and line 12 are the checks' `PreToolUse`; the second wait's check follows its Edit.
    assert_eq!(
        prompts,
        [
            (2, prompt("toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W")),
            (12, prompt("toolu_01Uy4Ka9Lo3Pv6Bs1Dc8Fh5R")),
        ]
    );
}

fn cards(log: &str) -> Vec<CardRequest> {
    let outputs = live(log).into_iter().flatten();
    outputs
        .filter_map(|output| match output {
            Output::Card(request) => Some(request),
            _ => None,
        })
        .collect()
}

#[test]
fn a_check_that_passes_or_fails_gets_a_card_on_its_command_and_result() {
    let checks: Vec<(String, String, Option<Check>)> = cards(CHECKS)
        .into_iter()
        .map(|card| (card.tool_use_id, card.change, card.check))
        .collect();

    let passed = concat!(
        "running 12 tests\n",
        "test a_waits_first_edit_gets_a_card_and_its_later_edits_do_not ... ok\n\n",
        "test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n",
        "    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.31s",
    );
    let failed = concat!(
        "Exit code 101\nrunning 12 tests\ntest a_check_gets_a_card ... FAILED\n\n",
        "test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\n",
        "error: test failed, to rerun pass `-p studio-core --test checks`",
    );
    let edit = "@@ -34,1 +34,1 @@\n-.guess { font-size: 15px; }\n+.guess { font-size: 17px; }\n";
    let untapped = |exit_passed| {
        Some(Check {
            guess: None,
            exit_passed,
        })
    };
    assert_eq!(
        checks,
        [
            (
                "toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W".into(),
                passed.into(),
                untapped(Some(true))
            ),
            ("toolu_01Hm3Jd6Wq9Tf2Ks5Ly8Bv1N".into(), edit.into(), None),
            (
                "toolu_01Uy4Ka9Lo3Pv6Bs1Dc8Fh5R".into(),
                failed.into(),
                untapped(Some(false))
            ),
        ]
    );
}

#[test]
fn a_tap_before_the_result_is_the_guess_and_none_is_recorded_without_one() {
    let check = "toolu_01Rt5Gb8Nc2Vk7Lp4Xs9Dq3W";
    // Daniel taps yes after line `tap_after`, or never; the guess on the first check's card.
    let guess = |tap_after: Option<usize>| {
        let mut studio = Studio::default();
        let mut cards = Vec::new();
        for (line, event) in CHECKS.lines().take(5).enumerate() {
            cards.extend(studio.apply(event).into_iter().filter_map(|o| match o {
                Output::Card(request) => request.check,
                _ => None,
            }));
            if tap_after == Some(line) {
                studio.guess(check, true);
            }
        }
        let [card] = &cards[..] else {
            panic!("one card");
        };
        card.guess
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

    // A restart mid-check keeps the prompt; the result, live, closes it and still gets the card.
    let outputs = studio.apply(lines[3]);
    assert_eq!(open(&studio), None);
    assert!(matches!(&outputs[..], [Output::Card(_)]), "{outputs:?}");
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
        let [card] = &cards(&first_wait(command))[..] else {
            panic!("one card on `{command}`");
        };
        card.check.as_ref().expect("a check").exit_passed
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
fn a_check_that_did_not_run_gets_no_card_and_leaves_the_card_step_open() {
    let lines = first_wait_lines(|post| {
        post["hook_event_name"] = "PostToolUseFailure".into();
        post["error"] = "The user doesn't want to proceed with this tool use.".into();
    });
    let passed = CHECKS.lines().nth(3).unwrap();
    // The prompt, the denied check, then the agent runs the same check again and it passes.
    let [prompt, start, denied] = [&lines[1], &lines[2], &lines[3]].map(String::as_str);
    let log = [prompt, start, denied, start, passed].join("\n");
    let outputs: Vec<Output> = live(&log).into_iter().flatten().collect();

    let [Output::Guess(_), Output::Guess(_), Output::Card(card)] = &outputs[..] else {
        panic!("two prompts, then one card: {outputs:?}");
    };
    assert_eq!(card.check.as_ref().unwrap().exit_passed, Some(true));
}

#[test]
fn a_check_whose_result_lands_before_its_start_gets_its_card_and_no_prompt() {
    let lines: Vec<&str> = CHECKS.lines().collect();
    // The hooks are async: the result, then the start.
    let log = [lines[1], lines[3], lines[2], lines[4]].join("\n");
    let outputs: Vec<Output> = live(&log).into_iter().flatten().collect();

    let [Output::Card(card)] = &outputs[..] else {
        panic!("one card and no prompt: {outputs:?}");
    };
    let untapped = Check {
        guess: None,
        exit_passed: Some(true),
    };
    assert_eq!(card.check, Some(untapped));
}

#[test]
fn a_check_run_in_the_background_is_no_check() {
    let log = first_wait(COMMAND).replace(
        &format!(r#""command":"{COMMAND}""#),
        &format!(r#""command":"{COMMAND}","run_in_background":true"#),
    );

    assert_eq!(log.matches("run_in_background").count(), 2);
    assert_eq!(live(&log).concat(), []);
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
