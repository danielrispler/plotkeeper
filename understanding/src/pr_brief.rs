//! The PR brief (spec #334, ticket #337): one entry, `write_pr_brief`, that writes a PR's brief with the sandboxed
//! writer from facts code assembles, holds it to deterministic checks, stores it per PR and head, and records
//! one line about the call.
//!
//! Public API, in the order a caller meets it:
//! - `fetch_meta` (`gh`) and `gather` (`git`) fill a `PrSource`, plain data, so a test builds one by hand;
//! - `build_facts` turns a source into `Facts`: the text the writer reads, the shape, the budget record and the
//!   `CheckContext` the checks need;
//! - `parse_reply` and `check_brief` read and check a reply; `pr_brief_check` (#338) then runs the claim check
//!   and the one retry from the flags `check_brief` leaves on the items;
//! - `BriefStore` keeps briefs and the records in the data folder;
//! - `write_pr_brief` does all of it with a `BriefWriter`, which `ClaudeWriter` is and a test fakes.

use crate::millis;

mod brief;
mod build;
mod checks;
mod facts;
mod gather;
mod prompt;
mod reply;
mod source;
mod store;

pub use brief::*;
pub use build::*;
pub use checks::*;
pub use facts::*;
pub use gather::*;
pub use prompt::*;
pub use reply::*;
pub use source::*;
pub use store::*;

/// The facts' budget: the writer is sandboxed with no tools, so everything it reads is here.
pub const BUDGET: usize = 64 << 10;
/// The default of `prBriefs.model` in `settings.json`.
pub const DEFAULT_MODEL: &str = "opus";
/// The stored brief's format version; a reader ignores fields it does not know.
pub const STORED_VERSION: u32 = 1;
/// Room kept for the "DIFF NOT SENT" section.
pub(crate) const CUT_NOTE_ROOM: usize = 4096;

/// The writer's instructions, in the prompt after its facts (`prompt`).
pub(crate) const INSTRUCTIONS: &str = r#"You write a PR brief: a short, stepped, plain-language explanation of what one pull request changed in a system, for the system's owner. He directs AI agents that write the code; he does not read code. The brief exists to keep his mental model accurate: what each part does now, how behaviour flows between parts, what must stay true, what could break. It is not a changelog and not a file tour. The brief plays on a map of the system: each step lights parts and may send a light along links.

You get FACTS, in the system prompt. The code has already decided the brief's SHAPE (single story or chapters, and which parts must be covered); follow it exactly.

SELECTION. Find the PR's central behaviours: things the owner would name ("fog", "broken laws"), each of which may cross several parts. Apply the deletion test: "if this change were removed, would the system do something different for the owner?" Keep those; leave out plumbing, renames, test scaffolding, docs and anything that only moves code around. In chapters, each chapter is ONE behaviour and lights every part it crosses, following the links between them; choose 2 to 4 behaviours so that together they light every part in SHAPE's required list. Use all 4 chapters when the PR body or the spec names more than 3 central behaviours. A part with only a small change is lit inside the behaviour it serves, never given a chapter of its own. Choose, do not list.

EVIDENCE LADDER. Say only what the facts support, at the strength they support it:
- "a test tagged to law X covers ..." ONLY when PARTS TABLE marks one of law X's tagged tests "changed in this PR", and then the step names that test in "covered_by"; otherwise you may still name the law, as "the map's law X says ...", but never claim a test covers this change;
- "checks passed" when CHECKS says so;
- "the PR says ..." or "the spec says ..." for anything only the PR body or spec states;
- otherwise leave it out. Do not hedge, do not guess, do not explain why something was left out.
Never write these words in your own prose: guarantee(s/d), ensure(s/d), cannot, can't, impossible, always works, never fails. Say what is covered, not what is promised.
Never mention in the story what you did not see or what was not sent. Put each gap you noticed in "unseen", one short line each.

CITES. Every step has "cites": a list of {"from": "PR" | "SPEC" | "MAP" | "DIFF" | "TESTS" | "CHECKS", "quote": "..."}, where quote is an exact phrase of 3 to 15 words copied from that section of FACTS. Code checks each quote; a step whose quote is not in the facts is marked unconfirmed.

Reply with ONE JSON object and nothing else:

{
  "title": "a plain title, at most 9 words",
  "overview": [ step, ... ],   only when SHAPE says an overview: at most 2 steps that run the changed flow's hops; the flow's order check, if any, is its last step
  "chapters": [ { "behaviour": "the behaviour, at most 4 words (or \"story\" for a single story)", "parts": [ "part ids this chapter covers" ], "title": "at most 6 words", "gist": "only on a chapter that lights a Core part (SHAPE's core parts); leave it out on any other: the chapter's whole story in ONE plain sentence, at most 30 words, that the owner could say back from memory; it says only what the chapter's steps say", "steps": [ step, ... ] } ],
  "unseen": [ "a gap you noticed in the facts" ],
  "decisions": [ { "id": "the id of the same decision in EARLIER DECISIONS, when there is one; else leave it out", "text": "a decision the agent made that the owner may want to accept or overturn, one sentence, from the PR body or spec" } ]
}

A step:
{
  "k": one of "Where", "Before", "Now", "Must stay true", "Could break", "Not in the plan", "Your turn",
  "h": "the step's one point, at most 14 words",
  "b": "at most 45 words of plain explanation; may use <b>...</b> once; empty string allowed",
  "on": [ part or actor ids this step lights ],
  "hops": [ [ "link-id", false ], ... ]   optional: the light's path, in order; true runs the link backwards,
  "law": "law-id"   optional, on "Must stay true": the map's law it is about,
  "covered_by": "the exact name of a tagged test of that law that PARTS TABLE marks changed in this PR"   optional, only on "Must stay true" and only with "law",
  "list": [ "short line" ]   optional, only for "Not in the plan",
  "risks": [ { "part": "part id", "trigger": "the concrete situation that would break it, from a law, link or the diff", "check": "the cheapest way to see it" } ]   only for "Could break": at most 3; an empty list is fine and then omit the step,
  "fig": figure   optional, only when one fits a change inside one part,
  "check": check   required on "Your turn" and only there; a step that would hold no check is not a "Your turn" step: leave it out (never a chore such as "restart" or "watch"),
  "cites": [ ... ]
}

Shape rules:
- A chapter is 3 or 4 steps: Before, Now, and one or two of Must stay true / Could break / Not in the plan / Your turn. Its title names the behaviour, so "Where" is only for a single story's first step.
- A single story is at most 7 steps: Where, Before, Now, Must stay true, Could break, then Not in the plan and Your turn when they apply.
- The whole brief is at most 12 steps, overview included.
- "Not in the plan" (only with a spec): what the agent improvised, changed or fixed that the spec did not say, and what review found. Only real departures from the spec; never list things built as specified. Put it in the chapter whose behaviour it concerns; at most one per chapter. If the spec was created the same day as the PR, put it first in its chapter.
- Plain words: name parts by their map names; never code identifiers, file names, function names or commands in h, b, list, risks or figures.

Strength of claims. A claim check reads every item against FACTS and sends back each claim stronger than the facts. Most of what it sends back is a claim widened past what the facts name, such as "Each agent's plugin calls the hook on every event" when the map says the plugins choose which events reach it, or "Every line was read by its shape, as Claude Code's" when the facts say only unnamed lines were. So:
- "every", "each", "all", "only", "always", "never", "whole", "no ... at all" need a fact that says exactly that. When the facts name some cases, name those cases ("the Codex and Cursor readers"), never all of them.
- Keep the facts' own limits: "on pull requests", "when a line has no name", "the first round".
- Before says only what the facts show of the old behaviour; do not sum it up in a phrase of your own ("the server mostly served").
- Give no cause, intent, origin or consequence the facts do not state: how a change was found, what it was built from, why it was made, what would go wrong without it.
- A count or a list says exactly what the facts list: "three fixes" only when the facts list three.

Figure kinds (code draws them; each label at most 18 characters):
- {"kind": "states", "frame": "what holds them", "items": [ {"label": "...", "tone": "live" | "lost" | "plain"} ]}  up to 3 items.
- {"kind": "sequence", "items": [ {"label": "...", "bad": false} ]}  up to 4 steps left to right; "bad": true on the one that goes wrong.
- {"kind": "compare", "before": [ "line" ], "after": [ "line" ]}  up to 3 lines each.

Checks (at most one per brief; omit when none fits):
- A change to a flow: {"kind": "order", "q": "...", "key": [ part ids in order ], "yes": "...", "no": "...", "flow": "flow-id"}. Only when the map's diff changes that flow's hops or a law on it.
- A change inside one part: {"kind": "pick", "q": "a concrete case to predict", "opts": [ "...", "..." ], "key": index, "yes": "...", "no": "...", "test": "the exact name of a test this PR added or changed that decides the answer"}. Only when such a test is in the diff.
"#;

/// Writes the brief of `source`'s PR at its head (a "Since you read" story when `source.since` is set, stored apart): builds the facts, asks the writer once, parses and checks the
/// reply, runs the claim check and the one retry (`pr_brief_check`), stores the brief and appends its one
/// record line. `log` hears one line per problem that does not stop the write.
pub fn write_pr_brief(
    writer: &(impl BriefWriter + ?Sized),
    model: &str,
    source: &PrSource,
    store: &BriefStore,
    mut log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    let facts = build_facts(source)?;
    let reply = timed_write_facts(
        writer,
        model,
        CallKind::Write,
        &facts_block(&facts),
        &prompt(&facts),
    )?;
    let mut brief = parse_reply(&reply.text)?;
    assign_decision_ids(&mut brief, &source.earlier_decisions);
    check_brief(&mut brief, &facts.context);
    let code_flags = brief.all_flags();
    let mut record = BriefRecord {
        pr: source.meta.number,
        head: source.meta.head.clone(),
        at: millis(),
        shape: facts.shape.label(),
        budget: facts.budget.clone(),
        flags: code_flags,
        ..BriefRecord::default()
    };
    let mut usage = reply.usage;
    usage.role = "writer".into();
    record.calls.push(usage);
    crate::pr_brief_check::claim_check_and_retry(
        writer,
        model,
        &facts,
        &facts.context,
        &mut brief,
        &mut record,
        &mut log,
        &mut |_| {},
    );
    let stored = StoredBrief {
        version: STORED_VERSION,
        pr: source.meta.number,
        head: source.meta.head.clone(),
        written_at: millis(),
        stale: false,
        model: model.to_string(),
        shape: facts.shape.clone(),
        brief,
        guard: facts.guard.clone(),
        partial_facts: PartialFacts {
            claim_error: record.claim_error.clone(),
            ..facts.partial_facts.clone()
        },
        link_evidence: source.link_evidence.clone(),
    };
    match &source.since {
        Some(from) => store.put_since(from, &stored),
        None => store.put(&stored),
    }
    .map_err(|e| format!("keep the brief of #{}: {e}", stored.pr))?;
    record.at = stored.written_at;
    record.flags_after = stored.brief.all_flags();
    record.unconfirmed = stored
        .brief
        .steps()
        .map(|s| &s.flags)
        .chain(stored.brief.also.iter().map(|a| &a.flags))
        .chain(stored.brief.decisions.iter().map(|d| &d.flags))
        .filter(|f| f.iter().any(|f| f == crate::pr_brief_check::UNCONFIRMED))
        .count();
    if let Err(e) = store.record(&record) {
        log(format!("record the brief of #{}: {e}", stored.pr));
    }
    Ok(stored)
}
