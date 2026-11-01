//! The weekly check on the questions (#328, module 6; resolution, "Understanding layer", point 7):
//! whether the questions make Daniel think. Only the weekly command prints these; the app shows no
//! rate. The thresholds are starting guesses.

use crate::{Outcome, StoredCard};
use std::fmt;

pub(crate) const WEEK: u64 = 7 * DAY;
const DAY: u64 = 86_400_000;
/// A reveal this long after its question counts as thinking, in milliseconds.
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
    /// Of the reveals of a question with a shown time, those 3 s or more after it.
    pub thinking: Rate,
    /// Of the settled guesses, those that were wrong.
    pub surprise: Rate,
    /// Of the check cards, those whose `passed` was unknown.
    pub unknown: Rate,
}

/// The rates of the cards added in the 7 days to `now`. Cards with no added time, from a store
/// written before it existed, are in no week.
pub fn week(cards: &[StoredCard], now: u64) -> Week {
    let cards: Vec<_> = cards
        .iter()
        .filter(|kept| kept.added_at.is_some_and(|at| at + WEEK >= now))
        .collect();
    let gaps = cards.iter().filter_map(|kept| {
        let (shown, revealed) = kept.shown_at.zip(kept.revealed_at)?;
        Some(revealed.saturating_sub(shown))
    });
    let settled = cards.iter().filter_map(|kept| match kept.outcome {
        Some(Outcome::NotCounted) | None => None,
        Some(outcome) => Some(outcome),
    });
    let checks = cards.iter().filter(|kept| kept.request.check.is_some());
    Week {
        to: now,
        thinking: rate(gaps, |gap| *gap >= THINKING),
        surprise: rate(settled, |outcome| *outcome == Outcome::Wrong),
        unknown: rate(checks, |kept| kept.passed.is_none()),
    }
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
            unknown: u,
        } = self;
        writeln!(f, "The 7 days to {}:", date(*to))?;
        writeln!(
            f,
            "thinking rate: {t} ({} of {} reveals came 3 s or more after the question)",
            t.count, t.of
        )?;
        writeln!(
            f,
            "surprise rate: {s} ({} of {} settled guesses were wrong)",
            s.count, s.of
        )?;
        writeln!(
            f,
            "unknown rate: {u} ({} of {} check cards had passed unknown)",
            u.count, u.of
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
