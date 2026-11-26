//! The saved project list (spec danielrispler/plotkeeper#380, "Projects"; ticket #381): the repositories plotkeeper keeps, in
//! `projects.json` in the data folder. A repository joins the first time a session is seen in it, as the session's project
//! folder (spec #659: the root of the folder it started in, a task's worktree's main checkout, set once by the core). It stays
//! after its sessions end or are deleted, and leaves only through Remove project. A removed project comes back only for a
//! session active after its removal, so the ended sessions still listed never bring it back.
//!
//! A saved folder that no longer exists is listed as gone, never as an error, and Remove project works on it.
//!
//! A folder under one of the system's temporary folders is temporary (spec #659): it is never saved, and one an earlier build saved is
//! listed as temporary, as a gone one is listed as gone, and stays in the file, so it too leaves only through Remove project. The page
//! hides both while they have no live session.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use studio_core::Session;

/// The list's file in the data folder.
const FILE: &str = "projects.json";

/// The folders under which a folder is temporary: `/tmp` and macOS's per-user temporary folders under `/var/folders`, each also by
/// the `/private` path macOS resolves it to.
const TEMPORARY: [&str; 4] = [
    "/tmp",
    "/private/tmp",
    "/var/folders",
    "/private/var/folders",
];

/// A saved project, as the page reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// The repository's main checkout.
    pub path: String,
    /// Its folder was moved or deleted.
    pub gone: bool,
    /// Its folder is under one of the system's temporary folders.
    pub temporary: bool,
}

/// What `projects.json` holds: the projects in the order they joined, and when each removed one was removed, in ms.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    removed: BTreeMap<String, u64>,
}

/// The saved list, read from its file once and kept in memory.
pub struct Projects {
    file: PathBuf,
    saved: Mutex<Option<Saved>>,
    /// The folders under which a folder is temporary.
    temporary: Vec<PathBuf>,
}

impl Projects {
    /// The list kept in `file`, with no temporary folders, as the tests' folders are under the system's.
    pub fn at(file: PathBuf) -> Self {
        Projects {
            file,
            saved: Mutex::new(None),
            temporary: Vec::new(),
        }
    }

    /// The list in the data folder that holds the event log `log`, with the system's temporary folders.
    pub fn beside(log: &Path) -> Self {
        Projects {
            temporary: TEMPORARY.iter().map(PathBuf::from).collect(),
            ..Self::at(log.with_file_name(FILE))
        }
    }

    /// Whether `path` is under one of the temporary folders.
    fn temporary(&self, path: &str) -> bool {
        self.temporary
            .iter()
            .any(|folder| Path::new(path).starts_with(folder))
    }

    /// Adds the project folder of each session not yet saved, unless it is temporary; returns whether the list changed. A removed
    /// project is added again only for a session whose latest event came after its removal.
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the list is changed and written under one lock, so two changes never lose one"
    )]
    pub fn see(&self, sessions: &[&Session]) -> Result<bool, String> {
        let mut held = self.saved.lock().unwrap();
        let saved = held.get_or_insert_with(|| read(&self.file));
        let mut changed = false;
        for session in sessions {
            let repo = session.project.as_str();
            if repo.is_empty() || self.temporary(repo) {
                continue;
            }
            if saved.projects.iter().any(|p| p == repo) {
                continue;
            }
            if saved
                .removed
                .get(repo)
                .is_some_and(|&at| session.agent.last_event_at <= at)
            {
                continue;
            }
            saved.removed.remove(repo);
            saved.projects.push(repo.to_string());
            changed = true;
        }
        if changed {
            write(&self.file, saved)?;
        }
        Ok(changed)
    }

    /// The saved projects, in the order they joined, each marked gone when its folder is not there and temporary when it is under a
    /// temporary folder.
    pub fn list(&self) -> Vec<Project> {
        let mut held = self.saved.lock().unwrap();
        let paths = held
            .get_or_insert_with(|| read(&self.file))
            .projects
            .clone();
        drop(held);
        paths
            .into_iter()
            .map(|path| Project {
                gone: !Path::new(&path).is_dir(),
                temporary: self.temporary(&path),
                path,
            })
            .collect()
    }

    /// Remove project: takes `path` off the list at `at` ms; returns the list as it is now.
    #[allow(
        clippy::significant_drop_tightening,
        reason = "the list is changed and written under one lock, so two changes never lose one"
    )]
    pub fn remove(&self, path: &str, at: u64) -> Result<Vec<Project>, String> {
        {
            let mut held = self.saved.lock().unwrap();
            let saved = held.get_or_insert_with(|| read(&self.file));
            saved.projects.retain(|p| p != path);
            saved.removed.insert(path.to_string(), at);
            write(&self.file, saved)?;
        }
        Ok(self.list())
    }
}

/// What `projects.json` may hold: the list as this build writes it, or the plain array of paths earlier builds wrote
/// (`workbench::projects`, #93, now in `archive/workbench/`), read as the saved list.
#[derive(Deserialize)]
#[serde(untagged)]
enum Stored {
    Saved(Saved),
    Paths(Vec<String>),
}

/// The list in `file`; a missing or unreadable file is an empty list.
fn read(file: &Path) -> Saved {
    let stored = std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    match stored {
        Some(Stored::Saved(saved)) => saved,
        Some(Stored::Paths(projects)) => Saved {
            projects,
            removed: BTreeMap::new(),
        },
        None => Saved::default(),
    }
}

/// Writes the list by a temporary file and a rename.
fn write(file: &Path, saved: &Saved) -> Result<(), String> {
    Ok(crate::json_file::write_json(file, "projects.json", saved)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_core::Studio;

    fn line(session: &str, event: &str, cwd: &str, at: u64) -> String {
        serde_json::json!({
            "session_id": session, "hook_event_name": event, "cwd": cwd, "studio_received_ms": at,
        })
        .to_string()
    }

    fn see(projects: &Projects, studio: &Studio) {
        projects.see(&studio.sessions()).unwrap();
    }

    const T: u64 = 1_793_262_000_000;

    // @law: projects-leave-only-by-remove
    #[test]
    fn a_project_joins_on_its_first_session_stays_after_its_sessions_end_and_are_deleted_and_leaves_on_remove()
     {
        let data = tempfile::tempdir().unwrap();
        let repo = data.path().join("plotkeeper");
        std::fs::create_dir(&repo).unwrap();
        let repo = repo.display().to_string();
        let file = data.path().join(FILE);
        let projects = Projects::at(file.clone());
        assert_eq!(projects.list(), []);

        let worktree = format!("{repo}/.claude/worktrees/issue-1");
        let mut studio = Studio::replayed(&[line("s1", "SessionStart", &worktree, T)]);
        see(&projects, &studio);
        let listed = vec![Project {
            path: repo.clone(),
            gone: false,
            temporary: false,
        }];
        assert_eq!(projects.list(), listed);

        studio.replay(&line("s1", "SessionEnd", &worktree, T + 1));
        see(&projects, &studio);
        assert_eq!(projects.list(), listed);
        studio.replay(&line("s1", studio_core::DELETE, &worktree, T + 2));
        assert!(studio.sessions().is_empty());
        see(&projects, &studio);
        assert_eq!(projects.list(), listed);
        // It is in the data folder, so a restart reads it back.
        assert_eq!(Projects::at(file.clone()).list(), listed);

        assert_eq!(projects.remove(&repo, T + 3).unwrap(), []);
        see(&projects, &studio);
        assert_eq!(projects.list(), []);
        assert_eq!(Projects::at(file).list(), []);
    }

    #[test]
    fn a_removed_project_comes_back_only_for_a_session_active_after_its_removal() {
        let data = tempfile::tempdir().unwrap();
        let repo = data.path().display().to_string();
        let projects = Projects::at(data.path().join(FILE));
        let mut studio = Studio::replayed(&[
            line("s1", "SessionStart", &repo, T),
            line("s1", "SessionEnd", &repo, T + 1),
        ]);
        see(&projects, &studio);
        projects.remove(&repo, T + 2).unwrap();
        // The ended session is still listed in History, and older than the removal.
        see(&projects, &studio);
        assert_eq!(projects.list(), []);
        studio.replay(&line("s2", "SessionStart", &repo, T + 3));
        see(&projects, &studio);
        assert_eq!(projects.list().len(), 1);
    }

    #[test]
    fn a_saved_project_whose_folder_is_gone_is_listed_as_gone_and_removable() {
        let data = tempfile::tempdir().unwrap();
        let repo = data.path().join("pokedex");
        std::fs::create_dir(&repo).unwrap();
        let repo = repo.display().to_string();
        let projects = Projects::at(data.path().join(FILE));
        let studio = Studio::replayed(&[line("s1", "SessionStart", &repo, T)]);
        see(&projects, &studio);
        std::fs::remove_dir(&repo).unwrap();
        assert_eq!(
            projects.list(),
            [Project {
                path: repo.clone(),
                gone: true,
                temporary: false,
            }]
        );
        assert_eq!(projects.remove(&repo, T + 1).unwrap(), []);
    }

    #[test]
    fn a_projects_file_from_earlier_builds_a_list_of_paths_is_read_as_the_saved_list() {
        let data = tempfile::tempdir().unwrap();
        let file = data.path().join(FILE);
        let repo = data.path().display().to_string();
        std::fs::write(&file, format!("[{repo:?}, \"/w/falinks\"]")).unwrap();
        let projects = Projects::at(file.clone());
        let paths = |listed: Vec<Project>| listed.into_iter().map(|p| p.path).collect::<Vec<_>>();
        assert_eq!(paths(projects.list()), [repo.clone(), "/w/falinks".into()]);
        // A write keeps them, in the new format.
        let studio = Studio::replayed(&[line("s1", "SessionStart", "/w/pokedex", T)]);
        see(&projects, &studio);
        assert_eq!(
            paths(Projects::at(file).list()),
            [repo, "/w/falinks".into(), "/w/pokedex".into()]
        );
    }

    #[test]
    fn a_session_s_project_is_saved_not_the_folders_it_moves_to() {
        let data = tempfile::tempdir().unwrap();
        let repo = data.path().display().to_string();
        let projects = Projects::at(data.path().join(FILE));
        let studio = Studio::replayed(&[
            line("s1", "SessionStart", &repo, T),
            line("s1", "UserPromptSubmit", &format!("{repo}/ui"), T + 1),
        ]);
        see(&projects, &studio);
        let paths: Vec<String> = projects.list().into_iter().map(|p| p.path).collect();
        assert_eq!(paths, [repo]);
    }

    #[test]
    fn a_session_in_a_temporary_folder_is_not_saved() {
        let data = tempfile::tempdir().unwrap();
        let scratch = data.path().join("tmp/scratch").display().to_string();
        let repo = data.path().join("repo").display().to_string();
        let file = data.path().join(FILE);
        let projects = Projects {
            temporary: vec![data.path().join("tmp")],
            ..Projects::at(file.clone())
        };
        let studio = Studio::replayed(&[
            line("s1", "SessionStart", &scratch, T),
            line("s2", "SessionStart", &repo, T),
        ]);
        see(&projects, &studio);
        let paths: Vec<String> = Projects::at(file)
            .list()
            .into_iter()
            .map(|p| p.path)
            .collect();
        assert_eq!(paths, [repo]);
    }

    #[test]
    fn a_saved_temporary_project_is_reported_temporary_and_stays_in_the_file() {
        let data = tempfile::tempdir().unwrap();
        let trial = data.path().join("tmp/trial");
        std::fs::create_dir_all(&trial).unwrap();
        let trial = trial.display().to_string();
        let repo = data.path().display().to_string();
        let file = data.path().join(FILE);
        let text = serde_json::json!({ "projects": [trial, repo], "removed": {} }).to_string();
        std::fs::write(&file, &text).unwrap();
        let projects = Projects {
            temporary: vec![data.path().join("tmp")],
            ..Projects::at(file.clone())
        };
        let listed = [
            Project {
                path: trial.clone(),
                gone: false,
                temporary: true,
            },
            Project {
                path: repo.clone(),
                gone: false,
                temporary: false,
            },
        ];
        assert_eq!(projects.list(), listed);
        // A session in it changes nothing: the file is not rewritten, so it leaves only through Remove project.
        let studio = Studio::replayed(&[line("s1", "SessionStart", &trial, T)]);
        assert!(!projects.see(&studio.sessions()).unwrap());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), text);
        assert_eq!(projects.remove(&trial, T + 1).unwrap(), listed[1..]);
    }

    #[test]
    fn the_saved_list_in_the_data_folder_marks_the_system_s_temporary_folders() {
        let data = tempfile::tempdir().unwrap();
        let log = data.path().join("events.jsonl");
        std::fs::write(
            log.with_file_name(FILE),
            r#"["/private/tmp/tmp.x1", "/tmp/trial", "/var/folders/ab/T/scratch", "/private/var/folders/ab/T/repo", "/tmpfiles", "/w/plotkeeper"]"#,
        )
        .unwrap();
        let temporary: Vec<bool> = Projects::beside(&log)
            .list()
            .into_iter()
            .map(|p| p.temporary)
            .collect();
        assert_eq!(temporary, [true, true, true, true, false, false]);
    }
}
