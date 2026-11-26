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

/// How long the trees of a Codex or Cursor line may take, how long its append may wait for the
/// log's lock, and how long the hook waits, once the line is logged, for trees it gave up to stop
/// their `git`. Their hook entries give the hook 5 seconds, Codex's `Interrupt` and `SessionEnd` 3
/// (plugin-codex/hooks.json, plugin-cursor/hooks.json), and one `git` call may take 20: the three
/// add up to 2.4 seconds, so past the budget the line is logged without its trees rather than killed
/// with them.
// shortcut: the budget counts from the thread's start, its wait for the turns file's lock included,
// so a second hook of one session that overlaps the first (Codex runs them async) can run out
// waiting; a `Stop` given up so leaves its end trees to the next prompt (`Turn::take`), which then
// also see the edits made between the turns.
const TREES_BUDGET: Duration = Duration::from_millis(1200);
const TIMED_PATIENCE: Duration = Duration::from_secs(1);
const STOP_BUDGET: Duration = Duration::from_millis(200);

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
    // The agent's process, so the studio ends a session whose agent is gone (#330). OpenCode's
    // plugin runs in its shared background service, which outlives its sessions (#370).
    let wrapper = env::var("PLOTKEEPER_AGENT_PID").ok();
    let parent = std::os::unix::process::parent_id();
    let shared = agent.as_deref() == Some("opencode");
    if let Some(pid) =
        studio_hook::agent_pid(wrapper.as_deref(), parent, studio_hook::inspect).filter(|_| !shared)
    {
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
    let timed = matches!(
        event.get("studio_agent").and_then(Value::as_str),
        Some("codex" | "cursor")
    );
    // Tests that check what the trees hold, not their budget, set a longer one.
    let budget = env::var("PLOTKEEPER_HOOK_TREES_BUDGET_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map_or(TREES_BUDGET, Duration::from_millis);
    let mut given_up = None;
    if let Err(e) = trees(&mut event, data, timed.then_some(budget), &mut given_up) {
        eprintln!("plotkeeper-hook: record the trees: {e}");
    }
    // A lock that never comes must not hold the agent: the event is dropped, with a trace (#43,
    // decision 11).
    let text = |key: &str| event.get(key)?.as_str().map(str::to_owned);
    let name = (text("hook_event_name").or_else(|| text("hookEventName")))
        .or_else(|| text("studio_event"));
    let terminal = text("studio_terminal_id");
    // Tests set their own.
    let patience = env::var("PLOTKEEPER_HOOK_PATIENCE_MS")
        .ok()
        .and_then(|ms| ms.parse().ok())
        .map(Duration::from_millis);
    let patience = patience.unwrap_or(if timed {
        TIMED_PATIENCE
    } else {
        studio_hook::PATIENCE
    });
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
    // Trees given up stop their `git` and remove its scratch index in a moment.
    if let Some(stopped) = given_up {
        // A closed channel or a timeout both mean carry on.
        stopped.recv_timeout(STOP_BUDGET).ok();
    }
    Ok(())
}

/// What the thread recording the trees sends back: the event with its trees, and how `studio_hook::trees` ended.
type Recorded = (Map<String, Value>, Result<(), String>);

/// `studio_hook::trees` on `event`, given `budget` when there is one; past it the line keeps none
/// of the trees, which are given up (`studio_hook::abandon_trees`), and `given_up` gets what says
/// their work has stopped.
fn trees(
    event: &mut Map<String, Value>,
    data: &Path,
    budget: Option<Duration>,
    given_up: &mut Option<mpsc::Receiver<Recorded>>,
) -> Result<(), String> {
    let Some(budget) = budget else {
        return studio_hook::trees(event, data).map_err(|e| e.to_string());
    };
    let (mut copy, data) = (event.clone(), data.to_owned());
    let (done, recorded) = mpsc::channel();
    thread::spawn(move || {
        let result = studio_hook::trees(&mut copy, &data).map_err(|e| e.to_string());
        // The receiver may have gone, and then no one waits for this.
        done.send((copy, result)).ok();
    });
    let (copy, result) = match recorded.recv_timeout(budget) {
        Ok(recorded) => recorded,
        Err(_) if studio_hook::abandon_trees() => {
            *given_up = Some(recorded);
            return Err(format!("they took over {budget:?}"));
        }
        // They were kept as the budget ran out: only their record's write is left.
        Err(_) => recorded.recv().map_err(|e| e.to_string())?,
    };
    *event = copy;
    result
}
