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
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

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
    match system_map::parse_lenient(&text) {
        Ok(_) => {
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

fn state_file() -> Result<PathBuf, String> {
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
#[tauri::command(async)]
pub fn map_state(
    project: String,
    fetch: bool,
    fetches: State<Fetches>,
    cache: State<crate::ci_laws::CiCache>,
    prs: State<crate::prs::Prs>,
    app: AppHandle,
) -> Result<MapState, String> {
    if fetch {
        crate::prs::ask_later(&app, crate::prs::Trigger::MapOpen);
        let fetched = self::fetch(&fetches, &project);
        // A fetch inside the cooldown asks nothing again, unless nothing was read yet.
        if fetched != Fetched::Skipped || !cache.has(&project) {
            crate::ci_laws::refresh_later(&app, &project);
        }
    }
    let repo = Path::new(&project);
    let file = state_file()?;
    // A `fetch` is an open or a focus: the first of them starts the project's fog clear. A quiet read starts nothing.
    if let Err(why) = crate::fog::refresh(repo, &file, &project, fetch) {
        log::warn!("map: fog of {project} failed: {why}");
    }
    let mut state = read_map(repo);
    state.user = read_user(&file, &project);
    if let Some(source) = &state.source {
        state.fog = crate::fog::effective(&state.user, source);
    }
    state.ci = cache.get(&project);
    state.ready = prs.ready(&project);
    Ok(state)
}

/// "I know this": clears `part`'s fog.
#[tauri::command(async)]
pub fn map_know(project: String, part: String) -> Result<UserState, String> {
    crate::fog::clear(&state_file()?, &project, &part)
}

/// A part's history strip: its first-parent commits on the default branch from the map's birth, oldest to newest, the ones Daniel
/// has not caught up on marked.
#[tauri::command(async)]
pub fn map_history(project: String, part: String) -> Result<Vec<crate::history::Entry>, String> {
    let fogged = read_user(&state_file()?, &project)
        .fog
        .parts
        .get(&part)
        .map(|entries| entries.iter().map(|e| e.commit.clone()).collect())
        .unwrap_or_default();
    Ok(crate::history::part_history(
        Path::new(&project),
        &part,
        &fogged,
    ))
}

/// The map as of `commit`: what one scrub of the history strip asks the shell for. A later ticket that records scrubs hooks here.
#[tauri::command(async)]
pub fn map_at(project: String, commit: String) -> Result<crate::history::At, String> {
    crate::history::at(Path::new(&project), &commit)
}

/// The files and diffs of `commit`, for the drawer.
#[tauri::command(async)]
pub fn map_commit(project: String, commit: String) -> Result<crate::history::Commit, String> {
    crate::history::commit_files(Path::new(&project), &commit)
}

/// The projects among `projects` that show a fogged Core part or a broken law, for Home's quiet mark. Reads local refs and the
/// last CI read only: it asks neither `origin` nor GitHub.
#[tauri::command(async)]
pub fn map_marks(
    projects: Vec<String>,
    cache: State<crate::ci_laws::CiCache>,
) -> Result<Vec<String>, String> {
    let file = state_file()?;
    Ok(projects
        .into_iter()
        .filter(|project| {
            if cache.broken(project) {
                return true;
            }
            let repo = Path::new(project);
            let _ = crate::fog::refresh(repo, &file, project, false);
            read_map(repo)
                .source
                .is_some_and(|source| crate::fog::core_fogged(&read_user(&file, project), &source))
        })
        .collect())
}

/// "See the proof": the tests that back `law` on the default branch, with the code around their `@law:` tag.
#[tauri::command(async)]
pub fn map_proof(project: String, law: String) -> Vec<crate::ci_laws::Proof> {
    crate::ci_laws::proofs(Path::new(&project), &law)
}

/// Saves the positions of parts or actors, by id; a position that is not a finite number is refused.
#[tauri::command(async)]
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

/// Sets part `part`'s tier, `null` clearing Daniel's choice so the map's own tier applies again.
#[tauri::command(async)]
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
#[tauri::command(async)]
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
pub fn merged(app: &AppHandle, task_path: &str) {
    let Ok(project) = studio_workbench::main_checkout(Path::new(task_path)) else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        fetch(&app.state::<Fetches>(), &project);
        crate::ci_laws::refresh_later(&app, &project);
        let _ = app.emit("map-landed", project);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_test_support::{git, origin, repo};

    const MAP: &str = r#"{"version":1,"parts":[],"actors":[],"links":[],"ignored":[],"laws":[]}"#;

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
