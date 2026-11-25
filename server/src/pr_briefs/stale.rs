//! Whether a brief's base went stale on the default branch: its root manifests and the map's moves.

use serde::Serialize;
use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

/// The build manifests at the repository's root whose change on the default branch bears on every brief (#494). A lockfile or a
/// manifest below the root is not one.
const ROOT_MANIFESTS: [&str; 4] = [
    "Cargo.toml",
    "package.json",
    "rust-toolchain.toml",
    "tsconfig.json",
];

/// What the default branch changed since a PR's merge base that the brief relies on (#494), each list sorted and each id once: the
/// map's laws, links and flows whose say on a lit part changed, the lit parts whose `paths` changed, and the changed files of lit
/// parts and root manifests.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StaleBase {
    pub laws: Vec<String>,
    pub links: Vec<String>,
    pub flows: Vec<String>,
    pub paths: Vec<String>,
    pub files: Vec<String>,
}

/// What the default branch of `repo` changed, by one net diff from the merge base of `head` to its tip, that bears on the parts in
/// `lit`: none when nothing did, or when git cannot answer.
pub fn stale_base(repo: &Path, head: &str, lit: &BTreeSet<String>) -> Option<StaleBase> {
    use crate::map::git;
    let (tip, _) = crate::map::default_ref(repo)?;
    let base = git(repo, &["merge-base", &tip, head]).ok()?;
    let changed = git(repo, &["diff", "--name-only", &base, &tip]).ok()?;
    let changed: Vec<String> = changed.lines().map(String::from).collect();
    let map_at = |commit: &str| -> serde_json::Value {
        git(
            repo,
            &["show", &format!("{commit}:{}", system_map::MAP_FILE)],
        )
        .ok()
        .and_then(|text| system_map::convert_value(&text).ok())
        .unwrap_or_default()
    };
    let new_map = map_at(&tip);
    let mut stale = StaleBase::default();
    // (i) A changed file a lit part owns by the current map, and (iii) a root build manifest.
    if let Ok(map) = system_map::parse_lenient(&new_map.to_string()) {
        let touched = system_map::touched_for_map(&map, &changed);
        stale.files = touched
            .parts
            .into_iter()
            .filter(|p| lit.contains(&p.id))
            .flat_map(|p| p.files)
            .collect();
    }
    stale.files.extend(
        changed
            .iter()
            .filter(|path| ROOT_MANIFESTS.contains(&path.as_str()))
            .cloned(),
    );
    // (ii) The map's say on a lit part, read as the brief reads it: laws on it or on a link at it, links at it, flows through those.
    if changed.iter().any(|path| path == system_map::MAP_FILE) {
        let old_map = map_at(&base);
        for part in lit {
            map_moves(&old_map, &new_map, part, &mut stale);
        }
    }
    for list in [
        &mut stale.laws,
        &mut stale.links,
        &mut stale.flows,
        &mut stale.paths,
        &mut stale.files,
    ] {
        list.sort();
        list.dedup();
    }
    (stale != StaleBase::default()).then_some(stale)
}

/// Adds to `stale` what differs between `old` and `new` in the map's say on `part`: an item counts when it is at the part in either
/// map and its value is not the same in both.
fn map_moves(old: &serde_json::Value, new: &serde_json::Value, part: &str, stale: &mut StaleBase) {
    use serde_json::Value;
    let items =
        |map: &Value, key: &str| -> Vec<Value> { map[key].as_array().cloned().unwrap_or_default() };
    let id = |item: &Value| item["id"].as_str().unwrap_or_default().to_string();
    let links_at = |map: &Value| -> BTreeSet<String> {
        items(map, "links")
            .iter()
            .filter(|l| l["from"] == part || l["to"] == part)
            .map(id)
            .collect()
    };
    let links: BTreeSet<String> = links_at(old).union(&links_at(new)).cloned().collect();
    let at = |link: &Value| links.contains(link.as_str().unwrap_or_default());
    let moved = |key: &str, here: &dyn Fn(&Value) -> bool| -> Vec<String> {
        let of = |map: &Value| -> HashMap<String, Value> {
            items(map, key)
                .into_iter()
                .filter(|i| here(i))
                .map(|i| (id(&i), i))
                .collect()
        };
        let (was, now) = (of(old), of(new));
        was.keys()
            .chain(now.keys())
            .filter(|k| was.get(*k) != now.get(*k))
            .cloned()
            .collect()
    };
    stale
        .laws
        .extend(moved("laws", &|l| l["on"] == part || at(&l["on"])));
    stale.links.extend(moved("links", &|l| at(&l["id"])));
    stale.flows.extend(moved("flows", &|f| {
        f["hops"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|h| at(&h["link"]))
    }));
    let paths_of = |map: &Value| {
        items(map, "parts")
            .into_iter()
            .find(|p| p["id"] == part)
            .map(|p| p["paths"].clone())
    };
    if paths_of(old) != paths_of(new) {
        stale.paths.push(part.to_string());
    }
}
