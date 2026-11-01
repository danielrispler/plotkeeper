//! Seam 1 (#369, Testing Decisions) for cards (#369, decisions 10 and 11): a card covers a prompt's
//! whole change, at its `Stop`. The shell fingerprints the worktree where the core asks it to and
//! passes in what changed; these tests play the shell's part.

use serde_json::{Value, json};
use studio_core::{Base, CardRequest, Change, Output, Stopped, Studio};

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
            prompt: Some(1793265000140),
        }]
    );
}

/// The `Stop`'s first line, and the rest of its last message.
const LAST_MESSAGE: &str =
    "Dropped the stray edit, formatted the core and wrote the note in `apps/studio/NOTES.md`.";

#[test]
fn a_stop_keeps_its_cards_id_and_the_first_line_of_its_message_until_the_next_prompt() {
    let mut studio = Studio::default();
    let stopped = |studio: &Studio| studio.session(SESSION).unwrap().stopped.clone();
    for line in LOG.lines() {
        let change = studio.fingerprint_folder(line).and(Some(found()));
        studio.apply_with_change(line, change);
    }
    let card = format!("{SESSION}@1793265021380");

    let live = stopped(&studio);
    assert_eq!(
        live,
        Some(Stopped {
            card: card.clone(),
            asked: true,
            message: Some(LAST_MESSAGE.into()),
        })
    );

    // Replayed, the same `Stop` asked for no card.
    let mut replayed = Studio::default();
    LOG.lines().for_each(|line| replayed.replay(line));
    assert_eq!(
        stopped(&replayed).map(|s| (s.card, s.asked)),
        Some((card, false))
    );

    replayed
        .apply(&json!({"session_id": SESSION, "hook_event_name": "UserPromptSubmit"}).to_string());
    assert_eq!(stopped(&replayed), None);
}

/// Session `s1`'s event `name` at `at`, with `fields` added.
fn event(name: &str, at: u64, fields: Value) -> String {
    let mut event =
        json!({"session_id": "s1", "hook_event_name": name, "cwd": "/w", "studio_received_ms": at});
    event
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    event.to_string()
}

#[test]
fn a_wake_ups_card_belongs_to_daniels_prompt_before_it() {
    let mut studio = Studio::default();
    let stop = |studio: &mut Studio, at| {
        let outputs = studio.apply_with_change(&event("Stop", at, json!({})), Some(found()));
        outputs.into_iter().find_map(|output| match output {
            Output::Card(request) => request.prompt,
            _ => None,
        })
    };
    studio.apply(&event(
        "UserPromptSubmit",
        100,
        json!({"prompt": "Split the spec"}),
    ));
    let spawn = json!({"tool_name": "Agent", "tool_input": {}, "tool_response": {"agentId": "a1"}});
    studio.apply(&event("PostToolUse", 110, spawn));
    let first = stop(&mut studio, 120);
    let wake_up =
        "<task-notification><task-id>a1</task-id><status>completed</status></task-notification>";
    studio.apply(&event(
        "UserPromptSubmit",
        200,
        json!({ "prompt": wake_up }),
    ));
    let woken = stop(&mut studio, 220);
    studio.apply(&event(
        "UserPromptSubmit",
        300,
        json!({"prompt": "What did a1 do?"}),
    ));
    let asked = stop(&mut studio, 320);

    assert_eq!((first, woken, asked), (Some(100), Some(100), Some(300)));
    let texts: Vec<&str> = studio
        .session("s1")
        .unwrap()
        .prompts
        .iter()
        .map(|p| p.text.as_str())
        .collect();
    assert_eq!(texts, ["Split the spec", wake_up, "What did a1 do?"]);
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
