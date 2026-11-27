//! The Map view's shell side (spec danielrispler/plotkeeper#306, ticket #309): it reads `system-map.json` from `origin`'s
//! default branch as last fetched, keeps Daniel's own part positions and tiers in the data folder, and guards `git fetch`.
//!
//! Seams for the tickets that build on this one:
//! - `MapState` is the one payload the window draws from. A ticket adds its field there (fog, live agents, broken laws),
//!   fills it in `state`, and adds the same field to `MapState` in `ui/types.ts`.
//! - `UserState` is Daniel's per-project choices (never in the repository); a ticket adds a `#[serde(default)]` field.
//! - `Fetches` guards every `git fetch` of a repository: use `Fetches::run` for any new trigger.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use studio_workbench::run::{self, run_until};

/// The map file's name at the repository's root.
const MAP_FILE: &str = system_map::MAP_FILE;
/// Daniel's per-project map state, in the data folder.
const STATE_FILE: &str = "map-state.json";
/// A repository is fetched at most this often by the Map view.
pub const COOLDOWN: Duration = Duration::from_secs(60);

/// What the window shows of a project's map.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// `source` holds the map file's text.
    Loaded,
    /// The default branch has no `system-map.json`, or the repository has no default branch to read.
    None,
    /// The file is there but does not read; `problem` says why.
    Unreadable,
}

/// The payload of the Map view: everything the window draws for one project. Extend it with a field per feature.
#[derive(Debug, PartialEq, Serialize)]
pub struct MapState {
    pub project: String,
    pub status: Status,
    /// The text of `system-map.json` on the default branch; the window parses it.
    pub source: Option<String>,
    /// The ref it was read from, such as `origin/main`.
    pub reference: Option<String>,
    /// The commit that ref was at.
    pub commit: Option<String>,
    /// A quiet note: the fallback to the local default branch, or why the file does not read.
    pub note: Option<String>,
    /// Daniel's positions and tiers.
    pub user: UserState,
    /// How strongly each part shows fog under Daniel's effective tiers (`fog.rs`); a part not listed is clear.
    pub fog: BTreeMap<String, crate::fog::Strength>,
    /// CI on the default branch, as last read through `gh` (`ci_laws.rs`); none until a read has finished.
    pub ci: Option<crate::ci_laws::Ci>,
    /// The PRs ready for review (open, not a draft), as last asked (`prs.rs`); the "Ready for review" tray lists them.
    pub ready: Vec<crate::prs::Ready>,
    /// The items the part sheet says are not yet ratified (`ratify.rs`): the map changes that landed after the first run without a
    /// record, and the ones marked so by hand.
    pub ratify: Vec<crate::ratify::Pending>,
    /// The project's background runs, queued, running and ended, and the line that says why the queues are paused
    /// (`runner.rs`).
    pub runs: crate::runner::Runs,
    /// What each fogged part's sheet offers: a part catch-up, or the one PR brief chapter that lights it (`part_catch_ups.rs`).
    pub catch_ups: BTreeMap<String, crate::part_catch_ups::Offer>,
    /// The staleness band (1 to 3) of each Core part Daniel has not engaged with for 3, 6 or 12 weeks (`staleness.rs`); a part not listed
    /// is not stale.
    pub stale: BTreeMap<String, u8>,
    /// What each stale part's sheet says of its band (`staleness::lines`): the band, and when Daniel last engaged with the part or that
    /// he has not since tracking began (#495).
    pub stale_why: BTreeMap<String, String>,
    /// The project's pre-flight (`pre_flight.rs`): the parts a piece of work will likely cross, framed on the map.
    pub pre_flight: Option<studio_understanding::pre_flight::PreFlight>,
    /// The project's micro-worlds and whether a run can build one (`micro_worlds.rs`).
    pub micro_worlds: crate::micro_worlds::WorldsState,
    /// The patches of the project's back a law runs: being proven, rejected, or proven and kept (`back_a_law.rs`).
    pub law_patches: Vec<crate::back_a_law::LawPatch>,
}

/// Daniel's choices for one project, kept in the data folder: a saved `(x, y)` per part or actor id, and a tier per part id
/// that beats the map's own. Every field may be missing in an older file.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserState {
    #[serde(default)]
    pub positions: BTreeMap<String, [f64; 2]>,
    #[serde(default)]
    pub tiers: BTreeMap<String, String>,
    /// What landed on the default branch since Daniel caught up (`fog.rs`).
    #[serde(default)]
    pub fog: crate::fog::FogState,
    /// The map's camera as Daniel last left it (#383); none until he first moves it.
    #[serde(default)]
    pub view: Option<View>,
    /// When Daniel last engaged with each part, and the baseline (`staleness.rs`).
    #[serde(default)]
    pub staleness: crate::staleness::Staleness,
}

/// The Map view's camera: a world point `p` shows at `p * scale + (x, y)` in the map area (`Camera` in `ui/types.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct View {
    pub scale: f64,
    pub x: f64,
    pub y: f64,
}

/// The longest a read of a project's repository may take.
pub(crate) const GIT_READ_LIMIT: Duration = Duration::from_secs(30);

/// What `git args` prints in `dir`, read within [`GIT_READ_LIMIT`]; else what went wrong, for a failed git its own message.
pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = run_until(run::git(dir).args(args), GIT_READ_LIMIT).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// The ref the map is read from, and a note when it is not `origin`'s: `origin/HEAD`'s target, else the local `main` or
/// `master`, else none.
pub fn default_ref(repo: &Path) -> Option<(String, Option<String>)> {
    if let Ok(head) = git(
        repo,
        &["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"],
    ) && !head.is_empty()
    {
        return Some((head, None));
    }
    ["main", "master"].into_iter().find_map(|name| {
        git(
            repo,
            &["rev-parse", "--verify", "-q", &format!("refs/heads/{name}")],
        )
        .ok()?;
        Some((
            name.to_string(),
            Some(format!(
                "origin/HEAD is not set, so this is the local {name}"
            )),
        ))
    })
}

/// Reads `repo`'s map from its default branch, with no user state yet.
pub fn read_map(repo: &Path) -> MapState {
    let mut state = MapState {
        project: repo.display().to_string(),
        status: Status::None,
        source: None,
        reference: None,
        commit: None,
        note: None,
        user: UserState::default(),
        fog: BTreeMap::new(),
        ci: None,
        ready: Vec::new(),
        ratify: Vec::new(),
        runs: crate::runner::Runs::default(),
        catch_ups: BTreeMap::new(),
        stale: BTreeMap::new(),
        stale_why: BTreeMap::new(),
        pre_flight: None,
        micro_worlds: crate::micro_worlds::WorldsState::default(),
        law_patches: Vec::new(),
    };
    let Some((reference, note)) = default_ref(repo) else {
        state.note = Some("this repository has no default branch to read".into());
        return state;
    };
    state.commit = git(repo, &["rev-parse", &reference]).ok();
    state.note = note;
    state.reference = Some(reference.clone());
    // A missing file is the empty state; any other failure of `git show` is too, since the ref was just found.
    let Ok(text) = git(repo, &["show", &format!("{reference}:{MAP_FILE}")]) else {
        return state;
    };
    // A version 1 map is stored converted, so everything after reads version 2 only.
    match system_map::convert(&text).and_then(|text| system_map::parse_lenient(&text).map(|_| text))
    {
        Ok(text) => {
            state.status = Status::Loaded;
            state.source = Some(text);
        }
        Err(problem) => {
            state.status = Status::Unreadable;
            state.note = Some(format!("{MAP_FILE} does not read: {problem}"));
        }
    }
    state
}

/// The outcome of a guarded fetch.
#[derive(Debug, PartialEq)]
pub enum Fetched {
    Done,
    /// Within the cooldown, or one is running.
    Skipped,
    Failed(String),
}

/// The per-repository cooldown and in-flight lock of `git fetch`, so rapid focus changes never spam it.
#[derive(Default)]
pub struct Fetches(Mutex<HashMap<String, Slot>>);

#[derive(Default)]
struct Slot {
    last: Option<Instant>,
    running: bool,
}

/// Marks the fetch of its repository as no longer running when dropped.
struct Running<'a>(&'a Fetches, &'a str);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(self.1.to_string())
            .or_default()
            .running = false;
    }
}

impl Fetches {
    /// Runs `fetch` for `repo` unless one is running or the last began within `COOLDOWN` of `now`.
    pub fn run(
        &self,
        repo: &str,
        now: Instant,
        fetch: impl FnOnce() -> Result<(), String>,
    ) -> Fetched {
        self.run_after(repo, now, COOLDOWN, fetch)
    }

    /// As `run`, with its own `cooldown`: zero asks at once unless one is running.
    pub fn run_after(
        &self,
        repo: &str,
        now: Instant,
        cooldown: Duration,
        fetch: impl FnOnce() -> Result<(), String>,
    ) -> Fetched {
        {
            let mut slots = self.0.lock().unwrap();
            let slot = slots.entry(repo.to_string()).or_default();
            let cooling = slot
                .last
                .is_some_and(|at| now.saturating_duration_since(at) < cooldown);
            if slot.running || cooling {
                return Fetched::Skipped;
            }
            slot.running = true;
            slot.last = Some(now);
            drop(slots);
        }
        // The lock is not held while git runs, so a second trigger meets `running`; the guard clears it even if `fetch` panics.
        let _running = Running(self, repo);
        let result = fetch();
        drop(_running);
        match result {
            Ok(()) => Fetched::Done,
            Err(e) => Fetched::Failed(e),
        }
    }
}

pub(crate) fn state_file() -> Result<PathBuf, String> {
    let home = std::env::home_dir().ok_or("no home folder")?;
    Ok(studio_hook::data_dir(&home).join(STATE_FILE))
}

/// Daniel's state for `project` from `file`; a missing or unreadable file is an empty state.
pub fn read_user(file: &Path, project: &str) -> UserState {
    crate::json_file::read_project(file, project)
}

/// Serialises the writes, so two changes never lose one another.
static WRITING: Mutex<()> = Mutex::new(());

/// Changes `project`'s state in `file` with `change`, keeping the other projects' and any fields this build does not know,
/// written by a temporary file and a rename.
pub fn update_user(
    file: &Path,
    project: &str,
    change: impl FnOnce(&mut UserState),
) -> Result<UserState, String> {
    Ok(crate::json_file::update_json_file(
        file,
        STATE_FILE,
        &WRITING,
        project,
        change,
        |_| {},
    )?)
}

/// Fetches `origin` for `project` through the cooldown. A failure is logged quietly and the refs stay as they were.
fn fetch(fetches: &Fetches, project: &str) -> Fetched {
    let result = fetches.run(project, Instant::now(), || {
        studio_workbench::fetch(Path::new(project))
    });
    if let Fetched::Failed(why) = &result {
        log::warn!("map: fetch of {project} failed: {why}");
    }
    result
}

/// The Map view's payload for `project`. `fetch` asks for `origin` first: when the view opens and when the window gains
/// focus while it is open, never on a timer; the cooldown and the in-flight lock decide whether it runs. The same triggers
/// read CI on the default branch in the background (`ci_laws::refresh_later`), which tells the window when it has an answer.
pub fn map_state(project: String, fetch: bool, app: &Arc<crate::App>) -> Result<MapState, String> {
    let (fetches, cache, events) = (&app.fetches, &app.ci, &app.events);
    if fetch {
        crate::prs::ask_later(app, crate::prs::Trigger::MapOpen, Some(&project));
        let fetched = self::fetch(fetches, &project);
        // A fetch inside the cooldown asks nothing again, unless nothing was read yet.
        if fetched != Fetched::Skipped || !cache.has(&project) {
            crate::ci_laws::refresh_later(cache, events, &project);
        }
    }
    let repo = Path::new(&project);
    let file = state_file()?;
    // A `fetch` is an open or a focus: the first of them starts the project's fog clear. A quiet read starts nothing.
    if let Err(why) = crate::fog::refresh(repo, &file, &project, fetch) {
        log::warn!("map: fog of {project} failed: {why}");
    }
    // The first map with this build is every part's staleness baseline.
    if let Err(why) = crate::staleness::start(&file, &project, crate::ratify::now()) {
        log::warn!("map: staleness of {project} failed: {why}");
    }
    let mut state = read_map(repo);
    state.user = read_user(&file, &project);
    if let Some(source) = &state.source {
        state.fog = crate::fog::effective(&state.user, source);
        let born = crate::staleness::births(&app.births, repo, source);
        let now = crate::ratify::now();
        state.stale = crate::staleness::bands(&state.user, source, &born, now);
        state.stale_why = crate::staleness::lines(&state.user, source, &born, now);
    }
    state.ci = cache.get(&project);
    state.ready = app.prs.ready(&project);
    app.briefs.annotate(&project, &mut state.ready);
    state.ratify = crate::ratify::file()
        .and_then(|file| crate::ratify::start(&file, &project, crate::ratify::now()))
        .map(|mine| crate::ratify::pending(&app.changes, repo, &mine))
        .unwrap_or_default();
    state.runs = app.runner.runs(&project);
    // A shown map starts the part catch-ups of up to eight fogged Core parts (#396, #450).
    if fetch && let Some(source) = &state.source {
        crate::part_catch_ups::shown(app, &project, &state.user, source);
    }
    state.catch_ups = crate::part_catch_ups::offers(app, &project, &state.user, &state.fog);
    state.pre_flight = crate::pre_flight::current(&project);
    state.micro_worlds = app.worlds.state(&project, state.commit.as_deref());
    state.law_patches = app.laws.patches(&project);
    Ok(state)
}

/// "I know this": clears `part`'s fog.
pub fn map_know(project: String, part: String) -> Result<UserState, String> {
    let file = state_file()?;
    crate::staleness::record(
        &file,
        &project,
        &part,
        crate::staleness::Activity::Know,
        crate::ratify::now(),
    )?;
    crate::fog::clear(&file, &project, &part)
}

/// The refresher of `part` was entered: an activity on the part, which clears its staleness band.
pub fn map_refresher(project: String, part: String) -> Result<UserState, String> {
    crate::staleness::record(
        &state_file()?,
        &project,
        &part,
        crate::staleness::Activity::Refresher,
        crate::ratify::now(),
    )
}

/// A part's history strip: its first-parent commits on the default branch from the map's birth, oldest to newest, the ones Daniel
/// has not caught up on marked.
pub fn map_history(project: String, part: String) -> Result<Vec<crate::history::Entry>, String> {
    let briefs = crate::pr_briefs::store_of(&project);
    let fogged = read_user(&state_file()?, &project)
        .fog
        .parts
        .get(&part)
        .map(|entries| entries.iter().map(|e| e.commit.clone()).collect())
        .unwrap_or_default();
    let mut entries = crate::history::part_history(Path::new(&project), &part, &fogged);
    // A commit whose PR has a stored brief offers it.
    for entry in &mut entries {
        entry.brief = crate::history::pr_of_label(&entry.label).filter(|n| {
            briefs
                .as_ref()
                .is_some_and(|store| store.latest(*n).is_some())
        });
    }
    Ok(entries)
}

/// The map as of `commit`: what one scrub of the history strip asks the shell for. A later ticket that records scrubs hooks here.
pub fn map_at(project: String, commit: String) -> Result<crate::history::At, String> {
    crate::history::at(Path::new(&project), &commit)
}

/// The files and diffs of `commit`, for the drawer.
pub fn map_commit(project: String, commit: String) -> Result<crate::history::Commit, String> {
    crate::history::commit_files(Path::new(&project), &commit)
}

/// What "See the code" of a PR's brief asks for (`PrDiffArgs` in `ui/types.ts`). `parts`: only the files owned by these parts, under the
/// map at `head` (the default branch's when `head` has none); `paths`: only these files (an Also touched group's, #492); all the PR's
/// files when both are absent.
#[derive(Debug, Serialize, Deserialize)]
pub struct PrDiffArgs {
    pub project: String,
    pub number: u64,
    pub head: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// A "Since you read" story's (#497): the head it runs from, so the diff runs from there, not from where the PR left the default
    /// branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// "See the code" of a PR's brief: the files its head changed since it left the default branch, with their diffs, for the drawer.
pub fn pr_diff(args: PrDiffArgs) -> Result<crate::history::Commit, String> {
    let PrDiffArgs {
        project,
        number,
        head,
        title,
        parts,
        paths,
        base,
    } = args;
    let repo = Path::new(&project);
    // The map at the PR's head, as its brief's chapters light parts by it; the default branch's when the head has none.
    let map = parts.as_ref().and_then(|_| {
        crate::history::at(repo, &head)
            .map(|at| at.source)
            .ok()
            .or_else(|| read_map(repo).source)
            .and_then(|text| system_map::parse_lenient(&text).ok())
    });
    let keep = |all: Vec<String>| match (parts, paths) {
        (None, None) => all,
        (None, Some(paths)) => all
            .into_iter()
            .filter(|path| paths.contains(path))
            .collect(),
        (Some(parts), paths) => {
            let owned: BTreeSet<String> = map
                .map(|map| system_map::touched_for_map(&map, &all).parts)
                .unwrap_or_default()
                .into_iter()
                .filter(|found| parts.contains(&found.id))
                .flat_map(|found| found.files)
                .collect();
            all.into_iter()
                .filter(|path| owned.contains(path))
                .filter(|path| paths.as_ref().is_none_or(|paths| paths.contains(path)))
                .collect()
        }
    };
    crate::history::pr_files(
        repo,
        &head,
        base.as_deref(),
        &format!("#{number} {title}"),
        keep,
    )
}

/// Why a project header's quiet dot lights (#344, #386): fog on a Core part, a broken law, a PR brief waiting, in that order.
#[derive(Debug, PartialEq, Serialize)]
pub struct Mark {
    pub project: String,
    pub why: Vec<MarkWhy>,
}

/// One reason a header's dot lights; `MarkWhy` in `ui/types.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkWhy {
    Fog,
    Broken,
    Brief,
}

/// The projects among `projects` that show a fogged Core part, a broken law or a waiting PR brief, with which of the three, for the project
/// header's quiet dot and its hover. Reads local refs and the last CI read only: it asks neither `origin` nor GitHub. A gone folder has
/// no map to read and gets no mark (#381).
pub fn map_marks(
    projects: Vec<String>,
    cache: &crate::ci_laws::CiCache,
    prs: &crate::prs::Prs,
) -> Result<Vec<Mark>, String> {
    let file = state_file()?;
    Ok(projects
        .into_iter()
        .filter_map(|project| {
            let repo = Path::new(&project);
            if !repo.is_dir() {
                return None;
            }
            if let Err(e) = crate::fog::refresh(repo, &file, &project, false) {
                log::warn!("the fog of {project} was not refreshed: {e}");
            }
            let fog = read_map(repo).source.is_some_and(|source| {
                crate::fog::core_fogged(&read_user(&file, &project), &source)
            });
            let broken = cache.broken(&project);
            let brief = crate::pr_briefs::any_waiting(&project, &prs.ready(&project));
            let why: Vec<MarkWhy> = [
                (fog, MarkWhy::Fog),
                (broken, MarkWhy::Broken),
                (brief, MarkWhy::Brief),
            ]
            .into_iter()
            .filter_map(|(lit, name)| lit.then_some(name))
            .collect();
            (!why.is_empty()).then_some(Mark { project, why })
        })
        .collect())
}

/// "See the proof": the tests that back `law` on the default branch, with the code around their `@law:` tag.
pub fn map_proof(project: String, law: String) -> Vec<crate::ci_laws::Proof> {
    crate::ci_laws::proofs(Path::new(&project), &law)
}

/// Saves the positions of parts or actors, by id; a position that is not a finite number is refused.
pub fn map_save_positions(
    project: String,
    positions: BTreeMap<String, [f64; 2]>,
) -> Result<UserState, String> {
    if positions.values().flatten().any(|n| !n.is_finite()) {
        return Err("a position must be a number".into());
    }
    update_user(&state_file()?, &project, |user| {
        user.positions.extend(positions);
    })
}

/// Saves `project`'s camera in `file`, beside its positions; a camera that is not finite numbers, or has no scale, is refused.
pub fn save_view(file: &Path, project: &str, view: View) -> Result<UserState, String> {
    if ![view.scale, view.x, view.y].iter().all(|n| n.is_finite()) || view.scale <= 0.0 {
        return Err("a view is a positive scale and a finite x and y".into());
    }
    update_user(file, project, |user| user.view = Some(view))
}

/// Saves the Map view's camera for `project` in the data folder.
pub fn map_save_view(project: String, view: View) -> Result<UserState, String> {
    save_view(&state_file()?, &project, view)
}

/// Sets part `part`'s tier, `null` clearing Daniel's choice so the map's own tier applies again.
pub fn map_set_tier(
    project: String,
    part: String,
    tier: Option<String>,
) -> Result<UserState, String> {
    if tier
        .as_deref()
        .is_some_and(|t| !["core", "interface", "black-box"].contains(&t))
    {
        return Err("a tier is core, interface or black-box".into());
    }
    update_user(&state_file()?, &project, |user| match tier {
        Some(tier) => drop(user.tiers.insert(part, tier)),
        None => drop(user.tiers.remove(&part)),
    })
}

/// The parts each group of paths touches, by the map on `repo`'s default branch: group key to part ids, in the map's order.
/// A group whose paths are all ignored or owned by no part maps to no part; with no readable map every group does.
pub fn place(repo: &Path, groups: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
    let map = read_map(repo)
        .source
        .and_then(|text| system_map::parse_lenient(&text).ok());
    groups
        .iter()
        .map(|(key, paths)| {
            let parts = map.as_ref().map_or_else(Vec::new, |map| {
                let found = system_map::touched_for_map(map, paths);
                found.parts.into_iter().map(|part| part.id).collect()
            });
            (key.clone(), parts)
        })
        .collect()
}

/// Which parts each live agent's current turn touched (ticket #311): the window sends its changed paths by agent key.
pub fn map_place(
    project: String,
    groups: BTreeMap<String, Vec<String>>,
) -> BTreeMap<String, Vec<String>> {
    place(Path::new(&project), &groups)
}

/// Whether a lookup of a task is a transition to merged: the task was looked up before and was not merged. A task first seen
/// merged (after a restart) is not a transition, so it starts no fetch.
pub fn became_merged(
    before: Option<studio_core::TaskFacts>,
    after: Option<studio_core::TaskFacts>,
) -> bool {
    before.is_some_and(|b| !b.merged) && after.is_some_and(|a| a.merged)
}

/// `refresh_landed` saw the task in `task_path` become merged: fetch its repository through the cooldown and read CI, on a
/// thread of its own so the caller's loop never waits on the network, then tell the window, which draws the map again if it
/// is open.
pub fn merged(
    fetches: &Arc<Fetches>,
    cache: &Arc<crate::ci_laws::CiCache>,
    events: &Arc<crate::Events>,
    task_path: &str,
) {
    let Ok(project) = studio_workbench::main_checkout(Path::new(task_path)) else {
        return;
    };
    let (fetches, cache, events) = (fetches.clone(), cache.clone(), events.clone());
    std::thread::spawn(move || {
        fetch(&fetches, &project);
        crate::ci_laws::refresh_later(&cache, &events, &project);
        events.emit("map-landed", project.into());
    });
}

#[cfg(test)]
mod tests;
