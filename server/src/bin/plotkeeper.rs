//! `plotkeeper`: serves the learning layer as a local page and opens the browser on it (danielrispler/plotkeeper#325, spec #319).
//! Started by hand in a terminal. A second one for the same data folder opens the browser on the first and exits; Ctrl+C stops it.
//!
//! `PLOTKEEPER_NO_BROWSER=1` leaves the browser closed (the URL is printed either way), for tests.

use std::{
    process::{Command, ExitCode},
    sync::{Arc, mpsc},
    thread,
};
use studio_server::{
    App, ServerEffects,
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
    let _ = log::set_logger(&Stderr).map(|()| log::set_max_level(log::LevelFilter::Warn));
    match run(&log, held) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("plotkeeper: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(log: &std::path::Path, held: lock::Held) -> std::io::Result<()> {
    let (writer_notices, writer_notice_reader) = mpsc::channel::<Option<String>>();
    let effects = Arc::new(ServerEffects::default());
    let app = Arc::new(App::open(log, effects.clone(), writer_notices)?);
    effects.attach(&app);
    // The writer settings.json names takes effect at start, with no page open to save it.
    app.writer
        .use_writer(studio_server::settings::summary_writer());
    let token = web::new_token();
    let page = Web::start(
        app.clone(),
        embedded::ASSETS,
        token.clone(),
        web::DEFAULT_PORT,
    )?;
    app.events.set(page.sink());
    held.publish(page.port(), &token)?;

    let prs = app.clone();
    thread::spawn(move || studio_server::prs(&prs));
    let retests = app.clone();
    thread::spawn(move || studio_server::write_due_retests(&retests));
    let notices = app.clone();
    thread::spawn(move || {
        for notice in writer_notice_reader {
            // The notice is the writer's own words; the log keeps only that one arrived or cleared.
            log::warn!(
                "card writer notice {}",
                if notice.is_some() { "shown" } else { "cleared" }
            );
            notices
                .events
                .emit("writer-notice", serde_json::json!(notice));
        }
    });
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
