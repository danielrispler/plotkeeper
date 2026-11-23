//! `plotkeeper`: serves the learning layer as a local page and opens the browser on it (danielrispler/plotkeeper#325, spec #319).
//! Started by hand in a terminal. A second one for the same data folder opens the browser on the first and exits; Ctrl+C stops it.
//!
//! `plotkeeper install` wires the agents it finds to plotkeeper's hooks, and `plotkeeper uninstall` reverses it (#357).
//!
//! Each start refreshes the hooks' copies of `plotkeeper-hook` and `plotkeeper-answer` in the data folder's `bin/` (#356).
//!
//! `PLOTKEEPER_NO_BROWSER=1` leaves the browser closed (the URL is printed either way), for tests.

use std::{
    process::{Command, ExitCode},
    sync::{Arc, mpsc},
    thread,
};
use studio_server::{
    App, ServerEffects, hook_copies, install,
    lock::{self, Instance},
    web::{self, Web},
};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/assets.rs"));
}

/// Warnings and errors to stderr, with the time.
struct Stderr;

impl log::Log for Stderr {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{}][{}] {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }
    fn flush(&self) {}
}

fn address(port: u16, token: &str) -> String {
    format!("http://127.0.0.1:{port}/?token={token}")
}

/// Prints the address and opens the browser on it, unless `PLOTKEEPER_NO_BROWSER` is set.
fn show(url: &str) {
    println!("plotkeeper: {url}");
    if std::env::var_os("PLOTKEEPER_NO_BROWSER").is_some() {
        return;
    }
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = Command::new(opener).arg(url).status() {
        eprintln!("plotkeeper: could not open the browser ({e}); open the address above");
    }
}

fn main() -> ExitCode {
    studio_server::uptime();
    let Some(home) = std::env::home_dir() else {
        eprintln!("plotkeeper: no home folder");
        return ExitCode::FAILURE;
    };
    if let Err(e) = studio_hook::move_old_data_dir(&home) {
        eprintln!("plotkeeper: could not move the old data folder: {e}");
        return ExitCode::FAILURE;
    }
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some(verb @ ("install" | "uninstall")) => return wire(verb, &home),
        Some(other) => {
            eprintln!(
                "plotkeeper: unknown command {other:?}; run plotkeeper, plotkeeper install or plotkeeper uninstall"
            );
            return ExitCode::FAILURE;
        }
    }
    let log = studio_hook::event_log(&home);
    let data = studio_hook::data_dir(&home);
    let held = match lock::acquire(&data) {
        Ok(Instance::First(held)) => held,
        Ok(Instance::Second(running)) => {
            show(&address(running.port, &running.token));
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("plotkeeper: {e}");
            return ExitCode::FAILURE;
        }
    };
    refresh_hook_copies(&home);
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Warn));
    match run(&log, held) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("plotkeeper: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The folder this executable is in, where the hook binaries ship.
fn beside() -> std::io::Result<std::path::PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    Ok(exe
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default())
}

/// `plotkeeper install` or `plotkeeper uninstall`, for the agents on the PATH.
fn wire(verb: &str, home: &std::path::Path) -> ExitCode {
    println!("plotkeeper {verb}");
    let path = std::env::var_os("PATH");
    let mut out = std::io::stdout();
    let done = if verb == "install" {
        beside()
            .map_err(|e| format!("could not find its own executable ({e})"))
            .and_then(|beside| install::install(home, &beside, path.as_deref(), &mut out))
    } else {
        install::uninstall(home, path.as_deref(), &mut out)
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("plotkeeper {verb}: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Brings the hooks' copies in the data folder's `bin/` up to the hook binaries beside this executable (#356), so an
/// upgrade reaches the hooks at this start. A failure is said and the server starts anyway.
fn refresh_hook_copies(home: &std::path::Path) {
    let beside = match beside() {
        Ok(beside) => beside,
        Err(e) => {
            eprintln!(
                "plotkeeper: could not find its own executable ({e}); the hooks keep the copies they have"
            );
            return;
        }
    };
    match hook_copies::refresh(&beside, &studio_hook::bin_dir(home)) {
        Ok(done) => {
            if let Some(line) = hook_copies::notice(&done, &beside) {
                eprintln!("{line}");
            }
        }
        Err(e) => eprintln!("plotkeeper: could not refresh the hooks' copies ({e})"),
    }
}

fn run(log: &std::path::Path, held: lock::Held) -> std::io::Result<()> {
    let (writer_notices, writer_notice_reader) = mpsc::channel::<Option<String>>();
    // The Test gate is read before the writer makes its folder, which the gate's first look reads (spec #320).
    let summaries_on = studio_server::settings::summaries_open(log.parent().unwrap_or(log));
    let effects = Arc::new(ServerEffects::default());
    let app = Arc::new(App::open(log, effects.clone(), writer_notices)?);
    effects.attach(&app);
    // The writer settings.json names takes effect at start, with no page open to save it.
    app.writer
        .use_writer(studio_server::settings::summary_writer());
    studio_server::set_summaries_on(&app, summaries_on);
    let token = web::new_token();
    let page = Web::start(
        app.clone(),
        embedded::ASSETS,
        token.clone(),
        web::DEFAULT_PORT,
    )?;
    app.events.set(page.sink());
    held.publish(page.port(), &token)?;

    studio_server::forward_writer_notices(&app, writer_notice_reader);
    let following = app.clone();
    let log = log.to_path_buf();
    thread::spawn(move || studio_server::follow(&following, &log));

    let (stop, stopped) = mpsc::channel();
    ctrlc::set_handler(move || {
        let _ = stop.send(());
    })
    .map_err(std::io::Error::other)?;
    show(&address(page.port(), &token));
    let _ = stopped.recv();
    // The stores write by temp file and rename, so a stop between writes leaves them whole.
    page.stop();
    drop(held);
    Ok(())
}
