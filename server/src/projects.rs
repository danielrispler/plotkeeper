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
mod tests;
