//! `build_facts`: a PR's source into the writer's facts.

use super::{
    BUDGET, BudgetRecord, CUT_NOTE_ROOM, CheckContext, DiffCut, Facts, FileDiff, Guard,
    PartialFacts, PrSource, Shape, TouchedFile, added_tags, cut_to, flow_key, ids, no_extractor,
    same_day, section, split_diff, trim_spec,
};
use crate::logic_file::{is_logic, is_test};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
/// Assembles the writer's facts: structure first, then the diff hunk by hunk inside `BUDGET`.
#[allow(
    clippy::too_many_lines,
    reason = "#556 splits it by responsibility (spec #548, Group 4)"
)]
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
    // The touched parts that fog nothing: kept on the shape for the pre-flight; the writer is not told of them (#477).
    let also = quiet;
    // The map's Core parts under Daniel's tiers: a chapter lighting one carries a gist (#397).
    let core: Vec<String> = all_parts
        .iter()
        .filter(|id| {
            source
                .tier_overrides
                .get(*id)
                .map_or_else(|| part_json(id)["tier"] == "core", |tier| tier == "core")
        })
        .cloned()
        .collect();
    let core_text = format!(
        "\ncore parts (a chapter lighting one carries a gist): {}",
        if core.is_empty() {
            "none".to_string()
        } else {
            core.join(", ")
        }
    );
    let shape_text = if chapters_mode {
        format!(
            "chapters by behaviour: 2 to 4 chapters, each one behaviour, lighting every part it crosses\nrequired parts (together the chapters must light every one): {}\noverview: {}",
            required.join(", "),
            if overview {
                format!("yes, for flow(s) {}", flows_changed.join(", "))
            } else {
                "no".into()
            },
        )
    } else {
        format!(
            "single story (one chapter with part \"story\") about: {}\noverview: no",
            required.join(", "),
        )
    };
    let shape_text = shape_text + &core_text;

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
            let d = file(f);
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
    if !touched.ignored.is_empty() || !touched.unowned.is_empty() {
        // Writing to a `String` cannot fail.
        writeln!(
            table,
            "noise (ignored or unowned paths, not the running system): {} files",
            touched.ignored.len() + touched.unowned.len()
        )
        .ok();
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
    // Guard with a law (#401): a bug fix that added no tag and changed no tagged test, on a part of the map (with none, no law is there to
    // back and no part to add one on).
    let guard = (source.meta.bug
        && tags.is_empty()
        && !tagged.iter().any(|t| t.changed)
        && !touched.parts.is_empty())
    .then(|| {
        let mut parts: Vec<String> = touched.parts.iter().map(|p| p.id.clone()).collect();
        parts.sort();
        Guard { parts }
    });
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
    let mut diff_cut = DiffCut::default();
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
    let cut_text = if cut.is_empty() {
        "nothing: the whole diff is below".to_string()
    } else {
        let mut t = format!(
            "{} files not fully sent; do not guess what they hold:\n",
            cut.len()
        );
        for (i, line) in cut.iter().enumerate() {
            if t.len() + line.len() + 1 > CUT_NOTE_ROOM - 200 {
                // Writing to a `String` cannot fail.
                writeln!(t, "- ... and {} more", cut.len() - i).ok();
                break;
            }
            t += line;
            t += "\n";
        }
        t
    };
    diff_cut.bytes_sent = diff_text.len();
    diff_cut.bytes_cut = source.diff.len().saturating_sub(diff_text.len());
    let partial_facts = PartialFacts {
        cut: (!cut.is_empty()).then_some(diff_cut),
        no_extractor: no_extractor(&source.map_text, &paths)?,
        ..PartialFacts::default()
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
        also,
        overview,
        flows_changed: flows_changed.clone(),
    };
    let context = CheckContext {
        facts: text.clone(),
        diff: source.diff.clone(),
        chapters_mode,
        required,
        files: files
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
                    part: part_of(&f.path),
                    ignored: touched.ignored.contains(&f.path),
                    laws: laws.into_iter().collect(),
                }
            })
            .collect(),
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
        core,
    };
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
