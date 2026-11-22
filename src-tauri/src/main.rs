//! The studio app's shell (danielrispler/Falinks#328, module 9). At start it replays the whole event log through the
//! core, then feeds it each new line live and hands the core's card requests to the card writer. The
//! window lists every session (danielrispler/Falinks#369, decision 18) and shows the one Daniel selected: its cards, with
//! their quizzes, his guesses, his ratings and the longer explanations he asked for, which the card
//! store keeps on disk, and its check lines. A session that newly needs Daniel or reaches your turn,
//! a new card and a failed worker raise a macOS notification while the window is not focused; while
//! it is, a turn that ends off screen goes to the window's toast. The Dock badge counts the sessions
//! that need him (danielrispler/Falinks#369, decision 22; danielrispler/Falinks#517, decision 6).
//!
//! The state and the logic behind the learning-layer and session-view commands are `studio_server`'s
//! (danielrispler/plotkeeper#321): `commands` forwards to it, and what stays here is what needs the window.

use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::Path,
    process::Command,
    sync::{Arc, LazyLock, Mutex, OnceLock, atomic::Ordering, mpsc},
    thread,
    time::{Duration, Instant},
};
use studio_core::{
    CardRequest, Output, Sound,
    State::{Ended, NeedsYou},
    Studio, compute_turn_files,
};
use studio_server::{
    App, Cleared, answer, clear_commit, report, resolve_clearing, scrubbed, session_ends,
    settle_checks, unsaved, warn_log,
};
use studio_understanding::{
    AGY_FIXTURE_VERSION, AgyVersions, Effects, Fingerprints, Versions, diff_turn,
    fingerprint_changes, found_files, millis,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

mod commands;
mod presence;
mod restart;
mod settings;
mod skills;
mod terminal;

/// Plan usage (danielrispler/plotkeeper#163; #165) of the `agents` asked for, or of every agent when `None`. The window asks for only
/// "claude" on its timer, which reads a snapshot file and runs no CLI; Codex and agy run theirs only when every agent is read, and
/// only if the CLI is on the login shell's `PATH`: an agent not installed has no entry. Claude's is from the status line's newest
/// snapshot in the `usage` folder beside the event log. An agent whose usage cannot be read says why.
#[tauri::command(async)]
fn usage(agents: Option<Vec<String>>) -> Vec<studio_workbench::AgentUsage> {
    let wanted = |agent: &str| {
        agents
            .as_ref()
            .is_none_or(|list| list.iter().any(|a| a == agent))
    };
    let mut read = Vec::new();
    if wanted("claude") {
        read.push(match std::env::home_dir() {
            Some(home) => {
                studio_workbench::claude_usage(&studio_hook::data_dir(&home).join("usage"))
            }
            None => studio_workbench::AgentUsage::failed(
                "claude",
                "No home folder to find Claude's usage in",
            ),
        });
    }
    let cli: Vec<&str> = ["codex", "agy"].into_iter().filter(|a| wanted(a)).collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    if !cli.is_empty() {
        // The login shell's PATH, read once: a Dock-started app lacks it, and an npm-installed CLI needs it to find `node`.
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        let path = studio_workbench::login_shell_path(
            &shell.to_string_lossy(),
            std::time::Duration::from_secs(5),
        )
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
        let find = |program: &str| studio_workbench::find_in_path(program, &path);
        read.extend(studio_workbench::cli_usages(&cli, &find, Some(&path), now));
    }
    read
}

/// What the read loop last sent the window, so it sends each again only when it changes.
#[derive(Default)]
struct Sent {
    list: Option<serde_json::Value>,
    selected: Option<serde_json::Value>,
    badge: Option<usize>,
    cards: Option<studio_server::CardsKey>,
}

impl Sent {
    /// Sends the session list, the Dock badge and the selection, each only if it changed.
    fn list(&mut self, app: &AppHandle) {
        let list = studio_server::sessions(&app.state::<Arc<App>>());
        let needing = list.iter().filter(|r| r.session.wants_you()).count();
        // macOS shows `Some(0)` as a "0" badge.
        if self.badge != Some(needing)
            && let Some(window) = app.get_webview_window("main")
        {
            let _ = window.set_badge_count((needing > 0).then_some(needing as i64));
            self.badge = Some(needing);
        }
        let list = serde_json::to_value(list).expect("sessions serialize");
        if self.list.as_ref() != Some(&list) {
            let _ = app.emit("sessions", &list);
            self.list = Some(list);
        }
        let selected = studio_server::selected(&app.state::<Arc<App>>());
        if self.selected.as_ref() != Some(&selected) {
            let _ = app.emit("selected", &selected);
            self.selected = Some(selected);
        }
    }

    /// Sends the cards when the viewed owner, the store's revision or the minute changed.
    fn cards(&mut self, app: &AppHandle) {
        let state = app.state::<Arc<App>>();
        // A card with buttons that is shown and has no note asks for one, before the key reads the notes' revision.
        studio_server::ask_notes(&state, studio_server::wanted_notes(&state));
        let key = studio_server::current_cards_key(&state);
        if self.cards.as_ref() != Some(&key) {
            let _ = app.emit("cards", studio_server::cards(&state));
            self.cards = Some(key);
        }
    }
}

/// The turns whose files the core asks for (danielrispler/plotkeeper#111): each is diffed by `git`
/// with the core unlocked, then handed back. Returns whether any was asked.
fn turn_files(state: &Mutex<Studio>) -> bool {
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

/// The app is quitting: the sessions cleared inside their undo window are deleted now (#244, story 35).
fn commit_clearing(app: &AppHandle) {
    let state = app.state::<Arc<App>>();
    let pending = std::mem::take(&mut *state.clearing.0.lock().unwrap());
    let studio = &state.core;
    // The window's list may be stale: what is landed and where it runs is read again from the studio now.
    let sessions = {
        let studio = studio.lock().unwrap();
        let all = studio.sessions();
        resolve_clearing(pending, |id| {
            let session = all.iter().find(|s| s.agent.id == id)?;
            Some((studio.landed(session), session.terminal.clone()))
        })
    };
    let in_use = |terminal: &str, cleared: &[Cleared]| {
        let studio = studio.lock().unwrap();
        (studio.sessions().into_iter()).any(|s| {
            s.terminal.as_deref() == Some(terminal)
                && s.state != Ended
                && !s.status_only
                && !cleared.iter().any(|c| c.id == s.agent.id)
        })
    };
    let (requests, lines) =
        clear_commit(&sessions, in_use, |terminal| session_ends(studio, terminal));
    for request in requests {
        let _ = terminal::terminal_send(request, app.state());
    }
    if !lines.is_empty()
        && let Err(e) = studio_server::append(&lines)
    {
        log::warn!("commit the cleared sessions: {e}");
    }
}

/// The daemon's `list` (danielrispler/Falinks#560): the core's status-only rows and mismatch lines follow its terminals'
/// OSC 7501 status. A row's notices go out as a Claude session's do.
fn listed(app: &AppHandle, terms: Vec<studio_core::Terminal>) {
    let state = app.state::<Arc<App>>();
    let outputs = state.core.lock().unwrap().terminals(&terms, millis());
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
    state.list_changed();
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
            studio_server::append(&[line.into()])
        });
    if let Err(e) = appended {
        log::error!("end {session_id}'s interrupted turn: {e}");
    }
}

/// Logs `line` at `info` under the `mismatch` target, for debugging (danielrispler/Falinks#560).
fn mismatch(line: &str) {
    log::info!(target: "mismatch", "{line}");
}

/// The Studio log (danielrispler/plotkeeper#188): `studio.log` in the app's log folder, beside stderr, 500 KB a file with
/// one older file kept, in local time. The studio's own crates log at `Debug` and libraries at `Warn`, in every build.
fn studio_log() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
    let mut log = tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets([
            Target::new(TargetKind::Stderr).format(stamped),
            Target::new(TargetKind::LogDir {
                file_name: Some("studio".into()),
            })
            .format(stamped),
            // Errors also show in the window's notice line (#191), as the bare message; warnings stay in the file.
            Target::new(TargetKind::Webview).filter(shows_in_window),
        ])
        .max_file_size(500_000)
        .rotation_strategy(RotationStrategy::KeepSome(1))
        .timezone_strategy(TimezoneStrategy::UseLocal)
        // The Builder's own format is the bare message: a target adds the prefix its output wants (the window's does not).
        .clear_format()
        .level(log::LevelFilter::Warn);
    for own in [
        "studio",
        "studio_core",
        "studio_hook",
        "studio_understanding",
        "studio_terminal_daemon",
        "studio_workbench",
        "mismatch",
    ] {
        log = log.level_for(own, log::LevelFilter::Debug);
    }
    log.build()
}

/// A line of `stderr` and `studio.log`: `[date][time][LEVEL][target] message`, the plugin's own shape, in local time.
fn log_line(at: [u32; 6], level: log::Level, target: &str, message: &str) -> String {
    let [year, month, day, hour, minute, second] = at;
    format!(
        "[{year:04}-{month:02}-{day:02}][{hour:02}:{minute:02}:{second:02}][{level}][{target}] {message}"
    )
}

/// `log_line` for a record now, in local time.
fn stamped(
    out: tauri_plugin_log::fern::FormatCallback,
    message: &std::fmt::Arguments,
    record: &log::Record,
) {
    let now = tauri_plugin_log::TimezoneStrategy::UseLocal.get_now();
    let at = [
        now.year() as u32,
        u8::from(now.month()).into(),
        now.day().into(),
        now.hour().into(),
        now.minute().into(),
        now.second().into(),
    ];
    out.finish(format_args!(
        "{}",
        log_line(at, record.level(), record.target(), &message.to_string())
    ));
}

/// Whether a record reaches the window's notice line: an `error` that did not come from the UI, which showed its own.
fn shows_in_window(metadata: &log::Metadata) -> bool {
    metadata.level() == log::Level::Error
        && !metadata
            .target()
            .starts_with(tauri_plugin_log::WEBVIEW_TARGET)
}

/// A panic is logged at `error` with its location, then reaches the default panic output.
fn keep_panics_in_the_log() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let at = info
            .location()
            .map_or_else(String::new, |l| format!(" at {}:{}", l.file(), l.line()));
        log::error!("panic{at}: {}", panic_message(info.payload()));
        previous(info);
    }));
}

/// A panic's message when it is text, scrubbed like a writer's line: an `unwrap` on an `Err` can carry what the code was working on.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    let text = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("not text");
    scrubbed(text)
}

/// Opens Finder with the Studio log selected.
#[tauri::command]
fn reveal_log(app: AppHandle) -> Result<(), String> {
    let file = app
        .path()
        .app_log_dir()
        .map_err(|e| e.to_string())?
        .join("studio.log");
    Command::new("open")
        .arg("-R")
        .arg(file)
        .status()
        .map_err(|e| e.to_string())?
        .success()
        .then_some(())
        .ok_or_else(|| "open -R failed".to_string())
}

/// Whether a navigation stays on the app's own pages (danielrispler/Falinks#525, decision 21): a backstop, as a rendered
/// link opens in the browser through `open_link` and never navigates the window.
fn in_app(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" || url.host_str() == Some("tauri.localhost")
}

fn main() {
    uptime();
    hover_delay();
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    // The window does not exist yet, so the writer's notices wait here for the setup to forward them.
    let (writer_notices, writer_notice_reader) = mpsc::channel::<Option<String>>();
    let effects = Arc::new(ShellEffects::default());
    let state = Arc::new(
        App::open(&log, effects.clone(), writer_notices).expect("open the studio's state"),
    );
    keep_panics_in_the_log();
    tauri::Builder::default()
        .plugin(studio_log())
        // First, so a second instance exits before it sets anything up (danielrispler/Falinks#470).
        .plugin(tauri_plugin_single_instance::init(restart::second_instance))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri::plugin::Builder::<_, ()>::new("navigation")
                .on_navigation(|_, url| in_app(url))
                .build(),
        )
        .manage(state.clone())
        // A reload loses the window's undo timer and its hidden rows, which it shows again: nothing is pending a commit then.
        .on_page_load(|webview, payload| {
            if let tauri::webview::PageLoadEvent::Started = payload.event() {
                webview
                    .state::<Arc<App>>()
                    .clearing
                    .0
                    .lock()
                    .unwrap()
                    .clear();
            }
        })
        .manage(terminal::Daemon::default())
        .invoke_handler(tauri::generate_handler![
            commands::sessions,
            commands::select,
            commands::select_terminal,
            commands::open,
            commands::selected,
            commands::subagent_log,
            commands::cards,
            commands::shown,
            commands::guess,
            commands::key_wrong,
            commands::quiz_tap,
            commands::learning,
            commands::rate,
            commands::explain_more,
            commands::ask,
            commands::rename,
            commands::delete_session,
            commands::clearing,
            commands::closed,
            usage,
            terminal::terminal_connect,
            terminal::terminal_send,
            terminal::open_link,
            terminal::open_file,
            skills::skills,
            terminal::projects,
            terminal::choose_folder,
            terminal::add_project,
            terminal::remove_project,
            terminal::reorder_projects,
            terminal::suggest_projects,
            terminal::repository_of,
            terminal::workspaces,
            terminal::workspace_git_state,
            terminal::fetch_repository,
            terminal::pull_main,
            terminal::claude_argv,
            terminal::agent_argv,
            terminal::agents,
            terminal::plugin_setup,
            terminal::fix_plugin_setup,
            terminal::agy_plugin_setup,
            terminal::fix_agy_plugin_setup,
            terminal::create_task,
            terminal::create_named_task,
            terminal::remove_task,
            terminal::delete_merged_branch,
            terminal::branches,
            terminal::switch_branch,
            terminal::terminal_lost,
            terminal::terminal_upgrade,
            restart::revival,
            restart::restart_into,
            commands::watch,
            commands::alive,
            commands::answer,
            commands::answer_blocked_command,
            commands::ask_again,
            settings::settings_files,
            settings::set_zoom,
            settings::open_settings,
            settings::open_keymap,
            settings::write_setting,
            commands::summary_settings,
            commands::hunk_opened,
            commands::check_lists,
            commands::check_output,
            commands::turn_file_hunk,
            commands::ask_facts,
            commands::ask_anchor,
            commands::ask_why_not,
            commands::turn_summary,
            commands::turn_reply,
            commands::agent_summary,
            commands::split_shown,
            commands::mark_unseen,
            commands::rail_shown,
            commands::refresh_landed_tasks,
            commands::map_state,
            commands::map_save_positions,
            commands::map_set_tier,
            commands::map_know,
            commands::map_history,
            commands::map_at,
            commands::map_commit,
            commands::map_marks,
            commands::map_proof,
            commands::map_place,
            commands::stop_log,
            commands::stop_facts,
            commands::stop_dismiss,
            reveal_log,
            commands::sounds
        ])
        .on_window_event(|window, event| {
            if let Some(path) = presence::path() {
                presence::window_event(&path, event);
            }
            // Leaving full screen, tao queues making its own view first responder, a view wry took
            // out of the window, so no ⌘ key reached the page (#23). The resize that follows
            // queues the webview's focus after it.
            if let tauri::WindowEvent::Resized(_) = event {
                let webviews = window.webviews();
                dispatch2::DispatchQueue::main().exec_async(move || {
                    for webview in webviews {
                        let _ = webview.set_focus();
                    }
                });
            }
        })
        .setup(move |app| {
            log::info!("start, version {}", app.package_info().version);
            if let Some(path) = presence::path() {
                presence::start(&path);
            }
            let app = app.handle().clone();
            let _ = effects.0.set(app.clone());
            let events_app = app.clone();
            state.events.set(move |name, payload| {
                let _ = events_app.emit(name, payload);
            });
            restart::watch_build(app.clone());
            settings::watch(app.clone());
            let prs_state = state.clone();
            thread::spawn(move || studio_server::prs(&prs_state));
            let retest_state = state.clone();
            thread::spawn(move || studio_server::write_due_retests(&retest_state));
            let notice_app = app.clone();
            thread::spawn(move || {
                for notice in writer_notice_reader {
                    // The notice is the writer's own words; the log keeps only that one arrived or cleared.
                    log::warn!(
                        "card writer notice {}",
                        if notice.is_some() { "shown" } else { "cleared" }
                    );
                    let _ = notice_app.emit("writer-notice", notice);
                }
            });
            thread::spawn(move || follow(&app, &log));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("build the studio")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commit_clearing(app);
            }
            if let Some(path) = presence::path() {
                presence::run_event(&path, &event);
            }
        });
}

/// Reads the log from its start, then from where it stopped. A line counts once it ends in a newline,
/// so a line the hook is still writing waits for the next read. The lines there at start are
/// replayed, so steps that ran while the app was down get no cards and no notifications, but for the
/// turns of the last 24 hours with no card, which get one on the hook's trees (danielrispler/Falinks#486). Live lines
/// get the worktree fingerprint the core asks for; replayed ones get none. Each pass ends with a
/// tick, the first right after the replay.
// shortcut: polls every 200 ms; switch to file-system events if the delay or the wake-ups matter.
fn follow(app: &AppHandle, log: &Path) {
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
    // The saved state, when there is one that fits the log, takes the replay from where it stopped
    // (danielrispler/plotkeeper#134).
    let shared = app.state::<Arc<App>>();
    let mut read_to = studio_server::resume(&shared.core, log);
    let mut roots_saved = 0;
    let mut live = false;
    // Each session's and each subagent's own, so another's prompt or step in the same worktree moves
    // no base.
    let mut fingerprints = HashMap::<String, Fingerprints>::new();
    let writing = &shared.writing;
    let versions = Versions::beside(log);
    let agy_versions = AgyVersions::beside(log);
    let mut sent = Sent::default();
    loop {
        let state = &shared.core;
        let mut read = false;
        if let Ok(mut file) = File::open(log)
            && file.seek(SeekFrom::Start(read_to)).is_ok()
        {
            let mut reader = BufReader::new(file);
            let mut line = Vec::new();
            let mut outputs = Vec::new();
            let read_from = read_to;
            // The core is locked per line, and not while the fingerprint runs `git`, so the pane's
            // commands do not wait on it.
            while reader.read_until(b'\n', &mut line).unwrap_or(0) > 0 && line.ends_with(b"\n") {
                read_to += line.len() as u64;
                let text = String::from_utf8_lossy(&line);
                if live {
                    // shortcut: fingerprints when the line is read, up to one poll after the prompt,
                    // so an edit in between counts as Daniel's; take the fingerprint in the hook if a
                    // card ever misses one.
                    let looks = state.lock().unwrap().fingerprint_folders(&text);
                    let changes = fingerprint_changes(looks, &mut fingerprints);
                    outputs.extend(state.lock().unwrap().apply_with_changes(&text, changes));
                } else {
                    state.lock().unwrap().replay(&text);
                }
                line.clear();
            }
            // A worktree's first fingerprint only records it, so the turns still open after the
            // replay get their base now, and their `Stop` has something to compare with. The list
            // goes first, so Daniel sees it while `git` runs.
            if !live {
                let mut studio = state.lock().unwrap();
                // What the replay made, before the card store's facts and the backfill change it.
                let replayed = studio.save();
                let store = shared.cards.lock().unwrap();
                store.restore(&mut studio);
                // The turns that ended while the app was down get their cards now (danielrispler/Falinks#486).
                let requests: Vec<CardRequest> =
                    store.cards().iter().map(|k| k.request.clone()).collect();
                let backfill = studio.backfill(millis(), &requests);
                drop((store, studio));
                let log = log.to_path_buf();
                thread::spawn(move || studio_server::save_state(&log, &replayed));
                state.lock().unwrap().tick(millis());
                sent.list(app);
                // The terminals' revival waits only for the replay, not for this pass's `git` work, which
                // can take a minute on a long log, while a click's pane waits on it (#182).
                restart::REPLAYED.store(true, std::sync::atomic::Ordering::Release);
                let waiting = state.lock().unwrap().waiting_folders();
                fingerprint_changes(waiting, &mut fingerprints);
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
                        studio_server::refresh_landed(&shared, Some(session_id.clone()));
                        tell(app, Notice::YourTurn, &session_id, None);
                    }
                    Output::CommandBlocked { session_id, agent } => {
                        // The card's buttons are live now; its note is asked for beside the notification, which waits for nothing.
                        let asks = {
                            let studio = state.lock().unwrap();
                            let blocked = studio.session(&session_id);
                            blocked.map(|session| shared.notes.lock().unwrap().asks(session))
                        };
                        studio_server::ask_notes(&shared, asks.unwrap_or_default());
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
                        unsaved(shared.cards.lock().unwrap().promoted(&agent));
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
                            match agy_versions.check(&version, AGY_FIXTURE_VERSION, millis()) {
                                Ok(Some(notice)) => {
                                    log::warn!("{notice}");
                                    let _ = app.emit("notice", &notice);
                                }
                                Ok(None) => {}
                                Err(e) => log::warn!("keep agy's version: {e}"),
                            }
                        }
                        None => log::warn!("agy's version: `agy --version` did not run"),
                    },
                    // Reads the transcript's lines and nothing else (`AGENTS.md`).
                    Output::Version { transcript_path } => {
                        match versions.check(Path::new(&transcript_path), millis()) {
                            Ok(Some(notice)) => {
                                log::warn!("{notice}");
                                let _ = app.emit("notice", &notice);
                            }
                            Ok(None) => {}
                            Err(e) => log::warn!("keep Claude Code's version: {e}"),
                        }
                    }
                }
            }
            read = read_to > read_from;
            if read {
                let roots = state.lock().unwrap().known_roots();
                if roots.len() != roots_saved {
                    roots_saved = roots.len();
                    studio_server::save_roots(log, &roots);
                }
                let keys = state.lock().unwrap().live_keys();
                fingerprints.retain(|key, _| keys.contains(key));
                // The decision history goes to the store, which keeps it across restarts (danielrispler/Falinks#479).
                let studio = state.lock().unwrap();
                let mut store = shared.cards.lock().unwrap();
                for session in studio.sessions() {
                    unsaved(store.keep_decisions(&session.agent.id, &session.decisions));
                }
                settle_checks(&studio, &mut store);
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
        let list_changed = shared.list_changed.swap(false, Ordering::AcqRel);
        let files_changed = turn_files(state);
        let changed =
            read || transcripts_changed || windows_changed || list_changed || files_changed;
        // The open request goes in the same pass as the list that shows "needs you" (danielrispler/Falinks#569).
        if let Ok(data) = answer::data()
            && let Some(ask) = shared
                .watching
                .lock()
                .unwrap()
                .pass(&data, studio_hook::answer::now_ms())
        {
            let _ = app.emit("ask", ask);
        }
        if let Ok(data) = answer::data()
            && let Some(blocked) = shared
                .watching
                .lock()
                .unwrap()
                .changed_blocked_requests(&data)
        {
            let _ = app.emit("blocked", blocked);
        }
        // Asking for focus waits on the main thread, which never takes this lock.
        let focused = || {
            let window = app.get_webview_window("main");
            window.and_then(|w| w.is_focused().ok()) == Some(true)
        };
        // Daniel sees the selected session in a focused window while the story rail shows it (danielrispler/Falinks#556);
        // the window is asked only while it is unseen, and not under the core's lock, which the main
        // thread's commands take.
        let unseen = state.lock().unwrap().selected().is_some_and(|s| s.unseen);
        let shown = unseen && shared.rail_shown.load(Ordering::Acquire) && focused();
        let seen = state.lock().unwrap().see_selected(shown);
        if let Some((id, turns)) = &seen {
            unsaved(shared.cards.lock().unwrap().seen(id, *turns));
        }
        if state.lock().unwrap().tick(millis()) || changed || seen.is_some() {
            sent.list(app);
        }
        // Takes the core's lock under the notices' lock; `tell` never holds both.
        let needs_you = |id: &str| {
            let studio = state.lock().unwrap();
            studio.session(id).is_some_and(|s| s.state == NeedsYou)
        };
        let due = shared
            .notices
            .lock()
            .unwrap()
            .due(uptime(), focused, needs_you, || {
                let studio = state.lock().unwrap();
                let selected = studio.selected().map(|s| s.agent.id.clone());
                let mut on_screen: Vec<String> = selected.into_iter().collect();
                // The split's pane shows a terminal: a status-only row's id, or a session's in it.
                if let Some(terminal) = shared.split_shown.lock().unwrap().clone() {
                    let in_it = studio.sessions().into_iter();
                    let in_it = in_it.filter(|s| s.terminal.as_ref() == Some(&terminal));
                    on_screen.extend(in_it.map(|s| s.agent.id.clone()));
                    on_screen.push(terminal);
                }
                on_screen
            });
        if let Some((title, body)) = due.notification {
            let _ = app.notification().builder().title(title).body(body).show();
        }
        if let Some(sound) = due.sound {
            play(sound);
        }
        if !due.finished.is_empty() {
            let _ = app.emit("finished", &due.finished);
        }
        // The cards also change with no line read: a card arrives, or one turns 30 minutes old.
        sent.cards(app);
        live = true;
        // Also when the log did not open, so its replay block never ran.
        restart::REPLAYED.store(true, std::sync::atomic::Ordering::Release);
        thread::sleep(Duration::from_millis(200));
    }
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

/// The shell's part of Turn writing: what only it can do. It holds the app once the window's setup has it.
#[derive(Default)]
struct ShellEffects(OnceLock<AppHandle>);

impl Effects for ShellEffects {
    fn list(&self) {
        if let Some(app) = self.0.get() {
            let _ = app.emit(
                "sessions",
                studio_server::sessions(&app.state::<Arc<App>>()),
            );
        }
    }

    fn new_card(&self, session_id: &str, agent: Option<&str>) {
        if let Some(app) = self.0.get() {
            tell(app, Notice::Titled("New card"), session_id, agent);
        }
    }

    fn settle_checks(&self) {
        let Some(app) = self.0.get() else { return };
        let state = app.state::<Arc<App>>();
        settle_checks(
            &state.core.lock().unwrap(),
            &mut state.cards.lock().unwrap(),
        );
    }

    fn name(&self, session_id: &str, name: &str) {
        let line = serde_json::json!({
            "session_id": session_id,
            "hook_event_name": studio_core::PROMPT_NAME,
            "studio_name": name,
            "studio_received_ms": millis(),
        });
        if let Err(e) = studio_server::append(&[line]) {
            log::error!("save the name of {session_id}: {e}");
        }
    }

    fn log(&self, line: String) {
        report(line);
    }
}

/// Adds a notice on session `session_id`'s line, naming its subagent `agent`, if any; the read loop
/// shows it. Called without the core's lock held.
fn tell(app: &AppHandle, notice: Notice, session_id: &str, agent: Option<&str>) {
    let state = app.state::<Arc<App>>();
    let body = state.core.lock().unwrap().notice_body(session_id, agent);
    let mut notices = state.notices.lock().unwrap();
    match notice {
        Notice::Titled(title) => notices.push(title, body, uptime()),
        Notice::NeedsYou => notices.push_needs_you(session_id, body, uptime()),
        Notice::YourTurn => notices.push_your_turn(session_id, body, uptime()),
    }
}

/// What a notice says: that a session needs you, which shows only if it still needs Daniel when
/// it is due, or its own title.
enum Notice<'a> {
    NeedsYou,
    YourTurn,
    Titled(&'a str),
}

/// Plays a macOS system sound, so the studio bundles none (danielrispler/Falinks#556).
fn play(sound: Sound) {
    let name = match sound {
        Sound::NeedsYou => "Glass",
        Sound::YourTurn => "Tink",
    };
    let file = format!("/System/Library/Sounds/{name}.aiff");
    // Waited on its own thread, so no finished `afplay` is left a zombie.
    thread::spawn(move || {
        if let Err(e) = Command::new("afplay").arg(file).status() {
            log::warn!("play the {name} sound: {e}");
        }
    });
}

// A hover's text shows after half a second, not macOS's 1.5 s (#220). WKWebView's `title` tooltips are AppKit's, which read
// NSInitialToolTipDelay; it is registered, not written, so `defaults write dev.falinks.studio NSInitialToolTipDelay` still wins.
fn hover_delay() {
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDictionary, NSNumber, NSUserDefaults, ns_string};
    let delay = NSNumber::new_i32(500);
    let defaults = NSDictionary::<_, AnyObject>::from_slices(
        &[ns_string!("NSInitialToolTipDelay")],
        &[&*delay],
    );
    // SAFETY: the dictionary maps a string key to an NSNumber, the types registerDefaults expects.
    unsafe { NSUserDefaults::standardUserDefaults().registerDefaults(&defaults) };
}

/// Time since the app started, a monotonic clock for combining notices (danielrispler/Falinks#434, decision 28).
fn uptime() -> Duration {
    static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);
    STARTED.elapsed()
}

#[cfg(test)]
mod tests {
    use super::{in_app, log_line, shows_in_window};

    #[test]
    fn a_file_line_carries_the_local_time_level_and_target() {
        assert_eq!(
            log_line(
                [2026, 11, 18, 9, 5, 7],
                log::Level::Error,
                "studio",
                "no answer"
            ),
            "[2026-11-18][09:05:07][ERROR][studio] no answer"
        );
    }

    #[test]
    fn the_window_shows_an_app_error_and_not_a_warning_or_the_uis_own_error() {
        let on = |level, target| {
            shows_in_window(&log::Metadata::builder().level(level).target(target).build())
        };
        assert!(on(log::Level::Error, "studio"));
        assert!(!on(log::Level::Warn, "studio"));
        assert!(!on(log::Level::Error, "webview"));
        assert!(!on(log::Level::Error, "webview::main.js:3"));
    }

    #[test]
    fn a_panic_message_is_scrubbed_like_a_writer_line() {
        assert_eq!(
            super::panic_message(&"called `Result::unwrap()` on an `Err` value: Parse(\"secret\")"),
            "called `Result::unwrap()` on an `Err` value: Parse("
        );
    }

    #[test]
    fn the_window_stays_on_the_app() {
        let url = |s: &str| tauri::Url::parse(s).unwrap();
        assert!(in_app(&url("tauri://localhost/index.html")));
        assert!(in_app(&url("http://tauri.localhost/index.html")));
        assert!(!in_app(&url("https://example.com/")));
        assert!(!in_app(&url("http://tauri.localhost.example.com/")));
        assert!(!in_app(&url("file:///etc/passwd")));
        assert!(!in_app(&url("javascript:alert(1)")));
    }
}
