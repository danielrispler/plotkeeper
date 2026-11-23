//! Part catch-ups (spec danielrispler/plotkeeper#378, ticket #391): a fogged part offers one story of everything that landed in it
//! since Daniel last caught up, written when he clicks it. A part fogged by exactly one PR whose brief lights it opens that chapter
//! instead.

use crate::App;
use crate::fog::Entry;
use crate::map::UserState;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
};
use studio_understanding::part_catch_up::{
    CatchUpStore, StoredCatchUp, gather_catch_up, write_part_catch_up,
};

/// What a fogged part's sheet offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Offer {
    /// The one PR's brief has a chapter that lights the part: that chapter opens.
    Chapter { pr: u64, chapter: usize },
    /// A part catch-up: `ready` when one is stored for the part's current fog list, `writing` while one is written, `failed` when
    /// the last write for this fog list failed.
    CatchUp {
        ready: bool,
        writing: bool,
        failed: bool,
    },
}

/// What `part`'s fog (`fog`, its entries) offers. `lit_of(pr)` is what each chapter of PR `pr`'s stored brief lights, `None` with
/// no brief stored.
pub fn offer(
    repo: &Path,
    fog: &[Entry],
    part: &str,
    lit_of: &dyn Fn(u64) -> Option<Vec<Vec<String>>>,
) -> Offer {
    let chapter = match fog {
        [one] => crate::fog::subject_pr(repo, &one.commit).and_then(|pr| {
            let lit = lit_of(pr)?;
            let chapter = lit
                .iter()
                .position(|parts| parts.iter().any(|p| p == part))?;
            Some(Offer::Chapter { pr, chapter })
        }),
        _ => None,
    };
    chapter.unwrap_or(Offer::CatchUp {
        ready: false,
        writing: false,
        failed: false,
    })
}

/// Reading `part`'s catch-up to its end: the fog entries it covered (`covered`, its fog list) leave the part. No PR brief is marked
/// read, so each stays to be entered on its own.
pub fn complete(
    file: &Path,
    project: &str,
    part: &str,
    covered: &[String],
) -> Result<crate::map::UserState, String> {
    let commits = covered.iter().cloned().collect();
    crate::fog::clear_commits(file, project, &commits, &[part.to_string()].into())
}

/// The catch-up stored for `part` when it covers exactly `fog`, its current fog list; a different list makes it stale.
pub fn fresh(store: &CatchUpStore, part: &str, fog: &[String]) -> Option<StoredCatchUp> {
    store.get(part).filter(|stored| stored.fog == fog)
}

/// What is being written and what failed, per project and part. A write's claim ends when its guard drops, a panic included.
#[derive(Default)]
pub struct CatchUps {
    busy: Arc<Mutex<HashSet<(String, String)>>>,
    /// The fog list whose write failed, per project and part: the sheet says so, and a click tries again.
    failed: Mutex<HashMap<(String, String), Vec<String>>>,
}

struct Claim {
    busy: Arc<Mutex<HashSet<(String, String)>>>,
    key: (String, String),
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.busy.lock().unwrap().remove(&self.key);
    }
}

impl CatchUps {
    fn claim(&self, project: &str, part: &str) -> Option<Claim> {
        let key = (project.to_string(), part.to_string());
        self.busy
            .lock()
            .unwrap()
            .insert(key.clone())
            .then(|| Claim {
                busy: self.busy.clone(),
                key,
            })
    }

    fn writing(&self, project: &str, part: &str) -> bool {
        self.busy
            .lock()
            .unwrap()
            .contains(&(project.to_string(), part.to_string()))
    }

    fn failed(&self, project: &str, part: &str, fog: &[String]) -> bool {
        self.failed
            .lock()
            .unwrap()
            .get(&(project.to_string(), part.to_string()))
            .is_some_and(|f| f == fog)
    }
}

/// A project's catch-ups live beside its PR briefs, in a folder of their own.
pub fn store_of(project: &str) -> Option<CatchUpStore> {
    let home = std::env::home_dir()?;
    let name: String = project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    CatchUpStore::open(
        &studio_hook::data_dir(&home)
            .join("pr-briefs-by-project")
            .join(name),
    )
    .ok()
}

fn fog_of(user: &UserState, part: &str) -> Vec<Entry> {
    user.fog.parts.get(part).cloned().unwrap_or_default()
}

fn commits(fog: &[Entry]) -> Vec<String> {
    fog.iter().map(|e| e.commit.clone()).collect()
}

/// What each part the map shows fogged (`shown`, `fog.rs`'s effective fog) offers on its sheet.
pub fn offers(
    app: &App,
    project: &str,
    user: &UserState,
    shown: &BTreeMap<String, crate::fog::Strength>,
) -> BTreeMap<String, Offer> {
    let repo = Path::new(project);
    let briefs = crate::pr_briefs::store_of(project);
    let catch_ups = store_of(project);
    let lit_of = |pr: u64| {
        let brief = briefs.as_ref()?.latest(pr)?;
        Some(crate::pr_briefs::lit_parts(&brief.brief))
    };
    shown
        .keys()
        .map(|part| {
            let fog = fog_of(user, part);
            let offered = match offer(repo, &fog, part, &lit_of) {
                Offer::CatchUp { .. } => {
                    let list = commits(&fog);
                    Offer::CatchUp {
                        ready: catch_ups
                            .as_ref()
                            .is_some_and(|s| fresh(s, part, &list).is_some()),
                        writing: app.catch_ups.writing(project, part),
                        failed: app.catch_ups.failed(project, part, &list),
                    }
                }
                chapter => chapter,
            };
            (part.clone(), offered)
        })
        .collect()
}

/// What a click on a part's catch-up answers: the catch-up, or that it is being written (it comes through `map-landed`).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Opened {
    CatchUp(Box<StoredCatchUp>),
    Writing { writing: bool },
}

/// A click on `part`'s catch-up: the stored one when it covers the part's current fog list, else it is written in the background
/// with the PR brief's writer and model, and the answer says so; the page gets it through `map-landed`. `None`: the part is clear.
pub fn part_catch_up(
    app: &Arc<App>,
    project: String,
    part: String,
) -> Result<Option<Opened>, String> {
    let file = crate::map::state_file()?;
    let fog = commits(&fog_of(&crate::map::read_user(&file, &project), &part));
    if fog.is_empty() {
        return Ok(None);
    }
    let store = store_of(&project).ok_or("no data folder")?;
    if let Some(stored) = fresh(&store, &part, &fog) {
        // Opening it is an entered activity on the part: its staleness band clears (#392).
        crate::staleness::record(
            &file,
            &project,
            &part,
            crate::staleness::Activity::CatchUp,
            crate::ratify::now(),
        )?;
        return Ok(Some(Opened::CatchUp(Box::new(stored))));
    }
    let writer = app
        .briefs
        .writer()
        .ok_or("Part catch-ups are written by Claude, which plotkeeper did not find.")?;
    if let Some(claim) = app.catch_ups.claim(&project, &part) {
        app.catch_ups
            .failed
            .lock()
            .unwrap()
            .remove(&(project.clone(), part.clone()));
        let app = app.clone();
        let project = project.clone();
        std::thread::spawn(move || {
            let model = app.briefs.model();
            let written = gather_catch_up(Path::new(&project), &part, &fog).and_then(|source| {
                write_part_catch_up(writer.as_ref(), &model, &source, &store, |line| {
                    log::warn!("part catch-up of {part}: {line}")
                })
            });
            if let Err(why) = written {
                log::warn!("write the part catch-up of {part}: {why}");
                app.catch_ups
                    .failed
                    .lock()
                    .unwrap()
                    .insert((project.clone(), part.clone()), fog);
            }
            drop(claim);
            app.events.emit("map-landed", serde_json::json!(project));
        });
    }
    // The sheet says it is being written.
    app.events
        .emit("map-landed", serde_json::json!(project.clone()));
    Ok(Some(Opened::Writing { writing: true }))
}

/// `part`'s catch-up over `fog` was read to its end (its last step seen, its check answered when it holds one): those entries leave
/// the part, and no PR brief is marked read.
pub fn part_catch_up_done(
    app: &App,
    project: String,
    part: String,
    fog: Vec<String>,
) -> Result<(), String> {
    complete(&crate::map::state_file()?, &project, &part, &fog)?;
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fog::{Strength, effective, refresh};
    use crate::map::read_user;
    use studio_test_support::{git as run, repo};

    const MAP: &str = r#"{"version":1,
      "parts":[
        {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}],
      "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":[],"laws":[]}"#;

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
            rig.commit(
                "Start the map",
                &[("system-map.json", MAP), ("a/x", "1"), ("b/x", "1")],
            );
            refresh(Path::new(&rig.main), &rig.file(), &rig.main, true).unwrap();
            rig
        }
        fn file(&self) -> std::path::PathBuf {
            self.state.path().join("map-state.json")
        }
        fn commit(&self, subject: &str, files: &[(&str, &str)]) {
            for (name, text) in files {
                let path = self.dir.path().join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            run(self.dir.path(), &["add", "-A"]);
            run(self.dir.path(), &["commit", "-q", "-m", subject]);
        }
        /// The part's fog entries after a walk.
        fn fog(&self, part: &str) -> Vec<Entry> {
            refresh(Path::new(&self.main), &self.file(), &self.main, false).unwrap();
            read_user(&self.file(), &self.main)
                .fog
                .parts
                .get(part)
                .cloned()
                .unwrap_or_default()
        }
        fn offer(&self, part: &str, lit_of: &dyn Fn(u64) -> Option<Vec<Vec<String>>>) -> Offer {
            offer(Path::new(&self.main), &self.fog(part), part, lit_of)
        }
    }

    const CATCH_UP: Offer = Offer::CatchUp {
        ready: false,
        writing: false,
        failed: false,
    };
    fn lit(chapters: &[&[&str]]) -> Option<Vec<Vec<String>>> {
        Some(
            chapters
                .iter()
                .map(|c| c.iter().map(|p| p.to_string()).collect())
                .collect(),
        )
    }

    #[test]
    fn a_part_with_two_fog_entries_gets_a_catch_up_even_when_each_pr_has_a_brief() {
        let rig = Rig::new();
        rig.commit("First (#7)", &[("a/x", "2")]);
        rig.commit("Second (#8)", &[("a/x", "3")]);
        assert_eq!(rig.offer("a", &|_| lit(&[&["a"]])), CATCH_UP);
    }

    #[test]
    fn a_direct_push_gets_a_catch_up() {
        let rig = Rig::new();
        rig.commit("fix pushed to main", &[("a/x", "2")]);
        assert_eq!(rig.offer("a", &|_| lit(&[&["a"]])), CATCH_UP);
    }

    #[test]
    fn a_pr_whose_brief_is_missing_or_lights_no_chapter_on_the_part_gets_a_catch_up() {
        let rig = Rig::new();
        rig.commit("Thing (#7)", &[("a/x", "2"), ("b/x", "2")]);
        assert_eq!(rig.offer("a", &|_| None), CATCH_UP);
        assert_eq!(rig.offer("a", &|_| lit(&[&["b"]])), CATCH_UP);
    }

    #[test]
    fn a_part_fogged_by_one_pr_whose_brief_lights_it_opens_that_chapter() {
        let rig = Rig::new();
        rig.commit("Thing (#7)", &[("a/x", "2"), ("b/x", "2")]);
        let asked = std::cell::Cell::new(0);
        let lit_of = |pr: u64| {
            asked.set(pr);
            lit(&[&["b"], &["a", "b"]])
        };
        assert_eq!(
            rig.offer("a", &lit_of),
            Offer::Chapter { pr: 7, chapter: 1 }
        );
        assert_eq!(asked.get(), 7);
        assert_eq!(
            rig.offer("b", &lit_of),
            Offer::Chapter { pr: 7, chapter: 0 }
        );
        // The map shows both parts fogged meanwhile.
        let user = read_user(&rig.file(), &rig.main);
        assert_eq!(effective(&user, MAP).get("a"), Some(&Strength::Thin));
    }

    #[test]
    fn completing_a_catch_up_clears_the_parts_fog_and_completes_no_brief_and_the_header_dot_goes_out()
     {
        let rig = Rig::new();
        rig.commit("Thing (#7)", &[("a/x", "2"), ("b/x", "2")]);
        rig.commit("fix pushed to main", &[("a/x", "3")]);
        let covered: Vec<String> = rig.fog("a").into_iter().map(|e| e.commit).collect();
        assert_eq!(covered.len(), 2);
        let user = read_user(&rig.file(), &rig.main);
        assert!(crate::fog::core_fogged(&user, MAP));

        complete(&rig.file(), &rig.main, "a", &covered).unwrap();
        assert!(rig.fog("a").is_empty());
        let user = read_user(&rig.file(), &rig.main);
        assert!(
            !crate::fog::fully_read(&user, 7),
            "the PR's brief is not completed"
        );
        assert!(user.fog.reads.is_empty());
        assert!(crate::fog::core_fogged(&user, MAP), "b is still fogged");

        let b: Vec<String> = rig.fog("b").into_iter().map(|e| e.commit).collect();
        complete(&rig.file(), &rig.main, "b", &b).unwrap();
        let user = read_user(&rig.file(), &rig.main);
        assert!(
            !crate::fog::core_fogged(&user, MAP),
            "no Core part stays fogged"
        );
    }

    #[test]
    fn an_entry_that_landed_after_the_catch_up_was_written_keeps_its_fog() {
        let rig = Rig::new();
        rig.commit("fix pushed to main", &[("a/x", "2")]);
        let covered: Vec<String> = rig.fog("a").into_iter().map(|e| e.commit).collect();
        rig.commit("another push", &[("a/x", "3")]);
        assert_eq!(rig.fog("a").len(), 2);
        complete(&rig.file(), &rig.main, "a", &covered).unwrap();
        assert_eq!(rig.fog("a").len(), 1);
    }

    #[test]
    fn a_stored_catch_up_is_read_as_it_is_only_for_the_fog_list_it_covers() {
        let data = tempfile::tempdir().unwrap();
        let store = CatchUpStore::open(data.path()).unwrap();
        let fog = |c: &[&str]| c.iter().map(|c| c.to_string()).collect::<Vec<_>>();
        assert!(fresh(&store, "a", &fog(&["c1"])).is_none());
        let stored = StoredCatchUp {
            part: "a".into(),
            fog: fog(&["c1", "c2"]),
            ..StoredCatchUp::default()
        };
        store.put(&stored).unwrap();
        assert_eq!(fresh(&store, "a", &fog(&["c1", "c2"])), Some(stored));
        assert!(
            fresh(&store, "a", &fog(&["c1", "c2", "c3"])).is_none(),
            "a new entry"
        );
        assert!(
            fresh(&store, "a", &fog(&["c2"])).is_none(),
            "part of it cleared"
        );
        assert!(fresh(&store, "b", &fog(&["c1", "c2"])).is_none());
    }
}
