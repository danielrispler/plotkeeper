//! The studio: every session, and what Daniel selected, fed one hook event at a time.

mod replay;
mod roots;
mod selection;
mod take;
mod tasks;
mod turn_files;

pub(crate) use roots::Roots;
pub use selection::Selection;
pub use tasks::{BranchQuery, TaskFacts, TaskQuery};

use crate::{
    AGY_CAPABILITIES, AgyDrift, AgyDriftKind, Bytes, CardRequest, Change, CheckLists, FactFile,
    Facts, Fingerprint, Kind, LiveFacts, MIN_HISTORY, Outcome, Output, Purpose, RootDiff, Row,
    Session, State, Status, Subagent, SubagentLog, SubagentLogFile, TranscriptFormat, TurnDiff,
    TurnFiles, TurnFilesSnapshot, TurnTrees, Work, agy, checks, claude, cline, codex,
    compute_turn_files, copilot, cursor,
    event::{Event, Happened, Start, ToolKind},
    opencode, qwen, range,
    session::{FALLBACK_MS, IDLE_AFTER_MS, Since, card_time, short, task_path},
};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
};

/// Who reads a line that is not Claude's: agy, when its hook entry named agy or named no agent (a
/// line written before entries named one), Codex (#359), Qwen Code (#360), or nobody, for another agent (#355). A new
/// harness is one arm here, beside its `Capabilities` constant and its parser.
enum Named {
    Agy,
    /// An agent read by a parser of its own, Codex's, Cursor's (#362), Qwen Code's, Copilot's (#361), OpenCode's (#370) or Cline's (#371), and the line's
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
            Some(cline::CLINE) => Named::Parsed(cline::parse, stamp.studio_received_ms),
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
    /// What the shell last found about each task's work, by the task's worktree folder: its files,
    /// its PR and its issue, asked at a turn's end, at the window's focus and on a refresh. The main
    /// checkout is never here, so it never lands.
    pub(crate) tasks: HashMap<String, TaskFacts>,
}

impl Studio {
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
            .filter(|s| !s.ended && !s.deleted)
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
