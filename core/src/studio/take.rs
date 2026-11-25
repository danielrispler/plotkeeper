//! `Studio::take_event`: one event into its session. It keeps the row and the marks Daniel made
//! (a rename, a prompt's name, a delete) on the studio, then hands the event to a `Take`, whose
//! handlers each follow one kind of `Happened`, and settles the session's state from what they did.

use super::{Roots, Studio};
use crate::{
    Agent, Answer, Asker, BlockedCommand, BlockedStatus, Change, CheckLists, Decision, Fingerprint,
    Kind, NameRequest, Output, Prompt, Purpose, Question, Sender, Session, Source, Span, State,
    Step, Stopped, Transcript, Turn, agy,
    checks::{self, matched},
    claude,
    event::{Event, Happened, Origin, Start, Tool, ToolKind},
    output::{Call, is_bare_command},
    session::{Block, Cause, card_id, claimed},
    transcript, wants_summary,
};
use serde_json::Value;
use std::collections::BTreeMap;

impl Studio {
    /// Takes one event of a log line into its session's row, and returns what the shell must do.
    pub(super) fn take_event(
        &mut self,
        mut event: Event,
        changes: &mut BTreeMap<String, Change>,
        replay: bool,
    ) -> Vec<Output> {
        let own_id = event.session_id.clone();
        event.session_id = self.row(&event);
        // A line the server wrote is Claude-shaped whatever the session's agent, so it reads as the
        // session's own agent's (#352).
        if event.written_by_studio()
            && let Some(session) = self.sessions.get(&event.session_id)
        {
            event.capabilities = session.capabilities;
        }
        let continues = event.session_id != own_id && !self.rows.contains_key(&own_id);
        if event.session_id != own_id {
            self.rows.insert(own_id.clone(), event.session_id.clone());
        }
        self.drop_throwaways(&event);
        if self.take_mark(&event) {
            return Vec::new();
        }
        let looks = self.looks(&event);
        let touches = self.touches(&event);
        let command = event.tool().and_then(|t| t.command.as_deref());
        let called = if event.tool().is_some() {
            self.named_roots(&event)
        } else {
            Vec::new()
        };
        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| {
                let mut agent = Agent::new(&event.session_id, event.studio_received_ms);
                agent.since.at = event.studio_received_ms;
                Session {
                    agent,
                    ..Session::default()
                }
            });
        let moves = moves_terminal(&event, session, self.listed, |own| {
            self.live_terminals.contains(own)
        });
        let mut take = Take {
            session,
            event: &event,
            changes,
            roots: &self.roots,
            check_lists: &self.check_lists,
            durations: &mut self.durations,
            replay,
            own: event.agent_id.is_none(),
            at: event.studio_received_ms,
            id: Some(event.call_id().to_string()).filter(|id| !id.is_empty()),
            called,
            starts: false,
            in_turn: false,
            outputs: Vec::new(),
        };
        take.follow_process(moves);
        // The session Daniel started in a new terminal takes the selection, so the rail follows its pane.
        if self.awaiting.is_some() && take.session.terminal == self.awaiting {
            self.awaiting = None;
            self.selected = Some(event.session_id.clone());
            self.viewed = None;
        }
        // So does one whose terminal's own row was selected, a status-only or shell row, whose id is
        // the terminal's: as agy's at its first prompt, or `claude` typed into a plain terminal.
        if take.session.terminal.is_some() && self.selected == take.session.terminal {
            self.selected = Some(event.session_id.clone());
            self.viewed = None;
        }
        let before = take.track(&own_id, continues, command);
        take.bound_turns(before.ends_early, before.opens);
        take.credit_subagent_edit();
        take.take_looks(&looks, before.opens);
        take.follow_retries();
        for happened in &event.happened {
            take.happened(happened);
        }
        let (outputs, phase_started) = take.settle(touches, &before);
        if phase_started {
            self.estimate(&event.session_id);
        }
        outputs
    }

    /// A resume or fork in a `claude` replaces the id it started under, which had no prompt
    /// (danielrispler/Falinks#562).
    fn drop_throwaways(&mut self, event: &Event) {
        if event.session_starts()
            && matches!(event.started_from(), Some(Start::Resume | Start::Fork))
            && event.studio_terminal_id.is_some()
        {
            let throwaway = |s: &&mut Session| {
                s.agent.id != event.session_id
                    && s.terminal == event.studio_terminal_id
                    && s.prompts.is_empty()
            };
            for s in self.sessions.values_mut().filter(throwaway) {
                s.deleted = true;
                // Daniel's new terminal selected the throwaway id; its replacement keeps the selection.
                if self.selected.as_deref() == Some(&s.agent.id) {
                    self.selected = Some(event.session_id.clone());
                }
            }
        }
    }

    /// Takes a rename, a prompt's name or a delete, which each name a known session and are none
    /// of its events. Returns whether the event was one.
    fn take_mark(&mut self, event: &Event) -> bool {
        if let Some(name) = event.happened.iter().find_map(|h| match h {
            Happened::Renamed(name) => Some(name),
            _ => None,
        }) {
            if let Some(session) = self.sessions.get_mut(&event.session_id) {
                session.name = Some(name.clone()).filter(|name| !name.is_empty());
            }
            return true;
        }
        if let Some(name) = event.happened.iter().find_map(|h| match h {
            Happened::PromptNamed(name) => Some(name),
            _ => None,
        }) {
            if let Some(session) = self.sessions.get_mut(&event.session_id) {
                session.prompt_name = Some(name.trim().to_owned()).filter(|n| !n.is_empty());
                session.name_after = None;
            }
            return true;
        }
        // A delete also drops the selection when it was this session.
        if event.has(|h| matches!(h, Happened::Deleted)) {
            if let Some(session) = self.sessions.get_mut(&event.session_id) {
                session.deleted = true;
            }
            if self.selected.as_deref() == Some(&event.session_id) {
                self.selected = None;
                self.viewed = None;
            }
            return true;
        }
        false
    }
}

/// Whether `event` moves `session`'s terminal link to its own. Every event of a studio-run
/// `claude` carries its terminal, so a resume elsewhere moves it (#101): a start does, and so does
/// an event from a terminal once the session's own is gone. An event with no terminal, or from a
/// second live terminal of the conversation, leaves the link as it is, so the pane neither loses a
/// running terminal nor flips between two. Before the first terminal list (`listed`) nothing is
/// known to be dead, so only a start or no link moves it.
fn moves_terminal(
    event: &Event,
    session: &Session,
    listed: bool,
    live: impl Fn(&String) -> bool,
) -> bool {
    match (&event.studio_terminal_id, &session.terminal) {
        (_, None) => true,
        (None, Some(_)) => event.session_starts(),
        (Some(new), Some(own)) => new == own || event.session_starts() || (listed && !live(own)),
    }
}

/// What `Take::track` found before the event's turn changes: what the event does to its turn, and
/// the session as it was, for the outputs `Take::settle` raises.
struct Before {
    /// The event ends the running turn before it starts a new one.
    ends_early: bool,
    /// The event opens a turn.
    opens: bool,
    phases: usize,
    needs_you: bool,
    state: State,
}

/// One event being taken into its session: the session, the studio's state its handlers read, and
/// the outputs so far.
struct Take<'a> {
    session: &'a mut Session,
    event: &'a Event,
    /// What the shell's fingerprints found, by key; a handler takes the change it cards.
    changes: &'a mut BTreeMap<String, Change>,
    roots: &'a Roots,
    check_lists: &'a CheckLists,
    durations: &'a mut checks::Durations,
    replay: bool,
    /// The event is the session's own agent's, not a subagent's.
    own: bool,
    at: u64,
    /// The event's call id, if it names one.
    id: Option<String>,
    /// The worktree roots its tool call names.
    called: Vec<String>,
    /// The event starts a turn, with the prompts queued for it.
    starts: bool,
    /// The session was in a turn before the event.
    in_turn: bool,
    outputs: Vec<Output>,
}

impl Take<'_> {
    /// The process facts every event carries: its terminal, pid, mode, model, branch and
    /// transcript format.
    fn follow_process(&mut self, moves: bool) {
        let (session, event) = (&mut *self.session, self.event);
        // A deleted session that starts again, by a resume, is back in the list; a late end is not.
        session.deleted &= !event.session_starts();
        // The hooks are async, so the latest event can land before an earlier one.
        session.agent.last_event_at = session.agent.last_event_at.max(event.studio_received_ms);
        if moves {
            session.terminal.clone_from(&event.studio_terminal_id);
        }
        // A subagent's event is the same process as its session's.
        if event.studio_pid.is_some() {
            session.pid = event.studio_pid;
        }
        if let Some(mode) = event.permission_mode() {
            session.permission_mode = Some(mode.into());
            if session.started_mode.is_none() {
                session.started_mode = Some(mode.into());
            }
        }
        session.capabilities = event.capabilities;
        if let Some(model) = event.model() {
            session.model = Some(model.into());
        }
        if let Some(branch) = event.branch() {
            session.transcript.branch = Some(branch.into());
        }
        session.transcript_file.format = event.capabilities.transcript_format;
    }

    /// Its work, its resume id, its subagents and its folder, before the event's turn changes; and
    /// the workers that failed or were promoted by it.
    fn track(&mut self, own_id: &str, continues: bool, command: Option<&str>) -> Before {
        let (session, event) = (&mut *self.session, self.event);
        let working = session.state == State::Running;
        session.work.passes(event.studio_received_ms, working);
        let phases = session.work.phases.len();
        // The old id of a fork goes on sending late events, but the fork is what `--resume` takes.
        if session.resume_id.is_empty() || event.session_starts() {
            session.resume_id = own_id.to_owned();
        }
        if continues && event.started_from() == Some(Start::Clear) {
            session.cleared.push(event.studio_received_ms);
        }
        let failed_before = session.failed_workers();
        self.in_turn = session.in_turn();
        let ends_early = session.ends_turn_early(event);
        self.starts = session.starts_turn(event);
        let opens = session.opens_turn(event);
        let helpers: Vec<String> = session
            .agents
            .iter()
            .filter(|a| a.kind == Kind::Helper)
            .map(|a| a.agent.id.clone())
            .collect();
        let its_own = |id: &String| self.changes.contains_key(id);
        let changed = event.agent_id.as_ref().is_some_and(its_own);
        session.track_agents(event, changed, self.roots);
        self.follow_folder(own_id, command);
        let session = &*self.session;
        let failed = session.failed_workers().into_iter();
        self.outputs.extend(
            failed
                .filter(|agent| !failed_before.contains(agent))
                .map(|agent| Output::Failed {
                    session_id: event.session_id.clone(),
                    agent,
                }),
        );
        let worker = |id: &String| session.subagent(id).is_some_and(|a| a.kind == Kind::Worker);
        let promoted = helpers.into_iter().filter(worker);
        self.outputs
            .extend(promoted.map(|agent| Output::Promoted { agent }));
        Before {
            ends_early,
            opens,
            phases,
            needs_you: session.needs_you(),
            state: session.state,
        }
    }

    /// The session's own folder, worktree, claimed issue and transcript. A child's events still
    /// keep its session from going idle, but its folder and steps are its own.
    fn follow_folder(&mut self, own_id: &str, command: Option<&str>) {
        let (session, event) = (&mut *self.session, self.event);
        if self.own && !event.cwd.is_empty() {
            session.cwd.clone_from(&event.cwd);
            let last = event.cwd.rsplit('/').next().unwrap_or_default();
            session.folder_name = if last.is_empty() { &event.cwd } else { last }.into();
            let root = self.roots.of(&event.cwd);
            let last = root.rsplit('/').next().unwrap_or_default();
            session.identity.worktree = if last.is_empty() { &root } else { last }.into();
            if let Some(issue) = claimed(command.unwrap_or_default()) {
                session.identity.claim = Some(issue);
            }
            session.identity.named();
        }
        // A resume or `/clear` may move it to another transcript, read from its start.
        if self.own
            && own_id == session.resume_id
            && !event.transcript_path.is_empty()
            && session.transcript_file.path != event.transcript_path
        {
            session.transcript_file = transcript::TranscriptFile::default();
            session
                .transcript_file
                .path
                .clone_from(&event.transcript_path);
            session.transcript_file.format = event.capabilities.transcript_format;
            session.transcript = Transcript::default();
        }
    }

    /// The turn this event ends, then the one it starts, with the prompts queued for it.
    fn bound_turns(&mut self, ends_early: bool, opens: bool) {
        let (session, event, at) = (&mut *self.session, self.event, self.at);
        if ends_early {
            // A naming turn Daniel cancelled before any call: his next prompt names the session.
            let naming = session.naming_turn.is_some() && session.naming_turn == session.turn;
            let called = |id: &str| {
                (session.ledger.rows().iter()).any(|r| r.prompt_id.as_deref() == Some(id))
            };
            if naming && !session.turn.as_deref().is_some_and(called) {
                session.name_asked = false;
                session.name_after = None;
            }
            session.naming_turn = None;
            let change = self.changes.remove(&session.agent.id);
            self.outputs.extend(session.end_turn(change, at));
            if event.capabilities.turn_trees {
                session.keep_end_trees(&event.studio_turn_end);
            }
        }
        if self.starts {
            session.turn.clone_from(&event.prompt_id);
            session.reopen_turn();
            session.woken = false;
            let queued = std::mem::take(&mut session.queued);
            for (index, mut prompt) in queued.into_iter().enumerate() {
                // The turn this event opens, pushed below.
                prompt.turn = Some(session.turns.len());
                session.take_prompt(prompt, index == 0);
            }
        }
        if opens {
            session.turns.push(Turn {
                started_at: at,
                ..Turn::default()
            });
            session.turn_span = Some(Span {
                start: at,
                end: None,
            });
            session.touched.clear();
            session.trees.clear();
        }
        if self.own {
            for (root, tree) in &event.studio_turn_start {
                session
                    .trees
                    .entry(root.clone())
                    .or_insert_with(|| tree.clone());
            }
            if event.capabilities.turn_trees {
                session.keep_start_trees(&event.studio_turn_start);
            }
        } else if event.capabilities.turn_trees {
            session.keep_worker_trees(event);
        }
    }

    /// Who changed what (danielrispler/Falinks#369, story 20): a subagent's edit in a worktree
    /// whose cards are another's.
    fn credit_subagent_edit(&mut self) {
        let (session, event) = (&mut *self.session, self.event);
        if let Some(agent) = &event.agent_id
            && event.tool_succeeds()
            && let Some(path) = event.tool().and_then(|t| t.path.as_deref())
        {
            let (owner, root) = session
                .subagent(agent)
                .map_or_default(|a| (a.owner.clone(), a.root.clone()));
            // Only a path inside the owner's worktree (danielrispler/Falinks#434, decision 15).
            let inside = self.roots.inside(&root, path);
            if owner != *agent
                && let Some(inside) = inside.filter(|_| !root.is_empty())
            {
                let by = &mut session.record_mut(&owner).since.by;
                by.entry(agent.clone()).or_default().insert(inside.into());
            }
        }
    }

    /// A worker's card at its commit, seen after its Bash step, or at its end, and any worktree's
    /// after 20 minutes: when its worktree changed since its last card (danielrispler/Falinks#369,
    /// decision 11). The session's own look at a prompt is its base, and at `Stop` its card's
    /// change, in `turn_end`.
    fn take_looks(&mut self, looks: &[Fingerprint], opens: bool) {
        let (session, at) = (&mut *self.session, self.at);
        for look in looks {
            let found = match look.purpose {
                Purpose::Base | Purpose::TurnEnd => continue,
                Purpose::Commit => {
                    (self.changes.remove(&look.key)).filter(|c| c.head != c.base.head)
                }
                Purpose::Card | Purpose::Fallback => self.changes.remove(&look.key),
            };
            let Some(found) = found else {
                // A quiet fallback restarts its worktree's 20-minute wait.
                if look.purpose == Purpose::Fallback {
                    session.record_mut(&look.key).since.at = at;
                }
                continue;
            };
            let worker = look.key != session.agent.id;
            // A session's card between turns is tagged with the subagents that ran then (danielrispler/Falinks#434,
            // decisions 16 and 17).
            if !worker && look.purpose == Purpose::Card {
                for agent in session.between_turns(at) {
                    session.agent.since.by.entry(agent).or_default();
                }
            }
            if !worker || session.has_cards(&look.key) {
                let request = session.card(&look.key, Some(found), at);
                if worker {
                    session.agent.since.cards.push(request.id.clone());
                }
                self.outputs.push(Output::Card(request));
            }
        }
        // A base for the turn: what Daniel changed himself before it is no one's.
        if opens
            && looks
                .iter()
                .any(|look| look.purpose == Purpose::Base && look.key == session.agent.id)
        {
            session.agent.since.at = at;
            session.agent.since.by.clear();
        }
    }

    /// A blocked command waits on Daniel until its agent moves on (#572): its next call, a new turn, or
    /// a subagent's end. Its turn's end does not move it on: the agent stopped for Daniel, who handles
    /// the command in his agent. A blocked command's retry, which only a log from before #572 holds,
    /// follows the run: its start, its end, and the wake-up of a background task.
    fn follow_retries(&mut self) {
        let event = self.event;
        let acts = event.has(|h| {
            matches!(
                h,
                Happened::ToolStart { .. } | Happened::TurnStart { .. } | Happened::SubagentEnd
            )
        });
        if acts {
            let agent = &event.agent_id;
            self.session
                .blocks
                .retain(|w| !(w.cause == Cause::Denied && w.agent == *agent));
        }
        for happened in &event.happened {
            match happened {
                Happened::ToolStart { tool, id } if tool.kind == ToolKind::Command => {
                    self.retry_started(tool, id);
                }
                Happened::TurnEnd { .. } | Happened::SubagentEnd => {
                    self.approvals_unused(matches!(happened, Happened::SubagentEnd));
                }
                Happened::ToolEnd { id, failed, .. } => self.retry_ended(id, *failed),
                _ => {}
            }
        }
    }

    /// An allowed card's command runs again, by call `id`.
    fn retry_started(&mut self, tool: &Tool, id: &str) {
        let event = self.event;
        let card = self.session.blocked_commands.iter_mut().find(|c| {
            matches!(
                c.status,
                BlockedStatus::Allowed | BlockedStatus::AllowedNotRun
            ) && c.retried_by(tool, &event.cwd, event.agent_id.as_deref())
        });
        if let Some(card) = card {
            card.status = BlockedStatus::Running;
            card.run = Some(id.to_owned());
            card.run_at = Some(self.at);
            card.run_by.clone_from(&event.agent_id);
        }
    }

    /// A turn that ends with no retry leaves the approval unused: the session's own turn covers its
    /// own cards and those of subagents that have ended; a running subagent's card waits for that
    /// subagent's own end (`own_end`), since its worker may still retry by itself.
    fn approvals_unused(&mut self, own_end: bool) {
        let (session, event) = (&mut *self.session, self.event);
        let ended = |agent: &Option<String>| {
            agent
                .as_deref()
                .is_none_or(|id| session.subagent(id).is_none_or(|a| a.ended_at.is_some()))
        };
        let flips: Vec<bool> = session
            .blocked_commands
            .iter()
            .map(|card| {
                if own_end {
                    card.agent.is_some() && card.agent == event.agent_id
                } else {
                    ended(&card.agent)
                }
            })
            .collect();
        for (card, flip) in session.blocked_commands.iter_mut().zip(flips) {
            if flip && card.status == BlockedStatus::Allowed {
                card.status = BlockedStatus::AllowedNotRun;
            }
        }
    }

    /// The retry's end: finished with its exit code, or running on in the background.
    fn retry_ended(&mut self, id: &str, failed: bool) {
        let event = self.event;
        let card = self
            .session
            .blocked_commands
            .iter_mut()
            .find(|c| c.status == BlockedStatus::Running && c.run.as_deref() == Some(id));
        if let Some(card) = card {
            match event.background_task() {
                Some(task) => {
                    card.status = BlockedStatus::RunningInBackground;
                    card.task = Some(task.into());
                }
                None => {
                    card.status = BlockedStatus::Finished;
                    card.exit_code = if failed { event.exit_code() } else { Some(0) };
                }
            }
        }
    }

    /// The dispatcher: hands one thing the event says happened to its handler.
    fn happened(&mut self, happened: &Happened) {
        let event = self.event;
        match happened {
            Happened::CommandBlocked { tool, id, reason } => self.command_blocked(tool, id, reason),
            Happened::BlockedAnswered { tool, id, answer } => {
                self.blocked_answered(tool, id, answer);
            }
            Happened::TurnStart { prompt, origin } => self.turn_start(prompt, origin.as_ref()),
            // A reply in an event of its own waits for its turn's end (#362). Only the main agent's: a
            // subagent's reply (Claude Code's `SubagentStop`) is never its parent turn's.
            Happened::Reply(reply)
                if event.agent_id.is_none()
                    && !event
                        .has(|h| matches!(h, Happened::TurnEnd { .. } | Happened::SubagentEnd)) =>
            {
                self.reply(reply);
            }
            Happened::TurnEnd { background_pending } => self.turn_end(*background_pending),
            Happened::SessionEnd => self.session_end(),
            Happened::SessionStart => self.session_start(),
            Happened::ToolStart { tool, .. } => self.tool_start(tool),
            // It has no `tool_use_id`, so it names its call by tool and input. With no agent, it is
            // the session's own agent's.
            // A retry's pass from the card allows it at once, so it does not wait on Daniel; its
            // `PreToolUse` may land after it, as the hooks are async.
            Happened::PermissionAsked { tool }
                if !tool.asks()
                    && !self
                        .session
                        .passes(tool, &event.cwd, event.agent_id.as_deref()) =>
            {
                let (name, input) = (tool.name.clone(), tool.input.clone());
                (self.session).block(Cause::Call, name, input, event.agent_id.clone(), self.at);
            }
            Happened::PermissionNotice => self.permission_notice(),
            Happened::ElicitationAsked => {
                let agent = event.agent_id.clone();
                (self.session).block(
                    Cause::Elicitation,
                    String::new(),
                    Value::Null,
                    agent,
                    self.at,
                );
            }
            Happened::Answered {
                tool,
                id: call_id,
                answer,
            } if answer.get("decision").is_some() => self.decided_on_card(tool, call_id, answer),
            // A question answered on the card: its result, which carries the answer, is the card's.
            Happened::Answered { id: call_id, .. } => {
                self.session.carded.insert(call_id.clone());
            }
            Happened::ToolEnd { tool, failed, .. } => self.tool_end(tool, *failed),
            _ => {}
        }
    }

    /// The hook blocked command call `id`: a card for it, or its own card's again after Allow.
    fn command_blocked(&mut self, tool: &Tool, id: &str, reason: &str) {
        let (session, event, at) = (&mut *self.session, self.event, self.at);
        let command = tool.command.clone().unwrap_or_default();
        // The blocked call never ran, so it is no run of a check.
        session.ledger.block(event.agent_id.as_deref(), id);
        let turn = session.ledger.row(event.agent_id.as_deref(), id);
        let turn = turn
            .and_then(|r| r.turn)
            .filter(|t| *t < session.turns.len());
        if let Some(turn) = turn {
            let list = self.check_lists.of(&session.cwd);
            session.recheck(turn, list, self.roots, self.durations);
        }
        // The same command blocked again after Allow is its card's, not a new one. A running
        // card takes only the block of its own retry: another call's is a card of its own.
        let again = session
            .blocked_commands
            .iter_mut()
            .find(|c| match c.status {
                BlockedStatus::Allowed | BlockedStatus::AllowedNotRun => {
                    c.retried_by(tool, &event.cwd, event.agent_id.as_deref())
                }
                BlockedStatus::Running => c.run.as_deref() == Some(id),
                _ => false,
            });
        // The blocked call never ran: it leaves the running calls and its step.
        session.running.retain(|c| c.id.as_deref() != Some(id));
        match &event.agent_id {
            Some(agent) => {
                let mut agents = session.agents.iter_mut();
                if let Some(a) = agents.find(|a| a.agent.id == *agent) {
                    a.agent.step_done(id, at);
                }
            }
            None => session.agent.step_done(id, at),
        }
        match again {
            Some(card) => {
                card.status = BlockedStatus::BlockedAgain;
                id.clone_into(&mut card.call);
                reason.clone_into(&mut card.reason);
                // A new block: the feed places the card in the turn running now.
                card.at = at;
                card.run = None;
                card.run_at = None;
            }
            None => session.blocked_commands.push(BlockedCommand {
                call: id.to_owned(),
                command,
                cwd: event.cwd.clone(),
                reason: reason.to_owned(),
                agent: event.agent_id.clone(),
                at,
                status: BlockedStatus::Blocked,
                run: None,
                run_at: None,
                run_by: None,
                exit_code: None,
                task: None,
            }),
        }
        let agent = event.agent_id.clone();
        session.block(
            Cause::Denied,
            tool.name.clone(),
            tool.input.clone(),
            agent,
            at,
        );
        session.blocks.last_mut().expect("just pushed").denied = Some(id.to_owned());
    }

    /// Daniel's Allow or Decline, in a log from before the card went (#572): the decision enters the
    /// history on the card.
    fn blocked_answered(&mut self, tool: &Tool, id: &str, answer: &Value) {
        let session = &mut *self.session;
        let allowed = answer["decision"] == "allow";
        if let Some(card) = session.blocked_commands.iter_mut().find(|c| c.call == *id) {
            card.status = if allowed {
                BlockedStatus::Allowed
            } else {
                BlockedStatus::Declined
            };
            session.decisions.push(Decision {
                call: id.to_owned(),
                agent: card.agent.clone(),
                tool: tool.name.clone(),
                input: tool.input.clone(),
                answer: answer["decision"].clone(),
                at: self.at,
                source: Source::Card,
            });
        }
        session.blocks.retain(|w| w.denied.as_deref() != Some(id));
    }

    /// A prompt opens a wait, also after an interrupt, which fires no `Stop`.
    fn turn_start(&mut self, prompt: &str, origin: Option<&Origin>) {
        let session = &mut *self.session;
        session.reply.clear();
        let sender = match origin {
            Some(Origin::Daniel) => Sender::Daniel,
            Some(Origin::WakeUp) => Sender::WakeUp(session.woken_by()),
            None => session.sender(prompt),
        };
        let wake_up = !sender.is_daniels();
        self.background_retries_reported(prompt);
        let session = &mut *self.session;
        match &sender {
            // The row says which worker returned until the session's next step.
            Sender::WakeUp(task_id) => {
                let returned = format!("worker {} returned", session.agent_name(task_id));
                session.returned = Some(returned);
            }
            Sender::Unrecognised(task_id) => self.outputs.push(Output::Unrecognised {
                session_id: self.event.session_id.clone(),
                task_id: task_id.clone(),
            }),
            Sender::Daniel => {}
        }
        if sender.is_daniels() {
            self.ask_name(prompt);
        }
        self.checks_reported();
        let (session, at) = (&mut *self.session, self.at);
        session.work.prompt(prompt, at);
        let prompt = Prompt {
            at,
            sender,
            turn: session.current_turn(),
            text: prompt.to_owned(),
        };
        // A prompt with its running turn's id is the next turn's (danielrispler/Falinks#434, decision 4).
        if self.event.prompt_id.is_some() && !self.starts && self.in_turn {
            session.queued.push(prompt);
        } else {
            session.take_prompt(prompt, self.starts || !self.in_turn);
            // Daniel's prompt answers what asked him; a wake-up is not his, so it answers
            // nothing.
            if !wake_up {
                session.returned = None;
                session.unblock();
            }
            session.reopen_turn();
        }
    }

    /// A background retry ends at the notification that names its task, one of perhaps several in
    /// the wake-up.
    fn background_retries_reported(&mut self, prompt: &str) {
        for (task, _) in self.event.reported() {
            for card in &mut self.session.blocked_commands {
                if card.status == BlockedStatus::RunningInBackground
                    && card.task.as_ref() == Some(task)
                {
                    card.status = BlockedStatus::Finished;
                    card.exit_code = claude::exit_code_of(prompt, task);
                }
            }
        }
    }

    /// Daniel's first prompt asks for a name; a bare slash command waits for its turn's end.
    fn ask_name(&mut self, prompt: &str) {
        let session = &mut *self.session;
        if session.name_asked {
            return;
        }
        session.name_asked = true;
        session.naming_turn.clone_from(&self.event.prompt_id);
        if is_bare_command(prompt) {
            session.name_after = Some(prompt.to_owned());
        } else if !self.replay {
            self.outputs.push(Output::Name(NameRequest {
                session_id: session.agent.id.clone(),
                prompt: prompt.to_owned(),
                response: None,
                cwd: session.cwd.clone(),
            }));
        }
    }

    /// A background check ends at the notification that names its task: its turn's checks are
    /// read again with what the notification reports.
    fn checks_reported(&mut self) {
        let (session, at) = (&mut *self.session, self.at);
        for (task, status) in self.event.reported() {
            let (task, status) = (task.as_str(), status.as_str());
            let row = session.ledger.report(task, status, at).cloned();
            let list = self.check_lists.of(&session.cwd);
            if let Some(row) = row {
                let ran = row.began().filter(|_| row.agent.is_none());
                if let (Some(began), "completed") = (ran, status) {
                    let command = row.command.as_deref().unwrap_or_default();
                    let took = at.saturating_sub(began);
                    self.durations.record(list, command, took);
                }
                if let Some(turn) = row.turn.filter(|t| *t < session.turns.len()) {
                    session.recheck(turn, list, self.roots, self.durations);
                }
            }
        }
    }

    /// The main agent's reply in an event of its own. Cursor's can land just after its turn's
    /// `stop` (#369): it is that turn's, unless the turn has one or another has started. The `stop`
    /// asked for no wanted summary, having no reply, so a long reply asks now, with the card it
    /// raised.
    fn reply(&mut self, reply: &str) {
        let session = &mut *self.session;
        let ended = session
            .turns
            .last()
            .filter(|t| !session.in_turn() && t.reply.is_empty() && t.id.is_some());
        match (ended.and_then(|t| t.id.clone()), &mut session.stopped) {
            (Some(id), Some(stopped)) if stopped.card == id => {
                let card = stopped.requested.then(|| id.clone());
                let reply = reply.trim();
                let message = reply.lines().map(str::trim).find(|l| !l.is_empty());
                stopped.message = message.map(Into::into);
                if let Some(turn) = session.turns.last_mut() {
                    turn.reply = reply.into();
                    turn.message = message.map(Into::into);
                }
                if !self.replay && wants_summary(reply, session.needs_you()) {
                    self.outputs.push(Output::Summary(
                        session.summary_request(id, reply, card, true),
                    ));
                }
            }
            _ => reply.clone_into(&mut session.reply),
        }
    }

    /// The turn's end: its card, its summary, its name, and the version of the agent.
    fn turn_end(&mut self, background_pending: bool) {
        let (session, event, at) = (&mut *self.session, self.event, self.at);
        // A reply that asks in text blocks nothing, so the turn ends on your turn unless a
        // subagent's block outlasts it (danielrispler/Falinks#540).
        let in_turn = session.in_turn();
        let waiting = std::mem::take(&mut session.reply);
        let full = match event.reply() {
            "" => waiting.as_str(),
            own => own,
        };
        session.background = background_pending;
        let output = session.end_turn(self.changes.remove(&session.agent.id), at);
        if in_turn && event.capabilities.turn_trees {
            session.keep_end_trees(&event.studio_turn_end);
        }
        self.ask_version();
        let session = &mut *self.session;
        let reply = full.trim();
        let wanted = wants_summary(reply, session.needs_you());
        // The turn's card goes first: its summary then finds the card stored.
        let card = match &output {
            Some(Output::Card(card)) => Some(card.id.clone()),
            _ => None,
        };
        self.outputs.extend(output);
        // A turn that changed code has its summary requested, which is written only when it is wanted.
        if !self.replay && (wanted || card.is_some()) {
            self.outputs.push(Output::Summary(session.summary_request(
                card_id(&event.session_id, at),
                reply,
                card.clone(),
                wanted,
            )));
        }
        if let Some(prompt) = session.name_after.take()
            && !self.replay
        {
            self.outputs.push(Output::Name(NameRequest {
                session_id: session.agent.id.clone(),
                prompt,
                response: Some(reply.into()),
                cwd: session.cwd.clone(),
            }));
        }
        let mut lines = full.lines().map(str::trim);
        let stopped = Stopped {
            card: card_id(&event.session_id, at),
            requested: card.is_some(),
            message: lines.find(|line| !line.is_empty()).map(Into::into),
        };
        if let Some(turn) = session.turns.last_mut().filter(|_| in_turn) {
            turn.id = Some(stopped.card.clone());
            turn.message.clone_from(&stopped.message);
            turn.reply = full.trim().into();
        }
        session.stopped = Some(stopped);
    }

    /// The agent's version, asked once: from the transcript where it has one, from `agy
    /// --version` for agy; another agent with no transcript version has none to ask for.
    fn ask_version(&mut self) {
        let (session, event) = (&mut *self.session, self.event);
        if !session.versioned
            && event.capabilities.transcript_version
            && !event.transcript_path.is_empty()
        {
            session.versioned = true;
            self.outputs.push(Output::Version {
                transcript_path: event.transcript_path.clone(),
            });
        }
        if !self.replay && !session.versioned && event.capabilities.agent == agy::CAPABILITIES.agent
        {
            session.versioned = true;
            self.outputs.push(Output::AgyVersion);
        }
    }

    /// The session ended, and its subagents with it.
    fn session_end(&mut self) {
        let (session, at) = (&mut *self.session, self.at);
        session.unblock();
        session.blocks.clear();
        session.running.clear();
        if session.in_turn() {
            session.close_spans(at);
            if let Some(turn) = session.turns.last_mut().filter(|t| t.ended_at.is_none()) {
                turn.ended_at = Some(at);
            }
        }
        session.turn_ended = true;
        session.ended = true;
        session.clearing = self.event.has(|h| matches!(h, Happened::EndedByClear));
    }

    /// A resumed session keeps its id; a compaction keeps the turn it happens in.
    fn session_start(&mut self) {
        let session = &mut *self.session;
        session.ended = false;
        session.clearing = false;
        session.turn_ended |= self.event.started_from() != Some(Start::Compact);
    }

    /// A tool call starts: its ledger row, its step, a question it asks, and a block that waited
    /// for it.
    fn tool_start(&mut self, tool: &Tool) {
        let (session, event, at) = (&mut *self.session, self.event, self.at);
        let turn = session.ledger_turn(event.agent_id.as_deref());
        let turn = session.ledger.start(event, tool, turn, self.called.clone());
        if let Some(turn) = turn.filter(|_| tool.kind == ToolKind::Command) {
            let list = self.check_lists.of(&session.cwd);
            if tool
                .command
                .as_deref()
                .is_some_and(|c| matched(list, c).is_some())
            {
                session.recheck(turn, list, self.roots, self.durations);
            }
        }
        if self.own {
            session.returned = None;
            let step = Step::new(&tool.name, &tool.input, &event.cwd);
            session.agent.step(step, event.call_id(), at);
            session
                .work
                .call(tool, at, self.check_lists.of(&session.cwd));
        }
        let call = Call {
            id: self.id.clone(),
            tool: tool.name.clone(),
            kind: tool.kind.clone(),
            input: tool.input.clone(),
            agent: event.agent_id.clone(),
        };
        if tool.kind == ToolKind::Question {
            session.questions.push(Question {
                tool_use_id: event.call_id().into(),
                agent: call.agent.clone(),
                questions: call.input["questions"].clone(),
                answer: Answer::Open,
                turn: session.current_turn(),
            });
        }
        // The hooks are async, so a block on this call may have landed first.
        // shortcut: a block whose call ended before the block landed stays until the turn
        // ends; remember ended calls if auto-denied calls (danielrispler/Falinks#322) show it.
        if let Some(id) = &call.id {
            let unbound = session.blocks.iter_mut();
            let unbound = unbound.filter(|w| w.calls.is_empty() && w.takes(&call));
            unbound.for_each(|w| w.calls.push((id.clone(), call.agent.clone())));
        }
        let asks = tool.asks();
        let (tool, input, agent) = (call.tool.clone(), call.input.clone(), call.agent.clone());
        session.running.push(call);
        if asks {
            session.block(Cause::Call, tool, input, agent, at);
        }
    }

    /// A permission notice echoes a block, of the agent it names if any, that has not had its
    /// notice yet. A sandboxed command's network request, and a subagent's prompt, fire only the
    /// notice.
    fn permission_notice(&mut self) {
        let session = &mut *self.session;
        let agent = &self.event.agent_id;
        let mut blocks = session.blocks.iter_mut();
        let echoed = |w: &&mut Block| {
            !w.noticed && w.cause != Cause::Notice && (agent.is_none() || w.agent == *agent)
        };
        match blocks.find(echoed) {
            Some(block) => block.noticed = true,
            None => session.block(
                Cause::Notice,
                String::new(),
                Value::Null,
                agent.clone(),
                self.at,
            ),
        }
    }

    /// Daniel decided a plan or a permission from the plotkeeper card (danielrispler/Falinks#478): it
    /// waits on him no more, though its result comes only when the call ends, or for a denial maybe
    /// never. A question's result carries its answer, so its block waits for it.
    fn decided_on_card(&mut self, tool: &Tool, call_id: &str, answer: &Value) {
        let session = &mut *self.session;
        let input = &tool.input;
        let agent = &self.event.agent_id;
        let decided = |w: &Block| w.tool == tool.name && w.input == *input && w.agent == *agent;
        if let Some(block) = session.blocks.iter().find(|w| decided(w)) {
            let call = block.calls.first().map(|(call, _)| call.clone());
            session.decisions.push(Decision {
                call: call.unwrap_or(call_id.to_owned()),
                agent: block.agent.clone(),
                tool: tool.name.clone(),
                input: block.input.clone(),
                answer: answer["decision"].clone(),
                at: self.at,
                source: Source::Card,
            });
        }
        session.blocks.retain(|w| !decided(w));
    }

    /// A tool call ends: its ledger row, a decision made in the terminal, its question's answer,
    /// its step and its check's duration.
    fn tool_end(&mut self, tool: &Tool, failed: bool) {
        let (session, event) = (&mut *self.session, self.event);
        let turn = session.ledger_turn(event.agent_id.as_deref());
        // The turn its call began in, which a new prompt since may have left.
        let turn = session
            .ledger
            .end(event, tool, turn, failed, self.called.clone());
        if let Some(turn) = turn.filter(|_| tool.kind == ToolKind::Command) {
            let list = self.check_lists.of(&session.cwd);
            if tool
                .command
                .as_deref()
                .is_some_and(|c| matched(list, c).is_some())
            {
                session.recheck(turn, list, self.roots, self.durations);
            }
        }
        session.running.retain(|c| c.id != self.id);
        self.decided_in_terminal(tool, failed);
        self.question_answered(tool, failed);
        // A denied or interrupted call, or a failed Edit or Write, changed nothing: only a
        // command that ran and failed has its exit code in its error.
        let ran = event.ran();
        let session = &mut *self.session;
        if self.own {
            session.agent.step_done(event.call_id(), self.at);
        }
        // A check's time counts toward its typical duration when it ran and did not fail.
        let list = self.check_lists.of(&session.cwd);
        let row = session
            .ledger
            .row(event.agent_id.as_deref(), event.call_id());
        let took = row.and_then(|r| r.began().zip(r.finished()));
        let counted = self.own && ran && !failed && row.is_some_and(|r| !r.background);
        if let (Some((began, ended)), true) = (took, counted) {
            let command = tool.command.as_deref().unwrap_or_default();
            self.durations
                .record(list, command, ended.saturating_sub(began));
        }
    }

    /// A plan or a permission that still waited on Daniel was decided in the terminal: allowed if
    /// its call went on, denied if Daniel rejected it. Its block resolves either way.
    fn decided_in_terminal(&mut self, tool: &Tool, failed: bool) {
        let (session, event, id) = (&mut *self.session, self.event, &self.id);
        // shortcut: a call that failed for another reason, such as an Edit whose text was
        // not found, records no decision; read its error if that loses real ones.
        let held = session.blocks.iter().find(|w| {
            w.tool == tool.name && w.calls.iter().any(|(call, _)| Some(call) == id.as_ref())
        });
        let answer = match failed {
            false => Some("allow"),
            true if event.rejected() => Some("deny"),
            true => None,
        };
        if let (Some(block), Some(answer), Some(call)) = (held, answer, id)
            && tool.kind != ToolKind::Question
        {
            session.decisions.push(Decision {
                call: call.clone(),
                agent: block.agent.clone(),
                tool: tool.name.clone(),
                input: block.input.clone(),
                answer: answer.into(),
                at: self.at,
                source: Source::Terminal,
            });
        }
        session.resolve(|(call, _)| Some(call) == id.as_ref());
    }

    /// The answer to the call's question. The hooks are async, so an answer that lands after its
    /// turn's `Stop` still counts.
    fn question_answered(&mut self, tool: &Tool, failed: bool) {
        let (session, event) = (&mut *self.session, self.event);
        let input = &tool.input;
        let mut asked = session.questions.iter_mut();
        if !failed && let Some(question) = asked.find(|q| q.tool_use_id == event.call_id()) {
            // Only an object of strings is kept, so the pane can read it (danielrispler/Falinks#434, decision 42).
            let null = Value::Null;
            let answers = event.answers().unwrap_or(&null);
            let strings = answers
                .as_object()
                .is_some_and(|a| a.values().all(Value::is_string));
            question.answer = if strings {
                Answer::Answered(answers.clone())
            } else {
                Answer::NotAnswered
            };
            if strings {
                let card = session.carded.remove(event.call_id());
                session.decisions.push(Decision {
                    call: event.call_id().into(),
                    agent: question.agent.clone(),
                    tool: tool.name.clone(),
                    input: input.clone(),
                    answer: answers.clone(),
                    at: self.at,
                    source: if card { Source::Card } else { Source::Terminal },
                });
            }
        }
    }

    /// The session's state after the event, and the notices it raises. Returns the outputs and
    /// whether a phase of its work started, which moves its estimate.
    fn settle(self, touches: Vec<String>, before: &Before) -> (Vec<Output>, bool) {
        let (session, event, mut outputs) = (self.session, self.event, self.outputs);
        // The worktrees its turn touched, each with its base from `looks` (danielrispler/Falinks#434, decision 9).
        if session.in_turn() {
            session.touched.extend(touches);
        }
        // An open question whose block cleared with no answer was not answered: its call failed, or
        // its turn or its agent ended first.
        let blocks = &session.blocks;
        // A blocked command whose block cleared was moved on from: Daniel handled it in his agent (#572).
        for card in &mut session.blocked_commands {
            let standing = blocks.iter().any(|w| w.denied.as_ref() == Some(&card.call));
            if card.status.waits() && !standing {
                card.status = BlockedStatus::MovedOn;
            }
        }
        for question in &mut session.questions {
            let mut calls = blocks.iter().flat_map(|w| &w.calls);
            let open = calls.any(|(id, _)| *id == question.tool_use_id);
            if question.answer == Answer::Open && !open {
                question.answer = Answer::NotAnswered;
            }
        }
        session.state = if session.ended {
            State::Ended
        } else if session.needs_you() {
            State::NeedsYou
        } else if session.turn_ended {
            State::YourTurn
        } else {
            State::Running
        };
        session.update_unseen();
        session.asks_before_closing = session.in_turn() || !session.ended && session.background;
        session.work.estimate.depends_on_you = session.state == State::NeedsYou;
        session.asker = session.blocks.first().map(|block| Asker {
            agent: block.asker(),
            since: block.at,
        });
        let phase_started = session.work.phases.len() != before.phases;
        // A blocked command's own notice stands for its needs-you, so one block shows one notification.
        let only_denied =
            !session.blocks.is_empty() && session.blocks.iter().all(|b| b.cause == Cause::Denied);
        if !self.replay && event.has(|h| matches!(h, Happened::CommandBlocked { .. })) {
            outputs.push(Output::CommandBlocked {
                session_id: event.session_id.clone(),
                agent: event.agent_id.clone(),
            });
        }
        if !before.needs_you && session.state == State::NeedsYou && !only_denied {
            outputs.push(Output::NeedsYou {
                session_id: event.session_id.clone(),
                agent: session.asker.as_ref().and_then(|w| w.agent.clone()),
            });
        }
        // A fresh session also starts at your turn, from its default `Running`, but has nothing to tell.
        let reached = matches!(before.state, State::Running | State::NeedsYou);
        if reached && session.state == State::YourTurn && !event.session_starts() {
            outputs.push(Output::YourTurn {
                session_id: event.session_id.clone(),
            });
        }
        (outputs, phase_started)
    }
}
