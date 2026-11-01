//! The weekly check on the questions (#328, module 6): prints the last 7 days' thinking and surprise
//! rates and Daniel's ratings by level and reason from the card store, then the drift in Claude Code
//! (#369, decision 23): the unrecognised wake-ups in the event log and the version changes. Words after the command are Daniel's one-line
//! verdict, which it appends with the rest to `weekly.txt` beside the store.

use std::{fs, fs::OpenOptions, io::Write};
use studio_core::Studio;
use studio_understanding::{CardStore, Versions, drift, millis, week};

fn main() -> std::io::Result<()> {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    let store = CardStore::open_beside(&log)?;
    let now = millis();
    let rates = week(store.cards(), now);
    let mut studio = Studio::default();
    for line in String::from_utf8_lossy(&fs::read(&log).unwrap_or_default()).lines() {
        studio.replay(line);
    }
    let drift = drift(&studio, &Versions::beside(&log), now);
    print!("{rates}{drift}");
    let verdict = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if !verdict.is_empty() {
        let weekly = log.with_file_name("weekly.txt");
        let mut file = OpenOptions::new().create(true).append(true).open(&weekly)?;
        writeln!(file, "{rates}{drift}verdict: {verdict}\n")?;
        println!("Kept your verdict in {}", weekly.display());
    }
    Ok(())
}
