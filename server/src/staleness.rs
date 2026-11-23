//! Staleness bands (spec danielrispler/plotkeeper#378, ticket #392): how long since Daniel last engaged with each Core part. Not since
//! the last commit: a quiet part he read last week is not stale. An entered activity on a part (a brief chapter that lights it, a part
//! catch-up, the refresher, "I know this") resets it; bands start at 3, 6 and 12 weeks.
//!
//! The baseline for every part is the project's first map with this build (`start`), so nothing is stale on day one. A brief chapter is
//! read from what fog already keeps (`fog.reads`, with when each chapter was read), so a chapter read before this shipped counts too, and
//! is kept here as well when it is read (`fog::mark_read`), since fog drops a brief's reads when it is rewritten; the other kinds are kept
//! here, in `map-state.json`, by `record`. Another module records its activity with `record` and a new `Activity`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

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

/// The band (1, 2 or 3) of each stale part of `source` whose effective tier is Core, at `now`; a part not listed is not stale.
pub fn bands(user: &UserState, source: &str, now: u64) -> BTreeMap<String, u8> {
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
        let read = chapter_read(user, id);
        let touched = user.staleness.parts.get(id).map(|t| t.at);
        let last = [Some(since), read, touched]
            .into_iter()
            .flatten()
            .max()
            .unwrap_or(since);
        let weeks = now.saturating_sub(last) / WEEK;
        let band = match weeks {
            12.. => 3,
            6.. => 2,
            3.. => 1,
            _ => continue,
        };
        stale.insert(id.to_string(), band);
    }
    stale
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
mod tests {
    use super::*;
    use crate::map::read_user;

    const MAP: &str = r#"{"version":1,
      "parts":[
        {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
        {"id":"i","name":"I","caption":"c","description":"d","tier":"interface","paths":["i/"]},
        {"id":"x","name":"X","caption":"c","description":"d","tier":"black-box","paths":["x/"]}],
      "actors":[],"links":[],"ignored":[],"laws":[]}"#;

    /// The day staleness shipped, in unix seconds.
    const DAY_ONE: u64 = 1_800_000_000;
    const DAY: u64 = 24 * 60 * 60;

    struct Rig {
        state: tempfile::TempDir,
    }
    impl Rig {
        /// A project first shown with this build on day one.
        fn new() -> Rig {
            let rig = Rig {
                state: tempfile::tempdir().unwrap(),
            };
            start(&rig.file(), "/p", DAY_ONE).unwrap();
            rig
        }
        fn file(&self) -> std::path::PathBuf {
            self.state.path().join("map-state.json")
        }
        fn bands(&self, at: u64) -> BTreeMap<String, u8> {
            bands(&read_user(&self.file(), "/p"), MAP, at)
        }
    }

    fn of(pairs: &[(&str, u8)]) -> BTreeMap<String, u8> {
        pairs.iter().map(|(id, b)| (id.to_string(), *b)).collect()
    }

    #[test]
    fn a_core_part_shows_band_1_after_3_weeks_2_after_6_and_3_after_12_and_other_tiers_never() {
        let rig = Rig::new();
        assert_eq!(rig.bands(DAY_ONE + 3 * WEEK - 1), of(&[]));
        assert_eq!(rig.bands(DAY_ONE + 3 * WEEK), of(&[("a", 1), ("b", 1)]));
        assert_eq!(rig.bands(DAY_ONE + 6 * WEEK), of(&[("a", 2), ("b", 2)]));
        assert_eq!(rig.bands(DAY_ONE + 12 * WEEK), of(&[("a", 3), ("b", 3)]));
        assert_eq!(rig.bands(DAY_ONE + 50 * WEEK), of(&[("a", 3), ("b", 3)]));
    }

    #[test]
    fn daniels_tier_decides_which_parts_band() {
        let rig = Rig::new();
        crate::map::update_user(&rig.file(), "/p", |u| {
            u.tiers.insert("a".into(), "interface".into());
            u.tiers.insert("i".into(), "core".into());
        })
        .unwrap();
        assert_eq!(rig.bands(DAY_ONE + 6 * WEEK), of(&[("b", 2), ("i", 2)]));
    }

    #[test]
    fn on_the_first_run_every_baseline_is_today_and_a_later_run_keeps_it() {
        let rig = Rig::new();
        assert_eq!(rig.bands(DAY_ONE), of(&[]));
        // A later map does not move the baseline.
        start(&rig.file(), "/p", DAY_ONE + 5 * WEEK).unwrap();
        assert_eq!(rig.bands(DAY_ONE + 6 * WEEK), of(&[("a", 2), ("b", 2)]));
        // A project never shown with this build has no baseline yet, so nothing is stale.
        assert_eq!(
            bands(&UserState::default(), MAP, DAY_ONE + 50 * WEEK),
            of(&[])
        );
    }

    #[test]
    fn each_kind_of_entered_activity_clears_the_band() {
        for kind in [Activity::Know, Activity::Refresher, Activity::CatchUp] {
            let rig = Rig::new();
            let later = DAY_ONE + 13 * WEEK;
            record(&rig.file(), "/p", "a", kind, later).unwrap();
            assert_eq!(rig.bands(later), of(&[("b", 3)]), "{kind:?}");
            assert_eq!(
                rig.bands(later + 3 * WEEK),
                of(&[("a", 1), ("b", 3)]),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_brief_chapter_that_lights_a_part_clears_its_band_and_one_read_before_the_baseline_does_not()
     {
        let rig = Rig::new();
        let later = DAY_ONE + 7 * WEEK;
        let lit = vec![vec!["a".to_string()], vec!["b".to_string()]];
        crate::fog::mark_read(&rig.file(), "/p", 7, 0, "h", lit.clone(), later).unwrap();
        assert_eq!(rig.bands(later + DAY), of(&[("b", 2)]));
        // A chapter read long before the baseline leaves the baseline in force.
        crate::fog::mark_read(&rig.file(), "/p", 8, 1, "h", lit, DAY_ONE - 20 * WEEK).unwrap();
        assert_eq!(rig.bands(later + DAY), of(&[("b", 2)]));
    }

    #[test]
    fn a_chapter_read_keeps_its_time_after_the_brief_is_rewritten_to_light_its_parts_differently() {
        let rig = Rig::new();
        let read = DAY_ONE + 7 * WEEK;
        let first = vec![vec!["a".to_string()], vec!["b".to_string()]];
        crate::fog::mark_read(&rig.file(), "/p", 7, 0, "h1", first, read).unwrap();
        // The brief is rewritten; its first chapter now lights b, and is read a day later.
        let rewritten = vec![vec!["b".to_string()], vec!["a".to_string()]];
        crate::fog::mark_read(&rig.file(), "/p", 7, 0, "h2", rewritten, read + DAY).unwrap();
        assert_eq!(rig.bands(read + 2 * WEEK), of(&[]));
        assert_eq!(rig.bands(read + 3 * WEEK), of(&[("a", 1)]));
    }

    #[test]
    fn an_older_activity_never_undoes_a_newer_one() {
        let rig = Rig::new();
        record(
            &rig.file(),
            "/p",
            "a",
            Activity::Refresher,
            DAY_ONE + 10 * WEEK,
        )
        .unwrap();
        record(&rig.file(), "/p", "a", Activity::Know, DAY_ONE + 2 * WEEK).unwrap();
        assert_eq!(rig.bands(DAY_ONE + 12 * WEEK), of(&[("b", 3)]));
    }

    #[test]
    fn a_kind_from_a_newer_build_still_reads_as_an_activity() {
        let touch: Touch = serde_json::from_str(r#"{"at":5,"kind":"explain-back"}"#).unwrap();
        assert_eq!(touch.kind, Activity::Other);
    }
}
