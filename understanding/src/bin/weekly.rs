//! The weekly check on Claude Code's drift (danielrispler/Falinks#369, decision 23): prints the unrecognised wake-ups in the
//! event log and the version changes of the last 7 days, then the events the hook dropped (#43, decision 11). The thinking and
//! surprise rates and the ratings it printed before cards were retired (danielrispler/plotkeeper#348) are gone. Words after the
//! command are Daniel's one-line verdict, which it appends with the rest to `weekly.txt` beside the event log.

use std::{fs, fs::OpenOptions, io::Write};
use studio_core::Studio;
use studio_understanding::{Versions, drift, dropped_events, millis};

fn main() -> std::io::Result<()> {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    let now = millis();
    let lines = String::from_utf8_lossy(&fs::read(&log).unwrap_or_default()).into_owned();
    let studio = Studio::replayed(lines.lines());
    let drift = drift(&studio, &Versions::beside(&log), now);
    let drops = format!("dropped events: {}\n", dropped_events(&log, now));
    print!("{drift}{drops}");
    let verdict = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if !verdict.is_empty() {
        let weekly = log.with_file_name("weekly.txt");
        let mut file = OpenOptions::new().create(true).append(true).open(&weekly)?;
        writeln!(file, "{drift}{drops}verdict: {verdict}\n")?;
        println!("Kept your verdict in {}", weekly.display());
    }
    Ok(())
}
