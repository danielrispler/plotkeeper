//! Part catch-ups (spec danielrispler/plotkeeper#378, tickets #391 and #396): a fogged part offers one story of everything that
//! landed in it since Daniel last caught up. When a project's map is shown, the catch-ups of up to eight fogged Core parts are
//! written, oldest fog first (#450); at a session's turn end, those of up to eight fogged Core and Interface-only parts, once
//! `origin` is fetched (#635); any other is written when he clicks it. A part fogged by exactly one PR whose brief lights it opens
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
use studio_understanding::pr_brief_check::Stage;

/// What a fogged part's sheet offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Offer {
    /// The one PR's brief has a chapter that lights the part: that chapter opens.
    Chapter { pr: u64, chapter: usize },
    /// A part catch-up: `ready` when one is stored for the part's current fog list, `writing` while one is written, `failed` when
    /// the last write for this fog list failed. `stage`: which call of the write runs now (#450). `outline`, when `ready`: what the
    /// sheet's catch-up card shows of it (#636). `older`, while none is stored for the current list: the stored one Daniel can read
    /// meanwhile (#638).
    CatchUp {
        ready: bool,
        writing: bool,
        failed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        stage: Option<Stage>,
        #[serde(skip_serializing_if = "Option::is_none")]
        outline: Option<Outline>,
        #[serde(skip_serializing_if = "Option::is_none")]
        older: Option<Older>,
    },
}

/// A part's stored catch-up written for an earlier fog list whose every entry is still in the part's fog (spec #633, "Opening the
/// older catch-up"): its outline and the fog entries it covers (`fog`, their commits), which reading it to the end clears.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Older {
    pub outline: Outline,
    pub fog: Vec<String>,
}

/// A stored catch-up as its part's catch-up card shows it (spec #633, "The offer carries what the card shows"): its title and, per
/// step of its one chapter, the step's kind and headline. Never its gist, which stays behind Explain-back.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outline {
    pub title: String,
    pub steps: Vec<OutlineStep>,
}

/// One step of an `Outline`: its kind (`k`: Where, Before, Now, ...) and headline (`h`), named as in the PR brief's steps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutlineStep {
    pub k: String,
    pub h: String,
}

/// `stored`'s outline.
fn outline(stored: &StoredCatchUp) -> Outline {
    let steps = stored.brief.chapters.first().map_or(&[][..], |c| &c.steps);
    Outline {
        title: stored.brief.title.clone(),
        steps: steps
            .iter()
            .map(|step| OutlineStep {
                k: step.k.clone(),
                h: step.h.clone(),
            })
            .collect(),
    }
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
        stage: None,
        outline: None,
        older: None,
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

/// The catch-up stored for `part` when it covers an earlier fog list than `fog`, its current one, whose every entry is still in
/// `fog`: Daniel can read it while the new one is written, and reading it clears only what it covers. None that holds an entry
/// cleared since, which would tell him again of a change he caught up on.
pub fn earlier(store: &CatchUpStore, part: &str, fog: &[String]) -> Option<StoredCatchUp> {
    store.get(part).filter(|stored| {
        !stored.fog.is_empty()
            && stored.fog != fog
            && stored.fog.iter().all(|commit| fog.contains(commit))
    })
}

/// What is being written and what failed, per project and part. A write's claim ends when its guard drops, a panic included, and
/// the page hears it either way.
#[derive(Default)]
pub struct CatchUps {
    busy: Arc<Mutex<Busy>>,
    /// The fog list whose write failed, per project and part: the sheet says so, and a click tries again.
    failed: Arc<Mutex<Failed>>,
    /// Which call of its write runs now, per project and part being written.
    stages: Arc<Mutex<Stages>>,
    /// Told the project whose write moved on to its next call, so the page draws the new stage.
    on_stage: Option<Tell>,
}

type Stages = HashMap<(String, String), Stage>;

/// Told a project's name.
type Tell = Arc<dyn Fn(&str) + Send + Sync>;

/// The fog list whose write failed, by project and part.
type Failed = HashMap<(String, String), Vec<String>>;

/// Per project and part being written: the fog list being written, and the one asked for meanwhile, written once that one ends.
type Busy = HashMap<(String, String), (Vec<String>, Option<Vec<String>>)>;

struct Claim {
    busy: Arc<Mutex<Busy>>,
    failed: Arc<Mutex<Failed>>,
    stages: Arc<Mutex<Stages>>,
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
            // The stage goes while the part is still this claim's, so it never wipes a new write's.
            self.stages.lock().unwrap().remove(&self.key);
            busy.remove(&self.key);
            drop(busy);
            self.released = true;
        }
        queued
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        // A write panicked: the newest list asked for is marked failed, so the sheet says so and a click tries it again.
        // A released claim no longer owns the part, so it touches neither its stage nor its claim.
        if !self.released {
            let mut busy = self.busy.lock().unwrap();
            self.stages.lock().unwrap().remove(&self.key);
            if let Some((writing, queued)) = busy.remove(&self.key) {
                let list = queued.unwrap_or(writing);
                self.failed.lock().unwrap().insert(self.key.clone(), list);
            }
        }
        if let Some(done) = self.done.take() {
            done();
        }
    }
}

impl CatchUps {
    /// The catch-ups, telling `on_stage` the project whose write moved on to its next call.
    pub fn telling(on_stage: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            on_stage: Some(Arc::new(on_stage)),
            ..Self::default()
        }
    }

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
        drop(busy);
        Some(Claim {
            busy: self.busy.clone(),
            failed: self.failed.clone(),
            stages: self.stages.clone(),
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

    fn stage(&self, project: &str, part: &str) -> Option<Stage> {
        self.stages
            .lock()
            .unwrap()
            .get(&(project.to_string(), part.to_string()))
            .cloned()
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
                Offer::CatchUp { .. } => catch_up_offer(
                    catch_ups.as_ref(),
                    &app.catch_ups,
                    project,
                    part,
                    &commits(&fog),
                ),
                chapter => chapter,
            };
            (part.clone(), offered)
        })
        .collect()
}

/// The catch-up offer of `part` over its fog list `list`, from what `store` holds and what `catch_ups` is writing.
fn catch_up_offer(
    store: Option<&CatchUpStore>,
    catch_ups: &CatchUps,
    project: &str,
    part: &str,
    list: &[String],
) -> Offer {
    let stored = store.and_then(|s| fresh(s, part, list));
    let older = store.and_then(|s| earlier(s, part, list));
    Offer::CatchUp {
        ready: stored.is_some(),
        writing: catch_ups.writing(project, part),
        failed: catch_ups.failed(project, part, list),
        stage: catch_ups.stage(project, part),
        outline: stored.as_ref().map(outline),
        older: older.map(|o| Older {
            outline: outline(&o),
            fog: o.fog,
        }),
    }
}

/// What a click on a part's catch-up answers: the catch-up, or that it is being written (it comes through `map-landed`).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Opened {
    CatchUp(Box<StoredCatchUp>),
    Writing { writing: bool },
}

type SharedWriter = Arc<dyn BriefWriter + Send + Sync>;

/// At most this many fogged Core parts get their catch-up written when a project's map is shown (spec #443, "Part catch-up
/// writing", which raised #396's 3).
pub const ON_SHOW: usize = 8;

/// Starts writing `part`'s catch-up over `fog` in the background. While one over another list is being written, `fog` is written
/// once that one ends, so the part is never left with neither a write nor a catch-up for its list. `done` runs when the writes end,
/// written or failed. Whether a write started.
#[allow(
    clippy::too_many_arguments,
    reason = "no type groups these inputs, and one made only for this call would be a new abstraction (#555)"
)]
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
    let (stages, on_stage) = (catch_ups.stages.clone(), catch_ups.on_stage.clone());
    let (project, part) = (project.to_string(), part.to_string());
    std::thread::spawn(move || {
        let mut fog = fog;
        let key = (project.clone(), part.clone());
        let staged = |stage: Stage| {
            stages.lock().unwrap().insert(key.clone(), stage);
            if let Some(tell) = &on_stage {
                tell(&project);
            }
        };
        loop {
            let written = gather_catch_up(Path::new(&project), &part, &fog).and_then(|source| {
                write_part_catch_up(
                    writer.as_ref(),
                    &model,
                    &source,
                    &store,
                    |line| log::warn!("part catch-up of {part}: {line}"),
                    staged,
                )
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
/// tried again) and the answer says it is being written. `older`: "Read the one you have" asks for the stored one even though it
/// covers an earlier list, answered with it only while `earlier` holds, else as any click.
#[allow(
    clippy::too_many_arguments,
    reason = "no type groups these inputs, and one made only for this call would be a new abstraction (#555)"
)]
pub fn ask(
    catch_ups: &CatchUps,
    writer: Option<SharedWriter>,
    model: String,
    project: &str,
    part: &str,
    fog: Vec<String>,
    older: bool,
    store: CatchUpStore,
    done: impl FnOnce() + Send + 'static,
) -> Result<Opened, String> {
    let stored =
        fresh(&store, part, &fog).or_else(|| older.then(|| earlier(&store, part, &fog)).flatten());
    if let Some(stored) = stored {
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
#[allow(
    clippy::too_many_arguments,
    reason = "no type groups these inputs, and one made only for this call would be a new abstraction (#555)"
)]
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
    let parts = crate::fog::core_fogged_parts(user, source);
    write_ahead(
        catch_ups, writer, model, project, user, parts, lit_of, store, done,
    )
}

/// Starts the catch-ups of at most `ON_SHOW` of `parts` (fogged ones), oldest fog first, as `show` says.
#[allow(
    clippy::too_many_arguments,
    reason = "no type groups these inputs, and one made only for this call would be a new abstraction (#555)"
)]
fn write_ahead(
    catch_ups: &CatchUps,
    writer: &SharedWriter,
    model: &str,
    project: &str,
    user: &UserState,
    parts: Vec<String>,
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
    let mut fogged: Vec<(u64, String, Vec<String>)> = parts
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

/// `project`'s session ended its turn: `origin` is fetched through the cooldown of `fetches` (`fetch`), and `read_ci` runs when
/// that fetch was not skipped; the fog is refreshed without starting it, and the catch-ups of at most `ON_SHOW` fogged Core and
/// Interface-only parts start writing as `show` does, into the store that `store` opens, with the PR briefs that `lit_of` opens.
/// `None` when the project's fog never started (its map was never opened): nothing is fetched, read, opened or written. Else the
/// parts whose write started; `done` runs as each ends.
#[allow(
    clippy::too_many_arguments,
    reason = "no type groups these inputs, and one made only for this call would be a new abstraction (#555)"
)]
pub fn turn_end<L: Fn(u64) -> Option<Vec<Vec<String>>>>(
    catch_ups: &CatchUps,
    writer: Option<&SharedWriter>,
    model: &str,
    fetches: &crate::map::Fetches,
    fetch: impl FnOnce() -> Result<(), String>,
    read_ci: impl FnOnce(),
    project: &str,
    file: &Path,
    lit_of: impl FnOnce() -> L,
    store: impl FnOnce() -> Option<CatchUpStore>,
    done: impl Fn() + Clone + Send + 'static,
) -> Option<Vec<String>> {
    let repo = Path::new(project);
    // A map never opened has no fog to refresh, so nothing is fetched for it either.
    crate::map::read_user(file, project)
        .fog
        .processed
        .as_ref()?;
    match fetches.run(project, std::time::Instant::now(), fetch) {
        crate::map::Fetched::Skipped => {}
        fetched => {
            if let crate::map::Fetched::Failed(why) = fetched {
                log::warn!("turn end: fetch of {project} failed: {why}");
            }
            // The map's own fetch now waits out this one's cooldown, so CI is read here, as the map's open would.
            read_ci();
        }
    }
    if let Err(why) = crate::fog::refresh(repo, file, project, false) {
        log::warn!("turn end: fog of {project} failed: {why}");
    }
    let (Some(writer), Some(source), Some(store)) =
        (writer, crate::map::read_map(repo).source, store())
    else {
        return Some(Vec::new());
    };
    let user = crate::map::read_user(file, project);
    // The fogged parts the map shows: Core and Interface-only, since a black box never shows fog (`fog::effective`).
    let parts = crate::fog::effective(&user, &source).into_keys().collect();
    let lit_of = lit_of();
    Some(write_ahead(
        catch_ups, writer, model, project, &user, parts, &lit_of, &store, done,
    ))
}

/// A session in `folder` ended its turn: in the background, `turn_end` on its main checkout with `origin`'s fetch, CI's read
/// (`ci_laws::refresh_later`), the PR brief's writer and model, and the page hears `map-landed` once the fog is refreshed and as
/// each write ends. Nothing for a project whose map was never opened, which gets no data folder either.
pub fn turn_ended(app: &Arc<App>, folder: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        let Ok(project) = studio_workbench::main_checkout(Path::new(&folder)) else {
            return;
        };
        let Ok(file) = crate::map::state_file() else {
            return;
        };
        let events = app.events.clone();
        let landed = project.clone();
        let started = turn_end(
            &app.catch_ups,
            app.briefs.writer().as_ref(),
            &app.briefs.model(),
            &app.fetches,
            || studio_workbench::fetch(Path::new(&project)),
            || crate::ci_laws::refresh_later(&app.ci, &app.events, &project),
            &project,
            &file,
            || lit_of(&project),
            || store_of(&project),
            move || events.emit("map-landed", serde_json::json!(landed)),
        );
        // The map redraws its fog, and the sheets their writes.
        if started.is_some() {
            app.events.emit("map-landed", serde_json::json!(project));
        }
    });
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

/// A click on `part`'s catch-up: the stored one when it covers the part's current fog list (or, with `older`, an earlier list as
/// `ask` says), else it is written in the background with the PR brief's writer and model, and the answer says so; the page gets
/// it through `map-landed`. `None`: the part is clear.
pub fn part_catch_up(
    app: &Arc<App>,
    project: String,
    part: String,
    older: bool,
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
        older,
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
mod tests;
