//! `build_facts`: a PR's source into the writer's facts. A `Pr` holds what the sections read (the
//! source, the map, the diff and where its files sit); each section, the shape, the budget's cuts
//! and the check context are built by a function of their own.

use super::{
    BUDGET, BudgetRecord, CUT_NOTE_ROOM, CheckContext, DiffCut, Facts, FileDiff, Guard,
    PartialFacts, PrMeta, PrSource, Shape, TaggedTest, TouchedFile, added_tags, cut_note, cut_to,
    flow_key, ids, no_extractor, same_day, section, split_diff, trim_spec,
};
use crate::logic_file::{is_logic, is_test};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

/// Assembles the writer's facts: structure first, then the diff hunk by hunk inside `BUDGET`.
pub fn build_facts(source: &PrSource) -> Result<Facts, String> {
    let map = system_map::parse_lenient(&source.map_text)?;
    let mj: serde_json::Value =
        serde_json::from_str(&source.map_text).map_err(|e| e.to_string())?;
    let files = split_diff(&source.diff);
    let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let touched = system_map::touched_for_map(&map, &paths);
    let map_diff = if !source.map_diff.is_empty() {
        source.map_diff.clone()
    } else {
        source.map_note.clone().unwrap_or_else(|| "none".into())
    };
    let pr = Pr {
        source,
        mj: &mj,
        files: &files,
        touched: &touched,
        map_diff: &map_diff,
    };
    let plan = pr.plan();
    let tagged = pr.tagged();
    let table = pr.parts_table(&tagged);
    let meta = &source.meta;
    let spec_same_day = meta
        .spec
        .as_ref()
        .is_some_and(|s| same_day(&s.created_at, &meta.created_at));
    let tags = added_tags(&files);
    let guard = pr.guard(&tags, &tagged);
    let mut sections = pr.sections(plan.text(), table, &tags, spec_same_day);
    fit_structure(&mut sections);
    let room = BUDGET.saturating_sub(used(&sections) + CUT_NOTE_ROOM);
    let (diff_text, cut, mut diff_cut, mut budget) = pr.fit_diff(&plan.required, room);
    diff_cut.bytes_sent = diff_text.len();
    diff_cut.bytes_cut = source.diff.len().saturating_sub(diff_text.len());
    let partial_facts = PartialFacts {
        cut: (!cut.is_empty()).then_some(diff_cut),
        no_extractor: no_extractor(&source.map_text, &paths)?,
        ..PartialFacts::default()
    };
    sections.push((
        "DIFF NOT SENT".into(),
        cut_note(&cut, "nothing: the whole diff is below"),
    ));
    sections.push(("DIFF".into(), diff_text));
    let text: String = sections.iter().map(|(n, t)| section(n, t)).collect();
    budget.total_bytes = text.len();
    budget.sections = sections
        .iter()
        .map(|(n, t)| (n.clone(), section(n, t).len()))
        .collect();
    let shape = Shape {
        chapters: plan.chapters_mode,
        required: plan.required.clone(),
        also: plan.also.clone(),
        overview: plan.overview,
        flows_changed: plan.flows_changed.clone(),
    };
    let context = pr.context(text.clone(), plan, tagged, spec_same_day);
    Ok(Facts {
        text,
        shape,
        budget,
        context,
        instructions: None,
        guard,
        partial_facts,
    })
}

/// The bytes the sections take in the facts.
fn used(sections: &[(String, String)]) -> usize {
    sections
        .iter()
        .map(|(n, t)| section(n, t).len())
        .sum::<usize>()
}

/// The structure alone must leave room for the diff's note: cut the PR body, then the spec, if it
/// does not.
fn fit_structure(sections: &mut [(String, String)]) {
    for name in ["SPEC", "PR"] {
        let over = (used(sections) + CUT_NOTE_ROOM).saturating_sub(BUDGET);
        if over == 0 {
            break;
        }
        if let Some((_, text)) = sections.iter_mut().find(|(n, _)| n == name) {
            *text = cut_to(text, text.len().saturating_sub(over));
        }
    }
}

/// The brief's plan, decided by code: which touched parts fog, and so the shape.
struct Plan {
    chapters_mode: bool,
    /// The fogging parts, most logic first: together the chapters must light every one.
    required: Vec<String>,
    /// The touched parts that fog nothing: kept on the shape for the pre-flight; the writer is not
    /// told of them (#477).
    also: Vec<String>,
    /// The flows the map's diff names.
    flows_changed: Vec<String>,
    overview: bool,
    /// The map's Core parts under Daniel's tiers: a chapter lighting one carries a gist (#397).
    core: Vec<String>,
}

impl Plan {
    /// SHAPE: the plan as the writer reads it.
    fn text(&self) -> String {
        let core_text = format!(
            "\ncore parts (a chapter lighting one carries a gist): {}",
            if self.core.is_empty() {
                "none".to_string()
            } else {
                self.core.join(", ")
            }
        );
        let shape_text = if self.chapters_mode {
            format!(
                "chapters by behaviour: 2 to 4 chapters, each one behaviour, lighting every part it crosses\nrequired parts (together the chapters must light every one): {}\noverview: {}",
                self.required.join(", "),
                if self.overview {
                    format!("yes, for flow(s) {}", self.flows_changed.join(", "))
                } else {
                    "no".into()
                },
            )
        } else {
            format!(
                "single story (one chapter with part \"story\") about: {}\noverview: no",
                self.required.join(", "),
            )
        };
        shape_text + &core_text
    }
}

/// What the facts are built from: the source, its map as JSON, its diff by file, the parts those
/// files touch, and the map's diff (or the note that stands for it).
struct Pr<'a> {
    source: &'a PrSource,
    mj: &'a serde_json::Value,
    files: &'a [FileDiff],
    touched: &'a system_map::Touched,
    map_diff: &'a str,
}

impl Pr<'_> {
    fn file(&self, path: &str) -> Option<&FileDiff> {
        self.files.iter().find(|f| f.path == path)
    }

    /// The lines `path` changed.
    fn size(&self, path: &str) -> usize {
        self.file(path).map_or(0, |f| f.added + f.removed)
    }

    /// The part that owns touched `path`.
    fn part_of(&self, path: &str) -> Option<String> {
        self.touched
            .parts
            .iter()
            .find(|p| p.files.iter().any(|f| f == path))
            .map(|p| p.id.clone())
    }

    fn part_json(&self, id: &str) -> serde_json::Value {
        self.mj["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == id)
            .cloned()
            .unwrap_or_default()
    }

    /// The ids of the links with part `id` at an end.
    fn links_of(&self, id: &str) -> Vec<String> {
        self.mj["links"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|l| l["from"] == id || l["to"] == id)
            .filter_map(|l| l["id"].as_str().map(String::from))
            .collect()
    }

    /// Which touched parts would fog, and the shape.
    fn plan(&self) -> Plan {
        let mut fogging: Vec<(String, usize)> = Vec::new();
        let mut quiet: Vec<String> = Vec::new();
        for p in &self.touched.parts {
            let pj = self.part_json(&p.id);
            let tier = self
                .source
                .tier_overrides
                .get(&p.id)
                .cloned()
                .unwrap_or_else(|| pj["tier"].as_str().unwrap_or("").to_string());
            let named = self.map_diff.contains(&format!("\"{}\"", p.id));
            let logic: usize = p
                .files
                .iter()
                .filter(|f| kind(f) == "core")
                .map(|f| self.size(f))
                .sum();
            if tier == "core" || (tier == "interface" && named) {
                fogging.push((p.id.clone(), logic));
            } else {
                quiet.push(p.id.clone());
            }
        }
        fogging.sort_by_key(|a| std::cmp::Reverse(a.1));
        let chapters_mode = fogging.len() > 2;
        let flows_changed: Vec<String> = ids(self.mj, "flows")
            .into_iter()
            .filter(|f| self.map_diff.contains(&format!("\"{f}\"")))
            .collect();
        let core: Vec<String> = ids(self.mj, "parts")
            .iter()
            .filter(|id| {
                self.source.tier_overrides.get(*id).map_or_else(
                    || self.part_json(id)["tier"] == "core",
                    |tier| tier == "core",
                )
            })
            .cloned()
            .collect();
        Plan {
            chapters_mode,
            required: fogging.iter().map(|p| p.0.clone()).collect(),
            also: quiet,
            overview: chapters_mode && !flows_changed.is_empty(),
            flows_changed,
            core,
        }
    }

    /// The tagged tests, and whether this PR's diff changed one.
    fn tagged(&self) -> Vec<TaggedTest> {
        let mut tagged = self.source.tagged.clone();
        for t in &mut tagged {
            t.changed = self.file(&t.path).is_some_and(|fd| {
                fd.hunks.iter().any(|h| {
                    (!t.test.is_empty() && h.contains(&t.test))
                        || h.contains(&format!("@law: {}", t.law))
                })
            });
        }
        tagged
    }

    /// PARTS TABLE: each touched part with its laws, the flows through it and its files, then the
    /// count of noise files.
    fn parts_table(&self, tagged: &[TaggedTest]) -> String {
        let mut table = String::new();
        for p in &self.touched.parts {
            let pj = self.part_json(&p.id);
            let links = self.links_of(&p.id);
            let laws = law_lines(self.mj, &p.id, &links, tagged);
            let flows = self.flows_through(&links);
            let mut fs: Vec<&String> = p.files.iter().collect();
            fs.sort_by_key(|f| {
                (
                    ["core", "test", "plumbing"]
                        .iter()
                        .position(|k| *k == kind(f)),
                    std::cmp::Reverse(self.size(f)),
                )
            });
            let tier = self
                .source
                .tier_overrides
                .get(&p.id)
                .map(String::as_str)
                .or(pj["tier"].as_str())
                .unwrap_or("");
            // Writing to a `String` cannot fail.
            writeln!(
                table,
                "## {} ({}, tier {tier})",
                p.id,
                pj["name"].as_str().unwrap_or("")
            )
            .ok();
            // Writing to a `String` cannot fail.
            write!(
                table,
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
            )
            .ok();
            for f in fs {
                let d = self.file(f);
                // Writing to a `String` cannot fail.
                writeln!(
                    table,
                    "- {} {} +{}/-{}",
                    kind(f),
                    f,
                    d.map_or(0, |d| d.added),
                    d.map_or(0, |d| d.removed)
                )
                .ok();
            }
            table += "\n";
        }
        let touched = self.touched;
        if !touched.ignored.is_empty() || !touched.unowned.is_empty() {
            // Writing to a `String` cannot fail.
            writeln!(
                table,
                "noise (ignored or unowned paths, not the running system): {} files",
                touched.ignored.len() + touched.unowned.len()
            )
            .ok();
        }
        table
    }

    /// The ids of the flows with a hop along one of `links`.
    fn flows_through(&self, links: &[String]) -> Vec<String> {
        self.mj["flows"]
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
            .collect()
    }

    /// Guard with a law (#401): a bug fix that added no tag and changed no tagged test, on a part of
    /// the map (with none, no law is there to back and no part to add one on).
    fn guard(&self, tags: &[String], tagged: &[TaggedTest]) -> Option<Guard> {
        let touched = self.touched;
        (self.source.meta.bug
            && tags.is_empty()
            && !tagged.iter().any(|t| t.changed)
            && !touched.parts.is_empty())
        .then(|| {
            let mut parts: Vec<String> = touched.parts.iter().map(|p| p.id.clone()).collect();
            parts.sort();
            Guard { parts }
        })
    }

    /// Every section before the diff's: the shape, the map and its diff, the parts table, the tags,
    /// the checks, the PR, its spec, and for a push since Daniel read it, what is new since and the
    /// decisions he saw.
    fn sections(
        &self,
        shape_text: String,
        table: String,
        tags: &[String],
        spec_same_day: bool,
    ) -> Vec<(String, String)> {
        let (source, meta) = (self.source, &self.source.meta);
        let mut sections: Vec<(String, String)> = vec![
            ("SHAPE (decided by code)".into(), shape_text),
            ("SYSTEM MAP".into(), source.map_text.clone()),
            ("MAP DIFF IN THIS PR".into(), self.map_diff.to_owned()),
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
            ("PR".into(), pr_text(meta)),
            ("SPEC".into(), spec_text(meta, spec_same_day)),
        ];
        if let Some(from) = &source.since {
            sections.push((
                "SINCE YOU READ".into(),
                format!(
                    "Daniel read this PR's full brief at {from}. This story tells only what the push since changed: the diff, the map \
                     diff and the law tags here run from {from} to {}, not from the PR's base. Tell what is new since, not the whole PR.",
                    meta.head
                ),
            ));
        }
        if !source.earlier_decisions.is_empty() {
            let lines: Vec<String> = source
                .earlier_decisions
                .iter()
                .map(|d| format!("- {}: {}", d.id.as_deref().unwrap_or("?"), d.text))
                .collect();
            sections.push(("EARLIER DECISIONS".into(), lines.join("\n")));
        }
        sections
    }

    /// The diff in `room` bytes: core files of the required parts, then other parts' core, then
    /// tests, then the rest, hunk by hunk. Returns the diff sent, a line for each file not fully
    /// sent, the files cut and the budget's file counts.
    fn fit_diff(
        &self,
        required: &[String],
        mut room: usize,
    ) -> (String, Vec<String>, DiffCut, BudgetRecord) {
        let rank = |f: &FileDiff| {
            let part = self.part_of(&f.path);
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
        let mut order: Vec<&FileDiff> = self.files.iter().collect();
        order.sort_by_key(|f| (rank(f), f.added + f.removed));
        let mut diff_text = String::new();
        let mut cut: Vec<String> = Vec::new();
        let mut diff_cut = DiffCut::default();
        let mut budget = BudgetRecord {
            diff_bytes_full: self.source.diff.len(),
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
            let part = self.part_of(&f.path).unwrap_or_else(|| "(noise)".into());
            if whole {
                budget.files_sent += 1;
            } else {
                if sent > 0 {
                    budget.files_partial += 1;
                    diff_cut.files_partial.push(f.path.clone());
                } else {
                    budget.files_cut += 1;
                    diff_cut.files_cut.push(f.path.clone());
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
        (diff_text, cut, diff_cut, budget)
    }

    /// What the checks need to hold a reply to the facts.
    fn context(
        &self,
        facts: String,
        plan: Plan,
        tagged: Vec<TaggedTest>,
        spec_same_day: bool,
    ) -> CheckContext {
        let (mj, source) = (self.mj, self.source);
        let all_parts = ids(mj, "parts");
        let mut flow_keys = BTreeMap::new();
        for flow in mj["flows"].as_array().into_iter().flatten() {
            let id = flow["id"].as_str().unwrap_or("");
            if plan.flows_changed.iter().any(|f| f == id) {
                flow_keys.insert(id.to_string(), flow_key(mj, flow, &all_parts));
            }
        }
        CheckContext {
            facts,
            diff: source.diff.clone(),
            chapters_mode: plan.chapters_mode,
            required: plan.required,
            files: self
                .files
                .iter()
                .map(|f| {
                    let laws: BTreeSet<String> = source
                        .tagged
                        .iter()
                        .filter(|t| t.path == f.path)
                        .map(|t| t.law.clone())
                        .collect();
                    TouchedFile {
                        path: f.path.clone(),
                        lines: f.added + f.removed,
                        part: self.part_of(&f.path),
                        ignored: self.touched.ignored.contains(&f.path),
                        laws: laws.into_iter().collect(),
                    }
                })
                .collect(),
            overview: plan.overview,
            parts: all_parts,
            actors: ids(mj, "actors"),
            links: ids(mj, "links"),
            laws: ids(mj, "laws"),
            flows: ids(mj, "flows"),
            law_texts: mj["laws"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|l| l["text"].as_str().map(str::to_lowercase))
                .collect(),
            flows_changed: flow_keys,
            tagged,
            spec_same_day,
            core: plan.core,
        }
    }
}

/// What a touched file is: a test, logic (`core`) or plumbing.
fn kind(path: &str) -> &'static str {
    if is_test(path) {
        "test"
    } else if is_logic(path) {
        "core"
    } else {
        "plumbing"
    }
}

/// One line per law on part `id` or one of its links, with its tagged tests.
fn law_lines(
    mj: &serde_json::Value,
    id: &str,
    links: &[String],
    tagged: &[TaggedTest],
) -> Vec<String> {
    mj["laws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["on"] == id || links.iter().any(|k| l["on"] == k.as_str()))
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
        .collect()
}

/// PR: its number, title, state, creation and body.
fn pr_text(meta: &PrMeta) -> String {
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
    )
}

/// SPEC: the spec the PR closes, trimmed, or none.
fn spec_text(meta: &PrMeta, spec_same_day: bool) -> String {
    match &meta.spec {
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
    }
}
