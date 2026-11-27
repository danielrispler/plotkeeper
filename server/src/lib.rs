//! The app's state, the module list and the re-exports of the functions behind its commands, with no Tauri in them
//! (danielrispler/plotkeeper#321, spec #319 "The server"). The `plotkeeper` server holds one [`App`] and answers each
//! command with the function that carries its name ([`learning`], [`session_view`] and the modules below), over HTTP (`api`).
//!
//! The core state, the card store, the risk notes, the card writer, the turn writing and the notices are the [`App`]'s. The
//! settings are read and written here ([`settings`]), and so are the PR lookups ([`prs`], [`refresh_landed`]) and the PR briefs
//! ([`pr_briefs`]). What reaches the page leaves through [`Events`], which holds the web server's [`Sink`]; the read loop is [`follow`].
//! Workbench-only commands (terminals, projects, worktrees, skills, usage, restart) stay in the shell.

use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use studio_core::{Notifications, Session, Studio, TaskFacts, TaskQuery};
use studio_understanding::{
    CardStore, ClaudeWriter, Core, Effects, FakeWriter, OpenItem, RiskNotes, RiskView, Row,
    SessionCards, StoredSummary, SummarySetup, TurnWriting, Writer, WriterKind, live_roots, millis,
    open_items, turn_hunk, write_agent_summary,
};

pub mod answer;
pub mod api;
pub mod back_a_law;
pub mod ci_laws;
pub mod fog;
pub mod history;
pub mod hook_copies;
pub mod install;
mod json_file;
pub mod learning;
pub mod lock;
pub mod map;
pub mod map_diff;
pub mod map_review;
pub mod map_scan;
pub mod micro_worlds;
pub mod part_catch_ups;
pub mod pr_briefs;
pub mod pre_flight;
pub mod projects;
pub mod prs;
pub mod ratify;
mod read_loop;
mod records;
pub mod runner;
pub mod session_view;
pub mod settings;
pub mod staleness;
pub mod stop_rules;
pub(crate) mod story_bar;
pub mod web;

pub use learning::*;
pub use read_loop::{Follower, Notice, ServerEffects, follow, tell, uptime};
pub use session_view::*;

/// The card store, which the log thread adds to and the pane records guesses in.
pub type Cards = Arc<Mutex<CardStore>>;

/// The risk notes of blocked commands (danielrispler/plotkeeper#298), in a store of their own beside the card store, which the
/// window's card read joins to the decision history. Since the blocked-command card went (#572) nothing asks for new ones.
pub type Notes = Arc<Mutex<RiskNotes>>;

/// The notices not shown yet, which the log thread and the card writer's threads add to.
pub type Notices = Mutex<Notifications>;

/// What the read loop and the state send out, and what they ask of the window or page they serve. The Tauri shell is one
/// implementation (the window's events and focus); the web server is another.
pub trait Sink: Send + Sync {
    /// Sends event `name` with `payload` to the window or the page.
    fn emit(&self, name: &str, payload: serde_json::Value);
    /// Whether Daniel is looking at the window or page now: the "seen" and notice rules read it through the loop.
    fn focused(&self) -> bool;
    /// The log's replay at start is done, and the first list is sent.
    fn replayed(&self);
}

/// Where what the state announces goes: a [`Sink`], which the shell sets. Nothing is sent until it is set, and an unset sink
/// is never focused.
#[derive(Default)]
pub struct Events(OnceLock<Arc<dyn Sink>>);

impl Events {
    /// Sets where events go; the first call wins.
    pub fn set(&self, sink: Arc<dyn Sink>) {
        // Set once; a later set keeps the first value, which is the same.
        self.0.set(sink).ok();
    }

    /// Sends event `name` with `payload`, or drops it while nothing is set.
    pub fn emit(&self, name: &str, payload: serde_json::Value) {
        if let Some(sink) = self.0.get() {
            sink.emit(name, payload);
        }
    }

    pub fn focused(&self) -> bool {
        self.0.get().is_some_and(|sink| sink.focused())
    }

    pub fn replayed(&self) {
        if let Some(sink) = self.0.get() {
            sink.replayed();
        }
    }
}

/// Everything the process owns that the commands read and change. Its locked state is reached through
/// its accessors (`core`, `cards`, `notes` and the rest); a holder of several takes them in one order,
/// core, then cards, then notes (`core_and_cards`, `core_cards_and_notes`). The notices are the exception:
/// their due check takes the core's lock under theirs, and nothing takes the notices' under the core's.
pub struct App {
    core: Core,
    cards: Cards,
    notes: Notes,
    /// The card writer: `claude -p` or `agy`, as `summaries.writer` says, or the fake when `PLOTKEEPER_CARD_WRITER=fake`, for checks that must not run `claude -p`.
    pub writer: Writer,
    pub writing: TurnWriting,
    notices: Notices,
    pub fetches: Arc<map::Fetches>,
    pub ci: Arc<ci_laws::CiCache>,
    /// The PRs found for each project's branches (`prs.rs`) and the PR briefs written for the ready ones (`pr_briefs.rs`).
    pub prs: prs::Prs,
    pub briefs: pr_briefs::PrBriefs,
    /// The part catch-ups being written and the ones that failed (`part_catch_ups.rs`).
    pub catch_ups: part_catch_ups::CatchUps,
    /// The saved project list (`projects.rs`), beside the log.
    pub projects: projects::Projects,
    watching: Mutex<answer::Watching>,
    pub events: Arc<Events>,
    /// The notice the card writer last raised and not yet cleared, for a page opened after it (danielrispler/plotkeeper#328).
    writer_notice: Mutex<Option<String>>,
    /// Whether summaries are written: off until a writer passes Test for a user who arrived with none (spec #320).
    pub summaries_on: AtomicBool,
    /// The list changed outside the read loop since it last sent it: a PR was found, or Daniel marked a session unseen.
    pub list_changed: AtomicBool,
    /// Whether the story rail shows, as the UI last said: not in focus mode, behind a closed overlay or under the skills
    /// browser, when a turn on it is not seen (danielrispler/Falinks#556).
    pub rail_shown: AtomicBool,
    /// Where the API-key writers' keys are kept: the OS's store at start (`with_keys`, `settings::os_keys`), memory before; and the writers whose
    /// key is set, all the page learns of them.
    pub keys: Arc<dyn settings::ApiKeys>,
    key_set: Mutex<Vec<WriterKind>>,
    /// The background runs (`runner.rs`): their queues, their processes and their outcomes.
    pub runner: Arc<runner::Runner>,
    /// The micro-worlds (`micro_worlds.rs`): built by background runs, checked on the host and kept per project.
    pub worlds: Arc<micro_worlds::MicroWorlds>,
    /// Back a law (`back_a_law.rs`): its runs' proofs and the patches kept.
    pub laws: Arc<back_a_law::LawRuns>,
    /// The map text reviews (`map_review.rs`): their runs and the verdicts kept per PR.
    pub reviews: Arc<map_review::MapReviews>,
    /// The map items each landed commit changed (`ratify::changes_of`).
    changes: ratify::ChangesCache,
    /// Each project's part births (`staleness::births`).
    births: staleness::Births,
}

impl App {
    /// The core, locked.
    pub fn core(&self) -> MutexGuard<'_, Studio> {
        self.core.lock().unwrap()
    }

    /// The card store, locked.
    pub fn cards(&self) -> MutexGuard<'_, CardStore> {
        self.cards.lock().unwrap()
    }

    /// The risk notes, locked.
    pub fn notes(&self) -> MutexGuard<'_, RiskNotes> {
        self.notes.lock().unwrap()
    }

    /// The core, then the card store, locked in that order.
    pub fn core_and_cards(&self) -> (MutexGuard<'_, Studio>, MutexGuard<'_, CardStore>) {
        let core = self.core();
        (core, self.cards())
    }

    /// The core, the card store and the risk notes, locked in that order.
    pub fn core_cards_and_notes(
        &self,
    ) -> (
        MutexGuard<'_, Studio>,
        MutexGuard<'_, CardStore>,
        MutexGuard<'_, RiskNotes>,
    ) {
        let (core, cards) = self.core_and_cards();
        (core, cards, self.notes())
    }

    /// The notices not shown yet, locked.
    pub fn notices(&self) -> MutexGuard<'_, Notifications> {
        self.notices.lock().unwrap()
    }

    /// What the page watches of the answer files, locked.
    pub fn watching(&self) -> MutexGuard<'_, answer::Watching> {
        self.watching.lock().unwrap()
    }

    /// The card writer's last notice not yet cleared, locked.
    pub fn writer_notice(&self) -> MutexGuard<'_, Option<String>> {
        self.writer_notice.lock().unwrap()
    }

    /// The writers whose API key is set, locked.
    pub fn key_set(&self) -> MutexGuard<'_, Vec<WriterKind>> {
        self.key_set.lock().unwrap()
    }

    /// The state over the event log `log`: the card store and the risk notes beside it, the worktree roots a launch before
    /// saved and the card writer, whose availability notices go to `writer_notices`. `effects` is what turn writing asks of the
    /// shell.
    pub fn open(
        log: &Path,
        effects: Arc<dyn Effects>,
        writer_notices: mpsc::Sender<Option<String>>,
    ) -> io::Result<App> {
        // The store sits beside the log, in the studio's data directory.
        let cards: Cards = Arc::new(Mutex::new(CardStore::open_beside(log)?));
        let notes: Notes = Arc::new(Mutex::new(RiskNotes::open_beside(log)?));
        let fake = std::env::var("PLOTKEEPER_CARD_WRITER").as_deref() == Ok("fake");
        let writer: Writer = if fake {
            Arc::new(FakeWriter)
        } else {
            let writer = ClaudeWriter::beside(log)?;
            let writer_notices = Mutex::new(writer_notices);
            Arc::new(writer.on_availability(move |notice| {
                // The receiver may have gone, and then no one waits for this.
                writer_notices.lock().unwrap().send(notice).ok();
            }))
        };
        let app = Self::open_with(log, cards, notes, writer, effects)?;
        // The checks' fake writer keeps its key in memory, so no check reads the OS's key store.
        Ok(if fake {
            app
        } else {
            app.with_keys(settings::os_keys())
        })
    }

    /// The state with the API-key writers' keys kept in `keys`: the writer is given each key set there.
    pub fn with_keys(mut self, keys: Arc<dyn settings::ApiKeys>) -> Self {
        let mut set = Vec::new();
        for kind in WriterKind::ALL.into_iter().filter(|kind| kind.http()) {
            let key = keys.get(kind);
            if key.is_some() {
                set.push(kind);
            }
            self.writer.use_key(kind, key);
        }
        self.key_set = Mutex::new(set);
        self.keys = keys;
        self
    }

    /// The state over `log` with the stores and the writer given, so a test can hold the writer.
    pub fn open_with(
        log: &Path,
        cards: Cards,
        notes: Notes,
        writer: Writer,
        effects: Arc<dyn Effects>,
    ) -> io::Result<App> {
        // The PR briefs' writer is Claude whatever `summaries.writer` says, and never the fake of the checks. Without
        // `claude` there is none, so only a short brief, made by code, is stored, and the page says briefs need Claude (spec #320, #634).
        let briefs =
            pr_briefs::PrBriefs::new(match std::env::var("PLOTKEEPER_CARD_WRITER").as_deref() {
                Ok("fake") => None,
                _ => ClaudeWriter::beside(log)
                    .ok()
                    .filter(ClaudeWriter::claude_found)
                    .map(|w| {
                        Arc::new(w.with_brief_effort(settings::pr_briefs_effort))
                            as Arc<dyn studio_understanding::pr_brief::BriefWriter + Send + Sync>
                    }),
            });
        let mut studio = Studio::with_roots(studio_understanding::worktree_root);
        studio.know_roots(saved_roots(log));
        let core: Core = Arc::new(Mutex::new(studio));
        let writing = TurnWriting::new(core.clone(), cards.clone(), writer.clone(), effects);
        let events: Arc<Events> = Arc::default();
        // A run that starts or ends draws its project's map again, as a brief that lands does.
        let runner = runner::Runner::new(runner::Config::beside(log));
        let data = log.parent().unwrap_or(Path::new(""));
        let worlds = micro_worlds::MicroWorlds::new(
            data.join("micro-worlds"),
            data.join("settings.json"),
            &runner,
        );
        let told = events.clone();
        worlds.on_change(move |project| told.emit("map-landed", project.into()));
        let reviews = map_review::MapReviews::new(
            map_review::Config {
                store: data.join("map-reviews"),
                gh: None,
                ratify: data.join(ratify::FILE),
            },
            &runner,
        );
        let told = events.clone();
        reviews.on_change(move |project| told.emit("map-landed", project.into()));
        let told = events.clone();
        // A run that ends may be a micro-world's, which is checked then, or a map text review's, whose answer is read then.
        let building = Arc::downgrade(&worlds);
        let reviewing = Arc::downgrade(&reviews);
        runner.on_change(move |project| {
            told.emit("map-landed", project.into());
            if let Some(worlds) = building.upgrade() {
                worlds.runs_changed(project);
            }
            if let Some(reviews) = reviewing.upgrade() {
                reviews.runs_changed(project);
            }
        });
        let laws = back_a_law::LawRuns::new(runner.clone(), back_a_law::Config::beside(log));
        let told = events.clone();
        laws.on_change(move |project| told.emit("map-landed", project.into()));
        // A part catch-up's write that moves on to its next call draws its stage on the waiting card (#450).
        let told = events.clone();
        let catch_ups = part_catch_ups::CatchUps::telling(move |project| {
            told.emit("map-landed", project.into());
        });
        Ok(App {
            core,
            cards,
            notes,
            writer,
            writing,
            notices: Notices::default(),
            fetches: Arc::default(),
            ci: Arc::new(ci_laws::load_cache()),
            prs: prs::Prs::default(),
            briefs,
            catch_ups,
            projects: projects::Projects::beside(log),
            watching: Mutex::default(),
            events,
            writer_notice: Mutex::new(None),
            summaries_on: AtomicBool::new(true),
            list_changed: AtomicBool::new(false),
            rail_shown: AtomicBool::new(true),
            keys: Arc::new(settings::MemoryKeys::default()),
            key_set: Mutex::new(Vec::new()),
            runner,
            worlds,
            laws,
            reviews,
            changes: Mutex::default(),
            births: Mutex::default(),
        })
    }

    /// The list changed outside the read loop: it sends it on its next pass.
    pub fn list_changed(&self) {
        self.list_changed.store(true, Ordering::Release);
    }
}

// What the card writer's code reports, and how it is logged.

/// What of a line from the writers' code may reach the Studio log: the line up to the first quote mark, so a reply's
/// words never do, whether quoted after a colon or inside a parse error (`invalid type: string "<reply>"`).
pub fn scrubbed(line: &str) -> &str {
    line.split('"')
        .next()
        .unwrap_or(line)
        .trim_end_matches([':', ' '])
}

/// Whether a line from the writers' code reports lost data (a save that failed), not a degraded step. Every such line of
/// the `understanding` crate starts `save `; a test reads their sources to keep it so.
pub fn lost_data(line: &str) -> bool {
    line.starts_with("save ")
}

/// What reaches the log: warnings and errors, and nothing of the HTTP client's, whose debug lines quote a request's headers,
/// an API key among them (spec #320, "API-key writers"), whatever level is asked for.
pub fn logged(metadata: &log::Metadata) -> bool {
    metadata.level() <= log::Level::Warn && !metadata.target().starts_with("ureq")
}

/// The target of the lines the page itself logs (`/api/log`).
pub(crate) const PAGE_TARGET: &str = "studio_server::page";

/// plotkeeper's logger: what is [`logged`] goes to stderr, with its level and target, and an error also to the page as a
/// `server-error` event with its message, which the notice line shows. An error logged while no page listens is only on stderr.
pub struct Logger(Events);

/// The one [`Logger`], which `plotkeeper` installs at start.
pub static LOGGER: Logger = Logger(Events(OnceLock::new()));

impl Logger {
    /// Sets the page the errors also go to; the first call wins.
    pub fn send_errors_to(&self, sink: Arc<dyn Sink>) {
        self.0.set(sink);
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        logged(metadata)
    }
    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // A stderr that cannot be written (a closed pipe, a gone terminal) loses the line: `eprintln!` would panic, and
        // with `exit_on_panic` that would stop plotkeeper, or abort it when the hook's own line panicked again.
        writeln!(
            io::stderr().lock(),
            "[{}][{}] {}",
            record.level(),
            record.target(),
            record.args()
        )
        .ok();
        // The page shows its own errors when it logs them, so they are not sent back to it.
        if record.level() == log::Level::Error && record.target() != PAGE_TARGET {
            let message = record.args().to_string();
            self.0
                .emit("server-error", serde_json::Value::String(message));
        }
    }
    fn flush(&self) {}
}

/// Makes a panic on any thread stop plotkeeper (spec danielrispler/plotkeeper#548): the hook logs the thread, the place
/// and the message as an error, then calls `exit` with a non-zero code. The binary passes `std::process::exit`, so no
/// thread goes on with a poisoned lock or without the read loop; a restart is safe, as the state is rebuilt from the
/// event log. A hook and not `panic = "abort"`, so a test that expects a panic still unwinds.
pub fn exit_on_panic(exit: impl Fn(i32) + Send + Sync + 'static) {
    std::panic::set_hook(Box::new(move |info| {
        let thread = thread::current();
        let name = thread.name().unwrap_or("unnamed");
        let place = info.location().map_or("an unknown place".to_string(), |l| {
            format!("{}:{}:{}", l.file(), l.line(), l.column())
        });
        let message = info.payload_as_str().unwrap_or("no message");
        log::error!("thread '{name}' panicked at {place}: {message}; plotkeeper stops");
        exit(PANIC_EXIT);
    }));
}

/// The exit code of a plotkeeper that panicked, the one Rust's own runtime uses.
const PANIC_EXIT: i32 = 101;

/// Logs a line from the writers' code: an error when it lost data, a warning otherwise.
pub fn report(line: String) {
    let line = scrubbed(&line);
    if lost_data(line) {
        log::error!("{line}");
    } else {
        log::warn!("{line}");
    }
}

/// Logs a card store change that could not be saved; it stays in memory until the next save.
pub fn unsaved(saved: io::Result<()>) {
    if let Err(e) = saved {
        log::error!("save the card store: {e}");
    }
}

/// The warning for an oversized turn, which the understanding layer hands its callers, goes to the Studio log.
pub fn warn_log(warning: String) {
    log::warn!("{warning}");
}

/// Sends each notice the card writer raises (or clears, as `None`) to the page as `writer-notice`, on a thread of its own that
/// ends with the sender. The log keeps only that one arrived or cleared: the notice is the writer's own words.
pub fn forward_writer_notices(app: &Arc<App>, notices: mpsc::Receiver<Option<String>>) {
    let app = app.clone();
    thread::spawn(move || {
        for notice in notices {
            log::warn!(
                "card writer notice {}",
                if notice.is_some() { "shown" } else { "cleared" }
            );
            *app.writer_notice() = notice.clone();
            app.events.emit("writer-notice", serde_json::json!(notice));
        }
    });
}

/// What the page asks at start (#527): plotkeeper's package version, for the "Updated to X.Y.Z" toast after an update.
#[derive(serde::Serialize)]
pub struct About {
    pub version: String,
}

pub fn about() -> About {
    About {
        version: env!("CARGO_PKG_VERSION").into(),
    }
}

/// Whether any harness is wired (#437), from install's record, read on each ask: the page shows "not recording: run
/// `plotkeeper install`" while none is, and asks again at its focus. No home folder counts as nothing wired.
pub fn wired() -> bool {
    std::env::home_dir().is_some_and(|home| install::any_wired(&home))
}

// The event log and what is kept beside it.

/// The worktree roots file beside the event log `log` (danielrispler/plotkeeper#132).
pub fn roots_file(log: &Path) -> PathBuf {
    log.with_file_name("roots.json")
}

/// The worktree root of each folder a launch before looked up, none when there is no file or it
/// does not read.
// shortcut: a root is kept for good, so a folder looked up before its own worktree existed keeps its
// parent's root; check the saved ones with `git` if a session ever shows under the wrong worktree.
pub fn saved_roots(log: &Path) -> HashMap<String, String> {
    let saved = std::fs::read(roots_file(log)).ok();
    let saved: HashMap<String, String> = saved
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    // An entry that maps a removed worktree's folder to another root is dropped, and the file
    // saved without it (danielrispler/plotkeeper#205).
    let live = live_roots(saved.clone());
    if live.len() != saved.len() {
        save_roots(log, &live);
    }
    live
}

/// Takes the state a launch before saved, when it fits the log, and returns the offset to replay the
/// log from: 0 when there is none (danielrispler/plotkeeper#134).
pub fn resume(studio: &Mutex<Studio>, log: &Path) -> u64 {
    let Ok(saved) = std::fs::read(studio_hook::state_file(log)) else {
        return 0;
    };
    let len = std::fs::metadata(log).map_or(0, |m| m.len());
    studio.lock().unwrap().resume(&saved, len)
}

/// Keeps the replayed state beside the log, through a new file that takes the old one's place.
pub fn save_state(log: &Path, state: &[u8]) {
    let saved = studio_workbench::write_atomic(&studio_hook::state_file(log), state);
    if let Err(e) = saved {
        log::error!("keep the replayed state: {e}");
    }
}

/// Keeps `roots` beside the log, through a new file that takes the old one's place.
pub fn save_roots(log: &Path, roots: &HashMap<String, String>) {
    let written = serde_json::to_vec(roots).map_err(io::Error::from);
    let saved = written.and_then(|bytes| studio_workbench::write_atomic(&roots_file(log), &bytes));
    if let Err(e) = saved {
        log::error!("keep the worktree roots: {e}");
    }
}

/// Appends `lines` to the event log in one write under the lock the hook takes, so they never
/// interleave with a hook's line.
pub fn append(lines: &[serde_json::Value]) -> Result<(), String> {
    let home = std::env::home_dir().ok_or("no home directory")?;
    append_to(&studio_hook::event_log(&home), lines)
}

/// Appends `lines` to the event log `log`, under its lock.
pub fn append_to(log: &Path, lines: &[serde_json::Value]) -> Result<(), String> {
    let locked = studio_hook::locked(log, Duration::from_secs(60));
    let mut log = locked
        .map_err(|e| e.to_string())?
        .ok_or("the event log stayed locked")?;
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    log.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

// The pull requests and the landed tasks, looked up with `gh`.

/// The longest a lookup at GitHub may take.
const GH_LIMIT: Duration = Duration::from_secs(30);

/// The `state` of the pull request or issue that `gh <args>` shows, run in `cwd`: `MERGED`, `CLOSED`
/// and the like; `None` when `gh` fails, is missing or is offline.
fn gh_state(gh: &str, cwd: &str, args: &[&str]) -> Option<String> {
    let mut gh = studio_workbench::run::gh_at(gh, Path::new(cwd));
    gh.args(args).args(["--json", "state", "--jq", ".state"]);
    let output = studio_workbench::run::run_until(&mut gh, GH_LIMIT).ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// What git and GitHub say about `task`, for **Landed**; every fact that cannot be found is false.
fn lookup_task(gh: &str, task: &TaskQuery) -> TaskFacts {
    let safe = studio_workbench::losses(Path::new(&task.path))
        .is_ok_and(|l| l.uncommitted.is_empty() && l.unpushed.is_empty());
    let merged = task.branch.as_deref().is_some_and(|branch| {
        gh_state(gh, &task.path, &["pr", "view", branch]).as_deref() == Some("MERGED")
    });
    let issue_closed = task.issue.is_some_and(|n| {
        gh_state(gh, &task.path, &["issue", "view", &n.to_string()]).as_deref() == Some("CLOSED")
    });
    TaskFacts {
        safe,
        merged,
        issue_closed,
    }
}

/// Looks up each task of the list, or only the one that holds `session`, on its own thread so the
/// read loop never waits on the network. Asked at a turn's end, at the window's focus and on the
/// palette's refresh, never on a timer.
pub fn refresh_landed(app: &Arc<App>, session: Option<String>) {
    let app = app.clone();
    thread::spawn(move || {
        let tasks = {
            let mut studio = app.core();
            studio.forget_gone_tasks();
            let mut tasks = studio.task_queries();
            if let Some(session) = session {
                let path = studio.task_of(&session);
                tasks.retain(|t| Some(&t.path) == path.as_ref());
            }
            drop(studio);
            tasks
        };
        let gh = studio_workbench::gh_path();
        for task in tasks {
            // A removed worktree has no facts.
            let facts = Path::new(&task.path)
                .exists()
                .then(|| lookup_task(gh, &task));
            let (before, changed) = {
                let mut studio = app.core();
                let before = studio.task_facts_at(&task.path);
                (before, studio.set_task_facts(&task.path, facts))
            };
            if map::became_merged(before, facts) {
                map::merged(&app.fetches, &app.ci, &app.events, &task.path);
            }
            if changed {
                app.list_changed();
            }
        }
    });
}

/// The window gained focus, or Daniel chose the palette's refresh: ask about every task again.
pub fn refresh_landed_tasks(app: &Arc<App>) {
    prs::ask_later(app, prs::Trigger::Focus, None);
    refresh_landed(app, None);
}

// The Map view's commands, over `map` and what it keeps.

/// The Map view's payload for `project`: see [`map::map_state`].
pub fn map_state(app: &Arc<App>, project: String, fetch: bool) -> Result<map::MapState, String> {
    map::map_state(project, fetch, app)
}

/// Start-up registration (spec #753, "Projects"): adds the project of `folder`, the one a `plotkeeper` started in, and sends the
/// open pages the saved list; returns the project, as [`projects::Projects::register`] does. A first start calls it in process, a
/// second through the API with the running server's token. The folder must be absolute: a relative one would be looked up from the
/// server's own.
pub fn register_folder(app: &App, folder: &str) -> Result<Option<String>, String> {
    if !Path::new(folder).is_absolute() {
        return Err(format!("{folder:?} is not an absolute folder"));
    }
    let project = app.projects.register(folder)?;
    let projects = app.projects.list();
    app.events
        .emit("projects", serde_json::to_value(&projects).unwrap());
    Ok(project)
}

/// The project header's quiet dot: see [`map::map_marks`].
pub fn map_marks(app: &App, projects: Vec<String>) -> Result<Vec<map::Mark>, String> {
    map::map_marks(projects, &app.ci, &app.prs)
}

#[cfg(test)]
mod tests;
