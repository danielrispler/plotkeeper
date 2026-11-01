//! Seam 1 (#369, Testing Decisions) for cards (#369, decisions 10 and 11): a card covers a prompt's
//! whole change, at its `Stop`. The shell fingerprints the worktree where the core asks it to and
//! passes in what changed; these tests play the shell's part.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use studio_core::{Base, CardRequest, Change, Fingerprint, Keep, Output, Stopped, Studio};

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
        head: Some("5f0c1e2d".into()),
    }
}

/// Takes `line` live, as the shell does when each fingerprint it takes before it finds `found`.
fn apply(studio: &mut Studio, line: &str, found: Option<Change>) -> Vec<Output> {
    let looks = studio.fingerprint_folders(line).into_iter();
    let changes = looks.filter_map(|look| Some((look.key, found.clone()?)));
    studio.apply_with_changes(line, changes.collect())
}

/// The card requests when every line of `log` arrives live and each fingerprint finds `found`.
fn cards(log: &str, found: Option<Change>) -> Vec<CardRequest> {
    let mut studio = Studio::default();
    let lines = log.lines();
    let outputs = lines.flat_map(|line| apply(&mut studio, line, found.clone()));
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
            agent: None,
            changed_by: Default::default(),
            workers: vec![],
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
        apply(&mut studio, line, Some(found()));
    }
    let card = format!("{SESSION}@1793265021380");

    let live = stopped(&studio);
    assert_eq!(
        live,
        Some(Stopped {
            card: card.clone(),
            requested: true,
            message: Some(LAST_MESSAGE.into()),
        })
    );

    // Replayed, the same `Stop` requested no card.
    let mut replayed = Studio::default();
    LOG.lines().for_each(|line| replayed.replay(line));
    assert_eq!(
        stopped(&replayed).map(|s| (s.card, s.requested)),
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
        let outputs = apply(studio, &event("Stop", at, json!({})), Some(found()));
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
        let at = !studio.fingerprint_folders(line).is_empty();
        studio.apply(line);
        at
    });
    at.map(|(index, _)| index).collect()
}

#[test]
fn the_shell_fingerprints_at_the_prompt_and_at_the_stop() {
    assert_eq!(fingerprinted(LOG), [0, 9]);
    assert_eq!(
        Studio::default().fingerprint_folders(LOG.lines().next().unwrap()),
        [Fingerprint {
            key: SESSION.into(),
            folder: WORKTREE.into(),
            keep: Keep::Always
        }]
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

/// The scripted run of `record-subagent-run.sh` (#370): its worktree worker writes `hello.txt`,
/// commits it (line 15) and ends (line 18); the session's own worktree never changes.
const RUN: &str = include_str!("fixtures/subagent-run.jsonl");
const RUN_SESSION: &str = "40e8e6ef-9b2f-4467-9b55-87c2d053b616";
const RUN_WORKER: &str = "add0c7bbaced40ed0";

/// #369, decision 11: a worker's card at its commit and at its end, each on its own worktree, and a
/// parent that changed nothing itself gets one card from its workers' cards, not their diffs.
#[test]
fn the_runs_worker_gets_a_card_at_its_commit_and_its_end_and_its_session_one_from_those_cards() {
    let mut studio = Studio::default();
    let mut looked = Vec::new();
    let mut requests = Vec::new();
    for (index, line) in RUN.lines().enumerate() {
        // The worker's worktree changed, with a commit, at each look but its first.
        let mut changes = BTreeMap::new();
        for look in studio.fingerprint_folders(line) {
            let seen = looked.iter().any(|(_, key, _)| key == &look.key);
            looked.push((index + 1, look.key.clone(), look.keep));
            if look.key == RUN_WORKER && seen {
                let change = Change {
                    base: Base::default(),
                    diff: "--- a/hello.txt\n+++ b/hello.txt\n@@ -0,0 +1 @@\n+hello\n".into(),
                    head: Some("c0ffee".into()),
                };
                changes.insert(look.key, change);
            }
        }
        for output in studio.apply_with_changes(line, changes) {
            if let Output::Card(request) = output {
                requests.push(request);
            }
        }
    }

    let (worker, helper) = (RUN_WORKER.to_string(), "a551b1ece0a4430ce".to_string());
    let session = RUN_SESSION.to_string();
    assert_eq!(
        looked,
        [
            (2, session.clone(), Keep::Always),
            // The worker's first event, its commit's Bash step and its end.
            (5, worker.clone(), Keep::Always),
            (8, helper.clone(), Keep::Always),
            (15, worker.clone(), Keep::OnCommit),
            // The helper shares the session's worktree; a change across its command, from before it
            // to after it, would still make it a worker.
            (16, helper.clone(), Keep::Always),
            (17, helper, Keep::OnCommit),
            (18, worker, Keep::Always),
            (28, session, Keep::Always),
        ]
    );
    let cards: Vec<(&str, Option<&str>, bool, &[String])> = requests
        .iter()
        .map(|r| (&*r.id, r.agent.as_deref(), r.change.is_empty(), &*r.workers))
        .collect();
    let commit = format!("{RUN_WORKER}@1793471824831");
    let end = format!("{RUN_WORKER}@1793471826289");
    let stop = format!("{RUN_SESSION}@1793471850661");
    let both = [commit.clone(), end.clone()];
    assert_eq!(
        cards,
        [
            (&*commit, Some(RUN_WORKER), false, &[][..]),
            (&*end, Some(RUN_WORKER), false, &[]),
            (&*stop, None, true, &both),
        ]
    );
    // A worker's cards belong to its task, from its spawn; its session's, to Daniel's prompt.
    assert_eq!(requests[0].prompt, Some(1_793_471_817_301));
    assert_eq!(requests[2].prompt, Some(1_793_471_812_760));
}
