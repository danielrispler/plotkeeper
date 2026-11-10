//! Seam 1 for #556: a your-turn session stays unseen until Daniel looks at it in a focused window,
//! and the sounds that go with needs you and your turn.

use std::time::Duration;
use studio_core::{Due, LiveFacts, Notifications, Output, Sound, Studio};

/// `two-sessions.jsonl`'s sessions, both in a turn at its end.
const TWO: &str = include_str!("fixtures/two-sessions.jsonl");
const FIRST: &str = "c41d8e27-6a3f-4b90-8e15-2f7a9b0c3d64";
const SECOND: &str = "9e0b5f13-d72a-4c6e-b8f4-61a3c2d9e807";

/// Session `id`'s `Stop`, under the prompt id its turn runs under.
fn stop(id: &str) -> String {
    let prompt_id = match id {
        FIRST => "e2a7c9d1-3f5b-4a8e-9c6d-0b1a2f3e4d5c",
        _ => "71c3e5a9-b2d4-4f6e-8a0c-1e3d5f7a9b2c",
    };
    format!(
        r#"{{"session_id":"{id}","prompt_id":"{prompt_id}","hook_event_name":"Stop","stop_hook_active":false,"studio_received_ms":1793261020000}}"#
    )
}

fn unseen(studio: &Studio, id: &str) -> bool {
    studio.session(id).expect("the session").unseen
}

/// Both sessions replayed, the first selected, and the second's turn ended live.
fn second_stopped() -> Studio {
    let mut studio = Studio::replayed(TWO.lines());
    studio.select(FIRST);
    studio.apply(&stop(SECOND));
    studio
}

#[test]
fn a_turn_ending_while_its_session_is_not_shown_leaves_it_unseen() {
    let mut studio = second_stopped();
    assert!(unseen(&studio, SECOND));
    // Looking at another session does not see it.
    assert_eq!(studio.see_selected(true), None);
    assert!(unseen(&studio, SECOND));
}

#[test]
fn a_running_session_is_never_unseen() {
    let studio = second_stopped();
    assert!(!unseen(&studio, FIRST));
}

#[test]
fn showing_it_while_focused_sees_it() {
    let mut studio = second_stopped();
    studio.select(SECOND);
    // The store keeps the turns seen, by the row's id.
    assert_eq!(studio.see_selected(true), Some((SECOND.into(), 1)));
    assert!(!unseen(&studio, SECOND));
    assert_eq!(studio.see_selected(true), None);
}

#[test]
fn showing_it_while_unfocused_does_not() {
    let mut studio = second_stopped();
    studio.select(SECOND);
    assert_eq!(studio.see_selected(false), None);
    assert!(unseen(&studio, SECOND));
}

#[test]
fn a_turn_ending_while_shown_and_focused_is_seen_at_the_next_pass() {
    let mut studio = Studio::replayed(TWO.lines());
    studio.select(SECOND);
    // Looking while it runs sees nothing, so the turn's end is not seen early.
    assert_eq!(studio.see_selected(true), None);
    studio.apply(&stop(SECOND));
    assert!(unseen(&studio, SECOND));
    assert_eq!(studio.see_selected(true), Some((SECOND.into(), 1)));
}

#[test]
fn mark_unseen_brings_the_mark_back_until_the_session_is_selected_again() {
    let mut studio = second_stopped();
    studio.select(SECOND);
    studio.see_selected(true);

    assert_eq!(studio.mark_unseen(SECOND), Some((SECOND.into(), 0)));
    assert!(unseen(&studio, SECOND));
    // Still shown and focused, it stays unseen until Daniel selects it again.
    assert_eq!(studio.see_selected(true), None);
    assert!(unseen(&studio, SECOND));
    studio.select(FIRST);
    studio.select(SECOND);
    assert_eq!(studio.see_selected(true), Some((SECOND.into(), 1)));
}

#[test]
fn a_restart_keeps_the_turns_seen() {
    let mut facts = LiveFacts::default();
    facts.seen.insert(SECOND.into(), 1);
    let mut studio = Studio::replayed(TWO.lines().chain([stop(SECOND).as_str()]));
    assert!(unseen(&studio, SECOND));
    studio.restore(&facts, &[]);
    assert!(!unseen(&studio, SECOND));
}

fn at(ms: u64) -> Duration {
    Duration::from_millis(ms)
}

/// `permission-write.jsonl`'s session waits on Daniel's permission at line 3.
const PERMISSION: &str = include_str!("fixtures/permission-write.jsonl");
const ASKING: &str = "7f3c2a9e-4b1d-4c8e-9a2f-1d6b5e8c0a31";

/// What is due 3 seconds after the session asks, as the shell queues it, with `selected` selected,
/// the window `focused` or not and the sounds `sounds` on or off.
fn needs_you(selected: Option<&str>, focused: bool, sounds: bool) -> Due {
    let mut studio = Studio::default();
    let mut notifications = Notifications::default();
    notifications.sounds = sounds;
    for line in PERMISSION.lines().take(3) {
        for output in studio.apply(line) {
            if let Output::NeedsYou { session_id, .. } = output {
                notifications.push_needs_you(&session_id, String::new(), at(1_000));
            }
        }
    }
    let selected = selected.map(String::from);
    notifications.due(at(4_000), || focused, |_| true, || selected)
}

/// What is due 3 seconds after the second session's turn ends, with the first selected.
fn your_turn(focused: bool, while_focused: bool) -> Due {
    let mut studio = second_stopped();
    let mut notifications = Notifications::default();
    notifications.while_focused = while_focused;
    studio.select(FIRST);
    notifications.push_your_turn(SECOND, String::new(), at(1_000));
    let selected = studio.selected().map(|s| s.agent.id.clone());
    notifications.due(at(4_000), || focused, |_| false, || selected)
}

#[test]
fn focused_a_needs_you_on_another_session_sounds_with_no_notification() {
    let due = needs_you(Some(FIRST), true, true);
    assert_eq!(due.sound, Some(Sound::NeedsYou));
    assert_eq!(due.notification, None);
}

#[test]
fn focused_the_viewed_session_makes_no_sound() {
    assert_eq!(needs_you(Some(ASKING), true, true).sound, None);
}

#[test]
fn focused_your_turn_makes_no_sound_even_when_it_notifies() {
    assert_eq!(your_turn(true, false).sound, None);
    let due = your_turn(true, true);
    assert!(due.notification.is_some());
    assert_eq!(due.sound, None);
}

#[test]
fn unfocused_a_sound_plays_with_each_needs_you_or_your_turn_notification() {
    let due = needs_you(None, false, true);
    assert!(due.notification.is_some());
    assert_eq!(due.sound, Some(Sound::NeedsYou));
    let due = your_turn(false, false);
    assert!(due.notification.is_some());
    assert_eq!(due.sound, Some(Sound::YourTurn));
}

#[test]
fn unfocused_no_notification_means_no_sound() {
    let mut notifications = Notifications::default();
    notifications.push_needs_you(ASKING, String::new(), at(1_000));
    // Answered within 3 seconds: no notification, so no sound.
    let due = notifications.due(at(4_000), || false, |_| false, || None);
    assert_eq!((due.notification, due.sound), (None, None));
    // A failed worker notifies with no sound.
    notifications.push("Worker failed", String::new(), at(5_000));
    let due = notifications.due(at(8_000), || false, |_| false, || None);
    assert!(due.notification.is_some());
    assert_eq!(due.sound, None);
}

#[test]
fn sounds_off_plays_none() {
    assert_eq!(needs_you(Some(FIRST), true, false).sound, None);
    assert_eq!(needs_you(None, false, false).sound, None);
}
