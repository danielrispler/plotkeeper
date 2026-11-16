//! The report of the self-test (danielrispler/plotkeeper#130): whether asking the Quiz first helps Daniel remember a turn's
//! design better than reading the summary. It reads the card store only and prints counts, never a verdict; the decision is
//! Daniel's.

use crate::{Arm, QuizTap, Retest, StoredSummary, Tap};
use std::fmt;

/// What one arm of the test holds.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArmReport {
    /// The turns assigned to the arm.
    pub turns: usize,
    /// The retests of those turns that were answered with a pick, and the ones that picked the key.
    pub answered: usize,
    pub right: usize,
    /// The turns on which Daniel opened a hunk.
    pub hunks: usize,
    /// The median time from a retest's first showing to its pick, in milliseconds.
    pub median_pick_ms: Option<u64>,
}

/// The two arms, quiz-first then body-only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelfTest {
    pub arms: [ArmReport; 2],
}

fn at(taps: &[QuizTap], find: impl Fn(&Tap) -> bool) -> Option<u64> {
    taps.iter().find(|t| find(&t.tap)).map(|t| t.at)
}

/// The report of the turns that were given an arm, from the summaries and the retests.
pub fn self_test(summaries: &[StoredSummary], retests: &[Retest]) -> SelfTest {
    let arm = |which: Arm| {
        let turns: Vec<_> = summaries.iter().filter(|s| s.arm == Some(which)).collect();
        let mut report = ArmReport {
            turns: turns.len(),
            hunks: turns
                .iter()
                .filter(|s| at(&s.taps, |t| *t == Tap::Hunk).is_some())
                .count(),
            ..ArmReport::default()
        };
        let mut waits = Vec::new();
        for retest in retests
            .iter()
            .filter(|r| turns.iter().any(|s| s.request.id == r.of))
        {
            let Some(quiz) = &retest.quiz else { continue };
            let pick = retest.taps.iter().find_map(|t| match t.tap {
                Tap::Pick { option } => Some((option, t.at)),
                _ => None,
            });
            let Some((option, picked)) = pick else {
                continue;
            };
            report.answered += 1;
            report.right += usize::from(option == quiz.key);
            if let Some(shown) = at(&retest.taps, |t| *t == Tap::Shown) {
                waits.push(picked.saturating_sub(shown));
            }
        }
        waits.sort_unstable();
        report.median_pick_ms = match waits.len() {
            0 => None,
            n if n % 2 == 1 => Some(waits[n / 2]),
            n => Some((waits[n / 2 - 1] + waits[n / 2]) / 2),
        };
        report
    };
    SelfTest {
        arms: [arm(Arm::QuizFirst), arm(Arm::BodyOnly)],
    }
}

impl fmt::Display for SelfTest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "arm          turns  retests answered  right  hunks opened  median time to pick"
        )?;
        for (name, a) in ["quiz-first", "body-only"].into_iter().zip(&self.arms) {
            let median = a
                .median_pick_ms
                .map_or("n/a".into(), |ms| format!("{:.1} s", ms as f64 / 1000.0));
            let right = match a.answered {
                0 => "n/a".into(),
                n => format!("{} of {n}", a.right),
            };
            writeln!(
                f,
                "{name:<12} {:>5}  {:>16}  {right:>5}  {:>12}  {median}",
                a.turns, a.answered, a.hunks
            )?;
        }
        writeln!(
            f,
            "About 20 turns per arm can show only a large difference. If quiz-first is not clearly better, the Quiz stays only on turns that get a card; the decision is yours."
        )
    }
}
