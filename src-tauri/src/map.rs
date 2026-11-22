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
    collections::{BTreeMap, HashMap, HashSet},
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
}

/// Daniel's choices for one project, kept in the data folder: a saved `(x, y)` per part or actor id, and a tier per part id
/// that beats the map's own. Every field may be missing in an older file.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserState {
    #[serde(default)]
    pub positions: BTreeMap<String, [f64; 2]>,
    #[serde(default)]
    pub tiers: BTreeMap<String, String>,
}

fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
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

impl Fetches {
    /// Runs `fetch` for `repo` unless one is running or the last began within `COOLDOWN` of `now`.
    pub fn run(
        &self,
        repo: &str,
        now: Instant,
        fetch: impl FnOnce() -> Result<(), String>,
    ) -> Fetched {
        {
            let mut slots = self.0.lock().unwrap();
            let slot = slots.entry(repo.to_string()).or_default();
            let cooling = slot
                .last
                .is_some_and(|at| now.saturating_duration_since(at) < COOLDOWN);
            if slot.running || cooling {
                return Fetched::Skipped;
            }
            slot.running = true;
            slot.last = Some(now);
        }
        // The lock is not held while git runs, so a second trigger meets `running`.
        let result = fetch();
        self.0
            .lock()
            .unwrap()
            .entry(repo.to_string())
            .or_default()
            .running = false;
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
/// focus while it is open, never on a timer; the cooldown and the in-flight lock decide whether it runs.
#[tauri::command(async)]
pub fn map_state(
    project: String,
    fetch: bool,
    fetches: State<Fetches>,
) -> Result<MapState, String> {
    if fetch {
        self::fetch(&fetches, &project);
    }
    let mut state = read_map(Path::new(&project));
    state.user = read_user(&state_file()?, &project);
    Ok(state)
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

/// The tasks seen merged, so a transition fetches once.
static MERGED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// `refresh_landed` found the task in `task_path` merged: on the first time, fetch its repository through the cooldown and
/// tell the window, which draws the map again if it is open.
pub fn merged(app: &AppHandle, task_path: &str) {
    if !MERGED
        .lock()
        .unwrap()
        .get_or_insert_default()
        .insert(task_path.to_string())
    {
        return;
    }
    let Ok(project) = studio_workbench::main_checkout(Path::new(task_path)) else {
        return;
    };
    fetch(&app.state::<Fetches>(), &project);
    let _ = app.emit("map-landed", project);
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
    fn a_file_that_does_not_parse_is_an_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("map-state.json");
        std::fs::write(&file, "{ broken").unwrap();
        assert_eq!(read_user(&file, "/p"), UserState::default());
    }
}
