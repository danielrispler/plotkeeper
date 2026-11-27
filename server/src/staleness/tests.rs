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
        self.bands_born(&BTreeMap::new(), at)
    }
    fn bands_born(&self, born: &BTreeMap<String, u64>, at: u64) -> BTreeMap<String, u8> {
        bands(&read_user(&self.file(), "/p"), MAP, born, at)
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
        bands(
            &UserState::default(),
            MAP,
            &BTreeMap::new(),
            DAY_ONE + 50 * WEEK
        ),
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
fn a_brief_chapter_that_lights_a_part_clears_its_band_and_one_read_before_the_baseline_does_not() {
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

fn born(pairs: &[(&str, u64)]) -> BTreeMap<String, u64> {
    pairs.iter().map(|(id, at)| (id.to_string(), *at)).collect()
}

#[test]
fn a_part_added_to_the_map_after_the_baseline_starts_fresh_from_its_first_commit() {
    let rig = Rig::new();
    let added = DAY_ONE + 20 * WEEK;
    let births = born(&[("a", added)]);
    assert_eq!(rig.bands_born(&births, added + 2 * WEEK), of(&[("b", 3)]));
    assert_eq!(
        rig.bands_born(&births, added + 3 * WEEK),
        of(&[("a", 1), ("b", 3)])
    );
}

#[test]
fn a_part_in_the_map_since_before_the_baseline_keeps_the_baseline() {
    let rig = Rig::new();
    let births = born(&[("a", DAY_ONE - 40 * WEEK), ("b", DAY_ONE - WEEK)]);
    assert_eq!(rig.bands_born(&births, DAY_ONE + 3 * WEEK - 1), of(&[]));
    assert_eq!(
        rig.bands_born(&births, DAY_ONE + 6 * WEEK),
        of(&[("a", 2), ("b", 2)])
    );
}

#[test]
fn a_new_parts_later_activity_still_wins_over_its_first_commit() {
    let rig = Rig::new();
    let added = DAY_ONE + 20 * WEEK;
    let births = born(&[("a", added)]);
    record(&rig.file(), "/p", "a", Activity::Know, added + 4 * WEEK).unwrap();
    assert_eq!(rig.bands_born(&births, added + 6 * WEEK), of(&[("b", 3)]));
    assert_eq!(
        rig.bands_born(&births, added + 7 * WEEK),
        of(&[("a", 1), ("b", 3)])
    );
}

#[test]
fn git_is_read_once_per_project_and_part() {
    let cache = Births::default();
    let calls = std::cell::RefCell::new(Vec::new());
    let ids = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    let ask = |project: &str, list: &[&str]| {
        births_in(&cache, project, &ids(list), |wanted| {
            calls
                .borrow_mut()
                .push((project.to_string(), wanted.to_vec()));
            Some(
                born(&[("a", 5), ("c", 7)])
                    .into_iter()
                    .filter(|(id, _)| wanted.contains(id))
                    .collect(),
            )
        })
    };
    assert_eq!(ask("/p", &["a", "b"]), born(&[("a", 5)]));
    // b was asked about and never found; it is not read again.
    assert_eq!(ask("/p", &["a", "b"]), born(&[("a", 5)]));
    assert_eq!(ask("/p", &["a", "b", "c"]), born(&[("a", 5), ("c", 7)]));
    assert_eq!(ask("/q", &["a"]), born(&[("a", 5)]));
    assert_eq!(ask("/p", &["c", "a"]), born(&[("a", 5), ("c", 7)]));
    assert_eq!(
        calls.into_inner(),
        [
            ("/p".to_string(), ids(&["a", "b"])),
            ("/p".to_string(), ids(&["c"])),
            ("/q".to_string(), ids(&["a"])),
        ]
    );
}

#[test]
fn one_projects_read_of_git_does_not_hold_up_another_projects_known_births() {
    let cache = Births::default();
    let ids = vec!["a".to_string()];
    births_in(&cache, "/q", &ids, |_| Some(born(&[("a", 5)])));
    let (cache, ids) = (&cache, &ids);
    let (started, wait) = std::sync::mpsc::channel();
    let (release, held) = std::sync::mpsc::channel::<()>();
    let (answered, answer) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            births_in(cache, "/p", ids, |_| {
                started.send(()).unwrap();
                held.recv().ok();
                Some(born(&[("a", 9)]))
            })
        });
        wait.recv().unwrap();
        scope.spawn(move || {
            answered
                .send(births_in(cache, "/q", ids, |_| None))
                .unwrap();
        });
        let got = answer.recv_timeout(std::time::Duration::from_secs(2));
        release.send(()).unwrap();
        assert_eq!(got.ok(), Some(born(&[("a", 5)])));
    });
}

#[test]
fn a_failed_read_of_git_is_not_kept_and_is_read_again() {
    let cache = Births::default();
    let ids = vec!["a".to_string(), "b".to_string()];
    assert_eq!(births_in(&cache, "/p", &ids, |_| None), born(&[]));
    let mut asked = vec![];
    let found = births_in(&cache, "/p", &ids, |wanted| {
        asked = wanted.to_vec();
        Some(born(&[("a", 5)]))
    });
    assert_eq!(found, born(&[("a", 5)]));
    assert_eq!(asked, ids, "both ids are read again after the failure");
}

#[test]
fn a_poisoned_cache_still_answers() {
    let cache = Births::default();
    // The panic is the point: it poisons the lock.
    std::panic::catch_unwind(|| {
        let _held = cache.lock().unwrap();
        panic!("a reader panicked holding the lock");
    })
    .unwrap_err();
    let ids = vec!["a".to_string()];
    assert_eq!(
        births_in(&cache, "/p", &ids, |_| Some(born(&[("a", 5)]))),
        born(&[("a", 5)])
    );
}

fn lines_at(rig: &Rig, born: &BTreeMap<String, u64>, at: u64) -> BTreeMap<String, String> {
    lines(&read_user(&rig.file(), "/p"), MAP, born, at)
}

#[test]
fn a_stale_parts_line_names_its_band_and_its_last_engagement_or_that_it_was_never_engaged() {
    let rig = Rig::new();
    let read = DAY_ONE + 7 * WEEK;
    record(&rig.file(), "/p", "a", Activity::Refresher, read).unwrap();
    let born = born(&[("b", DAY_ONE + 20 * WEEK)]);
    let found = lines_at(&rig, &born, DAY_ONE + 33 * WEEK);
    assert_eq!(
        found,
        BTreeMap::from([
            (
                "a".to_string(),
                "12 weeks or more since you last engaged with this part, on 2027-03-05".to_string()
            ),
            (
                "b".to_string(),
                "12 weeks or more: not engaged since tracking began on 2027-06-04".to_string()
            ),
        ])
    );
    let found = lines_at(&rig, &BTreeMap::new(), DAY_ONE + 3 * WEEK);
    assert_eq!(
        found["b"],
        "3 weeks or more: not engaged since tracking began on 2027-01-15"
    );
    assert!(!found.contains_key("a"), "a was engaged after day one");
    assert!(!found.contains_key("i"), "only Core parts are stale");
}

#[test]
fn a_kind_from_a_newer_build_still_reads_as_an_activity() {
    let touch: Touch = serde_json::from_str(r#"{"at":5,"kind":"explain-back"}"#).unwrap();
    assert_eq!(touch.kind, Activity::Other);
}
