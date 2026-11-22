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
    process::Command,
    sync::{Arc, mpsc},
    thread,
};
use studio_core::{Sound, State::Ended};
use studio_server::{App, Cleared, clear_commit, resolve_clearing, scrubbed, session_ends};
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
    studio_server::uptime();
    hover_delay();
    let home = std::env::home_dir().expect("a home directory");
    studio_hook::move_old_data_dir(&home).expect("move the old data folder");
    let log = studio_hook::event_log(&home);
    // The window does not exist yet, so the writer's notices wait here for the setup to forward them.
    let (writer_notices, writer_notice_reader) = mpsc::channel::<Option<String>>();
    let effects = Arc::new(studio_server::ServerEffects::default());
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
            effects.attach(&state);
            state.events.set(Arc::new(TauriSink(app.clone())));
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
            let follow_state = state.clone();
            thread::spawn(move || studio_server::follow(&follow_state, &log));
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

/// The window is the read loop's sink: events are emitted to it, and focus, the Dock badge, notifications and sounds are its.
struct TauriSink(AppHandle);

impl studio_server::Sink for TauriSink {
    fn emit(&self, name: &str, payload: serde_json::Value) {
        let _ = self.0.emit(name, payload);
    }

    fn focused(&self) -> bool {
        let window = self.0.get_webview_window("main");
        window.and_then(|w| w.is_focused().ok()) == Some(true)
    }

    fn badge(&self, count: Option<usize>) {
        if let Some(window) = self.0.get_webview_window("main") {
            let _ = window.set_badge_count(count.map(|n| n as i64));
        }
    }

    fn notify(&self, title: &str, body: &str) {
        let _ = self
            .0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }

    fn play(&self, sound: Sound) {
        play(sound);
    }

    fn replayed(&self) {
        restart::REPLAYED.store(true, std::sync::atomic::Ordering::Release);
    }
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
