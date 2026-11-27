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

/// The strength each live part of `new` gets from a commit that changed `changed`, against the map `old` of its parent, with why.
/// A side whose map is missing or does not read gives no fog: nothing can be told apart.
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
                        .map(ToString::to_string),
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
mod tests;
