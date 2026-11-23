//! Part catch-ups (spec danielrispler/plotkeeper#378, tickets #391 and #396): a fogged part offers one story of everything that
//! landed in it since Daniel last caught up. When a project's map is shown, the catch-ups of up to three fogged Core parts are
//! written, oldest fog first; any other is written when he clicks it. A part fogged by exactly one PR whose brief lights it opens
//! that chapter instead.

use crate::App;
use crate::fog::Entry;
use crate::map::UserState;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::{Arc, Mutex},
};
use studio_understanding::part_catch_up::{
    CatchUpStore, StoredCatchUp, gather_catch_up, write_part_catch_up,
};
use studio_understanding::pr_brief::BriefWriter;

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

/// What is being written and what failed, per project and part. A write's claim ends when its guard drops, a panic included, and
/// the page hears it either way.
#[derive(Default)]
pub struct CatchUps {
    busy: Arc<Mutex<Busy>>,
    /// The fog list whose write failed, per project and part: the sheet says so, and a click tries again.
    failed: Arc<Mutex<Failed>>,
}

/// The fog list whose write failed, by project and part.
type Failed = HashMap<(String, String), Vec<String>>;

/// Per project and part being written: the fog list being written, and the one asked for meanwhile, written once that one ends.
type Busy = HashMap<(String, String), (Vec<String>, Option<Vec<String>>)>;

struct Claim {
    busy: Arc<Mutex<Busy>>,
    failed: Arc<Mutex<Failed>>,
    key: (String, String),
    released: bool,
    /// Runs when the claim drops, so the page hears the writes ended even when one panicked.
    done: Option<Box<dyn FnOnce() + Send>>,
}

impl Claim {
    /// The fog list asked for while this one was written, now the one being written; else the claim ends.
    fn next(&mut self) -> Option<Vec<String>> {
        let mut busy = self.busy.lock().unwrap();
        let queued = busy.get_mut(&self.key).and_then(|(writing, queued)| {
            let next = queued.take()?;
            *writing = next.clone();
            Some(next)
        });
        if queued.is_none() {
            busy.remove(&self.key);
            self.released = true;
        }
        queued
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        // A write panicked: the newest list asked for is marked failed, so the sheet says so and a click tries it again.
        if !self.released
            && let Some((writing, queued)) = self.busy.lock().unwrap().remove(&self.key)
        {
            let list = queued.unwrap_or(writing);
            self.failed.lock().unwrap().insert(self.key.clone(), list);
        }
        if let Some(done) = self.done.take() {
            done();
        }
    }
}

impl CatchUps {
    /// Claims `part`'s write over `fog`; while another list is being written, `fog` waits its turn after it instead.
    fn claim(&self, project: &str, part: &str, fog: &[String]) -> Option<Claim> {
        let key = (project.to_string(), part.to_string());
        let mut busy = self.busy.lock().unwrap();
        if let Some((writing, queued)) = busy.get_mut(&key) {
            if writing != fog {
                *queued = Some(fog.to_vec());
            }
            return None;
        }
        busy.insert(key.clone(), (fog.to_vec(), None));
        Some(Claim {
            busy: self.busy.clone(),
            failed: self.failed.clone(),
            key,
            released: false,
            done: None,
        })
    }

    fn writing(&self, project: &str, part: &str) -> bool {
        self.busy
            .lock()
            .unwrap()
            .contains_key(&(project.to_string(), part.to_string()))
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
    CatchUpStore::open(&crate::pr_briefs::project_path(
        "pr-briefs-by-project",
        project,
    )?)
    .ok()
}

fn fog_of(user: &UserState, part: &str) -> Vec<Entry> {
    user.fog.parts.get(part).cloned().unwrap_or_default()
}

fn commits(fog: &[Entry]) -> Vec<String> {
    fog.iter().map(|e| e.commit.clone()).collect()
}

/// What each chapter of PR `pr`'s stored brief in `project` lights, `None` with no brief stored.
fn lit_of(project: &str) -> impl Fn(u64) -> Option<Vec<Vec<String>>> {
    let briefs = crate::pr_briefs::store_of(project);
    move |pr| {
        let brief = briefs.as_ref()?.latest(pr)?;
        Some(crate::pr_briefs::lit_parts(&brief.brief))
    }
}

/// What each part the map shows fogged (`shown`, `fog.rs`'s effective fog) offers on its sheet.
pub fn offers(
    app: &App,
    project: &str,
    user: &UserState,
    shown: &BTreeMap<String, crate::fog::Strength>,
) -> BTreeMap<String, Offer> {
    let repo = Path::new(project);
    let catch_ups = store_of(project);
    let lit_of = lit_of(project);
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

type SharedWriter = Arc<dyn BriefWriter + Send + Sync>;

/// At most this many fogged Core parts get their catch-up written when a project's map is shown (spec #378, "When it is
/// written").
pub const ON_SHOW: usize = 3;

/// Starts writing `part`'s catch-up over `fog` in the background. While one over another list is being written, `fog` is written
/// once that one ends, so the part is never left with neither a write nor a catch-up for its list. `done` runs when the writes end,
/// written or failed. Whether a write started.
#[allow(clippy::too_many_arguments)]
fn start(
    catch_ups: &CatchUps,
    writer: SharedWriter,
    model: String,
    project: &str,
    part: &str,
    fog: Vec<String>,
    store: CatchUpStore,
    done: impl FnOnce() + Send + 'static,
) -> bool {
    let Some(mut claim) = catch_ups.claim(project, part, &fog) else {
        return false;
    };
    claim.done = Some(Box::new(done));
    let failed = catch_ups.failed.clone();
    let (project, part) = (project.to_string(), part.to_string());
    std::thread::spawn(move || {
        let mut fog = fog;
        loop {
            let written = gather_catch_up(Path::new(&project), &part, &fog).and_then(|source| {
                write_part_catch_up(writer.as_ref(), &model, &source, &store, |line| {
                    log::warn!("part catch-up of {part}: {line}")
                })
            });
            if let Err(why) = written {
                log::warn!("write the part catch-up of {part}: {why}");
                failed
                    .lock()
                    .unwrap()
                    .insert((project.clone(), part.clone()), fog.clone());
            }
            match claim.next() {
                Some(next) => fog = next,
                None => break,
            }
        }
        // Dropping the claim runs `done`.
        drop(claim);
    });
    true
}

/// What a click on `part`'s catch-up over `fog` answers: the stored one when it covers `fog`, else a write starts (a failed one is
/// tried again) and the answer says it is being written.
#[allow(clippy::too_many_arguments)]
pub fn ask(
    catch_ups: &CatchUps,
    writer: Option<SharedWriter>,
    model: String,
    project: &str,
    part: &str,
    fog: Vec<String>,
    store: CatchUpStore,
    done: impl FnOnce() + Send + 'static,
) -> Result<Opened, String> {
    if let Some(stored) = fresh(&store, part, &fog) {
        return Ok(Opened::CatchUp(Box::new(stored)));
    }
    let writer =
        writer.ok_or("Part catch-ups are written by Claude, which plotkeeper did not find.")?;
    // A click tries a failed write again.
    catch_ups
        .failed
        .lock()
        .unwrap()
        .remove(&(project.to_string(), part.to_string()));
    start(catch_ups, writer, model, project, part, fog, store, done);
    Ok(Opened::Writing { writing: true })
}

/// A project's map was shown (page load, focus, project switch): the catch-ups of at most `ON_SHOW` fogged Core parts that get one
/// (not a part that opens its one PR's chapter) are written, oldest fog first. One already stored for its part's fog list, being
/// written, or failed for that list is not written again, and still takes its place among the `ON_SHOW`. Returns the parts whose
/// write started; `done` runs as each ends.
#[allow(clippy::too_many_arguments)]
pub fn show(
    catch_ups: &CatchUps,
    writer: &SharedWriter,
    model: &str,
    project: &str,
    user: &UserState,
    source: &str,
    lit_of: &dyn Fn(u64) -> Option<Vec<Vec<String>>>,
    store: &CatchUpStore,
    done: impl Fn() + Clone + Send + 'static,
) -> Vec<String> {
    let repo = Path::new(project);
    // How far down the history a commit sits: an older one has fewer commits behind it.
    let depth = |commit: &str| {
        crate::map::git(repo, &["rev-list", "--count", commit])
            .ok()
            .and_then(|out| out.trim().parse::<u64>().ok())
            .unwrap_or(u64::MAX)
    };
    let mut fogged: Vec<(u64, String, Vec<String>)> = crate::fog::core_fogged_parts(user, source)
        .into_iter()
        .filter_map(|part| {
            let fog = fog_of(user, &part);
            let first = fog.first()?.commit.clone();
            matches!(offer(repo, &fog, &part, lit_of), Offer::CatchUp { .. })
                .then(|| (depth(&first), part, commits(&fog)))
        })
        .collect();
    fogged.sort_by_key(|(depth, _, _)| *depth);
    fogged
        .into_iter()
        .take(ON_SHOW)
        .filter(|(_, part, fog)| {
            fresh(store, part, fog).is_none() && !catch_ups.failed(project, part, fog)
        })
        .filter_map(|(_, part, fog)| {
            start(
                catch_ups,
                writer.clone(),
                model.to_string(),
                project,
                &part,
                fog,
                store.clone(),
                done.clone(),
            )
            .then_some(part)
        })
        .collect()
}

/// `project`'s map was shown (`map_state` with `fetch`, after the fog's refresh): `show` with the PR brief's writer and model; the
/// page hears `map-landed` as each write ends. Nothing without a writer.
pub fn shown(app: &App, project: &str, user: &UserState, source: &str) {
    let (Some(writer), Some(store)) = (app.briefs.writer(), store_of(project)) else {
        return;
    };
    let events = app.events.clone();
    let landed = project.to_string();
    show(
        &app.catch_ups,
        &writer,
        &app.briefs.model(),
        project,
        user,
        source,
        &lit_of(project),
        &store,
        move || events.emit("map-landed", serde_json::json!(landed)),
    );
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
    let events = app.events.clone();
    let landed = project.clone();
    let opened = ask(
        &app.catch_ups,
        app.briefs.writer(),
        app.briefs.model(),
        &project,
        &part,
        fog,
        store,
        move || events.emit("map-landed", serde_json::json!(landed)),
    )?;
    match &opened {
        // Opening it is an entered activity on the part: its staleness band clears (#392).
        Opened::CatchUp(_) => {
            crate::staleness::record(
                &file,
                &project,
                &part,
                crate::staleness::Activity::CatchUp,
                crate::ratify::now(),
            )?;
        }
        // The sheet says it is being written.
        Opened::Writing { .. } => app.events.emit("map-landed", serde_json::json!(project)),
    }
    Ok(Some(opened))
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
            Rig::with(MAP, &[("a/x", "1"), ("b/x", "1")])
        }
        fn with(map: &str, files: &[(&str, &str)]) -> Rig {
            let (dir, main) = repo();
            let rig = Rig {
                dir,
                main,
                state: tempfile::tempdir().unwrap(),
            };
            let mut all = vec![("system-map.json", map)];
            all.extend_from_slice(files);
            rig.commit("Start the map", &all);
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

    // ---- written when the map shows ----

    const FIVE: &str = r#"{"version":1,
      "parts":[
        {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
        {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]},
        {"id":"d","name":"D","caption":"c","description":"d","tier":"core","paths":["d/"]},
        {"id":"e","name":"E","caption":"c","description":"d","tier":"core","paths":["e/"]},
        {"id":"f","name":"F","caption":"c","description":"d","tier":"interface","paths":["f/"]}],
      "actors":[],"links":[],"ignored":[],"laws":[]}"#;

    /// A fake writer: a catch-up of 3 steps on the part its prompt names, an empty claim check; it counts the catch-ups it wrote
    /// per part, and waits while its gate is shut.
    #[derive(Default)]
    struct Fake {
        wrote: Mutex<Vec<String>>,
        gate: (Mutex<bool>, std::sync::Condvar),
        /// Past its gate, the writer panics instead of writing.
        panics: bool,
    }

    impl Fake {
        fn shut() -> Arc<Fake> {
            let fake = Arc::new(Fake::default());
            *fake.gate.0.lock().unwrap() = true;
            fake
        }
        fn open(&self) {
            *self.gate.0.lock().unwrap() = false;
            self.gate.1.notify_all();
        }
        fn wrote(&self) -> Vec<String> {
            self.wrote.lock().unwrap().clone()
        }
    }

    impl BriefWriter for Fake {
        fn write(
            &self,
            _model: &str,
            prompt: &str,
        ) -> Result<studio_understanding::pr_brief::BriefReply, String> {
            let text = if prompt.starts_with("You write a part catch-up")
                && !prompt.contains("RETRY")
            {
                let mut shut = self.gate.0.lock().unwrap();
                while *shut {
                    shut = self.gate.1.wait(shut).unwrap();
                }
                drop(shut);
                assert!(!self.panics, "the writer broke");
                let part = prompt
                    .split("steps about part ")
                    .nth(1)
                    .and_then(|rest| rest.split(',').next())
                    .unwrap_or_default()
                    .to_string();
                self.wrote.lock().unwrap().push(part.clone());
                let step = |k: &str| serde_json::json!({ "k": k, "h": "h", "b": "b", "on": [part], "cites": [] });
                serde_json::json!({ "title": "t", "chapters": [ { "behaviour": "story", "parts": [part], "title": "t",
                    "steps": [step("Where"), step("Before"), step("Now")] } ], "also": [], "unseen": [], "decisions": [] })
                .to_string()
            } else {
                r#"{"items":[]}"#.to_string()
            };
            Ok(studio_understanding::pr_brief::BriefReply {
                text,
                usage: Default::default(),
            })
        }
    }

    struct Shows {
        rig: Rig,
        data: tempfile::TempDir,
        catch_ups: CatchUps,
        ended: std::sync::mpsc::Sender<()>,
        ends: std::sync::mpsc::Receiver<()>,
    }

    impl Shows {
        fn new() -> Shows {
            let files: Vec<(String, &str)> =
                "abcdef".chars().map(|p| (format!("{p}/x"), "1")).collect();
            let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), *t)).collect();
            let (ended, ends) = std::sync::mpsc::channel();
            Shows {
                rig: Rig::with(FIVE, &files),
                data: tempfile::tempdir().unwrap(),
                catch_ups: CatchUps::default(),
                ended,
                ends,
            }
        }
        fn push(&self, part: &str, text: &str) {
            self.rig
                .commit(&format!("push to {part}"), &[(&format!("{part}/x"), text)]);
        }
        fn store(&self) -> CatchUpStore {
            CatchUpStore::open(self.data.path()).unwrap()
        }
        /// The map is shown: fog is refreshed, then the writes start.
        fn show(&self, writer: &Arc<Fake>) -> Vec<String> {
            refresh(
                Path::new(&self.rig.main),
                &self.rig.file(),
                &self.rig.main,
                true,
            )
            .unwrap();
            let user = read_user(&self.rig.file(), &self.rig.main);
            let ended = self.ended.clone();
            let writer: SharedWriter = writer.clone();
            show(
                &self.catch_ups,
                &writer,
                "opus",
                &self.rig.main,
                &user,
                FIVE,
                &|_| None,
                &self.store(),
                move || ended.send(()).unwrap(),
            )
        }
        fn wait(&self, n: usize) {
            for _ in 0..n {
                self.ends
                    .recv_timeout(std::time::Duration::from_secs(30))
                    .expect("a write ends");
            }
        }
        fn fog(&self, part: &str) -> Vec<String> {
            commits(&self.rig.fog(part))
        }
    }

    #[test]
    fn showing_the_map_writes_three_fogged_core_parts_oldest_fog_first_and_a_fourth_on_click() {
        let shows = Shows::new();
        // The interface part's thin fog shows nothing; then five Core parts, c's fog the oldest.
        shows.push("f", "2");
        for part in ["c", "a", "e", "b", "d"] {
            shows.push(part, "2");
        }
        let writer = Arc::new(Fake::default());
        assert_eq!(shows.show(&writer), ["c", "a", "e"]);
        shows.wait(3);
        let mut wrote = writer.wrote();
        wrote.sort();
        assert_eq!(wrote, ["a", "c", "e"]);
        for part in ["c", "a", "e"] {
            assert!(
                fresh(&shows.store(), part, &shows.fog(part)).is_some(),
                "{part}"
            );
        }
        assert!(shows.store().get("b").is_none());

        // b is written when clicked.
        let ended = shows.ended.clone();
        let writer_: SharedWriter = writer.clone();
        let opened = ask(
            &shows.catch_ups,
            Some(writer_),
            "opus".into(),
            &shows.rig.main,
            "b",
            shows.fog("b"),
            shows.store(),
            move || ended.send(()).unwrap(),
        )
        .unwrap();
        assert!(matches!(opened, Opened::Writing { writing: true }));
        shows.wait(1);
        assert!(fresh(&shows.store(), "b", &shows.fog("b")).is_some());
        assert_eq!(writer.wrote().len(), 4);
    }

    #[test]
    fn a_new_fog_entry_makes_the_stored_catch_up_stale_and_the_next_show_rewrites_it() {
        let shows = Shows::new();
        shows.push("a", "2");
        let writer = Arc::new(Fake::default());
        assert_eq!(shows.show(&writer), ["a"]);
        shows.wait(1);
        // An unchanged fog list is never written again.
        assert_eq!(shows.show(&writer), Vec::<String>::new());
        assert_eq!(writer.wrote(), ["a"]);

        shows.push("a", "3");
        let list = shows.fog("a");
        assert_eq!(list.len(), 2);
        assert!(fresh(&shows.store(), "a", &list).is_none(), "stale");
        assert_eq!(shows.show(&writer), ["a"]);
        shows.wait(1);
        assert_eq!(writer.wrote(), ["a", "a"]);
        assert_eq!(shows.store().get("a").unwrap().fog, list);
    }

    #[test]
    fn a_click_for_a_longer_fog_list_while_the_shorter_one_is_written_gets_its_write_once_that_one_ends()
     {
        let shows = Shows::new();
        shows.push("a", "2");
        let writer = Fake::shut();
        assert_eq!(shows.show(&writer), ["a"]);
        shows.push("a", "3");
        let list = shows.fog("a");
        assert_eq!(list.len(), 2);
        let writer_: SharedWriter = writer.clone();
        let ended = shows.ended.clone();
        let opened = ask(
            &shows.catch_ups,
            Some(writer_),
            "opus".into(),
            &shows.rig.main,
            "a",
            list.clone(),
            shows.store(),
            move || ended.send(()).unwrap(),
        )
        .unwrap();
        assert!(matches!(opened, Opened::Writing { writing: true }));
        writer.open();
        shows.wait(1);
        assert_eq!(writer.wrote(), ["a", "a"]);
        assert_eq!(shows.store().get("a").unwrap().fog, list);
        assert!(!shows.catch_ups.writing(&shows.rig.main, "a"));
    }

    #[test]
    fn showing_the_map_twice_in_a_row_starts_one_write_of_each_catch_up() {
        let shows = Shows::new();
        shows.push("a", "2");
        shows.push("b", "2");
        let writer = Fake::shut();
        assert_eq!(shows.show(&writer), ["a", "b"]);
        assert_eq!(shows.show(&writer), Vec::<String>::new());
        writer.open();
        shows.wait(2);
        let mut wrote = writer.wrote();
        wrote.sort();
        assert_eq!(wrote, ["a", "b"]);
        assert_eq!(shows.show(&writer), Vec::<String>::new());
    }

    #[test]
    fn a_write_that_panics_tells_the_page_and_marks_the_newest_fog_list_failed() {
        let shows = Shows::new();
        shows.push("a", "2");
        let writer = Arc::new(Fake {
            panics: true,
            ..Default::default()
        });
        *writer.gate.0.lock().unwrap() = true;
        assert_eq!(shows.show(&writer), ["a"]);
        // A longer list is asked for while the first is written: it waits its turn.
        shows.push("a", "3");
        let list = shows.fog("a");
        let writer_: SharedWriter = writer.clone();
        let ended = shows.ended.clone();
        ask(
            &shows.catch_ups,
            Some(writer_),
            "opus".into(),
            &shows.rig.main,
            "a",
            list.clone(),
            shows.store(),
            move || ended.send(()).unwrap(),
        )
        .unwrap();
        writer.open();
        // The page hears the write ended, and the sheet offers a retry of the newest list.
        shows.wait(1);
        assert!(!shows.catch_ups.writing(&shows.rig.main, "a"));
        assert!(shows.catch_ups.failed(&shows.rig.main, "a", &list));
    }
}
