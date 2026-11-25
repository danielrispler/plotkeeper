//! The fog of the Map view (spec danielrispler/plotkeeper#306, ticket #310): what landed on `origin`'s default branch since
//! Daniel last caught up, kept per part as a list of commits, each `thin` or `thick`. Never a count: the window shows only the
//! strongest, and only what Daniel's effective tier lets through.
//!
//! The walk is first-parent, from the commit last processed to the ref's tip. Each commit's changed paths are placed by the
//! union of its map and its parent's, so a file moved between parts touches both. Strength is by what changed, never by how
//! much: thick when a law on the part, a link at it, a flow through it or its `paths` changed, or it was added; thin when
//! files it owns changed or only its `name`, `caption`, `purpose` or `holds` did (a version 1 map's `description` read as its
//! `purpose`); nothing for `zone`, `ignored` or `tier`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    path::Path,
};

use crate::map::{UserState, default_ref, git, read_user, update_user};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Strength {
    Thin,
    Thick,
}

/// One commit that changed a part and that Daniel has not caught up on: the PR it came from, when its subject names one, and why it
/// fogs the part (`Change::why`). An entry filed before #495 has neither.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub commit: String,
    pub strength: Strength,
    #[serde(default)]
    pub pr: Option<u64>,
    #[serde(default)]
    pub why: Vec<String>,
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
    /// What Daniel has read of each PR's brief, by PR number (spec #334, ticket #342).
    #[serde(default)]
    pub reads: BTreeMap<u64, PrRead>,
}

/// A chapter read to its end: the PR head the brief was written for, and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChapterRead {
    pub head: String,
    pub at: u64,
}

/// How far Daniel has read a PR's brief: the parts each chapter lights, in the brief's order, and the chapters finished.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrRead {
    #[serde(default)]
    pub lit: Vec<Vec<String>>,
    #[serde(default)]
    pub chapters: BTreeMap<usize, ChapterRead>,
}

impl PrRead {
    /// The parts whose every lighting chapter is read; a part no chapter lights is never in it.
    pub fn complete_parts(&self) -> BTreeSet<String> {
        let parts: BTreeSet<&String> = self.lit.iter().flatten().collect();
        parts
            .into_iter()
            .filter(|part| {
                self.lit
                    .iter()
                    .enumerate()
                    .filter(|(_, lit)| lit.contains(part))
                    .all(|(i, _)| self.chapters.contains_key(&i))
            })
            .cloned()
            .collect()
    }

    /// Every chapter of the brief is read.
    pub fn fully_read(&self) -> bool {
        !self.lit.is_empty() && (0..self.lit.len()).all(|i| self.chapters.contains_key(&i))
    }

    /// Every chapter is read, each at `head`: a read of an older head does not count once the PR moved on.
    pub fn fully_read_at(&self, head: &str) -> bool {
        self.fully_read() && self.chapters.values().all(|read| read.head == head)
    }

    /// The head `part` was read at: the earliest read of the chapters that light it, so a push after any of them is seen.
    fn read_head(&self, part: &str) -> Option<&str> {
        self.lit
            .iter()
            .enumerate()
            .filter(|(_, lit)| lit.iter().any(|p| p == part))
            .filter_map(|(i, _)| self.chapters.get(&i))
            .min_by_key(|read| read.at)
            .map(|read| read.head.as_str())
    }
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

/// Whether a part whose effective tier is Core shows fog: the project header's quiet dot.
pub fn core_fogged(user: &UserState, source: &str) -> bool {
    !core_fogged_parts(user, source).is_empty()
}

/// The parts whose effective tier is Core and that show fog, in the map's order.
pub fn core_fogged_parts(user: &UserState, source: &str) -> Vec<String> {
    let shown = effective(user, source);
    let Ok(map) = serde_json::from_str::<Value>(source) else {
        return vec![];
    };
    map["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            let id = part["id"].as_str().unwrap_or_default();
            let tier = user
                .tiers
                .get(id)
                .map(String::as_str)
                .or(part["tier"].as_str())
                .unwrap_or("core");
            (shown.contains_key(id) && tier == "core").then(|| id.to_string())
        })
        .collect()
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

/// The words of the links at `part`: each one's id with its kind and reason, whose change is thin for the parts at its ends.
fn link_words(map: &Value, part: &str) -> Vec<(Value, Value, Value)> {
    items(map, "links")
        .into_iter()
        .filter(|l| l["from"] == part || l["to"] == part)
        .map(|l| (l["id"].clone(), l["kind"].clone(), l["reason"].clone()))
        .collect()
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

/// What a commit did to a part: its strength, and why, in the order the sheet tells it (#495): "added", "paths", each law, link and flow
/// whose say on the part changed (`law <id>`, `link <id>`, `flow <id>`; a link's kind or reason too), each of the part's own words that
/// changed (`name`, `caption`, `purpose`, `holds`), and the count of its files changed (`3 files`).
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub strength: Strength,
    pub why: Vec<String>,
}

/// The ids, each with its kind's word, of the items whose value differs between `old` and `new`, in `new`'s order, then the removed.
fn changed_ids(kind: &str, old: &[Value], new: &[Value]) -> Vec<String> {
    let id = |item: &Value| item["id"].as_str().unwrap_or_default().to_string();
    let mut ids: Vec<String> = Vec::new();
    for item in new.iter().chain(old) {
        let key = id(item);
        let before = old.iter().find(|o| id(o) == key);
        let after = new.iter().find(|n| id(n) == key);
        if before != after && !ids.contains(&key) {
            ids.push(key);
        }
    }
    ids.into_iter().map(|key| format!("{kind} {key}")).collect()
}

/// The strength each live part of `new` gets from a commit that changed `changed`, against the map `old` of its parent.
/// A side whose map is missing or does not read gives no fog: nothing can be told apart.
pub fn classify(
    old: Option<&str>,
    new: Option<&str>,
    changed: &[String],
) -> BTreeMap<String, Strength> {
    explain(old, new, changed)
        .into_iter()
        .map(|(id, change)| (id, change.strength))
        .collect()
}

/// `classify`, with why each part fogs.
pub fn explain(
    old: Option<&str>,
    new: Option<&str>,
    changed: &[String],
) -> BTreeMap<String, Change> {
    let mut found = BTreeMap::new();
    let (Some(old), Some(new)) = (old, new) else {
        return found;
    };
    // Both sides as version 2, so a version 1 map's `description` is compared as its `purpose`.
    let (Ok(old_map), Ok(new_map)) = (
        system_map::parse_lenient(old),
        system_map::parse_lenient(new),
    ) else {
        return found;
    };
    let (Ok(old), Ok(new)) = (
        system_map::convert_value(old),
        system_map::convert_value(new),
    ) else {
        return found;
    };
    // The union of both maps places the paths, so a file that changed hands touches both parts.
    let mut files: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for map in [&old_map, &new_map] {
        for part in system_map::touched_for_map(map, changed).parts {
            files.entry(part.id).or_default().extend(part.files);
        }
    }
    for part in items(&new, "parts") {
        let Some(id) = part["id"].as_str() else {
            continue;
        };
        let mut why = Vec::new();
        let strength = match named(&old, "parts", id) {
            None => {
                why.push("added".to_string());
                Some(Strength::Thick)
            }
            Some(before) => {
                let (old_laws, old_links, old_flows) = structure(&old, id);
                let (new_laws, new_links, new_flows) = structure(&new, id);
                if before["paths"] != part["paths"] {
                    why.push("paths".to_string());
                }
                why.extend(changed_ids("law", &old_laws, &new_laws));
                let mut links = changed_ids("link", &old_links, &new_links);
                let thick = before["paths"] != part["paths"]
                    || (&old_laws, &old_links, &old_flows) != (&new_laws, &new_links, &new_flows);
                // A link's kind or reason is thin; its id is told once.
                let words = |map: &Value| {
                    link_words(map, id)
                        .into_iter()
                        .map(|(id, kind, reason)| {
                            serde_json::json!({"id": id, "kind": kind, "reason": reason})
                        })
                        .collect::<Vec<_>>()
                };
                for link in changed_ids("link", &words(&old), &words(&new)) {
                    if !links.contains(&link) {
                        links.push(link);
                    }
                }
                why.extend(links);
                why.extend(changed_ids("flow", &old_flows, &new_flows));
                why.extend(
                    ["name", "caption", "purpose", "holds"]
                        .iter()
                        .filter(|f| before[**f] != part[**f])
                        .map(|f| f.to_string()),
                );
                if thick {
                    Some(Strength::Thick)
                } else {
                    (!why.is_empty() || files.contains_key(id)).then_some(Strength::Thin)
                }
            }
        };
        if let Some(strength) = strength {
            if let Some(count) = files.get(id).map(BTreeSet::len) {
                why.push(format!("{count} file{}", if count == 1 { "" } else { "s" }));
            }
            found.insert(id.to_string(), Change { strength, why });
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
    let mut found: Vec<(String, Option<u64>, BTreeMap<String, Change>)> = Vec::new();
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
            let mut changes = explain(
                text_at(repo, &parent).as_deref(),
                text_at(repo, commit).as_deref(),
                &changed,
            );
            let pr = subject_pr(repo, commit);
            // A PR whose brief was read before it merged files only what changed since.
            if let Some(read) = pr.and_then(|pr| fog.reads.get(&pr)) {
                after_reading(repo, commit, &changed, read, &mut changes);
            }
            found.push((commit.to_string(), pr, changes));
        }
    }
    update_user(file, project, |u| apply(&mut u.fog, &done, head, found))?;
    Ok(())
}

/// The PR a squash-merge commit came from, read from its subject `Title (#N)`.
pub(crate) fn subject_pr(repo: &Path, commit: &str) -> Option<u64> {
    let subject = git(repo, &["log", "-1", "--format=%s", commit]).ok()?;
    let (_, number) = subject.trim_end().strip_suffix(')')?.rsplit_once(" (#")?;
    number.parse().ok()
}

/// What changed in `squash`'s paths since the head `x` a brief was read at, with the strength that diff earns. `None` when `x`
/// cannot be diffed (not in this repository): the commit's own fog stands then.
fn late_fog(
    repo: &Path,
    x: &str,
    squash: &str,
    squash_paths: &[String],
) -> Option<BTreeMap<String, Change>> {
    let old = text_at(repo, x)?;
    let since = git(repo, &["diff", "--no-renames", "--name-only", x, squash]).ok()?;
    // Main's changes merged into the branch are in the diff from `x` but not in the squash commit's own: they fog nothing.
    let changed: Vec<String> = since
        .lines()
        .filter(|path| squash_paths.iter().any(|p| p == path))
        .map(String::from)
        .collect();
    Some(explain(
        Some(&old),
        text_at(repo, squash).as_deref(),
        &changed,
    ))
}

/// Replaces, for each part whose brief chapters were all read, the squash commit's strength by what changed after the read.
fn after_reading(
    repo: &Path,
    squash: &str,
    squash_paths: &[String],
    read: &PrRead,
    strengths: &mut BTreeMap<String, Change>,
) {
    let mut late: BTreeMap<String, Option<BTreeMap<String, Change>>> = BTreeMap::new();
    for part in read.complete_parts() {
        let Some(head) = read.read_head(&part) else {
            continue;
        };
        let since = late
            .entry(head.to_string())
            .or_insert_with(|| late_fog(repo, head, squash, squash_paths));
        match since.as_ref().map(|s| s.get(&part)) {
            Some(Some(change)) => {
                strengths.insert(part, change.clone());
            }
            Some(None) => {
                strengths.remove(&part);
            }
            None => {}
        }
    }
}

/// Files a finished walk from `done` to `head`, unless the stored walk has moved since (an overlapping refresh filed it first,
/// or "I know this" cleared in between): then the result is dropped, so nothing is filed twice or brought back.
fn apply(
    fog: &mut FogState,
    done: &str,
    head: String,
    found: Vec<(String, Option<u64>, BTreeMap<String, Change>)>,
) {
    if fog.processed.as_deref() != Some(done) {
        return;
    }
    for (commit, pr, changes) in found {
        for (part, change) in changes {
            let entries = fog.parts.entry(part).or_default();
            if !entries.iter().any(|e| e.commit == commit) {
                entries.push(Entry {
                    commit: commit.clone(),
                    strength: change.strength,
                    pr,
                    why: change.why,
                });
            }
        }
    }
    fog.processed = Some(head);
}

/// Clears the entries of `commits` from `parts`.
pub fn clear_commits(
    file: &Path,
    project: &str,
    commits: &HashSet<String>,
    parts: &BTreeSet<String>,
) -> Result<UserState, String> {
    update_user(file, project, |u| {
        for part in parts {
            if let Some(entries) = u.fog.parts.get_mut(part) {
                entries.retain(|e| !commits.contains(&e.commit));
                if entries.is_empty() {
                    u.fog.parts.remove(part);
                }
            }
        }
    })
}

/// Chapter `chapter` of PR `pr`'s brief, written for `head`, was read to its end; `lit` is the parts each chapter of that brief
/// lights. Returns the parts whose every chapter is now read. A brief that lights its parts differently starts the count over.
pub fn mark_read(
    file: &Path,
    project: &str,
    pr: u64,
    chapter: usize,
    head: &str,
    lit: Vec<Vec<String>>,
    at: u64,
) -> Result<BTreeSet<String>, String> {
    let user = update_user(file, project, |u| {
        let read = u.fog.reads.entry(pr).or_default();
        if read.lit != lit {
            read.chapters.clear();
            read.lit = lit;
        }
        // Staleness keeps the read's time too, which a rewritten brief's cleared reads would lose (#392).
        for part in read.lit.get(chapter).cloned().unwrap_or_default() {
            crate::staleness::touch(u, &part, crate::staleness::Activity::Chapter, at);
        }
        let read = u.fog.reads.entry(pr).or_default();
        read.chapters.insert(
            chapter,
            ChapterRead {
                head: head.to_string(),
                at,
            },
        );
    })?;
    Ok(user.fog.reads[&pr].complete_parts())
}

/// The read of PR `pr`'s brief at `from` carries over to `to` (spec #477, part B: a pure rebase, or a push that fogs nothing): every
/// chapter read at `from` is read at `to`, at the time it was read. The parts the chapters light stay as they are; `mark_read` is not
/// used, since a brief lit differently clears every chapter.
pub fn carry_read(file: &Path, project: &str, pr: u64, from: &str, to: &str) -> Result<(), String> {
    update_user(file, project, |u| {
        if let Some(read) = u.fog.reads.get_mut(&pr) {
            for chapter in read.chapters.values_mut().filter(|c| c.head == from) {
                chapter.head = to.to_string();
            }
        }
    })
    .map(|_| ())
}

/// Reading finished a PR's chapters that light `parts`: the fog the PR's merged commits filed on them leaves at once.
pub fn clear_pr(
    repo: &Path,
    file: &Path,
    project: &str,
    pr: u64,
    parts: &BTreeSet<String>,
) -> Result<UserState, String> {
    let fog = read_user(file, project).fog;
    let commits: HashSet<String> = parts
        .iter()
        .filter_map(|part| fog.parts.get(part))
        .flatten()
        .map(|e| e.commit.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .filter(|commit| subject_pr(repo, commit) == Some(pr))
        .collect();
    clear_commits(file, project, &commits, parts)
}

/// Whether every chapter of PR `pr`'s brief is read.
pub fn fully_read(user: &UserState, pr: u64) -> bool {
    user.fog.reads.get(&pr).is_some_and(PrRead::fully_read)
}

/// Whether every chapter of PR `pr`'s brief is read at `head`.
pub fn fully_read_at(user: &UserState, pr: u64, head: &str) -> bool {
    user.fog
        .reads
        .get(&pr)
        .is_some_and(|read| read.fully_read_at(head))
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

    /// `BASE` as version 2: each `description` a `purpose`, with no held items.
    fn base_v2() -> String {
        BASE.replace(r#""version":1,"#, r#""version":2,"#)
            .replace(r#""description":"d""#, r#""purpose":"d","holds":[]"#)
    }

    #[test]
    fn an_edited_description_in_a_version_1_map_is_thin() {
        let edited = BASE.replacen(r#""description":"d""#, r#""description":"new words""#, 1);
        assert_eq!(
            classify(Some(BASE), Some(&edited), &[]),
            strengths(&[("a", Strength::Thin)])
        );
    }

    #[test]
    fn an_edited_purpose_or_held_item_is_thin() {
        let v2 = base_v2();
        let purpose = v2.replacen(r#""purpose":"d""#, r#""purpose":"new words""#, 1);
        assert_eq!(
            classify(Some(&v2), Some(&purpose), &[]),
            strengths(&[("a", Strength::Thin)])
        );
        let held = v2.replacen(
            r#""holds":[]"#,
            r#""holds":[{"name":"Reader","text":"Reads."}]"#,
            1,
        );
        assert_eq!(
            classify(Some(&v2), Some(&held), &[]),
            strengths(&[("a", Strength::Thin)])
        );
        let paths = held.replace(r#""text":"Reads."}"#, r#""text":"Reads.","paths":["a/x"]}"#);
        assert_eq!(
            classify(Some(&held), Some(&paths), &[]),
            strengths(&[("a", Strength::Thin)])
        );
    }

    #[test]
    fn a_link_s_new_kind_or_reason_is_thin_on_both_its_ends() {
        let link = r#"{"id":"a-b","from":"a","to":"b"}"#;
        let v2 = base_v2().replace(link, r#"{"id":"a-b","from":"a","to":"b","kind":"import"}"#);
        let kind = v2.replace(
            r#""kind":"import""#,
            r#""kind":"http","reason":"A asks b over HTTP.""#,
        );
        assert_eq!(
            classify(Some(&v2), Some(&kind), &[]),
            strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
        );
        let reason = kind.replace("A asks b over HTTP.", "A asks b for its list.");
        assert_eq!(
            classify(Some(&kind), Some(&reason), &[]),
            strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
        );
        // A version 1 link, with no kind, given one.
        assert_eq!(
            classify(Some(BASE), Some(&v2), &[]),
            strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
        );
    }

    #[test]
    fn moving_a_map_to_version_2_with_the_same_words_fogs_nothing() {
        assert!(classify(Some(BASE), Some(&base_v2()), &[]).is_empty());
        let reworded = base_v2().replacen(r#""purpose":"d""#, r#""purpose":"Short.""#, 1);
        assert_eq!(
            classify(Some(BASE), Some(&reworded), &[]),
            strengths(&[("a", Strength::Thin)])
        );
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
    fn a_part_with_no_tier_is_core_for_the_header_dot_as_it_is_for_the_map() {
        let source = r#"{"parts":[{"id":"a","name":"A","paths":[]}]}"#;
        let mut user = UserState::default();
        user.fog.parts.insert(
            "a".into(),
            vec![Entry {
                commit: "x".into(),
                strength: Strength::Thin,
                pr: None,
                why: Vec::new(),
            }],
        );
        assert!(effective(&user, source).contains_key("a"));
        assert!(core_fogged(&user, source));
    }

    #[test]
    fn an_overlapping_walk_files_nothing_twice_and_does_not_undo_a_clear() {
        let walk = |commit: &str| {
            let thin = Change {
                strength: Strength::Thin,
                why: Vec::new(),
            };
            vec![(
                commit.to_string(),
                None,
                BTreeMap::from([("a".to_string(), thin)]),
            )]
        };
        let mut fog = FogState {
            processed: Some("d".into()),
            ..FogState::default()
        };
        // Two walks from the same start: the first files, the second finds the walk moved and is dropped.
        apply(&mut fog, "d", "h".into(), walk("x"));
        apply(&mut fog, "d", "h".into(), walk("x"));
        assert_eq!(fog.parts["a"].len(), 1);
        // "I know this" in between is not brought back by the slower walk.
        fog.parts.remove("a");
        apply(&mut fog, "d", "h".into(), walk("x"));
        assert!(!fog.parts.contains_key("a"));
        assert_eq!(fog.processed.as_deref(), Some("h"));
    }

    /// The PR and the reasons of the last entry filed on `part`.
    fn why(rig: &Rig, part: &str) -> (Option<u64>, Vec<String>) {
        rig.fog();
        let entry = rig.raw().parts[part].last().cloned().unwrap();
        (entry.pr, entry.why)
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn a_fog_entry_keeps_its_pr_and_why_for_each_kind_of_change() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[("a/x", "2"), ("a/y", "1"), ("b/x", "2")]);
        assert_eq!(why(&rig, "a"), (None, words(&["2 files"])));
        assert_eq!(why(&rig, "b"), (None, words(&["1 file"])));
        // A squash-merge names its PR; a changed law on a.
        rig.branch();
        rig.commit(&[("system-map.json", LAW_CHANGED)]);
        rig.squash();
        assert_eq!(why(&rig, "a"), (Some(7), words(&["law l1"])));
        // A new link and a flow across it.
        let linked = LAW_CHANGED
            .replace(
                r#""links":[{"id":"a-b","from":"a","to":"b"}]"#,
                r#""links":[{"id":"a-b","from":"a","to":"b"},{"id":"b-c","from":"b","to":"c"}]"#,
            )
            .replace(
                r#""laws""#,
                r#""flows":[{"id":"f","name":"F","hops":[{"link":"b-c","text":"x"}]}],"laws""#,
            );
        rig.commit(&[("system-map.json", &linked)]);
        assert_eq!(why(&rig, "c"), (None, words(&["link b-c", "flow f"])));
        // Paths, and a part added.
        let moved = linked
            .replace(r#""paths":["c/"]"#, r#""paths":["c/","d/"]"#)
            .replace(r#""parts":["#, r#""parts":[{"id":"e","name":"E","caption":"c","description":"d","tier":"core","paths":["e/"]},"#);
        rig.commit(&[("system-map.json", &moved)]);
        assert_eq!(why(&rig, "c"), (None, words(&["paths"])));
        assert_eq!(why(&rig, "e"), (None, words(&["added"])));
        // Only its words: the field that changed.
        rig.commit(&[(
            "system-map.json",
            &moved.replace(
                r#""name":"B","caption":"c""#,
                r#""name":"B","caption":"new""#,
            ),
        )]);
        assert_eq!(why(&rig, "b"), (None, words(&["caption"])));
    }

    #[test]
    fn an_entry_filed_before_it_kept_why_still_loads() {
        let entry: Entry = serde_json::from_str(r#"{"commit":"abc","strength":"thick"}"#).unwrap();
        assert_eq!((entry.pr, entry.why), (None, Vec::new()));
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

    // ---- Reading a brief clears the fog it would cause (spec #334, ticket #342).

    impl Rig {
        fn head(&self) -> String {
            run(self.dir.path(), &["rev-parse", "HEAD"])
        }
        /// Starts a PR branch from main.
        fn branch(&self) {
            run(self.dir.path(), &["checkout", "-q", "-b", "pr"]);
        }
        /// Squash-merges the PR branch into main, as `Title (#7)`.
        fn squash(&self) {
            run(self.dir.path(), &["checkout", "-q", "main"]);
            run(self.dir.path(), &["merge", "--squash", "pr"]);
            run(self.dir.path(), &["commit", "-q", "-m", "Title (#7)"]);
        }
        fn read(&self, chapter: usize, head: &str, lit: &[&[&str]], at: u64) -> BTreeSet<String> {
            let lit = lit
                .iter()
                .map(|c| c.iter().map(|p| p.to_string()).collect())
                .collect();
            mark_read(&self.file(), &self.main, 7, chapter, head, lit, at).unwrap()
        }
        /// "I know this" on every part.
        fn clear_all(&self) {
            for part in ["a", "b", "c"] {
                clear(&self.file(), &self.main, part).unwrap();
            }
        }
        fn set(&self, parts: &[&str]) -> BTreeSet<String> {
            parts.iter().map(|p| p.to_string()).collect()
        }
    }

    const LAW_CHANGED: &str = r#"{"version":1,
      "parts":[
        {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
        {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}],
      "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":["ign/"],
      "laws":[{"id":"l1","on":"a","backing":"trust","text":"changed"}]}"#;

    #[test]
    fn clearing_a_set_of_commits_from_a_set_of_parts_leaves_the_rest() {
        let rig = Rig::new();
        rig.open();
        rig.commit(&[("a/x", "2"), ("b/x", "2"), ("c/x", "2")]);
        let first = rig.head();
        rig.commit(&[("a/x", "3"), ("b/x", "3")]);
        rig.fog();
        let commits: HashSet<String> = [first.clone()].into();
        clear_commits(&rig.file(), &rig.main, &commits, &rig.set(&["a", "c"])).unwrap();
        let raw = rig.raw();
        assert_eq!(raw.parts["a"].len(), 1, "only that commit left a");
        assert_ne!(raw.parts["a"][0].commit, first);
        assert!(!raw.parts.contains_key("c"), "c had only that commit");
        assert_eq!(raw.parts["b"].len(), 2, "b was not in the set");
    }

    #[test]
    fn a_part_lit_by_two_chapters_clears_only_after_both_are_read() {
        let rig = Rig::new();
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", "2"), ("b/x", "2")]);
        let x = rig.head();
        rig.squash();
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
        );
        let lit: &[&[&str]] = &[&["a", "b"], &["a"]];
        let done = rig.read(0, &x, lit, 1);
        assert_eq!(done, rig.set(&["b"]), "a still waits for chapter 1");
        clear_pr(Path::new(&rig.main), &rig.file(), &rig.main, 7, &done).unwrap();
        assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
        let done = rig.read(1, &x, lit, 2);
        assert_eq!(done, rig.set(&["a", "b"]));
        clear_pr(Path::new(&rig.main), &rig.file(), &rig.main, 7, &done).unwrap();
        assert!(rig.fog().is_empty());
        assert!(fully_read(&read_user(&rig.file(), &rig.main), 7));
    }

    #[test]
    fn a_part_no_chapter_lights_keeps_its_fog() {
        let rig = Rig::new();
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", "2"), ("c/x", "2")]);
        let x = rig.head();
        rig.squash();
        let done = rig.read(0, &x, &[&["a"]], 1);
        clear_pr(Path::new(&rig.main), &rig.file(), &rig.main, 7, &done).unwrap();
        assert_eq!(rig.fog(), strengths(&[("c", Strength::Thin)]));
    }

    #[test]
    fn reading_before_the_merge_with_no_later_push_files_nothing() {
        let rig = Rig::new();
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", "2"), ("b/x", "2")]);
        let x = rig.head();
        rig.read(0, &x, &[&["a", "b"]], 1);
        assert!(rig.fog().is_empty());
        rig.squash();
        assert!(rig.fog().is_empty(), "{:?}", rig.fog());
    }

    #[test]
    fn a_push_after_the_read_fogs_only_the_part_it_touched_thin_or_thick_by_its_diff() {
        let rig = Rig::new();
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", "2"), ("b/x", "2"), ("c/x", "2")]);
        let x = rig.head();
        rig.read(0, &x, &[&["a", "b", "c"]], 1);
        // One push edits b's files; another changes the law on a.
        rig.commit(&[("b/x", "3")]);
        rig.commit(&[("system-map.json", LAW_CHANGED)]);
        rig.squash();
        assert_eq!(
            rig.fog(),
            strengths(&[("a", Strength::Thick), ("b", Strength::Thin)])
        );
    }

    #[test]
    fn main_merged_into_the_branch_after_the_read_fogs_nothing() {
        let rig = Rig::new();
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", "2")]);
        let x = rig.head();
        rig.read(0, &x, &[&["a", "b", "c"]], 1);
        // Main moves on (its own commit fogs c), and the branch merges it in.
        run(rig.dir.path(), &["checkout", "-q", "main"]);
        rig.commit(&[("c/x", "9")]);
        run(rig.dir.path(), &["checkout", "-q", "pr"]);
        run(rig.dir.path(), &["merge", "-q", "-m", "sync", "main"]);
        rig.squash();
        assert_eq!(rig.fog(), strengths(&[("c", Strength::Thin)]));
        assert_eq!(rig.raw().parts["c"].len(), 1, "only main's own commit");
    }

    #[test]
    fn a_brief_whose_head_is_unknown_to_git_leaves_the_fog_as_filed() {
        let rig = Rig::new();
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", "2")]);
        rig.read(0, &"0".repeat(40), &[&["a"]], 1);
        rig.squash();
        assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
    }

    #[test]
    fn carrying_a_read_over_moves_every_chapter_to_the_new_head_and_keeps_what_they_light() {
        let rig = Rig::new();
        let lit: &[&[&str]] = &[&["a", "b"], &["c"]];
        rig.read(0, "h1", lit, 1);
        rig.read(1, "h1", lit, 2);
        carry_read(&rig.file(), &rig.main, 7, "h1", "h2").unwrap();
        let user = read_user(&rig.file(), &rig.main);
        let read = &user.fog.reads[&7];
        assert!(read.fully_read_at("h2"), "{read:?}");
        assert_eq!(read.lit, vec![vec!["a", "b"], vec!["c"]]);
        assert_eq!(read.chapters[&1].at, 2, "the time of the read is kept");
        assert!(!fully_read_at(&user, 7, "h1"));
    }

    #[test]
    fn fog_at_merge_after_a_rebase_carried_over_runs_from_the_new_head() {
        let rig = Rig::new();
        let lines = |first: &str, last: &str| format!("{first}\n2\n3\n4\n5\n6\n7\n8\n9\n{last}\n");
        rig.commit(&[("a/x", &lines("1", "10"))]);
        rig.open();
        rig.branch();
        rig.commit(&[("a/x", &lines("pr", "10"))]);
        let h1 = rig.head();
        rig.read(0, &h1, &[&["a"]], 1);
        // Main edits the other end of the PR's file, and the branch is rebased onto it: the same patch.
        run(rig.dir.path(), &["checkout", "-q", "main"]);
        rig.commit(&[("a/x", &lines("1", "main"))]);
        rig.fog();
        rig.clear_all();
        run(rig.dir.path(), &["checkout", "-q", "pr"]);
        run(rig.dir.path(), &["rebase", "-q", "main"]);
        let h2 = rig.head();
        carry_read(&rig.file(), &rig.main, 7, &h1, &h2).unwrap();
        rig.squash();
        // From H1, main's own edit of a/x would fog a again; from H2 nothing changed since the read.
        assert!(rig.fog().is_empty(), "{:?}", rig.fog());
    }
}
