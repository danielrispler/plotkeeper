//! The weekly check on the questions (#328, module 6; resolution, "Understanding layer", point 7;
//! #369, decision 23): whether the questions make Daniel think, and how he rated the cards. Only the
//! weekly command prints these; the app shows no rate. The thresholds are starting guesses.

use crate::{Level, Reason, StoredCard};
use std::fmt;

pub(crate) const WEEK: u64 = 7 * DAY;
const DAY: u64 = 86_400_000;
/// A guess this long after its quiz showed counts as thinking, in milliseconds.
const THINKING: u64 = 3_000;

/// `count` of `of` records.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rate {
    pub count: usize,
    pub of: usize,
}

/// The rates of the cards added in the 7 days to `to`, in Unix milliseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Week {
    pub to: u64,
    /// Of the guesses on a quiz with a shown time, those 3 s or more after it.
    pub thinking: Rate,
    /// Of the guesses, those that were wrong.
    pub surprise: Rate,
    pub ratings: Ratings,
}

/// Daniel's ratings of the week's cards, by level and by reason (#369, decision 23).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ratings {
    /// The week's cards, rated or not.
    pub cards: usize,
    pub good: usize,
    pub fair: usize,
    pub poor: usize,
    pub unclear: usize,
    pub wrong: usize,
    pub too_shallow: usize,
    pub too_easy: usize,
    /// The cards flagged "the key is wrong", from the quiz or from the rating.
    pub key_wrong: usize,
}

/// The rates of the cards added in the 7 days to `now`. Cards with no added time, from a store
/// written before it existed, are in no week.
pub fn week(cards: &[StoredCard], now: u64) -> Week {
    let cards: Vec<_> = cards
        .iter()
        .filter(|kept| kept.added_at.is_some_and(|at| at + WEEK >= now))
        .collect();
    let gaps = cards.iter().filter_map(|kept| {
        let (shown, guess) = kept.shown_at.zip(kept.guess)?;
        Some(guess.at.saturating_sub(shown))
    });
    let guesses = cards.iter().filter_map(|kept| kept.guess);
    Week {
        to: now,
        thinking: rate(gaps, |gap| *gap >= THINKING),
        surprise: rate(guesses, |guess| !guess.right),
        ratings: ratings(&cards),
    }
}

fn ratings(cards: &[&StoredCard]) -> Ratings {
    let mut counts = Ratings {
        cards: cards.len(),
        ..Ratings::default()
    };
    for kept in cards {
        counts.key_wrong += usize::from(kept.key_wrong);
        let Some(rating) = &kept.rating else {
            continue;
        };
        *match rating.level {
            Level::Good => &mut counts.good,
            Level::Fair => &mut counts.fair,
            Level::Poor => &mut counts.poor,
        } += 1;
        for reason in &rating.reasons {
            *match reason {
                Reason::Unclear => &mut counts.unclear,
                Reason::Wrong => &mut counts.wrong,
                Reason::TooShallow => &mut counts.too_shallow,
                Reason::TooEasy => &mut counts.too_easy,
            } += 1;
        }
    }
    counts
}

fn rate<T>(records: impl Iterator<Item = T>, counts: impl Fn(&T) -> bool) -> Rate {
    let (mut count, mut of) = (0, 0);
    for record in records {
        count += usize::from(counts(&record));
        of += 1;
    }
    Rate { count, of }
}

impl fmt::Display for Rate {
    /// The share in whole percent, or `n/a` with nothing to count, so an empty week divides by
    /// nothing.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.of {
            0 => write!(f, "n/a"),
            of => write!(f, "{:.0}%", self.count as f64 * 100.0 / of as f64),
        }
    }
}

impl fmt::Display for Week {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Week {
            to,
            thinking: t,
            surprise: s,
            ratings: r,
        } = self;
        writeln!(f, "The 7 days to {}:", date(*to))?;
        writeln!(
            f,
            "thinking rate: {t} ({} of {} answers came 3 s or more after the question)",
            t.count, t.of
        )?;
        writeln!(
            f,
            "surprise rate: {s} ({} of {} answers were wrong)",
            s.count, s.of
        )?;
        writeln!(
            f,
            "ratings: {} of {} cards (good {}, fair {}, poor {})",
            r.good + r.fair + r.poor,
            r.cards,
            r.good,
            r.fair,
            r.poor
        )?;
        writeln!(
            f,
            "reasons: unclear {}, wrong {}, too shallow {}, too easy {}, the key is wrong {}",
            r.unclear, r.wrong, r.too_shallow, r.too_easy, r.key_wrong
        )
    }
}

/// The UTC date of `ms`, in Unix milliseconds, as `YYYY-MM-DD` (Howard Hinnant's
/// `civil_from_days`).
pub(crate) fn date(ms: u64) -> String {
    let days = ms / DAY + 719_468;
    let (era, day_of_era) = (days / 146_097, days % 146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = era * 400 + year_of_era + u64::from(month <= 2);
    format!("{year}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn dates_are_utc_days_including_leap_days() {
        assert_eq!(super::date(0), "1970-01-01");
        assert_eq!(super::date(1_709_164_800_000), "2024-02-29");
        assert_eq!(super::date(1_709_251_199_999), "2024-02-29");
        assert_eq!(super::date(1_740_787_200_000), "2025-03-01");
    }
}
