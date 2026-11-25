//! The PR briefs of ready PRs (spec danielrispler/plotkeeper#334, ticket #339). When a PR becomes ready the server writes its
//! brief and stores it per PR and head; a new head marks it stale, and it is rewritten when opened or once two consecutive
//! lookups see the same head. The lookups are `prs.rs`'s (Landed's triggers, never a timer).
//!
//! `write_brief` is the one place the server calls the understanding crate's `write_pr_brief`, so the claim check and the retry
//! of ticket #338 take its place here.

use crate::App;
use crate::prs::Ready;
use serde::Serialize;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
};
use studio_understanding::pr_brief::{
    BriefStore, BriefWriter, PrSource, StoredBrief, build_facts, fetch_meta, gather, write_pr_brief,
};

/// What the tray row of a ready PR says of its brief.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BriefInfo {
    /// A newer head was pushed; it is rewritten when opened.
    pub stale: bool,
    /// Being written now.
    pub writing: bool,
    /// The chapters' titles, one tray row each; one title for a single story.
    pub chapters: Vec<String>,
    /// Every chapter of the brief was read to its end (#342). The project header's dot may ask this to leave a brief that is done; it does not yet.
    pub read: bool,
    /// The PR touches no part that would fog at Daniel's tiers, so it gets no brief.
    pub quiet: bool,
    /// The last write at this head failed (told to the log); opening it tries again.
    pub failed: bool,
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
                    // The tray row becomes the quiet one, which does not open.
                    seen.lock().unwrap().quiet.insert((pr, head.to_string()));
                    "Nothing you track changed in this PR, so it has no brief.".into()
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
    /// The PR matched the project's pre-flight (#394): the parts it expected against the parts the PR touched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<studio_understanding::pre_flight::Expected>,
    /// What the default branch changed since the PR's merge base that bears on the brief (#494); open PRs only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_base: Option<StaleBase>,
}

/// The build manifests at the repository's root whose change on the default branch bears on every brief (#494). A lockfile or a
/// manifest below the root is not one.
const ROOT_MANIFESTS: [&str; 4] = [
    "Cargo.toml",
    "package.json",
    "rust-toolchain.toml",
    "tsconfig.json",
];

/// What the default branch changed since a PR's merge base that the brief relies on (#494), each list sorted and each id once: the
/// map's laws, links and flows whose say on a lit part changed, the lit parts whose `paths` changed, and the changed files of lit
/// parts and root manifests.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct StaleBase {
    pub laws: Vec<String>,
    pub links: Vec<String>,
    pub flows: Vec<String>,
    pub paths: Vec<String>,
    pub files: Vec<String>,
}

/// What the default branch of `repo` changed, by one net diff from the merge base of `head` to its tip, that bears on the parts in
/// `lit`: none when nothing did, or when git cannot answer.
pub fn stale_base(repo: &Path, head: &str, lit: &BTreeSet<String>) -> Option<StaleBase> {
    use crate::map::git;
    let (tip, _) = crate::map::default_ref(repo)?;
    let base = git(repo, &["merge-base", &tip, head]).ok()?;
    let changed = git(repo, &["diff", "--name-only", &base, &tip]).ok()?;
    let changed: Vec<String> = changed.lines().map(String::from).collect();
    let map_at = |commit: &str| -> serde_json::Value {
        git(
            repo,
            &["show", &format!("{commit}:{}", system_map::MAP_FILE)],
        )
        .ok()
        .and_then(|text| system_map::convert_value(&text).ok())
        .unwrap_or_default()
    };
    let new_map = map_at(&tip);
    let mut stale = StaleBase::default();
    // (i) A changed file a lit part owns by the current map, and (iii) a root build manifest.
    if let Ok(map) = system_map::parse_lenient(&new_map.to_string()) {
        let touched = system_map::touched_for_map(&map, &changed);
        stale.files = touched
            .parts
            .into_iter()
            .filter(|p| lit.contains(&p.id))
            .flat_map(|p| p.files)
            .collect();
    }
    stale.files.extend(
        changed
            .iter()
            .filter(|path| ROOT_MANIFESTS.contains(&path.as_str()))
            .cloned(),
    );
    // (ii) The map's say on a lit part, read as the brief reads it: laws on it or on a link at it, links at it, flows through those.
    if changed.iter().any(|path| path == system_map::MAP_FILE) {
        let old_map = map_at(&base);
        for part in lit {
            map_moves(&old_map, &new_map, part, &mut stale);
        }
    }
    for list in [
        &mut stale.laws,
        &mut stale.links,
        &mut stale.flows,
        &mut stale.paths,
        &mut stale.files,
    ] {
        list.sort();
        list.dedup();
    }
    (stale != StaleBase::default()).then_some(stale)
}

/// Adds to `stale` what differs between `old` and `new` in the map's say on `part`: an item counts when it is at the part in either
/// map and its value is not the same in both.
fn map_moves(old: &serde_json::Value, new: &serde_json::Value, part: &str, stale: &mut StaleBase) {
    use serde_json::Value;
    let items =
        |map: &Value, key: &str| -> Vec<Value> { map[key].as_array().cloned().unwrap_or_default() };
    let id = |item: &Value| item["id"].as_str().unwrap_or_default().to_string();
    let links_at = |map: &Value| -> BTreeSet<String> {
        items(map, "links")
            .iter()
            .filter(|l| l["from"] == part || l["to"] == part)
            .map(id)
            .collect()
    };
    let links: BTreeSet<String> = links_at(old).union(&links_at(new)).cloned().collect();
    let at = |link: &Value| links.contains(link.as_str().unwrap_or_default());
    let moved = |key: &str, here: &dyn Fn(&Value) -> bool| -> Vec<String> {
        let of = |map: &Value| -> HashMap<String, Value> {
            items(map, key)
                .into_iter()
                .filter(|i| here(i))
                .map(|i| (id(&i), i))
                .collect()
        };
        let (was, now) = (of(old), of(new));
        was.keys()
            .chain(now.keys())
            .filter(|k| was.get(*k) != now.get(*k))
            .cloned()
            .collect()
    };
    stale
        .laws
        .extend(moved("laws", &|l| l["on"] == part || at(&l["on"])));
    stale.links.extend(moved("links", &|l| at(&l["id"])));
    stale.flows.extend(moved("flows", &|f| {
        f["hops"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|h| at(&h["link"]))
    }));
    let paths_of = |map: &Value| {
        items(map, "parts")
            .into_iter()
            .find(|p| p["id"] == part)
            .map(|p| p["paths"].clone())
    };
    if paths_of(old) != paths_of(new) {
        stale.paths.push(part.to_string());
    }
}

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
    writer: Option<SharedWriter>,
    seen: Mutex<HashMap<String, Arc<Mutex<Seen>>>>,
    busy: Arc<Mutex<HashSet<(String, u64)>>>,
    /// Which ready PRs the first lookup after start writes, and which wait to be opened.
    startup: Mutex<Startup>,
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
    #[allow(clippy::too_many_arguments)]
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
    fn annotate_with(
        &self,
        project: &str,
        store: &BriefStore,
        user: Option<&crate::map::UserState>,
        ready: &mut [Ready],
    ) {
        let busy = self.busy.lock().unwrap().clone();
        let (quiet, failed) = {
            let seen = self.seen_of(project);
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
                failed,
            });
        }
    }

    /// Claims `pr` of `project` as being written: `None` when it already is. The claim ends when the guard drops, a panic of the
    /// write included, so a dead write never leaves the PR busy.
    fn claim(&self, project: &str, pr: u64) -> Option<BusyGuard> {
        self.busy(project, pr, true).then(|| BusyGuard {
            busy: self.busy.clone(),
            key: (project.to_string(), pr),
        })
    }

    /// Opening PR `pr` of `project` whose brief must be written (port decision Q5): `write` runs on a thread of its own with the
    /// PR's claim, which it drops when done, so the open returns at once and the tray says "writing" meanwhile; the page gets the
    /// brief through `map-landed`. A PR being written already is left to that write. Either way the open is told it is being written.
    fn open_in_background(
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

/// A PR claimed as being written; see `PrBriefs::claim`.
struct BusyGuard {
    busy: Arc<Mutex<HashSet<(String, u64)>>>,
    key: (String, u64),
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.busy.lock().unwrap().remove(&self.key);
    }
}

/// The numbers of the ready PRs a brief is waiting for: the brief is stored and Daniel has not read all of it at the PR's current
/// head. A brief read to its end stops counting for the project header's dot and the rail's row (its tray row stays, to read again), and a push
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

/// Whether a brief waits for one of `project`'s ready PRs: the project header's quiet dot lights for it.
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

pub fn briefs_waiting(app: &App, cwd: String) -> Option<Waiting> {
    let project = studio_workbench::main_checkout(Path::new(&cwd)).ok()?;
    let ready = app.prs.ready(&project);
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

/// `project`'s own path under the data folder's `kind` folder: its path with each character but a letter or digit made `_`.
pub(crate) fn project_path(kind: &str, project: &str) -> Option<std::path::PathBuf> {
    let home = std::env::home_dir()?;
    let name: String = project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Some(studio_hook::data_dir(&home).join(kind).join(name))
}

/// A project's briefs live in a folder of their own, since PR numbers repeat across repositories.
pub(crate) fn store_of(project: &str) -> Option<BriefStore> {
    BriefStore::open(&project_path("pr-briefs-by-project", project)?).ok()
}

/// When PR head `head` was committed, in Unix milliseconds, when it is in `project`'s repository.
fn head_time_ms(project: &str, head: &str) -> Option<u64> {
    let at = crate::map::git(Path::new(project), &["show", "-s", "--format=%ct", head]).ok()?;
    Some(at.trim().parse::<u64>().ok()? * 1000)
}

/// What the writer reads for PR `number` at its head, gathered with `gh` and `git` in `project`.
fn source_of(project: &str, number: u64) -> Result<PrSource, String> {
    let repo = Path::new(project);
    let meta = fetch_meta(repo, number)?;
    crate::pre_flight::note_closing(project, &meta);
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

/// Whether a brief of `pr`'s head is stored and not stale.
fn has_fresh_brief(store: &BriefStore, pr: &Ready) -> bool {
    store.get(pr.number, &pr.head).is_some_and(|b| !b.stale)
}

/// Looks at each ready PR of `project` after a lookup of `prs.rs`: the first brief is written at once, a new head stales it. Each
/// PR goes on a thread of its own, since a write takes minutes; the Map view is told (`map-landed`) when a brief changed. At the
/// first lookup after start only the PRs `Startup` picks are written; the rest are written when opened.
pub fn lookup_later(app: &Arc<App>, project: &str, ready: Vec<Ready>) {
    let Some(store) = store_of(project) else {
        return;
    };
    let allowed = app.briefs.startup.lock().unwrap().allowed(
        project,
        app.prs.listed(project),
        &ready,
        &|pr| head_time_ms(project, &pr.head),
        studio_understanding::millis(),
        crate::settings::brief_backfill().window_ms,
        &|pr| has_fresh_brief(&store, pr),
    );
    for pr in ready.into_iter().filter(|pr| allowed.contains(&pr.number)) {
        let (app, project) = (app.clone(), project.to_string());
        std::thread::spawn(move || {
            let briefs = &app.briefs;
            let Some(writer) = briefs.writer.clone() else {
                return;
            };
            let Some(store) = store_of(&project) else {
                return;
            };
            let Some(claim) = briefs.claim(&project, pr.number) else {
                return;
            };
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
                    app.events
                        .emit("map-landed", serde_json::json!(project.clone()));
                    source_of(&project, pr.number)
                },
                |line| log::warn!("pr brief of #{}: {line}", pr.number),
            );
            drop(claim);
            if did != Did::Kept {
                app.events.emit("map-landed", serde_json::json!(project));
            }
        });
    }
}

/// Opening a PR's brief from the tray: the stored one, or, when it is stale or not written, the write starts in the background and
/// the answer says it is being written. A merged PR's brief is read as stored.
pub fn pr_brief(app: &Arc<App>, project: String, number: u64) -> Result<Option<Opened>, String> {
    let briefs = &app.briefs;
    let store = store_of(&project).ok_or("no data folder")?;
    let head = app
        .prs
        .ready(&project)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| r.head);
    let Some(head) = head else {
        return Ok(store
            .latest(number)
            .map(|b| Opened::Brief(Box::new(view_of(&project, &store, b, false)))));
    };
    if let Some(stored) = store.get(number, &head).filter(|b| !b.stale) {
        return Ok(Some(Opened::Brief(Box::new(view_of(
            &project, &store, stored, true,
        )))));
    }
    let Some(writer) = briefs.writer.clone() else {
        return Ok(store
            .latest(number)
            .map(|b| Opened::Brief(Box::new(view_of(&project, &store, b, true)))));
    };
    let write = {
        let (app, project) = (app.clone(), project.clone());
        move |claim: BusyGuard| {
            let briefs = &app.briefs;
            let model = briefs.model();
            let seen = briefs.seen_of(&project);
            let with = Writing {
                writer: writer.as_ref(),
                model: &model,
                store: &store,
            };
            // A failure is kept in `seen` (the tray row says so) and told to the log.
            let _ = on_open(
                &with,
                &seen,
                number,
                &head,
                || source_of(&project, number),
                |line| log::warn!("pr brief of #{number}: {line}"),
            );
            drop(claim);
            app.events.emit("map-landed", serde_json::json!(project));
        }
    };
    let opened = briefs.open_in_background(&project, number, write);
    // The tray says the brief is being written.
    app.events
        .emit("map-landed", serde_json::json!(project.clone()));
    Ok(Some(opened))
}

/// A stored brief as the window plays it, with whether the writer saw all of the diff, the line of a matched pre-flight, and, for an
/// `open` PR, what the default branch changed since its merge base that bears on the parts it lights (#494).
fn view_of(project: &str, store: &BriefStore, brief: StoredBrief, open: bool) -> BriefView {
    let partial = store
        .records()
        .iter()
        .rev()
        .find(|r| r.pr == brief.pr && r.head == brief.head)
        .is_some_and(|r| r.budget.files_cut + r.budget.files_partial > 0);
    let expected = crate::pre_flight::expected(project, brief.pr, &brief);
    let stale_base = open
        .then(|| {
            let lit: BTreeSet<String> = lit_parts(&brief.brief).into_iter().flatten().collect();
            stale_base(Path::new(project), &brief.head, &lit)
        })
        .flatten();
    BriefView {
        brief,
        partial,
        expected,
        stale_base,
    }
}

/// Guard with a law's new law (#401): the PR briefs' writer, which has no tools, drafts one from PR `number`'s stored brief, on a part
/// it touched, for Daniel to confirm or edit. The PR's text goes only to that writer; the run gets what Daniel confirms.
pub fn guard_draft(
    app: &App,
    project: String,
    number: u64,
) -> Result<crate::back_a_law::NewLaw, String> {
    let writer = app
        .briefs
        .writer()
        .ok_or("no writer: plotkeeper found no `claude` to ask")?;
    let brief = store_of(&project)
        .and_then(|store| store.latest(number))
        .filter(|brief| brief.guard.is_some())
        .ok_or("that brief offers no Guard with a law")?;
    let map_text = crate::map::read_map(Path::new(&project))
        .source
        .ok_or("this project has no map")?;
    crate::back_a_law::draft(
        writer.as_ref(),
        &crate::settings::pr_briefs_model(),
        &map_text,
        &brief,
    )
}

/// Chapter `chapter` of PR `number`'s brief was read to its end (the page saw its last step, focused, and the check was answered): the
/// read is kept as "read at head", and the fog the PR's merged commits filed leaves every part whose chapters are all read.
pub fn brief_chapter_done(
    app: &App,
    project: String,
    number: u64,
    chapter: usize,
    head: String,
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
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use studio_understanding::pr_brief::{BriefReply, CallUsage, PrMeta};

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

    /// The map of the stale-base tests: `alpha` is lit; the link `ab` is at it, `bg` is not; `f1` runs through `ab`, `f2` only
    /// through `bg`; `law-ab` sits on the link at it.
    fn stale_map() -> serde_json::Value {
        serde_json::json!({ "version": 2,
          "parts": [
            { "id": "alpha", "name": "Alpha", "caption": "c", "purpose": "p", "holds": [], "tier": "core", "paths": ["alpha/"] },
            { "id": "beta", "name": "Beta", "caption": "c", "purpose": "p", "holds": [], "tier": "core", "paths": ["beta/"] },
            { "id": "gamma", "name": "Gamma", "caption": "c", "purpose": "p", "holds": [], "tier": "core", "paths": ["gamma/"] }],
          "actors": [],
          "links": [
            { "id": "ab", "from": "alpha", "to": "beta", "kind": "import", "reason": "Alpha calls beta." },
            { "id": "bg", "from": "beta", "to": "gamma", "kind": "import", "reason": "Beta calls gamma." }],
          "ignored": ["tools/"],
          "laws": [
            { "id": "law-a", "on": "alpha", "backing": "trust", "text": "Alpha keeps order." },
            { "id": "law-ab", "on": "ab", "backing": "trust", "text": "The wire agrees." },
            { "id": "law-g", "on": "gamma", "backing": "trust", "text": "Gamma keeps order." }],
          "flows": [
            { "id": "f1", "name": "One", "hops": [{ "link": "ab", "text": "Alpha asks beta." }] },
            { "id": "f2", "name": "Two", "hops": [{ "link": "bg", "text": "Beta asks gamma." }] }] })
    }

    fn put(dir: &Path, path: &str, text: &str) {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }

    /// A repository whose PR changed `alpha` off the merge base, and whose `main` then wrote `files` and the map `change` made: its
    /// folder, and the PR's head.
    fn moved_main(
        files: &[&str],
        change: impl FnOnce(&mut serde_json::Value),
    ) -> (tempfile::TempDir, String) {
        use studio_test_support::{git, repo};
        let (dir, _) = repo();
        let at = dir.path();
        let mut map = stale_map();
        put(at, "system-map.json", &map.to_string());
        for path in [
            "alpha/a.rs",
            "beta/b.rs",
            "beta/package.json",
            "gamma/g.rs",
            "tools/Cargo.toml",
            "Cargo.toml",
            "Cargo.lock",
            "package.json",
            "package-lock.json",
            "rust-toolchain.toml",
            "tsconfig.json",
        ] {
            put(at, path, "one\n");
        }
        git(at, &["add", "-A"]);
        git(at, &["commit", "-q", "-m", "base"]);
        git(at, &["checkout", "-q", "-b", "pr"]);
        put(at, "alpha/a.rs", "the PR\n");
        git(at, &["commit", "-q", "-am", "the PR"]);
        let head = git(at, &["rev-parse", "HEAD"]);
        git(at, &["checkout", "-q", "main"]);
        for path in files {
            put(at, path, "main moved\n");
        }
        change(&mut map);
        put(at, "system-map.json", &map.to_string());
        git(at, &["add", "-A"]);
        git(at, &["commit", "-q", "--allow-empty", "-m", "main moved"]);
        (dir, head)
    }

    fn stale_of(files: &[&str], change: impl FnOnce(&mut serde_json::Value)) -> Option<StaleBase> {
        let (dir, head) = moved_main(files, change);
        stale_base(dir.path(), &head, &BTreeSet::from(["alpha".to_string()]))
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn main_changing_a_file_of_a_lit_part_names_the_file() {
        assert_eq!(
            stale_of(&["alpha/new.rs", "beta/b.rs"], |_| {}),
            Some(StaleBase {
                files: names(&["alpha/new.rs"]),
                ..StaleBase::default()
            })
        );
    }

    #[test]
    fn main_changing_the_map_at_a_lit_part_names_each_law_link_flow_and_paths_that_moved() {
        let law = stale_of(&[], |m| {
            m["laws"][0]["text"] = "Alpha keeps a new order.".into()
        });
        assert_eq!(law.map(|s| s.laws), Some(names(&["law-a"])));
        // A law on a link at the lit part is the brief's reading, not fog's.
        let on_link = stale_of(&[], |m| {
            m["laws"][1]["text"] = "The wire agrees again.".into()
        });
        assert_eq!(on_link.map(|s| s.laws), Some(names(&["law-ab"])));
        let link = stale_of(&[], |m| {
            m["links"][0]["reason"] = "Alpha now asks beta.".into()
        });
        assert_eq!(
            link,
            Some(StaleBase {
                links: names(&["ab"]),
                ..StaleBase::default()
            })
        );
        let flow = stale_of(&[], |m| {
            m["flows"][0]["hops"][0]["text"] = "Alpha tells beta.".into()
        });
        assert_eq!(flow.map(|s| s.flows), Some(names(&["f1"])));
        let paths = stale_of(&[], |m| {
            m["parts"][0]["paths"] = serde_json::json!(["alpha/", "extra/"])
        });
        assert_eq!(paths.map(|s| s.paths), Some(names(&["alpha"])));
        // A law added on the lit part moved too.
        let added = stale_of(&[], |m| {
            m["laws"].as_array_mut().unwrap().push(
                serde_json::json!({ "id": "law-new", "on": "alpha", "backing": "trust", "text": "New." }),
            )
        });
        assert_eq!(added.map(|s| s.laws), Some(names(&["law-new"])));
    }

    #[test]
    fn main_changing_a_root_build_manifest_names_it() {
        assert_eq!(
            stale_of(
                &[
                    "Cargo.toml",
                    "package.json",
                    "rust-toolchain.toml",
                    "tsconfig.json"
                ],
                |_| {}
            ),
            Some(StaleBase {
                files: names(&[
                    "Cargo.toml",
                    "package.json",
                    "rust-toolchain.toml",
                    "tsconfig.json"
                ]),
                ..StaleBase::default()
            })
        );
    }

    #[test]
    fn lockfiles_and_manifests_below_the_root_are_no_stale_base() {
        assert_eq!(
            stale_of(
                &[
                    "Cargo.lock",
                    "package-lock.json",
                    "tools/Cargo.toml",
                    "beta/package.json"
                ],
                |_| {}
            ),
            None
        );
    }

    #[test]
    fn main_moving_only_elsewhere_is_no_stale_base() {
        let elsewhere = stale_of(&["beta/b.rs", "gamma/g.rs"], |m| {
            m["laws"][2]["text"] = "Gamma keeps a new order.".into();
            m["links"][1]["reason"] = "Beta now asks gamma.".into();
            m["flows"][1]["hops"][0]["text"] = "Beta tells gamma.".into();
            m["parts"][1]["paths"] = serde_json::json!(["beta/", "more/"]);
        });
        assert_eq!(elsewhere, None);
    }

    #[test]
    fn a_merged_prs_brief_gets_no_stale_base_and_an_open_ones_does() {
        let (dir, head) = moved_main(&["alpha/new.rs"], |_| {});
        let data = tempfile::tempdir().unwrap();
        let store = BriefStore::open(data.path()).unwrap();
        let mut stored = StoredBrief {
            pr: 7,
            head,
            ..StoredBrief::default()
        };
        stored.brief.chapters = vec![studio_understanding::pr_brief::Chapter {
            steps: vec![studio_understanding::pr_brief::Step {
                on: vec!["alpha".into()],
                ..Default::default()
            }],
            ..Default::default()
        }];
        let project = dir.path().display().to_string();
        assert_eq!(
            view_of(&project, &store, stored.clone(), false).stale_base,
            None
        );
        assert_eq!(
            view_of(&project, &store, stored, true)
                .stale_base
                .map(|s| s.files),
            Some(names(&["alpha/new.rs"]))
        );
    }

    struct Fake(Cell<usize>, RefCell<Vec<String>>);
    impl Fake {
        fn new() -> Self {
            Fake(Cell::new(0), RefCell::new(vec![]))
        }
    }
    impl BriefWriter for Fake {
        fn write(&self, model: &str, _prompt: &str) -> Result<BriefReply, String> {
            self.0.set(self.0.get() + 1);
            self.1.borrow_mut().push(model.into());
            Ok(BriefReply {
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

    #[test]
    fn opening_a_pr_that_fogs_nothing_tells_the_tray_it_is_quiet() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let black = MAP.replace("\"core\"", "\"black-box\"");
        let opened = on_open(
            &with,
            &seen,
            7,
            "aaa",
            || Ok(quiet_source(&black, &[], "")),
            |_| {},
        );
        assert!(opened.is_err());
        assert!(
            seen.lock().unwrap().is_quiet(7, "aaa"),
            "the row becomes the quiet one"
        );
    }

    fn ready(number: u64) -> Ready {
        Ready {
            number,
            title: String::new(),
            head: "aaa".into(),
            branch: String::new(),
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
        fn write(&self, _model: &str, _prompt: &str) -> Result<BriefReply, String> {
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
    fn opening_a_pr_to_write_returns_at_once_and_the_pr_stays_claimed_until_the_write_ends() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let briefs = PrBriefs::new(None);
        let (release_tx, release_rx) = channel::<()>();
        let (done_tx, done_rx) = channel();
        let (returned_tx, returned_rx) = channel();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let opened = briefs.open_in_background("/p", 7, move |claim| {
                    let _ = release_rx.recv_timeout(Duration::from_secs(10));
                    drop(claim);
                    done_tx.send(()).unwrap();
                });
                returned_tx.send(opened).unwrap();
            });
            let returned = returned_rx.recv_timeout(Duration::from_secs(3));
            assert!(returned.is_ok(), "the open waited for the write");
            assert!(matches!(
                returned.unwrap(),
                Opened::Writing { writing: true }
            ));
            assert!(
                briefs.claim("/p", 7).is_none(),
                "the PR is claimed while written"
            );
            // A second open meanwhile is told it is being written, and starts nothing.
            assert!(matches!(
                briefs.open_in_background("/p", 7, |_| panic!("a second write")),
                Opened::Writing { writing: true }
            ));
            release_tx.send(()).unwrap();
            done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        });
        // The claim ended with the write.
        assert!(briefs.claim("/p", 7).is_some());
    }

    #[test]
    fn the_tray_says_when_the_write_at_the_head_failed() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let briefs = PrBriefs::new(None);
        let seen = briefs.seen_of("/p");
        let fake = Fake::new();
        let with = Writing {
            writer: &fake,
            model: "opus",
            store: &store,
        };
        let failed = |pr: u64, head: &str| {
            let mut tray = [ready_pr(pr, head)];
            briefs.annotate_with("/p", &store, None, &mut tray);
            tray[0].brief.as_ref().map(|b| b.failed)
        };
        on_lookup(&with, &seen, 7, "aaa", || Err("no gh".into()), |_| {});
        assert_eq!(failed(7, "aaa"), Some(true), "a PR with no brief");
        assert_eq!(failed(7, "bbb"), None, "another head is not known to fail");
        // A stored brief whose rewrite failed.
        let mut eight = source("aaa");
        eight.meta.number = 8;
        on_lookup(&with, &seen, 8, "aaa", || Ok(eight), |_| {});
        assert!(on_open(&with, &seen, 8, "bbb", || Err("no gh".into()), |_| {}).is_err());
        assert_eq!(failed(8, "bbb"), Some(true));
        assert_eq!(failed(8, "aaa"), Some(false));
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
    fn a_write_that_panics_leaves_the_pr_free_to_write_again() {
        let briefs = PrBriefs::new(None);
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _claim = briefs.claim("/p", 7).expect("free at first");
            assert!(briefs.claim("/p", 7).is_none(), "claimed while written");
            panic!("the writer died");
        }));
        assert!(panicked.is_err());
        assert!(
            briefs.claim("/p", 7).is_some(),
            "the claim ended with the panic"
        );
    }

    fn ready_pr(number: u64, head: &str) -> Ready {
        Ready {
            number,
            title: format!("PR {number}"),
            head: head.into(),
            branch: String::new(),
            brief: None,
        }
    }

    const HOUR: u64 = 3_600_000;
    const NOW: u64 = 100 * HOUR;

    /// The lookup the first start gives `ready`, with each head `ago` hours old, the window 2 hours.
    fn at_start(startup: &mut Startup, ready: &[Ready], ago: &[(&str, u64)]) -> Vec<u64> {
        let head_ms = |pr: &Ready| {
            ago.iter()
                .find(|(h, _)| *h == pr.head)
                .map(|(_, hours)| NOW - hours * HOUR)
        };
        startup.allowed("/p", true, ready, &head_ms, NOW, 2 * HOUR, &|_| false)
    }

    #[test]
    fn at_start_at_most_three_ready_prs_whose_head_moved_within_the_window_are_written_newest_head_first()
     {
        let ready: Vec<Ready> = (1..=6).map(|n| ready_pr(n, &format!("h{n}"))).collect();
        // PR 5's head is the newest, then 2, 4, 1; PR 3 is outside the 2-hour window and PR 6 has no known head time.
        let ago = [("h1", 1), ("h2", 0), ("h3", 5), ("h4", 1), ("h5", 0)];
        let mut startup = Startup::default();
        let mut written = at_start(&mut startup, &ready, &ago);
        written.sort();
        // The two heads made now, then the newer PR of the two made an hour ago; 3 is outside the window, 6 has no head time.
        assert_eq!(written, [2, 4, 5]);
        assert_eq!(START_BRIEFS, 3);
    }

    #[test]
    fn a_pr_left_for_opening_at_start_is_not_written_by_later_lookups_but_one_that_becomes_ready_is()
     {
        let mut startup = Startup::default();
        let first = [ready_pr(1, "h1"), ready_pr(2, "h2")];
        let ago = [("h1", 0), ("h2", 9), ("h4", 0)];
        assert_eq!(at_start(&mut startup, &first, &ago), [1]);
        // The next lookup finds PR 2 again, with a new head: still left for opening; PR 4 became ready while running.
        let second = [ready_pr(1, "h1"), ready_pr(2, "h2b"), ready_pr(4, "h4")];
        let later = startup.allowed("/p", true, &second, &|_| None, NOW, 2 * HOUR, &|_| false);
        assert_eq!(later, [1, 4]);
        // PR 2 leaves the ready list and comes back: it became ready while running.
        startup.allowed(
            "/p",
            true,
            &[ready_pr(1, "h1")],
            &|_| None,
            NOW,
            2 * HOUR,
            &|_| false,
        );
        let back = startup.allowed("/p", true, &second, &|_| None, NOW, 2 * HOUR, &|_| false);
        assert_eq!(back, [1, 2, 4]);
    }

    #[test]
    fn a_pr_whose_brief_is_not_fresh_counts_toward_the_start_cap_and_projects_start_on_their_own() {
        let mut startup = Startup::default();
        // PR 1's brief is of its head; 2, 3, 4 and 6 have a brief of an older head (or one marked stale); 5 has none.
        let ready: Vec<Ready> = (1..=6).map(|n| ready_pr(n, &format!("h{n}"))).collect();
        let ago = [
            ("h1", 9),
            ("h2", 0),
            ("h3", 1),
            ("h4", 1),
            ("h5", 0),
            ("h6", 9),
        ];
        let head_ms = |pr: &Ready| {
            ago.iter()
                .find(|(h, _)| *h == pr.head)
                .map(|(_, hours)| NOW - hours * HOUR)
        };
        let fresh = |pr: &Ready| pr.number == 1;
        // Rewrites count toward the cap: the newest heads 5 and 2, then 4 (the newer of the two an hour old). 3 and 6 stay stale
        // until opened; 1 is looked at and kept.
        let first = startup.allowed("/p", true, &ready, &head_ms, NOW, 2 * HOUR, &fresh);
        assert_eq!(first, [1, 2, 4, 5]);
        let again = startup.allowed("/p", true, &ready, &head_ms, NOW, 2 * HOUR, &fresh);
        assert_eq!(again, [1, 2, 4, 5]);
        // Another project's first lookup is its own start.
        let other = startup.allowed(
            "/q",
            true,
            &[ready_pr(1, "h1")],
            &|_| Some(NOW),
            NOW,
            2 * HOUR,
            &|_| false,
        );
        assert_eq!(other, [1]);
    }

    #[test]
    fn a_pr_left_for_opening_is_looked_at_as_usual_once_its_brief_was_written() {
        let mut startup = Startup::default();
        let ready = [ready_pr(3, "h3")];
        let old = |_: &Ready| Some(NOW - 9 * HOUR);
        assert!(
            startup
                .allowed("/p", true, &ready, &old, NOW, 2 * HOUR, &|_| false)
                .is_empty()
        );
        // Daniel opened it and its brief was written.
        assert_eq!(
            startup.allowed("/p", true, &ready, &old, NOW, 2 * HOUR, &|_| true),
            [3]
        );
        // A later push: the lookups stale and rewrite it as for any PR.
        let pushed = [ready_pr(3, "h3b")];
        assert_eq!(
            startup.allowed("/p", true, &pushed, &old, NOW, 2 * HOUR, &|_| false),
            [3]
        );
    }

    #[test]
    fn a_brief_of_an_older_head_or_marked_stale_is_not_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let store = BriefStore::open(dir.path()).unwrap();
        let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
        assert!(!has_fresh_brief(&store, &ready_pr(7, "aaa")));
        lookup(&store, &seen, &fake, "aaa");
        assert!(has_fresh_brief(&store, &ready_pr(7, "aaa")));
        assert!(!has_fresh_brief(&store, &ready_pr(7, "bbb")));
        store.mark_stale(7, "aaa").unwrap();
        assert!(!has_fresh_brief(&store, &ready_pr(7, "aaa")));
    }

    #[test]
    fn a_first_lookup_that_gh_did_not_answer_does_not_spend_the_start() {
        let mut startup = Startup::default();
        let ready: Vec<Ready> = (1..=5).map(|n| ready_pr(n, &format!("h{n}"))).collect();
        let old = |_: &Ready| Some(NOW - 9 * HOUR);
        // `gh` did not answer at start: nothing is known to be open, and nothing is written.
        assert!(
            startup
                .allowed("/p", false, &[], &old, NOW, 2 * HOUR, &|_| false)
                .is_empty()
        );
        // The first lookup that answers is the start: every head is old, so all wait to be opened.
        let first = startup.allowed("/p", true, &ready, &old, NOW, 2 * HOUR, &|_| false);
        assert!(first.is_empty(), "the start cap still holds: {first:?}");
    }
}
