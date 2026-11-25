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
    process::Command,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

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

pub(crate) fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
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
            .unwrap_or_else(|e| e.into_inner())
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

fn read_file(file: &Path) -> serde_json::Value {
    let parsed = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    parsed
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Daniel's state for `project` from `file`; a missing or unreadable file is an empty state.
pub fn read_user(file: &Path, project: &str) -> UserState {
    read_file(file)
        .get("projects")
        .and_then(|projects| projects.get(project))
        .and_then(|user| serde_json::from_value(user.clone()).ok())
        .unwrap_or_default()
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
    let _writing = WRITING.lock().unwrap();
    let mut whole = read_file(file);
    let mut user = read_user(file, project);
    change(&mut user);
    let entry = whole
        .as_object_mut()
        .unwrap()
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("map-state.json: projects is not an object")?
        .entry(project)
        .or_insert_with(|| serde_json::json!({}));
    let saved = serde_json::to_value(&user).map_err(|e| e.to_string())?;
    match entry.as_object_mut() {
        Some(object) => object.extend(saved.as_object().unwrap().clone()),
        None => *entry = saved,
    }
    let dir = file.parent().ok_or("no folder for map-state.json")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, &whole).map_err(|e| e.to_string())?;
    temp.persist(file).map_err(|e| e.to_string())?;
    Ok(user)
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
        let born = crate::staleness::births(repo, source);
        let now = crate::ratify::now();
        state.stale = crate::staleness::bands(&state.user, source, &born, now);
        state.stale_why = crate::staleness::lines(&state.user, source, &born, now);
    }
    state.ci = cache.get(&project);
    state.ready = app.prs.ready(&project);
    app.briefs.annotate(&project, &mut state.ready);
    state.ratify = crate::ratify::file()
        .and_then(|file| crate::ratify::start(&file, &project, crate::ratify::now()))
        .map(|mine| crate::ratify::pending(repo, &mine))
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
/// default branch's map; `paths`: only these files (an Also touched group's, #492); all the PR's files when both are absent.
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
    } = args;
    let repo = Path::new(&project);
    let map = parts.as_ref().and_then(|_| {
        read_map(repo)
            .source
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
    crate::history::pr_files(repo, &head, &format!("#{number} {title}"), keep)
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
            let _ = crate::fog::refresh(repo, &file, &project, false);
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
        user.positions.extend(positions)
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
mod tests {
    use super::*;
    use studio_test_support::{git, origin, repo};

    const MAP: &str = r#"{"version":2,"parts":[],"actors":[],"links":[],"ignored":[],"laws":[]}"#;

    #[test]
    fn a_gone_folder_gets_no_header_mark_and_its_map_is_not_read() {
        let data = tempfile::tempdir().unwrap();
        let gone = data.path().join("pokedex").display().to_string();
        let file = data.path().join("ci-last.json");
        std::fs::write(
            &file,
            serde_json::json!({ &gone: { "status": "broken", "parts": ["shell"] } }).to_string(),
        )
        .unwrap();
        let cache = crate::ci_laws::CiCache::load(file);
        assert!(cache.broken(&gone));
        assert_eq!(
            map_marks(vec![gone], &cache, &crate::prs::Prs::default()).unwrap(),
            []
        );
    }

    fn commit_file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
        git(dir, &["add", name]);
        git(dir, &["commit", "-q", "-m", name]);
    }

    #[test]
    fn the_map_comes_from_origins_default_branch_not_the_working_tree() {
        let o = origin();
        commit_file(&o.seed, MAP_FILE, MAP);
        git(&o.seed, &["push", "-q", "origin", "HEAD"]);
        git(&o.main, &["fetch", "-q", "origin"]);
        // The working tree's own copy differs and is never read.
        commit_file(&o.main, MAP_FILE, "not the map");
        let state = read_map(&o.main);
        assert_eq!(state.status, Status::Loaded);
        assert_eq!(state.source.as_deref(), Some(MAP));
        assert_eq!(state.reference.as_deref(), Some("origin/trunk"));
        assert_eq!(state.note, None);
        assert_eq!(
            state.commit,
            Some(git(&o.main, &["rev-parse", "origin/trunk"]))
        );
    }

    #[test]
    fn a_group_of_paths_is_placed_on_every_part_it_touches() {
        let o = origin();
        let map = r#"{"version":1,"parts":[
            {"id":"core","name":"Core","caption":"","description":"","tier":"core","paths":["core/"]},
            {"id":"window","name":"Window","caption":"","description":"","tier":"core","paths":["ui/"]}],
            "actors":[],"links":[],"ignored":["docs/"],"laws":[]}"#;
        commit_file(&o.seed, MAP_FILE, map);
        git(&o.seed, &["push", "-q", "origin", "HEAD"]);
        git(&o.main, &["fetch", "-q", "origin"]);
        let paths = |list: &[&str]| list.iter().map(|p| p.to_string()).collect::<Vec<_>>();
        let groups = BTreeMap::from([
            ("both".to_string(), paths(&["ui/a.ts", "core/b.rs"])),
            ("docs".to_string(), paths(&["docs/x.md"])),
            ("none".to_string(), paths(&[])),
        ]);
        let placed = place(&o.main, &groups);
        assert_eq!(placed["both"], ["core", "window"]);
        assert!(placed["docs"].is_empty());
        assert!(placed["none"].is_empty());
        // With no map on the branch, nothing is placed.
        let bare = origin();
        assert!(place(&bare.main, &groups)["both"].is_empty());
    }

    #[test]
    fn see_the_code_of_a_step_keeps_its_parts_files_before_the_cap_of_80() {
        let o = origin();
        let map = r#"{"version":1,"parts":[
            {"id":"a","name":"A","caption":"","description":"","tier":"core","paths":["a/"]},
            {"id":"z","name":"Z","caption":"","description":"","tier":"core","paths":["z/"]},
            {"id":"zz","name":"ZZ","caption":"","description":"","tier":"core","paths":["z/own/"]}],
            "actors":[],"links":[],"ignored":["z/docs/"],"laws":[]}"#;
        commit_file(&o.seed, MAP_FILE, map);
        git(&o.seed, &["push", "-q", "origin", "HEAD"]);
        git(&o.main, &["fetch", "-q", "origin"]);
        git(&o.main, &["checkout", "-q", "-b", "work", "origin/trunk"]);
        // 85 files of part a sort before part z's, so the cap of 80 would cut z's away.
        for i in 0..85 {
            let dir = o.main.join("a");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("f{i:02}")), "x\n").unwrap();
        }
        for name in ["z/one", "z/two", "z/own/three", "z/docs/four"] {
            let path = o.main.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x\n").unwrap();
        }
        git(&o.main, &["add", "-A"]);
        git(&o.main, &["commit", "-q", "-m", "work"]);
        let head = git(&o.main, &["rev-parse", "HEAD"]);
        let diff = |parts: Option<&[&str]>| {
            let found = pr_diff(PrDiffArgs {
                project: o.main.display().to_string(),
                number: 7,
                head: head.clone(),
                title: "Work".into(),
                parts: parts.map(|p| p.iter().map(|s| s.to_string()).collect()),
                paths: None,
            })
            .unwrap();
            found.files.into_iter().map(|f| f.path).collect::<Vec<_>>()
        };
        // A later part's pattern wins its files, and an ignored file is no part's.
        assert_eq!(diff(Some(&["z"])), ["z/one", "z/two"]);
        assert_eq!(diff(Some(&["zz", "nobody"])), ["z/own/three"]);
        assert!(diff(Some(&[])).is_empty());
        assert_eq!(diff(None).len(), 80);
        // An Also touched group's files (#492), by path, before the cap too; a path the PR did not change is not shown.
        let found = pr_diff(PrDiffArgs {
            project: o.main.display().to_string(),
            number: 7,
            head: head.clone(),
            title: "Work".into(),
            parts: None,
            paths: Some(vec!["z/docs/four".into(), "z/one".into(), "nowhere".into()]),
        })
        .unwrap();
        let shown: Vec<_> = found.files.into_iter().map(|f| f.path).collect();
        assert_eq!(shown, ["z/docs/four", "z/one"]);
    }

    #[test]
    fn a_version_1_map_is_stored_and_sent_as_version_2() {
        let (dir, main) = repo();
        let old = r#"{"version":1,"parts":[{"id":"core","name":"Core","caption":"c","description":"Plain Rust.","tier":"core","paths":["core/"]}],"actors":[],"links":[],"ignored":[],"laws":[]}"#;
        commit_file(dir.path(), MAP_FILE, old);
        let state = read_map(Path::new(&main));
        assert_eq!(state.status, Status::Loaded);
        let sent: serde_json::Value = serde_json::from_str(&state.source.unwrap()).unwrap();
        assert_eq!(sent["version"], 2);
        assert_eq!(sent["parts"][0]["purpose"], "Plain Rust.");
        assert_eq!(sent["parts"][0]["holds"], serde_json::json!([]));
    }

    #[test]
    fn a_default_branch_without_the_file_is_the_empty_state() {
        let o = origin();
        let state = read_map(&o.main);
        assert_eq!(state.status, Status::None);
        assert_eq!(state.source, None);
    }

    #[test]
    fn an_unset_origin_head_falls_back_to_local_main_with_a_quiet_note() {
        let (dir, main) = repo();
        commit_file(dir.path(), MAP_FILE, MAP);
        let state = read_map(Path::new(&main));
        assert_eq!(state.status, Status::Loaded);
        assert_eq!(state.reference.as_deref(), Some("main"));
        assert!(state.note.is_some_and(|n| n.contains("origin/HEAD")));
    }

    #[test]
    fn local_master_is_read_when_there_is_no_main() {
        let (dir, main) = repo();
        git(dir.path(), &["branch", "-m", "master"]);
        commit_file(dir.path(), MAP_FILE, MAP);
        assert_eq!(default_ref(Path::new(&main)).unwrap().0, "master");
    }

    #[test]
    fn a_file_that_does_not_read_is_unreadable_with_its_problem() {
        let (dir, main) = repo();
        commit_file(dir.path(), MAP_FILE, "{ nope");
        let state = read_map(Path::new(&main));
        assert_eq!(state.status, Status::Unreadable);
        assert!(state.note.is_some());
    }

    #[test]
    fn a_map_with_fields_this_build_does_not_know_still_loads() {
        let (dir, main) = repo();
        let newer = r#"{"version":1,"future":true,"parts":[],"actors":[],"links":[],"ignored":[],"laws":[]}"#;
        commit_file(dir.path(), MAP_FILE, newer);
        assert_eq!(read_map(Path::new(&main)).status, Status::Loaded);
    }

    #[test]
    fn a_fetch_waits_out_its_cooldown_per_repository() {
        let fetches = Fetches::default();
        let start = Instant::now();
        let mut count = 0;
        let mut go = |repo: &str, at: Instant| {
            fetches.run(repo, at, || {
                count += 1;
                Ok(())
            })
        };
        assert_eq!(go("a", start), Fetched::Done);
        assert_eq!(go("a", start + Duration::from_secs(59)), Fetched::Skipped);
        assert_eq!(go("b", start + Duration::from_secs(59)), Fetched::Done);
        assert_eq!(go("a", start + COOLDOWN), Fetched::Done);
        assert_eq!(count, 3);
    }

    #[test]
    fn a_fetch_in_flight_blocks_another_and_a_failure_is_reported() {
        let fetches = Fetches::default();
        let start = Instant::now();
        let inner = std::cell::Cell::new(None);
        let outer = fetches.run("a", start, || {
            // A second trigger while the first runs, long past any cooldown.
            inner.set(Some(fetches.run(
                "a",
                start + Duration::from_secs(600),
                || Ok(()),
            )));
            Err("offline".into())
        });
        assert_eq!(inner.take(), Some(Fetched::Skipped));
        assert_eq!(outer, Fetched::Failed("offline".into()));
        // A failed fetch still counts toward the cooldown, so a dead network is not hammered.
        assert_eq!(
            fetches.run("a", start + Duration::from_secs(1), || Ok(())),
            Fetched::Skipped
        );
    }

    #[test]
    fn user_state_round_trips_and_keeps_other_projects() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("map-state.json");
        assert_eq!(read_user(&file, "/p"), UserState::default());
        update_user(&file, "/p", |u| {
            u.positions.insert("core".into(), [10.0, 20.5]);
            u.tiers.insert("hook".into(), "black-box".into());
        })
        .unwrap();
        update_user(&file, "/q", |u| {
            u.positions.insert("x".into(), [1.0, 2.0]);
        })
        .unwrap();
        let p = read_user(&file, "/p");
        assert_eq!(p.positions["core"], [10.0, 20.5]);
        assert_eq!(p.tiers["hook"], "black-box");
        assert_eq!(read_user(&file, "/q").positions.len(), 1);
    }

    #[test]
    fn a_projects_view_is_saved_beside_its_positions_and_read_back_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("map-state.json");
        update_user(&file, "/p", |u| {
            u.positions.insert("core".into(), [10.0, 20.0]);
        })
        .unwrap();
        let view = View {
            scale: 0.75,
            x: -120.5,
            y: 40.0,
        };
        save_view(&file, "/p", view).unwrap();
        // A restart keeps nothing in memory: the view is read back from the file alone.
        let read = read_user(&file, "/p");
        assert_eq!(read.view, Some(view));
        assert_eq!(read.positions["core"], [10.0, 20.0], "positions are kept");
        assert_eq!(
            read_user(&file, "/q").view,
            None,
            "another project has its own view"
        );
        let moved = View { scale: 2.0, ..view };
        save_view(&file, "/p", moved).unwrap();
        assert_eq!(read_user(&file, "/p").view, Some(moved));
        let bad = View {
            scale: f64::NAN,
            ..view
        };
        assert!(save_view(&file, "/p", bad).is_err());
        assert!(save_view(&file, "/p", View { scale: 0.0, ..view }).is_err());
        assert_eq!(
            read_user(&file, "/p").view,
            Some(moved),
            "a refused view changes nothing"
        );
    }

    #[test]
    fn an_older_or_newer_state_file_is_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("map-state.json");
        // No tiers yet, and a field a later build wrote.
        std::fs::write(
            &file,
            r#"{"projects":{"/p":{"positions":{"core":[1,2]},"fog":{"core":[]}}},"version":9}"#,
        )
        .unwrap();
        assert_eq!(read_user(&file, "/p").positions["core"], [1.0, 2.0]);
        assert!(read_user(&file, "/p").tiers.is_empty());
        // A change keeps what it does not know.
        update_user(&file, "/p", |u| {
            u.tiers.insert("core".into(), "interface".into());
        })
        .unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            text.contains("\"fog\"") && text.contains("\"version\""),
            "{text}"
        );
    }

    #[test]
    fn only_a_lookup_that_turns_a_task_merged_is_a_transition() {
        let facts = |merged| {
            Some(studio_core::TaskFacts {
                merged,
                ..Default::default()
            })
        };
        assert!(became_merged(facts(false), facts(true)));
        // First seen merged (a restart), still merged, or not merged: no fetch.
        assert!(!became_merged(None, facts(true)));
        assert!(!became_merged(facts(true), facts(true)));
        assert!(!became_merged(facts(false), facts(false)));
        assert!(!became_merged(facts(false), None));
    }

    #[test]
    fn a_panicking_fetch_does_not_leave_the_repository_locked() {
        let fetches = Fetches::default();
        let now = Instant::now();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fetches.run("/p", now, || panic!("git panicked"))
        }));
        assert!(panicked.is_err());
        assert_eq!(fetches.run("/p", now + COOLDOWN, || Ok(())), Fetched::Done);
    }

    #[test]
    fn a_file_that_does_not_parse_is_an_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("map-state.json");
        std::fs::write(&file, "{ broken").unwrap();
        assert_eq!(read_user(&file, "/p"), UserState::default());
    }
}
