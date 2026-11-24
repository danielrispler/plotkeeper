// Removed from core/tests/blocked_commands.rs in #415 with `Session::wants_you` (the Dock badge). The other tests' `wants_you` asserts were dropped.


#[test]
fn an_ended_session_does_not_want_you_for_its_waiting_cards() {
    let end = line("SessionEnd", 8_000, json!({ "reason": "other" }));
    let lines = then(allowed(), vec![stop(6_000, None), end]);
    let s = session(&lines);
    assert_eq!(s.state, State::Ended);
    assert_eq!((s.blocked_waiting(), s.wants_you()), (1, false));
}
