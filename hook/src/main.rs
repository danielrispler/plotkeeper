//! The hook plugin's command (#328, module 1): appends the hook payload on stdin as one line to the
//! event log, with the terminal's `STUDIO_TERMINAL_ID` when set and the time it received the event.
//! At a turn's start and end it also records the git tree of each worktree the turn touched (#486).
//! It does nothing else. It prints nothing on stdout and always exits 0, so it never blocks the agent.

use serde_json::{Map, Value, json};
use std::{
    env,
    error::Error,
    fs::{self, OpenOptions},
    io::{Read, Seek, Write},
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use studio_hook::{Turn, record_tree, worktree_root};

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
    let path = studio_hook::event_log(&env::home_dir().ok_or("no home directory")?);
    let data = path.parent().ok_or("no data directory")?;
    fs::create_dir_all(data)?;
    // The line goes in the log even when the trees cannot be recorded.
    if let Err(e) = trees(&mut event, data) {
        eprintln!("studio-hook: record the trees: {e}");
    }
    let mut line = serde_json::to_vec(&event)?;
    line.push(b'\n');

    let mut log = OpenOptions::new().create(true).append(true).open(&path)?;
    // One write under an exclusive lock, so hooks that fire together never interleave their lines.
    log.lock()?;
    log.write_all(&line)?;
    Ok(())
}

/// Adds to `event` the trees of the turn it ends, `studio_turn_end`, and of the worktrees its turn
/// touches first with it, `studio_turn_start`, each by worktree root. Like the core, it takes a
/// session's own events, and its folder and an `Edit` or `Write` path as what it touches (#434,
/// decision 9). What it knows of the turn is kept in `turns/` in the data directory `data`, until the
/// session ends or, if its end never comes, for 7 days.
fn trees(event: &mut Map<String, Value>, data: &Path) -> Result<(), Box<dyn Error>> {
    let text = |key: &str| event.get(key).and_then(Value::as_str);
    let name = text("hook_event_name").unwrap_or_default();
    let idle = name == "Notification" && text("notification_type") == Some("idle_prompt");
    let outside = matches!(name, "SessionStart" | "SessionEnd" | "Notification");
    let session = text("session_id").unwrap_or_default();
    // The id names a file, so it must be a plain one.
    let plain = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
    let plain = !session.is_empty() && session.chars().all(plain);
    let turns = data.join("turns");
    let state = turns.join(format!("{session}.json"));
    // A session's record goes with it; a start, which is rare, prunes those whose end never came.
    if name == "SessionEnd" && plain {
        let _ = fs::remove_file(&state);
    }
    if name == "SessionStart" {
        prune(&turns);
    }
    if event.contains_key("agent_id") || outside && !idle || !plain {
        return Ok(());
    }
    fs::create_dir_all(&turns)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&state)?;
    // Hooks of one session that fire together take turns, so each sees the last one's record.
    file.lock()?;
    let mut kept = String::new();
    file.read_to_string(&mut kept)?;
    let kept: Value = serde_json::from_str(&kept).unwrap_or_default();
    let roots = kept["roots"].as_array().into_iter().flatten();
    let mut turn = Turn {
        prompt_id: kept["prompt_id"].as_str().map(Into::into),
        roots: roots.filter_map(|r| Some(r.as_str()?.to_owned())).collect(),
        ended: kept["ended"].as_bool().unwrap_or(true),
    };

    let (ended, in_turn) = turn.take(name, text("prompt_id"), idle);
    let end: Map<String, Value> = ended.iter().filter_map(|root| record(root)).collect();
    let mut start = Map::new();
    if in_turn {
        let tool = text("tool_name");
        let path = event
            .get("tool_input")
            .and_then(|input| input.get("file_path")?.as_str());
        let path = path.filter(|p| p.starts_with('/') && matches!(tool, Some("Edit" | "Write")));
        let folder = path.and_then(|p| Path::new(p).parent()?.to_str());
        for folder in [text("cwd"), folder].into_iter().flatten() {
            let Some(root) = worktree_root(Path::new(folder)) else {
                continue;
            };
            if !turn.roots.contains(&root)
                && let Some((root, tree)) = record(&root)
            {
                turn.roots.push(root.clone());
                start.insert(root, tree);
            }
        }
    }
    let kept = json!({ "prompt_id": turn.prompt_id, "roots": turn.roots, "ended": turn.ended });
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(kept.to_string().as_bytes())?;
    for (key, trees) in [("studio_turn_end", end), ("studio_turn_start", start)] {
        if !trees.is_empty() {
            event.insert(key.into(), trees.into());
        }
    }
    Ok(())
}

/// Deletes the records in `turns` unchanged for 7 days.
fn prune(turns: &Path) {
    let week = Duration::from_secs(7 * 24 * 3600);
    let Ok(entries) = fs::read_dir(turns) else {
        return;
    };
    for entry in entries.flatten() {
        let modified = entry.metadata().and_then(|m| m.modified());
        let age = modified.ok().and_then(|t| t.elapsed().ok());
        if age.is_some_and(|age| age > week) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Worktree `root`'s tree now, as a JSON pair.
fn record(root: &str) -> Option<(String, Value)> {
    record_tree(Path::new(root)).map(|(root, tree)| (root, tree.into()))
}
