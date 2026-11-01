//! Seam 1 (#369, Testing Decisions) for cards (#369, decisions 10 and 11): a card covers a prompt's
//! whole change, at its `Stop`. The shell fingerprints the worktree where the core asks it to and
//! passes in what changed; these tests play the shell's part.

use studio_core::{Base, CardRequest, Change, Output, Studio};

const LOG: &str = include_str!("fixtures/bash-steps.jsonl");
const SESSION: &str = "e4a7c2f9-3b6d-4e1a-8c5f-2d9b7e0a6c13";
const WORKTREE: &str = "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-344-bash";

/// What the fingerprint finds at each line it is taken at: the prompt's own `cargo fmt` and note.
fn found() -> Change {
    Change {
        base: Base {
            head: Some("5f0c1e2d".into()),
            files: vec![],
        },
        diff: "--- a/apps/studio/NOTES.md\n+++ b/apps/studio/NOTES.md\n@@ -0,0 +1 @@\n+live\n"
            .into(),
    }
}

/// The card requests when every line of `log` arrives live and each fingerprint finds `found`.
fn cards(log: &str, found: Option<Change>) -> Vec<CardRequest> {
    let mut studio = Studio::default();
    let lines = log.lines();
    let outputs = lines.flat_map(|line| {
        let change = studio.fingerprint_folder(line).and(found.clone());
        studio.apply_with_change(line, change)
    });
    let outputs: Vec<Output> = outputs.collect();
    outputs
        .into_iter()
        .filter_map(|output| match output {
            Output::Card(request) => Some(request),
            _ => None,
        })
        .collect()
}

#[test]
fn a_prompt_whose_worktree_changed_gets_one_card_on_the_whole_change_at_its_stop() {
    assert_eq!(
        cards(LOG, Some(found())),
        [CardRequest {
            session_id: SESSION.into(),
            id: format!("{SESSION}@1793265021380"),
            base: found().base,
            change: found().diff,
            check: None,
        }]
    );
}

#[test]
fn a_prompt_that_changed_nothing_gets_no_card() {
    assert_eq!(cards(LOG, None), []);
}

/// The lines of `log` the shell fingerprints, by index.
fn fingerprinted(log: &str) -> Vec<usize> {
    let mut studio = Studio::default();
    let lines = log.lines().enumerate();
    let at = lines.filter(|(_, line)| {
        let at = studio.fingerprint_folder(line).is_some();
        studio.apply(line);
        at
    });
    at.map(|(index, _)| index).collect()
}

#[test]
fn the_shell_fingerprints_at_the_prompt_and_at_the_stop() {
    assert_eq!(fingerprinted(LOG), [0, 9]);
    assert_eq!(
        Studio::default().fingerprint_folder(LOG.lines().next().unwrap()),
        Some((SESSION.into(), WORKTREE.into()))
    );
}

/// An interrupt fires no `Stop`, so the next prompt lands inside the open turn: it takes no new
/// base, and the `Stop` covers both prompts' change.
#[test]
fn a_prompt_after_an_interrupt_keeps_the_interrupted_prompts_base() {
    let log = include_str!("fixtures/interrupt-then-prompt.jsonl");

    assert_eq!(fingerprinted(log), [0, 8]);
}

#[test]
fn after_a_replay_the_shell_fingerprints_the_prompts_still_open() {
    let waiting = |replayed: usize| -> Vec<(String, String)> {
        let mut studio = Studio::default();
        LOG.lines()
            .take(replayed)
            .for_each(|line| studio.replay(line));
        let open = studio.waiting_folders();
        open.map(|(id, folder)| (id.into(), folder.into()))
            .collect()
    };

    assert_eq!(waiting(7), [(SESSION.into(), WORKTREE.into())]);
    assert_eq!(waiting(LOG.lines().count()), []);
}
