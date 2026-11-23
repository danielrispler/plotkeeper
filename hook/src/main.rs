//! The hook plugin's command (danielrispler/Falinks#328, module 1): appends the hook payload on stdin as one line to the
//! event log, with the terminal's `PLOTKEEPER_TERMINAL_ID` when set, the agent its hook entry names
//! (`--agent claude`, #355) and the time it received the event.
//! At a turn's start and end it also records the git tree of each worktree the turn touched (danielrispler/Falinks#486).
//! It does nothing else. It prints nothing on stdout and always exits 0, so it never blocks the agent.

use serde_json::{Map, Value};
use std::{
    env,
    error::Error,
    fs,
    io::Read,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn main() {
    if let Err(e) = append() {
        eprintln!("plotkeeper-hook: {e}");
    }
}

fn append() -> Result<(), Box<dyn Error>> {
    let received_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
    let mut payload = Vec::new();
    std::io::stdin().read_to_end(&mut payload)?;
    let mut event: Map<String, Value> = serde_json::from_slice(&payload)?;
    // Plotkeeper's own model runs (the outside review, the card writer) are never recorded (#323).
    if env::var_os("PLOTKEEPER_INTERNAL").is_some_and(|v| v == "1") {
        return Ok(());
    }
    let terminal = env::var("PLOTKEEPER_TERMINAL_ID");
    if let Ok(terminal) = terminal {
        event.insert("studio_terminal_id".into(), terminal.into());
    }
    // The agent's process, so the studio ends a session whose agent is gone (#330).
    let wrapper = env::var("PLOTKEEPER_AGENT_PID").ok();
    let parent = std::os::unix::process::parent_id();
    if let Some(pid) = studio_hook::agent_pid(wrapper.as_deref(), parent, studio_hook::inspect) {
        event.insert("studio_pid".into(), pid.into());
    }
    // The agent the hook entry names, so the line is read by its agent's parser, not its shape (#355).
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--agent"
            && let Some(agent) = args.next()
        {
            event.insert("studio_agent".into(), agent.into());
        }
    }
    event.insert("studio_received_ms".into(), received_ms.into());
    let home = env::home_dir().ok_or("no home directory")?;
    // The old data folder moves before anything is created in the new one (#322).
    studio_hook::move_old_data_dir(&home)?;
    let path = studio_hook::event_log(&home);
    let data = path.parent().ok_or("no data directory")?;
    fs::create_dir_all(data)?;
    // The line goes in the log even when the trees cannot be recorded.
    if let Err(e) = studio_hook::trees(&mut event, data) {
        eprintln!("plotkeeper-hook: record the trees: {e}");
    }
    // A lock that never comes must not hold the agent: the event is dropped, with a trace (#43,
    // decision 11).
    let text = |key: &str| event.get(key)?.as_str().map(str::to_owned);
    let name = text("hook_event_name").or_else(|| text("hookEventName"));
    let terminal = text("studio_terminal_id");
    // Tests shorten the 5 seconds.
    let patience = env::var("PLOTKEEPER_HOOK_PATIENCE_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(studio_hook::PATIENCE, Duration::from_millis);
    let mut event = event.into();
    studio_hook::lighten(&mut event);
    if !studio_hook::append(&path, &event, patience)? {
        let (name, terminal) = (name.as_deref(), terminal.as_deref());
        studio_hook::drop_event(
            &path,
            received_ms,
            name.unwrap_or("-"),
            terminal.unwrap_or("-"),
        )?;
    }
    Ok(())
}
