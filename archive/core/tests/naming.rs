// Removed from core/tests/naming.rs in #416 with `Studio::set_snapshot` and the context's window and share, which only these tests called. Fragments, kept verbatim.

/// The context's tokens, window, share and what is left of the window.
fn measured(studio: &Studio) -> (u64, Option<u64>, Option<u64>, Option<u64>) {
    let c = context(studio).unwrap();
    (c.tokens, c.window, c.share, c.left)
}

/// A status line snapshot's `context_window` of a 1M window, its `current_usage` holding `cached`
/// cache-read tokens, with Claude's own percentages when `percentages` gives them.
fn snapshot(cached: u64, percentages: Option<(u64, u64)>) -> Option<Value> {
    let mut window = json!({
        "total_input_tokens": 133_671,
        "context_window_size": 1_000_000,
        "current_usage": {"input_tokens": 2, "output_tokens": 8, "cache_creation_input_tokens": 4_901, "cache_read_input_tokens": cached},
    });
    if let Some((used, left)) = percentages {
        window["used_percentage"] = json!(used);
        window["remaining_percentage"] = json!(left);
    }
    Some(window)
}

#[test]
fn with_a_snapshot_the_context_is_claudes_status_line_view() {
    // danielrispler/Falinks#517, measurement 1's turn: the snapshot counts 2 + 4,901 + 84,899 = 89,802 tokens, its 9% used
    // and 91% left, whatever the transcript's latest message says.
    let (mut studio, file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);
    let claudes = snapshot(84_899, Some((9, 91)));

    assert!(studio.set_snapshot(SESSION, claudes.clone()));
    assert_eq!(
        measured(&studio),
        (89_802, Some(1_000_000), Some(9), Some(91))
    );
    // The same snapshot again changes nothing.
    assert!(!studio.set_snapshot(SESSION, claudes));
    // The transcript's next message keeps the snapshot's view.
    std::fs::write(file.path(), first(6)).unwrap();
    follow(&mut studio);
    assert_eq!(
        measured(&studio),
        (89_802, Some(1_000_000), Some(9), Some(91))
    );
}

#[test]
fn a_snapshot_without_percentages_rounds_to_nearest() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);

    // 2 + 4,901 + 197,797 = 202,700 tokens, 20.27% of the window: 20 used, 80 left, not 21.
    assert!(studio.set_snapshot(SESSION, snapshot(197_797, None)));
    assert_eq!(
        measured(&studio),
        (202_700, Some(1_000_000), Some(20), Some(80))
    );
}

#[test]
fn before_the_first_api_call_the_transcript_counts_in_the_snapshots_window() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");
    follow(&mut studio);

    let mut start = snapshot(0, None).unwrap();
    start["current_usage"] = Value::Null;
    assert!(studio.set_snapshot(SESSION, Some(start)));
    // The transcript's 100,002 tokens of 1M: 10% used, 90% left.
    assert_eq!(
        measured(&studio),
        (100_002, Some(1_000_000), Some(10), Some(90))
    );
}

#[test]
fn with_no_snapshot_a_session_has_a_token_count_and_no_share() {
    let (mut studio, _file) = session(TASK, &first(3), "gh issue view 480");

    follow(&mut studio);
    assert!(!studio.set_snapshot(SESSION, None));
    assert_eq!(measured(&studio), (100_002, None, None, None));
}
