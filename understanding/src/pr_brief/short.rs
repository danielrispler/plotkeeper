//! The short brief (#644, spec #634 "Short brief"): a PR that changes the map but fogs nothing at Daniel's tiers gets a brief made by
//! code, with no writer call and no claim check: one step, "The map in this PR", whose sentence counts what the Map diff marks, then
//! the page's Ratify step.

use super::{Brief, Chapter, PrSource, STORED_VERSION, Step, StoredBrief};
use serde::{Deserialize, Serialize};

/// What a PR's Map diff marks, per kind, and the map changes it draws no mark for (a held item, a changed link or law, a flow), which
/// Ratify lists. The server counts them (`map_diff::counts`); none at all means the map did not change.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapCounts {
    pub parts_added: usize,
    pub parts_removed: usize,
    pub parts_edited: usize,
    pub actors_added: usize,
    pub actors_removed: usize,
    pub links_added: usize,
    pub links_removed: usize,
    pub laws_added: usize,
    pub laws_removed: usize,
    pub unmarked: usize,
    /// The parts and actors the marks are on, sorted: each marked part or actor, a marked link's ends and a marked law's part. The
    /// short brief's step lights them.
    pub lit: Vec<String>,
}

/// The short brief's one step's heading.
pub const SHORT_HEADING: &str = "The map in this PR";

impl MapCounts {
    /// The step's sentence: "2 parts added, 1 removed, 3 edited, 4 links added, 1 law added.", each kind's noun once, then the
    /// changes with no mark.
    pub fn sentence(&self) -> String {
        let kinds: [(&str, &[(usize, &str)]); 4] = [
            (
                "part",
                &[
                    (self.parts_added, "added"),
                    (self.parts_removed, "removed"),
                    (self.parts_edited, "edited"),
                ],
            ),
            (
                "actor",
                &[
                    (self.actors_added, "added"),
                    (self.actors_removed, "removed"),
                ],
            ),
            (
                "link",
                &[(self.links_added, "added"), (self.links_removed, "removed")],
            ),
            (
                "law",
                &[(self.laws_added, "added"), (self.laws_removed, "removed")],
            ),
        ];
        let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
        let mut said: Vec<String> = Vec::new();
        for (noun, counts) in kinds {
            let mut named = false;
            for &(n, verb) in counts.iter().filter(|(n, _)| *n > 0) {
                said.push(if named {
                    format!("{n} {verb}")
                } else {
                    format!("{} {verb}", plural(n, noun))
                });
                named = true;
            }
        }
        let rest = (self.unmarked > 0).then(|| {
            format!(
                "{} with no mark on the map",
                plural(self.unmarked, "change")
            )
        });
        match (said.is_empty(), rest) {
            (true, Some(rest)) => format!("{rest}."),
            (false, Some(rest)) => format!("{}, and {rest}.", said.join(", ")),
            (_, None) => format!("{}.", said.join(", ")),
        }
    }
}

/// The short brief of `source`'s PR at its head, made by code: none when its Map diff is empty (`map_counts` unset, a reformatted
/// file included). Its step lights the parts the marks are on; the PR fogs nothing at Daniel's tiers, so reading it clears no fog. Its
/// link evidence and map check are the source's, as for any brief.
pub fn short_brief(source: &PrSource) -> Option<StoredBrief> {
    let counts = source.map_counts.as_ref()?;
    let meta = &source.meta;
    let step = Step {
        k: "Where".into(),
        h: SHORT_HEADING.into(),
        b: counts.sentence(),
        on: counts.lit.clone(),
        ..Step::default()
    };
    Some(StoredBrief {
        version: STORED_VERSION,
        pr: meta.number,
        head: meta.head.clone(),
        written_at: crate::millis(),
        short: true,
        brief: Brief {
            title: if meta.title.is_empty() {
                format!("PR #{}", meta.number)
            } else {
                meta.title.clone()
            },
            chapters: vec![Chapter {
                behaviour: "story".into(),
                title: SHORT_HEADING.into(),
                parts: counts.lit.clone(),
                steps: vec![step],
                ..Chapter::default()
            }],
            ..Brief::default()
        },
        link_evidence: source.link_evidence.clone(),
        map_problems: source.map_problems.clone(),
        ..StoredBrief::default()
    })
}

/// `brief`, a short brief, says `counts` from now: a push carried its read over to a head whose Map diff may differ (#644).
pub fn recount(brief: &mut StoredBrief, counts: &MapCounts) {
    for step in brief.brief.chapters.iter_mut().flat_map(|c| &mut c.steps) {
        if step.h == SHORT_HEADING {
            step.b = counts.sentence();
            step.on = counts.lit.clone();
        }
    }
    for chapter in &mut brief.brief.chapters {
        chapter.parts = counts.lit.clone();
    }
}
