//! Staleness bands (spec danielrispler/plotkeeper#378, ticket #392): how long since Daniel last engaged with each Core part. Not since
//! the last commit: a quiet part he read last week is not stale. An entered activity on a part (a brief chapter that lights it, a part
//! catch-up, the refresher, "I know this") resets it; bands start at 3, 6 and 12 weeks.
//!
//! A part's baseline is the project's first map with this build (`start`), so nothing is stale on day one, or, when later, the commit
//! time at which its id first entered the map on the default branch (`births`), so a part added to an old project starts fresh too;
//! that time is read from git, never stored. A brief chapter is
//! read from what fog already keeps (`fog.reads`, with when each chapter was read), so a chapter read before this shipped counts too, and
//! is kept here as well when it is read (`fog::mark_read`), since fog drops a brief's reads when it is rewritten; the other kinds are kept
//! here, in `map-state.json`, by `record`. Another module records its activity with `record` and a new `Activity`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Arc, Mutex},
};

use crate::map::{UserState, update_user};

/// A week, in seconds.
pub const WEEK: u64 = 7 * 24 * 60 * 60;

/// An entered activity on a part, other than a brief chapter (which fog keeps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Activity {
    /// "I know this" cleared the part's fog.
    Know,
    /// The part's refresher was entered.
    Refresher,
    /// A part catch-up on the part was entered (#391).
    CatchUp,
    /// A brief chapter that lights the part was read to its end. Fog keeps the read too, but drops it when the brief is rewritten to
    /// light its parts differently, so its time is kept here as well.
    Chapter,
    /// A kind a newer build wrote: still an activity.
    #[serde(other)]
    Other,
}

/// The last activity on a part: when (unix seconds) and what.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Touch {
    pub at: u64,
    pub kind: Activity,
}

/// A project's staleness, in `map-state.json`: the baseline (unix seconds; none before this build first showed the map) and each part's
/// last recorded activity.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Staleness {
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub parts: BTreeMap<String, Touch>,
}

/// Sets `project`'s baseline to `now` unless it has one: the first map with this build.
pub fn start(file: &Path, project: &str, now: u64) -> Result<UserState, String> {
    update_user(file, project, |u| {
        u.staleness.since.get_or_insert(now);
    })
}

/// Records `kind` on `part` at `at`: its band clears.
pub fn record(
    file: &Path,
    project: &str,
    part: &str,
    kind: Activity,
    at: u64,
) -> Result<UserState, String> {
    update_user(file, project, |u| touch(u, part, kind, at))
}

/// `record`, inside another change to the user's state: an older activity never undoes a newer one.
pub fn touch(u: &mut UserState, part: &str, kind: Activity, at: u64) {
    let last = u.staleness.parts.get(part).map_or(0, |t| t.at);
    if at >= last {
        u.staleness
            .parts
            .insert(part.to_string(), Touch { at, kind });
    }
}

/// A stale part: its band, when tracking it began (the baseline, or its birth when later), and its last engagement after that, if any.
struct Stale {
    band: u8,
    began: u64,
    engaged: Option<u64>,
}

/// Each stale part of `source` whose effective tier is Core, at `now`.
fn stale(
    user: &UserState,
    source: &str,
    born: &BTreeMap<String, u64>,
    now: u64,
) -> BTreeMap<String, Stale> {
    let Some(since) = user.staleness.since else {
        return BTreeMap::new();
    };
    let Ok(map) = serde_json::from_str::<Value>(source) else {
        return BTreeMap::new();
    };
    let mut stale = BTreeMap::new();
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
        if tier != "core" {
            continue;
        }
        let began = born.get(id).copied().map_or(since, |at| at.max(since));
        let engaged = [
            chapter_read(user, id),
            user.staleness.parts.get(id).map(|t| t.at),
        ]
        .into_iter()
        .flatten()
        .max()
        .filter(|at| *at > began);
        let last = engaged.unwrap_or(began);
        let weeks = now.saturating_sub(last) / WEEK;
        let band = match weeks {
            12.. => 3,
            6.. => 2,
            3.. => 1,
            _ => continue,
        };
        stale.insert(
            id.to_string(),
            Stale {
                band,
                began,
                engaged,
            },
        );
    }
    stale
}

/// The band (1, 2 or 3) of each stale part of `source` whose effective tier is Core, at `now`; a part not listed is not stale.
pub fn bands(
    user: &UserState,
    source: &str,
    born: &BTreeMap<String, u64>,
    now: u64,
) -> BTreeMap<String, u8> {
    stale(user, source, born, now)
        .into_iter()
        .map(|(id, found)| (id, found.band))
        .collect()
}

/// What each stale part's sheet says (#495): its band, and the UTC date Daniel last engaged with it, or that he has not since tracking
/// began.
pub fn lines(
    user: &UserState,
    source: &str,
    born: &BTreeMap<String, u64>,
    now: u64,
) -> BTreeMap<String, String> {
    let date = |at: u64| studio_understanding::utc_date(at * 1000);
    stale(user, source, born, now)
        .into_iter()
        .map(|(id, found)| {
            let weeks = match found.band {
                3 => 12,
                2 => 6,
                _ => 3,
            };
            let line = match found.engaged {
                Some(at) => format!(
                    "{weeks} weeks or more since you last engaged with this part, on {}",
                    date(at)
                ),
                None => format!(
                    "{weeks} weeks or more: not engaged since tracking began on {}",
                    date(found.began)
                ),
            };
            (id, line)
        })
        .collect()
}

/// Each project's births by part id, behind a lock of its own, kept for the server's life on `App`.
pub(crate) type Births = Mutex<HashMap<String, Arc<Mutex<HashMap<String, Option<u64>>>>>>;

/// When each part of `source` first entered the map on `repo`'s default branch (`history::part_births`), the later baseline of a part
/// added after the project's. Git's history is read at most once per project and part while the server runs.
pub(crate) fn births(cache: &Births, repo: &Path, source: &str) -> BTreeMap<String, u64> {
    let ids: Vec<String> = serde_json::from_str::<Value>(source)
        .ok()
        .and_then(|map| {
            map["parts"].as_array().map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p["id"].as_str().map(String::from))
                    .collect()
            })
        })
        .unwrap_or_default();
    births_in(cache, &repo.display().to_string(), &ids, |wanted| {
        crate::history::part_births(repo, wanted)
    })
}

/// `births` over `cache`, with `load` reading git for the ids not yet asked about. A failed read (`None`) is not kept, so the next map
/// reads them again.
#[allow(
    clippy::significant_drop_tightening,
    reason = "the project's lock is held across the read on purpose (the comment below)"
)]
fn births_in(
    cache: &Births,
    project: &str,
    ids: &[String],
    load: impl FnOnce(&[String]) -> Option<BTreeMap<String, u64>>,
) -> BTreeMap<String, u64> {
    // The project's lock is held across the read, so two first maps of one project never both read git, while another
    // project's map goes on. A reader that panicked holding a lock left whole entries: the cache stays usable.
    let project_cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry(project.to_string())
        .or_default()
        .clone();
    let mut known = project_cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let missing: Vec<String> = ids
        .iter()
        .filter(|id| !known.contains_key(*id))
        .cloned()
        .collect();
    if !missing.is_empty()
        && let Some(found) = load(&missing)
    {
        for id in missing {
            let at = found.get(&id).copied();
            known.insert(id, at);
        }
    }
    ids.iter()
        .filter_map(|id| Some((id.clone(), (*known.get(id)?)?)))
        .collect()
}

/// When Daniel last read to its end a brief chapter that lights `part`, as fog keeps it.
fn chapter_read(user: &UserState, part: &str) -> Option<u64> {
    user.fog
        .reads
        .values()
        .flat_map(|read| {
            read.chapters.iter().filter_map(|(i, chapter)| {
                read.lit
                    .get(*i)
                    .is_some_and(|lit| lit.iter().any(|p| p == part))
                    .then_some(chapter.at)
            })
        })
        .max()
}

#[cfg(test)]
mod tests;
