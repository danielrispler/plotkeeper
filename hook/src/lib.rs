//! The event log (danielrispler/Falinks#328, module 2): one append-only JSON Lines file, the only source of session state.

pub mod answer;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{self, Command},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

/// The studio's data directory, which is Tauri's app data directory for the identifier in
/// `src-tauri/tauri.conf.json`.
pub fn data_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support/dev.falinks.studio")
}

/// The log in the studio's data directory. The hook writes it; the app reads it.
pub fn event_log(home: &Path) -> PathBuf {
    data_dir(home).join("events.jsonl")
}

/// How long the hook waits for the log's lock before it drops the event (#43, decision 11).
pub const PATIENCE: Duration = Duration::from_secs(5);

/// The drops file beside the log `log`: a line `<Unix ms> <event> <terminal>` for each event the
/// hook dropped.
pub fn drops_file(log: &Path) -> PathBuf {
    log.with_file_name("drops.txt")
}

/// Appends `event` to the log at `log` as one line, with one write under an exclusive lock, so hooks
/// that fire together never interleave their lines. Waits up to `patience` for the lock; returns
/// `false`, having written nothing, when it never came.
pub fn append(log: &Path, event: &serde_json::Value, patience: Duration) -> std::io::Result<bool> {
    use std::io::Write;
    let mut line = serde_json::to_vec(event)?;
    line.push(b'\n');
    let Some(mut log) = locked(log, patience)? else {
        return Ok(false);
    };
    log.write_all(&line)?;
    Ok(true)
}

/// The log at `log`, open to append under its exclusive lock, which it waits up to `patience` for;
/// `None` when the lock never came. A prune puts a new file in the log's place while it holds the
/// lock (danielrispler/plotkeeper#132), so a file replaced while this waited is let go for the new one.
pub fn locked(log: &Path, patience: Duration) -> std::io::Result<Option<fs::File>> {
    use std::{fs::TryLockError, os::unix::fs::MetadataExt};
    let deadline = Instant::now() + patience;
    loop {
        let file = fs::OpenOptions::new().create(true).append(true).open(log)?;
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(20));
                }
                Err(TryLockError::WouldBlock) => return Ok(None),
                Err(TryLockError::Error(e)) => return Err(e),
            }
        }
        let ours = file.metadata().map(|m| (m.dev(), m.ino()))?;
        if fs::metadata(log).is_ok_and(|m| (m.dev(), m.ino()) == ours) {
            return Ok(Some(file));
        }
    }
}

/// Leaves out of hook event `event` the two fields no reader takes, a quarter of the log's bytes on
/// 2026-11-16 (danielrispler/plotkeeper#132): a response's `originalFile`, the whole file an `Edit` or
/// `Write` changed, and a `Read`'s file `content`. The hunks, output and errors stay. Returns whether
/// it left anything out.
pub fn lighten(event: &mut serde_json::Value) -> bool {
    use serde_json::Value;
    let read = event.get("tool_name").and_then(Value::as_str) == Some("Read");
    let response = event
        .get_mut("tool_response")
        .and_then(Value::as_object_mut);
    let Some(response) = response else {
        return false;
    };
    let mut left_out = response.remove("originalFile").is_some();
    if read && let Some(file) = response.get_mut("file").and_then(Value::as_object_mut) {
        left_out |= file.remove("content").is_some();
    }
    left_out
}

/// How long a session's events stay in the log after its last one (danielrispler/plotkeeper#132).
pub const KEEP: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The archive beside the log `log`, which a prune appends to and a launch never replays.
pub fn archive_file(log: &Path) -> PathBuf {
    log.with_file_name("events-archive.jsonl")
}

/// The state a replay of the log made, beside the log `log` (danielrispler/plotkeeper#134), which a
/// prune removes: it moves lines, so the byte offsets the state holds point elsewhere.
pub fn state_file(log: &Path) -> PathBuf {
    log.with_file_name("studio-state.json")
}

/// What a prune did: the lines it moved to the archive, and the kept lines it lightened.
#[derive(Debug, Default, PartialEq)]
pub struct Pruned {
    pub archived: usize,
    pub lightened: usize,
}

/// What a prune reads of a line: its session, Claude Code's or agy's, and when it was received.
#[derive(Default, serde::Deserialize)]
struct Stamp {
    session_id: Option<String>,
    #[serde(rename = "conversationId")]
    conversation_id: Option<String>,
    studio_received_ms: Option<u64>,
}

impl Stamp {
    fn of(line: &[u8]) -> Self {
        serde_json::from_slice(line).unwrap_or_default()
    }

    fn session(&self) -> Option<&str> {
        self.session_id
            .as_deref()
            .or(self.conversation_id.as_deref())
    }
}

/// Moves to the archive the lines of each session whose last event was received more than `KEEP`
/// before `now`, in Unix milliseconds, and lightens the lines it keeps (danielrispler/plotkeeper#132).
/// A line of no session goes by its own time; a line with no time stays. A log whose first line is
/// recent is left as it is, so most launches read one line. The new log takes the old one's place
/// under its lock, with the lines hooks appended meanwhile.
pub fn prune(log: &Path, now: u64) -> std::io::Result<Pruned> {
    use std::{
        collections::HashMap,
        io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
        os::unix::fs::MetadataExt,
    };
    let cutoff = now.saturating_sub(KEEP.as_millis() as u64);
    let file = match fs::File::open(log) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Pruned::default()),
        file => file?,
    };
    let mut first = Vec::new();
    BufReader::new(&file).read_until(b'\n', &mut first)?;
    if Stamp::of(&first)
        .studio_received_ms
        .is_some_and(|at| at >= cutoff)
    {
        return Ok(Pruned::default());
    }
    // Calls `each` with each whole line up to `end`, and returns the bytes they hold: a line a hook
    // is still writing waits for the tail.
    let lines = |end: u64, each: &mut dyn FnMut(&[u8]) -> io::Result<()>| -> io::Result<u64> {
        let mut reader = BufReader::new(fs::File::open(log)?.take(end));
        let (mut line, mut read) = (Vec::new(), 0);
        while reader.read_until(b'\n', &mut line)? > 0 && line.ends_with(b"\n") {
            read += line.len() as u64;
            each(&line)?;
            line.clear();
        }
        Ok(read)
    };
    let mut last = HashMap::<String, u64>::new();
    let end = lines(u64::MAX, &mut |line| {
        let stamp = Stamp::of(line);
        if let (Some(session), Some(at)) = (stamp.session(), stamp.studio_received_ms) {
            let last = last.entry(session.into()).or_default();
            *last = (*last).max(at);
        }
        Ok(())
    })?;
    let name = log.file_name().unwrap_or_default().to_string_lossy();
    let next = log.with_file_name(format!("{name}.next"));
    let mut kept = BufWriter::new(fs::File::create(&next)?);
    let mut archived = Vec::new();
    let mut pruned = Pruned::default();
    lines(end, &mut |line| {
        let stamp = Stamp::of(line);
        let session = stamp.session().and_then(|s| last.get(s));
        if session
            .copied()
            .or(stamp.studio_received_ms)
            .is_some_and(|at| at < cutoff)
        {
            archived.extend_from_slice(line);
            pruned.archived += 1;
        } else if let Some(light) = lightened(line) {
            kept.write_all(&light)?;
            pruned.lightened += 1;
        } else {
            kept.write_all(line)?;
        }
        Ok(())
    })?;
    if pruned == Pruned::default() {
        fs::remove_file(&next)?;
        return Ok(pruned);
    }
    // The archive first: a failure after it leaves a line in both files, never in neither.
    if !archived.is_empty() {
        let mut archive = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(archive_file(log))?;
        archive.write_all(&archived)?;
        archive.sync_all()?;
    }
    let held = locked(log, Duration::from_secs(60))?.ok_or(io::ErrorKind::TimedOut)?;
    let read = file.metadata().map(|m| (m.dev(), m.ino()))?;
    if held.metadata().map(|m| (m.dev(), m.ino()))? != read {
        fs::remove_file(&next)?;
        return Err(io::Error::other("the log was replaced during the prune"));
    }
    let mut tail = fs::File::open(log)?;
    tail.seek(SeekFrom::Start(end))?;
    io::copy(&mut tail, &mut kept)?;
    let kept = kept.into_inner().map_err(io::IntoInnerError::into_error)?;
    kept.sync_all()?;
    // Before the new log takes the old one's place: a failure between the two leaves no state, never
    // a state beside a log it does not fit.
    match fs::remove_file(state_file(log)) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    fs::rename(&next, log)?;
    drop(held);
    Ok(pruned)
}

/// Line `line` lightened, `None` when it has nothing to leave out.
fn lightened(line: &[u8]) -> Option<Vec<u8>> {
    let has = |text: &[u8]| line.windows(text.len()).any(|w| w == text);
    if !has(b"\"originalFile\"") && !has(b"\"content\"") {
        return None;
    }
    let mut event = serde_json::from_slice(line).ok()?;
    if !lighten(&mut event) {
        return None;
    }
    let mut light = serde_json::to_vec(&event).ok()?;
    light.push(b'\n');
    Some(light)
}

/// Notes in the drops file beside `log` that the hook dropped `event` of `terminal` at `at`, in Unix
/// milliseconds, with no lock and one small write.
pub fn drop_event(log: &Path, at: u64, event: &str, terminal: &str) -> std::io::Result<()> {
    use std::io::Write;
    // The fields are space-separated, so none may hold whitespace.
    let plain = |text: &str| text.split_whitespace().collect::<Vec<_>>().join("_");
    let line = format!("{at} {} {}\n", plain(event), plain(terminal));
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(drops_file(log))?;
    file.write_all(line.as_bytes())
}

/// What the hook knows of a session's latest turn, kept between its runs (danielrispler/Falinks#486): its prompt id, the
/// worktrees it recorded a start tree of, and whether it ended. It follows the core's turn rules
/// (danielrispler/Falinks#434, decisions 3 to 5).
#[derive(Debug, PartialEq)]
pub struct Turn {
    pub prompt_id: Option<String>,
    pub roots: Vec<String>,
    pub ended: bool,
}

impl Default for Turn {
    fn default() -> Self {
        Turn {
            prompt_id: None,
            roots: Vec::new(),
            ended: true,
        }
    }
}

impl Turn {
    /// Takes an event `name` of the session's own agent with `prompt_id`; `idle` when it is Claude
    /// Code's idle notice. Returns the worktrees whose end tree to record, those of the turn it
    /// ends, and whether it is in a turn whose first touches record a start tree.
    pub fn take(&mut self, name: &str, prompt_id: Option<&str>, idle: bool) -> (Vec<String>, bool) {
        let prompt = name == "UserPromptSubmit";
        let starts = !idle
            && prompt_id.is_some()
            && (self.prompt_id.as_deref() != prompt_id || prompt && self.ended);
        let keyed = self.prompt_id.is_some();
        let ends = !self.ended && (name == "Stop" || keyed && (idle || starts));
        let mut ended = Vec::new();
        if ends {
            ended = std::mem::take(&mut self.roots);
            self.ended = true;
        }
        if starts || prompt && self.ended {
            *self = Turn {
                prompt_id: prompt_id.map(Into::into),
                roots: Vec::new(),
                ended: name == "Stop",
            };
        }
        (ended, !self.ended && name != "Stop" && !idle)
    }
}

/// The root of the git worktree that holds `folder`, or its nearest existing parent, as `git`
/// spells it; `None` outside a worktree.
pub fn worktree_root(folder: &Path) -> Option<String> {
    let existing = folder.ancestors().find(|p| p.is_dir())?;
    git(existing, &["rev-parse", "--show-toplevel"], None)
}

/// The root of the git worktree that holds `folder`, or its nearest existing parent, and the git
/// tree of its files as they are now, untracked ones included and ignored ones not (danielrispler/Falinks#486). `None`
/// outside a worktree or when `git` fails. It writes the tree through a throwaway index, so the
/// working tree, the real index and `git status` stay as they were.
pub fn record_tree(folder: &Path) -> Option<(String, String)> {
    let root = worktree_root(folder)?;
    let index = git(
        Path::new(&root),
        &["rev-parse", "--git-path", "index"],
        None,
    )?;
    // One per call: Studio records trees from several threads at once.
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    let scratch = env::temp_dir().join(format!("studio-hook-{}-{call}.index", process::id()));
    // A copy of the real index keeps git's stat cache, so only changed files are hashed again.
    let _ = fs::copy(Path::new(&root).join(index), &scratch);
    let added = git(Path::new(&root), &["add", "-A"], Some(&scratch));
    let tree = added.and_then(|_| git(Path::new(&root), &["write-tree"], Some(&scratch)));
    let _ = fs::remove_file(&scratch);
    Some((root, tree?))
}

/// `git`'s trimmed output in `folder`, with `index` as its index when given, or `None` when it
/// fails. It takes no optional locks, so it never holds up the agent's own `git` commands.
fn git(folder: &Path, args: &[&str], index: Option<&Path>) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(folder);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    let output = command.output().ok()?;
    let out = String::from_utf8(output.stdout).ok()?;
    output.status.success().then(|| out.trim_end().to_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Turn;

    /// The hook's view of one session's turns, after `events`, each a name and a prompt id.
    fn after(turn: &mut Turn, name: &str, prompt: &str) -> (Vec<String>, bool) {
        turn.take(name, Some(prompt), false)
    }

    #[test]
    fn a_turn_records_its_starts_until_its_stop_ends_every_worktree_it_touched() {
        let mut turn = Turn::default();
        assert_eq!(after(&mut turn, "UserPromptSubmit", "p1"), (vec![], true));
        turn.roots.push("/w/a".into());
        turn.roots.push("/w/b".into());
        // A prompt queued with the running turn's id keeps the turn.
        assert_eq!(after(&mut turn, "UserPromptSubmit", "p1"), (vec![], true));
        let ended = vec!["/w/a".to_owned(), "/w/b".to_owned()];
        assert_eq!(after(&mut turn, "Stop", "p1"), (ended, false));
        // A late step of the ended turn records nothing.
        assert_eq!(after(&mut turn, "PostToolUse", "p1"), (vec![], false));
        assert_eq!(after(&mut turn, "UserPromptSubmit", "p1"), (vec![], true));
        assert!(turn.roots.is_empty());
    }

    /// Esc fires no `Stop`: the next turn's first event ends it, as the core sees it.
    #[test]
    fn the_next_turn_or_an_idle_notice_ends_a_turn_without_a_stop() {
        let mut turn = Turn::default();
        after(&mut turn, "UserPromptSubmit", "p1");
        turn.roots.push("/w/a".into());
        assert_eq!(
            after(&mut turn, "PreToolUse", "p2"),
            (vec!["/w/a".into()], true)
        );
        turn.roots.push("/w/a".into());
        assert_eq!(
            turn.take("Notification", Some("p2"), true),
            (vec!["/w/a".into()], false)
        );
    }

    #[test]
    fn the_log_lives_in_the_data_directory_of_the_apps_identifier() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../src-tauri/tauri.conf.json")).unwrap();
        let identifier = conf["identifier"].as_str().unwrap();
        let data = Path::new("/h/Library/Application Support").join(identifier);
        assert_eq!(super::event_log(Path::new("/h")), data.join("events.jsonl"));
    }
}
