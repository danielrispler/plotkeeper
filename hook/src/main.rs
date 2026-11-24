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
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// How long the trees of a Codex or Cursor line may take. Their hook entries give the hook 5
/// seconds, and one `git` call may take 20, so past this the line is logged without its trees
/// rather than killed with them; the append's lock wait takes the rest.
const TREES_BUDGET: Duration = Duration::from_secs(3);

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
    // The agent the hook entry names, so the line is read by its agent's parser, not its shape (#355).
    // The event it names, for a payload that names none (Copilot's camelCase ones, #361).
    let (mut agent, mut named) = (None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--agent" => agent = args.next(),
            "--event" => named = args.next(),
            _ => {}
        }
    }
    // Another harness's event, which its own entry records, is dropped here (#362).
    if let Some(harness) = studio_hook::foreign(agent.as_deref(), &event) {
        let name = event.get("hook_event_name").and_then(Value::as_str);
        eprintln!(
            "plotkeeper-hook: dropped {harness}'s {} event on the {} entry; {harness}'s own entry records it",
            name.unwrap_or("-"),
            agent.as_deref().unwrap_or("-"),
        );
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
    if let Some(agent) = agent {
        event.insert("studio_agent".into(), agent.into());
    }
    if let Some(named) = named {
        event.insert("studio_event".into(), named.into());
    }
    event.insert("studio_received_ms".into(), received_ms.into());
    let home = env::home_dir().ok_or("no home directory")?;
    // The old data folder moves before anything is created in the new one (#322).
    studio_hook::move_old_data_dir(&home)?;
    let path = studio_hook::event_log(&home);
    let data = path.parent().ok_or("no data directory")?;
    fs::create_dir_all(data)?;
    // The line goes in the log even when the trees cannot be recorded.
    if let Err(e) = trees(&mut event, data) {
        eprintln!("plotkeeper-hook: record the trees: {e}");
    }
    // A lock that never comes must not hold the agent: the event is dropped, with a trace (#43,
    // decision 11).
    let text = |key: &str| event.get(key)?.as_str().map(str::to_owned);
    let name = (text("hook_event_name").or_else(|| text("hookEventName")))
        .or_else(|| text("studio_event"));
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

/// `studio_hook::trees` on `event`, given `TREES_BUDGET` for a Codex or Cursor line; past it the
/// line keeps none of the trees, and the hook's exit stops their work.
fn trees(event: &mut Map<String, Value>, data: &Path) -> Result<(), String> {
    let agent = event.get("studio_agent").and_then(Value::as_str);
    if !matches!(agent, Some("codex" | "cursor")) {
        return studio_hook::trees(event, data).map_err(|e| e.to_string());
    }
    let (mut copy, data) = (event.clone(), data.to_owned());
    let (done, recorded) = mpsc::channel();
    thread::spawn(move || {
        let result = studio_hook::trees(&mut copy, &data).map_err(|e| e.to_string());
        let _ = done.send((copy, result));
    });
    let (copy, result) = recorded
        .recv_timeout(TREES_BUDGET)
        .map_err(|_| format!("they took over {TREES_BUDGET:?}"))?;
    *event = copy;
    result
}
