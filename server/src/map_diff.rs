//! The Map diff (spec #634, ticket #640): one picture of what a PR changed on the system map, drawn on today's map of the default
//! branch. It compares as `ratify::pr_changes` does (the merge base with the head while the PR is open, its squash commit with the
//! parent once it has landed) and adds to today's map each part, actor, link and law the PR adds (from the head) and each it removes
//! (from the base, kept as a ghost), each tagged `added`, `removed` or `edited`:
//! - a part is `edited` only when its own `name`, `purpose` or `caption` changed: held items and paths alone tag nothing;
//! - links are judged per pair of ends, as the canvas draws one line per pair: a pair on one side only is `added` or `removed`, and a
//!   pair on both sides is untagged whatever its links say (a changed kind, reason or direction is Ratify's);
//! - a law is `added` or `removed`, a changed one untagged; flows are not drawn.
//!
//! A PR whose maps give no map item (`ratify::map_items`: nothing changed, or only the file's layout) has no document.

use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

use crate::map::{default_ref, git};
use crate::ratify::{WALK, first_parents, map_at};
use studio_understanding::pr_brief::MapCounts;

/// What a PR did to one item on the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Tag {
    Added,
    Removed,
    Edited,
}

/// A line's tag: its two ends, in sorted order, as the canvas keys a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineTag {
    pub ends: [String; 2],
    pub change: Tag,
}

/// The Map diff of PR `pr`, compared from `base` to `head`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapDiff {
    pub pr: u64,
    pub base: String,
    pub head: String,
    /// Today's map with the PR's added items and its removed ones in it, as version 2 JSON text.
    pub source: String,
    pub parts: BTreeMap<String, Tag>,
    pub actors: BTreeMap<String, Tag>,
    pub lines: Vec<LineTag>,
    pub laws: BTreeMap<String, Tag>,
}

/// The Map diff of PR `number` in `repo`, at `open_head` while it is open; none when the PR changed nothing on the map, or git
/// cannot give the maps.
pub fn of(repo: &Path, number: u64, open_head: Option<&str>) -> Option<MapDiff> {
    let (base, head) = compared(repo, number, open_head)?;
    between(repo, number, base, head).map(|(diff, _)| diff)
}

/// The Map diff of PR `number` from `base` to `head`, with its counts.
fn between(repo: &Path, number: u64, base: String, head: String) -> Option<(MapDiff, MapCounts)> {
    let (old, new) = (map_at(repo, &base), map_at(repo, &head));
    let (old, new) = (old.text()?, new.text()?);
    let items = crate::ratify::map_items(Some(old), Some(new));
    if items.is_empty() {
        return None;
    }
    let today = crate::map::read_map(repo).source?;
    let read = |text: &str| system_map::convert_value(text).ok();
    let (today, old, new) = (read(&today)?, read(old)?, read(new)?);
    let diff = merge(today, &old, &new, number, base, head);
    let counts = count(&diff, &items, &old, &new);
    Some((diff, counts))
}

/// What the Map diff from `base` to `head` marks, counted per Ratify map item and links per line, for the short brief's sentence (#644): none when the PR
/// changed nothing on the map, or only its layout.
pub fn counts(repo: &Path, base: &str, head: &str) -> Option<MapCounts> {
    between(repo, 0, base.to_string(), head.to_string()).map(|(_, counts)| counts)
}

/// Counts each of Ratify's map `items` under the mark `diff` gives it: an added or removed part, actor or law by its id's tag, a
/// changed part by `edited`, and links per line, as the canvas draws one line per pair of ends: the links added or removed on a
/// tagged line count once. Any other (a held item, a changed link or law, a flow) is `unmarked`.
fn count(diff: &MapDiff, items: &[crate::ratify::MapItem], old: &Value, new: &Value) -> MapCounts {
    let mut counts = MapCounts::default();
    let mut lit: Vec<String> = Vec::new();
    let mut counted: Vec<&LineTag> = Vec::new();
    for item in items {
        let tag = match (item.kind.as_str(), item.change.as_str()) {
            ("part", _) => diff.parts.get(&item.id).copied(),
            ("actor", _) => diff.actors.get(&item.id).copied(),
            ("law", _) => diff.laws.get(&item.id).copied(),
            ("link", change @ ("added" | "removed")) => {
                let side = if change == "added" { new } else { old };
                let line = by_id(side, "links")
                    .get(item.id.as_str())
                    .and_then(|link| ends(link))
                    .and_then(|pair| diff.lines.iter().find(|l| l.ends == pair));
                // A line counts once, however many of its links the PR added or removed.
                if let Some(line) = line {
                    if counted.contains(&line) {
                        continue;
                    }
                    counted.push(line);
                }
                line.map(|line| line.change)
            }
            _ => None,
        };
        let slot = match (item.kind.as_str(), item.change.as_str(), tag) {
            ("part", "added", Some(Tag::Added)) => &mut counts.parts_added,
            ("part", "removed", Some(Tag::Removed)) => &mut counts.parts_removed,
            ("part", "changed", Some(Tag::Edited)) => &mut counts.parts_edited,
            ("actor", "added", Some(Tag::Added)) => &mut counts.actors_added,
            ("actor", "removed", Some(Tag::Removed)) => &mut counts.actors_removed,
            ("link", "added", Some(Tag::Added)) => &mut counts.links_added,
            ("link", "removed", Some(Tag::Removed)) => &mut counts.links_removed,
            ("law", "added", Some(Tag::Added)) => &mut counts.laws_added,
            ("law", "removed", Some(Tag::Removed)) => &mut counts.laws_removed,
            _ => {
                counts.unmarked += 1;
                continue;
            }
        };
        *slot += 1;
        let side = if item.change == "removed" { old } else { new };
        lit.extend(on_map(side, &item.kind, &item.id));
    }
    lit.sort();
    lit.dedup();
    counts.lit = lit;
    counts
}

/// The parts and actors a marked item of `kind` and `id` sits on in `map`: a part or actor itself, a link's ends, a law's part, or
/// its link's ends.
fn on_map(map: &Value, kind: &str, id: &str) -> Vec<String> {
    let link_ends = |id: &str| -> Vec<String> {
        by_id(map, "links")
            .get(id)
            .and_then(|link| ends(link))
            .map(Vec::from)
            .unwrap_or_default()
    };
    match kind {
        "part" | "actor" => vec![id.to_string()],
        "link" => link_ends(id),
        "law" => by_id(map, "laws")
            .get(id)
            .and_then(|law| law["on"].as_str())
            .map(|on| {
                let ends = link_ends(on);
                if ends.is_empty() {
                    vec![on.to_string()]
                } else {
                    ends
                }
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

/// The two commits `ratify::pr_changes` compares for PR `number`: the merge base and the head while it is open, else its squash
/// commit's parent and the commit.
fn compared(repo: &Path, number: u64, open_head: Option<&str>) -> Option<(String, String)> {
    let (reference, _) = default_ref(repo)?;
    if let Some(head) = open_head {
        let base = git(repo, &["merge-base", &reference, head]).ok()?;
        return Some((base, head.to_string()));
    }
    let landed = first_parents(repo, &reference, WALK)
        .into_iter()
        .find(|l| l.pr == Some(number))?;
    let parent = git(repo, &["rev-parse", &format!("{}^", landed.commit)]).ok()?;
    Some((parent, landed.commit))
}

/// The items of list `key` of `map`, by id.
fn by_id<'a>(map: &'a Value, key: &str) -> BTreeMap<&'a str, &'a Value> {
    map[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| Some((item["id"].as_str()?, item)))
        .collect()
}

/// A link's ends in sorted order: the line it is drawn on. Ratify's rows key a link's mark by it too (`ratify::mark_of`).
pub(crate) fn ends(link: &Value) -> Option<[String; 2]> {
    let mut ends = [
        link["from"].as_str()?.to_string(),
        link["to"].as_str()?.to_string(),
    ];
    ends.sort();
    Some(ends)
}

/// The links of `map`, by the line they are drawn on.
fn lines_of(map: &Value) -> BTreeMap<[String; 2], Vec<&Value>> {
    let mut lines: BTreeMap<[String; 2], Vec<&Value>> = BTreeMap::new();
    for link in map["links"].as_array().into_iter().flatten() {
        if let Some(ends) = ends(link) {
            lines.entry(ends).or_default().push(link);
        }
    }
    lines
}

/// The fields of a part whose change marks it `edited`.
const OWN: [&str; 3] = ["name", "purpose", "caption"];

/// Today's map with what `new` adds to `old` and what it removes put in it, and each one's tag.
fn merge(
    mut today: Value,
    old: &Value,
    new: &Value,
    pr: u64,
    base: String,
    head: String,
) -> MapDiff {
    let mut tags: BTreeMap<&str, BTreeMap<String, Tag>> = BTreeMap::new();
    for key in ["parts", "actors", "laws"] {
        let (before, after) = (by_id(old, key), by_id(new, key));
        let mut list = today[key].as_array().cloned().unwrap_or_default();
        let shown: Vec<String> = list
            .iter()
            .filter_map(|item| Some(item["id"].as_str()?.to_string()))
            .collect();
        let mine = tags.entry(key).or_default();
        let only =
            |of: &BTreeMap<&str, &Value>, not: &BTreeMap<&str, &Value>| -> Vec<(String, Value)> {
                of.iter()
                    .filter(|(id, _)| !not.contains_key(*id))
                    .map(|(id, item)| (id.to_string(), (*item).clone()))
                    .collect()
            };
        for (tag, items) in [
            (Tag::Added, only(&after, &before)),
            (Tag::Removed, only(&before, &after)),
        ] {
            for (id, item) in items {
                if !shown.contains(&id) {
                    list.push(item);
                }
                mine.insert(id, tag);
            }
        }
        if key == "parts" {
            for (id, was) in &before {
                if let Some(now) = after.get(id)
                    && shown.iter().any(|s| s == id)
                    && OWN.iter().any(|field| was[field] != now[field])
                {
                    mine.insert(id.to_string(), Tag::Edited);
                }
            }
        }
        today[key] = Value::Array(list);
    }
    let (before, after) = (lines_of(old), lines_of(new));
    let mut links = today["links"].as_array().cloned().unwrap_or_default();
    let mut lines = Vec::new();
    for (tag, of, not) in [
        (Tag::Added, &after, &before),
        (Tag::Removed, &before, &after),
    ] {
        for (pair, found) in of.iter().filter(|(pair, _)| !not.contains_key(*pair)) {
            for link in found {
                if !links.iter().any(|l| l["id"] == link["id"]) {
                    links.push((*link).clone());
                }
            }
            lines.push(LineTag {
                ends: pair.clone(),
                change: tag,
            });
        }
    }
    today["links"] = Value::Array(links);
    let mut take = |key| tags.remove(key).unwrap_or_default();
    MapDiff {
        pr,
        base,
        head,
        parts: take("parts"),
        actors: take("actors"),
        laws: take("laws"),
        lines,
        source: today.to_string(),
    }
}

#[cfg(test)]
mod tests;
