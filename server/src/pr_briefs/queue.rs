//! The briefs being written: the writer, the claims on each PR and the start's picks.

use super::{BriefInfo, BriefView, Seen, chapter_titles, store_of};
use crate::prs::Ready;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use studio_understanding::pr_brief::{BriefStore, BriefWriter};

/// What opening a brief answers: the brief, or that it is being written (it comes through `map-landed`).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Opened {
    Brief(Box<BriefView>),
    Writing { writing: bool },
}

type SharedWriter = Arc<dyn BriefWriter + Send + Sync>;

/// The shell's PR briefs: the writer, the "PR briefs" model, and what is being written.
pub struct PrBriefs {
    /// `None` when the writer is the fake of the checks, which writes no brief.
    pub(super) writer: Option<SharedWriter>,
    seen: Mutex<HashMap<String, Arc<Mutex<Seen>>>>,
    busy: Arc<Mutex<HashSet<(String, u64)>>>,
    /// Which ready PRs the first lookup after start writes, and which wait to be opened.
    pub(super) startup: Mutex<Startup>,
}

/// At most this many ready PRs get their brief written at start (danielrispler/plotkeeper#334, port decision Q5): the newest head first,
/// among those whose head moved within `briefBackfill.hours`.
pub const START_BRIEFS: usize = 3;

/// What the lookups write at once, given that plotkeeper has just started: the ready PRs found at the first lookup of a project that
/// `gh pr list` answered, and with no brief of their head, are written only when their head moved within the catch-up window, newest
/// head first and no more than `START_BRIEFS`. That cap counts the rewrites of stale briefs too. The others are written (or
/// rewritten) when opened, and are looked at as usual after that. A PR that becomes ready later is written at once.
#[derive(Default)]
pub struct Startup {
    asked: HashSet<String>,
    /// Ready PRs the first lookup left for Daniel to open, per project.
    waiting: HashSet<(String, u64)>,
}

impl Startup {
    /// The numbers of `ready` (of `project`) this lookup may write. `answered`: `gh pr list` has answered for the project, so
    /// `ready` is what is really open. `head_ms` is when a PR's head commit was made, if known; `fresh` whether a brief of the
    /// PR's head is stored and not stale.
    #[allow(
        clippy::too_many_arguments,
        reason = "no type groups these inputs, and one made only for this call would be a new abstraction (#555)"
    )]
    pub fn allowed(
        &mut self,
        project: &str,
        answered: bool,
        ready: &[Ready],
        head_ms: &dyn Fn(&Ready) -> Option<u64>,
        now_ms: u64,
        window_ms: u64,
        fresh: &dyn Fn(&Ready) -> bool,
    ) -> Vec<u64> {
        // A PR that left the ready list and returns became ready while running: it is written at once, not left for opening. One
        // whose brief was written when opened is looked at as usual from then on.
        self.waiting
            .retain(|(p, n)| p != project || ready.iter().any(|r| r.number == *n && !fresh(r)));
        // Until `gh pr list` has answered, what is open is not known: the start waits for the first lookup that answered.
        if !answered && !self.asked.contains(project) {
            return vec![];
        }
        if self.asked.insert(project.to_string()) {
            let since = now_ms.saturating_sub(window_ms);
            let mut moved: Vec<(&Ready, u64)> = ready
                .iter()
                .filter(|r| !fresh(r))
                .filter_map(|r| Some((r, head_ms(r)?)))
                .filter(|(_, at)| *at >= since)
                .collect();
            // The newest head first; of two made together, the newer PR.
            moved.sort_by_key(|(r, at)| std::cmp::Reverse((*at, r.number)));
            let picked: Vec<u64> = moved
                .iter()
                .take(START_BRIEFS)
                .map(|(r, _)| r.number)
                .collect();
            for r in ready {
                if !fresh(r) && !picked.contains(&r.number) {
                    self.waiting.insert((project.to_string(), r.number));
                }
            }
        }
        ready
            .iter()
            .filter(|r| fresh(r) || !self.waiting.contains(&(project.to_string(), r.number)))
            .map(|r| r.number)
            .collect()
    }
}

impl PrBriefs {
    pub fn new(writer: Option<SharedWriter>) -> Self {
        Self {
            writer,
            seen: Mutex::default(),
            busy: Arc::default(),
            startup: Mutex::default(),
        }
    }

    /// Whether briefs are written: not without the `claude` program, or with the fake writer of the checks.
    pub fn writes(&self) -> bool {
        self.writer.is_some()
    }

    /// The writer, which has no tools; part catch-ups (#391) and pre-flight (`pre_flight.rs`) ask it too.
    pub fn writer(&self) -> Option<SharedWriter> {
        self.writer.clone()
    }

    /// The model of the `prBriefs.model` setting, read from the file now.
    pub fn model(&self) -> String {
        crate::settings::pr_briefs_model()
    }

    /// Gives each ready PR its brief's state, for the tray.
    pub fn annotate(&self, project: &str, ready: &mut [Ready]) {
        let Some(store) = store_of(project) else {
            return;
        };
        let user = crate::map::state_file()
            .ok()
            .map(|file| crate::map::read_user(&file, project));
        self.annotate_with(project, &store, user.as_ref(), ready);
    }

    /// `annotate` with the store and Daniel's state in hand. It never waits on a write: `busy` and `seen` are held only to copy.
    pub(super) fn annotate_with(
        &self,
        project: &str,
        store: &BriefStore,
        user: Option<&crate::map::UserState>,
        ready: &mut [Ready],
    ) {
        let busy = self.busy.lock().unwrap().clone();
        let seen = self.seen_of(project);
        let (quiet, failed) = {
            let seen = seen.lock().unwrap();
            (seen.quiet.clone(), seen.failed.clone())
        };
        for pr in ready {
            let latest = store.latest(pr.number);
            let writing = busy.contains(&(project.to_string(), pr.number));
            let failed = !writing && failed.get(&pr.number) == Some(&pr.head);
            if latest.is_none() && !writing && !failed {
                if quiet.contains(&(pr.number, pr.head.clone())) {
                    pr.brief = Some(BriefInfo {
                        stale: false,
                        writing: false,
                        chapters: vec![],
                        quiet: true,
                        read: false,
                        failed: false,
                        note: None,
                        since: None,
                        since_chapters: vec![],
                    });
                }
                continue;
            }
            let chapters = latest
                .as_ref()
                .map(|b| chapter_titles(&b.brief))
                .unwrap_or_default();
            let story = store.since(pr.number, &pr.head).map(|(_, story)| story);
            pr.brief = Some(BriefInfo {
                stale: latest
                    .as_ref()
                    .is_some_and(|b| b.stale || b.head != pr.head),
                writing,
                chapters,
                read: user.is_some_and(|u| crate::fog::fully_read(u, pr.number)),
                quiet: false,
                failed,
                note: seen.lock().unwrap().note(pr.number, &pr.head),
                since: story.as_ref().map(|story| story.brief.title.clone()),
                since_chapters: story
                    .as_ref()
                    .filter(|story| story.brief.chapters.len() > 1)
                    .map(|story| chapter_titles(&story.brief))
                    .unwrap_or_default(),
            });
        }
    }

    /// Claims `pr` of `project` as being written: `None` when it already is. The claim ends when the guard drops, a panic of the
    /// write included, so a dead write never leaves the PR busy.
    pub(super) fn claim(&self, project: &str, pr: u64) -> Option<BusyGuard> {
        self.busy(project, pr, true).then(|| BusyGuard {
            busy: self.busy.clone(),
            key: (project.to_string(), pr),
        })
    }

    /// Opening PR `pr` of `project` whose brief must be written (port decision Q5): `write` runs on a thread of its own with the
    /// PR's claim, which it drops when done, so the open returns at once and the tray says "writing" meanwhile; the page gets the
    /// brief through `map-landed`. A PR being written already is left to that write. Either way the open is told it is being written.
    pub(super) fn open_in_background(
        &self,
        project: &str,
        pr: u64,
        write: impl FnOnce(BusyGuard) + Send + 'static,
    ) -> Opened {
        if let Some(claim) = self.claim(project, pr) {
            std::thread::spawn(move || write(claim));
        }
        Opened::Writing { writing: true }
    }

    /// Marks `pr` of `project` as being written (`on`) or done; false when it already was in that state.
    pub(super) fn busy(&self, project: &str, pr: u64, on: bool) -> bool {
        let key = (project.to_string(), pr);
        let mut busy = self.busy.lock().unwrap();
        if on {
            busy.insert(key)
        } else {
            busy.remove(&key)
        }
    }

    pub(super) fn seen_of(&self, project: &str) -> Arc<Mutex<Seen>> {
        self.seen
            .lock()
            .unwrap()
            .entry(project.to_string())
            .or_default()
            .clone()
    }
}

/// A PR claimed as being written; see `PrBriefs::claim`.
pub(super) struct BusyGuard {
    busy: Arc<Mutex<HashSet<(String, u64)>>>,
    key: (String, u64),
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.busy.lock().unwrap().remove(&self.key);
    }
}
