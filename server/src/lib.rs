//! The app's state and the functions behind its learning-layer and session-view commands, with no Tauri in them
//! (danielrispler/plotkeeper#321, spec #319 "The server"). The `plotkeeper` server holds one [`App`] and answers each
//! command with the function here that carries its name, over HTTP (`api`).
//!
//! The core state, the card store, the risk notes, the card writer, the turn writing and the notices are the [`App`]'s. The
//! settings are read and written here ([`settings`]), and so are the PR lookups ([`prs`], [`refresh_landed`]) and the PR briefs
//! ([`pr_briefs`]). What reaches the page leaves through [`Events`], which holds the web server's [`Sink`]; the read loop is [`follow`].
//! Workbench-only commands (terminals, projects, worktrees, skills, usage, restart) stay in the shell.

use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};
use studio_core::{Notifications, Session, State::Ended, Studio, TaskFacts, TaskQuery};
use studio_understanding::{
    CardStore, ClaudeWriter, Core, Effects, FakeWriter, OpenItem, RiskNotes, RiskRequest, RiskView,
    Row, SessionCards, StoredSummary, SummarySetup, TurnWriting, Writer, WriterKind, judge,
    live_roots, millis, open_items, turn_hunk, write_agent_summary,
};

pub mod answer;
pub mod api;
pub mod back_a_law;
pub mod ci_laws;
pub mod fog;
pub mod history;
pub mod hook_copies;
pub mod install;
pub mod lock;
pub mod map;
pub mod micro_worlds;
pub mod part_catch_ups;
pub mod pr_briefs;
pub mod pre_flight;
pub mod projects;
pub mod prs;
pub mod ratify;
mod read_loop;
pub mod runner;
pub mod settings;
pub mod staleness;
pub mod stop_rules;
pub mod story_bar;
pub mod web;

pub use read_loop::{Follower, Notice, ServerEffects, follow, listed, tell, uptime};

/// The card store, which the log thread adds to and the pane records guesses in.
pub type Cards = Arc<Mutex<CardStore>>;

/// The risk notes of blocked commands (danielrispler/plotkeeper#298), in a store of their own beside the card store: the log thread asks for
/// them, a thread of each judge keeps them, and the window's card read joins them to the cards.
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
        let _ = self.0.set(sink);
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

/// The landed sessions Daniel cleared whose undo window is still open (danielrispler/plotkeeper#244): the window tells the shell,
/// as `(session, terminal)`, so a quit inside the window can commit them.
#[derive(Default)]
pub struct Clearing(pub Mutex<Vec<Cleared>>);

/// A cleared session and the terminal it runs in.
#[derive(serde::Deserialize)]
pub struct Cleared {
    pub id: String,
    pub terminal: Option<String>,
}

/// Everything the process owns that the commands read and change.
pub struct App {
    pub core: Core,
    pub cards: Cards,
    pub notes: Notes,
    /// The card writer: `claude -p` or `agy`, as `summaries.writer` says, or the fake when `PLOTKEEPER_CARD_WRITER=fake`, for checks that must not run `claude -p`.
    pub writer: Writer,
    pub writing: TurnWriting,
    pub notices: Notices,
    pub clearing: Clearing,
    pub fetches: Arc<map::Fetches>,
    pub ci: Arc<ci_laws::CiCache>,
    /// The PRs found for each project's branches (`prs.rs`) and the PR briefs written for the ready ones (`pr_briefs.rs`).
    pub prs: prs::Prs,
    pub briefs: pr_briefs::PrBriefs,
    /// The part catch-ups being written and the ones that failed (`part_catch_ups.rs`).
    pub catch_ups: part_catch_ups::CatchUps,
    /// The saved project list (`projects.rs`), beside the log.
    pub projects: projects::Projects,
    pub watching: Mutex<answer::Watching>,
    pub events: Arc<Events>,
    /// The notice the card writer last raised and not yet cleared, for a page opened after it (danielrispler/plotkeeper#328).
    pub writer_notice: Mutex<Option<String>>,
    /// Whether summaries are written: off until a writer passes Test for a user who arrived with none (spec #320).
    pub summaries_on: AtomicBool,
    /// The list changed outside the read loop since it last sent it: a PR was found, or Daniel marked a session unseen.
    pub list_changed: AtomicBool,
    /// Whether the story rail shows, as the UI last said: not in focus mode, behind a closed overlay or under the skills
    /// browser, when a turn on it is not seen (danielrispler/Falinks#556).
    pub rail_shown: AtomicBool,
    /// The terminal the split's second pane shows, as the UI last said, `None` with no split: its session is on screen, so its
    /// turn's end raises no toast.
    pub split_shown: Mutex<Option<String>>,
    /// Where the API-key writers' keys are kept: the Keychain at start (`with_keys`), memory before; and the writers whose
    /// key is set, all the page learns of them.
    pub keys: Arc<dyn settings::ApiKeys>,
    pub key_set: Mutex<Vec<WriterKind>>,
    /// The background runs (`runner.rs`): their queues, their processes and their outcomes.
    pub runner: Arc<runner::Runner>,
    /// The micro-worlds (`micro_worlds.rs`): built by background runs, checked on the host and kept per project.
    pub worlds: Arc<micro_worlds::MicroWorlds>,
    /// Back a law (`back_a_law.rs`): its runs' proofs and the patches kept.
    pub laws: Arc<back_a_law::LawRuns>,
}

impl App {
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
        let writer: Writer = match fake {
            true => Arc::new(FakeWriter),
            false => {
                let writer = ClaudeWriter::beside(log)?;
                let writer_notices = Mutex::new(writer_notices);
                Arc::new(writer.on_availability(move |notice| {
                    let _ = writer_notices.lock().unwrap().send(notice);
                }))
            }
        };
        let app = Self::open_with(log, cards, notes, writer, effects)?;
        // The checks' fake writer keeps its key in memory, so no check reads the Keychain.
        Ok(match fake {
            true => app,
            false => app.with_keys(Arc::new(settings::Keychain::default())),
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
        // `claude` there is none, so no brief is attempted and the page says briefs need Claude (spec #320).
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
        let told = events.clone();
        // A run that ends may be a micro-world's, which is checked then.
        let building = Arc::downgrade(&worlds);
        runner.on_change(move |project| {
            told.emit("map-landed", project.into());
            if let Some(worlds) = building.upgrade() {
                worlds.runs_changed(project);
            }
        });
        let laws = back_a_law::LawRuns::new(runner.clone(), back_a_law::Config::beside(log));
        let told = events.clone();
        laws.on_change(move |project| told.emit("map-landed", project.into()));
        // A part catch-up's write that moves on to its next call draws its stage on the waiting card (#450).
        let told = events.clone();
        let catch_ups = part_catch_ups::CatchUps::telling(move |project| {
            told.emit("map-landed", project.into())
        });
        Ok(App {
            core,
            cards,
            notes,
            writer,
            writing,
            notices: Notices::default(),
            clearing: Clearing::default(),
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
            split_shown: Mutex::new(None),
            keys: Arc::new(settings::MemoryKeys::default()),
            key_set: Mutex::new(Vec::new()),
            runner,
            worlds,
            laws,
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
        eprintln!(
            "[{}][{}] {}",
            record.level(),
            record.target(),
            record.args()
        );
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

// The settings' commands.

/// The settings' `summaries` (danielrispler/Falinks#517, decision 17): the writer's model and effort, and whether a
/// summary is written when Daniel opens its turn instead of at the turn's end.
pub fn summary_settings(
    app: &App,
    model: String,
    effort: String,
    writer: WriterKind,
    on_open: bool,
) {
    app.writer.use_writer(writer);
    // A base URL edited in `settings.json` by hand takes effect with the page's next read of it.
    app.writer.use_base_url(settings::openai_base_url());
    app.writing.settings(
        SummarySetup {
            model,
            effort,
            writer,
        },
        on_open,
    );
}

/// Turns summaries on or off (spec #320, "The Test gate"): the writer's calls, and what the page's writer section reads.
pub fn set_summaries_on(app: &App, on: bool) {
    app.summaries_on.store(on, Ordering::Release);
    app.writer.summaries_on(on);
}

/// What the page's writer section lists (spec #320, "Writer settings on the page"): the writers the login shell's
/// `command -v` finds, whether summaries are on, and whether PR briefs can be written, which needs `claude`.
#[derive(serde::Serialize)]
pub struct Writers {
    pub writers: Vec<studio_understanding::FoundWriter>,
    pub summaries_on: bool,
    pub briefs: bool,
    /// The API-key writers whose key is set; never a key.
    pub keys: Vec<WriterKind>,
    /// The OpenAI-compatible writer's base URL, from `settings.json`; none: OpenAI's own.
    pub base_url: Option<String>,
}

pub fn writers(app: &App) -> Writers {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let mut writers = studio_understanding::found_writers(shell.as_ref());
    let keys = app.key_set.lock().unwrap().clone();
    let base_url = settings::openai_base_url();
    // An API-key writer is found when its key is set, the OpenAI-compatible one also when its base URL is; the key itself
    // never reaches the page.
    for writer in writers.iter_mut().filter(|w| w.kind.http()) {
        writer.found = keys.contains(&writer.kind)
            || (writer.kind == WriterKind::OpenAI && base_url.is_some());
    }
    Writers {
        writers,
        summaries_on: app.summaries_on.load(Ordering::Acquire),
        briefs: app.briefs.writes(),
        keys,
        base_url,
    }
}

/// A Test's outcome: `passed` or the failure's kind, the line the page shows, and `settings.json`'s new text when the pass
/// saved the choice.
#[derive(serde::Serialize)]
pub struct WriterTest {
    pub outcome: &'static str,
    pub line: String,
    pub settings: Option<String>,
}

/// The page's Test of `writer` at `model` and `effort`: one tiny call. A pass saves the choice in `settings.json` (`expected`
/// is the text the page last read) and, once saved, turns summaries on, now and at every start; a failure changes nothing.
pub fn test_writer(
    app: &App,
    writer: WriterKind,
    model: String,
    effort: String,
    expected: Option<String>,
) -> WriterTest {
    use studio_understanding::FailureKind;
    if let Err(error) = app.writer.test(writer, &model, &effort) {
        let outcome = match error.kind {
            FailureKind::NotLoggedIn => "not_logged_in",
            FailureKind::Quota => "quota",
            FailureKind::Timeout => "timeout",
            FailureKind::NotInstalled => "not_installed",
            FailureKind::Empty => "empty",
            FailureKind::Other => "other",
        };
        return WriterTest {
            outcome,
            line: error.reason(),
            settings: None,
        };
    }
    let saved = settings::save_writer(expected, writer, &model, &effort).and_then(|text| {
        let [file, _] = settings::paths().ok_or("no home folder")?;
        settings::open_summaries(file.parent().unwrap_or(Path::new("")))?;
        Ok(text)
    });
    // Only a saved choice opens summaries: one not saved would write with a writer the next start does not know.
    if saved.is_ok() {
        set_summaries_on(app, true);
    }
    let name = writer.program();
    match saved {
        Ok(text) => WriterTest {
            outcome: "passed",
            line: format!("Passed: `{name}` answered. Summaries are written with it."),
            settings: Some(text),
        },
        Err(e) => WriterTest {
            outcome: "passed",
            line: format!("Passed: `{name}` answered, but the choice was not saved: {e}"),
            settings: None,
        },
    }
}

/// Saves what the page set for API-key writer `writer` (spec #320, "API-key writers"): the key pasted, in the Keychain,
/// replacing the one set, and given to the writer; for the OpenAI-compatible writer (#367), also its base URL, in
/// `settings.json` (`expected` is the text the page last read; empty: OpenAI's own), and its key is optional. Returns
/// `settings.json`'s new text when it changed. The key is never sent back, logged or written anywhere else.
pub fn set_api_key(
    app: &App,
    writer: WriterKind,
    key: Option<String>,
    base_url: Option<String>,
    expected: Option<String>,
) -> Result<Option<String>, String> {
    if !writer.http() {
        return Err(format!("`{}` takes no API key", writer.program()));
    }
    let key = key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    let base_url = base_url.filter(|_| writer == WriterKind::OpenAI);
    let url = base_url.as_deref().map(str::trim);
    if let Some(url) = url
        && !url.is_empty()
        && !url.starts_with("http://")
        && !url.starts_with("https://")
    {
        return Err(
            "The base URL starts with http:// or https://, such as http://localhost:11434/v1."
                .into(),
        );
    }
    // The key first: one the store refuses leaves `settings.json` as the page read it.
    if let Some(key) = &key {
        app.keys.set(writer, key)?;
        app.writer.use_key(writer, Some(key.clone()));
        let mut set = app.key_set.lock().unwrap();
        if !set.contains(&writer) {
            set.push(writer);
        }
    }
    let Some(url) = url else { return Ok(None) };
    let text = settings::save_base_url(expected, Some(url).filter(|url| !url.is_empty()))?;
    app.writer.use_base_url(settings::openai_base_url());
    Ok(Some(text))
}

/// Removes API-key writer `writer`'s key from the Keychain and from the writer, whose calls then fail as having no key; for
/// the OpenAI-compatible writer, its base URL too. Returns `settings.json`'s new text when it changed.
pub fn remove_api_key(
    app: &App,
    writer: WriterKind,
    expected: Option<String>,
) -> Result<Option<String>, String> {
    app.keys.remove(writer)?;
    app.writer.use_key(writer, None);
    app.key_set.lock().unwrap().retain(|&kind| kind != writer);
    if writer != WriterKind::OpenAI {
        return Ok(None);
    }
    let text = settings::save_base_url(expected, None)?;
    app.writer.use_base_url(None);
    Ok(Some(text))
}

/// The settings' `checks` (danielrispler/plotkeeper#112): each project folder's list of commands that are checks.
pub fn check_lists(app: &App, lists: HashMap<String, Vec<String>>) {
    app.core.lock().unwrap().set_check_lists(lists);
    app.list_changed();
}

// The turn's commands.

/// A check run's exit code and output tail (danielrispler/plotkeeper#112), read from the event log through
/// its ledger row's offset, off the studio's lock. `None` when the session has no such call.
pub fn check_output(
    app: &App,
    session: String,
    agent: Option<String>,
    call: String,
) -> Result<Option<studio_core::CheckOutput>, String> {
    let row = {
        let studio = app.core.lock().unwrap();
        let ledger = &studio.session(&session).map(|s| &s.ledger);
        ledger.and_then(|l| l.row(agent.as_deref(), &call)).cloned()
    };
    let Some(row) = row else { return Ok(None) };
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    studio_core::check_output(&row, &log)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The recorded facts of an Ask anchor (danielrispler/plotkeeper#125): from the ledger and the event log through its
/// offsets, with no writer in the call. `None` when the session or the thing is not known.
pub fn ask_facts(
    app: &App,
    session: String,
    anchor: studio_core::Anchor,
) -> Result<Option<studio_core::AskFacts>, String> {
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    // The lock is held only to copy what the studio holds: the log is read after it is freed.
    let recorded = app.core.lock().unwrap().ask_recorded(&session, &anchor);
    Ok(recorded.map(|r| r.read(&log)))
}

/// A file's hunk of turn `turn` (danielrispler/plotkeeper#114), whole for the diff pane (#131) when `all`: `git diff` between the turn's two trees of worktree
/// `root`, off the studio's lock. `None` when the turn has no trees there, or `git` cannot read them.
pub fn turn_file_hunk(
    app: &App,
    session: String,
    turn: usize,
    root: String,
    path: String,
    from: Option<String>,
    all: Option<bool>,
) -> Option<studio_understanding::Hunk> {
    let (start, end) = app.core.lock().unwrap().turn_trees(&session, turn, &root)?;
    turn_hunk(
        Path::new(&root),
        &start,
        end.as_deref(),
        &path,
        from.as_deref(),
        all.unwrap_or(false),
    )
}

/// Daniel opened turn `id` (danielrispler/Falinks#526): its summary, written now when it waits for him, which takes
/// seconds. `None` when the turn has none. A failed write keeps the turn waiting and is an error, so the window asks again
/// only at the next opening.
pub fn turn_summary(app: &App, id: String) -> Result<Option<StoredSummary>, String> {
    app.writing.opened(&id)
}

/// The whole final reply of turn `index` of session `id` (danielrispler/Falinks#574), which the rail fetches when Daniel
/// opens that turn's folded response; the session list never carries it.
pub fn turn_reply(app: &App, id: String, index: usize) -> Option<String> {
    app.core.lock().unwrap().turn_reply(&id, index)
}

/// Daniel opened subagent `agent_id` (danielrispler/Falinks#535): the summary of its purpose and result, written now
/// when it has none and has ended, which takes seconds.
/// `None` while it runs or when the call failed, and the rail keeps its first line.
pub fn agent_summary(app: &App, agent_id: String) -> Option<String> {
    let (prompt, message) = {
        let studio = app.core.lock().unwrap();
        let mut agents = studio.sessions().into_iter().flat_map(|s| &s.agents);
        let agent = agents.find(|a| a.agent.id == agent_id && a.ended_at.is_some())?;
        (agent.prompt.clone(), agent.message.clone())
    };
    // A subagent that ended without a last message (killed, or its session ended) has no result to sum up.
    if message.is_empty() {
        return None;
    }
    let setup = app.writing.setup();
    write_agent_summary(
        &app.cards,
        &*app.writer,
        &setup,
        &agent_id,
        &prompt,
        &message,
        report,
    )
}

// The session view's commands.

/// The session list, in its order.
pub fn sessions(app: &App) -> Vec<Row> {
    let studio = app.core.lock().unwrap();
    let now = studio.now();
    let list: Vec<Session> = studio.sessions().into_iter().cloned().collect();
    let landed: HashSet<String> = (list.iter())
        .filter(|s| studio.landed(s))
        .map(|s| s.agent.id.clone())
        .collect();
    drop(studio);
    let cards = app.cards.lock().unwrap();
    let row = |mut session: Session| Row {
        line: cards.line(&session, now),
        session: {
            session.landed = landed.contains(&session.agent.id);
            session
        },
    };
    list.into_iter().map(row).collect()
}

/// Daniel clicked session `id`'s row, or, with no `id`, went back to the bare map; returns the selection,
/// which no longer names a subagent, and is `null` with none selected.
pub fn select(app: &App, id: Option<String>) -> serde_json::Value {
    let mut studio = app.core.lock().unwrap();
    match id {
        Some(id) => studio.select(&id),
        None => studio.select_none(),
    }
    selection(&studio)
}

/// Daniel started a session in studio terminal `terminal`; returns the selection, `null` until the
/// session's first event names that terminal and selects it (danielrispler/Falinks#567).
pub fn select_terminal(app: &App, terminal: String) -> serde_json::Value {
    let mut studio = app.core.lock().unwrap();
    studio.select_terminal(&terminal);
    selection(&studio)
}

/// Daniel opened subagent `agent_id`'s line; returns the selection, which names it. The card store
/// keeps the opening of a failed line, so it stays faded after a restart.
pub fn open(app: &App, agent_id: String) -> serde_json::Value {
    let mut studio = app.core.lock().unwrap();
    if studio.open(&agent_id) {
        unsaved(app.cards.lock().unwrap().opened(&agent_id));
    }
    selection(&studio)
}

/// The log of subagent `agent_id` of session `session` from byte `offset`: its entries after it and
/// the offset read to, `null` when there is no log (danielrispler/plotkeeper#54). Not pushed on events: the pane
/// asks when the selection changes, and reads on from the offset it kept.
pub fn subagent_log(
    app: &App,
    session: String,
    agent_id: String,
    offset: u64,
) -> Option<studio_core::SubagentLog> {
    subagent_log_through(app, session, agent_id, offset, |file| file.read())
}

/// `subagent_log` with the file read by `read`, which runs with no lock on the studio held (spec
/// danielrispler/plotkeeper#548): the lock is held only to copy where the log is. The seam its test reads through.
pub fn subagent_log_through(
    app: &App,
    session: String,
    agent_id: String,
    offset: u64,
    read: impl FnOnce(&studio_core::SubagentLogFile) -> Option<studio_core::SubagentLog>,
) -> Option<studio_core::SubagentLog> {
    let file = {
        let studio = app.core.lock().unwrap();
        studio.subagent_log_file(&session, &agent_id, offset)?
    };
    read(&file)
}

/// The session Daniel selected and the subagent of it he opened, or `null` before his first click.
pub fn selected(app: &App) -> serde_json::Value {
    selection(&app.core.lock().unwrap())
}

fn selection(studio: &Studio) -> serde_json::Value {
    serde_json::to_value(studio.selection()).expect("selection serializes")
}

/// Daniel renamed session `id`: a `StudioRename` line in the event log, which the log thread then
/// applies and a restart replays (danielrispler/Falinks#469). An empty `name` gives the row its folder's again.
pub fn rename(id: String, name: String) -> Result<(), String> {
    append(&[serde_json::json!({
        "session_id": id,
        "hook_event_name": studio_core::RENAME,
        "studio_name": name.trim(),
        "studio_received_ms": millis(),
    })])
}

/// Daniel deleted session `id`: a `StudioDelete` line in the event log, which hides its row now and
/// after a restart.
pub fn delete_session(id: String) -> Result<(), String> {
    append(&[serde_json::json!({
        "session_id": id,
        "hook_event_name": studio_core::DELETE,
        "studio_received_ms": millis(),
    })])
}

/// Replaces the cleared sessions pending their delete; an empty list after Undo or after the window's commit.
pub fn clearing(app: &App, sessions: Vec<Cleared>) {
    *app.clearing.0.lock().unwrap() = sessions;
}

/// What committing the cleared `sessions` sends: the daemon's `kill` and `forget` of each terminal, and a `StudioDelete`
/// line for each session, as Delete does. The cards stay in the store.
/// `ends` gives the lines that end the sessions of a terminal, as closing its pane does (`closed`), which come first.
pub fn clear_commit(
    sessions: &[Cleared],
    in_use: impl Fn(&str, &[Cleared]) -> bool,
    ends: impl Fn(&str) -> Vec<serde_json::Value>,
) -> (Vec<serde_json::Value>, Vec<serde_json::Value>) {
    let mut requests = Vec::new();
    let mut lines = Vec::new();
    let mut ended = HashSet::new();
    for terminal in sessions.iter().filter_map(|s| s.terminal.as_ref()) {
        // Each terminal once, and never one a session that is not being cleared still uses.
        if in_use(terminal, sessions) || !ended.insert(terminal) {
            continue;
        }
        requests.push(serde_json::json!({ "op": "kill", "id": terminal }));
        requests.push(serde_json::json!({ "op": "forget", "id": terminal }));
        lines.extend(ends(terminal));
    }
    lines.extend(sessions.iter().map(|s| {
        serde_json::json!({
            "session_id": s.id,
            "hook_event_name": studio_core::DELETE,
            "studio_received_ms": millis(),
        })
    }));
    (requests, lines)
}

/// The cleared `pending` sessions as they are now, from `current(id)`, which gives whether the session is still landed and the terminal
/// it runs in now: a session that is gone or no longer landed is dropped, and each keeps its current terminal.
pub fn resolve_clearing(
    pending: Vec<Cleared>,
    current: impl Fn(&str) -> Option<(bool, Option<String>)>,
) -> Vec<Cleared> {
    (pending.into_iter())
        .filter_map(|cleared| {
            let (landed, terminal) = current(&cleared.id)?;
            landed.then_some(Cleared {
                id: cleared.id,
                terminal,
            })
        })
        .collect()
}

/// Daniel closed terminal `terminal` (danielrispler/Falinks#461, story 15 and decision 8): a `SessionEnd` line in the event
/// log for each session in it that has not ended, as its `claude` may die before its own hook runs.
/// A real `SessionEnd` after it changes nothing.
pub fn closed(app: &App, terminal: String) -> Result<(), String> {
    append(&session_ends(&app.core, &terminal))
}

/// The `SessionEnd` lines for the sessions of `terminal` that have not ended.
pub fn session_ends(studio: &Mutex<Studio>, terminal: &str) -> Vec<serde_json::Value> {
    studio
        .lock()
        .unwrap()
        .sessions()
        .into_iter()
        .filter(|s| s.terminal.as_deref() == Some(terminal) && s.state != Ended)
        .filter(|s| !s.status_only)
        .map(|s| {
            serde_json::json!({
                "session_id": s.resume_id,
                "hook_event_name": "SessionEnd",
                "reason": "studio_close",
                "studio_received_ms": millis(),
            })
        })
        .collect()
}

/// How often the follower checks each session's agent process (#330).
pub const PROCESS_CHECK: Duration = Duration::from_secs(30);

/// The `SessionEnd` lines for the sessions whose agent process `alive` no longer lists (#330, spec #319,
/// "Session ending"). A session with no pid is never listed.
pub fn process_ends(studio: &Mutex<Studio>, alive: impl Fn(u32) -> bool) -> Vec<serde_json::Value> {
    let gone = studio.lock().unwrap().gone(alive);
    gone.into_iter()
        .map(|id| {
            serde_json::json!({
                "session_id": id,
                "hook_event_name": "SessionEnd",
                "reason": "process_gone",
                "studio_received_ms": millis(),
            })
        })
        .collect()
}

/// Daniel marked session `id` unseen (danielrispler/Falinks#556), from the rail's menu.
pub fn mark_unseen(app: &App, id: String) {
    if let Some((id, turns)) = app.core.lock().unwrap().mark_unseen(&id) {
        unsaved(app.cards.lock().unwrap().seen(&id, turns));
        app.list_changed();
    }
}

/// The UI's rail showed or hid.
pub fn rail_shown(app: &App, shown: bool) {
    app.rail_shown.store(shown, Ordering::Release);
}

/// The UI's split opened, closed or pinned another terminal.
pub fn split_shown(app: &App, terminal: Option<String>) {
    *app.split_shown.lock().unwrap() = terminal;
}

// The session's brief and its risk notes.

/// The brief, decision history and risk notes of the selected session, or of the worker of it Daniel opened.
pub fn cards(app: &App) -> SessionCards {
    let studio = app.core.lock().unwrap();
    let Some(owner) = studio.viewed() else {
        return SessionCards::default();
    };
    let cards = app.cards.lock().unwrap();
    let session = studio.session(owner);
    let mut shown = cards.session_cards(owner);
    // The Open slot is the session's: a worker's view has none.
    if let Some(session) = session {
        shown.open = open_items(session, studio.task_facts(owner), &cards);
    }
    // Each blocked command's note is joined here, so the core's type holds none.
    join_notes(&studio, &app.notes.lock().unwrap(), &mut shown);
    shown
}

/// The session whose blocked commands the window docks: the selected one, also while Daniel views a worker of it, whose
/// own id is no session's.
pub fn noted_session(studio: &Studio) -> Option<&Session> {
    studio.selected()
}

/// The risk notes the window shows for its session's cards and its decision history, which stay empty with no session.
fn risk_views(studio: &Studio, notes: &RiskNotes) -> (Vec<RiskView>, Vec<RiskView>) {
    noted_session(studio).map_or_default(|s| (notes.views(s), notes.decision_views(s)))
}

/// Joins the cards of the session the window shows to their risk notes, in `shown`.
pub fn join_notes(studio: &Studio, notes: &RiskNotes, shown: &mut SessionCards) {
    (shown.risk_notes, shown.decision_notes) = risk_views(studio, notes);
}

/// What the session's brief was built for.
#[derive(PartialEq)]
pub struct CardsKey {
    viewed: Option<String>,
    revision: u64,
    open: Vec<OpenItem>,
    /// What the window shows of the risk notes: a note arriving or failing changes nothing stored.
    notes: (Vec<RiskView>, Vec<RiskView>),
}

/// The key of the cards the window would be sent now.
pub fn cards_key(studio: &Studio, cards: &CardStore, notes: &RiskNotes) -> CardsKey {
    let viewed = studio.viewed();
    let session = viewed.and_then(|id| studio.session(id));
    // The Open slot moves with the core's state, not only with the store's.
    let open = session.map(|s| open_items(s, studio.task_facts(&s.agent.id), cards));
    CardsKey {
        viewed: viewed.map(String::from),
        revision: cards.revision(),
        open: open.unwrap_or_default(),
        notes: risk_views(studio, notes),
    }
}

/// The key of the cards the window would be sent now, from the state's own locks.
pub fn current_cards_key(app: &App) -> CardsKey {
    let studio = app.core.lock().unwrap();
    let cards = app.cards.lock().unwrap();
    let notes = app.notes.lock().unwrap();
    cards_key(&studio, &cards, &notes)
}

/// The risk notes the selected session wants asked for (danielrispler/plotkeeper#298), none with no session.
pub fn wanted_notes(app: &App) -> Vec<RiskRequest> {
    let studio = app.core.lock().unwrap();
    let shown = noted_session(&studio);
    shown
        .map(|session| app.notes.lock().unwrap().asks(session))
        .unwrap_or_default()
}

/// Starts the judge on each of `asks`, the risk notes a session wants (danielrispler/plotkeeper#298; `RiskNotes::asks`): a block that arrives
/// live, or a card with buttons that is shown and has none, once per command per run of the app. Each call runs on a thread of its
/// own, off the read loop and outside the turn-writing queue, as a side question does; the card shows with its buttons meanwhile.
pub fn ask_notes(app: &App, asks: Vec<RiskRequest>) {
    for ask in asks {
        let notes: Notes = app.notes.clone();
        let writer: Writer = app.writer.clone();
        thread::spawn(move || judge(&notes, &*writer, &ask, millis, report));
    }
}

/// Daniel's side `question` on `item`, a response, card or intention of session `session` (danielrispler/Falinks#461,
/// decision 20): the writer's answer, read with the turn's intentions, or why there is none, which
/// the window shows with a retry. It takes seconds; nothing reaches the session.
pub fn ask(app: &App, session: String, item: String, question: String) -> Result<String, String> {
    // The lock ends with the statement, before the writer runs.
    let transcript = app
        .core
        .lock()
        .unwrap()
        .session(&session)
        .map(|s| s.transcript.clone());
    let answer = studio_understanding::ask_about(
        &*app.writer,
        &transcript.unwrap_or_default(),
        &item,
        &question,
    );
    if let Err(line) = &answer {
        log::error!("no answer on {session}: {}", scrubbed(line));
    }
    answer
}

/// Daniel's `question` on an Ask anchor (danielrispler/plotkeeper#126): the writer's answer, read with the anchor's recorded
/// facts and, for a file, its hunk. The facts stay above it in the window; this is the interpretation.
pub fn ask_anchor(
    app: &App,
    session: String,
    anchor: studio_core::Anchor,
    what: String,
    question: String,
) -> Result<String, String> {
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    let (recorded, transcript, trees) = {
        let studio = app.core.lock().unwrap();
        let trees = match &anchor {
            studio_core::Anchor::File { turn, root, .. } => {
                studio.turn_trees(&session, *turn, root)
            }
            _ => None,
        };
        let transcript = studio.session(&session).map(|s| s.transcript.clone());
        (studio.ask_recorded(&session, &anchor), transcript, trees)
    };
    // The event log is read with the studio's lock freed.
    let facts = recorded.ok_or("nothing is recorded about this")?.read(&log);
    let hunk = match (&anchor, trees) {
        (
            studio_core::Anchor::File {
                root, path, from, ..
            },
            Some((start, end)),
        ) => turn_hunk(
            Path::new(root),
            &start,
            end.as_deref(),
            path,
            from.as_deref(),
            false,
        )
        .filter(|h| !h.binary)
        .map(|h| {
            format!(
                "{}{}",
                h.text,
                if h.left_out > 0 {
                    format!("... {} more lines", h.left_out)
                } else {
                    String::new()
                }
            )
        }),
        _ => None,
    };
    let item = studio_understanding::ask_item(&what, &facts, hunk.as_deref());
    let answer = studio_understanding::ask_about(
        &*app.writer,
        &transcript.unwrap_or_default(),
        &item,
        &question,
    );
    if let Err(line) = &answer {
        log::error!("no answer on {session}: {}", scrubbed(line));
    }
    answer
}

/// "Why didn't it touch `name`?" in turn `turn` (danielrispler/plotkeeper#126): what was recorded about `name`, and the
/// writer's possible reasons only when something was.
pub fn ask_why_not(
    app: &App,
    session: String,
    turn: usize,
    name: String,
) -> Result<studio_understanding::WhyNot, String> {
    let (facts, transcript) = {
        let studio = app.core.lock().unwrap();
        (
            studio.ask_named(&session, turn, &name),
            studio.session(&session).map(|s| s.transcript.clone()),
        )
    };
    let facts = facts.ok_or("this turn is not known")?;
    studio_understanding::why_not(&*app.writer, &transcript.unwrap_or_default(), &name, facts)
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
            *app.writer_notice.lock().unwrap() = notice.clone();
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
    let file = studio_hook::state_file(log);
    let next = file.with_extension("json.next");
    let saved = std::fs::write(&next, state).and_then(|()| std::fs::rename(&next, &file));
    if let Err(e) = saved {
        log::error!("keep the replayed state: {e}");
    }
}

/// Keeps `roots` beside the log, through a new file that takes the old one's place.
pub fn save_roots(log: &Path, roots: &HashMap<String, String>) {
    let next = roots_file(log).with_extension("json.next");
    let written = serde_json::to_vec(roots).map_err(io::Error::from);
    let saved = written.and_then(|bytes| std::fs::write(&next, bytes));
    if let Err(e) = saved.and_then(|()| std::fs::rename(&next, roots_file(log))) {
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

/// The `state` of the pull request or issue that `gh <args>` shows, run in `cwd`: `MERGED`, `CLOSED`
/// and the like; `None` when `gh` fails, is missing or is offline.
fn gh_state(gh: &str, cwd: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(gh)
        .args(args)
        .args(["--json", "state", "--jq", ".state"])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
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
            let mut studio = app.core.lock().unwrap();
            studio.forget_gone_tasks();
            let mut tasks = studio.task_queries();
            if let Some(session) = session {
                let path = studio.task_of(&session);
                tasks.retain(|t| Some(&t.path) == path.as_ref());
            }
            tasks
        };
        let gh = studio_workbench::gh_path();
        for task in tasks {
            // A removed worktree has no facts.
            let facts = Path::new(&task.path)
                .exists()
                .then(|| lookup_task(gh, &task));
            let (before, changed) = {
                let mut studio = app.core.lock().unwrap();
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

/// The project header's quiet dot: see [`map::map_marks`].
pub fn map_marks(app: &App, projects: Vec<String>) -> Result<Vec<map::Mark>, String> {
    map::map_marks(projects, &app.ci, &app.prs)
}

/// Opens the sessions' notices: `sounds` is the setting from `settings.json`, which the window parses.
pub fn sounds(app: &App, on: bool) {
    app.notices.lock().unwrap().sounds = on;
}

#[cfg(test)]
mod tests {
    use super::{cards_key, join_notes, lost_data, saved_roots, scrubbed};

    #[test]
    fn a_second_card_of_a_judged_command_changes_what_the_window_is_sent() {
        let line = |name: &str, extra: serde_json::Value| {
            let mut line =
                serde_json::json!({ "session_id": "s1", "hook_event_name": name, "cwd": "/repo" });
            line.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            line.to_string()
        };
        let block = |call: &str, agent: &str| {
            line(
                "PermissionDenied",
                serde_json::json!({ "agent_id": agent, "tool_name": "Bash", "tool_use_id": call, "prompt_id": "p-1",
                    "tool_input": { "command": "git push" }, "reason": "[Unauthorized Access]", "permission_mode": "auto" }),
            )
        };
        let mut studio = studio_core::Studio::replayed([
            line("SessionStart", serde_json::json!({ "source": "startup" })),
            line(
                "UserPromptSubmit",
                serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
            ),
            block("toolu_1", "w1"),
        ]);
        studio.select("s1");
        let dir = tempfile::tempdir().unwrap();
        let cards = studio_understanding::CardStore::open(dir.path().join("cards.json")).unwrap();
        let mut notes =
            studio_understanding::RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();
        let ask = notes.asks(studio.selected().unwrap()).remove(0);
        let judged =
            studio_understanding::Risk::from_reply(r#"{"level": "high", "line": "Pushes."}"#);
        notes.keep(&ask, judged, 5_000).unwrap();
        let before = cards_key(&studio, &cards, &notes);

        // Another card of the same command and folder: its note is stored, so nothing is asked and no note is kept.
        studio.replay(&block("toolu_2", "w2"));
        assert!(notes.asks(studio.selected().unwrap()).is_empty());

        assert!(cards_key(&studio, &cards, &notes) != before);
    }

    #[test]
    fn a_workers_view_joins_the_notes_and_asks_of_its_sessions_blocked_commands() {
        let line = |name: &str, extra: serde_json::Value| {
            let mut line =
                serde_json::json!({ "session_id": "s1", "hook_event_name": name, "cwd": "/repo" });
            line.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            line.to_string()
        };
        let mut studio = studio_core::Studio::replayed([
            line("SessionStart", serde_json::json!({ "source": "startup" })),
            line(
                "UserPromptSubmit",
                serde_json::json!({ "prompt": "Go.", "prompt_id": "p-1" }),
            ),
            line(
                "PermissionDenied",
                serde_json::json!({ "agent_id": "w1", "tool_name": "Bash", "tool_use_id": "toolu_1", "prompt_id": "p-1",
                    "tool_input": { "command": "git push" }, "reason": "[Unauthorized Access]", "permission_mode": "auto" }),
            ),
        ]);
        studio.select("s1");
        studio.open("w1");
        // Daniel views the worker: its id is no session's, so the session is the selected one.
        assert!(studio.viewed() == Some("w1"));
        let dir = tempfile::tempdir().unwrap();
        let mut notes =
            studio_understanding::RiskNotes::open(dir.path().join("risk-notes.json")).unwrap();

        let asks = super::noted_session(&studio).map(|session| notes.asks(session));
        assert_eq!(asks.map(|a| a.len()), Some(1));
        let mut shown = studio_understanding::SessionCards::default();
        join_notes(&studio, &notes, &mut shown);
        assert_eq!(shown.risk_notes.len(), 1);
    }

    #[test]
    fn a_session_whose_agent_process_is_gone_gets_a_synthetic_end_and_ends() {
        let line = |session: &str, name: &str, at: u64, pid: Option<u32>| {
            let mut line = serde_json::json!({
                "session_id": session, "hook_event_name": name, "cwd": "/x", "studio_received_ms": at,
            });
            if let Some(pid) = pid {
                line["studio_pid"] = pid.into();
            }
            line.to_string()
        };
        let lines = [
            line("dead", "SessionStart", 1_793_262_000_000, Some(4001)),
            line("live", "SessionStart", 1_793_262_000_001, Some(4002)),
            line("unwatched", "SessionStart", 1_793_262_000_002, None),
        ];
        let studio = std::sync::Mutex::new(studio_core::Studio::replayed(&lines));
        let alive = |pid: u32| pid == 4002;
        let ends = super::process_ends(&studio, alive);
        let ended: Vec<_> = ends
            .iter()
            .map(|l| {
                (
                    l["session_id"].as_str().unwrap(),
                    l["hook_event_name"].as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(ended, [("dead", "SessionEnd")]);
        assert!(ends[0]["studio_received_ms"].as_u64().unwrap() > 0);
        // The lines go in the log, and the next read ends the session.
        for end in &ends {
            studio.lock().unwrap().replay(&end.to_string());
        }
        assert_eq!(
            studio.lock().unwrap().session("dead").unwrap().state,
            studio_core::State::Ended
        );
        assert!(super::process_ends(&studio, alive).is_empty());
    }

    #[test]
    fn a_quit_inside_the_clear_window_kills_each_terminal_and_deletes_each_session() {
        let cleared = |id: &str, terminal: Option<&str>| super::Cleared {
            id: id.into(),
            terminal: terminal.map(Into::into),
        };
        // The sessions that run in terminal t1 and have not ended: the same lines the window's close of the pane writes.
        let ends = |terminal: &str| {
            vec![serde_json::json!({
                "session_id": format!("resume-{terminal}"),
                "hook_event_name": "SessionEnd",
            })]
        };
        let (requests, lines) = super::clear_commit(
            &[cleared("a", Some("t1")), cleared("b", None)],
            |_, _| false,
            ends,
        );
        assert_eq!(
            requests,
            vec![
                serde_json::json!({ "op": "kill", "id": "t1" }),
                serde_json::json!({ "op": "forget", "id": "t1" }),
            ]
        );
        let deleted: Vec<_> = lines
            .iter()
            .map(|l| {
                (
                    l["session_id"].as_str().unwrap(),
                    l["hook_event_name"].as_str().unwrap(),
                )
            })
            .collect();
        // Its sessions end first, as Delete's closing of the pane does, then each cleared session is deleted.
        assert_eq!(
            deleted,
            [
                ("resume-t1", "SessionEnd"),
                ("a", studio_core::DELETE),
                ("b", studio_core::DELETE)
            ]
        );
        assert!(
            super::clear_commit(&[], |_, _| false, |_| vec![])
                .1
                .is_empty()
        );
    }

    fn cleared(id: &str, terminal: Option<&str>) -> super::Cleared {
        super::Cleared {
            id: id.into(),
            terminal: terminal.map(Into::into),
        }
    }

    #[test]
    fn two_cleared_sessions_in_one_terminal_end_it_once() {
        let ends = |t: &str| vec![serde_json::json!({ "session_id": format!("r-{t}") })];
        let (requests, lines) = super::clear_commit(
            &[cleared("a", Some("t1")), cleared("b", Some("t1"))],
            |_, _| false,
            ends,
        );
        assert_eq!(requests.len(), 2);
        // One session end, then the two deletes.
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn a_terminal_a_session_not_being_cleared_uses_is_not_killed_but_its_cleared_session_is_still_deleted()
     {
        let ends = |t: &str| vec![serde_json::json!({ "session_id": format!("r-{t}") })];
        let (requests, lines) = super::clear_commit(
            &[cleared("a", Some("t1")), cleared("b", Some("t2"))],
            |t, _| t == "t1",
            ends,
        );
        assert_eq!(
            requests,
            vec![
                serde_json::json!({ "op": "kill", "id": "t2" }),
                serde_json::json!({ "op": "forget", "id": "t2" }),
            ]
        );
        let ids: Vec<_> = lines
            .iter()
            .map(|l| l["session_id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["r-t2", "a", "b"]);
    }

    #[test]
    fn a_quit_commits_only_sessions_still_landed_each_in_the_terminal_it_has_now() {
        let current = |id: &str| match id {
            "landed" => Some((true, Some("t9".to_string()))),
            "moved on" => Some((false, Some("t2".to_string()))),
            _ => None,
        };
        let pending = vec![
            cleared("landed", None),
            cleared("moved on", Some("t2")),
            cleared("gone", Some("t3")),
        ];
        let kept = super::resolve_clearing(pending, current);
        assert_eq!(kept.len(), 1);
        assert_eq!(
            (kept[0].id.as_str(), kept[0].terminal.as_deref()),
            ("landed", Some("t9"))
        );
    }

    #[test]
    fn an_event_reaches_what_the_shell_set_and_is_dropped_before() {
        let events = super::Events::default();
        events.emit("early", serde_json::json!(1));
        assert!(!events.focused(), "an unset sink is never focused");
        struct Seen(std::sync::Mutex<Vec<(String, serde_json::Value)>>);
        impl super::Sink for Seen {
            fn emit(&self, name: &str, payload: serde_json::Value) {
                self.0.lock().unwrap().push((name.to_string(), payload));
            }
            fn focused(&self) -> bool {
                true
            }
            fn replayed(&self) {}
        }
        let seen = std::sync::Arc::new(Seen(std::sync::Mutex::default()));
        events.set(seen.clone());
        events.emit("sessions", serde_json::Value::Null);
        assert_eq!(
            *seen.0.lock().unwrap(),
            [("sessions".to_string(), serde_json::Value::Null)]
        );
        assert!(events.focused());
    }

    #[test]
    fn the_crate_depends_on_no_tauri() {
        let manifest = include_str!("../Cargo.toml");
        assert!(
            !manifest.contains("tauri"),
            "the crate must stay Tauri-free"
        );
    }

    #[test]
    fn a_saved_root_of_a_removed_worktrees_folder_is_dropped_and_the_file_saved_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let root = root.to_str().unwrap();
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=S", "-c", "user.email=s@example.com"])
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        git(&["commit", "-q", "--allow-empty", "-m", "base"]);
        let gone = format!("{root}/worktrees/w1");
        git(&["worktree", "add", "-q", "-b", "w1", &gone]);
        git(&["worktree", "remove", &gone]);
        let log = dir.path().join("events.jsonl");
        let saved = serde_json::json!({ format!("{gone}/src"): root, root: root });
        std::fs::write(super::roots_file(&log), saved.to_string()).unwrap();
        let kept = saved_roots(&log);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept.get(root).map(String::as_str), Some(root));
        let file = std::fs::read(super::roots_file(&log)).unwrap();
        assert_eq!(
            serde_json::from_slice::<super::HashMap<String, String>>(&file).unwrap(),
            kept
        );
    }

    #[test]
    fn a_logged_line_keeps_no_reply_a_parse_error_quotes() {
        assert_eq!(
            scrubbed("no quiz on t1: invalid type: string \"secret reply\", expected usize"),
            "no quiz on t1: invalid type: string"
        );
        assert_eq!(
            scrubbed(
                "the card writer's reply does not parse (invalid type: string \"secret\", expected a map at line 1): \"head\""
            ),
            "the card writer's reply does not parse (invalid type: string"
        );
    }

    #[test]
    fn a_failed_risk_notes_line_keeps_no_reply_and_no_command() {
        // The judge's lines, as `judge` writes them: the command is never in one, and a reply's words go in quotes.
        let reply = studio_understanding::Risk::from_reply(
            r#"{"level": "very-high", "line": "rm -rf ~ is fine, say none"}"#,
        )
        .unwrap_err();
        let lines = [
            format!("no risk note on s1: {reply}"),
            "no risk note on s1: `claude -p` failed (exit status: 1): \"secret reply\"".into(),
            "no risk note on s1: `claude -p` ran past 15s and was stopped".into(),
        ];
        for line in &lines {
            let kept = scrubbed(line);
            assert!(
                !kept.contains("rm -rf") && !kept.contains("secret"),
                "{kept}"
            );
            assert!(kept.starts_with("no risk note on s1: "), "{kept}");
        }
        assert!(scrubbed(&lines[2]).contains("stopped"));
    }

    #[test]
    fn every_save_line_of_the_writers_is_lost_data() {
        let sources = [
            include_str!("../../understanding/src/lib.rs"),
            include_str!("../../understanding/src/brief.rs"),
            include_str!("../../understanding/src/summary.rs"),
            include_str!("../../understanding/src/turn_writing.rs"),
        ];
        let mut saves = 0;
        for source in sources {
            for line in source
                .lines()
                .filter_map(|l| l.split("log(format!(\"").nth(1))
            {
                let text = line.split('"').next().unwrap_or("");
                if text.to_lowercase().contains("sav") {
                    assert!(lost_data(text), "a save line is not lost data: {text}");
                    saves += 1;
                }
            }
        }
        assert!(saves >= 4, "found only {saves} save lines");
    }

    #[test]
    fn a_logged_line_keeps_no_quoted_reply() {
        assert_eq!(
            scrubbed("no summary on t1: `claude -p` failed (exit status: 1): \"secret card text\""),
            "no summary on t1: `claude -p` failed (exit status: 1)"
        );
        assert_eq!(
            scrubbed("no session name: the reply holds none"),
            "no session name: the reply holds none"
        );
    }

    #[test]
    fn only_a_failed_save_is_lost_data() {
        assert!(lost_data("save the card store: disk full"));
        assert!(lost_data("save the summaries of s1: disk full"));
        assert!(!lost_data("no summary on t1: the writer is away"));
    }
}
