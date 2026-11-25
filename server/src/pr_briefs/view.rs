//! What the window shows of a brief: the tray row, the played brief and its lit parts.

use super::{StaleBase, stale_base};
use serde::Serialize;
use std::{collections::BTreeSet, path::Path};
use studio_understanding::pr_brief::{BriefStore, StoredBrief};

/// What the tray row of a ready PR says of its brief.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BriefInfo {
    /// A newer head was pushed; it is rewritten when opened.
    pub stale: bool,
    /// Being written now.
    pub writing: bool,
    /// The chapters' titles, one tray row each; one title for a single story.
    pub chapters: Vec<String>,
    /// Every chapter of the brief was read to its end (#342). The project header's dot may ask this to leave a brief that is done; it does not yet.
    pub read: bool,
    /// The PR touches no part that would fog at Daniel's tiers, so it gets no brief.
    pub quiet: bool,
    /// The last write at this head failed (told to the log); opening it tries again.
    pub failed: bool,
    /// Why the brief at this head is what it is after a push (spec #477, part B): the read carried over, or it was rewritten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The title of the "Since you read" story stored for this head (#497), which the tray opens first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// The story's chapters' titles when it has more than one: one tray row each, so each chapter can be read to its end.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub since_chapters: Vec<String>,
}

/// What a lookup of a ready PR did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Did {
    /// The brief of this head is stored already.
    Kept,
    /// A new head: the stored brief is marked stale, to be rewritten on open or at the next lookup of the same head.
    Staled,
    /// A brief was written.
    Wrote,
    /// The write, or the stale mark of a new head, failed (told to `log`).
    Failed,
    /// No part the PR touches would fog at Daniel's tiers, so it gets no brief.
    Quiet,
    /// The new head carried Daniel's read over (a pure rebase or a push that fogs nothing): the brief is copied to it.
    Carried,
}

/// A stored brief as the window plays it.
#[derive(Debug, Clone, Serialize)]
pub struct BriefView {
    pub brief: StoredBrief,
    /// The writer did not see the whole diff (files cut, or some of their hunks).
    pub partial: bool,
    /// The PR matched the project's pre-flight (#394): the parts it expected against the parts the PR touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<studio_understanding::pre_flight::Expected>,
    /// What the default branch changed since the PR's merge base that bears on the brief (#494); open PRs only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_base: Option<StaleBase>,
    /// This is a "Since you read" story (#497), of the push from this head to its own; the full brief is one click away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

/// The parts each chapter of `brief` lights: the union of its steps' `on`, which is what the checks validate, not the chapter's
/// own `parts` (a single story names `["story"]`, and a chapter can light a part it did not declare).
pub fn lit_parts(brief: &studio_understanding::pr_brief::Brief) -> Vec<Vec<String>> {
    brief
        .chapters
        .iter()
        .map(|c| {
            let mut lit: Vec<String> = Vec::new();
            for part in c.steps.iter().flat_map(|s| &s.on) {
                if !lit.contains(part) {
                    lit.push(part.clone());
                }
            }
            lit
        })
        .collect()
}

/// The tray rows' titles: one per chapter, or the one story's title.
pub fn chapter_titles(brief: &studio_understanding::pr_brief::Brief) -> Vec<String> {
    let titles: Vec<String> = brief
        .chapters
        .iter()
        .map(|c| {
            if c.title.is_empty() {
                c.behaviour.clone()
            } else {
                c.title.clone()
            }
        })
        .collect();
    if titles.len() > 1 {
        titles
    } else {
        vec![brief.title.clone()]
    }
}

/// A stored brief as the window plays it, with whether the writer saw all of the diff, the line of a matched pre-flight, and, for an
/// `open` PR, what the default branch changed since its merge base that bears on the parts it lights (#494).
pub(super) fn view_of(
    project: &str,
    store: &BriefStore,
    brief: StoredBrief,
    open: bool,
) -> BriefView {
    let partial = store
        .records()
        .iter()
        .rev()
        .find(|r| r.pr == brief.pr && r.head == brief.head)
        .is_some_and(|r| r.budget.files_cut + r.budget.files_partial > 0);
    let expected = crate::pre_flight::expected(project, brief.pr, &brief);
    let stale_base = open
        .then(|| {
            let lit: BTreeSet<String> = lit_parts(&brief.brief).into_iter().flatten().collect();
            stale_base(Path::new(project), &brief.head, &lit)
        })
        .flatten();
    BriefView {
        brief,
        partial,
        expected,
        stale_base,
        since: None,
    }
}

/// A "Since you read" story as the window plays it (#497): its own Partial facts (its `partial` flag from them alone, since its record
/// shares the full brief's head), and the head it runs from. No stale-base line or pre-flight: those are the full brief's.
pub(super) fn since_view(from: String, story: StoredBrief) -> BriefView {
    BriefView {
        partial: story.partial_facts.cut.is_some(),
        brief: story,
        expected: None,
        stale_base: None,
        since: Some(from),
    }
}
