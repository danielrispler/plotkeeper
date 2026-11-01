//! The weekly check on the questions (#328, module 6; #369, decision 23): the last 7 days' rates,
//! from Daniel's guesses on the cards' quizzes.

use studio_core::{Base, CardRequest};
use studio_understanding::{Card, Guess, Rate, StoredCard, week};

/// 2025-10-29, 13:20 UTC.
const NOW: u64 = 1_761_744_000_000;
const DAY: u64 = 86_400_000;

/// A card added `days` ago. `shown` is seconds after it was added; `answered` is seconds after it
/// was added, and whether Daniel's tap was right.
fn record(days: u64, shown: Option<u64>, answered: Option<(u64, bool)>) -> StoredCard {
    let added_at = NOW - days * DAY;
    StoredCard {
        request: CardRequest {
            session_id: "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092".into(),
            id: format!("toolu_{days}"),
            base: Base::default(),
            change: String::new(),
        },
        card: Card {
            explanation: "כי.".into(),
            paths: Vec::new(),
            code: String::new(),
            why_it_matters: String::new(),
            not_checked: String::new(),
            quiz: None,
        },
        added_at: Some(added_at),
        shown_at: shown.map(|s| added_at + s * 1000),
        guess: answered.map(|(s, right)| Guess {
            option: 0,
            right,
            at: added_at + s * 1000,
        }),
        key_wrong: false,
    }
}

#[test]
fn the_rates_count_answers_after_3_s_and_wrong_answers_of_the_last_7_days() {
    let cards = [
        // Right, 5 s after the question: thinking.
        record(1, Some(10), Some((15, true))),
        // Right after 1 s: tapped through.
        record(2, Some(10), Some((11, true))),
        // Wrong, exactly 3 s after the question.
        record(3, Some(0), Some((3, false))),
        // Shown, never answered.
        record(4, Some(0), None),
        // Eight days ago: last week's.
        record(8, Some(0), Some((10, false))),
        // Wrong, from a store with no question time: no tap to time, but a wrong answer.
        record(5, None, Some((10, false))),
        // From a store with no times at all.
        StoredCard {
            added_at: None,
            ..record(1, Some(0), Some((10, false)))
        },
    ];

    let rates = week(&cards, NOW);

    assert_eq!(
        (rates.thinking, rates.surprise),
        (Rate { count: 2, of: 3 }, Rate { count: 2, of: 4 })
    );
    assert_eq!(
        rates.to_string(),
        "The 7 days to 2025-10-29:\n\
         thinking rate: 67% (2 of 3 answers came 3 s or more after the question)\n\
         surprise rate: 50% (2 of 4 answers were wrong)\n"
    );
}

#[test]
fn an_empty_store_prints_no_rate() {
    assert_eq!(
        week(&[], NOW).to_string(),
        "The 7 days to 2025-10-29:\n\
         thinking rate: n/a (0 of 0 answers came 3 s or more after the question)\n\
         surprise rate: n/a (0 of 0 answers were wrong)\n"
    );
}
