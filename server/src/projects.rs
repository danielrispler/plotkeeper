//! The saved project list (spec danielrispler/plotkeeper#380, "Projects"; ticket #381): the repositories plotkeeper keeps, in
//! `projects.json` in the data folder. A repository joins the first time a session is seen in it, as its main checkout (a
//! task's worktree `<main>/.claude/worktrees/<task>` belongs to `<main>`, the rule the page's `repositoryOf` uses). It stays
//! after its sessions end or are deleted, and leaves only through Remove project. A removed project comes back only for a
//! session active after its removal, so the ended sessions still listed never bring it back.
//!
//! A saved folder that no longer exists is listed as gone, never as an error, and Remove project works on it.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use studio_core::Session;

/// The list's file in the data folder.
const FILE: &str = "projects.json";

/// A saved project, as the page reads it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Project {
    /// The repository's main checkout.
    pub path: String,
    /// Its folder was moved or deleted.
    pub gone: bool,
}

/// What `projects.json` holds: the projects in the order they joined, and when each removed one was removed, in ms.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    projects: Vec<String>,
    #[serde(default)]
    removed: BTreeMap<String, u64>,
}

/// The main checkout a session's folder belongs to: a task's worktree's main checkout, else the folder itself.
pub fn repository_of(cwd: &str) -> &str {
    match cwd.rfind("/.claude/worktrees/") {
        Some(at) if at > 0 && cwd.len() > at + "/.claude/worktrees/".len() => &cwd[..at],
        _ => cwd,
    }
}

/// The saved list, read from its file once and kept in memory.
pub struct Projects {
    file: PathBuf,
    saved: Mutex<Option<Saved>>,
}

impl Projects {
    /// The list kept in `file`.
    pub fn at(file: PathBuf) -> Self {
        Projects {
            file,
            saved: Mutex::new(None),
        }
    }

    /// The list in the data folder that holds the event log `log`.
    pub fn beside(log: &Path) -> Self {
        Self::at(log.with_file_name(FILE))
    }

    /// Adds the repository of each session not yet saved; returns whether the list changed. A removed project is added
    /// again only for a session whose latest event came after its removal.
    pub fn see(&self, sessions: &[&Session]) -> Result<bool, String> {
        let mut held = self.saved.lock().unwrap();
        let saved = held.get_or_insert_with(|| read(&self.file));
        let mut changed = false;
        for session in sessions {
            if session.cwd.is_empty() {
                continue;
            }
            let repo = repository_of(&session.cwd);
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

    /// The saved projects, in the order they joined, each marked gone when its folder is not there.
    pub fn list(&self) -> Vec<Project> {
        let mut held = self.saved.lock().unwrap();
        let saved = held.get_or_insert_with(|| read(&self.file));
        (saved.projects.iter())
            .map(|path| Project {
                path: path.clone(),
                gone: !Path::new(path).is_dir(),
            })
            .collect()
    }

    /// Remove project: takes `path` off the list at `at` ms; returns the list as it is now.
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

/// The list in `file`; a missing or unreadable file is an empty list.
fn read(file: &Path) -> Saved {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the list by a temporary file and a rename.
fn write(file: &Path, saved: &Saved) -> Result<(), String> {
    let dir = file.parent().ok_or("no folder for projects.json")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, saved).map_err(|e| e.to_string())?;
    temp.persist(file).map_err(|e| e.to_string())?;
    Ok(())
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
            }]
        );
        assert_eq!(projects.remove(&repo, T + 1).unwrap(), []);
    }

    #[test]
    fn a_worktree_belongs_to_its_main_checkout_and_any_other_folder_to_itself() {
        assert_eq!(repository_of("/r/p/.claude/worktrees/x/ui"), "/r/p");
        assert_eq!(repository_of("/r/p"), "/r/p");
        assert_eq!(repository_of("/"), "/");
    }
}
