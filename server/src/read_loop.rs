//! The read loop (danielrispler/plotkeeper#324, spec #319 "The server"): it reads the event log from its start, then from where
//! it stopped, and sends what changed to the [`Sink`](crate::Sink) the shell set: the session list, the selection, the cards, the
//! notices and the badge. It holds no window: whether Daniel is looking comes from the sink's `focused`, and what he last said
//! the page shows (`rail_shown`, `split_shown`) from the [`App`].

use crate::{
    App, CardsKey, PROCESS_CHECK, append_to, ask_notes, cards, current_cards_key, pid_alive,
    process_ends, refresh_landed, report, resume, save_roots, save_state, selected, sessions,
    settle_checks, unsaved, wanted_notes, warn_log,
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
use studio_core::{
    CardRequest, Output,
    State::{Ended, NeedsYou},
    compute_turn_files,
};
use studio_understanding::{
    AGY_FIXTURE_VERSION, AgyVersions, Effects, Fingerprints, Versions, diff_turn,
    fingerprint_changes, found_files, millis,
};

/// What the read loop last sent, so it sends each again only when it changes.
#[derive(Default)]
struct Sent {
    list: Option<serde_json::Value>,
    selected: Option<serde_json::Value>,
    badge: Option<usize>,
    cards: Option<CardsKey>,
}

impl Sent {
    /// Sends the session list, the badge and the selection, each only if it changed.
    fn list(&mut self, app: &App) {
        let list = sessions(app);
        let needing = list.iter().filter(|r| r.session.wants_you()).count();
        // macOS shows `Some(0)` as a "0" badge.
        if self.badge != Some(needing) {
            app.events.badge((needing > 0).then_some(needing));
            self.badge = Some(needing);
        }
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
        // A card with buttons that is shown and has no note asks for one, before the key reads the notes' revision.
        ask_notes(app, wanted_notes(app));
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

/// The daemon's `list` (danielrispler/Falinks#560): the core's status-only rows and mismatch lines follow its terminals'
/// OSC 7501 status. A row's notices go out as a Claude session's do.
pub fn listed(app: &App, terms: Vec<studio_core::Terminal>) {
    let outputs = app.core.lock().unwrap().terminals(&terms, millis());
    for output in outputs {
        match output {
            Output::NeedsYou { session_id, agent } => {
                tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
            }
            Output::YourTurn { session_id } => tell(app, Notice::YourTurn, &session_id, None),
            Output::Mismatch { line } => mismatch(&line),
            Output::Interrupted {
                session_id,
                prompt_id,
            } => {
                thread::spawn(move || interrupted(&session_id, &prompt_id));
            }
            _ => {}
        }
    }
    app.list_changed();
}

/// Daniel interrupted turn `prompt_id` of session `session_id`, which fires no `Stop`: a
/// `StudioInterrupted` line in the event log, with the turn's end trees as the hook records them,
/// which the log thread then applies and a restart replays.
fn interrupted(session_id: &str, prompt_id: &str) {
    let mut line = serde_json::Map::new();
    line.insert("session_id".into(), session_id.into());
    line.insert("hook_event_name".into(), studio_core::INTERRUPTED.into());
    line.insert("prompt_id".into(), prompt_id.into());
    line.insert("studio_received_ms".into(), millis().into());
    let appended = std::env::home_dir()
        .ok_or("no home directory".to_owned())
        .and_then(|home| {
            let log = studio_hook::event_log(&home);
            let data = log.parent().ok_or("no data directory")?;
            // The line goes in the log even when the trees cannot be recorded, as the hook's does.
            if let Err(e) = studio_hook::trees(&mut line, data) {
                log::warn!("record the trees of {session_id}'s interrupted turn: {e}");
            }
            crate::append(&[line.into()])
        });
    if let Err(e) = appended {
        log::error!("end {session_id}'s interrupted turn: {e}");
    }
}

/// Logs `line` at `info` under the `mismatch` target, for debugging (danielrispler/Falinks#560).
fn mismatch(line: &str) {
    log::info!(target: "mismatch", "{line}");
}

/// The part of Turn writing that needs the whole [`App`]: the writer's threads call it, and it reaches the state once
/// [`attach`](Self::attach)ed. It holds the app weakly, as the app holds it.
#[derive(Default)]
pub struct ServerEffects(OnceLock<Weak<App>>);

impl ServerEffects {
    /// Gives the effects the state they work on; the first call wins.
    pub fn attach(&self, app: &Arc<App>) {
        let _ = self.0.set(Arc::downgrade(app));
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

    fn new_card(&self, session_id: &str, agent: Option<&str>) {
        if let Some(app) = self.app() {
            tell(&app, Notice::Titled("New card"), session_id, agent);
        }
    }

    fn settle_checks(&self) {
        let Some(app) = self.app() else { return };
        settle_checks(&app.core.lock().unwrap(), &mut app.cards.lock().unwrap());
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
    let body = app.core.lock().unwrap().notice_body(session_id, agent);
    let mut notices = app.notices.lock().unwrap();
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
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
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
        let log = self.log.as_path();
        let state = &app.core;
        let writing = &app.writing;
        let mut read = false;
        if let Ok(mut file) = File::open(log)
            && file.seek(SeekFrom::Start(self.read_to)).is_ok()
        {
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
                    let looks = state.lock().unwrap().fingerprint_folders(&text);
                    let changes = fingerprint_changes(looks, &mut self.fingerprints);
                    outputs.extend(state.lock().unwrap().apply_with_changes(&text, changes));
                } else {
                    state.lock().unwrap().replay(&text);
                }
                line.clear();
            }
            // A worktree's first fingerprint only records it, so the turns still open after the
            // replay get their base now, and their `Stop` has something to compare with. The list
            // goes first, so Daniel sees it while `git` runs.
            if !self.live {
                let mut studio = state.lock().unwrap();
                // What the replay made, before the card store's facts and the backfill change it.
                let replayed = studio.save();
                let store = app.cards.lock().unwrap();
                store.restore(&mut studio);
                // The turns that ended while the app was down get their cards now (danielrispler/Falinks#486).
                let requests: Vec<CardRequest> =
                    store.cards().iter().map(|k| k.request.clone()).collect();
                let backfill = studio.backfill(millis(), &requests);
                drop((store, studio));
                let log = log.to_path_buf();
                thread::spawn(move || save_state(&log, &replayed));
                state.lock().unwrap().tick(millis());
                self.sent.list(app);
                // The terminals' revival waits only for the replay, not for this pass's `git` work, which
                // can take a minute on a long log, while a click's pane waits on it (#182).
                app.events.replayed();
                let waiting = state.lock().unwrap().waiting_folders();
                fingerprint_changes(waiting, &mut self.fingerprints);
                writing.backfill(backfill);
            }
            for output in outputs {
                match output {
                    Output::Card(request) => writing.card(request, true),
                    Output::Brief(request) => writing.brief(request),
                    Output::Summary(request) => writing.summary(request),
                    Output::Name(request) => writing.name(request),
                    Output::NeedsYou { session_id, agent } => {
                        tell(app, Notice::NeedsYou, &session_id, agent.as_deref());
                    }
                    Output::YourTurn { session_id } => {
                        refresh_landed(&shared, Some(session_id.clone()));
                        tell(app, Notice::YourTurn, &session_id, None);
                    }
                    Output::CommandBlocked { session_id, agent } => {
                        // The card's buttons are live now; its note is asked for beside the notification, which waits for nothing.
                        let asks = {
                            let studio = state.lock().unwrap();
                            let blocked = studio.session(&session_id);
                            blocked.map(|session| app.notes.lock().unwrap().asks(session))
                        };
                        ask_notes(app, asks.unwrap_or_default());
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
                        unsaved(app.cards.lock().unwrap().promoted(&agent));
                    }
                    Output::Unrecognised {
                        session_id,
                        task_id,
                    } => log::warn!(
                        "unrecognised wake-up: a prompt of session {session_id} names \
                         task {task_id} in markup the studio does not know"
                    ),
                    Output::Mismatch { line } => mismatch(&line),
                    Output::Interrupted {
                        session_id,
                        prompt_id,
                    } => {
                        thread::spawn(move || interrupted(&session_id, &prompt_id));
                    }
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
            read = self.read_to > read_from;
            if read {
                let roots = state.lock().unwrap().known_roots();
                if roots.len() != self.roots_saved {
                    self.roots_saved = roots.len();
                    save_roots(log, &roots);
                }
                let keys = state.lock().unwrap().live_keys();
                self.fingerprints.retain(|key, _| keys.contains(key));
                // The decision history goes to the store, which keeps it across restarts (danielrispler/Falinks#479).
                let studio = state.lock().unwrap();
                let mut store = app.cards.lock().unwrap();
                for session in studio.sessions() {
                    unsaved(store.keep_decisions(&session.agent.id, &session.decisions));
                }
                settle_checks(&studio, &mut store);
            }
        }
        // A session whose agent is gone ends within about 30 s (#330); the lines are read on the next pass.
        if self.live && self.checked.is_none_or(|at| at.elapsed() >= PROCESS_CHECK) {
            self.checked = Some(Instant::now());
            let ends = process_ends(state, pid_alive);
            if !ends.is_empty()
                && let Err(e) = append_to(&self.log, &ends)
            {
                log::warn!("end the sessions whose process is gone: {e}");
            }
        }
        // Reads the transcripts' lines and nothing else (`AGENTS.md`).
        let transcripts_changed = state.lock().unwrap().follow_transcripts(report);
        // Each live session's context, from its status line snapshot (danielrispler/Falinks#517, decision 1; danielrispler/Falinks#519); the
        // snapshot is named by the conversation's id, which a `/clear` changes.
        let running: Vec<_> = (state.lock().unwrap().sessions().into_iter())
            .filter(|s| s.state != Ended && !s.status_only)
            .map(|s| (s.agent.id.clone(), s.resume_id.clone()))
            .collect();
        let home = std::env::home_dir().unwrap_or_default();
        let usage = studio_hook::data_dir(&home).join("usage");
        let windows_changed = running.into_iter().fold(false, |changed, (id, resume_id)| {
            let snapshot = studio_workbench::context_window(&usage, &resume_id);
            state.lock().unwrap().set_snapshot(&id, snapshot) | changed
        });
        let list_changed = app.list_changed.swap(false, Ordering::AcqRel);
        let files_changed = turn_files(state);
        let changed =
            read || transcripts_changed || windows_changed || list_changed || files_changed;
        // The open request goes in the same pass as the list that shows "needs you" (danielrispler/Falinks#569).
        if let Ok(data) = crate::answer::data()
            && let Some(ask) = app
                .watching
                .lock()
                .unwrap()
                .pass(&data, studio_hook::answer::now_ms())
        {
            app.events.emit("ask", serde_json::to_value(ask).unwrap());
        }
        if let Ok(data) = crate::answer::data()
            && let Some(blocked) = app.watching.lock().unwrap().changed_blocked_requests(&data)
        {
            app.events
                .emit("blocked", serde_json::to_value(blocked).unwrap());
        }
        // Asking for focus waits on the main thread, which never takes this lock.
        let focused = || app.events.focused();
        // Daniel sees the selected session in a focused window while the story rail shows it (danielrispler/Falinks#556);
        // the sink is asked only while it is unseen, and not under the core's lock, which the main
        // thread's commands take.
        let unseen = state.lock().unwrap().selected().is_some_and(|s| s.unseen);
        let shown = unseen && app.rail_shown.load(Ordering::Acquire) && focused();
        let seen = state.lock().unwrap().see_selected(shown);
        if let Some((id, turns)) = &seen {
            unsaved(app.cards.lock().unwrap().seen(id, *turns));
        }
        if state.lock().unwrap().tick(millis()) || changed || seen.is_some() {
            self.sent.list(app);
        }
        // Takes the core's lock under the notices' lock; `tell` never holds both.
        let needs_you = |id: &str| {
            let studio = state.lock().unwrap();
            studio.session(id).is_some_and(|s| s.state == NeedsYou)
        };
        let due = app
            .notices
            .lock()
            .unwrap()
            .due(uptime(), focused, needs_you, || {
                let studio = state.lock().unwrap();
                let selected = studio.selected().map(|s| s.agent.id.clone());
                let mut on_screen: Vec<String> = selected.into_iter().collect();
                // The split's pane shows a terminal: a status-only row's id, or a session's in it.
                if let Some(terminal) = app.split_shown.lock().unwrap().clone() {
                    let in_it = studio.sessions().into_iter();
                    let in_it = in_it.filter(|s| s.terminal.as_ref() == Some(&terminal));
                    on_screen.extend(in_it.map(|s| s.agent.id.clone()));
                    on_screen.push(terminal);
                }
                on_screen
            });
        if let Some((title, body)) = due.notification {
            app.events.notify(&title, &body);
        }
        if let Some(sound) = due.sound {
            app.events.play(sound);
        }
        if !due.finished.is_empty() {
            app.events
                .emit("finished", serde_json::to_value(&due.finished).unwrap());
        }
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        self.sent.cards(app);
        self.live = true;
        // Also when the log did not open, so its replay block never ran.
        app.events.replayed();
    }
}
