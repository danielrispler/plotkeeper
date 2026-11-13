//! Seam 1 (danielrispler/Falinks#434, Testing Decisions) for turns (danielrispler/Falinks#434, decisions 3 to 7): a turn is keyed by Claude
//! Code's prompt id. Two fixtures are cut from the checkpoint day (danielrispler/Falinks#370): the wake-up turn of session
//! 2afc85f5 and the prompt Daniel queued during it, and the Esc in session 5bb84a4a.

use serde_json::{Value, json};
use studio_core::{CardRequest, Change, Fingerprint, Output, Purpose, State, Studio};

use State::{Running, YourTurn};

/// Lines 1297 to 1329 of the day, session 2afc85f5 only: its worker's handback (lines 1 to 5), the
/// wake-up (line 6) and its step, the prompt Daniel queued (line 9, the running turn's prompt id),
/// the wake-up turn's `Stop` (line 10), then the queued prompt's steps (lines 11 to 14, a new prompt
/// id and no prompt event of their own) and its `Stop` (line 15).
const QUEUED: &str = include_str!("fixtures/queued-prompt.jsonl");
const QUEUED_SESSION: &str = "2afc85f5-22b5-4ffc-b65e-ab6d69faf302";

/// Lines 923 to 970 of the day, session 5bb84a4a only: a prompt and its steps, a command Daniel
/// interrupted with Esc (line 20, no result and no `Stop`), his next prompt (line 21, still the
/// interrupted turn's prompt id), and the next turn's step (lines 22 and 23, a new prompt id) and
/// `Stop` (line 24).
const ESC: &str = include_str!("fixtures/interrupt-by-esc.jsonl");
const ESC_SESSION: &str = "5bb84a4a-6f08-46b4-8329-b0b88df06518";

/// The session's state after each line of `log`, replayed.
fn states(log: &str, session: &str) -> Vec<State> {
    let mut studio = Studio::default();
    let states = log.lines().map(|line| {
        studio.replay(line);
        studio.session(session).expect("the session").state
    });
    states.collect()
}

fn received(line: &str) -> u64 {
    let event: Value = serde_json::from_str(line).unwrap();
    event["studio_received_ms"].as_u64().unwrap()
}

/// What the fingerprint finds at each look: a change, so every turn end makes a card.
fn found() -> Change {
    Change {
        base: Default::default(),
        diff: "+changed\n".into(),
        head: None,
    }
}

/// The session's looks and card requests when the lines of `log` arrive live, each look finding
/// `found`: each look as its line number and purpose.
fn live(log: &str, session: &str) -> (Vec<(usize, Purpose)>, Vec<CardRequest>) {
    let mut studio = Studio::default();
    let mut looks = Vec::new();
    let mut cards = Vec::new();
    for (index, line) in log.lines().enumerate() {
        let taken: Vec<Fingerprint> = studio.fingerprint_folders(line);
        let ours = taken.iter().filter(|look| look.key == session);
        looks.extend(ours.map(|look| (index + 1, look.purpose)));
        let changes = taken.into_iter().map(|look| (look.key, found()));
        for output in studio.apply_with_changes(line, changes.collect()) {
            if let Output::Card(request) = output {
                cards.push(request);
            }
        }
    }
    (looks, cards)
}

#[test]
fn a_queued_prompt_runs_as_its_own_turn_after_the_one_it_was_typed_in() {
    let expected = [
        // The worker's steps, the previous turn's `Stop`, the worker's handback.
        Running, Running, YourTurn, YourTurn, YourTurn,
        // The wake-up, its step, the queued prompt, the wake-up turn's `Stop`.
        Running, Running, Running, Running, YourTurn,
        // The queued prompt's steps, then its `Stop`.
        Running, Running, Running, Running, YourTurn,
    ];

    assert_eq!(states(QUEUED, QUEUED_SESSION), expected);
}

#[test]
fn a_queued_prompts_text_is_the_prompt_of_the_turn_it_starts() {
    let lines: Vec<&str> = QUEUED.lines().collect();
    let texts = |upto: usize| {
        let studio = Studio::replayed(&lines[..upto]);
        let session = studio.session(QUEUED_SESSION).unwrap();
        let prompts = session.prompts.iter().map(|p| p.text.clone());
        prompts.collect::<Vec<String>>()
    };
    let wake_up = r#"<agent-message from="aaf57a4236e21f65b">…</agent-message>"#;
    let queued = r#"<pasted_content id="8720">…</pasted_content id="8720">"#;

    // Up to the wake-up turn's `Stop`, the queued prompt is not that turn's.
    assert_eq!(texts(10), [wake_up]);
    assert_eq!(texts(11), [wake_up, queued]);

    // The wake-up turn's card belongs to no prompt of Daniel's; the queued turn's to his queued one.
    let (_, cards) = live(QUEUED, QUEUED_SESSION);
    let prompts: Vec<(u64, Option<u64>)> = cards
        .iter()
        .map(|c| (c.id.rsplit('@').next().unwrap().parse().unwrap(), c.prompt))
        .collect();
    assert_eq!(
        prompts,
        [
            // The turn before, whose prompt is not in the cut.
            (received(lines[2]), None),
            (received(lines[9]), None),
            (received(lines[14]), Some(received(lines[8]))),
        ]
    );
}

#[test]
fn an_interrupted_turn_ends_at_the_next_turns_first_event_with_its_own_card() {
    let lines: Vec<&str> = ESC.lines().collect();

    let (looks, cards) = live(ESC, ESC_SESSION);

    // The prompt's base, the interrupted turn's end at the next turn's first step, and its `Stop`.
    assert_eq!(
        looks,
        [
            (1, Purpose::Base),
            (22, Purpose::TurnEnd),
            (24, Purpose::TurnEnd)
        ]
    );
    let prompts: Vec<Option<u64>> = cards.iter().map(|c| c.prompt).collect();
    // The prompt typed after Esc is the next turn's.
    assert_eq!(
        prompts,
        [Some(received(lines[0])), Some(received(lines[20]))]
    );
    assert_eq!(
        states(ESC, ESC_SESSION),
        vec![Running; 23]
            .into_iter()
            .chain([YourTurn])
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_idle_prompt_with_no_agent_ends_an_interrupted_turn() {
    let lines: Vec<&str> = ESC.lines().collect();
    let mut idle: Value = serde_json::from_str(lines[19]).unwrap();
    for key in ["tool_name", "tool_input", "tool_use_id", "effort"] {
        idle.as_object_mut().unwrap().remove(key);
    }
    idle["hook_event_name"] = json!("Notification");
    idle["notification_type"] = json!("idle_prompt");
    idle["studio_received_ms"] = json!(received(lines[19]) + 60_000);
    let log = [lines[..20].join("\n"), idle.to_string()].join("\n");

    let (looks, cards) = live(&log, ESC_SESSION);

    assert_eq!(states(&log, ESC_SESSION).last(), Some(&YourTurn));
    assert_eq!(looks.last(), Some(&(21, Purpose::TurnEnd)));
    assert_eq!(cards.len(), 1);
}
