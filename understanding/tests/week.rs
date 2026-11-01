//! The weekly check on the questions (#328, module 6): the last 7 days' rates, from the card records.

use studio_core::{Base, CardRequest, Check};
use studio_understanding::{Card, Outcome, Rate, StoredCard, week};

/// 2025-10-29, 13:20 UTC.
const NOW: u64 = 1_761_744_000_000;
const DAY: u64 = 86_400_000;

/// A card added `days` ago. `shown` and `revealed` are seconds after it was added; `check` is
/// Daniel's tap and the settled `passed`, for a check's card.
fn record(
    days: u64,
    shown: Option<u64>,
    revealed: Option<u64>,
    check: Option<(Option<bool>, Option<bool>)>,
) -> StoredCard {
    let added_at = NOW - days * DAY;
    let outcome = check.map(|check| match check {
        (Some(guess), Some(passed)) if guess == passed => Outcome::Right,
        (Some(_), Some(_)) => Outcome::Wrong,
        _ => Outcome::NotCounted,
    });
    StoredCard {
        request: CardRequest {
            session_id: "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092".into(),
            id: format!("toolu_{days}"),
            base: Base::default(),
            change: String::new(),
            check: check.map(|(guess, _)| Check {
                guess,
                exit_passed: None,
            }),
        },
        card: Card {
            question: "למה?".into(),
            answer: "כי.".into(),
            paths: Vec::new(),
            code: String::new(),
            why_it_matters: String::new(),
            not_checked: String::new(),
            passed: None,
        },
        added_at: Some(added_at),
        shown_at: shown.map(|s| added_at + s * 1000),
        revealed_at: revealed.map(|s| added_at + s * 1000),
        passed: check.and_then(|(_, passed)| passed),
        outcome,
    }
}

#[test]
fn the_rates_count_reveals_after_3_s_wrong_settled_guesses_and_unknown_checks_of_the_last_7_days() {
    let edits = [
        // Revealed 5 s after the question: thinking.
        record(1, Some(10), Some(15), None),
        // Revealed after 1 s: clicked through.
        record(2, Some(10), Some(11), None),
    ];
    let checks = [
        // Tapped yes, failed: wrong; revealed exactly 3 s after the question.
        record(3, Some(0), Some(3), Some((Some(true), Some(false)))),
        // Tapped no, failed: right; never revealed.
        record(4, Some(0), None, Some((Some(false), Some(false)))),
        // Tapped yes, verdict unknown: not settled.
        record(5, None, None, Some((Some(true), None))),
        // No tap before the result: not settled.
        record(6, None, None, Some((None, Some(true)))),
    ];
    let outside = [
        // Eight days ago: last week's.
        record(8, Some(0), Some(10), Some((Some(true), Some(false)))),
        // Revealed, but from a store with no question time: no reveal to time.
        StoredCard {
            shown_at: None,
            ..record(1, None, Some(10), None)
        },
        // From a store with no times at all.
        StoredCard {
            added_at: None,
            ..record(1, Some(0), Some(10), None)
        },
    ];
    let cards = [&edits[..], &checks, &outside].concat();

    let rates = week(&cards, NOW);

    assert_eq!(
        (rates.thinking, rates.surprise, rates.unknown),
        (
            Rate { count: 2, of: 3 },
            Rate { count: 1, of: 2 },
            Rate { count: 1, of: 4 },
        )
    );
    assert_eq!(
        rates.to_string(),
        "The 7 days to 2025-10-29:\n\
         thinking rate: 67% (2 of 3 reveals came 3 s or more after the question)\n\
         surprise rate: 50% (1 of 2 settled guesses were wrong)\n\
         unknown rate: 25% (1 of 4 check cards had passed unknown)\n"
    );
}

#[test]
fn a_week_with_no_checks_prints_no_surprise_or_unknown_rate() {
    let cards = [
        record(1, Some(10), Some(15), None),
        record(2, Some(10), Some(11), None),
    ];

    let rates = week(&cards, NOW);

    assert_eq!(
        rates.to_string(),
        "The 7 days to 2025-10-29:\n\
         thinking rate: 50% (1 of 2 reveals came 3 s or more after the question)\n\
         surprise rate: n/a (0 of 0 settled guesses were wrong)\n\
         unknown rate: n/a (0 of 0 check cards had passed unknown)\n"
    );
}

#[test]
fn an_empty_store_prints_no_rate() {
    assert_eq!(
        week(&[], NOW).to_string(),
        "The 7 days to 2025-10-29:\n\
         thinking rate: n/a (0 of 0 reveals came 3 s or more after the question)\n\
         surprise rate: n/a (0 of 0 settled guesses were wrong)\n\
         unknown rate: n/a (0 of 0 check cards had passed unknown)\n"
    );
}
