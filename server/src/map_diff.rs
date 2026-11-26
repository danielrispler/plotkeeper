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

/// What the Map diff from `base` to `head` marks, counted per Ratify map item for the short brief's sentence (#644): none when the PR
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
mod tests {
    use super::*;
    use studio_test_support::{git, repo};

    fn commit(dir: &Path, map: &str, subject: &str) -> String {
        std::fs::write(dir.join("system-map.json"), map).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-q", "--allow-empty", "-m", subject]);
        git(dir, &["rev-parse", "HEAD"])
    }

    const PR500: [&str; 2] = [
        include_str!("../tests/map-diff/pr500-before.json"),
        include_str!("../tests/map-diff/pr500-after.json"),
    ];

    /// A repository where PR `number` landed by a squash commit from `before` to `after`.
    fn landed(before: &str, after: &str, number: u64) -> (tempfile::TempDir, String) {
        let (dir, main) = repo();
        commit(dir.path(), before, "Map (#1)");
        commit(dir.path(), after, &format!("The change (#{number})"));
        (dir, main)
    }

    fn tagged(tags: &BTreeMap<String, Tag>, tag: Tag) -> Vec<&str> {
        tags.iter()
            .filter(|(_, t)| **t == tag)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    fn lines(diff: &MapDiff, tag: Tag) -> Vec<String> {
        let mut found: Vec<String> = diff
            .lines
            .iter()
            .filter(|l| l.change == tag)
            .map(|l| l.ends.join("|"))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn pr_500s_split_shows_its_two_new_parts_its_rewritten_parts_as_edited_and_its_new_and_gone_lines()
     {
        let (_dir, main) = landed(PR500[0], PR500[1], 500);
        let diff = of(Path::new(&main), 500, None).expect("PR 500 changed the map");
        // The prototype's tags (local-evidence/2026-11-26-step4-pr-map-grilling/prototype, build_data.py).
        assert_eq!(
            tagged(&diff.parts, Tag::Added),
            ["install", "map-briefs-runs"]
        );
        assert_eq!(
            tagged(&diff.parts, Tag::Edited),
            [
                "core",
                "hook",
                "plugins",
                "server",
                "understanding",
                "window",
                "workbench"
            ]
        );
        assert!(tagged(&diff.parts, Tag::Removed).is_empty());
        assert!(diff.actors.is_empty());
        assert_eq!(
            lines(&diff, Tag::Removed),
            ["agents|server", "plugins|server"]
        );
        assert_eq!(
            lines(&diff, Tag::Added),
            [
                "agents|install",
                "agents|map-briefs-runs",
                "core|map-briefs-runs",
                "core|understanding",
                "core|workbench",
                "hook|install",
                "hook|map-briefs-runs",
                "hook|understanding",
                "install|plugins",
                "install|server",
                "install|workbench",
                "map-briefs-runs|server",
                "map-briefs-runs|understanding",
                "map-briefs-runs|workbench",
            ]
        );
        // Two laws moved `on`: changed, so untagged; the flow is not drawn.
        assert!(diff.laws.is_empty());
    }

    const PR351: [&str; 2] = [
        include_str!("../tests/map-diff/pr351-before.json"),
        include_str!("../tests/map-diff/pr351-after.json"),
    ];

    fn drawn(diff: &MapDiff, key: &str) -> Vec<String> {
        let map: Value = serde_json::from_str(&diff.source).unwrap();
        by_id(&map, key).keys().map(ToString::to_string).collect()
    }

    #[test]
    fn pr_351s_removal_keeps_the_gone_parts_their_lines_and_laws_on_todays_map_as_ghosts() {
        let (_dir, main) = landed(PR351[0], PR351[1], 351);
        let diff = of(Path::new(&main), 351, None).expect("PR 351 changed the map");
        assert_eq!(
            tagged(&diff.parts, Tag::Removed),
            ["shell", "terminal-daemon"]
        );
        assert!(tagged(&diff.parts, Tag::Added).is_empty());
        assert!(tagged(&diff.parts, Tag::Edited).is_empty());
        assert_eq!(
            lines(&diff, Tag::Removed),
            [
                "core|shell",
                "hook|shell",
                "server|shell",
                "shell|terminal-daemon",
                "shell|understanding",
                "shell|window",
                "shell|workbench",
            ]
        );
        assert!(lines(&diff, Tag::Added).is_empty());
        assert_eq!(
            tagged(&diff.laws, Tag::Removed),
            ["no-notices-lock-on-main-thread", "terminals-outlive-app"]
        );
        // Today's map (the squash commit's) lacks them; the document draws them from the base.
        let parts = drawn(&diff, "parts");
        assert!(parts.contains(&"shell".into()) && parts.contains(&"terminal-daemon".into()));
        let map: Value = serde_json::from_str(&diff.source).unwrap();
        assert!(lines_of(&map).contains_key(&["shell".to_string(), "terminal-daemon".to_string()]));
        assert!(drawn(&diff, "laws").contains(&"terminals-outlive-app".into()));
    }

    const PR558: [&str; 2] = [
        include_str!("../tests/map-diff/pr558-before.json"),
        include_str!("../tests/map-diff/pr558-after.json"),
    ];

    #[test]
    fn pr_558s_held_item_edits_tag_nothing_and_only_its_own_purpose_change_is_edited() {
        let (_dir, main) = landed(PR558[0], PR558[1], 558);
        let diff = of(Path::new(&main), 558, None).expect("PR 558 changed the map");
        // Held items changed on six more parts, and `map-briefs-runs`'s paths: no tag on them.
        assert_eq!(
            diff.parts,
            BTreeMap::from([("workbench".into(), Tag::Edited)])
        );
        assert_eq!(
            lines(&diff, Tag::Added),
            ["hook|workbench", "understanding|workbench"]
        );
        assert!(lines(&diff, Tag::Removed).is_empty());
        // `wire-types-agree`'s text changed: untagged.
        assert_eq!(
            diff.laws,
            BTreeMap::from([("git-and-gh-go-through-the-runner".into(), Tag::Added)])
        );
    }

    /// A version 2 map of parts `a`, `b` and `c` (`a`'s held item saying `held`), with `links` and `laws` as JSON.
    fn small(held: &str, links: &str, laws: &str) -> String {
        let part = |id: &str, holds: &str| {
            format!(
                r#"{{"id":"{id}","name":"{id}","caption":"c","purpose":"p","tier":"core","paths":["{id}/"],"holds":{holds}}}"#
            )
        };
        format!(
            r#"{{"version":2,"parts":[{},{},{}],"actors":[],"links":{links},"ignored":[],"laws":{laws},"flows":[]}}"#,
            part("a", &format!(r#"[{{"name":"H","text":"{held}"}}]"#)),
            part("b", "[]"),
            part("c", "[]"),
        )
    }

    #[test]
    fn a_held_items_edit_alone_tags_nothing_but_still_gives_a_document() {
        let (_dir, main) = landed(&small("Old.", "[]", "[]"), &small("New.", "[]", "[]"), 7);
        let diff = of(Path::new(&main), 7, None).expect("a held item changed");
        assert!(diff.parts.is_empty() && diff.lines.is_empty() && diff.laws.is_empty());
    }

    #[test]
    fn a_flipped_link_is_one_line_on_both_sides_so_it_is_untagged() {
        let before = small(
            "h",
            r#"[{"id":"a-b","from":"a","to":"b","kind":"import"}]"#,
            "[]",
        );
        let after = small(
            "h",
            r#"[{"id":"b-a","from":"b","to":"a","kind":"import"}]"#,
            "[]",
        );
        let (_dir, main) = landed(&before, &after, 7);
        let diff = of(Path::new(&main), 7, None).expect("the link changed");
        assert!(diff.lines.is_empty(), "{:?}", diff.lines);
    }

    #[test]
    fn an_open_prs_counts_say_what_its_map_diff_marks_per_kind_and_what_it_marks_nothing_for() {
        let (dir, main) = repo();
        let root = dir.path();
        let before = small(
            "Old.",
            r#"[{"id":"a-b","from":"a","to":"b","reason":"A asks b."}]"#,
            "[]",
        );
        let base = commit(root, &before, "Map (#1)");
        git(root, &["checkout", "-q", "-b", "work"]);
        let mut after: Value = serde_json::from_str(&small(
            "New.",
            r#"[{"id":"a-b","from":"a","to":"b","reason":"A tells b."},{"id":"a-d","from":"a","to":"d"}]"#,
            r#"[{"id":"l1","on":"a","backing":"trust","text":"First."}]"#,
        ))
        .unwrap();
        let parts = after["parts"].as_array_mut().unwrap();
        // Part c goes, d comes, and b's own purpose changes; a's held item and a-b's reason change too, which draw no mark.
        parts.retain(|p| p["id"] != "c");
        parts[1]["purpose"] = "B, now plainer.".into();
        parts.push(serde_json::json!(
            {"id":"d","name":"D","caption":"c","purpose":"p","tier":"core","paths":["d/"],"holds":[]}
        ));
        let head = commit(root, &after.to_string(), "work");
        let found = counts(Path::new(&main), &base, &head).expect("the PR changed the map");
        assert_eq!(
            found.sentence(),
            "1 part added, 1 removed, 1 edited, 1 link added, 1 law added, and 2 changes with no mark on the map."
        );
        // What the short brief's step lights: each marked part, a marked line's ends and a marked law's part.
        assert_eq!(found.lit, ["a", "b", "c", "d"]);
        // A reformatted map counts nothing.
        let pretty =
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(&before).unwrap()).unwrap();
        git(root, &["checkout", "-q", "main"]);
        let reformatted = commit(root, &pretty, "Pretty (#2)");
        assert_eq!(counts(Path::new(&main), &base, &reformatted), None);
    }

    #[test]
    fn links_are_counted_per_line_drawn_so_two_links_on_one_new_pair_are_one_link_added() {
        let (dir, main) = repo();
        let root = dir.path();
        let base = commit(
            root,
            &small(
                "h",
                r#"[{"id":"a-b","from":"a","to":"b"},{"id":"b-a","from":"b","to":"a"}]"#,
                "[]",
            ),
            "Map (#1)",
        );
        git(root, &["checkout", "-q", "-b", "work"]);
        // Two links come on the pair a and c, and both of a and b's go: one line drawn new, one gone.
        let head = commit(
            root,
            &small(
                "h",
                r#"[{"id":"a-c","from":"a","to":"c"},{"id":"c-a","from":"c","to":"a"}]"#,
                "[]",
            ),
            "work",
        );
        let found = counts(Path::new(&main), &base, &head).expect("the PR changed the map");
        assert_eq!(found.sentence(), "1 link added, 1 removed.");
        assert_eq!(found.lit, ["a", "b", "c"]);
    }

    #[test]
    fn a_reformatted_map_has_no_document() {
        let map = small("h", r#"[{"id":"a-b","from":"a","to":"b"}]"#, "[]");
        let pretty =
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(&map).unwrap()).unwrap();
        let (_dir, main) = landed(&map, &pretty, 7);
        assert_eq!(of(Path::new(&main), 7, None), None);
        // A PR that is not on the default branch and not open has nothing to compare either.
        assert_eq!(of(Path::new(&main), 8, None), None);
    }

    #[test]
    fn an_open_pr_compares_its_merge_base_with_its_head_and_main_moving_after_it_shows_untagged() {
        let (dir, main) = repo();
        let root = dir.path();
        let law = r#"[{"id":"l1","on":"a","backing":"trust","text":"First."}]"#;
        commit(root, &small("h", "[]", "[]"), "Map (#1)");
        git(root, &["checkout", "-q", "-b", "work"]);
        // The PR adds a link a-c and a law.
        let head = commit(
            root,
            &small("h", r#"[{"id":"a-c","from":"a","to":"c"}]"#, law),
            "work",
        );
        git(root, &["checkout", "-q", "main"]);
        // Main gains part d after the branch left it.
        let mut moved: Value = serde_json::from_str(&small("h", "[]", "[]")).unwrap();
        moved["parts"].as_array_mut().unwrap().push(serde_json::json!(
            {"id":"d","name":"D","caption":"c","purpose":"p","tier":"core","paths":["d/"],"holds":[]}
        ));
        commit(root, &moved.to_string(), "Part d (#2)");
        let diff = of(Path::new(&main), 3, Some(&head)).expect("the PR changed the map");
        assert_eq!(diff.head, head);
        assert_eq!(
            diff.base,
            git(root, &["rev-parse", "HEAD~1"]),
            "the merge base, not main's tip"
        );
        assert_eq!(lines(&diff, Tag::Added), ["a|c"]);
        assert_eq!(diff.laws, BTreeMap::from([("l1".into(), Tag::Added)]));
        // Today's map: main's part d is there, untagged; the PR's link and law are drawn on it.
        assert!(drawn(&diff, "parts").contains(&"d".into()));
        assert!(!diff.parts.contains_key("d"));
        assert!(drawn(&diff, "links").contains(&"a-c".into()));
        assert!(drawn(&diff, "laws").contains(&"l1".into()));
        // Once it lands by a squash commit, it compares that commit with its parent, and today's map already holds the change.
        commit(
            root,
            &{
                let mut landed: Value = moved.clone();
                landed["links"] = serde_json::json!([{"id":"a-c","from":"a","to":"c"}]);
                landed["laws"] = serde_json::from_str(law).unwrap();
                landed.to_string()
            },
            "The PR (#3)",
        );
        let done = of(Path::new(&main), 3, None).expect("the landed PR changed the map");
        assert_eq!(done.head, git(root, &["rev-parse", "HEAD"]));
        assert_eq!(done.base, git(root, &["rev-parse", "HEAD~1"]));
        assert_eq!(lines(&done, Tag::Added), ["a|c"]);
        assert_eq!(drawn(&done, "links"), ["a-c"], "drawn once");
    }
}
