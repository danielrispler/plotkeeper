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

use crate::{
    ClaudeWriter,
    logic_file::{is_logic, is_test},
    millis,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The facts' budget: the writer is sandboxed with no tools, so everything it reads is here.
pub const BUDGET: usize = 64 << 10;
/// The default of `prBriefs.model` in `settings.json`.
pub const DEFAULT_MODEL: &str = "opus";
/// The stored brief's format version; a reader ignores fields it does not know.
pub const STORED_VERSION: u32 = 1;

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
/// Room kept for the "DIFF NOT SENT" section.
const CUT_NOTE_ROOM: usize = 4096;

// ---------------------------------------------------------------------------------------------------------------
// The source: what git and gh give, as plain data.

/// A spec or ticket the PR closes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub number: u64,
    pub title: String,
    pub created_at: String,
    /// The issue's whole body; the facts keep the sections the writer needs.
    pub body: String,
}

/// What `gh` says of a PR.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrMeta {
    pub number: u64,
    pub title: String,
    pub body: String,
    /// The head commit's id.
    pub head: String,
    /// When it merged, empty while open.
    pub merged_at: String,
    /// When it was created (ISO 8601), empty when unknown.
    #[serde(default)]
    pub created_at: String,
    /// One "name conclusion" line per check.
    pub checks: Vec<String>,
    pub spec: Option<Spec>,
}

/// A test that carries a `@law:` tag.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaggedTest {
    pub law: String,
    pub path: String,
    pub test: String,
    /// Whether this PR's diff touches it; set by `build_facts`.
    #[serde(default)]
    pub changed: bool,
}

/// Everything the facts builder reads, gathered.
#[derive(Clone, Debug, Default)]
pub struct PrSource {
    pub meta: PrMeta,
    /// `system-map.json` at the PR's head (or, for a PR older than the map, as of the map's birth).
    pub map_text: String,
    /// The map's diff in this PR, empty when it did not change.
    pub map_diff: String,
    /// A line saying why `map_diff` is empty when the map is older than the PR.
    pub map_note: Option<String>,
    /// `git diff -M base head`.
    pub diff: String,
    /// Every `@law:` tag at the head with its test, `changed` not yet set.
    pub tagged: Vec<TaggedTest>,
    /// Daniel's tier per part where it differs from the map's (`fog.rs`'s effective tiers).
    pub tier_overrides: BTreeMap<String, String>,
    /// The decisions of an earlier brief of this PR, which a rewrite keeps the ids of.
    pub earlier_decisions: Vec<Decision>,
}

// ---------------------------------------------------------------------------------------------------------------
// The brief.

/// A decision the agent declared, for Daniel to accept or overturn. The writer may give a bare sentence.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "RawDecision")]
pub struct Decision {
    /// Assigned by code at the first write.
    pub id: Option<String>,
    pub text: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawDecision {
    Text(String),
    Full {
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        text: String,
        #[serde(default)]
        flags: Vec<String>,
    },
}

impl From<RawDecision> for Decision {
    fn from(raw: RawDecision) -> Self {
        match raw {
            RawDecision::Text(text) => Decision {
                id: None,
                text,
                flags: vec![],
            },
            RawDecision::Full { id, text, flags } => Decision { id, text, flags },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Cite {
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub quote: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RiskItem {
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub trigger: String,
    #[serde(default)]
    pub check: String,
}

/// A link of a light's path and whether it runs backwards.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Hop(pub String, #[serde(default)] pub bool);

/// A figure's item; the kinds use the fields they need.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FigItem {
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub tone: Option<String>,
    #[serde(default)]
    pub bad: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Figure {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub frame: Option<String>,
    #[serde(default)]
    pub items: Vec<FigItem>,
    #[serde(default)]
    pub before: Vec<String>,
    #[serde(default)]
    pub after: Vec<String>,
}

/// The one check of a brief: `pick` (a case inside one part) or `order` (the parts of a flow, in order).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BriefCheck {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub q: String,
    #[serde(default)]
    pub opts: Vec<String>,
    /// A pick's index, or an order's part ids.
    #[serde(default)]
    pub key: serde_json::Value,
    #[serde(default)]
    pub yes: String,
    #[serde(default)]
    pub no: String,
    #[serde(default)]
    pub flow: Option<String>,
    #[serde(default)]
    pub test: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// Where, Before, Now, Must stay true, Could break, Not in the plan or Your turn.
    #[serde(default)]
    pub k: String,
    #[serde(default)]
    pub h: String,
    #[serde(default)]
    pub b: String,
    #[serde(default)]
    pub on: Vec<String>,
    #[serde(default)]
    pub hops: Vec<Hop>,
    #[serde(default)]
    pub law: Option<String>,
    /// The tagged test, changed in this PR, that covers the step's law.
    #[serde(default)]
    pub covered_by: Option<String>,
    #[serde(default)]
    pub list: Vec<String>,
    #[serde(default)]
    pub risks: Vec<RiskItem>,
    #[serde(default)]
    pub fig: Option<Figure>,
    #[serde(default)]
    pub check: Option<BriefCheck>,
    #[serde(default)]
    pub cites: Vec<Cite>,
    /// What the code checks (and, later, the claim check) found wrong with this step.
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    #[serde(default)]
    pub behaviour: String,
    #[serde(default)]
    pub parts: Vec<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Also {
    #[serde(default)]
    pub part: String,
    #[serde(default)]
    pub line: String,
    #[serde(default)]
    pub flags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub overview: Vec<Step>,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    #[serde(default)]
    pub also: Vec<Also>,
    #[serde(default)]
    pub unseen: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    /// Problems of the brief as a whole (chapter count, a required part no chapter lights).
    #[serde(default)]
    pub flags: Vec<String>,
}

impl Brief {
    /// Every step, overview first.
    pub fn steps(&self) -> impl Iterator<Item = &Step> {
        self.overview
            .iter()
            .chain(self.chapters.iter().flat_map(|c| &c.steps))
    }

    pub(crate) fn steps_mut(&mut self) -> impl Iterator<Item = &mut Step> {
        self.overview
            .iter_mut()
            .chain(self.chapters.iter_mut().flat_map(|c| &mut c.steps))
    }

    /// How many items carry a flag.
    pub fn flagged(&self) -> usize {
        self.steps().filter(|s| !s.flags.is_empty()).count()
            + self.chapters.iter().filter(|c| !c.flags.is_empty()).count()
            + self.also.iter().filter(|a| !a.flags.is_empty()).count()
            + self
                .decisions
                .iter()
                .filter(|d| !d.flags.is_empty())
                .count()
            + usize::from(!self.flags.is_empty())
    }

    /// Every flag, with where it is, for the record.
    pub fn all_flags(&self) -> Vec<String> {
        let mut out: Vec<String> = self.flags.clone();
        for c in &self.chapters {
            out.extend(
                c.flags
                    .iter()
                    .map(|f| format!("chapter {}: {f}", c.behaviour)),
            );
        }
        for (i, s) in self.overview.iter().enumerate() {
            out.extend(s.flags.iter().map(|f| format!("overview {}: {f}", i + 1)));
        }
        for c in &self.chapters {
            for (i, s) in c.steps.iter().enumerate() {
                out.extend(
                    s.flags
                        .iter()
                        .map(|f| format!("{} {}: {f}", c.behaviour, i + 1)),
                );
            }
        }
        out.extend(
            self.also
                .iter()
                .flat_map(|a| a.flags.iter().map(|f| format!("also {}: {f}", a.part))),
        );
        out.extend(
            self.decisions
                .iter()
                .flat_map(|d| d.flags.iter().map(|f| format!("decision: {f}"))),
        );
        out
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The facts.

/// What the facts builder knows that the checks need.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CheckContext {
    /// The facts the writer was given: every quote must be in them.
    #[serde(default)]
    pub facts: String,
    /// The whole diff, which a pick check's test must be in. Not stored in samples; they use the facts.
    #[serde(default)]
    pub diff: String,
    pub chapters_mode: bool,
    /// The parts the chapters (or the single story) must light.
    pub required: Vec<String>,
    /// The parts that only get a line in "also".
    pub also: Vec<String>,
    pub overview: bool,
    pub parts: BTreeSet<String>,
    pub actors: BTreeSet<String>,
    pub links: BTreeSet<String>,
    pub laws: BTreeSet<String>,
    pub flows: BTreeSet<String>,
    /// Each law's lowercased map text, which the banned-word check leaves out.
    pub law_texts: Vec<String>,
    /// The flows the map's diff changes, with the parts their hops run through in order.
    pub flows_changed: BTreeMap<String, Vec<String>>,
    pub tagged: Vec<TaggedTest>,
    /// The spec was created the same day as the PR: its "Not in the plan" step comes first in its chapter.
    #[serde(default)]
    pub spec_same_day: bool,
}

/// How the budget was spent, for the record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetRecord {
    pub total_bytes: usize,
    pub sections: Vec<(String, usize)>,
    pub diff_bytes_full: usize,
    pub files_sent: usize,
    /// Files with some hunks sent and some not.
    pub files_partial: usize,
    pub files_cut: usize,
}

/// The shape code decided.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub chapters: bool,
    pub required: Vec<String>,
    pub also: Vec<String>,
    pub overview: bool,
    pub flows_changed: Vec<String>,
}

impl Shape {
    pub fn label(&self) -> String {
        if self.chapters {
            format!("chapters over {}", self.required.join(", "))
        } else {
            "single story".into()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub text: String,
    pub shape: Shape,
    pub budget: BudgetRecord,
    pub context: CheckContext,
}

struct FileDiff {
    path: String,
    header: String,
    hunks: Vec<String>,
    added: usize,
    removed: usize,
}

fn split_diff(diff: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    for chunk in diff.split("\ndiff --git ").filter(|c| !c.is_empty()) {
        let chunk = chunk.strip_prefix("diff --git ").unwrap_or(chunk);
        let path = chunk
            .lines()
            .next()
            .and_then(|l| l.split(" b/").last())
            .unwrap_or("")
            .to_string();
        let mut parts = chunk.split("\n@@");
        let header = format!("diff --git {}\n", parts.next().unwrap_or(""));
        let hunks: Vec<String> = parts.map(|h| format!("@@{h}\n")).collect();
        let (mut added, mut removed) = (0, 0);
        for h in &hunks {
            for l in h.lines().skip(1) {
                if l.starts_with('+') {
                    added += 1;
                } else if l.starts_with('-') {
                    removed += 1;
                }
            }
        }
        files.push(FileDiff {
            path,
            header,
            hunks,
            added,
            removed,
        });
    }
    files
}

/// The `@law:` tags the PR added, each with its file and the next added line that names a test.
fn added_tags(files: &[FileDiff]) -> Vec<String> {
    let mut tags = Vec::new();
    for f in files {
        for h in &f.hunks {
            let added: Vec<&str> = h.lines().filter(|l| l.starts_with('+')).collect();
            for (i, l) in added.iter().enumerate() {
                if let Some(at) = l.find("@law:") {
                    let id = l[at + 5..].split_whitespace().next().unwrap_or("");
                    let test = added[i + 1..]
                        .iter()
                        .find(|n| n.contains("fn ") || n.contains("test("))
                        .map(|n| n.trim_start_matches('+').trim())
                        .unwrap_or("(no test line found)");
                    tags.push(format!("- law `{id}` in {}: {test}", f.path));
                }
            }
        }
    }
    tags
}

/// Whether two ISO 8601 timestamps fall on the same calendar day (their date parts, as GitHub gives them in UTC); false when either
/// is missing.
fn same_day(a: &str, b: &str) -> bool {
    let day = |t: &str| {
        t.get(..10)
            .filter(|d| d.as_bytes().get(4) == Some(&b'-'))
            .map(str::to_string)
    };
    matches!((day(a), day(b)), (Some(a), Some(b)) if a == b)
}

/// The spec's sections the writer gets: a spec's User Stories and Implementation Decisions, a ticket's What to
/// build and Acceptance criteria.
pub fn trim_spec(body: &str) -> String {
    const KEEP: &[&str] = &[
        "User Stories",
        "Implementation Decisions",
        "What to build",
        "Acceptance criteria",
    ];
    let mut out = String::new();
    let mut keep = false;
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            keep = KEEP.iter().any(|k| h.trim().eq_ignore_ascii_case(k));
        }
        if keep {
            out += line;
            out += "\n";
        }
    }
    out
}

fn section(name: &str, text: &str) -> String {
    format!("=== {name} ===\n{}\n\n", text.trim_end())
}

fn ids(map: &serde_json::Value, key: &str) -> BTreeSet<String> {
    map[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x["id"].as_str().map(String::from))
        .collect()
}

/// `text` cut to `max` bytes at a character boundary, with a note of what was cut.
fn cut_to(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let end = text.floor_char_boundary(max);
    format!("{}\n[{} more bytes cut]", &text[..end], text.len() - end)
}

/// The parts a flow's hops run through, each once, in the order the flow first reaches it; actors are left out. A hop
/// that leaves the part the flow is at through a link's `to` end is read backwards. A part the flow revisits is not
/// repeated: an order check taps each part once, so a key that repeated one could not be answered.
fn flow_key(
    map: &serde_json::Value,
    flow: &serde_json::Value,
    parts: &BTreeSet<String>,
) -> Vec<String> {
    let mut key: Vec<String> = Vec::new();
    for hop in flow["hops"].as_array().into_iter().flatten() {
        let link = map["links"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|l| l["id"] == hop["link"]);
        if let Some(link) = link {
            let (from, to) = (
                link["from"].as_str().unwrap_or(""),
                link["to"].as_str().unwrap_or(""),
            );
            let ends = if key.last().map(String::as_str) == Some(to) {
                [to, from]
            } else {
                [from, to]
            };
            for id in ends {
                if parts.contains(id) && !key.iter().any(|k| k == id) {
                    key.push(id.to_string());
                }
            }
        }
    }
    key
}

/// Assembles the writer's facts: structure first, then the diff hunk by hunk inside `BUDGET`.
pub fn build_facts(source: &PrSource) -> Result<Facts, String> {
    let map = system_map::parse_lenient(&source.map_text)?;
    let mj: serde_json::Value =
        serde_json::from_str(&source.map_text).map_err(|e| e.to_string())?;
    let files = split_diff(&source.diff);
    let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let touched = system_map::touched_for_map(&map, &paths);
    let file = |path: &str| files.iter().find(|f| f.path == path);
    let size = |path: &str| file(path).map_or(0, |f| f.added + f.removed);
    let part_of = |path: &str| {
        touched
            .parts
            .iter()
            .find(|p| p.files.iter().any(|f| f == path))
            .map(|p| p.id.clone())
    };
    let kind = |path: &str| {
        if is_test(path) {
            "test"
        } else if is_logic(path) {
            "core"
        } else {
            "plumbing"
        }
    };
    let part_json = |id: &str| {
        mj["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == id)
            .cloned()
            .unwrap_or_default()
    };
    let links_of = |id: &str| -> Vec<String> {
        mj["links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["from"] == id || l["to"] == id)
            .filter_map(|l| l["id"].as_str().map(String::from))
            .collect()
    };
    let map_diff = if !source.map_diff.is_empty() {
        source.map_diff.clone()
    } else {
        source.map_note.clone().unwrap_or_else(|| "none".into())
    };

    // Which touched parts would fog, and the shape.
    let mut fogging: Vec<(String, usize)> = Vec::new();
    let mut quiet: Vec<String> = Vec::new();
    for p in &touched.parts {
        let pj = part_json(&p.id);
        let tier = source
            .tier_overrides
            .get(&p.id)
            .cloned()
            .unwrap_or_else(|| pj["tier"].as_str().unwrap_or("").to_string());
        let named = map_diff.contains(&format!("\"{}\"", p.id));
        let logic: usize = p
            .files
            .iter()
            .filter(|f| kind(f) == "core")
            .map(|f| size(f))
            .sum();
        if tier == "core" || (tier == "interface" && named) {
            fogging.push((p.id.clone(), logic));
        } else {
            quiet.push(p.id.clone());
        }
    }
    fogging.sort_by_key(|a| std::cmp::Reverse(a.1));
    let chapters_mode = fogging.len() > 2;
    let required: Vec<String> = fogging.iter().map(|p| p.0.clone()).collect();
    let all_parts = ids(&mj, "parts");
    let flows_changed: Vec<String> = ids(&mj, "flows")
        .into_iter()
        .filter(|f| map_diff.contains(&format!("\"{f}\"")))
        .collect();
    let overview = chapters_mode && !flows_changed.is_empty();
    let also = quiet;
    let also_text = if also.is_empty() {
        "none".to_string()
    } else {
        also.join(", ")
    };
    let shape_text = if chapters_mode {
        format!(
            "chapters by behaviour: 2 to 4 chapters, each one behaviour, lighting every part it crosses\nrequired parts (together the chapters must light every one): {}\noverview: {}\nalso-touched (one line each, no chapter): {also_text}",
            required.join(", "),
            if overview {
                format!("yes, for flow(s) {}", flows_changed.join(", "))
            } else {
                "no".into()
            },
        )
    } else {
        format!(
            "single story (one chapter with part \"story\") about: {}\noverview: no\nalso-touched (one line each): {also_text}",
            required.join(", "),
        )
    };

    // The tagged tests, and whether this PR's diff changed one.
    let mut tagged = source.tagged.clone();
    for t in &mut tagged {
        t.changed = file(&t.path).is_some_and(|fd| {
            fd.hunks.iter().any(|h| {
                (!t.test.is_empty() && h.contains(&t.test))
                    || h.contains(&format!("@law: {}", t.law))
            })
        });
    }

    // The parts table.
    let mut table = String::new();
    for p in &touched.parts {
        let pj = part_json(&p.id);
        let links = links_of(&p.id);
        let laws: Vec<String> = mj["laws"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["on"] == p.id.as_str() || links.iter().any(|k| l["on"] == k.as_str()))
            .map(|l| {
                let id = l["id"].as_str().unwrap_or("");
                let tests: Vec<String> = tagged
                    .iter()
                    .filter(|t| t.law == id)
                    .map(|t| {
                        format!(
                            "{} in {} ({})",
                            t.test,
                            t.path,
                            if t.changed {
                                "changed in this PR"
                            } else {
                                "not changed in this PR"
                            }
                        )
                    })
                    .collect();
                format!(
                    "{} ({}): {} [tagged tests: {}]",
                    id,
                    l["backing"].as_str().unwrap_or(""),
                    l["text"].as_str().unwrap_or(""),
                    if tests.is_empty() {
                        "none".into()
                    } else {
                        tests.join("; ")
                    }
                )
            })
            .collect();
        let flows: Vec<String> = mj["flows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| {
                f["hops"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|h| links.iter().any(|k| h["link"] == k.as_str()))
            })
            .filter_map(|f| f["id"].as_str().map(String::from))
            .collect();
        let mut fs: Vec<&String> = p.files.iter().collect();
        fs.sort_by_key(|f| {
            (
                ["core", "test", "plumbing"]
                    .iter()
                    .position(|k| *k == kind(f)),
                std::cmp::Reverse(size(f)),
            )
        });
        let tier = source
            .tier_overrides
            .get(&p.id)
            .map(String::as_str)
            .or(pj["tier"].as_str())
            .unwrap_or("");
        table += &format!(
            "## {} ({}, tier {tier})\n",
            p.id,
            pj["name"].as_str().unwrap_or("")
        );
        table += &format!(
            "laws:\n  {}\nflows through it: {}\n",
            if laws.is_empty() {
                "none".into()
            } else {
                laws.join("\n  ")
            },
            if flows.is_empty() {
                "none".into()
            } else {
                flows.join(", ")
            }
        );
        for f in fs {
            let d = file(f);
            table += &format!(
                "- {} {} +{}/-{}\n",
                kind(f),
                f,
                d.map_or(0, |d| d.added),
                d.map_or(0, |d| d.removed)
            );
        }
        table += "\n";
    }
    if !touched.ignored.is_empty() || !touched.unowned.is_empty() {
        table += &format!(
            "noise (ignored or unowned paths, not the running system): {} files\n",
            touched.ignored.len() + touched.unowned.len()
        );
    }

    let meta = &source.meta;
    let spec_same_day = meta
        .spec
        .as_ref()
        .is_some_and(|s| same_day(&s.created_at, &meta.created_at));
    let spec_text = match &meta.spec {
        Some(s) => format!(
            "#{} {}\ncreated {}{}\n(trimmed to its user stories and implementation decisions)\n\n{}",
            s.number,
            s.title,
            s.created_at,
            if spec_same_day {
                " (the same day as the PR)"
            } else {
                ""
            },
            trim_spec(&s.body)
        ),
        None => "none".into(),
    };
    let tags = added_tags(&files);
    let mut sections: Vec<(String, String)> = vec![
        ("SHAPE (decided by code)".into(), shape_text),
        ("SYSTEM MAP".into(), source.map_text.clone()),
        ("MAP DIFF IN THIS PR".into(), map_diff),
        ("PARTS TABLE".into(), table),
        (
            "LAW TAGS ADDED".into(),
            if tags.is_empty() {
                "none".into()
            } else {
                tags.join("\n")
            },
        ),
        (
            "CHECKS".into(),
            if meta.checks.is_empty() {
                "none recorded".into()
            } else {
                meta.checks.join("\n")
            },
        ),
        (
            "PR".into(),
            format!(
                "#{} {}\n{}{}\n\n{}",
                meta.number,
                meta.title,
                if meta.merged_at.is_empty() {
                    "open".to_string()
                } else {
                    format!("merged {}", meta.merged_at)
                },
                if meta.created_at.is_empty() {
                    String::new()
                } else {
                    format!("\ncreated {}", meta.created_at)
                },
                meta.body
            ),
        ),
        ("SPEC".into(), spec_text),
    ];
    if !source.earlier_decisions.is_empty() {
        let lines: Vec<String> = source
            .earlier_decisions
            .iter()
            .map(|d| format!("- {}: {}", d.id.as_deref().unwrap_or("?"), d.text))
            .collect();
        sections.push(("EARLIER DECISIONS".into(), lines.join("\n")));
    }
    // The structure alone must leave room for the diff's note: cut the PR body, then the spec, if it does not.
    let used = |sections: &[(String, String)]| {
        sections
            .iter()
            .map(|(n, t)| section(n, t).len())
            .sum::<usize>()
    };
    for name in ["SPEC", "PR"] {
        let over = (used(&sections) + CUT_NOTE_ROOM).saturating_sub(BUDGET);
        if over == 0 {
            break;
        }
        if let Some((_, text)) = sections.iter_mut().find(|(n, _)| n == name) {
            *text = cut_to(text, text.len().saturating_sub(over));
        }
    }
    let used_bytes = used(&sections);

    // The diff: core files of the required parts, then other parts' core, then tests, then the rest.
    let rank = |f: &FileDiff| {
        let part = part_of(&f.path);
        let in_required = part.as_ref().is_some_and(|p| required.contains(p));
        match (in_required, part.is_some(), kind(&f.path)) {
            (true, _, "core") => 0,
            (_, true, "core") => 1,
            (true, _, "test") => 2,
            (_, true, "test") => 3,
            (_, true, _) => 4,
            _ => 5,
        }
    };
    let mut order: Vec<&FileDiff> = files.iter().collect();
    order.sort_by_key(|f| (rank(f), f.added + f.removed));
    let mut room = BUDGET.saturating_sub(used_bytes + CUT_NOTE_ROOM);
    let mut diff_text = String::new();
    let mut cut: Vec<String> = Vec::new();
    let mut budget = BudgetRecord {
        diff_bytes_full: source.diff.len(),
        ..BudgetRecord::default()
    };
    for f in order {
        let mut sent = 0;
        let mut text = String::new();
        if f.header.len() < room {
            text += &f.header;
            for h in &f.hunks {
                if text.len() + h.len() <= room {
                    text += h;
                    sent += 1;
                } else {
                    break;
                }
            }
        }
        let whole = sent == f.hunks.len() && !text.is_empty();
        if sent > 0 || (f.hunks.is_empty() && !text.is_empty()) {
            room -= text.len();
            diff_text += &text;
        }
        let part = part_of(&f.path).unwrap_or_else(|| "(noise)".into());
        if whole {
            budget.files_sent += 1;
        } else {
            if sent > 0 {
                budget.files_partial += 1;
            } else {
                budget.files_cut += 1;
            }
            cut.push(format!(
                "- {} ({part}): +{}/-{}, {} of {} hunks not sent",
                f.path,
                f.added,
                f.removed,
                f.hunks.len() - sent,
                f.hunks.len()
            ));
        }
    }
    let cut_text = if cut.is_empty() {
        "nothing: the whole diff is below".to_string()
    } else {
        let mut t = format!(
            "{} files not fully sent; do not guess what they hold:\n",
            cut.len()
        );
        for (i, line) in cut.iter().enumerate() {
            if t.len() + line.len() + 1 > CUT_NOTE_ROOM - 200 {
                t += &format!("- ... and {} more\n", cut.len() - i);
                break;
            }
            t += line;
            t += "\n";
        }
        t
    };
    sections.push(("DIFF NOT SENT".into(), cut_text));
    sections.push(("DIFF".into(), diff_text));
    let text: String = sections.iter().map(|(n, t)| section(n, t)).collect();
    budget.total_bytes = text.len();
    budget.sections = sections
        .iter()
        .map(|(n, t)| (n.clone(), section(n, t).len()))
        .collect();

    let mut flow_keys = BTreeMap::new();
    for flow in mj["flows"].as_array().into_iter().flatten() {
        let id = flow["id"].as_str().unwrap_or("");
        if flows_changed.iter().any(|f| f == id) {
            flow_keys.insert(id.to_string(), flow_key(&mj, flow, &all_parts));
        }
    }
    let shape = Shape {
        chapters: chapters_mode,
        required: required.clone(),
        also: also.clone(),
        overview,
        flows_changed: flows_changed.clone(),
    };
    let context = CheckContext {
        facts: text.clone(),
        diff: source.diff.clone(),
        chapters_mode,
        required,
        also,
        overview,
        parts: all_parts,
        actors: ids(&mj, "actors"),
        links: ids(&mj, "links"),
        laws: ids(&mj, "laws"),
        flows: ids(&mj, "flows"),
        law_texts: mj["laws"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["text"].as_str().map(str::to_lowercase))
            .collect(),
        flows_changed: flow_keys,
        tagged,
        spec_same_day,
    };
    Ok(Facts {
        text,
        shape,
        budget,
        context,
    })
}

// ---------------------------------------------------------------------------------------------------------------
// The prompt.

pub(crate) const INSTRUCTIONS: &str = r#"You write a PR brief: a short, stepped, plain-language explanation of what one pull request changed in a system, for the system's owner. He directs AI agents that write the code; he does not read code. The brief exists to keep his mental model accurate: what each part does now, how behaviour flows between parts, what must stay true, what could break. It is not a changelog and not a file tour. The brief plays on a map of the system: each step lights parts and may send a light along links.

You get FACTS below. The code has already decided the brief's SHAPE (single story or chapters, and which parts must be covered); follow it exactly.

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
  "chapters": [ { "behaviour": "the behaviour, at most 4 words (or \"story\" for a single story)", "parts": [ "part ids this chapter covers" ], "title": "at most 6 words", "steps": [ step, ... ] } ],
  "also": [ { "part": "part id from SHAPE's also-touched list, or a required part no chapter could light", "line": "one line, what changed in it" } ],
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

Figure kinds (code draws them; each label at most 18 characters):
- {"kind": "states", "frame": "what holds them", "items": [ {"label": "...", "tone": "live" | "lost" | "plain"} ]}  up to 3 items.
- {"kind": "sequence", "items": [ {"label": "...", "bad": false} ]}  up to 4 steps left to right; "bad": true on the one that goes wrong.
- {"kind": "compare", "before": [ "line" ], "after": [ "line" ]}  up to 3 lines each.

Checks (at most one per brief; omit when none fits):
- A change to a flow: {"kind": "order", "q": "...", "key": [ part ids in order ], "yes": "...", "no": "...", "flow": "flow-id"}. Only when the map's diff changes that flow's hops or a law on it.
- A change inside one part: {"kind": "pick", "q": "a concrete case to predict", "opts": [ "...", "..." ], "key": index, "yes": "...", "no": "...", "test": "the exact name of a test this PR added or changed that decides the answer"}. Only when such a test is in the diff.
"#;

/// The prompt for `facts`.
pub fn prompt(facts: &Facts) -> String {
    format!("{INSTRUCTIONS}\nFACTS\n\n{}", facts.text)
}

// ---------------------------------------------------------------------------------------------------------------
// Reading a reply.

/// What a call cost.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CallUsage {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cost_usd: f64,
    /// The call's wall time, from asking the writer to its reply (danielrispler/plotkeeper#334, port decision Q5).
    #[serde(default)]
    pub duration_ms: u64,
}

/// A writer's reply: the text, and what the call cost when it says.
#[derive(Clone, Debug, Default)]
pub struct BriefReply {
    pub text: String,
    pub usage: CallUsage,
}

/// What writes a brief: `ClaudeWriter` in the app, a canned reply in a test.
pub trait BriefWriter {
    /// The reply to `prompt` at `model`, or one line for the log when the call failed.
    fn write(&self, model: &str, prompt: &str) -> Result<BriefReply, String>;
}

/// `writer`'s reply to `prompt` at `model`, with the call's wall time (`duration_ms`) set on its usage.
pub fn timed_write(
    writer: &(impl BriefWriter + ?Sized),
    model: &str,
    prompt: &str,
) -> Result<BriefReply, String> {
    let started = std::time::Instant::now();
    let mut reply = writer.write(model, prompt)?;
    reply.usage.duration_ms = started.elapsed().as_millis() as u64;
    Ok(reply)
}

/// Reads `claude -p --output-format json`'s output: the `result` text and the usage. A reply that is not that
/// envelope is taken as the text itself.
pub fn read_envelope(output: &str) -> Result<BriefReply, String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(output) else {
        return Ok(BriefReply {
            text: output.to_string(),
            usage: CallUsage::default(),
        });
    };
    let Some(result) = v.get("result").and_then(|r| r.as_str()) else {
        return Ok(BriefReply {
            text: output.to_string(),
            usage: CallUsage::default(),
        });
    };
    if v["is_error"].as_bool() == Some(true) {
        return Err(format!(
            "`claude -p` reported an error: {}",
            cut_to(result, 300)
        ));
    }
    let usage = CallUsage {
        role: String::new(),
        model: v["modelUsage"]
            .as_object()
            .map(|m| m.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_default(),
        input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0)
            + v["usage"]["cache_creation_input_tokens"]
                .as_u64()
                .unwrap_or(0)
            + v["usage"]["cache_read_input_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
        cost_usd: v["total_cost_usd"].as_f64().unwrap_or(0.0),
        duration_ms: 0,
    };
    Ok(BriefReply {
        text: result.to_string(),
        usage,
    })
}

impl BriefWriter for ClaudeWriter {
    fn write(&self, model: &str, prompt: &str) -> Result<BriefReply, String> {
        let out = self.brief_stdout(model, prompt)?;
        read_envelope(&out)
    }
}

/// The brief in a writer's reply text (or in the whole `claude` envelope): the first `{` to the last `}`.
pub fn parse_reply(text: &str) -> Result<Brief, String> {
    let text = read_envelope(text)?.text;
    let (a, b) = text
        .find('{')
        .zip(text.rfind('}'))
        .filter(|(a, b)| a < b)
        .ok_or("the reply holds no JSON object")?;
    serde_json::from_str(&text[a..=b]).map_err(|e| format!("the reply's JSON does not read: {e}"))
}

// ---------------------------------------------------------------------------------------------------------------
// The code checks.

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

fn check_step(s: &mut Step, ctx: &CheckContext, facts_norm: &str) {
    let mut f: Vec<String> = Vec::new();
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
    if let Some(c) = &mut s.check {
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
                        f.push(format!("order check keyed to `{flow}`, not a flow"))
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
    s.flags = f;
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
    }
    for r in &ctx.required {
        if !lit_by_chapters.contains(r) {
            let kept = brief.also.iter().any(|a| &a.part == r);
            whole.push(format!(
                "required part `{r}` lit by no chapter{}",
                if kept {
                    " (in also: it keeps its fog)"
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
    for a in &mut brief.also {
        a.flags.clear();
        if !ctx.also.contains(&a.part) && !ctx.required.contains(&a.part) {
            a.flags
                .push(format!("also-touched `{}`, not on code's list", a.part));
        }
        prose_flags(&a.line, ctx, &mut a.flags);
    }
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

// ---------------------------------------------------------------------------------------------------------------
// Keeping briefs and records.

/// A brief as stored: per PR and head, in the data folder. A reader ignores fields it does not know, and a
/// missing one has its default, so an older or newer build reads it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StoredBrief {
    #[serde(default)]
    pub version: u32,
    pub pr: u64,
    pub head: String,
    #[serde(default)]
    pub written_at: u64,
    /// A newer head was pushed: rewritten when opened (#334).
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub brief: Brief,
}

/// One line per brief written, for tuning prompts, caps and models from real data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BriefRecord {
    pub pr: u64,
    pub head: String,
    pub at: u64,
    pub shape: String,
    pub budget: BudgetRecord,
    /// The code checks' flags.
    #[serde(default)]
    pub flags: Vec<String>,
    /// Items still flagged after the retry, each carrying "not confirmed from the facts".
    #[serde(default)]
    pub unconfirmed: usize,
    /// The claim check's flags, before the retry.
    #[serde(default)]
    pub claim_flags_before: Vec<String>,
    /// The claim check's flags that still stand after the retry (those of items the retry did not rewrite).
    #[serde(default)]
    pub claim_flags_after: Vec<String>,
    /// The flags the stored brief carries in the end.
    #[serde(default)]
    pub flags_after: Vec<String>,
    /// Why the claim check or the retry gave nothing (a failed call or a reply that does not read).
    #[serde(default)]
    pub claim_error: Option<String>,
    #[serde(default)]
    pub retry_error: Option<String>,
    /// The writer's call, the claim check's and the retry's, in that order, each when it was made.
    #[serde(default)]
    pub calls: Vec<CallUsage>,
}

/// The briefs and their records in the studio's data folder.
pub struct BriefStore {
    dir: PathBuf,
}

impl BriefStore {
    /// The store in `data`, a folder `pr-briefs/` and the file `pr-brief-records.jsonl` inside it.
    pub fn open(data: &Path) -> std::io::Result<Self> {
        let dir = data.join("pr-briefs");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, pr: u64, head: &str) -> PathBuf {
        let head: String = head.chars().filter(char::is_ascii_alphanumeric).collect();
        self.dir.join(format!("{pr}-{head}.json"))
    }

    /// The brief of `pr` at `head`.
    pub fn get(&self, pr: u64, head: &str) -> Option<StoredBrief> {
        serde_json::from_slice(&std::fs::read(self.path(pr, head)).ok()?).ok()
    }

    /// The newest brief of `pr`, whichever head.
    pub fn latest(&self, pr: u64) -> Option<StoredBrief> {
        let prefix = format!("{pr}-");
        std::fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .filter_map(|e| {
                serde_json::from_slice::<StoredBrief>(&std::fs::read(e.path()).ok()?).ok()
            })
            .filter(|b| b.pr == pr)
            .max_by_key(|b| b.written_at)
    }

    /// Writes `brief`, through a temporary file so a crash leaves the old one.
    pub fn put(&self, brief: &StoredBrief) -> std::io::Result<()> {
        let path = self.path(brief.pr, &brief.head);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(brief).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }

    /// Marks the brief of `pr` at `head` stale; false when there is none.
    pub fn mark_stale(&self, pr: u64, head: &str) -> std::io::Result<bool> {
        match self.get(pr, head) {
            Some(mut b) => {
                b.stale = true;
                self.put(&b).map(|()| true)
            }
            None => Ok(false),
        }
    }

    /// Appends one record line.
    pub fn record(&self, record: &BriefRecord) -> std::io::Result<()> {
        let mut line = serde_json::to_string(record).map_err(std::io::Error::other)?;
        line.push('\n');
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("pr-brief-records.jsonl"))?
            .write_all(line.as_bytes())
    }

    /// The records written so far, oldest first.
    pub fn records(&self) -> Vec<BriefRecord> {
        let text =
            std::fs::read_to_string(self.dir.join("pr-brief-records.jsonl")).unwrap_or_default();
        text.lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The entry.

/// Writes the brief of `source`'s PR at its head: builds the facts, asks the writer once, parses and checks the
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
    let reply = timed_write(writer, model, &prompt(&facts))?;
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
    };
    store
        .put(&stored)
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

// ---------------------------------------------------------------------------------------------------------------
// Gathering: git and gh.

fn run(program: &str, dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program} did not run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The name of the test a `@law:` tag sits above, from `body`'s lines after line `n` (1-based).
fn tagged_test_name(body: &str, n: usize) -> String {
    let test = body
        .lines()
        .skip(n)
        .find(|l| l.contains("fn ") || l.contains("test("))
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    let quoted = test
        .split(['(', '"', '\''])
        .find(|w| w.trim().len() > 6 && !w.contains("fn ") && !w.contains("test"))
        .map(|w| w.trim().to_string());
    quoted
        .or_else(|| {
            test.split("fn ")
                .nth(1)
                .map(|t| t.split('(').next().unwrap_or("").trim().to_string())
        })
        .unwrap_or(test)
}

/// What `git` gives for the PR whose changes run from `base` to `head` in `repo`: the map as of `head` (or its
/// birth for a PR older than the map), its diff, the whole diff and the tagged tests. `meta` is `fetch_meta`'s.
pub fn gather(repo: &Path, base: &str, head: &str, meta: PrMeta) -> Result<PrSource, String> {
    let has_map = run(
        "git",
        repo,
        &[
            "cat-file",
            "-e",
            &format!("{head}:{}", system_map::MAP_FILE),
        ],
    )
    .is_ok();
    let (map_text, map_diff, map_note) = if has_map {
        let text = run(
            "git",
            repo,
            &["show", &format!("{head}:{}", system_map::MAP_FILE)],
        )?;
        let diff = run(
            "git",
            repo,
            &["diff", base, head, "--", system_map::MAP_FILE],
        )?;
        (text, diff, None)
    } else {
        let birth = run(
            "git",
            repo,
            &[
                "log",
                "--diff-filter=A",
                "--format=%H",
                "-1",
                "--",
                system_map::MAP_FILE,
            ],
        )?;
        let birth = birth.trim();
        if birth.is_empty() {
            return Err("the repository has no system map".into());
        }
        let text = run(
            "git",
            repo,
            &["show", &format!("{birth}:{}", system_map::MAP_FILE)],
        )?;
        let note = format!(
            "none: this PR is older than the map; the map shown is as of its birth ({})",
            &birth[..birth.len().min(9)]
        );
        (text, String::new(), Some(note))
    };
    let diff = run("git", repo, &["diff", "-M", base, head])?;
    let laws: BTreeSet<String> = serde_json::from_str::<serde_json::Value>(&map_text)
        .map(|m| ids(&m, "laws"))
        .unwrap_or_default();
    let mut tagged = Vec::new();
    // `git grep` exits 1 when nothing matches.
    let grep = Command::new("git")
        .args(["grep", "-n", "@law:", head])
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git did not run: {e}"))?;
    for line in String::from_utf8_lossy(&grep.stdout).lines() {
        let mut f = line.splitn(4, ':');
        let (_, path, num, text) = (
            f.next(),
            f.next().unwrap_or(""),
            f.next().unwrap_or("0"),
            f.next().unwrap_or(""),
        );
        let Some(at) = text.find("@law:") else {
            continue;
        };
        let Some(law) = text[at + 5..].split_whitespace().next() else {
            continue;
        };
        if !laws.contains(law)
            || path == system_map::MAP_FILE
            || path.ends_with(".md")
            || path.ends_with(".log")
        {
            continue;
        }
        let body = run("git", repo, &["show", &format!("{head}:{path}")])?;
        tagged.push(TaggedTest {
            law: law.to_string(),
            path: path.to_string(),
            test: tagged_test_name(&body, num.parse().unwrap_or(0)),
            changed: false,
        });
    }
    Ok(PrSource {
        meta,
        map_text,
        map_diff,
        map_note,
        diff,
        tagged,
        ..PrSource::default()
    })
}

/// What `gh` says of PR `number` in `repo`: its text, head, checks and the lowest-numbered issue it closes.
/// Calls `gh` as a normal user, nothing else.
pub fn fetch_meta(repo: &Path, number: u64) -> Result<PrMeta, String> {
    let n = number.to_string();
    let json = |args: &[&str]| -> Result<serde_json::Value, String> {
        serde_json::from_str(&run("gh", repo, args)?)
            .map_err(|e| format!("gh printed no JSON: {e}"))
    };
    let v = json(&[
        "pr",
        "view",
        &n,
        "--json",
        "title,body,headRefOid,mergedAt,createdAt,statusCheckRollup,closingIssuesReferences",
    ])?;
    let checks = v["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            format!(
                "{} {}",
                c["name"].as_str().unwrap_or(""),
                c["conclusion"].as_str().unwrap_or("")
            )
        })
        .filter(|c| !c.ends_with("SKIPPED"))
        .collect();
    let spec = match v["closingIssuesReferences"]
        .as_array()
        .and_then(|a| a.iter().filter_map(|i| i["number"].as_u64()).min())
    {
        Some(i) => {
            let s = json(&[
                "issue",
                "view",
                &i.to_string(),
                "--json",
                "title,body,createdAt",
            ])?;
            Some(Spec {
                number: i,
                title: s["title"].as_str().unwrap_or("").into(),
                created_at: s["createdAt"].as_str().unwrap_or("").into(),
                body: s["body"].as_str().unwrap_or("").into(),
            })
        }
        None => None,
    };
    Ok(PrMeta {
        number,
        title: v["title"].as_str().unwrap_or("").into(),
        body: v["body"].as_str().unwrap_or("").into(),
        head: v["headRefOid"].as_str().unwrap_or("").into(),
        merged_at: v["mergedAt"].as_str().unwrap_or("").into(),
        created_at: v["createdAt"].as_str().unwrap_or("").into(),
        checks,
        spec,
    })
}

#[cfg(test)]
mod flow_key_tests {
    use super::*;

    #[test]
    fn a_flow_that_revisits_a_part_keys_each_part_once_in_first_visit_order() {
        let map = serde_json::json!({"links": [
            {"id": "core-shell", "from": "core", "to": "shell"},
            {"id": "shell-understanding", "from": "shell", "to": "understanding"},
            {"id": "shell-window", "from": "shell", "to": "window"},
        ]});
        let flow = serde_json::json!({"hops": [
            {"link": "core-shell"},
            {"link": "shell-understanding"},
            {"link": "shell-understanding"},
            {"link": "shell-window"},
        ]});
        let parts: BTreeSet<String> = ["core", "shell", "understanding", "window"]
            .map(String::from)
            .into();
        assert_eq!(
            flow_key(&map, &flow, &parts),
            ["core", "shell", "understanding", "window"]
        );
    }

    #[test]
    fn a_hop_run_backwards_keys_the_part_it_arrives_at_after_the_one_it_left() {
        let map = serde_json::json!({"links": [
            {"id": "a-b", "from": "a", "to": "b"},
            {"id": "c-b", "from": "c", "to": "b"},
        ]});
        let flow = serde_json::json!({"hops": [{"link": "a-b"}, {"link": "c-b"}]});
        let parts: BTreeSet<String> = ["a", "b", "c"].map(String::from).into();
        assert_eq!(flow_key(&map, &flow, &parts), ["a", "b", "c"]);
    }
}
