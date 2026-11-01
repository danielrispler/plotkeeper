//! Seam 1 (#328, Testing Decisions) for checks: replay hook events through the core and check the
//! session's check lines (#369, decision 14). The fixture follows the real payload shapes of the
//! other fixtures; a check that fails comes as `PostToolUseFailure` with its exit code in `error`.

use studio_core::{CheckLine, Output, Studio};

const CHECKS: &str = include_str!("fixtures/checks.jsonl");
const SESSION: &str = "a3e6c9f2-1b4d-4e7a-8c0f-5d2b9e6a3c18";
const COMMAND: &str = "cd apps/studio && cargo test";

/// The outputs of each line when the log arrives live, but for the first `Stop`'s ask for Claude
/// Code's version (`wake_ups.rs`).
fn live(log: &str) -> Vec<Vec<Output>> {
    let mut studio = Studio::default();
    let mut outputs = |line| {
        let mut outputs = studio.apply(line);
        outputs.retain(|output| !matches!(output, Output::Version { .. }));
        outputs
    };
    log.lines().map(&mut outputs).collect()
}

/// The session's check lines once `log` arrived live.
fn lines(log: &str) -> Vec<CheckLine> {
    let mut studio = Studio::default();
    log.lines().for_each(|line| drop(studio.apply(line)));
    studio
        .session(SESSION)
        .map_or_else(Vec::new, |s| s.checks.clone())
}

/// Before #375 the wait's first check asked "Will it pass?" (#343).
#[test]
fn a_check_that_passes_or_fails_shows_as_a_line_and_raises_no_guess_or_card() {
    let line = |passed| CheckLine {
        command: COMMAND.into(),
        passed,
    };

    assert_eq!(
        lines(CHECKS),
        [line(Some(true)), line(Some(true)), line(Some(false))]
    );
    assert_eq!(live(CHECKS).concat(), []);
}

/// The first wait, a passing check, with its command replaced by `command`, as JSON spells it.
fn first_wait(command: &str) -> String {
    let lines: Vec<&str> = CHECKS.lines().take(5).collect();
    lines.join("\n").replace(COMMAND, command)
}

#[test]
fn the_exit_code_counts_only_when_nothing_after_the_check_can_mask_it() {
    let passed = |command| {
        let [line] = &lines(&first_wait(command))[..] else {
            panic!("one check line on `{command}`");
        };
        line.passed
    };

    assert_eq!(passed("cargo test"), Some(true));
    assert_eq!(passed("set -o pipefail; cargo test | tail"), Some(true));
    // `tail` exits 0 even when the tests fail, so the line says only that it ran.
    assert_eq!(passed("cargo test 2>&1 | tail -5"), None);
    // So do `true` after `||`, `echo` after `;` and a later line (JSON-escaped here).
    assert_eq!(passed("cargo test || true"), None);
    assert_eq!(passed("cargo test; echo $?"), None);
    assert_eq!(passed(r"cargo test\necho done"), None);
    // `pipefail` covers the pipe, not a later statement, and only when a `set` turns it on.
    assert_eq!(
        passed(r"set -euo pipefail\ncargo test 2>&1 | tail -20"),
        Some(true)
    );
    assert_eq!(passed("set -o pipefail && cargo test | tail"), Some(true));
    assert_eq!(
        passed("set -o pipefail; cargo test | tail; echo done"),
        None
    );
    assert_eq!(passed("cargo test pipefail_cases | tail"), None);
    // `!` flips the code, and `&` runs the check in the background.
    assert_eq!(passed("! cargo test"), None);
    assert_eq!(passed("cargo test & wait"), None);
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
fn a_check_that_did_not_run_gets_no_line() {
    let wait = first_wait_lines(|post| {
        post["hook_event_name"] = "PostToolUseFailure".into();
        post["error"] = "The user doesn't want to proceed with this tool use.".into();
    });
    let passed = CHECKS.lines().nth(3).unwrap();
    // The prompt, the denied check, then the agent runs the same check again and it passes.
    let [prompt, start, denied] = [&wait[1], &wait[2], &wait[3]].map(String::as_str);
    let log = [prompt, start, denied, start, passed].join("\n");

    let [line] = &lines(&log)[..] else {
        panic!("one check line");
    };
    assert_eq!(line.passed, Some(true));
}

#[test]
fn a_check_whose_result_lands_before_its_start_gets_its_line() {
    let wait: Vec<&str> = CHECKS.lines().collect();
    // The hooks are async: the result, then the start.
    let log = [wait[1], wait[3], wait[2], wait[4]].join("\n");

    let passed: Vec<Option<bool>> = lines(&log).into_iter().map(|l| l.passed).collect();
    assert_eq!(passed, [Some(true)]);
}

#[test]
fn a_check_run_in_the_background_is_no_check() {
    let log = first_wait(COMMAND).replace(
        &format!(r#""command":"{COMMAND}""#),
        &format!(r#""command":"{COMMAND}","run_in_background":true"#),
    );

    assert_eq!(log.matches("run_in_background").count(), 2);
    assert_eq!(lines(&log), []);
}

#[test]
fn the_check_list_names_which_commands_are_checks() {
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
        assert_eq!(lines(&first_wait(command)).len(), 1, "{command}");
    }
    for command in not_checks {
        assert_eq!(lines(&first_wait(command)), [], "{command}");
    }
}
