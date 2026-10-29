//! Seam 1 (#328, Testing Decisions): Bash steps as card steps. The shell fingerprints the worktree
//! where the core asks it to and passes in what changed; these tests play the shell's part.

use studio_core::{CardRequest, Output, Step, Studio};

const LOG: &str = include_str!("fixtures/bash-steps.jsonl");
const REVERT: &str = "toolu_01Rv3Gt5Hy7Ju9Ki1Lo3Pa5S";
const FORMAT: &str = "toolu_01Qw2Er4Ty6Ui8Op0As2Df4G";
const NOTE: &str = "toolu_01Zx3Cv5Bn7Mq9Wa1Se3Rd5F";

const FORMAT_DIFF: &str = concat!(
    "--- a/apps/studio/core/src/lib.rs\n",
    "+++ b/apps/studio/core/src/lib.rs\n",
    "@@ -90,3 +90,3 @@\n",
    "     let plan_file = input[\"file_path\"]\n",
    "-        .as_str().is_some_and(|path| path.contains(\"/.claude/plans/\"));\n",
    "+        .as_str()\n",
);

/// The card requests after an app start that replays the first `replayed` lines of the log and
/// gets the rest live. The shell's fingerprint finds a change only after the steps in `changed`,
/// by tool use id, with their diffs.
fn restart_after(replayed: usize, changed: &[(&str, &str)]) -> Vec<CardRequest> {
    let lines: Vec<&str> = LOG.lines().collect();
    let mut studio = Studio::default();
    for line in &lines[..replayed] {
        studio.replay(line);
    }
    lines[replayed..]
        .iter()
        .flat_map(|line| {
            let fingerprinted = studio.fingerprint_folder(line).is_some();
            let change = changed
                .iter()
                .find(|(id, _)| fingerprinted && line.contains(id))
                .map(|(_, diff)| diff.to_string());
            studio.apply_with_change(line, change)
        })
        .filter_map(|output| match output {
            Output::Card(request) => Some(request),
            _ => None,
        })
        .collect()
}

fn live(changed: &[(&str, &str)]) -> Vec<CardRequest> {
    restart_after(0, changed)
}

#[test]
fn the_waits_first_bash_step_that_changed_the_worktree_gets_its_card() {
    let note = "--- a/apps/studio/NOTES.md\n+++ b/apps/studio/NOTES.md\n@@ -0,0 +1 @@\n+live\n";

    let cards = live(&[(FORMAT, FORMAT_DIFF), (NOTE, note)]);

    assert_eq!(
        cards,
        [CardRequest {
            session_id: "e4a7c2f9-3b6d-4e1a-8c5f-2d9b7e0a6c13".into(),
            tool_use_id: FORMAT.into(),
            step: Step {
                tool: "Bash".into(),
                detail: Some("cargo fmt --manifest-path apps/studio/Cargo.toml".into()),
            },
            change: FORMAT_DIFF.into(),
            check: None,
        }]
    );
}

#[test]
fn bash_steps_that_changed_nothing_get_no_card() {
    assert_eq!(live(&[]), []);
}

#[test]
fn a_git_step_gets_no_card_even_when_it_changed_the_worktree() {
    let revert = "--- a/apps/studio/core/src/lib.rs\n+++ b/apps/studio/core/src/lib.rs\n";

    assert_eq!(live(&[(REVERT, revert)]), []);
}

#[test]
fn a_replayed_bash_step_requests_no_card() {
    let carded = |replayed| restart_after(replayed, &[(FORMAT, FORMAT_DIFF)]);

    assert_eq!(carded(LOG.lines().count()), []);
    assert_eq!(carded(7), []);
    let cards = carded(5);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].tool_use_id, FORMAT);
}

#[test]
fn a_restart_after_the_waits_carded_bash_step_gives_the_wait_no_second_card() {
    let lines: Vec<&str> = LOG.lines().collect();
    // A successful Edit, moved into this wait.
    let edit: Vec<String> = include_str!("fixtures/first-edit.jsonl")
        .lines()
        .skip(5)
        .take(2)
        .map(|line| {
            line.replace(
                "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092",
                "e4a7c2f9-3b6d-4e1a-8c5f-2d9b7e0a6c13",
            )
        })
        .collect();
    // Replays the wait up to its `cargo fmt`'s result, then gets the Edit live.
    let edit_cards = |stored: &[&str]| {
        let mut studio = Studio::default();
        studio.set_carded_steps(stored.iter().map(|id| id.to_string()));
        lines[..7].iter().for_each(|line| studio.replay(line));
        let outputs = edit.iter().flat_map(|line| studio.apply(line));
        outputs.filter(|o| matches!(o, Output::Card(_))).count()
    };

    // A replayed Bash step gets no fingerprint; the card store says it had its card.
    assert_eq!(edit_cards(&[FORMAT]), 0);
    assert_eq!(edit_cards(&[]), 1);
}

#[test]
fn a_failed_bash_step_that_changed_the_worktree_gets_its_card() {
    let log = include_str!("fixtures/failing-bash.jsonl").replace("ls does-not-exist", "sh gen.sh");
    // The script wrote half its output before it failed.
    let half = "--- a/apps/studio/NOTES.md\n+++ b/apps/studio/NOTES.md\n@@ -0,0 +1 @@\n+half\n";
    let mut studio = Studio::default();

    let outputs: Vec<Output> = log
        .lines()
        .flat_map(|line| {
            let failed = line.contains(r#""PostToolUseFailure""#);
            studio.apply_with_change(line, failed.then(|| half.into()))
        })
        .collect();

    let [Output::Card(card)] = &outputs[..] else {
        panic!("one card: {outputs:?}");
    };
    assert_eq!(card.tool_use_id, "toolu_01FpQ7wXk2Lr9sV3bN6mT4cY");
    assert_eq!(card.step.detail.as_deref(), Some("sh gen.sh"));
    assert_eq!(card.change, half);
}

/// The lines of `log` the shell fingerprints, when the fingerprint finds a change only after the
/// step `changed`.
fn fingerprinted(log: &str, changed: &str) -> Vec<usize> {
    let mut studio = Studio::default();
    let lines = log.lines().enumerate();
    let at = lines.filter(|(_, line)| {
        let at = studio.fingerprint_folder(line).is_some();
        let change = (at && line.contains(changed)).then(|| FORMAT_DIFF.into());
        studio.apply_with_change(line, change);
        at
    });
    at.map(|(index, _)| index).collect()
}

const WORKTREE: &str = "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-344-bash";

#[test]
fn the_shell_fingerprints_at_each_prompt_and_each_steps_end_until_the_card_step() {
    assert_eq!(fingerprinted(LOG, "no step"), [0, 2, 4, 6, 8]);
    assert_eq!(fingerprinted(LOG, FORMAT), [0, 2, 4, 6]);
    assert_eq!(
        fingerprinted(include_str!("fixtures/failing-bash.jsonl"), "no step"),
        [1, 3]
    );
    assert_eq!(
        Studio::default()
            .fingerprint_folder(LOG.lines().next().unwrap())
            .as_deref(),
        Some(WORKTREE)
    );
}

#[test]
fn after_a_replay_the_shell_fingerprints_the_waits_still_open() {
    let waiting = |replayed: usize| -> Vec<String> {
        let mut studio = Studio::default();
        LOG.lines()
            .take(replayed)
            .for_each(|line| studio.replay(line));
        studio.waiting_folders().map(Into::into).collect()
    };

    assert_eq!(waiting(7), [WORKTREE]);
    assert_eq!(waiting(LOG.lines().count()), [] as [&str; 0]);
}
