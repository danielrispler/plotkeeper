//! The PR briefs of ready PRs (spec danielrispler/plotkeeper#334, ticket #339). When a PR becomes ready the shell writes its
//! brief and stores it per PR and head; a new head marks it stale, and it is rewritten when opened or once two consecutive
//! lookups see the same head. The lookups are `prs.rs`'s (Landed's triggers, never a timer).
//!
//! `write_brief` is the one place the shell calls the understanding crate's `write_pr_brief`, so the claim check and the retry
//! of ticket #338 take its place here.

use crate::prs::Ready;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
};
use studio_understanding::pr_brief::{
    BriefStore, BriefWriter, PrSource, StoredBrief, build_facts, fetch_meta, gather, write_pr_brief,
};
use tauri::{AppHandle, Emitter, Manager, State};

/// What the tray row of a ready PR says of its brief.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BriefInfo {
    /// A newer head was pushed; it is rewritten when opened.
    pub stale: bool,
    /// Being written now.
    pub writing: bool,
    /// The chapters' titles, one tray row each; one title for a single story.
    pub chapters: Vec<String>,
    /// Every chapter of the brief was read to its end (#342). Home's dot may ask this to leave a brief that is done; it does not yet.
    pub read: bool,
    /// The PR touches no part that would fog at Daniel's tiers, so it gets no brief.
    pub quiet: bool,
}

/// What a lookup of a ready PR did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Did {
    /// The brief of this head is stored already.
    Kept,
    /// A new head: the stored brief is marked stale, to be rewritten on open or at the next lookup of the same head.
    Staled,
    /// A brief was written.
    Wrote,
    /// The write failed (told to `log`).
    Failed,
    /// No part the PR touches would fog at Daniel's tiers, so it gets no brief.
    Quiet,
}

/// The heads the lookups saw last, per PR, for "the same head at two lookups in a row".
#[derive(Default)]
pub struct Seen {
    heads: HashMap<u64, String>,
    /// The PRs and heads found to touch no part that would fog at Daniel's tiers: they get no brief and are not gathered again.
    quiet: HashSet<(u64, String)>,
    /// The head whose write failed, per PR: it is not retried at every lookup, only when the head changes or Daniel opens the brief.
    failed: HashMap<u64, String>,
}

impl Seen {
    pub fn is_quiet(&self, pr: u64, head: &str) -> bool {
        self.quiet.contains(&(pr, head.to_string()))
    }

    fn has_failed(&self, pr: u64, head: &str) -> bool {
        self.failed.get(&pr).is_some_and(|h| h == head)
    }
}

/// What writes and keeps the briefs: the writer, the model of the "PR briefs" setting and the store.
pub struct Writing<'a> {
    pub writer: &'a dyn BriefWriter,
    pub model: &'a str,
    pub store: &'a BriefStore,
}

/// The one call into the understanding crate that writes a brief.
pub fn write_brief(
    with: &Writing,
    source: &PrSource,
    log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    write_pr_brief(with.writer, with.model, source, with.store, log)
}

/// Whether a PR touches no part whose fog would show at Daniel's tiers (the facts' required parts): such a PR gets no brief.
fn fogs_nothing(source: &PrSource) -> Result<bool, String> {
    Ok(build_facts(source)?.shape.required.is_empty())
}

/// Builds the source and writes; the failure of either is told to `log` and returned. `Ok(None)`: the PR would fog nothing.
fn write_now(
    with: &Writing,
    source: impl FnOnce() -> Result<PrSource, String>,
    mut log: impl FnMut(String),
) -> Result<Option<StoredBrief>, String> {
    let written = source().and_then(|source| {
        if fogs_nothing(&source)? {
            return Ok(None);
        }
        write_brief(with, &source, &mut log).map(Some)
    });
    if let Err(why) = &written {
        log(format!("write the PR brief: {why}"));
    }
    written
}

/// One lookup of ready PR `pr` at `head`. `source` builds what the writer reads, and is only called when a write starts. `seen` is
/// locked only around its bookkeeping, never across the write, which takes minutes.
pub fn on_lookup(
    with: &Writing,
    seen: &Mutex<Seen>,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    log: impl FnMut(String),
) -> Did {
    let (again, skip) = {
        let mut seen = seen.lock().unwrap();
        let before = seen.heads.insert(pr, head.to_string());
        let skip = seen.is_quiet(pr, head) || seen.has_failed(pr, head);
        (before.as_deref() == Some(head), skip)
    };
    if skip || with.store.get(pr, head).is_some() {
        return Did::Kept;
    }
    if !again && let Some(old) = with.store.latest(pr) {
        let _ = with.store.mark_stale(pr, &old.head);
        return Did::Staled;
    }
    // The first brief of a ready PR is written at once; a stale one when the same head is seen twice. The write takes minutes and
    // holds no lock.
    match write_now(with, source, log) {
        Ok(Some(_)) => Did::Wrote,
        Ok(None) => {
            seen.lock().unwrap().quiet.insert((pr, head.to_string()));
            Did::Quiet
        }
        Err(_) => {
            seen.lock().unwrap().failed.insert(pr, head.to_string());
            Did::Failed
        }
    }
}

/// Opening a PR's brief: the stored one when it is of the current head and not stale, else it is written now. Opening is Daniel
/// asking, so a write that failed at this head is tried again.
pub fn on_open(
    with: &Writing,
    seen: &Mutex<Seen>,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    match with.store.get(pr, head) {
        Some(stored) if !stored.stale => Ok(stored),
        _ => {
            seen.lock().unwrap().failed.remove(&pr);
            write_now(with, source, log)
                .inspect_err(|_| {
                    seen.lock().unwrap().failed.insert(pr, head.to_string());
                })?
                .ok_or_else(|| {
                    "Nothing you track changed in this PR any more, so it has no brief.".into()
                })
        }
    }
}

/// A stored brief as the window plays it.
#[derive(Debug, Clone, Serialize)]
pub struct BriefView {
    pub brief: StoredBrief,
    /// The writer did not see the whole diff (files cut, or some of their hunks).
    pub partial: bool,
}

type SharedWriter = Arc<dyn BriefWriter + Send + Sync>;

/// The shell's PR briefs: the writer, the "PR briefs" model, and what is being written.
pub struct PrBriefs {
    /// `None` when the writer is the fake of the checks, which writes no brief.
    writer: Option<SharedWriter>,
    /// The "PR briefs" setting's model (`pr_brief_settings`), the default until the window has sent it.
    model: Mutex<String>,
    seen: Mutex<HashMap<String, Arc<Mutex<Seen>>>>,
    busy: Mutex<HashSet<(String, u64)>>,
}

impl PrBriefs {
    pub fn new(writer: Option<SharedWriter>) -> Self {
        Self {
            writer,
            model: Mutex::new(studio_understanding::pr_brief::DEFAULT_MODEL.into()),
            seen: Mutex::default(),
            busy: Mutex::default(),
        }
    }

    /// The window read the `pr_briefs.model` setting.
    pub fn set_model(&self, model: String) {
        *self.model.lock().unwrap() = model;
    }

    pub fn model(&self) -> String {
        self.model.lock().unwrap().clone()
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
    fn annotate_with(
        &self,
        project: &str,
        store: &BriefStore,
        user: Option<&crate::map::UserState>,
        ready: &mut [Ready],
    ) {
        let busy = self.busy.lock().unwrap().clone();
        let quiet: HashSet<(u64, String)> = self.seen_of(project).lock().unwrap().quiet.clone();
        for pr in ready {
            let latest = store.latest(pr.number);
            let writing = busy.contains(&(project.to_string(), pr.number));
            if latest.is_none() && !writing {
                if quiet.contains(&(pr.number, pr.head.clone())) {
                    pr.brief = Some(BriefInfo {
                        stale: false,
                        writing: false,
                        chapters: vec![],
                        quiet: true,
                        read: false,
                    });
                }
                continue;
            }
            let chapters = latest
                .as_ref()
                .map(|b| chapter_titles(&b.brief))
                .unwrap_or_default();
            pr.brief = Some(BriefInfo {
                stale: latest
                    .as_ref()
                    .is_some_and(|b| b.stale || b.head != pr.head),
                writing,
                chapters,
                read: user.is_some_and(|u| crate::fog::fully_read(u, pr.number)),
                quiet: false,
            });
        }
    }

    /// Marks `pr` of `project` as being written (`on`) or done; false when it already was in that state.
    fn busy(&self, project: &str, pr: u64, on: bool) -> bool {
        let key = (project.to_string(), pr);
        let mut busy = self.busy.lock().unwrap();
        if on {
            busy.insert(key)
        } else {
            busy.remove(&key)
        }
    }

    fn seen_of(&self, project: &str) -> Arc<Mutex<Seen>> {
        self.seen
            .lock()
            .unwrap()
            .entry(project.to_string())
            .or_default()
            .clone()
    }
}

/// The numbers of the ready PRs a brief is waiting for: the brief is stored and Daniel has not read all of it at the PR's current
/// head. A brief read to its end stops counting for Home's dot and the rail's row (its tray row stays, to read again), and a push
/// after the reading counts it again.
pub fn waiting_numbers(
    store: &BriefStore,
    ready: &[Ready],
    user: Option<&crate::map::UserState>,
) -> Vec<u64> {
    ready
        .iter()
        .filter(|pr| store.latest(pr.number).is_some())
        .filter(|pr| !user.is_some_and(|u| crate::fog::fully_read_at(u, pr.number, &pr.head)))
        .map(|pr| pr.number)
        .collect()
}

fn user_of(project: &str) -> Option<crate::map::UserState> {
    crate::map::state_file()
        .ok()
        .map(|file| crate::map::read_user(&file, project))
}

/// Whether a brief waits for one of `project`'s ready PRs: Home's quiet dot lights for it.
pub fn any_waiting(project: &str, ready: &[Ready]) -> bool {
    store_of(project)
        .is_some_and(|store| !waiting_numbers(&store, ready, user_of(project).as_ref()).is_empty())
}

/// The ready PRs of the project holding `cwd` that a brief waits for, for the rail's row in the session of one of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Waiting {
    pub project: String,
    pub numbers: Vec<u64>,
}

#[tauri::command(async)]
pub fn briefs_waiting(cwd: String, prs: State<crate::prs::Prs>) -> Option<Waiting> {
    let project = studio_workbench::main_checkout(Path::new(&cwd)).ok()?;
    let ready = prs.ready(&project);
    let user = user_of(&project);
    let numbers = store_of(&project).map(|s| waiting_numbers(&s, &ready, user.as_ref()))?;
    Some(Waiting { project, numbers })
}

/// The parts each chapter of `brief` lights: the union of its steps' `on`, which is what the checks validate, not the chapter's
/// own `parts` (a single story names `["story"]`, and a chapter can light a part it did not declare).
pub fn lit_parts(brief: &studio_understanding::pr_brief::Brief) -> Vec<Vec<String>> {
    brief
        .chapters
        .iter()
        .map(|c| {
            let mut lit: Vec<String> = Vec::new();
            for part in c.steps.iter().flat_map(|s| &s.on) {
                if !lit.contains(part) {
                    lit.push(part.clone());
                }
            }
            lit
        })
        .collect()
}

/// The tray rows' titles: one per chapter, or the one story's title.
pub fn chapter_titles(brief: &studio_understanding::pr_brief::Brief) -> Vec<String> {
    let titles: Vec<String> = brief
        .chapters
        .iter()
        .map(|c| {
            if c.title.is_empty() {
                c.behaviour.clone()
            } else {
                c.title.clone()
            }
        })
        .collect();
    if titles.len() > 1 {
        titles
    } else {
        vec![brief.title.clone()]
    }
}

/// A project's briefs live in a folder of their own, since PR numbers repeat across repositories.
pub(crate) fn store_of(project: &str) -> Option<BriefStore> {
    let home = std::env::home_dir()?;
    let name: String = project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    BriefStore::open(
        &studio_hook::data_dir(&home)
            .join("pr-briefs-by-project")
            .join(name),
    )
    .ok()
}

/// What the writer reads for PR `number` at its head, gathered with `gh` and `git` in `project`.
fn source_of(project: &str, number: u64) -> Result<PrSource, String> {
    let repo = Path::new(project);
    let meta = fetch_meta(repo, number)?;
    let (reference, _) =
        crate::map::default_ref(repo).ok_or("the repository has no default branch")?;
    let base = crate::map::git(repo, &["merge-base", &reference, &meta.head])?;
    let head = meta.head.clone();
    let mut source = gather(repo, &base, &head, meta)?;
    if let Ok(file) = crate::map::state_file() {
        source.tier_overrides = crate::map::read_user(&file, project).tiers;
    }
    source.earlier_decisions = store_of(project)
        .and_then(|s| s.latest(number))
        .map(|b| b.brief.decisions)
        .unwrap_or_default();
    Ok(source)
}

/// Looks at each ready PR of `project` after a lookup of `prs.rs`: the first brief is written at once, a new head stales it. Each
/// PR goes on a thread of its own, since a write takes minutes; the Map view is told (`map-landed`) when a brief changed.
pub fn lookup_later(app: &AppHandle, project: &str, ready: Vec<Ready>) {
    for pr in ready {
        let (app, project) = (app.clone(), project.to_string());
        std::thread::spawn(move || {
            let briefs = app.state::<PrBriefs>();
            let Some(writer) = briefs.writer.clone() else {
                return;
            };
            let Some(store) = store_of(&project) else {
                return;
            };
            if !briefs.busy(&project, pr.number, true) {
                return;
            }
            let model = briefs.model();
            let seen = briefs.seen_of(&project);
            let with = Writing {
                writer: writer.as_ref(),
                model: &model,
                store: &store,
            };
            let did = on_lookup(
                &with,
                &seen,
                pr.number,
                &pr.head,
                || {
                    // A write starts: the tray says so.
                    let _ = app.emit("map-landed", project.clone());
                    source_of(&project, pr.number)
                },
                |line| log::warn!("pr brief of #{}: {line}", pr.number),
            );
            briefs.busy(&project, pr.number, false);
            if did != Did::Kept {
                let _ = app.emit("map-landed", project);
            }
        });
    }
}

/// Opening a PR's brief from the tray: the stored one, rewritten first when it is stale. A merged PR's brief is read as stored.
#[tauri::command(async)]
pub fn pr_brief(
    project: String,
    number: u64,
    prs: State<crate::prs::Prs>,
    briefs: State<PrBriefs>,
    app: AppHandle,
) -> Result<Option<BriefView>, String> {
    let store = store_of(&project).ok_or("no data folder")?;
    let view = |brief: StoredBrief| {
        let partial = store
            .records()
            .iter()
            .rev()
            .find(|r| r.pr == brief.pr && r.head == brief.head)
            .is_some_and(|r| r.budget.files_cut + r.budget.files_partial > 0);
        BriefView { brief, partial }
    };
    let head = prs
        .ready(&project)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| r.head);
    let Some(head) = head else {
        return Ok(store.latest(number).map(view));
    };
    if let Some(stored) = store.get(number, &head).filter(|b| !b.stale) {
        return Ok(Some(view(stored)));
    }
    let Some(writer) = briefs.writer.clone() else {
        return Ok(store.latest(number).map(view));
    };
    if !briefs.busy(&project, number, true) {
        return Err("The brief is being written; open it again in a moment.".into());
    }
    let _ = app.emit("map-landed", project.clone());
    let model = briefs.model();
    let seen = briefs.seen_of(&project);
    let with = Writing {
        writer: writer.as_ref(),
        model: &model,
        store: &store,
    };
    let written = on_open(
        &with,
        &seen,
        number,
        &head,
        || source_of(&project, number),
        |line| log::warn!("pr brief of #{number}: {line}"),
    );
    briefs.busy(&project, number, false);
    let _ = app.emit("map-landed", project);
    written.map(|b| Some(view(b)))
}

/// The window's `pr_briefs.model` setting.
#[tauri::command]
pub fn pr_brief_settings(model: String, briefs: State<PrBriefs>) {
    briefs.set_model(model);
}

/// Chapter `chapter` of PR `number`'s brief was read to its end (the window saw its last step and the check was answered): the
/// read is kept as "read at head", and the fog the PR's merged commits filed leaves every part whose chapters are all read.
#[tauri::command(async)]
pub fn brief_chapter_done(
    project: String,
    number: u64,
    chapter: usize,
    head: String,
    app: AppHandle,
) -> Result<(), String> {
    let stored = store_of(&project)
        .and_then(|store| store.get(number, &head))
        .ok_or("that brief is not stored")?;
    let lit = lit_parts(&stored.brief);
    let file = crate::map::state_file()?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let done = crate::fog::mark_read(&file, &project, number, chapter, &head, lit, at)?;
    crate::fog::clear_pr(Path::new(&project), &file, &project, number, &done)?;
    let _ = app.emit("map-landed", project);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use studio_understanding::pr_brief::{CallUsage, PrMeta, WriterReply};

    const MAP: &str = r#"{ "version": 1,
      "parts": [{ "id": "alpha", "name": "Alpha", "caption": "c", "description": "d", "tier": "core", "paths": ["alpha/"] }],
      "actors": [], "links": [], "ignored": [], "laws": [], "flows": [] }"#;

    fn source(head: &str) -> PrSource {
        PrSource {
            meta: PrMeta {
                number: 7,
                title: "Make the thing".into(),
                head: head.into(),
                ..PrMeta::default()
            },
            map_text: MAP.into(),
            diff: "diff --git a/alpha/a.rs b/alpha/a.rs\nindex 1..2 100644\n--- a/alpha/a.rs\n+++ b/alpha/a.rs\n@@ -1,1 +1,2 @@\n+one\n".into(),
            ..PrSource::default()
        }
    }

    struct Fake(Cell<usize>, RefCell<Vec<String>>);
    impl Fake {
        fn new() -> Self {
            Fake(Cell::new(0), RefCell::new(vec![]))
        }
    }
    impl BriefWriter for Fake {
        fn write(&self, model: &str, _prompt: &str) -> Result<WriterReply, String> {
            self.0.set(self.0.get() + 1);
            self.1.borrow_mut().push(model.into());
            Ok(WriterReply {
                text: r#"{"title":"Alpha","chapters":[{"behaviour":"story","parts":["alpha"],"title":"Order","steps":[{"k":"Where","h":"Alpha","b":"Alpha is here.","on":["alpha"]}]}]}"#.into(),
                usage: CallUsage::default(),
            })
        }
    }

    fn lookup(store: &BriefStore, seen: &Mutex<Seen>, fake: &Fake, head: &str) -> Did {
        let with = Writing {
            writer: fake,
            model: "opus",
            store,
        };
        on_lookup(&with, seen, 7, head, || Ok(source(head)), |_| {})
    }

    #[test]
    fn a_ready_pr_gets_a_stored_brief_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        assert_eq!(lookup(&store, &seen, &fake, "aaa"), Did::Wrote);
        let stored = store.get(7, "aaa").expect("the brief is stored");
        assert_eq!(stored.brief.title, "Alpha");
        assert_eq!(fake.1.borrow()[0], "opus");
        // Another lookup of the same head writes nothing.
        assert_eq!(lookup(&store, &seen, &fake, "aaa"), Did::Kept);
        assert_eq!(store.records().len(), 1);
    }

    #[test]
    fn a_new_head_marks_the_brief_stale_and_a_second_lookup_of_it_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        lookup(&store, &seen, &fake, "aaa");
        assert_eq!(lookup(&store, &seen, &fake, "bbb"), Did::Staled);
        assert!(store.get(7, "aaa").unwrap().stale);
        assert!(store.get(7, "bbb").is_none(), "not rewritten at once");
        // A burst of pushes: the head moved again, so the count starts over.
        assert_eq!(lookup(&store, &seen, &fake, "ccc"), Did::Staled);
        assert_eq!(store.records().len(), 1);
        // The same head at two lookups in a row: rewritten.
        assert_eq!(lookup(&store, &seen, &fake, "ccc"), Did::Wrote);
        assert!(store.get(7, "ccc").is_some());
        assert_eq!(store.records().len(), 2);
    }

    #[test]
    fn opening_a_stale_brief_rewrites_it_and_a_fresh_one_is_read_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        lookup(&store, &seen, &fake, "aaa");
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let open = |head: &str| on_open(&with, &seen, 7, head, || Ok(source(head)), |_| {});
        assert_eq!(open("aaa").unwrap().head, "aaa");
        assert_eq!(store.records().len(), 1, "a fresh brief is not rewritten");
        lookup(&store, &seen, &fake, "bbb");
        assert_eq!(open("bbb").unwrap().head, "bbb");
        assert_eq!(store.records().len(), 2);
        assert!(store.get(7, "bbb").is_some());
    }

    fn quiet_source(map: &str, tiers: &[(&str, &str)], map_diff: &str) -> PrSource {
        let mut source = source("aaa");
        source.map_text = map.into();
        source.map_diff = map_diff.into();
        for (part, tier) in tiers {
            source
                .tier_overrides
                .insert(part.to_string(), tier.to_string());
        }
        source
    }

    #[test]
    fn a_pr_whose_touched_parts_would_not_fog_at_the_tiers_gets_no_brief() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let black = MAP.replace("\"core\"", "\"black-box\"");
        let interface = MAP.replace("\"core\"", "\"interface\"");
        let cases = [
            // The map's own tier is black box.
            (quiet_source(&black, &[], ""), Did::Quiet),
            // Daniel's tier wins over the map's Core.
            (quiet_source(MAP, &[("alpha", "black-box")], ""), Did::Quiet),
            // An Interface part fogs only when the map's diff names it.
            (quiet_source(&interface, &[], ""), Did::Quiet),
            (quiet_source(&interface, &[], "\"alpha\""), Did::Wrote),
            (quiet_source(MAP, &[], ""), Did::Wrote),
        ];
        for (n, (source, want)) in cases.into_iter().enumerate() {
            let pr = 100 + n as u64;
            let mut source = source;
            source.meta.number = pr;
            let did = on_lookup(&with, &seen, pr, "aaa", || Ok(source), |_| {});
            assert_eq!(did, want, "case {n}");
            assert_eq!(store.latest(pr).is_some(), want == Did::Wrote, "case {n}");
        }
        assert_eq!(
            store.records().len(),
            2,
            "only the two that fog were written"
        );
    }

    #[test]
    fn a_quiet_pr_is_not_gathered_again_at_the_same_head() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let black = MAP.replace("\"core\"", "\"black-box\"");
        let gathered = Cell::new(0);
        let look = |head: &str| {
            let source = quiet_source(&black, &[], "");
            on_lookup(
                &with,
                &seen,
                7,
                head,
                || {
                    gathered.set(gathered.get() + 1);
                    Ok(source)
                },
                |_| {},
            )
        };
        assert_eq!(look("aaa"), Did::Quiet);
        assert_eq!(look("aaa"), Did::Kept);
        assert_eq!(gathered.get(), 1);
        assert_eq!(look("bbb"), Did::Quiet);
        assert_eq!(gathered.get(), 2);
    }

    fn ready(number: u64) -> Ready {
        Ready {
            number,
            title: String::new(),
            head: "aaa".into(),
            brief: None,
        }
    }

    #[test]
    fn a_brief_is_waiting_for_a_ready_pr_that_has_one_stored() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        on_lookup(&with, &seen, 7, "aaa", || Ok(source("aaa")), |_| {});
        // #8 has none yet; #7 does; the stale one still waits.
        assert_eq!(
            waiting_numbers(&store, &[ready(7), ready(8)], None),
            vec![7]
        );
        store.mark_stale(7, "aaa").unwrap();
        assert_eq!(waiting_numbers(&store, &[ready(7)], None), vec![7]);
        // A PR that is not ready (merged, closed, a draft) is not in the list given, so nothing waits for it.
        assert!(waiting_numbers(&store, &[], None).is_empty());
    }

    #[test]
    fn a_failed_write_is_told_and_stores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        let told = RefCell::new(vec![]);
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let did = on_lookup(
            &with,
            &seen,
            7,
            "aaa",
            || Err("no git".into()),
            |l| told.borrow_mut().push(l),
        );
        assert_eq!(did, Did::Failed);
        assert!(told.borrow()[0].contains("no git"));
        assert!(store.latest(7).is_none());
    }

    fn read_at(chapters: usize, head: &str) -> crate::map::UserState {
        let mut user = crate::map::UserState::default();
        let read = user.fog.reads.entry(7).or_default();
        read.lit = (0..chapters).map(|_| vec!["alpha".to_string()]).collect();
        for i in 0..chapters {
            read.chapters.insert(
                i,
                crate::fog::ChapterRead {
                    head: head.into(),
                    at: 1,
                },
            );
        }
        user
    }

    #[test]
    fn a_brief_read_to_its_end_stops_waiting_until_the_pr_is_pushed_again() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        lookup(&store, &seen, &fake, "aaa");
        // Unread, or read in part: waiting.
        assert_eq!(
            waiting_numbers(&store, &[ready(7)], Some(&crate::map::UserState::default())),
            vec![7]
        );
        let mut half = read_at(2, "aaa");
        half.fog.reads.get_mut(&7).unwrap().chapters.remove(&1);
        assert_eq!(waiting_numbers(&store, &[ready(7)], Some(&half)), vec![7]);
        // Read to the end at the PR's head: nothing waits.
        let done = read_at(2, "aaa");
        assert!(waiting_numbers(&store, &[ready(7)], Some(&done)).is_empty());
        // The PR moved on after the reading: waiting again.
        let mut pushed = ready(7);
        pushed.head = "bbb".into();
        assert_eq!(waiting_numbers(&store, &[pushed], Some(&done)), vec![7]);
    }

    /// A writer that tells when its write starts and blocks until it is released.
    struct Blocking {
        started: Mutex<std::sync::mpsc::Sender<()>>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl BriefWriter for Blocking {
        fn write(&self, _model: &str, _prompt: &str) -> Result<WriterReply, String> {
            self.started.lock().unwrap().send(()).unwrap();
            // A closed channel (the test is done) releases every write, the claim check's too.
            let _ = self.release.lock().unwrap().recv();
            Fake::new().write("opus", "")
        }
    }

    #[test]
    fn the_tray_is_annotated_while_a_brief_is_being_written() {
        use std::sync::mpsc::{RecvTimeoutError, channel};
        use std::time::Duration;
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let briefs = PrBriefs::new(None);
        let seen = briefs.seen_of("/p");
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let blocking = Blocking {
            started: Mutex::new(started_tx),
            release: Mutex::new(release_rx),
        };
        let (done_tx, done_rx) = channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert!(briefs.busy("/p", 7, true));
                let with = Writing {
                    writer: &blocking,
                    model: "opus",
                    store: &store,
                };
                on_lookup(&with, &seen, 7, "aaa", || Ok(source("aaa")), |_| {});
                briefs.busy("/p", 7, false);
            });
            started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            // The write is running: the Map view's read of the tray must not wait for it.
            scope.spawn(|| {
                let mut tray = [ready(7)];
                briefs.annotate_with("/p", &store, None, &mut tray);
                done_tx.send(tray[0].brief.clone()).unwrap();
            });
            let annotated = done_rx.recv_timeout(Duration::from_secs(3));
            drop(release_tx);
            match annotated {
                Ok(brief) => assert!(brief.unwrap().writing, "the row says it is being written"),
                Err(RecvTimeoutError::Timeout) => panic!("annotate waited for the write"),
                Err(e) => panic!("{e}"),
            }
        });
    }

    #[test]
    fn a_failed_write_is_not_retried_at_the_same_head_until_the_brief_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let sourced = Cell::new(0);
        let failing = || {
            sourced.set(sourced.get() + 1);
            Err("no git".to_string())
        };
        assert_eq!(
            on_lookup(&with, &seen, 7, "aaa", failing, |_| {}),
            Did::Failed
        );
        assert_eq!(
            on_lookup(&with, &seen, 7, "aaa", failing, |_| {}),
            Did::Kept
        );
        assert_eq!(sourced.get(), 1, "the second lookup did not try again");
        // A new head is tried.
        assert_eq!(
            on_lookup(&with, &seen, 7, "bbb", failing, |_| {}),
            Did::Failed
        );
        assert_eq!(sourced.get(), 2);
        // Opening the brief is Daniel asking: it tries, and a success is stored.
        assert!(on_open(&with, &seen, 7, "bbb", || Ok(source("bbb")), |_| {}).is_ok());
        assert!(store.get(7, "bbb").is_some());
    }

    #[test]
    fn the_parts_a_chapter_lights_are_its_steps_not_its_declared_parts() {
        let step = |on: &[&str]| studio_understanding::pr_brief::Step {
            on: on.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        let chapter = |parts: &[&str], steps: Vec<_>| studio_understanding::pr_brief::Chapter {
            parts: parts.iter().map(|s| s.to_string()).collect(),
            steps,
            ..Default::default()
        };
        let brief = studio_understanding::pr_brief::Brief {
            chapters: vec![
                // A single story declares "story" and lights real parts through its steps.
                chapter(&["story"], vec![step(&["alpha"]), step(&["beta", "alpha"])]),
                // A chapter lights a part it did not declare.
                chapter(&["beta"], vec![step(&["beta"]), step(&["writer"])]),
            ],
            ..Default::default()
        };
        assert_eq!(
            lit_parts(&brief),
            vec![vec!["alpha", "beta"], vec!["beta", "writer"]]
        );
    }

    #[test]
    fn the_model_of_the_setting_is_the_one_a_write_uses() {
        let briefs = PrBriefs::new(None);
        assert_eq!(
            briefs.model(),
            studio_understanding::pr_brief::DEFAULT_MODEL
        );
        briefs.set_model("haiku".into());
        assert_eq!(briefs.model(), "haiku");
    }
}
