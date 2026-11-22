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
}

impl Seen {
    pub fn is_quiet(&self, pr: u64, head: &str) -> bool {
        self.quiet.contains(&(pr, head.to_string()))
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

/// One lookup of ready PR `pr` at `head`. `source` builds what the writer reads, and is only called when a write starts.
pub fn on_lookup(
    with: &Writing,
    seen: &mut Seen,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    log: impl FnMut(String),
) -> Did {
    let before = seen.heads.insert(pr, head.to_string());
    if with.store.get(pr, head).is_some() || seen.is_quiet(pr, head) {
        return Did::Kept;
    }
    let again = before.as_deref() == Some(head);
    if !again && let Some(old) = with.store.latest(pr) {
        let _ = with.store.mark_stale(pr, &old.head);
        return Did::Staled;
    }
    // The first brief of a ready PR is written at once; a stale one when the same head is seen twice.
    match write_now(with, source, log) {
        Ok(Some(_)) => Did::Wrote,
        Ok(None) => {
            seen.quiet.insert((pr, head.to_string()));
            Did::Quiet
        }
        Err(_) => Did::Failed,
    }
}

/// Opening a PR's brief: the stored one when it is of the current head and not stale, else it is written now.
pub fn on_open(
    with: &Writing,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    match with.store.get(pr, head) {
        Some(stored) if !stored.stale => Ok(stored),
        _ => write_now(with, source, log)?.ok_or_else(|| {
            "Nothing you track changed in this PR any more, so it has no brief.".into()
        }),
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
    /// The "PR briefs" setting's model; the settings page does not carry it yet, so it is the default.
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

    /// Gives each ready PR its brief's state, for the tray.
    pub fn annotate(&self, project: &str, ready: &mut [Ready]) {
        let Some(store) = store_of(project) else {
            return;
        };
        let busy = self.busy.lock().unwrap();
        let seen = self.seen_of(project);
        let seen = seen.lock().unwrap();
        for pr in ready {
            let latest = store.latest(pr.number);
            let writing = busy.contains(&(project.to_string(), pr.number));
            if latest.is_none() && !writing {
                if seen.is_quiet(pr.number, &pr.head) {
                    pr.brief = Some(BriefInfo {
                        stale: false,
                        writing: false,
                        chapters: vec![],
                        quiet: true,
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

/// The numbers of the ready PRs whose brief is stored: the ones a brief is waiting for.
pub fn waiting_numbers(store: &BriefStore, ready: &[Ready]) -> Vec<u64> {
    ready
        .iter()
        .filter(|pr| store.latest(pr.number).is_some())
        .map(|pr| pr.number)
        .collect()
}

/// Whether a brief waits for one of `project`'s ready PRs: Home's quiet dot lights for it.
pub fn any_waiting(project: &str, ready: &[Ready]) -> bool {
    store_of(project).is_some_and(|store| !waiting_numbers(&store, ready).is_empty())
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
    let numbers = store_of(&project).map(|s| waiting_numbers(&s, &ready))?;
    Some(Waiting { project, numbers })
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
            let model = briefs.model.lock().unwrap().clone();
            let seen = briefs.seen_of(&project);
            let with = Writing {
                writer: writer.as_ref(),
                model: &model,
                store: &store,
            };
            let did = on_lookup(
                &with,
                &mut seen.lock().unwrap(),
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
    let model = briefs.model.lock().unwrap().clone();
    let with = Writing {
        writer: writer.as_ref(),
        model: &model,
        store: &store,
    };
    let written = on_open(
        &with,
        number,
        &head,
        || source_of(&project, number),
        |line| log::warn!("pr brief of #{number}: {line}"),
    );
    briefs.busy(&project, number, false);
    let _ = app.emit("map-landed", project);
    written.map(|b| Some(view(b)))
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

    fn lookup(store: &BriefStore, seen: &mut Seen, fake: &Fake, head: &str) -> Did {
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
        let (mut seen, fake) = (Seen::default(), Fake::new());
        assert_eq!(lookup(&store, &mut seen, &fake, "aaa"), Did::Wrote);
        let stored = store.get(7, "aaa").expect("the brief is stored");
        assert_eq!(stored.brief.title, "Alpha");
        assert_eq!(fake.1.borrow()[0], "opus");
        // Another lookup of the same head writes nothing.
        assert_eq!(lookup(&store, &mut seen, &fake, "aaa"), Did::Kept);
        assert_eq!(store.records().len(), 1);
    }

    #[test]
    fn a_new_head_marks_the_brief_stale_and_a_second_lookup_of_it_rewrites() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (mut seen, fake) = (Seen::default(), Fake::new());
        lookup(&store, &mut seen, &fake, "aaa");
        assert_eq!(lookup(&store, &mut seen, &fake, "bbb"), Did::Staled);
        assert!(store.get(7, "aaa").unwrap().stale);
        assert!(store.get(7, "bbb").is_none(), "not rewritten at once");
        // A burst of pushes: the head moved again, so the count starts over.
        assert_eq!(lookup(&store, &mut seen, &fake, "ccc"), Did::Staled);
        assert_eq!(store.records().len(), 1);
        // The same head at two lookups in a row: rewritten.
        assert_eq!(lookup(&store, &mut seen, &fake, "ccc"), Did::Wrote);
        assert!(store.get(7, "ccc").is_some());
        assert_eq!(store.records().len(), 2);
    }

    #[test]
    fn opening_a_stale_brief_rewrites_it_and_a_fresh_one_is_read_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (mut seen, fake) = (Seen::default(), Fake::new());
        lookup(&store, &mut seen, &fake, "aaa");
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let open = |head: &str| on_open(&with, 7, head, || Ok(source(head)), |_| {});
        assert_eq!(open("aaa").unwrap().head, "aaa");
        assert_eq!(store.records().len(), 1, "a fresh brief is not rewritten");
        lookup(&store, &mut seen, &fake, "bbb");
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
        let (mut seen, fake) = (Seen::default(), Fake::new());
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
            let did = on_lookup(&with, &mut seen, pr, "aaa", || Ok(source), |_| {});
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
        let (mut seen, fake) = (Seen::default(), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let black = MAP.replace("\"core\"", "\"black-box\"");
        let gathered = Cell::new(0);
        let mut look = |head: &str| {
            let source = quiet_source(&black, &[], "");
            on_lookup(
                &with,
                &mut seen,
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
        let (mut seen, fake) = (Seen::default(), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        on_lookup(&with, &mut seen, 7, "aaa", || Ok(source("aaa")), |_| {});
        // #8 has none yet; #7 does; the stale one still waits.
        assert_eq!(waiting_numbers(&store, &[ready(7), ready(8)]), vec![7]);
        store.mark_stale(7, "aaa").unwrap();
        assert_eq!(waiting_numbers(&store, &[ready(7)]), vec![7]);
        // A PR that is not ready (merged, closed, a draft) is not in the list given, so nothing waits for it.
        assert!(waiting_numbers(&store, &[]).is_empty());
    }

    #[test]
    fn a_failed_write_is_told_and_stores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (mut seen, fake) = (Seen::default(), Fake::new());
        let told = RefCell::new(vec![]);
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let did = on_lookup(
            &with,
            &mut seen,
            7,
            "aaa",
            || Err("no git".into()),
            |l| told.borrow_mut().push(l),
        );
        assert_eq!(did, Did::Failed);
        assert!(told.borrow()[0].contains("no git"));
        assert!(store.latest(7).is_none());
    }
}
