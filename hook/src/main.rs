//! The hook plugin's command (#328, module 1): appends the hook payload on stdin as one line to the
//! event log, with the terminal's `STUDIO_TERMINAL_ID` when set and the time it received the event.
//! It does nothing else. It prints nothing on stdout and always exits 0, so it never blocks the agent.

use serde_json::{Map, Value};
use std::{
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::{Read, Write},
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    if let Err(e) = append() {
        eprintln!("studio-hook: {e}");
    }
}

fn append() -> Result<(), Box<dyn Error>> {
    let received_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload)?;
    let mut event: Map<String, Value> = serde_json::from_slice(&payload)?;
    if let Ok(terminal) = env::var("STUDIO_TERMINAL_ID") {
        event.insert("studio_terminal_id".into(), terminal.into());
    }
    event.insert("studio_received_ms".into(), received_ms.into());
    let mut line = serde_json::to_vec(&event)?;
    line.push(b'\n');

    let path = studio_hook::event_log(&env::home_dir().ok_or("no home directory")?);
    fs::create_dir_all(path.parent().ok_or("no data directory")?)?;
    let mut log = OpenOptions::new().create(true).append(true).open(&path)?;
    // One write under an exclusive lock, so hooks that fire together never interleave their lines.
    log.lock()?;
    log.write_all(&line)?;
    Ok(())
}
