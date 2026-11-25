//! The event log (danielrispler/Falinks#328, module 2): one append-only JSON Lines file, the only source of session state.

pub mod answer;

use std::{
    env,
    ffi::OsStr,
    fs,
    io::{BufRead, BufReader, Read},
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{self, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

/// The data folder, `~/Library/Application Support/plotkeeper/`: the one place that names it.
pub fn data_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support").join(DATA_FOLDER)
}

const DATA_FOLDER: &str = "plotkeeper";

/// The data folder's `bin/`, where the hooks find their binaries at a path that never moves (#356): copies of
/// [`HOOK_BINARIES`], which each `plotkeeper` start refreshes from the ones beside its own executable.
pub fn bin_dir(home: &Path) -> PathBuf {
    data_dir(home).join("bin")
}

/// The binaries the hooks run, which ship beside `plotkeeper` and are copied into [`bin_dir`].
pub const HOOK_BINARIES: [&str; 2] = ["plotkeeper-hook", "plotkeeper-answer"];

/// The folder the data lived in before it was named for plotkeeper (#322).
const OLD_DATA_FOLDER: &str = "dev.falinks.studio";

/// The one move (#322): when the old data folder exists as a real folder and the new one does not,
/// renames it to the new name and leaves a symbolic link at the old path, so the plugin path in
/// `~/.zshrc` (through the folder's `checkout` link) and a hook still built with the old path keep
/// working. Never moves over an existing new folder. Returns whether it moved.
pub fn move_old_data_dir(home: &Path) -> std::io::Result<bool> {
    let (new, old) = (
        data_dir(home),
        home.join("Library/Application Support")
            .join(OLD_DATA_FOLDER),
    );
    let is_dir = |p: &Path| fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
    if !is_dir(&old) || fs::symlink_metadata(&new).is_ok() {
        return Ok(false);
    }
    // A second process that moved first leaves a link at `old` and a folder at `new`; losing the
    // race is not an error.
    if fs::rename(&old, &new).is_err() {
        return Ok(false);
    }
    leave_link(&old)?;
    Ok(true)
}

/// The symbolic link at `old`, which now names the new folder. Something that took the old path after the rename
/// (a hook still built with the old path makes its folder) leaves it, and is no failure: the data moved.
fn leave_link(old: &Path) -> std::io::Result<()> {
    match std::os::unix::fs::symlink(DATA_FOLDER, old) {
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod link_tests {
    #[test]
    fn a_path_taken_after_the_rename_is_no_failure_of_the_move() {
        let home = std::env::temp_dir().join(format!("plotkeeper-link-{}", std::process::id()));
        let old = home.join("old");
        std::fs::create_dir_all(&old).unwrap();
        let left = super::leave_link(&old);
        std::fs::remove_dir_all(&home).unwrap();
        assert!(left.is_ok(), "{left:?}");
    }
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

/// What a prune reads of a line: its session, by `session_id` (Claude Code's, Codex's and Qwen Code's), agy's
/// `conversationId`, Cursor's `conversation_id` or Copilot's camelCase `sessionId`, the agent its hook entry
/// named, and when it was received.
#[derive(Default, serde::Deserialize)]
struct Stamp {
    session_id: Option<String>,
    #[serde(rename = "conversationId")]
    conversation_id: Option<String>,
    /// Cursor's session (#362).
    #[serde(rename = "conversation_id")]
    cursor_conversation: Option<String>,
    /// Copilot's session, in its camelCase payloads (#361).
    #[serde(rename = "sessionId")]
    copilot_session: Option<String>,
    studio_agent: Option<String>,
    studio_received_ms: Option<u64>,
}

impl Stamp {
    fn of(line: &[u8]) -> Self {
        serde_json::from_slice(line).unwrap_or_default()
    }

    /// The session id the line's agent uses (#355); a line that names no agent, written before
    /// entries named one or by the server, goes by whichever id it has.
    fn session(&self) -> Option<&str> {
        let (claude, agy) = (self.session_id.as_deref(), self.conversation_id.as_deref());
        match self.studio_agent.as_deref() {
            Some("agy") => agy,
            Some("claude" | "codex" | "qwen") => claude,
            Some("cursor") => self.cursor_conversation.as_deref(),
            Some("copilot") => claude.or(self.copilot_session.as_deref()),
            _ => claude.or(agy),
        }
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
    /// Code's idle notice or the shell's `INTERRUPTED` line. Returns the worktrees whose end tree
    /// to record, those of the turn it ends, and whether it is in a turn whose first touches
    /// record a start tree.
    pub fn take(&mut self, name: &str, prompt_id: Option<&str>, idle: bool) -> (Vec<String>, bool) {
        // The shell's line ends the turn it names, never one that started after it.
        if name == INTERRUPTED && self.prompt_id.as_deref() != prompt_id {
            return (Vec::new(), false);
        }
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

/// Whether the trees this process records still count: the hook gives them up when they run past
/// its budget, and their line is logged without them (#426 review).
#[derive(Clone, Copy, PartialEq)]
enum Fate {
    Open,
    Kept,
    Abandoned,
}

static FATE: Mutex<Fate> = Mutex::new(Fate::Open);

fn fate() -> std::sync::MutexGuard<'static, Fate> {
    FATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Gives up the trees this process is still recording: its `git` calls are killed, and `trees`
/// keeps no record of them, so the turn's next touch of their worktrees records them again.
/// `false` when `trees` has already kept its record, which then lands in a moment.
pub fn abandon_trees() -> bool {
    let mut fate = fate();
    if *fate == Fate::Kept {
        return false;
    }
    *fate = Fate::Abandoned;
    true
}

/// Whether the trees were given up (`abandon_trees`).
fn abandoned() -> bool {
    *fate() == Fate::Abandoned
}

/// Whether `trees` may keep its record: the trees were not given up, and now never will be.
fn keep_trees() -> bool {
    let mut fate = fate();
    if *fate == Fate::Abandoned {
        return false;
    }
    *fate = Fate::Kept;
    true
}

/// The most untracked files, and bytes of them, that a tree is recorded with: past either, hashing
/// them costs more than a hook may (#141).
pub const MAX_UNTRACKED_FILES: usize = 10_000;
pub const MAX_UNTRACKED_BYTES: u64 = 256 << 20;

/// How long one of the hook's `git` calls may run before it is killed (#141).
const GIT_TIMEOUT: Duration = Duration::from_secs(20);

/// The root of the git worktree that holds `folder`, or its nearest existing parent, and the git
/// tree of its files as they are now, untracked ones included and ignored ones not (danielrispler/Falinks#486). `None`
/// outside a worktree or when `git` fails. It writes the tree through a throwaway index, so the
/// working tree, the real index and `git status` stay as they were.
pub fn record_tree(folder: &Path) -> Option<(String, String)> {
    record_tree_within(folder, MAX_UNTRACKED_FILES, MAX_UNTRACKED_BYTES)
}

/// `record_tree`, which gives up, and says so on stderr, when the worktree holds more than `files`
/// untracked files or `bytes` of them.
pub fn record_tree_within(folder: &Path, files: usize, bytes: u64) -> Option<(String, String)> {
    let root = worktree_root(folder)?;
    if let Err(why) = untracked_within(Path::new(&root), files, bytes) {
        eprintln!("plotkeeper-hook: no tree of {root}: {why}");
        return None;
    }
    let index = git(
        Path::new(&root),
        &["rev-parse", "--git-path", "index"],
        None,
    )?;
    // One per call: Studio records trees from several threads at once.
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let call = CALLS.fetch_add(1, Ordering::Relaxed);
    let scratch = env::temp_dir().join(format!("plotkeeper-hook-{}-{call}.index", process::id()));
    // A copy of the real index keeps git's stat cache, so only changed files are hashed again.
    let _ = fs::copy(Path::new(&root).join(index), &scratch);
    let added = git(Path::new(&root), &["add", "-A"], Some(&scratch));
    let tree = added.and_then(|_| git(Path::new(&root), &["write-tree"], Some(&scratch)));
    let _ = fs::remove_file(&scratch);
    // A killed `add` leaves its lock behind.
    let _ = fs::remove_file(scratch.with_extension("index.lock"));
    Some((root, tree?))
}

/// `Ok` when the untracked, unignored files under the worktree `root` are at most `files` files
/// and `bytes` bytes. It stops listing them at the first cap they pass, so a huge untracked
/// directory costs no more than the caps.
fn untracked_within(root: &Path, files: usize, bytes: u64) -> Result<(), String> {
    let args = ["ls-files", "-z", "--others", "--exclude-standard"];
    let mut child = command(root, &args, None)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("git ls-files: {e}"))?;
    let stdout = child.stdout.take().ok_or("git ls-files: no output")?;
    // Read on a thread, so the deadline holds even while git prints nothing.
    let (paths, listed) = mpsc::channel();
    thread::spawn(move || {
        for path in BufReader::new(stdout).split(b'\0').map_while(Result::ok) {
            if paths.send(path).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + GIT_TIMEOUT;
    let (mut count, mut size) = (0, 0);
    let passed = loop {
        let path = match listed.recv_timeout(Duration::from_millis(20)) {
            Ok(path) => path,
            Err(RecvTimeoutError::Disconnected) => break None,
            Err(RecvTimeoutError::Timeout) if abandoned() => break Some("given up".into()),
            Err(RecvTimeoutError::Timeout) if Instant::now() > deadline => {
                break Some(format!(
                    "listing its untracked files took over {GIT_TIMEOUT:?}"
                ));
            }
            Err(RecvTimeoutError::Timeout) => continue,
        };
        count += 1;
        size += fs::symlink_metadata(root.join(OsStr::from_bytes(&path))).map_or(0, |m| m.len());
        if count > files {
            break Some(format!("more than {files} untracked files"));
        }
        if size > bytes {
            break Some(format!("more than {bytes} bytes of untracked files"));
        }
    };
    if let Some(why) = passed {
        let _ = child.kill();
        let _ = child.wait();
        return Err(why);
    }
    let status = child.wait().map_err(|e| format!("git ls-files: {e}"))?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "git ls-files failed".into())
}

/// The hook's `git` in `folder`, with `index` as its index when given. It takes no optional locks,
/// so it never holds up the agent's own `git` commands, and never starts auto maintenance, whose
/// repacks stacked up by the dozen in #141.
fn command(folder: &Path, args: &[&str], index: Option<&Path>) -> Command {
    let mut command = studio_workbench::run::git(folder);
    command
        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
        .args(args);
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    command
}

/// `git`'s trimmed output in `folder`, with `index` as its index when given, or `None` when it
/// fails or runs past `GIT_TIMEOUT`, when it is killed.
fn git(folder: &Path, args: &[&str], index: Option<&Path>) -> Option<String> {
    if abandoned() {
        return None;
    }
    use studio_workbench::run::{RunError, run_until_unless};
    let ran = run_until_unless(&mut command(folder, args, index), GIT_TIMEOUT, &abandoned);
    if matches!(ran, Err(RunError::TimedOut(_))) && !abandoned() {
        let args = args.join(" ");
        eprintln!(
            "plotkeeper-hook: git {args} in {} ran over {GIT_TIMEOUT:?}",
            folder.display()
        );
    }
    let out = String::from_utf8(ran.ok()?.stdout).ok()?;
    Some(out.trim_end().to_owned())
}

/// The roots of the worktrees `git worktree list` gives for the repository that holds `folder`, as
/// it spells them; empty outside a repository.
fn listed_worktrees(folder: &Path) -> Vec<String> {
    let Some(existing) = folder.ancestors().find(|p| p.is_dir()) else {
        return Vec::new();
    };
    let list = git(existing, &["worktree", "list", "--porcelain"], None).unwrap_or_default();
    let roots = list
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "));
    roots.map(Into::into).collect()
}

/// Whether `root` holds `path`: the path is the root or lies below it, on a path boundary.
fn holds(root: &str, path: &str) -> bool {
    path.strip_prefix(root)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/') || root.ends_with('/'))
}

/// The absolute paths a Bash `command` names, as the text spells them.
fn named_paths(command: &str) -> Vec<&str> {
    let separators = |c: char| c.is_whitespace() || "\"'`;&|<>()=,:{}$*?[]".contains(c);
    let tokens = command.split(separators);
    let paths = tokens.filter(|t| t.starts_with('/') && t.len() > 1);
    paths.collect()
}

/// `path` with its nearest existing folder spelled as `git` spells it, and that folder.
fn spelled(path: &Path) -> Option<(PathBuf, PathBuf)> {
    let existing = path.ancestors().find(|p| p.is_dir())?;
    let real = existing.canonicalize().ok()?;
    let rest = path.strip_prefix(existing).ok()?;
    Some((real.join(rest), real))
}

/// The worktree roots the folders and paths a call names touch (danielrispler/plotkeeper#202). A
/// path counts for the deepest listed root that holds it. The lists come from `git worktree list`
/// at the first path of a repository the call names, so a worktree an earlier command made is
/// found. A path in the folder of a worktree that is no longer listed, which `known` remembers
/// from earlier lists or which sits in a `worktrees` folder, gives no root.
struct Named<'a> {
    lists: Vec<Vec<String>>,
    known: &'a mut Vec<String>,
}

impl Named<'_> {
    fn root(&mut self, path: &Path) -> Option<String> {
        let (spelled, existing) = spelled(path)?;
        let at = spelled.to_str()?;
        // A path whose nearest folder is a `worktrees` folder is a worktree to be made or one removed.
        let below = existing.file_name().is_some_and(|n| n == "worktrees") && existing != spelled;
        // A root the hook already knows holds the path, with no worktree made between: no listing.
        let known = self.known.iter().filter(|r| holds(r, at));
        if let Some(deepest) = known.max_by_key(|r| r.len()).filter(|_| !below) {
            let alive = Path::new(deepest).join(".git").exists();
            let between = |p: &&Path| {
                let at_root = p.to_str().is_some_and(|p| p == deepest);
                let unknown = |p: &Path| {
                    p.to_str()
                        .is_some_and(|p| !self.known.iter().any(|r| r == p))
                };
                !at_root && p.join(".git").exists() && unknown(p)
            };
            let made = existing.ancestors().take_while(|p| p.starts_with(deepest));
            if !alive {
                return None;
            }
            if made.clone().all(|p| !between(&p)) {
                return Some(deepest.clone());
            }
        }
        let held = |list: &Vec<String>| list.iter().any(|r| holds(r, at));
        let index = match self.lists.iter().position(held) {
            Some(index) => index,
            None => {
                let list = listed_worktrees(&existing);
                for root in &list {
                    if !self.known.contains(root) {
                        self.known.push(root.clone());
                    }
                }
                self.lists.push(list);
                self.lists.len() - 1
            }
        };
        let list = &self.lists[index];
        let root = list
            .iter()
            .filter(|r| holds(r, at))
            .max_by_key(|r| r.len())?;
        let gone = |r: &&String| !list.contains(r) && holds(r, at) && r.len() > root.len();
        let removed = self.known.iter().any(|r| gone(&r));
        (!removed && !below).then(|| root.clone())
    }
}

/// The line the studio's shell appends for a turn Daniel interrupted, which fires no `Stop`: as
/// `studio_core::INTERRUPTED`. The shell runs `trees` on it, so it carries that turn's end trees.
pub const INTERRUPTED: &str = "StudioInterrupted";

/// Adds to `event` the trees of the turn it ends, `studio_turn_end`, and of the worktrees its turn
/// touches first with it, `studio_turn_start`, each by worktree root. Like the core, it takes an
/// event's folder and an `Edit` or `Write` path as what it touches (danielrispler/Falinks#434,
/// decision 9). A worker's event takes trees for the turn that spawned it, the turn the hook is in at
/// its first event (danielrispler/plotkeeper#142): a start at its first touch of a worktree that turn
/// has none of, and an end of each worktree it touched at the later of that turn's end and its own
/// stop. What it knows of the turn and its workers is kept in `turns/` in the data directory `data`,
/// until the session ends or, if its end never comes, for 7 days.
/// Codex's and Cursor's turns are mapped by their lines in Claude Code's names (#426); another
/// agent's line records none, as its capabilities' `turn_trees: false` says, though its payload may
/// be Claude-shaped (#359).
pub fn trees(
    event: &mut serde_json::Map<String, serde_json::Value>,
    data: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let agent = event.get("studio_agent").and_then(|a| a.as_str());
    let mut shaped = match agent {
        None | Some("claude") => return claude_trees(event, data),
        Some("codex") => codex_shaped(event),
        Some("cursor") => cursor_shaped(event),
        Some(_) => return Ok(()),
    };
    claude_trees(&mut shaped, data)?;
    for key in [
        "studio_touched_roots",
        "studio_turn_end",
        "studio_turn_start",
    ] {
        if let Some(value) = shaped.remove(key) {
            event.insert(key.into(), value);
        }
    }
    Ok(())
}

type Line = serde_json::Map<String, serde_json::Value>;

/// Codex line `event` in the names `claude_trees` reads: its turn's `turn_id` is the prompt id, an
/// `Interrupt` ends its turn as the studio's `INTERRUPTED` line does, and an `apply_patch` edits the
/// first file its patch names (as `studio_core`'s Codex adapter reads it).
// The patch reading copies `patched_file` in core/src/codex.rs, as the hook does not depend on core.
// shortcut: a patch that changes several files is taken as touching its first.
// shortcut: Codex runs its hooks with `"async": true` (plugin-codex/hooks.json), so the `PreToolUse`
// of a call that first touches a worktree other than the session's folder may record its start tree
// after the call changed it, and the turn loses that change.
fn codex_shaped(event: &Line) -> Line {
    use serde_json::{Value, json};
    let mut shaped = event.clone();
    shaped.remove("studio_agent");
    if let Some(turn) = event.get("turn_id") {
        shaped.insert("prompt_id".into(), turn.clone());
    }
    if event.get("hook_event_name").and_then(Value::as_str) == Some("Interrupt") {
        shaped.insert("hook_event_name".into(), INTERRUPTED.into());
    }
    if event.get("tool_name").and_then(Value::as_str) == Some("apply_patch") {
        let patch = event.get("tool_input").and_then(|i| i["command"].as_str());
        let file = patch.and_then(|patch| {
            patch.lines().find_map(|line| {
                let line = line.trim_start().strip_prefix("*** ")?;
                Some(line.split_once(" File: ")?.1.trim())
            })
        });
        // A path from the folder the call ran in.
        let cwd = event.get("cwd").and_then(Value::as_str).unwrap_or_default();
        let file = file.map(|f| Path::new(cwd).join(f).to_string_lossy().into_owned());
        shaped.insert("tool_name".into(), "Edit".into());
        shaped.insert("tool_input".into(), json!({ "file_path": file }));
    }
    shaped
}

/// Cursor line `event` in the names `claude_trees` reads: its `conversation_id` is the session, its
/// `generation_id` the prompt id, an event with no folder works in its first workspace root, and a
/// `Write` or `Delete` edits its file and a `Shell` call runs its command, in the folder it names.
// The tool names copy `tool` in core/src/cursor.rs, as the hook does not depend on core.
fn cursor_shaped(event: &Line) -> Line {
    use serde_json::Value;
    let text = |key: &str| event.get(key).and_then(Value::as_str);
    let mut shaped = event.clone();
    shaped.remove("studio_agent");
    let name = match text("hook_event_name").unwrap_or_default() {
        "sessionStart" => "SessionStart",
        "sessionEnd" => "SessionEnd",
        "beforeSubmitPrompt" => "UserPromptSubmit",
        "stop" => "Stop",
        "postToolUse" | "postToolUseFailure" => "PostToolUse",
        other => other,
    };
    shaped.insert("hook_event_name".into(), name.into());
    if let Some(session) = event.get("conversation_id") {
        shaped.insert("session_id".into(), session.clone());
    }
    if let Some(generation) = text("generation_id").filter(|g| !g.is_empty()) {
        shaped.insert("prompt_id".into(), generation.into());
    }
    let roots = event.get("workspace_roots").and_then(Value::as_array);
    let root = roots.and_then(|r| r.first()?.as_str());
    // A `Shell` call names the folder it ran in, which may lie outside every root.
    let ran_in = (text("tool_name") == Some("Shell"))
        .then(|| event.get("tool_input")?.get("cwd")?.as_str())
        .flatten();
    let cwd = ran_in.filter(|c| !c.is_empty());
    if let Some(cwd) = cwd.or(text("cwd").filter(|c| !c.is_empty())).or(root) {
        shaped.insert("cwd".into(), cwd.into());
    }
    // Cursor sends no event before a call, only `postToolUse` after it, so a worktree's first touch
    // would record its start tree after the call changed it: the turn's start takes the trees of
    // every workspace root.
    // shortcut: a worktree outside every root still records its start after its first call, so a file
    // its first call changes is lost. Cursor's events before a call (`preToolUse`,
    // `beforeShellExecution`) are permission hooks, not wired: Cursor blocks the call when one replies
    // with nothing (docs/studio/plugin.md).
    if name == "UserPromptSubmit"
        && let Some(roots) = roots
    {
        shaped.insert("studio_turn_roots".into(), roots.clone().into());
    }
    let tool = match text("tool_name") {
        Some("Write" | "Delete") => Some("Edit"),
        Some("Shell") => Some("Bash"),
        _ => None,
    };
    if let Some(tool) = tool {
        shaped.insert("tool_name".into(), tool.into());
    }
    // A `Delete` may name its file by `path`.
    if tool == Some("Edit")
        && let Some(input) = shaped.get_mut("tool_input").and_then(Value::as_object_mut)
        && !input.contains_key("file_path")
        && let Some(path) = input.get("path").cloned()
    {
        input.insert("file_path".into(), path);
    }
    shaped
}

fn claude_trees(event: &mut Line, data: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use serde_json::{Map, Value, json};
    use std::io::{Seek, Write};
    let text = |key: &str| event.get(key).and_then(Value::as_str);
    let name = text("hook_event_name").unwrap_or_default();
    let idle = name == "Notification" && text("notification_type") == Some("idle_prompt")
        || name == INTERRUPTED;
    let outside = matches!(name, "SessionStart" | "SessionEnd" | "Notification");
    let session = text("session_id").unwrap_or_default();
    // The id names a file, so it must be a plain one.
    let plain = answer::is_file_safe(session);
    let turns = data.join("turns");
    let state = turns.join(format!("{session}.json"));
    // A session's record goes with it; a start, which is rare, prunes those whose end never came.
    if name == "SessionEnd" && plain {
        let _ = fs::remove_file(&state);
    }
    if name == "SessionStart" {
        prune_turns(&turns);
    }
    if outside && !idle || !plain {
        return Ok(());
    }
    fs::create_dir_all(&turns)?;
    let mut file = fs::OpenOptions::new()
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
    let strings = |value: &Value| -> Vec<String> {
        let values = value.as_array().into_iter().flatten();
        values
            .filter_map(|v| Some(v.as_str()?.to_owned()))
            .collect()
    };
    let mut turn = Turn {
        prompt_id: kept["prompt_id"].as_str().map(Into::into),
        roots: strings(&kept["roots"]),
        ended: kept["ended"].as_bool().unwrap_or(true),
    };
    // Each worker's turn, by its prompt id, and the worktrees it touched.
    let mut workers = kept["workers"].as_object().cloned().unwrap_or_default();

    let tool = text("tool_name");
    let path = event
        .get("tool_input")
        .and_then(|input| input.get("file_path")?.as_str());
    let path = path.filter(|p| p.starts_with('/') && matches!(tool, Some("Edit" | "Write")));
    let folder = path.and_then(|p| Path::new(p).parent()?.to_str());
    let cwd = text("cwd");
    let command = text("tool_name")
        .filter(|t| *t == "Bash")
        .and_then(|_| event.get("tool_input")?.get("command")?.as_str());
    let named = command.map(named_paths).unwrap_or_default();
    // The folders a turn starts in, which Cursor's prompt names (`cursor_shaped`): each gets its
    // start tree, but is no touch of the call.
    let starts = event.get("studio_turn_roots").and_then(Value::as_array);
    let starts: Vec<&str> = starts
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let mut known = strings(&kept["worktrees"]);
    // The worktrees the call touches, listed once: the folder's, an `Edit` or `Write` path's, and
    // those a Bash command names.
    let touched = |known: &mut Vec<String>| {
        let mut found: Vec<String> = Vec::new();
        let mut names = Named {
            lists: Vec::new(),
            known,
        };
        let roots = cwd.and_then(|f| names.root(Path::new(f))).into_iter();
        let roots = roots.chain(folder.and_then(|f| names.root(Path::new(f))));
        let mut roots: Vec<String> = roots.collect();
        roots.extend(named.iter().filter_map(|p| names.root(Path::new(p))));
        for root in roots {
            if !found.contains(&root) {
                found.push(root);
            }
        }
        found
    };
    let started = |known: &mut Vec<String>| {
        let mut names = Named {
            lists: Vec::new(),
            known,
        };
        let roots = starts.iter().filter_map(|p| names.root(Path::new(p)));
        roots.collect::<Vec<String>>()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    // When the turn began, in Unix milliseconds; a record from before it is kept does not say.
    let mut began = kept["began"].as_u64();
    let mut touches = Vec::new();
    let mut end = Map::new();
    let mut start = Map::new();
    match text("agent_id") {
        None => {
            let (before, was_ended) = (turn.prompt_id.clone(), turn.ended);
            let (ended, in_turn) = turn.take(name, text("prompt_id"), idle);
            if turn.prompt_id != before || was_ended && !turn.ended {
                began = Some(now);
            }
            end = ended.iter().filter_map(|root| record(root)).collect();
            if in_turn {
                touches = touched(&mut known);
                let mut firsts = touches.clone();
                for root in started(&mut known) {
                    if !firsts.contains(&root) {
                        firsts.push(root);
                    }
                }
                for root in firsts {
                    if !turn.roots.contains(&root)
                        && let Some((root, tree)) = first_tree(&root, began)
                    {
                        turn.roots.push(root.clone());
                        start.insert(root, tree);
                    }
                }
            }
        }
        Some(agent) => {
            let worker = workers.entry(agent).or_insert_with(
                || json!({ "prompt_id": turn.prompt_id, "roots": [], "began": now }),
            );
            let worker_began = worker["began"].as_u64();
            let mut roots = strings(&worker["roots"]);
            // While its turn runs, that turn's end takes the trees of the worktrees it touched.
            let live = !turn.ended && worker["prompt_id"].as_str() == turn.prompt_id.as_deref();
            if name == "SubagentStop" {
                if !live {
                    end = roots.iter().filter_map(|root| record(root)).collect();
                }
            } else {
                touches = touched(&mut known);
                for root in touches.clone() {
                    if roots.contains(&root) {
                        continue;
                    }
                    if live && turn.roots.contains(&root) {
                        roots.push(root);
                    } else if let Some((root, tree)) = first_tree(&root, worker_began) {
                        roots.push(root.clone());
                        if live {
                            turn.roots.push(root.clone());
                        }
                        start.insert(root, tree);
                    }
                }
            }
            worker["roots"] = roots.into();
        }
    }
    // Trees given up are logged with none, so the record must not say they were taken.
    if !keep_trees() {
        return Ok(());
    }
    let kept = json!({
        "prompt_id": turn.prompt_id, "roots": turn.roots, "ended": turn.ended, "began": began,
        "workers": workers,
        "worktrees": known,
    });
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(kept.to_string().as_bytes())?;
    // The roots the call touched, which the core reads for the session's own touches.
    if !touches.is_empty() {
        event.insert("studio_touched_roots".into(), touches.into());
    }
    for (key, trees) in [("studio_turn_end", end), ("studio_turn_start", start)] {
        if !trees.is_empty() {
            event.insert(key.into(), trees.into());
        }
    }
    Ok(())
}

/// Deletes the records in `turns` unchanged for 7 days.
fn prune_turns(turns: &Path) {
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
fn record(root: &str) -> Option<(String, serde_json::Value)> {
    record_tree(Path::new(root)).map(|(root, tree)| (root, tree.into()))
}

/// The commit worktree `root` was created at and the Unix second it was created, from the first
/// line of its `HEAD` reflog (danielrispler/plotkeeper#204); `None` when it has no such line.
pub fn creation(root: &Path) -> Option<(String, u64)> {
    let log = git(root, &["rev-parse", "--git-path", "logs/HEAD"], None)?;
    let log = fs::read_to_string(root.join(log)).ok()?;
    let who = log.lines().next()?.split('\t').next()?;
    let mut shas = who.split(' ');
    let commit = shas.nth(1)?.to_owned();
    // `<sha> <sha> <name> <<email>> <seconds> <zone>`
    let mut tail = who.rsplit(' ');
    let at = tail.nth(1)?.parse().ok()?;
    Some((commit, at))
}

/// Worktree `root`'s start tree for a turn or worker that began at `began` (Unix milliseconds):
/// the tree of the commit the worktree was created at when that was in a later second, otherwise
/// its tree now.
fn first_tree(root: &str, began: Option<u64>) -> Option<(String, serde_json::Value)> {
    let made = creation(Path::new(root)).filter(|(_, at)| began.is_some_and(|b| *at > b / 1000));
    let tree = made.and_then(|(commit, _)| {
        git(
            Path::new(root),
            &["rev-parse", &format!("{commit}^{{tree}}")],
            None,
        )
    });
    match tree {
        Some(tree) => Some((root.to_owned(), tree.into())),
        None => record(root),
    }
}

/// The agent's process for the hook that runs now (#330): `wrapper`, the pid a wrapper script passed in
/// `PLOTKEEPER_AGENT_PID` (agy's, which runs the hook from a backgrounded subshell), else the hook's
/// `parent`, or its parent's parent when that is a shell the agent started the hook through.
/// `inspect` gives a process's command and parent pid. `None` when no agent can be told, as for a hook
/// reparented to init, or for a process that outlives its sessions: one inside an IDE app's bundle
/// (spec #320, "IDE-hosted pids") or Codex's shared daemon (#369); the session then takes the idle
/// path. Cursor's per-window extension host, which renames itself so that no bundle path shows, is
/// recorded: its session ends when its window closes (#369).
pub fn agent_pid(
    wrapper: Option<&str>,
    parent: u32,
    inspect: impl Fn(u32) -> Option<(String, u32)>,
) -> Option<u32> {
    let real = |pid: u32| (pid > 1).then_some(pid);
    if let Some(pid) = wrapper.and_then(|w| w.trim().parse().ok()).and_then(real) {
        return Some(pid);
    }
    let parent = real(parent)?;
    let (agent, command) = match inspect(parent) {
        Some((command, up)) if is_shell(&command) => (real(up)?, inspect(up).map(|(c, _)| c)),
        found => (parent, found.map(|(c, _)| c)),
    };
    match command {
        Some(command) if in_app(&command) || codex_daemon(&command) => None,
        _ => Some(agent),
    }
}

/// Whether `command` is Codex's `app-server` daemon, which runs the hooks of every Codex TUI session
/// and outlives them (#369, Codex 0.172.0), under `$CODEX_HOME/packages/` (`~/.codex` by default);
/// `codex exec` runs its hooks itself.
fn codex_daemon(command: &str) -> bool {
    command.contains("/packages/app-server-daemon/") && command.ends_with("/codex")
}

/// Whether `command`, a process's executable path, lies inside a macOS app bundle, as an IDE's
/// processes do; an agent's CLI is a program of its own.
fn in_app(command: &str) -> bool {
    command.contains(".app/Contents/")
}

fn is_shell(command: &str) -> bool {
    let name = command.rsplit('/').next().unwrap_or(command);
    let name = name.trim_start_matches('-');
    matches!(name, "sh" | "bash" | "zsh" | "dash" | "fish")
}

/// The harness whose own payload `event` is, by a marker field only its payloads carry, when that is
/// another than `agent`, the agent its hook entry names (spec #320, "One event, one record, the right
/// agent"): Cursor's `cursor_version`, as Cursor runs Claude Code's hooks too (#362). Such an event is
/// dropped, as that harness's own entry records it.
/// Returns that harness's display name.
pub fn foreign(
    agent: Option<&str>,
    event: &serde_json::Map<String, serde_json::Value>,
) -> Option<&'static str> {
    let (marked, name) = if event.contains_key("cursor_version") {
        ("cursor", "Cursor")
    } else if copilot_payload(event) {
        ("copilot", "Copilot")
    } else {
        return None;
    };
    agent.is_some_and(|a| a != marked).then_some(name)
}

/// Whether `payload` carries Copilot's marker, for the agent guard above: Copilot reads
/// `.claude/settings.json`, so a payload of Copilot's could reach Claude Code's entry. The marker is
/// `sessionId`, camelCase, on every payload Copilot CLI sent its own plugin in wave 1's trial (#369);
/// no other agent's payload has it. The snake_case payloads its docs give VS Code's harness carry no
/// marker, and that harness is not yet trialled.
fn copilot_payload(payload: &serde_json::Map<String, serde_json::Value>) -> bool {
    payload.contains_key("sessionId")
}

/// The command and parent pid of process `pid`, from `ps`; `None` when it has gone or `ps` fails.
pub fn inspect(pid: u32) -> Option<(String, u32)> {
    let out = Command::new("ps")
        .args(["-o", "ppid=", "-o", "comm=", "-p", &pid.to_string()])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let (parent, command) = text.trim().split_once(char::is_whitespace)?;
    Some((command.trim().to_string(), parent.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{INTERRUPTED, Turn, command};

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

    /// The shell's line for an interrupted turn ends that turn only, never a later one.
    #[test]
    fn the_shells_interrupted_line_ends_its_own_turn_only() {
        let mut turn = Turn::default();
        after(&mut turn, "UserPromptSubmit", "p1");
        turn.roots.push("/w/a".into());
        assert_eq!(
            turn.take(INTERRUPTED, Some("p1"), true),
            (vec!["/w/a".into()], false)
        );

        after(&mut turn, "UserPromptSubmit", "p2");
        turn.roots.push("/w/a".into());
        assert_eq!(turn.take(INTERRUPTED, Some("p1"), true), (vec![], false));
        assert!(!turn.ended);
        assert_eq!(turn.roots, ["/w/a"]);
    }

    #[test]
    fn the_hooks_git_calls_never_start_auto_maintenance() {
        let command = command(Path::new("/"), &["add", "-A"], None);
        let args: Vec<_> = command.get_args().filter_map(|a| a.to_str()).collect();
        let configs = ["-c", "maintenance.auto=false", "-c", "gc.auto=0"];
        assert!(args.windows(4).any(|w| w == configs), "{args:?}");
        assert_eq!(&args[args.len() - 2..], ["add", "-A"]);
    }
}
