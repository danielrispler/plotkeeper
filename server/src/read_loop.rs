//! The read loop (danielrispler/plotkeeper#324, spec #319 "The server"): it reads the event log from its start, then from where
//! it stopped, and sends what changed to the [`Sink`](crate::Sink) the shell set: the session list, the selection, the cards, the
//! notices (a `notification` event). It holds no window: whether Daniel is looking comes from the sink's `focused`, and what he last said
//! the page shows (`rail_shown`) from the [`App`].

use crate::{
    App, CardsKey, PROCESS_CHECK, append_to, cards, current_cards_key, process_ends,
    refresh_landed, report, resume, save_roots, save_state, selected, sessions, unsaved, warn_log,
};
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, LazyLock, Mutex, OnceLock, Weak, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};
use studio_core::{Output, State::NeedsYou, compute_turn_files};
use studio_understanding::{
    AGY_FIXTURE_VERSION, AgyVersions, Effects, Fingerprints, Versions, diff_turn,
    fingerprint_changes, found_files, millis,
};

/// What the read loop last sent, so it sends each again only when it changes.
#[derive(Default)]
struct Sent {
    list: Option<serde_json::Value>,
    projects: Option<Vec<crate::projects::Project>>,
    selected: Option<serde_json::Value>,
    cards: Option<CardsKey>,
    settings: Option<serde_json::Value>,
}

impl Sent {
    /// Sends the text of `settings.json` and `keymap.json` when either changed, as the `settings_files` command gives
    /// it: a hand edit or another tab's save reaches the open page, whose next save then names the text the file holds.
    fn settings(&mut self, app: &App) {
        let files = crate::settings::settings_files();
        if self.settings.as_ref() != Some(&files) {
            app.events.emit("settings-files", files.clone());
            self.settings = Some(files);
        }
    }

    /// Sends the session list, the saved projects and the selection, each only if it changed. A session in a repository not
    /// yet saved adds it first (`projects.rs`).
    fn list(&mut self, app: &App) {
        let seen = app.projects.see(&app.core().sessions());
        if let Err(e) = seen {
            log::error!("save the project list: {e}");
        }
        let projects = app.projects.list();
        if self.projects.as_ref() != Some(&projects) {
            app.events
                .emit("projects", serde_json::to_value(&projects).unwrap());
            self.projects = Some(projects);
        }
        let list = sessions(app);
        let list = serde_json::to_value(list).expect("sessions serialize");
        if self.list.as_ref() != Some(&list) {
            app.events.emit("sessions", list.clone());
            self.list = Some(list);
        }
        let selected = selected(app);
        if self.selected.as_ref() != Some(&selected) {
            app.events.emit("selected", selected.clone());
            self.selected = Some(selected);
        }
    }

    /// Sends the cards when the viewed owner, the store's revision or the minute changed.
    fn cards(&mut self, app: &App) {
        let key = current_cards_key(app);
        if self.cards.as_ref() != Some(&key) {
            let cards = serde_json::to_value(cards(app)).expect("cards serialize");
            app.events.emit("cards", cards);
            self.cards = Some(key);
        }
    }
}

/// The turns whose files the core asks for (danielrispler/plotkeeper#111): each is diffed by `git`
/// with the core unlocked, then handed back. Returns whether any was asked.
fn turn_files(state: &Mutex<studio_core::Studio>) -> bool {
    /// At most this many turns a pass, so a launch over a long log does not hold the loop.
    const PER_PASS: usize = 8;
    let asks = state.lock().unwrap().turn_diffs(millis(), PER_PASS);
    for ask in &asks {
        let diffs = diff_turn(state, ask, &mut warn_log);
        let found = found_files(state, ask, diffs, compute_turn_files);
        state.lock().unwrap().store_turn_files(ask, found);
    }
    !asks.is_empty()
}

/// The part of Turn writing that needs the whole [`App`]: the writer's threads call it, and it reaches the state once
/// [`attach`](Self::attach)ed. It holds the app weakly, as the app holds it.
#[derive(Default)]
pub struct ServerEffects(OnceLock<Weak<App>>);

impl ServerEffects {
    /// Gives the effects the state they work on; the first call wins.
    pub fn attach(&self, app: &Arc<App>) {
        // Set once; a later set keeps the first value, which is the same.
        self.0.set(Arc::downgrade(app)).ok();
    }

    fn app(&self) -> Option<Arc<App>> {
        self.0.get().and_then(Weak::upgrade)
    }
}

impl Effects for ServerEffects {
    fn list(&self) {
        if let Some(app) = self.app() {
            app.events
                .emit("sessions", serde_json::to_value(sessions(&app)).unwrap());
        }
    }

    fn name(&self, session_id: &str, name: &str) {
        let line = serde_json::json!({
            "session_id": session_id,
            "hook_event_name": studio_core::PROMPT_NAME,
            "studio_name": name,
            "studio_received_ms": millis(),
        });
        if let Err(e) = crate::append(&[line]) {
            log::error!("save the name of {session_id}: {e}");
        }
    }

    fn log(&self, line: String) {
        report(line);
    }
}

/// Adds a notice on session `session_id`'s line, naming its subagent `agent`, if any; the read loop
/// shows it. Called without the core's lock held.
pub fn tell(app: &App, notice: Notice, session_id: &str, agent: Option<&str>) {
    let body = app.core().notice_body(session_id, agent);
    let mut notices = app.notices();
    match notice {
        Notice::Titled(title) => notices.push(title, body, uptime()),
        Notice::NeedsYou => notices.push_needs_you(session_id, body, uptime()),
        Notice::YourTurn => notices.push_your_turn(session_id, body, uptime()),
    }
}

/// What a notice says: that a session needs you, which shows only if it still needs Daniel when
/// it is due, or its own title.
pub enum Notice<'a> {
    NeedsYou,
    YourTurn,
    Titled(&'a str),
}

/// Time since the process started, a monotonic clock for combining notices (danielrispler/Falinks#434, decision 28).
pub fn uptime() -> Duration {
    static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
    STARTED.elapsed()
}

/// agy's version, from `agy --version` run in Daniel's login shell, so a Dock-started app finds it
/// on his `PATH`; `None` when agy is missing or fails.
fn agy_version() -> Option<String> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let out = Command::new(shell)
        .args(["-lc", "agy --version"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let version = text.lines().last()?.trim();
    (out.status.success() && !version.is_empty()).then(|| version.to_owned())
}

/// Reads the log, pass after pass, for the life of the process; see [`Follower`].
// shortcut: polls every 200 ms; switch to file-system events if the delay or the wake-ups matter.
pub fn follow(app: &Arc<App>, log: &Path) {
    let mut follower = Follower::start(app, log);
    loop {
        follower.pass();
        thread::sleep(Duration::from_millis(200));
    }
}

/// The read loop's state between passes. The lines there at start are replayed, so steps that ran while the app was down get no
/// cards and no notifications, but for the turns of the last 24 hours with no card, which get one on the hook's trees
/// (danielrispler/Falinks#486). Live lines get the worktree fingerprint the core asks for; replayed ones get none. Each pass
/// ends with a tick, the first right after the replay.
pub struct Follower {
    app: Arc<App>,
    log: PathBuf,
    read_to: u64,
    roots_saved: usize,
    live: bool,
    /// Each session's and each subagent's own, so another's prompt or step in the same worktree moves no base.
    fingerprints: HashMap<String, Fingerprints>,
    versions: Versions,
    agy_versions: AgyVersions,
    sent: Sent,
    /// When the sessions' agent processes were last checked (#330).
    checked: Option<Instant>,
}

impl Follower {
    /// Prunes the log, then takes the saved state, when there is one that fits it, which starts the replay from where it
    /// stopped (danielrispler/plotkeeper#134).
    pub fn start(app: &Arc<App>, log: &Path) -> Self {
        // Before the replay, so it reads none of what the prune moves (danielrispler/plotkeeper#132).
        match studio_hook::prune(log, millis()) {
            Ok(pruned) if pruned != studio_hook::Pruned::default() => log::info!(
                "pruned the event log: {} lines archived, {} lightened",
                pruned.archived,
                pruned.lightened
            ),
            Ok(_) => {}
            Err(e) => log::warn!("prune the event log: {e}"),
        }
        Follower {
            app: app.clone(),
            log: log.to_path_buf(),
            read_to: resume(&app.core, log),
            roots_saved: 0,
            live: false,
            fingerprints: HashMap::new(),
            versions: Versions::beside(log),
            agy_versions: AgyVersions::beside(log),
            sent: Sent::default(),
            checked: None,
        }
    }

    /// One pass: reads what the log gained, applies it, and sends what changed to the sink.
    pub fn pass(&mut self) {
        let shared = self.app.clone();
        let app: &App = &shared;
        let read = self.read_log();
        // A session whose agent is gone ends within about 30 s (#330); the lines are read on the next pass.
        if self.live && self.checked.is_none_or(|at| at.elapsed() >= PROCESS_CHECK) {
            self.checked = Some(Instant::now());
            let ends = process_ends(&app.core, studio_hook::sys::pid_alive);
            if !ends.is_empty()
                && let Err(e) = append_to(&self.log, &ends)
            {
                log::warn!("end the sessions whose process is gone: {e}");
            }
        }
        // Reads the transcripts' lines and nothing else (`AGENTS.md`).
        let transcripts_changed = app.core().follow_transcripts(report);
        let list_changed = app.list_changed.swap(false, Ordering::AcqRel);
        let files_changed = turn_files(&app.core);
        let changed = read || transcripts_changed || list_changed || files_changed;
        emit_watched(app);
        let seen = see_selected(app);
        if app.core().tick(millis()) || changed || seen {
            self.sent.list(app);
        }
        notify(app);
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        self.sent.cards(app);
        self.sent.settings(app);
        self.live = true;
        // Also when the log did not open, so its replay block never ran.
        app.events.replayed();
    }

    /// Reads what the log gained and applies it: replayed before the first pass ends, live after,
    /// with the outputs acted on. Returns whether a line was read.
    fn read_log(&mut self) -> bool {
        let shared = self.app.clone();
        let app: &App = &shared;
        let Ok(mut file) = File::open(&self.log) else {
            return false;
        };
        if file.seek(SeekFrom::Start(self.read_to)).is_err() {
            return false;
        }
        let mut reader = BufReader::new(file);
        let mut line = Vec::new();
        let mut outputs = Vec::new();
        let read_from = self.read_to;
        // The core is locked per line, and not while the fingerprint runs `git`, so the pane's
        // commands do not wait on it.
        while reader.read_until(b'\n', &mut line).unwrap_or(0) > 0 && line.ends_with(b"\n") {
            self.read_to += line.len() as u64;
            let text = String::from_utf8_lossy(&line);
            if self.live {
                // shortcut: fingerprints when the line is read, up to one poll after the prompt,
                // so an edit in between counts as Daniel's; take the fingerprint in the hook if a
                // card ever misses one.
                let looks = app.core().fingerprint_folders(&text);
                let changes = fingerprint_changes(looks, &mut self.fingerprints);
                outputs.extend(app.core().apply_with_changes(&text, changes));
            } else {
                app.core().replay(&text);
            }
            line.clear();
        }
        if !self.live {
            self.replayed();
        }
        for output in outputs {
            self.act(output);
        }
        let read = self.read_to > read_from;
        if read {
            let roots = app.core().known_roots();
            if roots.len() != self.roots_saved {
                self.roots_saved = roots.len();
                save_roots(&self.log, &roots);
            }
            let keys = app.core().live_keys();
            self.fingerprints.retain(|key, _| keys.contains(key));
            // The decision history goes to the store, which keeps it across restarts (danielrispler/Falinks#479).
            let (studio, mut store) = app.core_and_cards();
            for session in studio.sessions() {
                unsaved(store.keep_decisions(&session.agent.id, &session.decisions));
            }
        }
        read
    }

    /// The replay is done. A worktree's first fingerprint only records it, so the turns still open
    /// after the replay get their base now, and their `Stop` has something to compare with. The
    /// list goes first, so Daniel sees it while `git` runs.
    fn replayed(&mut self) {
        let shared = self.app.clone();
        let app: &App = &shared;
        let (mut studio, store) = app.core_and_cards();
        // What the replay made, before the card store's facts change it.
        let replayed = studio.save();
        store.restore(&mut studio);
        drop((store, studio));
        let log = self.log.clone();
        thread::spawn(move || save_state(&log, &replayed));
        app.core().tick(millis());
        self.sent.list(app);
        // The sink hears the replay is done before this pass's `git` work, which can take a minute
        // on a long log (#182).
        app.events.replayed();
        // The first lookup of the PRs, which writes the briefs of the ready ones the start picks (`pr_briefs::Startup`).
        crate::prs::ask_later(&shared, crate::prs::Trigger::TurnEnd, None);
        let waiting = app.core().waiting_folders();
        fingerprint_changes(waiting, &mut self.fingerprints);
    }

    /// Does what one live line's output asks.
    fn act(&mut self, output: Output) {
        let shared = self.app.clone();
        let app: &App = &shared;
        let writing = &app.writing;
        match output {
            Output::Card(request) | Output::Brief(request) => writing.brief(request),
            Output::Summary(request) => writing.summary(request),
            Output::Name(request) => writing.name(request),
            Output::NeedsYou { session_id, agent } => {
                tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
            }
            Output::YourTurn { session_id } => {
                refresh_landed(&shared, Some(session_id.clone()));
                crate::prs::ask_later(&shared, crate::prs::Trigger::TurnEnd, None);
                // A merge that landed in the turn gets its part catch-ups written before the map is next shown.
                let folder = app.core().session(&session_id).map(|s| s.cwd.clone());
                if let Some(folder) = folder.filter(|f| !f.is_empty()) {
                    crate::part_catch_ups::turn_ended(&shared, folder);
                }
                tell(app, Notice::YourTurn, &session_id, None);
            }
            Output::CommandBlocked { session_id, agent } => {
                tell(
                    app,
                    Notice::Titled("Auto mode blocked a command"),
                    &session_id,
                    agent.as_deref(),
                );
            }
            Output::Failed { session_id, agent } => {
                tell(
                    app,
                    Notice::Titled("Worker failed"),
                    &session_id,
                    Some(&agent),
                );
            }
            Output::Promoted { agent } => {
                unsaved(app.cards().promoted(&agent));
            }
            Output::Unrecognised {
                session_id,
                task_id,
            } => log::warn!(
                "unrecognised wake-up: a prompt of session {session_id} names \
                 task {task_id} in markup the studio does not know"
            ),
            Output::AgyVersion => match agy_version() {
                Some(version) => {
                    match self
                        .agy_versions
                        .check(&version, AGY_FIXTURE_VERSION, millis())
                    {
                        Ok(Some(notice)) => {
                            log::warn!("{notice}");
                            app.events.emit("notice", notice.into());
                        }
                        Ok(None) => {}
                        Err(e) => log::warn!("keep agy's version: {e}"),
                    }
                }
                None => log::warn!("agy's version: `agy --version` did not run"),
            },
            // Reads the transcript's lines and nothing else (`AGENTS.md`).
            Output::Version { transcript_path } => {
                match self.versions.check(Path::new(&transcript_path), millis()) {
                    Ok(Some(notice)) => {
                        log::warn!("{notice}");
                        app.events.emit("notice", notice.into());
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("keep Claude Code's version: {e}"),
                }
            }
        }
    }
}

/// Sends the open answer request when it changed. The open request goes
/// in the same pass as the list that shows "needs you" (danielrispler/Falinks#569).
fn emit_watched(app: &App) {
    if let Ok(data) = crate::answer::data()
        && let Some(ask) = app.watching().pass(&data, studio_hook::answer::now_ms())
    {
        app.events.emit("ask", serde_json::to_value(ask).unwrap());
    }
}

/// Daniel sees the selected session in a focused window while the story rail shows it
/// (danielrispler/Falinks#556); the sink is asked only while it is unseen, and not under the core's
/// lock, which the main thread's commands take. Returns whether a turn was seen.
fn see_selected(app: &App) -> bool {
    let unseen = app.core().selected().is_some_and(|s| s.unseen);
    // Asking for focus waits on the main thread, which never takes this lock.
    let shown = unseen && app.rail_shown.load(Ordering::Acquire) && app.events.focused();
    let seen = app.core().see_selected(shown);
    if let Some((id, turns)) = &seen {
        unsaved(app.cards().seen(id, *turns));
    }
    seen.is_some()
}

/// Shows the notices that are due, as one notification, and sends the sessions whose turn finished.
fn notify(app: &App) {
    // Asking for focus waits on the main thread, which never takes this lock.
    let focused = || app.events.focused();
    // Takes the core's lock under the notices' lock; `tell` never holds both.
    let needs_you = |id: &str| {
        let studio = app.core();
        studio.session(id).is_some_and(|s| s.state == NeedsYou)
    };
    let due = app.notices().due(uptime(), focused, needs_you, || {
        let studio = app.core();
        let selected = studio.selected().map(|s| s.agent.id.clone());
        let on_screen: Vec<String> = selected.into_iter().collect();
        drop(studio);
        on_screen
    });
    if let Some((title, body)) = due.notification {
        // The page shows it as a browser notification, when no tab of it is visible and focused (#331).
        app.events.emit(
            "notification",
            serde_json::json!({ "title": title, "body": body }),
        );
    }
    if !due.finished.is_empty() {
        app.events
            .emit("finished", serde_json::to_value(&due.finished).unwrap());
    }
}
