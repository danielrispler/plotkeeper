//! The fog of the Map view (spec danielrispler/plotkeeper#306, ticket #310): what landed on `origin`'s default branch since
//! Daniel last caught up, kept per part as a list of commits, each `thin` or `thick`. Never a count: the window shows only the
//! strongest, and only what Daniel's effective tier lets through.
//!
//! The walk is first-parent, from the commit last processed to the ref's tip. Each commit's changed paths are placed by the
//! union of its map and its parent's, so a file moved between parts touches both. Strength is by what changed, never by how
//! much: thick when a law on the part, a link at it, a flow through it or its `paths` changed, or it was added; thin when
//! files it owns changed or only its `name`, `caption` or `description` did; nothing for `zone`, `ignored` or `tier`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

use crate::map::{UserState, default_ref, git, read_user, update_user};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    Thin,
    Thick,
}

/// One commit that changed a part and that Daniel has not caught up on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub commit: String,
    pub strength: Strength,
}

/// A project's fog, in `map-state.json`: the commit of the first open, the last commit walked, and each part's entries.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct FogState {
    #[serde(default)]
    pub first: Option<String>,
    #[serde(default)]
    pub processed: Option<String>,
    #[serde(default)]
    pub parts: BTreeMap<String, Vec<Entry>>,
}

/// How strongly each part of `source` shows fog under Daniel's effective tiers: black box never, interface only for thick.
/// A part that is no longer in the map shows nothing.
pub fn effective(user: &UserState, source: &str) -> BTreeMap<String, Strength> {
    let Ok(map) = serde_json::from_str::<Value>(source) else {
        return BTreeMap::new();
    };
    let mut shown = BTreeMap::new();
    for part in map["parts"].as_array().into_iter().flatten() {
        let Some(id) = part["id"].as_str() else {
            continue;
        };
        let tier = user
            .tiers
            .get(id)
            .map(String::as_str)
            .or(part["tier"].as_str())
            .unwrap_or("core");
        let own = user.fog.parts.get(id).into_iter().flatten();
        let strength = match tier {
            "black-box" => None,
            "interface" => own
                .map(|e| e.strength)
                .filter(|s| *s == Strength::Thick)
                .max(),
            _ => own.map(|e| e.strength).max(),
        };
        if let Some(strength) = strength {
            shown.insert(id.to_string(), strength);
        }
    }
    shown
}

/// Whether a part whose effective tier is Core shows fog: Home's quiet mark.
pub fn core_fogged(user: &UserState, source: &str) -> bool {
    let shown = effective(user, source);
    let Ok(map) = serde_json::from_str::<Value>(source) else {
        return false;
    };
    map["parts"].as_array().into_iter().flatten().any(|part| {
        let id = part["id"].as_str().unwrap_or_default();
        let tier = user
            .tiers
            .get(id)
            .map(String::as_str)
            .or(part["tier"].as_str());
        shown.contains_key(id) && tier == Some("core")
    })
}

/// The items of `map[key]` as a list, empty when the key is missing.
fn items<'a>(map: &'a Value, key: &str) -> Vec<&'a Value> {
    map[key]
        .as_array()
        .map(|a| a.iter().collect())
        .unwrap_or_default()
}

fn named<'a>(map: &'a Value, key: &str, id: &str) -> Option<&'a Value> {
    items(map, key).into_iter().find(|item| item["id"] == id)
}

/// What a map says about `part` that makes a change to it structural: the laws on it, the links at it, the flows through them.
fn structure(map: &Value, part: &str) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let pick = |item: &Value, fields: &[&str]| {
        Value::Object(
            fields
                .iter()
                .map(|f| (f.to_string(), item[*f].clone()))
                .collect(),
        )
    };
    let laws = items(map, "laws")
        .into_iter()
        .filter(|l| l["on"] == part)
        .map(|l| pick(l, &["id", "on", "backing", "text"]))
        .collect();
    let touching: Vec<&Value> = items(map, "links")
        .into_iter()
        .filter(|l| l["from"] == part || l["to"] == part)
        .collect();
    let flows = items(map, "flows")
        .into_iter()
        .filter(|f| {
            items(f, "hops")
                .iter()
                .any(|h| touching.iter().any(|l| h["link"] == l["id"]))
        })
        .cloned()
        .collect();
    let links = touching
        .into_iter()
        .map(|l| pick(l, &["id", "from", "to"]))
        .collect();
    (laws, links, flows)
}

/// The strength each live part of `new` gets from a commit that changed `changed`, against the map `old` of its parent.
/// A side whose map is missing or does not read gives no fog: nothing can be told apart.
pub fn classify(
    old: Option<&str>,
    new: Option<&str>,
    changed: &[String],
) -> BTreeMap<String, Strength> {
    let mut found = BTreeMap::new();
    let (Some(old), Some(new)) = (old, new) else {
        return found;
    };
    let (Ok(old_map), Ok(new_map)) = (
        system_map::parse_lenient(old),
        system_map::parse_lenient(new),
    ) else {
        return found;
    };
    let (Ok(old), Ok(new)) = (
        serde_json::from_str::<Value>(old),
        serde_json::from_str::<Value>(new),
    ) else {
        return found;
    };
    // The union of both maps places the paths, so a file that changed hands touches both parts.
    let touched: Vec<String> = [&old_map, &new_map]
        .iter()
        .flat_map(|map| system_map::touched_for_map(map, changed).parts)
        .map(|part| part.id)
        .collect();
    for part in items(&new, "parts") {
        let Some(id) = part["id"].as_str() else {
            continue;
        };
        let strength = match named(&old, "parts", id) {
            None => Some(Strength::Thick),
            Some(before)
                if before["paths"] != part["paths"]
                    || structure(&old, id) != structure(&new, id) =>
            {
                Some(Strength::Thick)
            }
            Some(before) => (touched.iter().any(|t| t == id)
                || ["name", "caption", "description"]
                    .iter()
                    .any(|f| before[*f] != part[*f]))
            .then_some(Strength::Thin),
        };
        if let Some(strength) = strength {
            found.insert(id.to_string(), strength);
        }
    }
    found
}

fn text_at(repo: &Path, commit: &str) -> Option<String> {
    git(
        repo,
        &["show", &format!("{commit}:{}", system_map::MAP_FILE)],
    )
    .ok()
}

/// Walks the commits that landed on the default branch since the last walk and files their fog; the first walk of a project
/// records the tip as the start when `open` (the view being opened), so every part starts clear, and does nothing otherwise.
pub fn refresh(repo: &Path, file: &Path, project: &str, open: bool) -> Result<(), String> {
    let Some((reference, _)) = default_ref(repo) else {
        return Ok(());
    };
    let head = git(repo, &["rev-parse", &reference])?;
    let fog = read_user(file, project).fog;
    let Some(done) = fog.processed else {
        if open {
            update_user(file, project, |u| {
                u.fog.first = Some(head.clone());
                u.fog.processed = Some(head.clone());
            })?;
        }
        return Ok(());
    };
    if done == head {
        return Ok(());
    }
    // A tip that no longer descends from the last one (a rewritten branch) starts the walk over from the tip, clear.
    let mut found: Vec<(String, BTreeMap<String, Strength>)> = Vec::new();
    if git(repo, &["merge-base", "--is-ancestor", &done, &head]).is_ok() {
        let commits = git(
            repo,
            &[
                "rev-list",
                "--first-parent",
                "--reverse",
                &format!("{done}..{head}"),
            ],
        )?;
        for commit in commits.lines() {
            let parent = format!("{commit}^");
            let Ok(changed) = git(
                repo,
                &["diff", "--no-renames", "--name-only", &parent, commit],
            ) else {
                continue;
            };
            let changed: Vec<String> = changed.lines().map(String::from).collect();
            found.push((
                commit.to_string(),
                classify(
                    text_at(repo, &parent).as_deref(),
                    text_at(repo, commit).as_deref(),
                    &changed,
                ),
            ));
        }
    }
    update_user(file, project, |u| {
        for (commit, strengths) in found {
            for (part, strength) in strengths {
                u.fog.parts.entry(part).or_default().push(Entry {
                    commit: commit.clone(),
                    strength,
                });
            }
        }
        u.fog.processed = Some(head);
    })?;
    Ok(())
}

/// "I know this": clears every entry of `part`.
pub fn clear(file: &Path, project: &str, part: &str) -> Result<UserState, String> {
    update_user(file, project, |u| {
        u.fog.parts.remove(part);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_test_support::{git as run, repo};

    const BASE: &str = r#"{"version":1,
      "parts":[
        {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
        {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}],
      "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":["ign/"],
      "laws":[{"id":"l1","on":"a","backing":"trust","text":"t"}]}"#;

    struct Rig {
        dir: tempfile::TempDir,
        main: String,
        state: tempfile::TempDir,
    }
    impl Rig {
        fn new() -> Rig {
            let (dir, main) = repo();
            let rig = Rig {
                dir,
                main,
                state: tempfile::tempdir().unwrap(),
            };
            rig.commit(&[
                ("system-map.json", BASE),
                ("a/x", "1"),
                ("b/x", "1"),
                ("c/x", "1"),
                ("ign/x", "1"),
            ]);
            rig
        }
        fn file(&self) -> std::path::PathBuf {
            self.state.path().join("map-state.json")
        }
        fn commit(&self, files: &[(&str, &str)]) {
            for (name, text) in files {
                let path = self.dir.path().join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            run(self.dir.path(), &["add", "-A"]);
            run(self.dir.path(), &["commit", "-q", "-m", "change"]);
        }
        fn open(&self) {
            refresh(Path::new(&self.main), &self.file(), &self.main, true).unwrap();
        }
        fn fog(&self) -> BTreeMap<String, Strength> {
            refresh(Path::new(&self.main), &self.file(), &self.main, false).unwrap();
            let user = read_user(&self.file(), &self.main);
            effective(
                &user,
                &run(
                    self.dir.path(),
                    &["show", &format!("main:{}", "system-map.json")],
                ),
            )
        }
        fn raw(&self) -> FogState {
            read_user(&self.file(), &self.main).fog
        }
    }

    fn strengths(pairs: &[(&str, Strength)]) -> BTreeMap<String, Strength> {
        pairs.iter().map(|(id, s)| (id.to_string(), *s)).collect()
    }

    #[test]
    fn the_first_open_starts_clear_and_later_commits_fog() {
        let rig = Rig::new();
        assert!(rig.fog().is_empty());
        assert_eq!(rig.raw().first, None, "without an open there is no start");
        rig.open();
        assert!(rig.fog().is_empty());
        assert!(rig.raw().first.is_some());
        rig.commit(&[("a/x", "2")]);
        assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
    }

    #[test]
    fn internal_edits_are_thin_and_never_count() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[("a/x", "2")]);
        rig.commit(&[("a/x", "3"), ("a/y", "1")]);
        rig.commit(&[("c/x", "2")]);
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thin), ("c", Strength::Thin)])
        );
        assert_eq!(rig.raw().parts["a"].len(), 2);
    }

    #[test]
    fn a_thick_change_beats_thin() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[("a/x", "2")]);
        rig.commit(&[("system-map.json", &BASE.replace("\"t\"", "\"changed\""))]);
        assert_eq!(rig.fog(), strengths(&[("a", Strength::Thick)]));
    }

    #[test]
    fn a_new_link_is_thick_on_both_ends_and_a_new_flow_on_the_parts_it_crosses() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[(
            "system-map.json",
            &BASE.replace(
                r#""links":[{"id":"a-b","from":"a","to":"b"}]"#,
                r#""links":[{"id":"a-b","from":"a","to":"b"},{"id":"b-c","from":"b","to":"c"}]"#,
            ),
        )]);
        assert_eq!(
            rig.fog(),
            strengths(&[("b", Strength::Thick), ("c", Strength::Thick)])
        );
        let rig = Rig::new();
        rig.open();
        let flow = r#""flows":[{"id":"f","name":"F","hops":[{"link":"a-b","text":"x"}]}],"laws""#;
        rig.commit(&[("system-map.json", &BASE.replace(r#""laws""#, flow))]);
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thick), ("b", Strength::Thick)])
        );
    }

    #[test]
    fn moving_a_path_between_parts_touches_both_by_the_union_of_the_maps() {
        let rig = Rig::new();
        rig.open();
        // `a` hands `a/` to `c`, and a file in it changes in the same commit.
        let moved = BASE
            .replace(r#""paths":["a/"]"#, r#""paths":[]"#)
            .replace(r#""paths":["c/"]"#, r#""paths":["c/","a/"]"#);
        rig.commit(&[("system-map.json", &moved), ("a/x", "2")]);
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thick), ("c", Strength::Thick)])
        );
    }

    #[test]
    fn a_file_moved_without_changing_the_map_fogs_its_source_and_destination() {
        let rig = Rig::new();
        rig.open();
        run(rig.dir.path(), &["mv", "a/x", "b/moved"]);
        run(rig.dir.path(), &["commit", "-q", "-m", "move"]);
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
        );
    }

    #[test]
    fn a_part_added_is_thick_and_one_removed_drops_out() {
        let rig = Rig::new();
        rig.open();
        let added = BASE.replace(r#""parts":["#, r#""parts":[{"id":"d","name":"D","caption":"c","description":"d","tier":"core","paths":["d/"]},"#);
        rig.commit(&[("system-map.json", &added), ("d/x", "1")]);
        assert_eq!(rig.fog(), strengths(&[("d", Strength::Thick)]));
        // Removing `c`: its own files go too, and it shows nothing.
        let removed = added.replace(
            r#",
        {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}"#,
            "",
        );
        assert_ne!(removed, added);
        run(rig.dir.path(), &["rm", "-q", "-r", "c"]);
        rig.commit(&[("system-map.json", &removed)]);
        assert!(!rig.fog().contains_key("c"));
    }

    #[test]
    fn a_new_caption_or_description_is_thin_and_zone_ignored_and_tier_are_nothing() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[(
            "system-map.json",
            &BASE.replace(
                r#""id":"c","name":"C","caption":"c""#,
                r#""id":"c","name":"C","caption":"new words""#,
            ),
        )]);
        assert_eq!(rig.fog(), strengths(&[("c", Strength::Thin)]));
        let rig = Rig::new();
        rig.open();
        let quiet = BASE
            .replace(r#""paths":["b/"]}"#, r#""paths":["b/"],"zone":"left"}"#)
            .replace(r#""ignored":["ign/"]"#, r#""ignored":["ign/","other/"]"#)
            .replace(
                r#""id":"c","name":"C","caption":"c","description":"d","tier":"core""#,
                r#""id":"c","name":"C","caption":"c","description":"d","tier":"interface""#,
            );
        rig.commit(&[("system-map.json", &quiet), ("ign/x", "2")]);
        assert!(rig.fog().is_empty(), "{:?}", rig.fog());
    }

    #[test]
    fn a_map_with_fields_this_build_does_not_know_still_walks() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[
            (
                "system-map.json",
                &BASE.replace(r#""version":1,"#, r#""version":1,"future":{"x":1},"#),
            ),
            ("a/x", "2"),
        ]);
        assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
    }

    #[test]
    fn daniels_tier_wins_and_black_box_and_interface_filter() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[("a/x", "2"), ("b/x", "2")]);
        rig.commit(&[("system-map.json", &BASE.replace("\"t\"", "\"changed\""))]);
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thick), ("b", Strength::Thin)])
        );
        let source = run(rig.dir.path(), &["show", "main:system-map.json"]);
        let mut user = read_user(&rig.file(), &rig.main);
        // The map says core; Daniel says black box for a, interface for b.
        user.tiers.insert("a".into(), "black-box".into());
        user.tiers.insert("b".into(), "interface".into());
        assert!(effective(&user, &source).is_empty());
        assert!(!core_fogged(&user, &source));
        // Interface shows a thick entry.
        user.tiers.insert("a".into(), "interface".into());
        assert_eq!(
            effective(&user, &source),
            strengths(&[("a", Strength::Thick)])
        );
        assert!(
            !core_fogged(&user, &source),
            "an interface part is not Core"
        );
        // Daniel's core over the map's own black box.
        let black = source.replace(
            r#""id":"b","name":"B","caption":"c","description":"d","tier":"core""#,
            r#""id":"b","name":"B","caption":"c","description":"d","tier":"black-box""#,
        );
        user.tiers.clear();
        assert_eq!(
            effective(&user, &black),
            strengths(&[("a", Strength::Thick)])
        );
        user.tiers.insert("b".into(), "core".into());
        assert_eq!(
            effective(&user, &black),
            strengths(&[("a", Strength::Thick), ("b", Strength::Thin)])
        );
        assert!(core_fogged(&user, &black));
    }

    #[test]
    fn i_know_this_clears_a_part_and_the_clear_persists() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[("a/x", "2"), ("b/x", "2")]);
        assert_eq!(rig.fog().len(), 2);
        clear(&rig.file(), &rig.main, "a").unwrap();
        assert_eq!(rig.fog(), strengths(&[("b", Strength::Thin)]));
        // Persisted, and a later walk does not bring back what was cleared.
        assert!(rig.raw().parts.get("a").is_none_or(Vec::is_empty));
        assert_eq!(rig.fog(), strengths(&[("b", Strength::Thin)]));
        rig.commit(&[("a/x", "3")]);
        assert_eq!(rig.fog().len(), 2);
    }

    #[test]
    fn an_older_state_file_without_fog_reads() {
        let rig = Rig::new();
        std::fs::write(
            rig.file(),
            format!(r#"{{"projects":{{"{}":{{"positions":{{}}}}}}}}"#, rig.main),
        )
        .unwrap();
        assert_eq!(rig.raw(), FogState::default());
    }
}
