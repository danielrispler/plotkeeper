//! The event log (#328, module 2): one append-only JSON Lines file, the only source of session state.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{self, Command},
};

/// The log in the studio's data directory, which is Tauri's app data directory for the identifier
/// in `src-tauri/tauri.conf.json`. The hook writes it; the app reads it.
pub fn event_log(home: &Path) -> PathBuf {
    home.join("Library/Application Support/dev.falinks.studio/events.jsonl")
}

/// What the hook knows of a session's latest turn, kept between its runs (#486): its prompt id, the
/// worktrees it recorded a start tree of, and whether it ended. It follows the core's turn rules
/// (#434, decisions 3 to 5).
#[derive(Debug, PartialEq)]
pub struct Turn {
    pub prompt_id: Option<String>,
    pub roots: Vec<String>,
    pub ended: bool,
}

impl Default for Turn {
    fn default() -> Self {
        Turn {
            prompt_id: None,
            roots: Vec::new(),
            ended: true,
        }
    }
}

impl Turn {
    /// Takes an event `name` of the session's own agent with `prompt_id`; `idle` when it is Claude
    /// Code's idle notice. Returns the worktrees whose end tree to record, those of the turn it
    /// ends, and whether it is in a turn whose first touches record a start tree.
    pub fn take(&mut self, name: &str, prompt_id: Option<&str>, idle: bool) -> (Vec<String>, bool) {
        let prompt = name == "UserPromptSubmit";
        let starts = !idle
            && prompt_id.is_some()
            && (self.prompt_id.as_deref() != prompt_id || prompt && self.ended);
        let keyed = self.prompt_id.is_some();
        let ends = !self.ended && (name == "Stop" || keyed && (idle || starts));
        let mut ended = Vec::new();
        if ends {
            ended = std::mem::take(&mut self.roots);
            self.ended = true;
        }
        if starts || prompt && self.ended {
            *self = Turn {
                prompt_id: prompt_id.map(Into::into),
                roots: Vec::new(),
                ended: name == "Stop",
            };
        }
        (ended, !self.ended && name != "Stop" && !idle)
    }
}

/// The root of the git worktree that holds `folder`, or its nearest existing parent, as `git`
/// spells it; `None` outside a worktree.
pub fn worktree_root(folder: &Path) -> Option<String> {
    let existing = folder.ancestors().find(|p| p.is_dir())?;
    git(existing, &["rev-parse", "--show-toplevel"], None)
}

/// The root of the git worktree that holds `folder`, or its nearest existing parent, and the git
/// tree of its files as they are now, untracked ones included and ignored ones not (#486). `None`
/// outside a worktree or when `git` fails. It writes the tree through a throwaway index, so the
/// working tree, the real index and `git status` stay as they were.
pub fn record_tree(folder: &Path) -> Option<(String, String)> {
    let root = worktree_root(folder)?;
    let index = git(
        Path::new(&root),
        &["rev-parse", "--git-path", "index"],
        None,
    )?;
    let scratch = env::temp_dir().join(format!("studio-hook-{}.index", process::id()));
    // A copy of the real index keeps git's stat cache, so only changed files are hashed again.
    let _ = fs::copy(Path::new(&root).join(index), &scratch);
    let added = git(Path::new(&root), &["add", "-A"], Some(&scratch));
    let tree = added.and_then(|_| git(Path::new(&root), &["write-tree"], Some(&scratch)));
    let _ = fs::remove_file(&scratch);
    Some((root, tree?))
}

/// `git`'s trimmed output in `folder`, with `index` as its index when given, or `None` when it
/// fails. It takes no optional locks, so it never holds up the agent's own `git` commands.
fn git(folder: &Path, args: &[&str], index: Option<&Path>) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(folder);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output().ok()?;
    let out = String::from_utf8(output.stdout).ok()?;
    output.status.success().then(|| out.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Turn;

    /// The hook's view of one session's turns, after `events`, each a name and a prompt id.
    fn after(turn: &mut Turn, name: &str, prompt: &str) -> (Vec<String>, bool) {
        turn.take(name, Some(prompt), false)
    }

    #[test]
    fn a_turn_records_its_starts_until_its_stop_ends_every_worktree_it_touched() {
        let mut turn = Turn::default();
        assert_eq!(after(&mut turn, "UserPromptSubmit", "p1"), (vec![], true));
        turn.roots.push("/w/a".into());
        turn.roots.push("/w/b".into());
        // A prompt queued with the running turn's id keeps the turn.
        assert_eq!(after(&mut turn, "UserPromptSubmit", "p1"), (vec![], true));
        let ended = vec!["/w/a".to_owned(), "/w/b".to_owned()];
        assert_eq!(after(&mut turn, "Stop", "p1"), (ended, false));
        // A late step of the ended turn records nothing.
        assert_eq!(after(&mut turn, "PostToolUse", "p1"), (vec![], false));
        assert_eq!(after(&mut turn, "UserPromptSubmit", "p1"), (vec![], true));
        assert!(turn.roots.is_empty());
    }

    /// Esc fires no `Stop`: the next turn's first event ends it, as the core sees it.
    #[test]
    fn the_next_turn_or_an_idle_notice_ends_a_turn_without_a_stop() {
        let mut turn = Turn::default();
        after(&mut turn, "UserPromptSubmit", "p1");
        turn.roots.push("/w/a".into());
        assert_eq!(
            after(&mut turn, "PreToolUse", "p2"),
            (vec!["/w/a".into()], true)
        );
        turn.roots.push("/w/a".into());
        assert_eq!(
            turn.take("Notification", Some("p2"), true),
            (vec!["/w/a".into()], false)
        );
    }

    #[test]
    fn the_log_lives_in_the_data_directory_of_the_apps_identifier() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
        let identifier = conf["identifier"].as_str().unwrap();
        let data = Path::new("/h/Library/Application Support").join(identifier);
        assert_eq!(super::event_log(Path::new("/h")), data.join("events.jsonl"));
    }
}
