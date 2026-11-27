//! The studio: every session, and what Daniel selected, fed one hook event at a time.

mod take;

use crate::{
    AGY_CAPABILITIES, Agent, AgyDrift, AgyDriftKind, Bytes, CardRequest, Change, CheckLists,
    FactFile, Facts, Fingerprint, Kind, LiveFacts, MIN_HISTORY, Outcome, Output, Purpose, Revive,
    RootDiff, Row, Session, State, Status, Subagent, SubagentLog, SubagentLogFile, Terminal,
    TranscriptFormat, TurnDiff, TurnFiles, TurnFilesSnapshot, TurnTrees, Work, agy, checks, claude,
    codex, compute_turn_files, copilot, cursor,
    event::{Event, Happened, Start, ToolKind},
    opencode, qwen, range,
    session::{
        FALLBACK_MS, IDLE_AFTER_MS, MISMATCH_AFTER_MS, Since, card_time, folder_name, short,
        shown_state, task_path,
    },
    turn_files,
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
};

/// Who reads a line that is not Claude's: agy, when its hook entry named agy or named no agent (a
/// line written before entries named one), Codex (#359), Qwen Code (#360), or nobody, for another agent (#355). A new
/// harness is one arm here, beside its `Capabilities` constant and its parser.
enum Named {
    Agy,
    /// An agent read by a parser of its own, Codex's, Cursor's (#362), Qwen Code's, Copilot's (#361) or OpenCode's (#370), and the line's
    /// time, for its drift when that parser cannot read it.
    Parsed(Parse, u64),
    /// The line's time, for its drift.
    Other(u64),
}

/// An adapter's reading of one log line into its event.
type Parse = fn(&str) -> Result<Event, serde_json::Error>;

impl Named {
    fn of(line: &str) -> Named {
        #[derive(Default, Deserialize)]
        struct Stamp {
            studio_agent: Option<String>,
            #[serde(default)]
            studio_received_ms: u64,
        }
        let stamp: Stamp = serde_json::from_str(line).unwrap_or_default();
        match stamp.studio_agent.as_deref() {
            None => Named::Agy,
            Some(agent) if agent == AGY_CAPABILITIES.agent => Named::Agy,
            Some(codex::CODEX) => Named::Parsed(codex::parse, stamp.studio_received_ms),
            Some(cursor::CURSOR) => Named::Parsed(cursor::parse, stamp.studio_received_ms),
            Some(qwen::QWEN) => Named::Parsed(qwen::parse, stamp.studio_received_ms),
            Some(copilot::COPILOT) => Named::Parsed(copilot::parse, stamp.studio_received_ms),
            Some(opencode::OPENCODE) => Named::Parsed(opencode::parse, stamp.studio_received_ms),
            Some(_) => Named::Other(stamp.studio_received_ms),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Studio {
    /// The agy adapter, which keeps its conversations (`agy.rs`).
    pub(crate) agy: agy::Adapter,
    pub(crate) sessions: HashMap<String, Session>,
    /// The session Daniel clicked last, which the session view shows.
    pub(crate) selected: Option<String>,
    /// The subagent of it whose line he opened since, whose own view shows instead.
    pub(crate) viewed: Option<String>,
    /// The studio terminal of a session Daniel started, which is selected when its first event
    /// names that terminal (danielrispler/Falinks#567).
    pub(crate) awaiting: Option<String>,
    /// The session Daniel marked unseen, which looking at it does not see until he selects it
    /// again (danielrispler/Falinks#556).
    pub(crate) held: Option<String>,
    /// The shell's latest tick, in Unix milliseconds.
    #[serde(skip)]
    pub(crate) now: u64,
    /// The shell's lookup and what it found, which `Studio::resume` keeps (`saved.rs`).
    #[serde(skip)]
    pub(crate) roots: Roots,
    /// The commands that are checks, per project (danielrispler/plotkeeper#112), from the settings.
    #[serde(skip)]
    pub(crate) check_lists: CheckLists,
    /// Where the next log line starts in the event log, in bytes: every line given to `replay`,
    /// `apply` or `apply_with_changes` moves it, as the shell reads the log from its start.
    pub(crate) log_end: u64,
    /// How long each command of checks ran when it did not fail (danielrispler/Falinks#461, decision 13).
    pub(crate) durations: checks::Durations,
    /// The row of each session that continues another after a `/clear`, by its own id (danielrispler/Falinks#461,
    /// decision 18). A session not here is its own row.
    pub(crate) rows: HashMap<String, String>,
    /// Summaries are written when Daniel opens a turn, not at its end, so they move no intention
    /// (danielrispler/Falinks#517, decisions 13 and 16), from the settings.
    #[serde(skip)]
    pub(crate) summaries_on_open: bool,
    /// Since when each Claude session's hook state and its terminal's OSC 7501 state disagree, the
    /// 7501 state it disagrees with, and whether that disagreement was logged (danielrispler/Falinks#560).
    /// A new 7501 state starts a new disagreement.
    pub(crate) mismatched: HashMap<String, (u64, String, bool)>,
    /// Whether `terminals` has had its first list: the rows that list finds tell nothing, as Claude
    /// sessions replayed at start tell nothing (danielrispler/Falinks#522).
    pub(crate) listed: bool,
    /// The terminals of the daemon's last list that have not exited (#101): a session keeps the
    /// terminal it has while that one is here.
    pub(crate) live_terminals: HashSet<String>,
    /// What the shell last found about each task's work, by the task's worktree folder: its files,
    /// its PR and its issue, asked at a turn's end, at the window's focus and on a refresh. The main
    /// checkout is never here, so it never lands.
    pub(crate) tasks: HashMap<String, TaskFacts>,
}

/// What the shell found about a task, for **Landed**. A lookup that failed is `false`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskFacts {
    /// Nothing uncommitted and nothing unpushed.
    pub safe: bool,
    /// Its PR is merged.
    pub merged: bool,
    /// Its issue is closed; only read when a session of the task has an issue.
    pub issue_closed: bool,
}

/// What the shell asks `gh pr view` about: a branch, the folder of a session on it, and that session's project, whose PRs
/// the answer joins and in whose folder `gh` runs (spec #659).
#[derive(Clone, Debug, PartialEq)]
pub struct BranchQuery {
    pub project: String,
    pub cwd: String,
    pub branch: String,
}

/// What the shell looks up about a task: its worktree folder, its branch, its issue.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskQuery {
    pub path: String,
    pub branch: Option<String>,
    pub issue: Option<u64>,
}

type FindRoot = dyn Fn(&str) -> Option<String> + Send;

/// Each folder's worktree root (danielrispler/Falinks#434, decision 11), found once per folder by the shell's lookup. With
/// none found, a folder is its own root.
#[derive(Default)]
pub(crate) struct Roots {
    find: Option<Box<FindRoot>>,
    known: std::cell::RefCell<HashMap<String, String>>,
}

impl Roots {
    pub(crate) fn of(&self, folder: &str) -> String {
        if folder.is_empty() {
            return String::new();
        }
        let mut known = self.known.borrow_mut();
        if let Some(root) = known.get(folder) {
            return root.clone();
        }
        let found = self.find.as_ref().and_then(|find| find(folder));
        let root = found.unwrap_or_else(|| folder.into());
        known.insert(folder.into(), root.clone());
        root
    }

    /// `path` relative to worktree `root`, when it lies inside it. A root is spelled as git spells
    /// it, a path as its tool does: one through a symlink (`/tmp` for `/private/tmp`) is inside the
    /// root whose top folder holds it, the shortest of its folders that has `root` as its root.
    pub(crate) fn inside<'a>(&self, root: &str, path: &'a str) -> Option<&'a str> {
        let direct = path.strip_prefix(root).and_then(|p| p.strip_prefix('/'));
        if direct.is_some() || root.is_empty() {
            return direct;
        }
        let mut folders: Vec<&std::path::Path> = std::path::Path::new(path).ancestors().collect();
        folders.reverse();
        let top = folders.into_iter().skip(1).find_map(|f| {
            let f = f.to_str()?;
            (self.of(f) == root).then_some(f)
        })?;
        path.strip_prefix(top)?.strip_prefix('/')
    }
}

/// What the session view shows: the session Daniel selected, and the subagent of it he opened since,
/// if any. The pane draws the pressed row and the view from it, and keeps no selection of its own.
#[derive(Debug, PartialEq, Serialize)]
pub struct Selection<'a> {
    pub session: &'a Session,
    pub agent: Option<&'a str>,
}

impl Studio {
    /// A studio that finds a folder's worktree root with `find` (danielrispler/Falinks#434, decision 11), such as the
    /// shell's `git` lookup. `Studio::default` takes each folder as its own root.
    pub fn with_roots(find: impl Fn(&str) -> Option<String> + Send + 'static) -> Self {
        let roots = Roots {
            find: Some(Box::new(find)),
            ..Roots::default()
        };
        Studio {
            roots,
            ..Studio::default()
        }
    }

    /// Each folder's worktree root found so far, which the shell keeps across launches
    /// (danielrispler/plotkeeper#132).
    pub fn known_roots(&self) -> HashMap<String, String> {
        self.roots.known.borrow().clone()
    }

    /// Takes the roots a launch before found, so their folders need no lookup: on the real log the
    /// lookups were most of the replay's time (danielrispler/plotkeeper#132).
    pub fn know_roots(&mut self, roots: HashMap<String, String>) {
        self.roots.known.borrow_mut().extend(roots);
    }

    /// The worktree roots that `event` of a session's own agent touches (danielrispler/Falinks#434, decision 9): its
    /// folder's, and an `Edit` or `Write` path's, but those of its running workers with cards. A
    /// start, an end or a notice touches none.
    // shortcut: a Bash write through a path, without a `cd`, is not seen; parse commands for paths
    // if a live run shows cards missing such writes.
    fn touches(&self, event: &Event) -> Vec<String> {
        if event.agent_id.is_some() || event.outside_turns() {
            return Vec::new();
        }
        let session = self.sessions.get(&event.session_id);
        let mut roots = self.named_roots(event);
        roots.retain(|root| !session.is_some_and(|s| s.workers_own(root)));
        roots
    }

    /// The worktree roots that the call of `event` names: its folder's, an `Edit` or `Write` path's,
    /// and those the hook found a command touched (danielrispler/plotkeeper#203).
    fn named_roots(&self, event: &Event) -> Vec<String> {
        let path = event.tool().and_then(|t| t.path.as_deref());
        let path = path.filter(|p| p.starts_with('/'));
        let folder = path.and_then(|p| std::path::Path::new(p).parent()?.to_str());
        // With no lookup, a path under the event's folder is in its worktree.
        let below = |f: &&str| f.starts_with(&format!("{}/", event.cwd));
        let folder = folder.filter(|f| self.roots.find.is_some() || !below(f));
        let mut roots: Vec<String> = Vec::new();
        let listed = event.studio_touched_roots.iter().flatten();
        let folders = [Some(event.cwd.as_str()), folder].into_iter().flatten();
        // The hook's list holds roots already, so they need no lookup.
        let found = folders.map(|f| self.roots.of(f));
        for root in found.chain(listed.cloned()) {
            if !root.is_empty() && !roots.contains(&root) {
                roots.push(root);
            }
        }
        roots
    }

    /// The worktrees the shell fingerprints before it applies this live line (danielrispler/Falinks#328, module 5), each
    /// with its own key: none, one, or at a wake-up also its workers' ends (danielrispler/Falinks#369, decisions 6, 8, 10
    /// and 11).
    pub fn fingerprint_folders(&self, line: &str) -> Vec<Fingerprint> {
        let event = self.peek(line);
        let event = event.filter(|e| {
            !e.has(|h| {
                matches!(
                    h,
                    Happened::Renamed(_) | Happened::PromptNamed(_) | Happened::Deleted
                )
            })
        });
        event.map_or_else(Vec::new, |mut event| {
            event.session_id = self.row(&event);
            self.looks(&event)
        })
    }

    /// The row `event` belongs to: its session's, or for a start from `/clear`, the cleared session's
    /// on the same terminal, or in the same folder outside studio terminals (danielrispler/Falinks#461, decision 18); for a
    /// start from a fork, the live session with a prompt on the same terminal (danielrispler/Falinks#562).
    // shortcut: a start that lands before its clear's `SessionEnd` makes a new row; match both ways
    // if the async hooks show it.
    fn row(&self, event: &Event) -> String {
        if let Some(row) = self.rows.get(&event.session_id) {
            return row.clone();
        }
        let fresh = !self.sessions.contains_key(&event.session_id);
        if fresh && event.is_start_from(Start::Clear) {
            let same = |s: &&Session| match &event.studio_terminal_id {
                Some(terminal) => s.terminal.as_ref() == Some(terminal),
                None => s.terminal.is_none() && s.cwd == event.cwd,
            };
            let cleared = self.sessions.values().filter(|s| s.clearing).filter(same);
            if let Some(s) = cleared.max_by_key(|s| s.agent.last_event_at) {
                return s.agent.id.clone();
            }
        }
        // A fork mid-conversation goes on in the same `claude`, so on the same terminal (danielrispler/Falinks#562).
        if fresh && event.is_start_from(Start::Fork) {
            let forked = self.sessions.values().filter(|s| {
                s.terminal.is_some()
                    && s.terminal == event.studio_terminal_id
                    && !s.ended
                    && !s.prompts.is_empty()
            });
            if let Some(s) = forked.max_by_key(|s| s.agent.last_event_at) {
                return s.agent.id.clone();
            }
        }
        event.session_id.clone()
    }

    /// The fingerprints before `event`: its own (`look`), and the end of each worker with cards that
    /// it ends, by any signal (danielrispler/Falinks#434, decision 19).
    fn looks(&self, event: &Event) -> Vec<Fingerprint> {
        let mut looks = self.look(event);
        let Some(session) = self.sessions.get(&event.session_id) else {
            return looks;
        };
        for (id, ..) in session.ends(event) {
            let Some(agent) = session.subagent(&id) else {
                continue;
            };
            // One in its session's worktree, outside a turn, ends with the session's card (danielrispler/Falinks#434,
            // decision 16).
            let key = &session.agent.id;
            if agent.owner == *key && !session.in_turn() {
                let folder = agent.root.clone();
                let seen = |l: &Fingerprint| l.key == *key && l.folder == folder;
                if !looks.iter().any(seen) {
                    let purpose = Purpose::Card;
                    looks.push(Fingerprint {
                        key: key.clone(),
                        folder,
                        purpose,
                        began: None,
                    });
                }
            }
            let folder = agent.folder.clone();
            let Some(folder) = folder.filter(|_| session.has_cards(&id)) else {
                continue;
            };
            match looks.iter_mut().find(|look| look.key == id) {
                Some(look) => look.purpose = Purpose::Card,
                None => looks.push(Fingerprint {
                    key: id,
                    folder,
                    purpose: Purpose::Card,
                    began: Some(agent.agent.started_at),
                }),
            }
        }
        looks
    }

    /// The fingerprints before `event`. A session's own, one per worktree (danielrispler/Falinks#434, decisions 8 to
    /// 10): at a turn's first touch of a worktree, its base, so what Daniel changed himself before
    /// does not count; at a turn's end, its `Stop` or the next turn's first event (danielrispler/Falinks#434, decision 5),
    /// every worktree it touched, whose change since its base is the card's, and whose record is
    /// the next turn's base. A turn that starts after the last one ended first cards what its
    /// subagents changed between them (decision 17). A subagent's: its first event's, its base; a
    /// helper's before and after each Bash step, for a change across it that makes it a worker; a
    /// worker's after each Bash step, for a commit; and a worker's end. On a step 20 minutes after
    /// the last card of the worktree it is in, that worktree's, for the fallback card; a session's
    /// only in a turn. A helper's look after its Bash step comes beside such a fallback's.
    #[allow(
        clippy::too_many_lines,
        reason = "left whole when the lint came in (#555); spec #548 names no split of it"
    )]
    fn look(&self, event: &Event) -> Vec<Fingerprint> {
        let step = event.tool_ends();
        let due = |since: &Since| step && event.studio_received_ms >= since.at + FALLBACK_MS;
        let session = self.sessions.get(&event.session_id);
        // A session's own fallback is on a prompt's steps only, not between its turns.
        let session_due = |s: &Session| s.in_turn() && due(&s.agent.since);
        let look = |key: &str, folder: &str, purpose| {
            let began = session.and_then(|s| match s.subagent(key) {
                Some(agent) => Some(agent.agent.started_at),
                // A turn that opens with this event begins at it.
                None if session.is_some_and(|s| s.opens_turn(event)) => {
                    Some(event.studio_received_ms)
                }
                None => s.turns.last().map(|t| t.started_at),
            });
            let look = Fingerprint {
                key: key.into(),
                folder: folder.into(),
                purpose,
                began,
            };
            (!folder.is_empty()).then_some(look)
        };
        let Some(id) = &event.agent_id else {
            let blank = Session::default();
            let s = session.unwrap_or(&blank);
            let key = &event.session_id;
            let ends = s.ends_turn_early(event) || event.turn_ends();
            let opens = s.opens_turn(event);
            // A turn's end, and a due fallback, look at every worktree it touched (danielrispler/Falinks#434, decision
            // 10).
            let mut closing = s.closing();
            // A turn seen only from its end, such as one the log starts in, ends where it ends.
            if closing.is_empty() && ends {
                closing = vec![self.roots.of(&event.cwd)];
            }
            let purpose = if ends {
                Some(Purpose::TurnEnd)
            } else {
                session
                    .is_some_and(session_due)
                    .then_some(Purpose::Fallback)
            };
            let mut looks: Vec<Fingerprint> = purpose
                .into_iter()
                .flat_map(|purpose| closing.iter().filter_map(move |r| look(key, r, purpose)))
                .collect();
            // Each worktree a turn touches first gets its base, which a turn's end just took. A
            // turn that starts after the last one ended first cards what subagents changed
            // between them (danielrispler/Falinks#434, decision 17).
            if opens || s.in_turn() && !event.turn_ends() {
                let between = if opens && !s.in_turn() {
                    s.between_turns(event.studio_received_ms)
                } else {
                    Vec::new()
                };
                let purpose = if between.is_empty() {
                    Purpose::Base
                } else {
                    Purpose::Card
                };
                let known = |root: &String| {
                    closing.contains(root) && ends || s.touched.contains(root) && !opens
                };
                let mut first: Vec<String> = self
                    .touches(event)
                    .into_iter()
                    .filter(|r| !known(r))
                    .collect();
                // The worktrees those subagents worked in, also those the turn touches only later.
                for id in &between {
                    let root = s.subagent(id).map(|a| a.root.clone()).unwrap_or_default();
                    if !root.is_empty() && !first.contains(&root) && !known(&root) {
                        first.push(root);
                    }
                }
                looks.extend(first.iter().filter_map(|root| look(key, root, purpose)));
            }
            return looks;
        };
        let Some(session) = session else {
            return Vec::new();
        };
        let agent = session.subagent(id);
        // The worktree this event makes its home, which its fingerprints are taken in from now on.
        let home = session.new_home(id, event, &self.roots);
        let Some(folder) = home
            .as_deref()
            .or_else(|| agent.and_then(|a| a.folder.as_deref()))
        else {
            return look(id, &event.cwd, Purpose::Base).into_iter().collect();
        };
        if home.is_some() {
            return look(id, folder, Purpose::Base).into_iter().collect();
        }
        let worker = |a: &Subagent| a.kind == Kind::Worker;
        let bash = event.tool().is_some_and(|t| t.kind == ToolKind::Command);
        // A helper's base right before its command, so only a change across it promotes it.
        // shortcut: the hooks are async, so a command that ends before the shell reads its
        // `PreToolUse` line is absorbed into this base and never promotes its helper; make the
        // hook synchronous if a live run shows missed promotions matter (danielrispler/Falinks#434, decision 23).
        if bash && event.tool_starts() && agent.is_some_and(|a| !worker(a)) {
            return look(id, folder, Purpose::Base).into_iter().collect();
        }
        let bash = step && bash;
        let owner = agent.map_or_default(|a| a.owner.as_str());
        let since = &session.record(owner).since;
        let fallback: Vec<Fingerprint> = match session.subagent(owner) {
            Some(own) if &own.agent.id == id => if worker(own) && due(since) {
                look(id, folder, Purpose::Fallback)
            } else {
                look(id, folder, Purpose::Commit).filter(|_| bash)
            }
            .into_iter()
            .collect(),
            // It works in the worktree of a worker above it, or of its session.
            Some(above) if worker(above) && due(since) => above
                .folder
                .as_deref()
                .and_then(|above_folder| look(&above.agent.id, above_folder, Purpose::Fallback))
                .into_iter()
                .collect(),
            None if session_due(session) => {
                let closing = session.closing().into_iter();
                let key = &session.agent.id;
                closing
                    .filter_map(|root| look(key, &root, Purpose::Fallback))
                    .collect()
            }
            _ => Vec::new(),
        };
        // A due fallback's look comes beside a helper's own (danielrispler/Falinks#434, decision 22).
        let helper =
            look(id, folder, Purpose::Commit).filter(|_| bash && agent.is_some_and(|a| !worker(a)));
        let helper = helper.filter(|h| fallback.iter().all(|f| f.key != h.key));
        fallback.into_iter().chain(helper).collect()
    }

    /// The sessions in a turn and the running subagents, with their folders, by their fingerprints'
    /// keys. After a replay the shell fingerprints them once, so a restart mid-turn leaves the
    /// turn's `Stop` and each subagent's next look a base to compare with.
    pub fn waiting_folders(&self) -> Vec<Fingerprint> {
        let waiting = self.sessions.values().filter(|s| s.in_turn());
        let agents = self.sessions.values().flat_map(|s| &s.agents);
        let running = agents.filter(|a| a.status == Status::Running);
        let running = running.filter_map(|a| Some((a.agent.id.as_str(), a.folder.as_deref()?)));
        // Every worktree a turn touched gets its base.
        let touched =
            waiting.flat_map(|s| s.touched.iter().map(|r| (s.agent.id.as_str(), r.as_str())));
        let folders = touched.chain(running);
        let print = |(key, folder): (&str, &str)| Fingerprint {
            key: key.into(),
            folder: folder.into(),
            purpose: Purpose::Base,
            began: None,
        };
        folders.map(print).collect()
    }

    /// Takes one live event log line that the shell took no fingerprint for, and returns what the
    /// shell must do.
    pub fn apply(&mut self, line: &str) -> Vec<Output> {
        self.apply_with_changes(line, BTreeMap::new())
    }

    /// Takes one live event log line, with what the shell's fingerprints before it found: the change
    /// in each worktree `fingerprint_folders` named since its base, by the fingerprint's key, with no
    /// entry when nothing changed or the shell took none. Returns what the shell must do. A line that
    /// is not a hook event is skipped.
    pub fn apply_with_changes(
        &mut self,
        line: &str,
        changes: BTreeMap<String, Change>,
    ) -> Vec<Output> {
        self.take(line, changes, false)
    }

    /// `apply_with_changes`, which on a replayed line keeps the turn it ends for `backfill`.
    fn take(
        &mut self,
        line: &str,
        mut changes: BTreeMap<String, Change>,
        replay: bool,
    ) -> Vec<Output> {
        let events = self.events(line);
        // A line's newline is not part of it; one the caller left off is counted all the same.
        let len = line.strip_suffix('\n').unwrap_or(line).len() as u64;
        let at = Bytes {
            offset: self.log_end,
            len,
        };
        self.log_end += len + 1;
        let mut outputs = Vec::new();
        for mut event in events {
            event.line = at;
            outputs.extend(self.take_event(event, &mut changes, replay));
        }
        outputs
    }

    /// The events of one log line, read by the agent its hook entry named (#355): Claude's, agy's,
    /// Codex's, Qwen Code's, or none and drift for an agent plotkeeper cannot read or a line its agent's parser
    /// cannot read. A line that names none is Claude's by its shape, or else agy's.
    fn events(&mut self, line: &str) -> Vec<Event> {
        match claude::parse(line) {
            Ok(event) => vec![event],
            Err(_) => match Named::of(line) {
                Named::Agy => self.agy.events(line).unwrap_or_default(),
                Named::Parsed(parse, at) => match parse(line) {
                    Ok(event) => vec![event],
                    Err(_) => {
                        self.agy.drift(at, AgyDriftKind::Agent);
                        Vec::new()
                    }
                },
                Named::Other(at) => {
                    self.agy.drift(at, AgyDriftKind::Agent);
                    Vec::new()
                }
            },
        }
    }

    /// The main event `line` would give, with agy's state left as it is.
    fn peek(&self, line: &str) -> Option<Event> {
        match claude::parse(line) {
            Ok(event) => Some(event),
            Err(_) => match Named::of(line) {
                Named::Agy => self.agy.peek(line),
                Named::Parsed(parse, _) => parse(line).ok(),
                Named::Other(_) => None,
            },
        }
    }

    /// Sets session `id`'s estimate from the start of its phase, from the sessions of its kind that
    /// started before it and have ended (danielrispler/Falinks#461, decision 14): a paused session's time may be cut short.
    fn estimate(&mut self, id: &str) {
        let Some(session) = self.sessions.get(id) else {
            return;
        };
        let work = &session.work;
        let past: Vec<&Work> = self
            .sessions
            .values()
            .filter(|s| {
                s.agent.started_at < session.agent.started_at
                    && work.kind.is_some()
                    && s.work.kind == work.kind
                    && s.ended
            })
            .map(|s| &s.work)
            .collect();
        let range = work.phase().and_then(|p| range(&past, p, MIN_HISTORY));
        if let Some(session) = self.sessions.get_mut(id) {
            session.work.estimate.range = range;
        }
    }

    /// A new studio with `lines` replayed into it, in order.
    pub fn replayed<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> Self {
        let mut studio = Self::default();
        lines
            .into_iter()
            .for_each(|line| studio.replay(line.as_ref()));
        studio
    }

    /// Takes one event log line replayed at app start. It updates state exactly like `apply` and
    /// requests nothing, so turns that ended while the app was down get no summaries and no notices
    /// here.
    pub fn replay(&mut self, line: &str) {
        self.take(line, BTreeMap::new(), true);
    }

    /// The trees the hook recorded of worktree `root` for turn `turn` of `session`: its start, and its end once the
    /// turn ended (danielrispler/plotkeeper#114) and no worker it spawned runs on there (danielrispler/plotkeeper#142).
    pub fn turn_trees(
        &self,
        session: &str,
        turn: usize,
        root: &str,
    ) -> Option<(String, Option<String>)> {
        let session = self.session(session)?;
        let (start, end) = session.turns.get(turn)?.trees.get(root)?.clone();
        let open = session.open_roots().contains(&(turn, root.into()));
        Some((start, end.filter(|_| !open)))
    }

    /// The turns whose files the shell should diff now, newest first, at most `limit`
    /// (danielrispler/plotkeeper#111): a running turn each 5 seconds, its end tree taken on demand,
    /// and an ended one of the last 7 days once, from the trees the hook recorded.
    pub fn turn_diffs(&mut self, now: u64, limit: usize) -> Vec<TurnDiff> {
        let mut asks = Vec::new();
        for session in self.sessions.values_mut() {
            if !session.capabilities.turn_trees || session.deleted {
                continue;
            }
            let running = session.open_roots();
            let windows: Vec<(u64, u64)> = (0..session.turns.len())
                .map(|index| session.trees_window(index))
                .collect();
            let (turns, session_ended, last_event) = (
                session.turns.len(),
                session.ended,
                session.agent.last_event_at,
            );
            for (index, turn) in session.turns.iter_mut().enumerate() {
                // A turn with no end of an ended session, or before a later turn, is over all
                // the same.
                let stale = turn.ended_at.is_none() && (session_ended || index + 1 < turns);
                let ended = turn.ended_at.is_some() || stale;
                let open = |root: &String| !ended || running.contains(&(index, root.clone()));
                // Final once it ended and no worker it spawned runs on in one of its worktrees.
                let last = ended && !turn.trees.keys().any(open);
                // A turn that never ended (its session was killed) ages from its start.
                let recent = turn.ended_at.unwrap_or(turn.started_at) + turn_files::SINCE_MS >= now;
                let due = match turn.files_asked {
                    None => true,
                    Some((_, true)) => false,
                    // The worktree only changes through the agent's own events, so a turn still open is asked again only
                    // once its session has had one since the last ask: a killed session's turn keeps its last result.
                    Some((at, false)) => {
                        now.saturating_sub(at) >= turn_files::RUNNING_REFRESH_MS
                            && (last || session.agent.last_event_at >= at)
                    }
                };
                if !recent || !due || turn.trees.is_empty() {
                    continue;
                }
                let trees: Vec<TurnTrees> = turn
                    .trees
                    .iter()
                    // An ended turn's worktree with no end tree has nothing to compare.
                    .filter(|(root, (_, end))| open(root) || end.is_some())
                    .map(|(root, (start, end))| TurnTrees {
                        root: root.clone(),
                        start: start.clone(),
                        end: end.clone().filter(|_| !open(root)),
                    })
                    .collect();
                if trees.is_empty() {
                    turn.files_asked = Some((now, last));
                } else {
                    let session_id = session.agent.id.clone();
                    let ask = TurnDiff {
                        session_id,
                        turn: index,
                        ended: last,
                        trees,
                        window: windows[index],
                    };
                    // A stale turn sorts by its start, or the session's last event, never as now.
                    let at = match turn.ended_at {
                        Some(at) => at,
                        None if !stale => now,
                        None if session_ended => last_event,
                        None => turn.started_at,
                    };
                    asks.push((at, ask));
                }
            }
        }
        asks.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        asks.truncate(limit);
        // Only the asks that survive the cut are asked, so the rest are due again in the next pass.
        for (_, ask) in &asks {
            let turn = self
                .sessions
                .get_mut(&ask.session_id)
                .and_then(|s| s.turns.get_mut(ask.turn));
            if let Some(turn) = turn {
                turn.files_asked = Some((now, ask.ended));
            }
        }
        asks.into_iter().map(|(_, ask)| ask).collect()
    }

    /// The diff the shell is asked for to write the summary of the turn named `id`
    /// (`SummaryRequest::id`): its trees as they ended. `None` for a turn with no trees.
    pub fn summary_ask(&self, id: &str) -> Option<TurnDiff> {
        let session = self
            .sessions
            .values()
            .find(|s| s.turns.iter().any(|t| t.id.as_deref() == Some(id)))?;
        let (index, turn) = session
            .turns
            .iter()
            .enumerate()
            .find(|(_, t)| t.id.as_deref() == Some(id))?;
        let trees: Vec<TurnTrees> = (turn.trees.iter())
            .filter_map(|(root, (start, end))| {
                let end = end.clone()?;
                Some(TurnTrees {
                    root: root.clone(),
                    start: start.clone(),
                    end: Some(end),
                })
            })
            .collect();
        (session.capabilities.turn_trees && !trees.is_empty()).then(|| TurnDiff {
            session_id: session.agent.id.clone(),
            turn: index,
            ended: true,
            trees,
            window: session.trees_window(index),
        })
    }

    /// The facts of the turn `ask` names, from the files `compute_turn_files` found for it.
    pub fn facts_of(&self, ask: &TurnDiff, found: Option<TurnFiles>) -> Facts {
        let Some(session) = self.sessions.get(&ask.session_id) else {
            return Facts::default();
        };
        let Some(turn) = session.turns.get(ask.turn) else {
            return Facts::default();
        };
        let found = found.unwrap_or_default();
        Facts {
            files: found
                .files
                .into_iter()
                .map(|file| FactFile {
                    file,
                    hunk: String::new(),
                })
                .collect(),
            checks: checks::ended(&turn.checks),
            external: 0,
        }
    }

    /// Marks the turn of `ask` as asked for the last time, for one whose diff the shell skipped as
    /// too large: it is not asked again until a new tree lands.
    pub fn settle_turn_files(&mut self, ask: &TurnDiff) {
        let turn = self
            .sessions
            .get_mut(&ask.session_id)
            .and_then(|s| s.turns.get_mut(ask.turn));
        if let Some(turn) = turn {
            turn.files_asked = Some((0, true));
        }
    }

    /// Takes what the shell found for `ask`: the numstat of each of its worktrees, `None` when git
    /// could not diff them. The turn's files are every file that differs, but those another
    /// session's or turn's call names (danielrispler/plotkeeper#207). The shell does the three
    /// steps apart, the matching with the core unlocked: `turn_files_snapshot`,
    /// `compute_turn_files`, `store_turn_files`.
    pub fn set_turn_files(&mut self, ask: &TurnDiff, diffs: Option<Vec<RootDiff>>) {
        let found = diffs.map(|diffs| {
            let snapshot = self.turn_files_snapshot(ask, &diffs);
            compute_turn_files(&snapshot, diffs)
        });
        self.store_turn_files(ask, found);
    }

    /// Stores the files `compute_turn_files` found for `ask`, `None` when git could not diff the turn.
    pub fn store_turn_files(&mut self, ask: &TurnDiff, found: Option<TurnFiles>) {
        let Some(session) = self.sessions.get_mut(&ask.session_id) else {
            return;
        };
        if let Some(turn) = session.turns.get_mut(ask.turn) {
            turn.files = found;
        }
        session.update_unseen();
    }

    /// What the matching of the files of `ask` among `diffs` reads: the calls that touched the same
    /// worktrees while its trees ran, which name the files that differ but are not the turn's, and
    /// the turn's own, which name those that are. Another call is any of another session's, and any
    /// of its own session's other turns.
    pub fn turn_files_snapshot(&self, ask: &TurnDiff, diffs: &[RootDiff]) -> TurnFilesSnapshot {
        let (session_id, turn) = (&ask.session_id, ask.turn);
        let Some(session) = self.sessions.get(session_id) else {
            return TurnFilesSnapshot::default();
        };
        let (since, until) = session.trees_window(turn);
        let made = |r: &&Row| {
            let rejected = r.outcome == Some(Outcome::Rejected);
            !rejected && !(r.edits() && r.failed())
        };
        let others: Vec<&Row> = (self.sessions.values())
            .flat_map(|s| {
                s.ledger
                    .rows()
                    .iter()
                    .map(move |r| (s.agent.id == *session_id, r))
            })
            .filter(|(own, r)| !*own || r.turn != Some(turn))
            .map(|(_, r)| r)
            .filter(made)
            .filter(|r| r.at().is_some_and(|at| (since..=until).contains(&at)))
            .collect();
        let ours: Vec<&Row> = (session.ledger.turn(turn)).filter(made).collect();
        let name = |agent: &Option<String>| match agent {
            Some(id) => session.agent_name(id).to_string(),
            None => session.capabilities.agent.to_string(),
        };
        let worker = |root: &str| {
            let spawned = session.agents.iter().filter(|a| a.turn == Some(turn));
            let mut own = spawned.filter(|a| a.root == root && session.has_cards(&a.agent.id));
            own.next().map(|a| name(&Some(a.agent.id.clone())))
        };
        TurnFilesSnapshot::of(
            &self.roots,
            diffs.iter().map(|d| d.root.as_str()),
            &others,
            &ours,
            |row| name(&row.agent),
            worker,
        )
    }

    /// Applies, after the replay, what the live app learned before a restart (danielrispler/Falinks#434, decisions 37 and
    /// 38): the card store's live-only `facts`, and its `cards`' requests.
    pub fn restore(&mut self, facts: &LiveFacts, cards: &[CardRequest]) {
        let agents = self.sessions.values_mut().flat_map(|s| &mut s.agents);
        for agent in agents {
            if facts.promoted.contains(&agent.agent.id) {
                agent.kind = Kind::Worker;
            }
            agent.opened |= facts.opened.contains(&agent.agent.id);
        }
        for session in self.sessions.values_mut() {
            let id = session.agent.id.clone();
            session.seen = facts.seen.get(&id).copied().unwrap_or_default();
            session.update_unseen();
            let ours: Vec<(&CardRequest, u64)> = cards
                .iter()
                .filter(|r| r.session_id == id)
                .map(|r| (r, card_time(&r.id)))
                .collect();
            // Its own last card, or else its last `Stop`, took the worker cards before it.
            let own = ours.iter().filter(|(r, _)| r.agent.is_none());
            let taken = own.map(|(_, at)| *at).fold(session.stopped_at, u64::max);
            for (request, at) in ours {
                let owner = request.owner();
                // Each owner's fallback counts from its newest card.
                if owner == id || session.subagent(owner).is_some() {
                    let since = &mut session.record_mut(owner).since;
                    since.at = since.at.max(at);
                }
                if request.agent.is_some() && at > taken {
                    session.agent.since.cards.push(request.id.clone());
                }
            }
        }
    }

    /// The shell's clock, in Unix milliseconds, on each pass of its read loop, the first right after
    /// the replay. Rules that fire with no event use it. Returns whether a session changed state.
    pub fn tick(&mut self, now: u64) -> bool {
        self.now = now;
        let mut changed = false;
        for session in self.sessions.values_mut() {
            // With no process to watch, a session in a turn or waiting on Daniel goes idle too: it
            // never ends by a guess, and its next event revives it (#330).
            let unwatched =
                session.pid.is_none() && matches!(session.state, State::Running | State::NeedsYou);
            if (session.state == State::YourTurn || unwatched)
                && now.saturating_sub(session.agent.last_event_at) >= IDLE_AFTER_MS
            {
                session.state = State::Idle;
                session.update_unseen();
                changed = true;
            }
        }
        changed
    }

    /// The resume ids of the sessions whose leader pid `alive` no longer lists (#330).
    pub fn gone(&self, alive: impl Fn(u32) -> bool) -> Vec<String> {
        let gone: Vec<&Session> = (self.sessions.values())
            .filter(|s| !s.ended && !s.deleted && !s.status_only)
            .filter(|s| s.pid.is_some_and(|pid| !alive(pid)))
            .collect();
        let mut ids: Vec<String> = gone.into_iter().map(|s| s.resume_id.clone()).collect();
        ids.sort();
        ids
    }

    /// The keys of the sessions that have not ended and of their subagents: the shell drops the
    /// fingerprint records and writer queues of every other key (danielrispler/Falinks#410, decision 18).
    pub fn live_keys(&self) -> std::collections::HashSet<String> {
        let live = self.sessions.values().filter(|s| !s.ended);
        live.flat_map(|s| std::iter::once(&s.agent.id).chain(s.agents.iter().map(|a| &a.agent.id)))
            .cloned()
            .collect()
    }

    /// The latest tick, in Unix milliseconds.
    pub fn now(&self) -> u64 {
        self.now
    }

    /// How each terminal of `terms` that has not exited comes back in a new daemon: a terminal spawned
    /// with an argv, a Claude terminal, reopens its latest session with `claude --resume`, whether or
    /// not the hangup ended it. A shell whose foreground program, or else its 7501 `app`, is another
    /// agent with a resume command (`studio_workbench::resume`, danielrispler/Falinks#561) comes back as that agent, rerun
    /// the same way at the next revival; any other shell restarts in its folder. A Claude terminal
    /// no session links to does not come back (#17). `busy` says what runs in it now.
    pub fn revival(&self, terms: &[Terminal]) -> Vec<Revive> {
        let running = terms.iter().filter(|t| t.exited.is_none());
        let revive = |t: &Terminal| {
            let linked = self
                .sessions
                .values()
                .filter(|s| s.terminal.as_ref() == Some(&t.id));
            let latest = linked.max_by_key(|s| s.agent.last_event_at);
            let program = || Some(t.foreground.clone().unwrap_or_else(|| "a program".into()));
            let busy = if t.argv.is_empty() {
                // Its leader is the shell, so its prompt is when the shell itself has the foreground.
                let leader = t.processes.iter().find(|p| Some(p.pid) == t.pid);
                let at_prompt = leader.is_some_and(|l| Some(&l.command) == t.foreground.as_ref());
                if at_prompt { None } else { program() }
            } else {
                match latest {
                    // The rule a close asks by (danielrispler/Falinks#469).
                    Some(s) if s.in_turn() => Some("a turn".into()),
                    Some(s) if s.asks_before_closing => Some("background tasks".into()),
                    Some(s) if !s.ended => None,
                    // Started, with no event yet, or its session ended while the program runs on.
                    _ => program(),
                }
            };
            // A shell's `busy` is its foreground program. The conversation's id is not known yet, so
            // the agent resumes the latest one in the folder.
            let agent = busy.as_deref().filter(|_| t.argv.is_empty());
            // A node-launched agent's foreground is `node`: its 7501 `app` names it then.
            let resume = |program: &str| studio_workbench::resume(program, None);
            let argv = agent.and_then(|p| resume(p).or_else(|| t.app.as_deref().and_then(resume)));
            // An agent the picker started (agy, Codex) resumes by its own command: its linked
            // session's conversation, else the folder's latest, as a status-only row has no id.
            // Only Claude's is `claude --resume`.
            let claude = t.argv.iter().any(|a| a == "claude");
            let picked = t.argv.first().filter(|_| !claude).and_then(|program| {
                let id = latest
                    .map(|s| s.resume_id.as_str())
                    .filter(|id| !id.is_empty());
                studio_workbench::resume(program, id)
            });
            Revive {
                id: t.id.clone(),
                cwd: t.cwd.clone(),
                argv: argv.or(picked).unwrap_or_else(|| t.argv.clone()),
                session: claude
                    .then(|| latest.map(|s| s.resume_id.clone()))
                    .flatten(),
                busy,
                revives: latest.is_some() || !t.argv.iter().any(|a| a == "claude"),
            }
        };
        running.map(revive).collect()
    }

    /// Sets whether session `id`'s terminal reports it blocked, and moves its state with it.
    fn terminal_blocks(&mut self, id: &str, blocked: bool) -> Option<Output> {
        let session = self.sessions.get_mut(id)?;
        if std::mem::replace(&mut session.terminal_blocked, blocked) == blocked || session.ended {
            return None;
        }
        let before = session.state;
        session.state = if session.needs_you() {
            State::NeedsYou
        } else if session.turn_ended {
            State::YourTurn
        } else {
            State::Running
        };
        session.update_unseen();
        session.work.estimate.depends_on_you = session.state == State::NeedsYou;
        (before != State::NeedsYou && session.state == State::NeedsYou).then(|| Output::NeedsYou {
            session_id: id.into(),
            agent: None,
        })
    }

    /// The daemon's `list` at `now` (danielrispler/Falinks#560). A terminal that has not exited, whose program reports OSC
    /// 7501 and is not Claude Code, gets a status-only row named by its `app`, else its foreground
    /// program, and its folder; the row goes when the status clears or the terminal does. A Claude
    /// session keeps its hook state; when its terminal's 7501 state disagrees past
    /// `MISMATCH_AFTER_MS`, one `Output::Mismatch` line says so. A plain terminal New terminal
    /// started (`shell-…`) gets a shell row, named "terminal · <folder>", while nothing in it reports
    /// a status or links a session. Returns the notices a status-only row raises, as a Claude
    /// session's would.
    #[allow(
        clippy::too_many_lines,
        reason = "left whole when the lint came in (#555); spec #548 names no split of it"
    )]
    pub fn terminals(&mut self, terms: &[Terminal], now: u64) -> Vec<Output> {
        let mut outputs = Vec::new();
        let reporting = terms.iter().filter(|t| t.exited.is_none());
        let reporting: Vec<(&Terminal, &str)> = reporting
            .filter_map(|t| Some((t, t.state.as_deref()?)))
            .collect();
        let claude_on = |sessions: &HashMap<String, Session>, id: &str| {
            let on = |s: &&Session| !s.status_only && !s.ended && s.terminal.as_deref() == Some(id);
            sessions.values().find(on).map(|s| s.agent.id.clone())
        };
        // A Claude session's hook state stays; a lasting disagreement is logged once.
        let mut disagreeing = BTreeSet::new();
        for (t, reported) in &reporting {
            let Some(id) = claude_on(&self.sessions, &t.id) else {
                continue;
            };
            // An agent with no permission hook of its own needs Daniel while its terminal says `blocked`.
            if !self.sessions[&id].capabilities.asks {
                outputs.extend(self.terminal_blocks(&id, *reported == "blocked"));
                continue;
            }
            let hook = self.sessions[&id].state;
            let hook_shown = if hook == State::Idle {
                State::YourTurn
            } else {
                hook
            };
            if shown_state(reported).is_none_or(|s| s == hook_shown) {
                continue;
            }
            disagreeing.insert(id.clone());
            let fresh = (now, reported.to_string(), false);
            let entry = self
                .mismatched
                .entry(id.clone())
                .or_insert_with(|| fresh.clone());
            if entry.1 != *reported {
                *entry = fresh;
            }
            let (since, _, logged) = entry;
            if !*logged && now.saturating_sub(*since) >= MISMATCH_AFTER_MS {
                *logged = true;
                let line = format!("{now} session {id}: hook {hook:?}, 7501 {reported}");
                outputs.push(Output::Mismatch { line });
                // A turn that ends normally reports `done`; only an interrupted one goes `idle`.
                let session = &self.sessions[&id];
                if hook == State::Running
                    && *reported == "idle"
                    && session.in_turn()
                    && let Some(prompt_id) = session.turn.clone()
                {
                    outputs.push(Output::Interrupted {
                        session_id: id.clone(),
                        prompt_id,
                    });
                }
            }
        }
        self.mismatched.retain(|id, _| disagreeing.contains(id));
        // A terminal that is gone, or reports no state, no longer says `blocked`.
        let silent: Vec<String> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.terminal_blocked)
            .filter(|(_, s)| {
                !reporting
                    .iter()
                    .any(|(t, _)| s.terminal.as_deref() == Some(&t.id))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in silent {
            outputs.extend(self.terminal_blocks(&id, false));
        }
        // Everyone else's terminal: a status-only row.
        let mut rows = BTreeSet::new();
        let at_start = !std::mem::replace(&mut self.listed, true);
        self.live_terminals = (terms.iter().filter(|t| t.exited.is_none()))
            .map(|t| t.id.clone())
            .collect();
        for (t, reported) in &reporting {
            let claude = t.app.as_deref() == Some("claude-code");
            if claude || claude_on(&self.sessions, &t.id).is_some() {
                continue;
            }
            // An agent typed into a plain terminal turns its row into the agent's.
            if self.sessions.get(&t.id).is_some_and(|s| s.shell) {
                self.sessions.remove(&t.id);
            }
            let shown = shown_state(reported);
            // An unknown state leaves a row as it was, and makes none.
            let Some(state) = shown.or(self.sessions.get(&t.id).map(|s| s.state)) else {
                continue;
            };
            rows.insert(t.id.clone());
            let at = t.status_ms.unwrap_or(now);
            let first_seen = !self.sessions.contains_key(&t.id);
            let row = self
                .sessions
                .entry(t.id.clone())
                .or_insert_with(|| Session {
                    agent: Agent::new(&t.id, at),
                    terminal: Some(t.id.clone()),
                    status_only: true,
                    state,
                    reported_turn: t.turn,
                    ..Session::default()
                });
            row.cwd.clone_from(&t.cwd);
            row.set_project_once(|| self.roots.of(&t.cwd));
            row.folder_name = folder_name(&t.cwd);
            let program = t.app.as_ref().or(t.foreground.as_ref());
            let program = program.map_or("a program", String::as_str);
            row.identity.name = format!("{program} · {}", row.folder_name);
            // Revival resumed the folder's latest conversation, which may not be the one it had (danielrispler/Falinks#561).
            if studio_workbench::resumes_latest(&t.argv) {
                row.identity.name.push_str(" · latest conversation");
            }
            row.agent.last_event_at = at;
            // A turn ended since the last list, even one that ended and restarted between two lists.
            let turn_ended = t.turn > std::mem::replace(&mut row.reported_turn, t.turn);
            if first_seen {
                if !at_start && state == State::NeedsYou {
                    outputs.push(Output::NeedsYou {
                        session_id: t.id.clone(),
                        agent: None,
                    });
                }
                continue;
            }
            // The tick's idle stays until the status changes.
            let idle = row.state == State::Idle && state == State::YourTurn && !turn_ended;
            let was = if idle {
                row.state
            } else {
                std::mem::replace(&mut row.state, state)
            };
            let reached = matches!(was, State::Running | State::NeedsYou) && was != state;
            match state {
                State::NeedsYou if was != state || turn_ended => {
                    outputs.push(Output::NeedsYou {
                        session_id: t.id.clone(),
                        agent: None,
                    });
                }
                State::NeedsYou => {}
                _ if turn_ended || state == State::YourTurn && reached => {
                    outputs.push(Output::YourTurn {
                        session_id: t.id.clone(),
                    });
                }
                _ => {}
            }
        }
        // A plain terminal New terminal started, with no row of its own yet: a shell row.
        let shells = terms
            .iter()
            .filter(|t| t.exited.is_none() && t.id.starts_with("shell-"));
        for t in shells {
            if rows.contains(&t.id) || claude_on(&self.sessions, &t.id).is_some() {
                continue;
            }
            rows.insert(t.id.clone());
            // An agent that quit leaves the plain terminal's row again.
            if self.sessions.get(&t.id).is_some_and(|s| !s.shell) {
                self.sessions.remove(&t.id);
            }
            let row = self
                .sessions
                .entry(t.id.clone())
                .or_insert_with(|| Session {
                    agent: Agent::new(&t.id, now),
                    terminal: Some(t.id.clone()),
                    status_only: true,
                    shell: true,
                    state: State::YourTurn,
                    ..Session::default()
                });
            row.cwd.clone_from(&t.cwd);
            row.set_project_once(|| self.roots.of(&t.cwd));
            row.folder_name = folder_name(&t.cwd);
            row.identity.name = format!("terminal · {}", row.folder_name);
        }
        self.sessions
            .retain(|id, s| !s.status_only || rows.contains(id));
        // A row from the list, not a hook event, for the terminal Daniel just started takes the
        // selection, as a session's first event does (danielrispler/Falinks#567).
        if let Some(id) = self.awaiting.take_if(|id| rows.contains(id)) {
            self.selected = Some(id);
            self.viewed = None;
        }
        outputs
    }

    /// Reads what was added to the transcripts of the sessions that have not ended, and of the
    /// selected one, since the last call (danielrispler/Falinks#461, decision 12). `log` gets one line when a session's
    /// transcript becomes unavailable. A session is read only from its first step or `Stop`, as its
    /// transcript may not exist before. Returns whether any session's transcript changed.
    pub fn follow_transcripts(&mut self, mut log: impl FnMut(String)) -> bool {
        let mut changed = false;
        for session in self.sessions.values_mut() {
            let shown = self.selected.as_ref() == Some(&session.agent.id);
            // shortcut: after a `/clear` the row keeps its old step, so the new transcript may read as
            // gone for one poll (one stderr line); track a step per transcript if that shows.
            let started = session.agent.last_step.is_some() || session.stopped.is_some();
            if !started
                || session.transcript_file.path.is_empty()
                || session.state == State::Ended && !shown
            {
                continue;
            }
            let before = session.transcript.clone();
            if let Some(why) = session.transcript_file.follow(&mut session.transcript) {
                log(format!(
                    "session {}: transcript unavailable, showing steps only: {why}",
                    session.agent.id
                ));
            }
            let ended = !session.in_turn();
            session.transcript.end(ended);
            if session.transcript.branch.is_some()
                && session.transcript.branch != session.identity.branch
            {
                session
                    .identity
                    .branch
                    .clone_from(&session.transcript.branch);
                session.identity.pr = None;
                session.identity.named();
            }
            changed |= session.transcript != before;
        }
        changed
    }

    /// What the agy adapter did not recognise, for `weekly` (decisions 6 and 15).
    pub fn agy_drift(&self) -> &[AgyDrift] {
        &self.agy.drift
    }

    /// The check lists of the settings, by project folder (`checks` in `settings.json`); a project
    /// with none uses the default list. Every turn's checks are read again against them.
    pub fn set_check_lists(&mut self, projects: impl IntoIterator<Item = (String, Vec<String>)>) {
        self.check_lists.set(projects);
        for session in self.sessions.values_mut() {
            let list = self.check_lists.of(&session.cwd);
            for turn in 0..session.turns.len() {
                session.recheck(turn, list, &self.roots, &self.durations);
            }
            session.update_unseen();
        }
    }

    /// Whether summaries are written when Daniel opens a turn (`summaries.on_open` in the settings).
    pub fn set_summaries_on_open(&mut self, on: bool) {
        self.summaries_on_open = on;
    }

    pub fn summaries_on_open(&self) -> bool {
        self.summaries_on_open
    }

    /// The whole final reply of turn `index` of session `session_id` (danielrispler/Falinks#574), which the rail shows
    /// folded; `None` when there is no such turn.
    pub fn turn_reply(&self, session_id: &str, index: usize) -> Option<String> {
        let turn = self.sessions.get(session_id)?.turns.get(index)?;
        Some(turn.reply.clone())
    }

    /// The branch, folder and project of each session that has not ended, and of the selected one, one per
    /// branch: what the shell asks `gh pr view` about, off the poll (danielrispler/Falinks#461, decision 19).
    pub fn branches(&self) -> Vec<BranchQuery> {
        let mut seen = BTreeMap::new();
        for session in self.sessions.values() {
            let shown = self.selected.as_ref() == Some(&session.agent.id);
            if let Some(branch) = &session.identity.branch
                && (session.state != State::Ended || shown)
            {
                seen.entry(branch.clone()).or_insert_with(|| BranchQuery {
                    project: session.project.clone(),
                    cwd: session.cwd.clone(),
                    branch: branch.clone(),
                });
            }
        }
        seen.into_values().collect()
    }

    /// The tasks the shell looks up for **Landed**: each worktree of a listed session that is under
    /// `.claude/worktrees/`, once, with its branch and issue.
    pub fn task_queries(&self) -> Vec<TaskQuery> {
        let mut queries = BTreeMap::<String, TaskQuery>::new();
        for session in self.sessions() {
            let Some(path) = task_path(&session.cwd) else {
                continue;
            };
            let query = queries.entry(path.clone()).or_insert(TaskQuery {
                path,
                branch: None,
                issue: None,
            });
            query.branch = query.branch.take().or(session.identity.branch.clone());
            query.issue = query.issue.or(session.identity.issue);
        }
        queries.into_values().collect()
    }

    /// The shell looked a task up: `None` when it could not, which leaves the task not landed.
    /// Returns whether the facts changed.
    pub fn set_task_facts(&mut self, path: &str, facts: Option<TaskFacts>) -> bool {
        let before = self.tasks.get(path).copied();
        match facts {
            Some(facts) => self.tasks.insert(path.into(), facts),
            None => self.tasks.remove(path),
        };
        before != facts
    }

    /// What the shell last found about the task folder `path`, `None` before its first lookup.
    pub fn task_facts_at(&self, path: &str) -> Option<TaskFacts> {
        self.tasks.get(path).copied()
    }

    /// Drops the facts of tasks no listed session works in any more, whose worktree is gone with them.
    pub fn forget_gone_tasks(&mut self) {
        let kept: Vec<String> = self.task_queries().into_iter().map(|q| q.path).collect();
        self.tasks.retain(|path, _| kept.contains(path));
    }

    /// What the shell last found about the task session `id` works in, `None` in the main checkout
    /// or before the first lookup.
    pub fn task_facts(&self, id: &str) -> Option<TaskFacts> {
        let path = task_path(&self.sessions.get(id)?.cwd)?;
        self.tasks.get(&path).copied()
    }

    /// The task folder session `id` works in, `None` in the main checkout or for no such session.
    pub fn task_of(&self, id: &str) -> Option<String> {
        task_path(&self.sessions.get(id)?.cwd)
    }

    /// Whether `session`'s task has landed: its work has shipped, so removing its worktree loses
    /// nothing. Nothing uncommitted or unpushed, its PR merged, its issue closed when a session
    /// there has one, and none of the sessions listed in the worktree running or needing Daniel.
    pub fn landed(&self, session: &Session) -> bool {
        let Some(path) = task_path(&session.cwd) else {
            return false;
        };
        let Some(facts) = self.tasks.get(&path) else {
            return false;
        };
        let there: Vec<&Session> = (self.sessions().into_iter())
            .filter(|s| task_path(&s.cwd).as_deref() == Some(path.as_str()))
            .collect();
        let has_issue = there.iter().any(|s| s.identity.issue.is_some());
        facts.safe
            && facts.merged
            && (!has_issue || facts.issue_closed)
            && there
                .iter()
                .all(|s| !matches!(s.state, State::Running | State::NeedsYou))
    }

    /// The shell found `pr` for `branch`; returns whether any session's name changed.
    pub fn set_pr(&mut self, branch: &str, pr: Option<u64>) -> bool {
        let mut changed = false;
        for session in self.sessions.values_mut() {
            let identity = &mut session.identity;
            if identity.branch.as_deref() == Some(branch) && identity.pr != pr {
                identity.pr = pr;
                identity.named();
                changed = true;
            }
        }
        changed
    }

    /// The typical duration of check `command`, in milliseconds: the median of how long its earlier
    /// runs took when they did not fail, its backgrounded runs included (danielrispler/Falinks#461, decisions 13 to 15).
    /// `None` when it never ran. Runs group by their check statements (`checks::key`) under the list
    /// of a session working in `cwd`.
    pub fn typical(&self, cwd: &str, command: &str) -> Option<u64> {
        self.durations.typical(self.check_lists.of(cwd), command)
    }

    /// Session `id`'s row: its own, or the row it continues after a `/clear`.
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions
            .get(self.rows.get(id).map_or(id, String::as_str))
    }

    /// The session list: every session Daniel has not deleted, grouped in `State` order, the newest first in each group.
    /// A session with no prompt is no row while its terminal's live session has prompts: a session
    /// loaded beside it, as by the `/resume` picker backed out of, until its first prompt (danielrispler/Falinks#562).
    /// Nor is a session with no folder and no prompt of an agent whose capabilities say it starts
    /// sessions from an empty window: the one Cursor.app's empty window starts at its launch, until its
    /// first prompt (#427).
    pub fn sessions(&self) -> Vec<&Session> {
        let empty_window = |s: &Session| {
            s.capabilities.empty_window_starts && s.cwd.is_empty() && s.prompts.is_empty()
        };
        let beside_live = |s: &Session| {
            s.prompts.is_empty()
                && s.terminal.is_some()
                && self.sessions.values().any(|live| {
                    live.terminal == s.terminal
                        && !live.ended
                        && !live.deleted
                        && !live.prompts.is_empty()
                })
        };
        let mut sessions: Vec<&Session> = (self.sessions.values())
            .filter(|s| !s.deleted && !beside_live(s) && !empty_window(s))
            .collect();
        sessions.sort_by_key(|s| (s.state, Reverse(s.agent.started_at), &s.agent.id));
        sessions
    }

    /// Daniel clicked session `id`'s row, whose own view shows. Only this and a session he starts
    /// (`select_terminal`) change the selection.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.into());
        let id = self.rows.get(id).map_or(id, String::as_str);
        if let Some(session) = self.sessions.get_mut(id) {
            session.seen_at_selection = session.seen;
            session.update_unseen();
        }
        self.viewed = None;
        self.awaiting = None;
        self.held = None;
    }

    /// Daniel went Home (spec #53): no session is selected or viewed.
    pub fn select_none(&mut self) {
        self.selected = None;
        self.viewed = None;
        self.awaiting = None;
        self.held = None;
    }

    /// Daniel looks at the selected session (danielrispler/Falinks#556): while it is unseen, and only if `shown`, the
    /// window focused with the story rail showing it, its ended turns count as seen. Returns its row's id and the turns seen when that
    /// changed, for the card store to keep.
    pub fn see_selected(&mut self, shown: bool) -> Option<(String, usize)> {
        let id = self.selected()?.agent.id.clone();
        let session = self.sessions.get_mut(&id).filter(|s| s.unseen)?;
        if !shown || self.held.as_ref() == Some(&id) {
            return None;
        }
        session.seen = session.turns.len();
        session.update_unseen();
        Some((id, session.seen))
    }

    /// Daniel marked session `id` unseen (danielrispler/Falinks#556): its last turn is unseen again, until he selects it
    /// again and looks. Returns its row's id and the turns seen, for the card store to keep.
    pub fn mark_unseen(&mut self, id: &str) -> Option<(String, usize)> {
        let id = self.session(id)?.agent.id.clone();
        let session = self.sessions.get_mut(&id)?;
        session.seen = session.turns.len().saturating_sub(1);
        session.seen_at_selection = session.seen;
        session.update_unseen();
        self.held = Some(id.clone());
        Some((id, session.seen))
    }

    /// Daniel started a session in studio terminal `terminal`: nothing is selected until its first
    /// event names that terminal, and then it is (danielrispler/Falinks#567). The pane and the rail never show two sessions.
    pub fn select_terminal(&mut self, terminal: &str) {
        self.selected = None;
        self.viewed = None;
        self.awaiting = Some(terminal.into());
    }

    /// Daniel opened subagent `id`'s line, so its view shows, with its own cards and brief, and a
    /// failed helper folds into its parent's line. Returns whether the opening counts, which the
    /// card store records: only a line whose agent had already failed (danielrispler/Falinks#434, decision 39).
    pub fn open(&mut self, id: &str) -> bool {
        self.viewed = Some(id.into());
        let agents = self.sessions.values_mut().flat_map(|s| &mut s.agents);
        let mut opened = false;
        for agent in agents.filter(|a| a.agent.id == id && a.failed()) {
            agent.opened = true;
            opened = true;
        }
        opened
    }

    /// The session Daniel selected, which the session view shows; `None` until his first click.
    pub fn selected(&self) -> Option<&Session> {
        self.session(self.selected.as_deref()?)
    }

    /// The log of subagent `agent` of session `session`, from byte `from`: the entries after it and
    /// the offset read to (danielrispler/plotkeeper#54). `None` is "no log": an unknown session or subagent, no
    /// file, or a transcript path that is no `.jsonl`. Stateless: the caller keeps the offset. It reads
    /// the file, so a caller that holds a lock on the studio takes `subagent_log_file` and reads after.
    pub fn subagent_log(&self, session: &str, agent: &str, from: u64) -> Option<SubagentLog> {
        self.subagent_log_file(session, agent, from)?.read()
    }

    /// Where the log of subagent `agent` of session `session` is and how to read it from byte `from`, with
    /// no file read (spec danielrispler/plotkeeper#548). A Claude subagent's file is in the parent's
    /// transcript folder, `<transcript without .jsonl>/subagents/agent-<id>.jsonl`; an agy subagent's
    /// is its own conversation's transcript. `None` for an unknown session or subagent, or a transcript
    /// path that is no `.jsonl`.
    pub fn subagent_log_file(
        &self,
        session: &str,
        agent: &str,
        from: u64,
    ) -> Option<SubagentLogFile> {
        let session = self.session(session)?;
        session.subagent(agent)?;
        let transcript = std::path::Path::new(&session.transcript_file.path);
        if transcript.extension()? != "jsonl" {
            return None;
        }
        let format = session.capabilities.transcript_format;
        let path = match format {
            TranscriptFormat::Claude => transcript
                .with_extension("")
                .join("subagents")
                .join(format!("agent-{agent}.jsonl")),
            TranscriptFormat::Agy => agy::subagent_transcript(transcript, agent)?,
            TranscriptFormat::Unread => return None,
        };
        Some(SubagentLogFile {
            path,
            agent: agent.into(),
            format,
            from,
            cwd: session.cwd.clone(),
        })
    }

    /// The selected session and the subagent of it Daniel opened; `None` until his first click.
    pub fn selection(&self) -> Option<Selection<'_>> {
        let session = self.selected()?;
        Some(Selection {
            session,
            agent: self.viewed.as_deref(),
        })
    }

    /// Whose cards and brief the session view shows: the subagent Daniel opened since he selected
    /// its session, else that session; `None` until his first click.
    pub fn viewed(&self) -> Option<&str> {
        self.viewed.as_deref().or(self.selected.as_deref())
    }

    /// The body of a notice on session `id`'s line: its folder and short id, as its row shows them,
    /// and the subagent `agent` it names, by its name.
    pub fn notice_body(&self, id: &str, agent: Option<&str>) -> String {
        let session = self.session(id);
        let folder = session.map(|s| s.folder_name.as_str());
        let mut body = format!("{} {}", folder.unwrap_or_default(), short(id, 8));
        if let Some(agent) = agent {
            let name = session.map_or(short(agent, 5), |s| s.agent_name(agent));
            body = format!("{body} · {name}");
        }
        body
    }
}
