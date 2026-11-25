//! The code checks a reply is held to.

use super::{AlsoGroup, AlsoKind, Brief, Chapter, CheckContext, Decision, Hop, Step, TouchedFile};
use std::collections::{BTreeMap, BTreeSet};
const BANNED: &[&str] = &[
    "guarantee",
    "guarantees",
    "guaranteed",
    "ensure",
    "ensures",
    "ensured",
    "cannot",
    "can't",
    "impossible",
    "always works",
    "never fails",
];
const LEAKS: &[&str] = &[
    "was not sent",
    "were not sent",
    "not sent",
    "was cut",
    "were cut",
    "didn't see",
    "did not see",
    "not shown here",
];

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

/// `text` as quotes are compared: Markdown emphasis, inline backticks and underscores stripped, whitespace folded.
fn norm_quote(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '*' | '`' | '_'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn banned_in(text: &str, law_texts: &[String]) -> Vec<&'static str> {
    let mut t = text.to_lowercase();
    for law in law_texts {
        t = t.replace(law.as_str(), " ");
    }
    let toks: Vec<&str> = t
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .collect();
    let joined = format!(" {} ", toks.join(" "));
    BANNED
        .iter()
        .filter(|b| joined.contains(&format!(" {b} ")))
        .copied()
        .collect()
}

/// The problems of one piece of the writer's own prose.
fn prose_flags(text: &str, ctx: &CheckContext, flags: &mut Vec<String>) {
    for b in banned_in(text, &ctx.law_texts) {
        flags.push(format!("banned word \"{b}\""));
    }
    let lower = text.to_lowercase();
    for l in LEAKS {
        if lower.contains(l) {
            flags.push(format!("mentions input it did not get (\"{l}\")"));
            break;
        }
    }
}

fn step_prose(s: &Step) -> Vec<&str> {
    let mut prose: Vec<&str> = vec![&s.h, &s.b];
    prose.extend(s.list.iter().map(String::as_str));
    for r in &s.risks {
        prose.push(&r.trigger);
        prose.push(&r.check);
    }
    if let Some(c) = &s.check {
        prose.extend([c.q.as_str(), c.yes.as_str(), c.no.as_str()]);
        prose.extend(c.opts.iter().map(String::as_str));
    }
    if let Some(f) = &s.fig {
        prose.extend(f.items.iter().map(|i| i.label.as_str()));
        prose.extend(f.before.iter().chain(&f.after).map(String::as_str));
    }
    prose
}

/// Runs the code checks on one step, replacing its flags. An order check's key is replaced by the
/// changed flow's hops, and one whose parts disagree with them is dropped.
fn check_step(s: &mut Step, ctx: &CheckContext, facts_norm: &str) {
    let mut f: Vec<String> = Vec::new();
    text_flags(s, ctx, &mut f);
    check_flags(s, ctx, &mut f);
    map_flags(s, ctx, &mut f);
    cite_flags(s, facts_norm, &mut f);
    s.flags = f;
}

/// The step's text: its lengths, its prose, its risks and its figure's labels.
fn text_flags(s: &Step, ctx: &CheckContext, f: &mut Vec<String>) {
    if words(&s.h) > 14 {
        f.push(format!("heading {} words", words(&s.h)));
    }
    let body = s.b.replace("<b>", "").replace("</b>", "");
    if words(&body) > 45 {
        f.push(format!("body {} words", words(&body)));
    }
    if s.k == "Your turn" && s.check.is_none() {
        f.push("\"Your turn\" with no check".into());
    }
    let mut prose = Vec::new();
    for p in step_prose(s) {
        prose_flags(p, ctx, &mut prose);
    }
    prose.dedup();
    f.extend(prose);
    if s.risks.len() > 3 {
        f.push("more than 3 risks".into());
    }
    for r in &s.risks {
        if !ctx.parts.contains(&r.part) {
            f.push(format!("risk on `{}`, not a part", r.part));
        }
    }
    if let Some(fig) = &s.fig {
        let labels = fig
            .items
            .iter()
            .map(|i| i.label.as_str())
            .chain(fig.before.iter().chain(&fig.after).map(String::as_str));
        for l in labels {
            if l.chars().count() > 18 {
                f.push(format!("figure label \"{l}\" over 18 characters"));
            }
        }
    }
}

/// The step's check: a pick check's test must be in the diff, an order check's parts must be its
/// changed flow's hops.
fn check_flags(s: &mut Step, ctx: &CheckContext, f: &mut Vec<String>) {
    let Some(c) = &mut s.check else {
        return;
    };
    match c.kind.as_str() {
        "pick" => {
            let test = c.test.clone().unwrap_or_default();
            let name = test
                .rsplit([':', ' '])
                .next()
                .unwrap_or(&test)
                .trim_end_matches("()");
            let diff = if ctx.diff.is_empty() {
                &ctx.facts
            } else {
                &ctx.diff
            };
            if name.is_empty() || !diff.contains(name) {
                f.push(format!("pick check cites test `{test}`, not in the diff"));
            }
        }
        "order" => {
            let flow = c.flow.clone().unwrap_or_default();
            match ctx.flows_changed.get(&flow) {
                None if !ctx.flows.contains(&flow) => {
                    f.push(format!("order check keyed to `{flow}`, not a flow"));
                }
                None => f.push(format!(
                    "order check on flow `{flow}`, which this PR's map diff does not change"
                )),
                Some(hops) => {
                    let theirs: BTreeSet<&str> = c
                        .key
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|k| k.as_str())
                        .collect();
                    let ours: BTreeSet<&str> = hops.iter().map(String::as_str).collect();
                    if theirs == ours {
                        c.key = serde_json::json!(hops);
                    } else {
                        s.check = None;
                        f.push(format!(
                            "order check dropped: its parts disagree with flow `{flow}`'s hops"
                        ));
                    }
                }
            }
        }
        other => f.push(format!("check kind {other:?}")),
    }
}

/// What the step names on the map: the parts and actors it lights, its hops, its law, and the
/// tagged test it says covers that law.
fn map_flags(s: &Step, ctx: &CheckContext, f: &mut Vec<String>) {
    for id in &s.on {
        if !ctx.parts.contains(id) && !ctx.actors.contains(id) {
            f.push(format!("lights `{id}`, not in the map"));
        }
    }
    for Hop(id, _) in &s.hops {
        if !ctx.links.contains(id) {
            f.push(format!("hop `{id}`, not a link"));
        }
    }
    if let Some(id) = &s.law
        && !ctx.laws.contains(id)
    {
        f.push(format!("law `{id}`, not in the map"));
    }
    if let Some(test) = &s.covered_by {
        let want = norm_quote(test);
        let covered = s.law.as_ref().is_some_and(|law| {
            ctx.tagged.iter().any(|t| {
                t.changed && &t.law == law && {
                    let have = norm_quote(&t.test);
                    !want.is_empty()
                        && (have == want
                            || have.contains(&want)
                            || want.contains(&have) && !have.is_empty())
                }
            })
        });
        if !covered {
            f.push(format!(
                "covered_by `{test}` is not a tagged test of the step's law changed in this PR"
            ));
        }
    }
}

/// The step's cites: it has some, and each quote is in the facts.
fn cite_flags(s: &Step, facts_norm: &str, f: &mut Vec<String>) {
    if s.cites.is_empty() {
        f.push("no cites".into());
    }
    let bad: Vec<String> = s
        .cites
        .iter()
        .map(|c| norm_quote(&c.quote))
        .filter(|q| q.is_empty() || !facts_norm.contains(q))
        .collect();
    if !bad.is_empty() {
        f.push(format!(
            "unconfirmed, quote(s) not in the facts: {}",
            bad.iter()
                .map(|q| format!("\"{q}\""))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
}

/// Runs every code check on `brief`, replacing its flags: each failure is a flag on the item it concerns, and
/// the problems of the brief as a whole are on `brief.flags`. An order check's key is replaced by the changed
/// flow's hops. Returns how many items are flagged.
/// Code, not the prompt, puts "Not in the plan" first in its chapter (after the "Where" a single story opens with) when
/// the spec is the PR's own day's. Returns where each step went: `moved[old]` is the new position among all the
/// brief's steps, in the order `steps()` walks them (the overview first).
pub fn put_plan_first(brief: &mut Brief, ctx: &CheckContext) -> Vec<usize> {
    let mut moved: Vec<usize> = (0..brief.steps().count()).collect();
    if !ctx.spec_same_day {
        return moved;
    }
    let mut base = brief.overview.len();
    for c in &mut brief.chapters {
        let first = usize::from(c.steps.first().is_some_and(|s| s.k == "Where"));
        if let Some(at) = c.steps.iter().position(|s| s.k == "Not in the plan")
            && at > first
        {
            let step = c.steps.remove(at);
            c.steps.insert(first, step);
            // The steps from `first` to `at` each moved one place down, and the one at `at` moved to `first`.
            for old in moved
                .iter_mut()
                .filter(|o| (base + first..base + at).contains(o))
            {
                *old += 1;
            }
            moved[base + at] = base + first;
        }
        base += c.steps.len();
    }
    moved
}

/// The longest gist, in words.
const GIST_WORDS: usize = 30;

/// A chapter lighting a Core part (declared or lit) must carry a one-sentence gist; any other chapter's gist is dropped (#397).
fn check_gist(c: &mut Chapter, ctx: &CheckContext) {
    c.gist_flags.clear();
    let core = c
        .parts
        .iter()
        .chain(c.steps.iter().flat_map(|s| &s.on))
        .any(|p| ctx.core.contains(p));
    c.gist = c.gist.trim().to_string();
    if !core {
        c.gist.clear();
        return;
    }
    let gist = c.gist.clone();
    if gist.is_empty() {
        c.gist_flags.push("a Core chapter without a gist".into());
        return;
    }
    if words(&gist) > GIST_WORDS {
        c.gist_flags.push(format!("gist {} words", words(&gist)));
    }
    let body = gist.trim_end_matches(['.', '!', '?']);
    if body.contains(". ") || body.contains("! ") || body.contains("? ") {
        c.gist_flags.push("gist of more than one sentence".into());
    }
    prose_flags(&gist, ctx, &mut c.gist_flags);
}

/// The kind of a file by its path (#477, "Also touched by kind"), the first rule that matches winning: tests, then
/// config/build, then docs. Its test rule is spec #477's, not `logic_file::is_test`'s: no `test_` prefix.
fn path_kind(path: &str) -> Option<AlsoKind> {
    let (dirs, name) = path.rsplit_once('/').unwrap_or(("", path));
    let ext = name.rfind('.').map_or("", |at| &name[at..]);
    let test_dir = dirs
        .split('/')
        .any(|d| matches!(d, "tests" | "test" | "__tests__"));
    let test_name = name.contains(".test.")
        || name.contains(".spec.")
        || name[..name.len() - ext.len()].ends_with("_test");
    const MANIFESTS: &[&str] = &[
        "go.mod",
        "go.sum",
        "Gemfile",
        "Pipfile",
        "requirements.txt",
        "bun.lockb",
    ];
    if test_dir || test_name {
        Some(AlsoKind::Tests)
    } else if path.starts_with(".github/")
        || MANIFESTS.contains(&name)
        || matches!(ext, ".toml" | ".yml" | ".yaml" | ".json" | ".lock")
    {
        Some(AlsoKind::Config)
    } else if path.starts_with("docs/") || ext == ".md" {
        Some(AlsoKind::Docs)
    } else {
        None
    }
}

/// Also touched: `files` that belong to no part in `lit`, in groups by kind, the first that matches winning (No part,
/// tests, config/build, docs, Ignored, other), each group's files sorted by path. Empty groups are left out.
pub fn also_groups(files: &[TouchedFile], lit: &BTreeSet<String>) -> Vec<AlsoGroup> {
    let mut groups: BTreeMap<AlsoKind, Vec<TouchedFile>> = BTreeMap::new();
    for f in files {
        if f.part.as_ref().is_some_and(|p| lit.contains(p)) {
            continue;
        }
        let kind = if f.part.is_none() && !f.ignored {
            AlsoKind::NoPart
        } else if let Some(kind) = path_kind(&f.path) {
            kind
        } else if f.ignored {
            AlsoKind::Ignored
        } else {
            AlsoKind::Other
        };
        groups.entry(kind).or_default().push(f.clone());
    }
    groups
        .into_iter()
        .map(|(kind, mut files)| {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            files.dedup_by(|a, b| a.path == b.path);
            AlsoGroup {
                kind,
                lines: files.iter().map(|f| f.lines).sum(),
                files,
            }
        })
        .collect()
}

pub fn check_brief(brief: &mut Brief, ctx: &CheckContext) -> usize {
    let facts_norm = norm_quote(&ctx.facts);
    brief.flags.clear();
    for s in brief.steps_mut() {
        check_step(s, ctx, &facts_norm);
    }
    let mut whole: Vec<String> = Vec::new();
    if words(&brief.title) > 9 {
        whole.push(format!("title {} words", words(&brief.title)));
    }
    prose_flags(&brief.title, ctx, &mut whole);
    if brief.overview.len() > 2 {
        whole.push(format!("overview of {} steps", brief.overview.len()));
    }
    if !brief.overview.is_empty() && !ctx.overview {
        whole.push("an overview where SHAPE said none".into());
    }
    let n = brief.chapters.len();
    if ctx.chapters_mode && !(2..=4).contains(&n) {
        whole.push(format!("{n} chapters"));
    }
    if !ctx.chapters_mode && n != 1 {
        whole.push(format!("{n} chapters in a single story"));
    }
    put_plan_first(brief, ctx);
    let mut lit_by_chapters: BTreeSet<String> = BTreeSet::new();
    for c in &mut brief.chapters {
        let mut f = Vec::new();
        let lit: BTreeSet<&String> = c.steps.iter().flat_map(|s| &s.on).collect();
        for d in &c.parts {
            if d != "story" && !ctx.parts.contains(d) {
                f.push(format!("declares `{d}`, not a part"));
            } else if d != "story" && !lit.contains(d) {
                f.push(format!("declares `{d}` but no step lights it"));
            }
        }
        let steps = c.steps.len();
        if ctx.chapters_mode && !(3..=4).contains(&steps) {
            f.push(format!("{steps} steps"));
        }
        if !ctx.chapters_mode && steps > 7 {
            f.push(format!("single story: {steps} steps"));
        }
        lit_by_chapters.extend(c.steps.iter().flat_map(|s| s.on.iter().cloned()));
        c.flags = f;
        check_gist(c, ctx);
    }
    brief.also = Vec::new();
    brief.also_groups = also_groups(&ctx.files, &lit_by_chapters);
    for r in &ctx.required {
        if !lit_by_chapters.contains(r) {
            let kept = brief
                .also_groups
                .iter()
                .flat_map(|g| &g.files)
                .any(|f| f.part.as_ref() == Some(r));
            whole.push(format!(
                "required part `{r}` lit by no chapter{}",
                if kept {
                    " (in Also touched: it keeps its fog)"
                } else {
                    ""
                }
            ));
        }
    }
    let total = brief.steps().count();
    if total > 12 {
        whole.push(format!("{total} steps in all"));
    }
    let checks = brief.steps().filter(|s| s.check.is_some()).count();
    if checks > 1 {
        whole.push(format!("{checks} checks"));
    }
    brief.flags = whole;
    for d in &mut brief.decisions {
        d.flags.clear();
        prose_flags(&d.text, ctx, &mut d.flags);
    }
    brief.flagged()
}

/// Gives each decision without an id one: an earlier decision's id when its text is the same, else the next
/// unused `d<n>`.
pub fn assign_decision_ids(brief: &mut Brief, earlier: &[Decision]) {
    let mut used: BTreeSet<String> = brief
        .decisions
        .iter()
        .filter_map(|d| d.id.clone())
        .collect();
    used.extend(earlier.iter().filter_map(|d| d.id.clone()));
    let mut next = 1;
    for d in &mut brief.decisions {
        if d.id.as_ref().is_some_and(|id| {
            earlier.iter().any(|e| e.id.as_ref() == Some(id)) || id.starts_with('d')
        }) {
            continue;
        }
        if let Some(same) = earlier.iter().find(|e| e.text == d.text) {
            d.id = same.id.clone();
            continue;
        }
        while used.contains(&format!("d{next}")) {
            next += 1;
        }
        let id = format!("d{next}");
        used.insert(id.clone());
        d.id = Some(id);
    }
}
