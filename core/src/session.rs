//! One session, its turns, subagents, blocks and cards.

use crate::{
    Asker, Capabilities, CardRequest, Change, Check, CheckStatus, Facts, Ledger, Listed, Output,
    SummaryRequest, Transcript, TurnFiles, Work, checks, claude,
    event::{self, Event, Happened, ToolKind},
    output::{Call, step_detail},
    studio::Roots,
    transcript,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Its own agent: the session id, last step, first and latest events, and worktree since.
    #[serde(flatten)]
    pub agent: Agent,
    /// The folder the session works in, from its latest event.
    pub cwd: String,
    /// The last segment of `cwd`, or the whole `cwd` when that segment is empty.
    pub folder_name: String,
    /// Its project folder (spec #659): the root of the first folder it got, set once and never changed by later
    /// events. A saved session without it takes it from its saved `cwd` (`Studio::resume`).
    #[serde(default)]
    pub project: String,
    /// The studio terminal it runs in, its `PLOTKEEPER_TERMINAL_ID` from its latest event, `None` when
    /// it runs outside the studio (danielrispler/Falinks#461, decision 8).
    pub terminal: Option<String>,
    /// Its agent's process, `studio_pid` of its latest event that has one; `None` when its hook
    /// could not tell, and then it never ends by a dead process (#330).
    #[serde(default)]
    pub pid: Option<u32>,
    /// Daniel's name for it, from the studio's latest rename line in the log (danielrispler/Falinks#469); `None` for its
    /// folder's.
    pub name: Option<String>,
    /// A few words on what it is about, written from Daniel's first prompt and logged as a
    /// `StudioPromptName` line; shown after his name and before the identity's.
    pub prompt_name: Option<String>,
    /// Every tool call of it and its workers (danielrispler/plotkeeper#110).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub ledger: Ledger,
    /// Whether Daniel's first prompt arrived, which asks for the prompt name once.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) name_asked: bool,
    /// His first prompt, a bare slash command, whose name waits for its turn's end.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) name_after: Option<String>,
    /// The prompt id of the turn whose prompt asked for the name: when that turn ends without its
    /// `Stop` and with no call, Daniel cancelled it, and his next prompt asks again.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) naming_turn: Option<String>,
    /// The id `claude --resume` takes: its latest conversation's, which a `/clear` changes while the
    /// row keeps its first id (danielrispler/Falinks#461, decision 18).
    pub resume_id: String,
    /// Claude Code's permission mode, from its latest event that names one (danielrispler/Falinks#541): `default`,
    /// `auto`, `plan` and the like; `None` until one does, as `SessionStart` does not.
    pub permission_mode: Option<String>,
    /// The permission mode it started in, from its first event that names one, which a resume keeps
    /// (danielrispler/Falinks#541); `None` until one does.
    pub started_mode: Option<String>,
    /// When each `/clear` continued it, oldest first: the "cleared" markers in its history.
    pub cleared: Vec<u64>,
    /// Its latest `SessionEnd` was a `/clear`, so the next `SessionStart` from `clear` on its
    /// terminal, or in its folder, continues it.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) clearing: bool,
    pub state: State,
    /// Whether closing its terminal asks first: it is in a turn, or its latest `Stop` listed
    /// background tasks (danielrispler/Falinks#469).
    pub asks_before_closing: bool,
    /// Its task has landed; the shell sets it from `Studio::landed` as it sends the list.
    pub landed: bool,
    /// Daniel deleted it: the list leaves it out until it starts again.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) deleted: bool,
    /// Its latest `Stop` listed background tasks.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) background: bool,
    /// The tool calls whose `PreToolUse` landed and whose `PostToolUse` has not.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) running: Vec<Call>,
    /// What blocks it on Daniel, so that it needs you, oldest first (danielrispler/Falinks#434, decision 24).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) blocks: Vec<Block>,
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) turn_ended: bool,
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) ended: bool,
    /// Whether a `Stop` of it requested Claude Code's version.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) versioned: bool,
    /// Its subagents at any depth, in the order they were first seen (danielrispler/Falinks#369, decision 5).
    #[serde(serialize_with = "serialize_agents")]
    pub agents: Vec<Subagent>,
    /// The slash command Daniel typed whose forked child is not yet seen, by its prompt id.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) typed_skill: Option<(String, String)>,
    /// Who asks Daniel, and since when, while it needs him.
    pub asker: Option<Asker>,
    /// Its prompts, oldest first, each marked with who sent it (danielrispler/Falinks#369, decision 7).
    pub prompts: Vec<Prompt>,
    /// Its turns, oldest first: the rail's feed (danielrispler/Falinks#574). Its checks, subagents and questions name the
    /// turn they started in.
    pub turns: Vec<Turn>,
    /// At your turn, or idle since, with a turn Daniel has not seen (danielrispler/Falinks#556): its row's filled dot and
    /// bold name.
    pub unseen: bool,
    /// How many of its turns Daniel has seen, which the card store keeps (danielrispler/Falinks#556). The rail's
    /// "Where we were" banner counts what happened after them (danielrispler/plotkeeper#124).
    #[serde(default)]
    pub(crate) seen: usize,
    /// How many it had seen when Daniel selected it: the point "Since you looked" counts from, which the poll's looking
    /// at the rail does not move (danielrispler/plotkeeper#124).
    #[serde(default)]
    pub(crate) seen_at_selection: usize,
    /// What happened after `seen_at_selection`, kept up to date by `update_unseen` for the banner.
    #[serde(default)]
    pub(crate) since_looked: SinceSeen,
    /// The ids of its background tasks, commands included, from their responses and `Stop`'s
    /// `background_tasks`. With its subagents' ids, they are the task ids a wake-up can name.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) tasks: BTreeSet<String>,
    /// The questions its agents asked, oldest first (danielrispler/Falinks#369, decision 21).
    pub questions: Vec<Question>,
    /// Its answered questions, plans and permissions, oldest first: its decision history (danielrispler/Falinks#461,
    /// decision 17). The card store keeps them across restarts and gives them to the pane.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub decisions: Vec<Decision>,
    /// The Bash commands auto mode blocked, in block order (danielrispler/plotkeeper#170).
    #[serde(default)]
    pub blocked_commands: Vec<BlockedCommand>,
    /// The questions answered on the plotkeeper card, by call, whose result has not landed yet.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) carded: BTreeSet<String>,
    /// When its latest turn started and ended (danielrispler/Falinks#461, decision 13).
    pub turn_span: Option<Span>,
    /// Its latest `Stop`, until its next prompt (danielrispler/Falinks#369, decision 19).
    pub stopped: Option<Stopped>,
    /// When its latest turn ended, which took its workers' cards since its last card.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) stopped_at: u64,
    /// The reply its running turn's agent sent in an event of its own before the turn's end, as Cursor's
    /// `afterAgentResponse` does (#362); its turn end takes it when that carries no reply.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) reply: String,
    /// Whether its turn was opened by a wake-up and has had no prompt of Daniel's since.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) woken: bool,
    /// Claude Code's prompt id of its latest turn (danielrispler/Falinks#434, decision 3), `None` until an event of its
    /// own agent carries one.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) turn: Option<String>,
    /// The prompts that arrived during its turn, queued or typed after an interrupt: the next turn's
    /// (danielrispler/Falinks#434, decision 4).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) queued: Vec<Prompt>,
    /// The worktree roots its latest turn touched, each with its base from the first touch (danielrispler/Falinks#434,
    /// decision 9).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) touched: BTreeSet<String>,
    /// The tree the hook recorded at its latest turn's first touch of each worktree, by root (danielrispler/Falinks#486).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) trees: BTreeMap<String, String>,
    /// "worker <name> returned", from its latest wake-up until its own next step or Daniel's next
    /// prompt (danielrispler/Falinks#410, decision 27).
    pub returned: Option<String>,
    /// What its transcript says of its latest turn: the intention, the agent's texts, or why it
    /// cannot be read (danielrispler/Falinks#461, decision 12).
    pub transcript: Transcript,
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) transcript_file: transcript::TranscriptFile,
    /// What its agent's adapter reports, which decides what its view shows (decision 4).
    pub capabilities: Capabilities,
    /// The model it runs at, where its agent's hooks say (agy's `modelName`); Claude's is in
    /// `transcript.model`.
    pub model: Option<String>,
    /// Its issue, PR and worktree, which name it unless Daniel renamed it (danielrispler/Falinks#461, decision 19).
    pub identity: Identity,
    /// Its work on its ticket: its kind, its phases and how long its agent worked (danielrispler/Falinks#461, decision
    /// 14).
    pub work: Work,
    /// A row of another agent than Claude Code, from its terminal's OSC 7501 status only (danielrispler/Falinks#560): it
    /// has no intention, card, brief, estimate or decisions. Its id is its terminal's.
    pub status_only: bool,
    /// A status-only row of a plain terminal that New terminal started, while no agent in it reports
    /// a status or links a session: it has no state of its own. Its id is its terminal's.
    pub shell: bool,
    /// Its terminal's OSC 7501 state is `blocked`, for an agent that reports no permission or question of its own
    /// (agy, decision 5): it needs Daniel, with no text.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) terminal_blocked: bool,
    /// A status-only row's terminal `turn` at the last list (danielrispler/Falinks#560).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) reported_turn: u64,
}

/// What names a session (danielrispler/Falinks#461, decision 19): the issue of its `issue-<n>-…` branch, else the one
/// it claimed with `wayfinder-startup --claim <n>`, the PR the shell finds for its branch, and its
/// worktree's folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Identity {
    pub issue: Option<u64>,
    /// Its folder's branch, from its transcript.
    pub branch: Option<String>,
    pub pr: Option<u64>,
    /// The last segment of its folder's worktree root.
    pub worktree: String,
    /// The derived name: "danielrispler/Falinks#480 · PR danielrispler/Falinks#490 · agent-a1b2", "fix-typo · agent-a1b2" or "main · Falinks".
    pub name: String,
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) claim: Option<u64>,
}

impl Identity {
    pub(crate) fn named(&mut self) {
        let branch = self.branch.as_deref();
        let from_branch = branch
            .and_then(|b| b.strip_prefix("issue-"))
            .and_then(|rest| {
                let digits = rest.split(|c: char| !c.is_ascii_digit()).next()?;
                digits.parse().ok()
            });
        self.issue = from_branch.or(self.claim);
        let shown = branch.filter(|_| from_branch.is_none());
        let parts = [
            self.issue.map(|n| format!("#{n}")),
            shown.map(String::from),
            self.pr.map(|n| format!("PR #{n}")),
            // A task's worktree is often named after its branch.
            Some(self.worktree.clone()).filter(|w| !w.is_empty() && Some(w.as_str()) != shown),
        ];
        self.name = parts.into_iter().flatten().collect::<Vec<_>>().join(" · ");
    }
}

/// The last segment of `cwd`, a list row's folder name; `cwd` itself when it has none.
pub(crate) fn folder_name(cwd: &str) -> String {
    let last = cwd.rsplit('/').next().unwrap_or_default();
    if last.is_empty() { cwd } else { last }.into()
}

/// The project a folder whose root is `root` belongs to (spec #659): a task's worktree
/// `<main>/.claude/worktrees/<task>` belongs to `<main>`, any other root to itself.
pub(crate) fn project_of(root: &str) -> &str {
    const MARK: &str = "/.claude/worktrees/";
    match root.rfind(MARK) {
        Some(at) if at > 0 && root.len() > at + MARK.len() => &root[..at],
        _ => root,
    }
}

/// The worktree folder of the task that `cwd` is in: the path through the folder under
/// `.claude/worktrees/`; `None` outside one, as in a main checkout.
pub(crate) fn task_path(cwd: &str) -> Option<String> {
    const MARK: &str = "/.claude/worktrees/";
    let at = cwd.find(MARK)? + MARK.len();
    let name = cwd[at..].split('/').next().filter(|n| !n.is_empty())?;
    Some(format!("{}{name}", &cwd[..at]))
}

/// The issue a `wayfinder-startup … --claim <n>` command claims.
pub(crate) fn claimed(command: &str) -> Option<u64> {
    if !command.contains("wayfinder-startup") {
        return None;
    }
    let mut words = command.split_whitespace();
    words.find(|w| *w == "--claim")?;
    words.next()?.parse().ok()
}

/// A terminal of the daemon, as its `list` gives it, or as its record of the terminals it ran gives
/// them after it died (`archive/terminal-daemon/src/lib.rs`). Fields the record lacks are empty.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Terminal {
    pub id: String,
    /// What it was spawned with; empty for a login shell.
    #[serde(default)]
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: String,
    /// Its leader's pid, which leads its session.
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub exited: Option<String>,
    /// The program in its foreground.
    #[serde(default)]
    pub foreground: Option<String>,
    #[serde(default)]
    pub processes: Vec<Process>,
    /// Its program's OSC 7501 root status (danielrispler/Falinks#555): `working`, `blocked`, `done` and the like; `None`
    /// until it reports and after `clear` or its exit.
    #[serde(default)]
    pub state: Option<String>,
    /// The program's name for itself in its reports, such as `codex` or `claude-code`.
    #[serde(default)]
    pub app: Option<String>,
    /// When the state last changed, in Unix milliseconds.
    #[serde(default)]
    pub status_ms: Option<u64>,
    /// How often its program left `working`: a turn ended each time it went up (danielrispler/Falinks#555).
    #[serde(default)]
    pub turn: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Process {
    pub pid: u32,
    pub command: String,
}

/// How a terminal comes back after its daemon is gone, and what a restart now would end.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Revive {
    pub id: String,
    pub cwd: String,
    /// What it reruns: its spawn's argv, or an agent's resume command, empty for a shell.
    pub argv: Vec<String>,
    /// The session to reopen with `claude --resume`, for a Claude terminal whose session is known.
    pub session: Option<String>,
    /// What runs in it that a restart would end: a program, "a turn" or "background tasks".
    pub busy: Option<String>,
    /// Whether it comes back: not a Claude terminal no session links to (#17). Its `claude` never
    /// sent an event, so no row shows it and it has no conversation to resume.
    pub revives: bool,
}

/// What happened in a worktree since the last card on it: a session's own, or a subagent's that
/// works in a folder of its own (danielrispler/Falinks#369, decisions 10 and 11).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Since {
    /// When that card, or else the first base, was taken.
    pub(crate) at: u64,
    /// The files that other subagents edited or wrote there, relative to its folder, by subagent.
    pub(crate) by: BTreeMap<String, BTreeSet<String>>,
    /// A session's only: its workers' cards since, at any depth.
    pub(crate) cards: Vec<String>,
}

/// 20 minutes, a starting guess (danielrispler/Falinks#369, decision 11 and Further Notes).
pub(crate) const FALLBACK_MS: u64 = 20 * 60 * 1000;

/// A prompt of a session (danielrispler/Falinks#369, decision 7), received at `at`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub at: u64,
    pub sender: Sender,
    /// The turn it is in, which a wake-up opened (danielrispler/plotkeeper#212); `None` in a log read before.
    #[serde(default)]
    pub turn: Option<usize>,
    /// What it says, which the brief's rewrite reads (danielrispler/Falinks#369, decision 16); the pane never shows it.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub text: String,
}

/// A session's `Stop`, which its row tells about while its turn is over (danielrispler/Falinks#369, decision 19).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stopped {
    /// The id a card on it has (`CardRequest::id`).
    pub card: String,
    /// Whether it requested a card (`Output::Card`). Only a live `Stop` does.
    pub requested: bool,
    /// The first line of the agent's last message.
    pub message: Option<String>,
}

/// A turn of a session, a block in the rail's feed (danielrispler/Falinks#574): from what opened it until it ended.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub started_at: u64,
    pub ended_at: Option<u64>,
    /// The first line of the prompt that opened it, or "worker <name> returned" for a wake-up; empty
    /// until a prompt arrives, as a queued prompt's turn may start on a step.
    pub prompt: String,
    /// The id its `Stop` gives a card or a summary on it (`Stopped::card`); `None` until it stopped.
    pub id: Option<String>,
    /// The first line of its final reply, from its `Stop`.
    pub message: Option<String>,
    /// Its whole final reply, which the rail fetches when its block opens (`Studio::turn_reply`).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub reply: String,
    /// The files its agent and workers changed, `None` until the shell has diffed its trees, and
    /// for an agent that reports no trees (danielrispler/plotkeeper#111).
    pub files: Option<TurnFiles>,
    /// The tree the hook recorded of each worktree it touched, at its first touch and at its end,
    /// by root: the start, and the end once it came.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) trees: BTreeMap<String, (String, Option<String>)>,
    /// When the shell was last asked for its files, and whether that was its final ask.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) files_asked: Option<(u64, bool)>,
    /// How its checks ended, its workers' included: each suite's last run (danielrispler/plotkeeper#112).
    pub checks: Vec<Check>,
    /// The checks the feed lists under it: of its own agent, each one's latest run while it runs or
    /// after it failed, newest first.
    #[serde(default)]
    pub listed: Vec<Listed>,
}

/// Who sent a prompt: Daniel, or Claude Code waking the session when one of its tasks returns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sender {
    Daniel,
    /// A wake-up, with the id of the task it names.
    WakeUp(String),
    /// A known task id in markup the studio does not know: drift, counted as Daniel's.
    Unrecognised(String),
}

impl Turn {
    /// A turn asked for its files for the last time is asked again once new trees `landed`.
    fn ask_again_if_last(&mut self, landed: bool) {
        if landed && self.files_asked.is_some_and(|(_, last)| last) {
            self.files_asked = None;
        }
    }
}

impl Sender {
    /// Whether the prompt counts as Daniel's: anything but a wake-up, drift included.
    pub fn is_daniels(&self) -> bool {
        !matches!(self, Sender::WakeUp(_))
    }
}

/// An `AskUserQuestion` call, which Daniel answers in the terminal (danielrispler/Falinks#369, decision 21).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Question {
    /// The call's id, which the answer hook's request names (danielrispler/Falinks#461, decision 16).
    pub(crate) tool_use_id: String,
    /// The subagent that asks, `None` for the session's own agent.
    pub agent: Option<String>,
    /// Its questions and their options, as the tool input gives them.
    pub questions: Value,
    pub answer: Answer,
    /// The turn it was asked in (`Session::turns`).
    pub turn: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Asked, with no result yet.
    Open,
    /// Daniel's choice for each question, keyed by its text: the `PostToolUse`'s `answers`, kept only
    /// when it is an object of strings (danielrispler/Falinks#434, decision 42).
    Answered(Value),
    /// Its call failed or was interrupted, or its turn or its agent ended first.
    NotAnswered,
}

/// An answered question, plan or permission in its session's decision history (danielrispler/Falinks#461, decision 17).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// The call it decided, which keys it in the store.
    pub call: String,
    /// The subagent that asked, `None` for the session's own agent.
    pub agent: Option<String>,
    /// `AskUserQuestion`, `ExitPlanMode`, or the tool a permission was asked for.
    pub tool: String,
    /// What it asked: the call's input, with a question's questions and options or a plan's text.
    pub input: Value,
    /// A question's answers by question text; `"allow"` or `"deny"` for a plan or a permission.
    pub answer: Value,
    /// When it was answered, in Unix milliseconds.
    pub at: u64,
    pub source: Source,
}

/// A Bash command auto mode blocked, as a card on its session's view (danielrispler/plotkeeper#170).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockedCommand {
    /// The denied call's `tool_use_id`.
    pub call: String,
    /// The command in full, and the folder it was asked to run in.
    pub command: String,
    pub cwd: String,
    /// The rule that blocked it.
    pub reason: String,
    /// The subagent that asked, `None` for the session's own agent.
    pub agent: Option<String>,
    /// When it was blocked, in Unix milliseconds: its latest block, which a block again renews.
    pub at: u64,
    pub status: BlockedStatus,
    /// The retry's `tool_use_id` once it runs, which `check_output` reads the tail by, and when it started.
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub run_at: Option<u64>,
    /// The agent whose call the retry is, which its tail is read under: the session's own agent may
    /// run a subagent's command (`None`).
    #[serde(default)]
    pub run_by: Option<String>,
    /// Its exit code once Finished; `None` when it is unknown, as after a background run.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// The background task a backgrounded retry started, until its wake-up.
    #[serde(default)]
    pub task: Option<String>,
}

impl BlockedCommand {
    /// Whether `agent`'s call of `tool` in `cwd` is this card's retry: the same command and folder, by
    /// the agent that was blocked or by the session's own agent, which a subagent's card asks to run it.
    pub(crate) fn retried_by(&self, tool: &event::Tool, cwd: &str, agent: Option<&str>) -> bool {
        Some(self.command.as_str()) == tool.command.as_deref()
            && self.cwd == cwd
            && (agent.is_none() || self.agent.as_deref() == agent)
    }
}

/// Where a blocked command stands. Since the card went (#572) a block only waits until the session moves on; the
/// statuses of Daniel's Allow and its retry come only from a log from before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockedStatus {
    /// Blocked, and Daniel has not chosen.
    Blocked,
    /// Daniel allowed it from the card.
    Allowed,
    /// Its retry runs, from the matching `PreToolUse`.
    Running,
    /// Its retry ended.
    Finished,
    /// Its retry runs in the background, until the wake-up that names its task.
    RunningInBackground,
    /// A new `PermissionDenied` of the same command followed the Allow.
    BlockedAgain,
    /// Its turn ended with no matching call: Daniel's Allow was not used, and "Ask again" retries it.
    AllowedNotRun,
    /// Daniel declined it from the card, which closes it.
    Declined,
    /// The session moved on with no choice made here: Daniel handled it in his agent (#572).
    MovedOn,
}

impl BlockedStatus {
    /// Whether a blocked command in this status waits on Daniel: blocked, also again, until the session moves on.
    pub fn waits(self) -> bool {
        matches!(self, Self::Blocked | Self::BlockedAgain)
    }
}

/// Where Daniel answered: the plotkeeper card, through the answer hook, or the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Card,
    Terminal,
}

/// A subagent of a session (danielrispler/Falinks#369, decisions 5, 6 and 8).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Subagent {
    /// Its `agent_id`, last step, first and latest events, and worktree since.
    #[serde(flatten)]
    pub agent: Agent,
    pub parent: Parent,
    pub kind: Kind,
    /// Its name: its spawn's description, else the first 5 characters of its id. Its type, such as
    /// `Explore`.
    pub name: String,
    pub agent_type: Option<String>,
    /// `running`, then how it ended: its wake-up's `<status>`, such as `completed` or `killed`, or
    /// `completed` when only its `SubagentStop`, its spawn's response or its session's `Stop` says it
    /// ended.
    pub status: Status,
    /// When its end was received, in Unix milliseconds.
    pub ended_at: Option<u64>,
    /// Each time its end was received, which a step of its own after the end does not take back
    /// (danielrispler/plotkeeper#212).
    #[serde(default)]
    pub ends: Vec<u64>,
    /// Each time a step of its own after an end ran it again (`SendMessage`), with the turn it ran in.
    #[serde(default)]
    pub resumes: Vec<(u64, Option<usize>)>,
    /// The first line of its last message, from its `SubagentStop`.
    pub result: Option<String>,
    /// Its whole last message, which its summary reads (danielrispler/Falinks#535); the pane never shows it.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub message: String,
    /// Daniel opened its line after it failed (danielrispler/Falinks#434, decision 39).
    pub opened: bool,
    /// The turn of its session it was first seen in (`Session::turns`).
    pub turn: Option<usize>,
    /// The folder of its first event that has one: its worktree, or its parent's folder.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub folder: Option<String>,
    /// Its folder's worktree root, and the owner key of the cards on its changes there, decided at
    /// its first folder and never again (danielrispler/Falinks#434, decision 11).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) root: String,
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) owner: String,
    /// What its spawn asked it to do, which its brief's rewrite reads; the pane never shows it.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub prompt: String,
    /// Its spawn's call, by `tool_use_id`, and the agent that made it, `None` for the session's own.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    spawn: Option<(String, Option<String>)>,
    /// The prompt id of its first event.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    first_prompt: Option<String>,
}

/// The state a session's own agent and each subagent share (danielrispler/Falinks#410, decision 22). Its id is the owner
/// key of its card stream.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub last_step: Option<Step>,
    /// When the hook received its first and its latest event (`studio_received_ms`), in Unix
    /// milliseconds.
    pub started_at: u64,
    pub last_event_at: u64,
    /// When its last step started, and ended at its result or its turn's end (danielrispler/Falinks#461, decision 13).
    pub step_span: Option<Span>,
    /// The last step's `tool_use_id`, whose result ends it.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    step_call: String,
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) since: Since,
}

/// When something started and ended, in Unix milliseconds as the hook received them; `end` is `None`
/// while it runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: u64,
    pub end: Option<u64>,
}

impl Agent {
    pub(crate) fn new(id: &str, at: u64) -> Self {
        Agent {
            id: id.into(),
            started_at: at,
            last_event_at: at,
            ..Agent::default()
        }
    }

    /// Its step of call `call` starts at `at`.
    pub(crate) fn step(&mut self, step: Step, call: &str, at: u64) {
        self.last_step = Some(step);
        self.step_call = call.into();
        self.step_span = Some(Span {
            start: at,
            end: None,
        });
    }

    /// Call `call`'s result came at `at`: it ends the step when it is the last step's.
    // shortcut: the hooks are async, so a result that lands before its step's start leaves that step
    // running until the next one or the turn's end; remember ended calls if that shows.
    pub(crate) fn step_done(&mut self, call: &str, at: u64) {
        let span = self.step_span.as_mut().filter(|_| self.step_call == call);
        if let Some(span) = span.filter(|s| s.end.is_none()) {
            span.end = Some(at);
        }
    }
}

impl Subagent {
    /// Ends it at `at`, with its wake-up's `status`, or with none from any other signal. A wake-up's
    /// status overrides the others.
    fn end(&mut self, status: Option<&str>, at: u64) {
        if let Some(status) = status {
            self.status = Status::from(status);
        } else if self.status == Status::Running {
            self.status = Status::Completed;
        }
        if self.ended_at.is_none() {
            self.ends.push(at);
        }
        self.ended_at.get_or_insert(at);
    }

    /// It ended with a status other than `completed`, such as `killed`.
    pub fn failed(&self) -> bool {
        matches!(self.status, Status::Ended(_))
    }

    /// A subagent that succeeded, or one that failed and Daniel opened, folds into its parent's line
    /// (danielrispler/Falinks#369, story 18), a worker as a helper does (danielrispler/Falinks#535). A failed child stays visible until he
    /// opens it.
    pub fn faded(&self) -> bool {
        self.status == Status::Completed || self.failed() && self.opened
    }
}

/// A subagent's status, serialized as its text: `running`, `completed`, or its wake-up's other
/// `<status>`, such as `killed`.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Running,
    Completed,
    Ended(String),
}

impl From<&str> for Status {
    fn from(text: &str) -> Self {
        match text {
            "running" => Status::Running,
            "completed" => Status::Completed,
            other => Status::Ended(other.into()),
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Status::Running => "running",
            Status::Completed => "completed",
            Status::Ended(text) => text,
        })
    }
}

impl<'de> Deserialize<'de> for Status {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Status::from(String::deserialize(deserializer)?.as_str()))
    }
}

impl Serialize for Status {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Each subagent with its computed `failed` and `faded`, and its prompt's first line, `asked`, which
/// the rail's history shows under its name (danielrispler/Falinks#535).
fn serialize_agents<S: serde::Serializer>(
    agents: &[Subagent],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Wire<'a> {
        #[serde(flatten)]
        agent: &'a Subagent,
        failed: bool,
        faded: bool,
        asked: Option<&'a str>,
    }
    let wire = agents.iter().map(|agent| Wire {
        agent,
        failed: agent.failed(),
        faded: agent.faded(),
        asked: agent.prompt.lines().map(str::trim).find(|l| !l.is_empty()),
    });
    serializer.collect_seq(wire)
}

/// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parent {
    Session,
    Agent(String),
    #[default]
    Unattached,
}

/// A worker is spawned with its own worktree, or changes files; any other subagent is a helper
/// (danielrispler/Falinks#369, decision 6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Worker,
    #[default]
    Helper,
}

/// What happened in a session since Daniel looked: its turns after the last he saw, the files they changed and how
/// their checks ended (danielrispler/plotkeeper#124).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinceSeen {
    pub turns: usize,
    /// Distinct files, by worktree and path.
    pub files: usize,
    pub passed: usize,
    pub failed: usize,
    pub ran: usize,
}

impl Session {
    /// How many of its turns Daniel has seen.
    pub fn seen(&self) -> usize {
        self.seen
    }

    /// What its turns after the last one Daniel saw before he selected it did.
    pub fn since(&self) -> SinceSeen {
        self.since_looked
    }

    fn count_since(&self) -> SinceSeen {
        let turns = &self.turns[self.seen_at_selection.min(self.turns.len())..];
        let files: BTreeSet<(&str, &str)> = turns
            .iter()
            .filter_map(|turn| turn.files.as_ref())
            .flat_map(|found| &found.files)
            .map(|file| (file.root.as_str(), file.path.as_str()))
            .collect();
        let count = |status| {
            turns
                .iter()
                .flat_map(|turn| &turn.checks)
                .filter(|check| check.status == status)
                .count()
        };
        SinceSeen {
            turns: turns.len(),
            files: files.len(),
            passed: count(CheckStatus::Passed),
            failed: count(CheckStatus::Failed),
            ran: count(CheckStatus::Ran),
        }
    }

    pub(crate) fn update_unseen(&mut self) {
        // Going idle keeps the mark: only Daniel seeing the session clears it (danielrispler/Falinks#556).
        self.unseen =
            matches!(self.state, State::YourTurn | State::Idle) && self.turns.len() > self.seen;
        self.since_looked = self.count_since();
    }

    /// Its subagent `id`, at any depth.
    pub fn subagent(&self, id: &str) -> Option<&Subagent> {
        self.agents.iter().find(|a| a.agent.id == id)
    }

    /// The name of subagent `id`: its own, or else the first 5 characters of `id` (danielrispler/Falinks#410, decision 26).
    pub fn agent_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.subagent(id).map_or(short(id, 5), |a| &a.name)
    }

    /// Subagent `id`, added in "unattached" when this is the first event that names it.
    fn agent(&mut self, id: &str, at: u64) -> &mut Subagent {
        let index = match self.agents.iter().position(|a| a.agent.id == id) {
            Some(index) => index,
            None => {
                let turn = self.current_turn();
                self.agents.push(Subagent {
                    agent: Agent::new(id, at),
                    name: short(id, 5).into(),
                    status: Status::Running,
                    turn,
                    ..Subagent::default()
                });
                self.agents.len() - 1
            }
        };
        &mut self.agents[index]
    }

    /// The agent that spawned subagent `a`, when that is another subagent: by its spawn's response,
    /// or else by the spawn call it was matched to.
    fn spawner<'a>(&self, a: &'a Subagent) -> Option<&'a str> {
        match &a.parent {
            Parent::Agent(parent) => Some(parent),
            Parent::Session => None,
            Parent::Unattached => a.spawn.as_ref()?.1.as_deref(),
        }
    }

    /// The running subagents that `event` ends (danielrispler/Falinks#434, decision 18), each with its status, `None`
    /// when the signal tells none, and when it ended.
    pub(crate) fn ends(&self, event: &Event) -> Vec<(String, Option<String>, u64)> {
        let at = event.studio_received_ms;
        let mut ends: Vec<(String, Option<String>, u64)> = Vec::new();
        let mut end = |id: &str, status: Option<&str>, at| {
            ends.push((id.into(), status.map(Into::into), at));
        };
        let spawned_by = |call: &str| {
            let agents = self.agents.iter();
            let call = call.to_string();
            agents.filter(move |a| a.spawn.as_ref().is_some_and(|(id, _)| *id == call))
        };
        for happened in &event.happened {
            match happened {
                Happened::SubagentEnd => event.agent_id.iter().for_each(|id| end(id, None, at)),
                // A wake-up names the task it reports on, and how it ended (danielrispler/Falinks#370 note).
                Happened::TasksReported(tasks) => {
                    tasks.iter().for_each(|(id, s)| end(id, Some(s), at));
                }
                // A foreground spawn's response comes at its end.
                Happened::SubagentSpawned(spawned) if spawned.finished => {
                    end(&spawned.child, None, at);
                }
                Happened::ToolEnd {
                    tool,
                    id,
                    failed: true,
                } if tool.kind == ToolKind::Spawn => {
                    let children = spawned_by(id);
                    children.for_each(|a| end(&a.agent.id, Some("interrupted"), at));
                }
                // The running tasks that a turn's end lists: a child of the session's own agent
                // that it no longer lists has ended, at its last event.
                // shortcut: only the session's own children, since whether it lists their children is
                // unseen; widen it if a recorded log shows it does. An unattached subagent with
                // no spawn call ends too (danielrispler/plotkeeper#67): an id that Claude Code
                // names in one stray event lists in no `Stop`. A step after its end runs it again.
                Happened::TurnEnd { .. } if event.agent_id.is_none() => {
                    if let Some(tasks) = event.background_tasks() {
                        let listed = |id: &str| tasks.iter().any(|task| task == id);
                        let children = self.agents.iter().filter(|a| {
                            a.parent == Parent::Session
                                || (a.parent == Parent::Unattached && a.spawn.is_none())
                        });
                        let gone = children.filter(|a| !listed(&a.agent.id));
                        gone.for_each(|a| end(&a.agent.id, None, a.agent.last_event_at));
                    }
                }
                Happened::SessionEnd => self.agents.iter().for_each(|a| end(&a.agent.id, None, at)),
                _ => {}
            }
        }
        // A turn that ends without its `Stop` ends the subagents it runs in the foreground: their
        // spawn calls are still open. A background spawn's call closes at its launch.
        if self.ends_turn_early(event) {
            let own = self
                .running
                .iter()
                .filter(|c| c.agent.is_none() && c.kind == ToolKind::Spawn);
            let calls = own.filter_map(|c| c.id.as_deref());
            let children = calls.flat_map(spawned_by);
            children.for_each(|a| end(&a.agent.id, Some("interrupted"), at));
        }
        // An agent that ends other than completed ends its running descendants too.
        let mut index = 0;
        while let Some((id, status, at)) = ends.get(index).cloned() {
            index += 1;
            let Some(status) = status.filter(|s| s != "completed") else {
                continue;
            };
            let children = self.agents.iter().filter(|a| self.spawner(a) == Some(&id));
            let children: Vec<String> = children.map(|a| a.agent.id.clone()).collect();
            for child in children {
                if !ends.iter().any(|(ended, ..)| *ended == child) {
                    ends.push((child, Some(status.clone()), at));
                }
            }
        }
        let running = |id: &str| self.subagent(id).is_some_and(|a| a.ended_at.is_none());
        ends.retain(|(id, ..)| running(id));
        ends
    }

    /// Whether `id` is an internal fork of a subagent (danielrispler/plotkeeper#37): an unknown id
    /// that matches no open spawn call, whose first event is in a worktree that another subagent
    /// owns. Claude Code runs it under an `agent_id` of its own, and it gets no subagent.
    fn is_fork(&self, id: &str, event: &Event, roots: &Roots) -> bool {
        let root = roots.of(&event.cwd);
        let bound = |call: &Option<String>| {
            let mut spawns = self.agents.iter().filter_map(|a| a.spawn.as_ref());
            spawns.any(|(id, _)| Some(id) == call.as_ref())
        };
        let mut open = self.running.iter().filter(|c| c.kind == ToolKind::Spawn);
        self.subagent(id).is_none()
            && !root.is_empty()
            && !open.any(|c| c.agent.as_deref() != Some(id) && !bound(&c.id))
            && self
                .agents
                .iter()
                .any(|a| a.root == root && a.owner == a.agent.id)
    }

    /// Updates the subagent tree with one event. A spawn's response names its child (`agentId`),
    /// and every child event names its agent (`agent_id`).
    #[allow(
        clippy::too_many_lines,
        reason = "left whole when the lint came in (#555); spec #548 names no split of it"
    )]
    pub(crate) fn track_agents(&mut self, event: &Event, worktree_changed: bool, roots: &Roots) {
        let at = event.studio_received_ms;
        for (id, status, at) in self.ends(event) {
            self.agent(&id, at).end(status.as_deref(), at);
        }
        if let Some(id) = event
            .agent_id
            .as_ref()
            .filter(|id| !self.is_fork(id, event, roots))
        {
            // A child's first event matches it to the oldest open spawn call with no child yet.
            // shortcut: no event names both a child and its spawn's call until the spawn's response,
            // so two spawns open at once may swap; match by a shared field if a recording shows one.
            if self.subagent(id).is_none() {
                let bound = |call: &Option<String>| {
                    let mut spawns = self.agents.iter().filter_map(|a| a.spawn.as_ref());
                    spawns.any(|(id, _)| Some(id) == call.as_ref())
                };
                let open = self.running.iter().filter(|c| c.kind == ToolKind::Spawn);
                let call = open
                    .filter(|c| c.agent.as_ref() != Some(id))
                    .find(|c| !bound(&c.id));
                let spawn = call.and_then(|c| Some((c.id.clone()?, c.agent.clone())));
                let agent = self.agent(id, at);
                agent.spawn = spawn;
                agent.first_prompt.clone_from(&event.prompt_id);
            }
            // Its owner, at its first folder: itself in a worktree other than its parent's, else its
            // parent's owner.
            let first = self.subagent(id).is_some_and(|a| a.folder.is_none());
            let root = roots.of(&event.cwd);
            let owner = match self.subagent(id).and_then(|a| self.spawner(a)) {
                Some(parent) => self
                    .subagent(parent)
                    .filter(|p| !p.owner.is_empty() && p.root == root)
                    .map(|p| p.owner.clone()),
                None => (roots.of(&self.cwd) == root).then(|| self.agent.id.clone()),
            };
            let home = self.new_home(id, event, roots);
            let turn = self.current_turn();
            let agent = self.agent(id, at);
            agent.agent.last_event_at = agent.agent.last_event_at.max(at);
            if agent.agent_type.is_none() {
                agent.agent_type.clone_from(&event.agent_type);
            }
            // Its first folder is its first base (`Studio::look`).
            if first && !event.cwd.is_empty() {
                agent.folder = Some(event.cwd.clone());
                agent.agent.since.at = at;
                agent.owner = owner.unwrap_or_else(|| id.clone());
                agent.root = root;
            }
            // The first other worktree it touches is its home from then on (danielrispler/plotkeeper#206).
            if let Some(home) = home {
                agent.folder = Some(home.clone());
                agent.root = home;
                agent.owner = id.clone();
                agent.agent.since.at = at;
            }
            // A changed file promotes a helper at that step: an `Edit` or `Write` that succeeded, or
            // a command that changed its worktree and either succeeded or ran and failed (danielrispler/Falinks#434,
            // decision 21).
            let kind = event.tool().map(|t| &t.kind);
            let changed = kind == Some(&ToolKind::Edit) && event.tool_succeeds()
                || kind == Some(&ToolKind::Command) && worktree_changed && event.ran();
            for happened in &event.happened {
                match happened {
                    // A step after its end means its parent continued it (`SendMessage`).
                    Happened::ToolStart { tool, id } => {
                        let step = Step::new(&tool.name, &tool.input, &event.cwd);
                        agent.agent.step(step, id, at);
                        agent.status = Status::Running;
                        if agent.ended_at.take().is_some() {
                            agent.resumes.push((at, turn));
                        }
                    }
                    Happened::ToolEnd { id, .. } => {
                        agent.agent.step_done(id, at);
                        if changed {
                            agent.kind = Kind::Worker;
                        }
                    }
                    Happened::SubagentEnd => {
                        let mut lines = event.reply().lines().map(str::trim);
                        agent.result = lines.find(|line| !line.is_empty()).map(Into::into);
                        agent.message = event.reply().into();
                        agent.end(None, at);
                    }
                    _ => {}
                }
            }
        }
        if let Some(task) = event.background_task() {
            self.tasks.insert(task.into());
        }
        self.tasks
            .extend(event.background_tasks().into_iter().flatten().cloned());
        for happened in &event.happened {
            match happened {
                Happened::SubagentSpawned(spawned) => {
                    let parent = event
                        .agent_id
                        .clone()
                        .map_or(Parent::Session, Parent::Agent);
                    let call = spawned.call.clone();
                    for agent in &mut self.agents {
                        if agent.spawn.as_ref().is_some_and(|(id, _)| *id == call) {
                            agent.spawn = None;
                        }
                    }
                    let agent = self.agent(&spawned.child, at);
                    agent.spawn = Some((call, event.agent_id.clone()));
                    agent.parent = parent;
                    if let Some(name) = &spawned.name {
                        agent.name.clone_from(name);
                    }
                    agent.prompt.clone_from(&spawned.prompt);
                    if agent.agent_type.is_none() {
                        agent.agent_type.clone_from(&spawned.agent_type);
                    }
                    if spawned.worktree {
                        agent.kind = Kind::Worker;
                    }
                    // A foreground spawn's response comes at its end.
                    if spawned.finished {
                        agent.end(None, at);
                    }
                }
                // A wake-up's status overrides how another signal ended its task.
                Happened::TasksReported(tasks) => {
                    for (id, status) in tasks {
                        if let Some(agent) = self.agents.iter_mut().find(|a| a.agent.id == *id) {
                            agent.end(Some(status), at);
                        }
                    }
                }
                Happened::SkillTyped(command) => {
                    self.typed_skill = event.prompt_id.clone().zip(Some(command.clone()));
                }
                _ => {}
            }
        }
        // A skill Daniel typed forks its child with no call that names it: its first child of
        // the same prompt id with no spawn call is the fork, whichever of the two the hooks
        // deliver first. A stray id with no type (danielrispler/plotkeeper#67) is none.
        if let Some((prompt, command)) = &self.typed_skill {
            let fork = self.agents.iter_mut().find(|a| {
                a.parent == Parent::Unattached
                    && a.spawn.is_none()
                    && a.first_prompt.as_ref() == Some(prompt)
                    && a.agent_type.as_ref().is_some_and(|t| !t.is_empty())
            });
            if let Some(fork) = fork {
                fork.parent = Parent::Session;
                fork.name.clone_from(command);
                self.typed_skill = None;
            }
        }
        // A subagent that ended asks Daniel nothing, and its calls never resolve.
        let ended = self.agents.iter().filter(|a| a.ended_at.is_some());
        let ended: BTreeSet<String> = ended.map(|a| a.agent.id.clone()).collect();
        let ended = |agent: &Option<String>| agent.as_ref().is_some_and(|a| ended.contains(a));
        self.blocks.retain(|w| !(w.named && ended(&w.agent)));
        self.running.retain(|call| !ended(&call.agent));
        self.resolve(|(_, agent)| ended(agent));
    }

    /// The subagent a wake-up with no markup is taken to be from: the one that sent the latest
    /// event, as its report is what wakes the session (its `Stop` may land after the wake-up);
    /// empty when there is none.
    pub(crate) fn woken_by(&self) -> String {
        let last = self.agents.iter().max_by_key(|a| a.agent.last_event_at);
        last.map(|a| a.agent.id.clone()).unwrap_or_default()
    }

    /// Who sent `prompt`. A wake-up names a known task id of the session in markup the studio
    /// knows: `<agent-message from="…">` or `<task-id>…</task-id>`. A known id in other markup is
    /// drift, and one in plain text is Daniel's.
    pub(crate) fn sender(&self, prompt: &str) -> Sender {
        let known = self.agents.iter().map(|a| &a.agent.id).chain(&self.tasks);
        let mut unrecognised = None;
        for id in known {
            match claude::names(prompt, id) {
                claude::Names::WakeUp => return Sender::WakeUp(id.clone()),
                claude::Names::Unrecognised => {
                    unrecognised.get_or_insert_with(|| id.clone());
                }
                claude::Names::No => {}
            }
        }
        unrecognised.map_or(Sender::Daniel, Sender::Unrecognised)
    }

    /// Whether a card's Allow covers `agent`'s call of `tool` in `cwd`: its pass lets the call through.
    pub(crate) fn passes(&self, tool: &event::Tool, cwd: &str, agent: Option<&str>) -> bool {
        self.blocked_commands.iter().any(|c| {
            matches!(
                c.status,
                BlockedStatus::Allowed | BlockedStatus::AllowedNotRun | BlockedStatus::Running
            ) && c.retried_by(tool, cwd, agent)
        })
    }

    /// The home that `event` gives subagent `id`, if it is not homed yet (danielrispler/plotkeeper#206): a subagent in
    /// its session's worktree, whose call touches another one, makes the first such worktree its home. An
    /// isolated subagent, and one already homed, have none to take.
    pub(crate) fn new_home(&self, id: &str, event: &Event, roots: &Roots) -> Option<String> {
        let session_root = roots.of(&self.cwd);
        let agent = self.subagent(id).filter(|a| a.folder.is_some());
        let in_session = match agent {
            Some(a) => a.owner == self.agent.id,
            None => roots.of(&event.cwd) == session_root,
        };
        let mut listed = event.studio_touched_roots.iter().flatten();
        let other = listed.find(|r| !r.is_empty() && **r != session_root);
        other.filter(|_| in_session).cloned()
    }

    /// Turn `turn`'s checks against `list`. A call that recorded no worktree is in the session's
    /// folder's, or its worker's own.
    fn checks_of(&self, turn: usize, list: &[String], roots: &Roots) -> Vec<checks::Check> {
        let home = |agent: Option<&str>| match agent.and_then(|id| self.subagent(id)) {
            Some(worker) if !worker.root.is_empty() => worker.root.clone(),
            _ => roots.of(&self.cwd),
        };
        checks::of_turn(&self.ledger, turn, list, &home)
    }

    /// Turn `turn`'s checks and the ones its feed lists, read again from the ledger.
    pub(crate) fn recheck(
        &mut self,
        turn: usize,
        list: &[String],
        roots: &Roots,
        durations: &checks::Durations,
    ) {
        let found = self.checks_of(turn, list, roots);
        let typical = |command: &str| durations.typical(list, command);
        self.turns[turn].listed = checks::listed(&found, &typical);
        self.turns[turn].checks = found;
    }

    /// Whether subagent `id` gets cards of its own: a worker in a worktree of its own.
    pub(crate) fn has_cards(&self, id: &str) -> bool {
        self.subagent(id)
            .is_some_and(|a| a.kind == Kind::Worker && a.owner == id)
    }

    /// Whether worktree `root` is a running worker's with cards of its own, which the session's
    /// cards leave out (danielrispler/Falinks#434, decision 10).
    pub(crate) fn workers_own(&self, root: &str) -> bool {
        let running = self.agents.iter().filter(|a| a.ended_at.is_none());
        running
            .filter(|a| a.root == root)
            .any(|a| self.has_cards(&a.agent.id))
    }

    /// The worktrees its turn touched, but those of running workers with cards.
    pub(crate) fn closing(&self) -> Vec<String> {
        let touched = self.touched.iter().filter(|root| !self.workers_own(root));
        touched.cloned().collect()
    }

    /// Its subagents in its own worktree that ran since its last card and its turn's end: the
    /// between-turn card's tags (danielrispler/Falinks#434, decisions 16 and 17).
    pub(crate) fn between_turns(&self, at: u64) -> Vec<String> {
        let after = self.agent.since.at.max(self.stopped_at);
        let own = self.agents.iter().filter(|a| a.owner == self.agent.id);
        let ran = own.filter(|a| a.agent.last_event_at > after || a.ended_at == Some(at));
        ran.map(|a| a.agent.id.clone()).collect()
    }

    /// The agent record of owner key `key`: subagent `key`'s, or else the session's own.
    pub(crate) fn record(&self, key: &str) -> &Agent {
        self.subagent(key).map_or(&self.agent, |a| &a.agent)
    }

    pub(crate) fn record_mut(&mut self, key: &str) -> &mut Agent {
        match self.agents.iter_mut().find(|a| a.agent.id == key) {
            Some(a) => &mut a.agent,
            None => &mut self.agent,
        }
    }

    /// The card on `owner`'s change since its last card, as `Since` says, which starts the next one
    /// at `at`: `change`, or else its workers' cards.
    pub(crate) fn card(&mut self, owner: &str, change: Option<Change>, at: u64) -> CardRequest {
        // A worker's cards belong to its task, and a session's to Daniel's latest prompt.
        let worker = owner != self.agent.id;
        let prompt = if worker {
            self.subagent(owner).map(|a| a.agent.started_at)
        } else {
            let daniels = self.prompts.iter().rfind(|p| p.sender.is_daniels());
            daniels.map(|p| p.at)
        };
        let record = self.record_mut(owner);
        let since = std::mem::take(&mut record.since);
        record.since.at = at;
        let (base, change) = change.map_or_default(|c| (c.base, Some(c.diff)));
        CardRequest {
            session_id: self.agent.id.clone(),
            id: card_id(owner, at),
            base,
            change,
            prompt,
            agent: worker.then(|| owner.into()),
            changed_by: since.by,
            workers: since.cards,
            check: None,
            asks: self.needs_you(),
        }
    }

    /// The summary request for the turn that just ended with `reply`, its card id `id`.
    pub(crate) fn summary_request(
        &self,
        id: String,
        reply: &str,
        card: Option<String>,
        wanted: bool,
    ) -> SummaryRequest {
        // shortcut: the transcript may lag the `Stop` by one poll, so the final reply
        // is taken from the `Stop`; read the transcript first if earlier texts go missing.
        let mut turn = self.transcript.clone();
        if !reply.is_empty() && turn.responses.last().map(String::as_str) != Some(reply) {
            turn.responses.push(reply.into());
        }
        turn.ended = true;
        let daniels = self.prompts.iter().rfind(|p| p.sender.is_daniels());
        SummaryRequest {
            session_id: self.agent.id.clone(),
            id,
            prompt: daniels.map(|p| p.text.clone()).unwrap_or_default(),
            intentions: turn.intentions(),
            responses: turn.responses,
            card,
            facts: Facts::default(),
            wanted,
        }
    }

    /// Whether it is in a turn: prompted, and neither stopped nor ended since.
    pub(crate) fn in_turn(&self) -> bool {
        !self.turn_ended && !self.ended
    }

    /// Whether `event` starts a turn by its prompt id (danielrispler/Falinks#434, decision 3): an event of the session's
    /// own agent with a new prompt id, or a prompt with the last turn's id once that turn ended. A
    /// start, an end or a notice never starts one: in the day log they carry ids no turn ran under.
    /// An event with no prompt id starts none; a prompt then opens a wait by the old rule (decision 6).
    pub(crate) fn starts_turn(&self, event: &Event) -> bool {
        let Some(id) = &event.prompt_id else {
            return false;
        };
        event.agent_id.is_none()
            && !event.outside_turns()
            && (self.turn.as_ref() != Some(id) || event.prompt().is_some() && !self.in_turn())
    }

    /// Whether `event` opens a turn: it starts one by its prompt id, or it is a prompt outside one.
    pub(crate) fn opens_turn(&self, event: &Event) -> bool {
        self.starts_turn(event) || event.prompt().is_some() && !self.in_turn()
    }

    /// Puts it back in a turn, with no stopped line.
    pub(crate) fn reopen_turn(&mut self) {
        self.turn_ended = false;
        self.stopped = None;
    }

    /// Whether `event` ends its turn, keyed by a prompt id, without a `Stop` (danielrispler/Falinks#434, decision 5): the
    /// next turn starts, Claude Code says the agent waits for input, or the shell's line says Daniel
    /// interrupted this turn. Esc fires no `Stop`.
    pub(crate) fn ends_turn_early(&self, event: &Event) -> bool {
        let interrupted = event.has(|h| matches!(h, Happened::Interrupted))
            && event.prompt_id.is_some()
            && event.prompt_id == self.turn;
        let idle = event.agent_id.is_none()
            && (event.has(|h| matches!(h, Happened::IdlePrompt)) || interrupted);
        let keyed = self.turn.is_some();
        keyed && self.in_turn() && !event.turn_ends() && (idle || self.starts_turn(event))
    }

    /// Takes `prompt` as a prompt of its turn; `opens` when it opened the turn. The turn's first
    /// prompt names it in the feed.
    pub(crate) fn take_prompt(&mut self, prompt: Prompt, opens: bool) {
        self.woken = !prompt.sender.is_daniels() && (self.woken || opens);
        let line = match &prompt.sender {
            Sender::WakeUp(_) => self.returned.clone().unwrap_or_default(),
            _ => prompt
                .text
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or_default()
                .into(),
        };
        if let Some(turn) = self.turns.last_mut().filter(|t| t.prompt.is_empty()) {
            turn.prompt = line;
        }
        self.prompts.push(prompt);
    }

    /// The turn a call of `agent` is in: the latest, or for a subagent's call the one that spawned it.
    pub(crate) fn ledger_turn(&self, agent: Option<&str>) -> Option<usize> {
        let spawned = agent.and_then(|a| self.subagent(a)?.turn);
        spawned.or_else(|| self.current_turn())
    }

    /// The index of its latest turn, which a check, subagent or question that starts now is in.
    pub(crate) fn current_turn(&self) -> Option<usize> {
        self.turns.len().checked_sub(1)
    }

    /// Ends its turn at `at`, with the change since the turn's base: a card on it, else on its
    /// workers' cards since its last card, or only its brief when a wake-up opened the turn.
    pub(crate) fn end_turn(&mut self, change: Option<Change>, at: u64) -> Option<Output> {
        // A blocked command outlasts the turn it stopped: it waits on Daniel until his next prompt (#572).
        let denied: Vec<Block> = (self.blocks.iter())
            .filter(|w| w.cause == Cause::Denied && w.agent.is_none())
            .cloned()
            .collect();
        self.unblock();
        self.blocks.extend(denied);
        self.close_spans(at);
        if let Some(turn) = self.turns.last_mut().filter(|t| t.ended_at.is_none()) {
            turn.ended_at = Some(at);
        }
        self.turn_ended = true;
        self.stopped_at = at;
        let workers = !self.agent.since.cards.is_empty();
        let id = self.agent.id.clone();
        let output = match change {
            Some(change) => Some(Output::Card(self.card(&id, Some(change), at))),
            None if workers && self.woken => Some(Output::Brief(self.card(&id, None, at))),
            None if workers => Some(Output::Card(self.card(&id, None, at))),
            None => None,
        };
        self.agent.since.by.clear();
        output
    }

    /// Keeps the trees the hook recorded at its latest turn's first touch of each worktree.
    pub(crate) fn keep_start_trees(&mut self, starts: &BTreeMap<String, String>) {
        let Some(turn) = self.turns.last_mut() else {
            return;
        };
        for (root, tree) in starts {
            turn.trees
                .entry(root.clone())
                .or_insert_with(|| (tree.clone(), None));
        }
    }

    /// Keeps the trees the hook recorded at its latest turn's end, for the worktrees it has a start
    /// tree of.
    pub(crate) fn keep_end_trees(&mut self, ends: &BTreeMap<String, String>) {
        let Some(turn) = self.turns.last_mut() else {
            return;
        };
        for (root, tree) in ends {
            if let Some(trees) = turn.trees.get_mut(root) {
                trees.1 = Some(tree.clone());
            }
        }
        turn.ask_again_if_last(!ends.is_empty());
    }

    /// Keeps the trees the hook recorded on worker `event` in the turn that spawned the worker, as
    /// its calls are (`ledger_turn`), even once that turn ended (danielrispler/plotkeeper#142): a start
    /// at its first touch of a worktree, and an end at its stop, which comes after the turn's own.
    pub(crate) fn keep_worker_trees(&mut self, event: &Event) {
        let Some(turn) = self
            .ledger_turn(event.agent_id.as_deref())
            .and_then(|t| self.turns.get_mut(t))
        else {
            return;
        };
        for (root, tree) in &event.studio_turn_start {
            turn.trees
                .entry(root.clone())
                .or_insert_with(|| (tree.clone(), None));
        }
        for (root, tree) in &event.studio_turn_end {
            if let Some(trees) = turn.trees.get_mut(root) {
                trees.1 = Some(tree.clone());
            }
        }
        let landed = !event.studio_turn_start.is_empty() || !event.studio_turn_end.is_empty();
        turn.ask_again_if_last(landed);
    }

    /// The worktree of each running subagent, with the turn that spawned it: its trees there go on
    /// changing after that turn ends (danielrispler/plotkeeper#142).
    pub(crate) fn open_roots(&self) -> BTreeSet<(usize, String)> {
        let running = self.agents.iter().filter(|a| a.ended_at.is_none());
        running
            .filter_map(|a| Some((a.turn?, a.root.clone())))
            .collect()
    }

    /// The span of time its turn `turn`'s trees run in, from its start to its end, or its workers'
    /// last end, as a worker edits on after its turn (danielrispler/plotkeeper#142).
    pub(crate) fn trees_window(&self, turn: usize) -> (u64, u64) {
        let (since, end) = self
            .turns
            .get(turn)
            .map_or((0, None), |t| (t.started_at, t.ended_at));
        let spawned = self.agents.iter().filter(|a| a.turn == Some(turn));
        let ends: Option<Vec<u64>> = spawned.map(|a| a.ended_at).collect();
        // A turn with no end that nothing runs in any more stops at the next turn's start, or at the
        // session's last event once it ended.
        let stale = self
            .turns
            .get(turn + 1)
            .map(|t| t.started_at)
            .or(self.ended.then_some(self.agent.last_event_at));
        // Until now while it or a worker it spawned runs.
        let until = match (end, ends, stale) {
            (Some(end), Some(ends), _) => ends.into_iter().fold(end, u64::max),
            (None, Some(ends), Some(stale)) => ends.into_iter().fold(stale, u64::max),
            _ => u64::MAX,
        };
        (since, until)
    }

    /// Ends its turn's span and its step's at `at`, when they run.
    pub(crate) fn close_spans(&mut self, at: u64) {
        let spans = [&mut self.turn_span, &mut self.agent.step_span];
        for span in spans.into_iter().flatten().filter(|s| s.end.is_none()) {
            span.end = Some(at);
        }
    }

    /// Whether Daniel deleted it, and it has not started again.
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    pub fn needs_you(&self) -> bool {
        !self.blocks.is_empty() || self.terminal_blocked
    }

    /// How many blocked commands wait on Daniel: blocked, also again, and the session has not moved on (#572).
    pub fn blocked_waiting(&self) -> usize {
        let waits = |c: &&BlockedCommand| c.status.waits();
        self.blocked_commands.iter().filter(waits).count()
    }

    /// Its workers that failed: they ended with a status other than `completed`, such as `killed`.
    pub(crate) fn failed_workers(&self) -> Vec<String> {
        self.agents
            .iter()
            .filter(|a| a.kind == Kind::Worker && a.failed())
            .map(|a| a.agent.id.clone())
            .collect()
    }

    /// Adds a block by `agent` on the running calls it holds (danielrispler/Falinks#434, decisions 25 and 26), so that
    /// their `PostToolUse` clears it. A named block takes its agent's latest such call. A notice or a
    /// request for input with no agent names none: it takes every such call of the agents still
    /// running. With none, its call's `PreToolUse` may still land (the hooks are async); otherwise
    /// it lasts until its turn ends.
    pub(crate) fn block(
        &mut self,
        cause: Cause,
        tool: String,
        input: Value,
        agent: Option<String>,
        at: u64,
    ) {
        let named = agent.is_some() || cause == Cause::Call;
        let mut block = Block {
            cause,
            tool,
            input,
            agent,
            named,
            calls: Vec::new(),
            at,
            noticed: false,
            denied: None,
        };
        let mut calls = self.running.iter().filter(|c| block.takes(c));
        let calls: Vec<&Call> = if named {
            calls.next_back().into_iter().collect()
        } else {
            calls.collect()
        };
        let calls = calls
            .into_iter()
            .filter_map(|c| Some((c.id.clone()?, c.agent.clone())));
        block.calls = calls.collect();
        self.blocks.push(block);
    }

    /// Takes the calls that `gone` names off every block; a block whose last call went clears.
    pub(crate) fn resolve(&mut self, gone: impl Fn(&(String, Option<String>)) -> bool) {
        self.blocks.retain_mut(|w| {
            let had = !w.calls.is_empty();
            w.calls.retain(|c| !gone(c));
            !had || !w.calls.is_empty()
        });
    }

    /// Clears the blocks of the session's own agent and those that name no agent (danielrispler/Falinks#328, module 3,
    /// "Needs you": Daniel's `UserPromptSubmit`, not a wake-up; a turn's end; `SessionEnd`). The
    /// turn's calls end with it, so a call that never reported back takes no later block. Subagents run
    /// past their session's turn, so their own blocks stay.
    pub(crate) fn unblock(&mut self) {
        self.blocks.retain(|w| w.named && w.agent.is_some());
        self.running.retain(|c| c.agent.is_some());
    }
}

/// What blocks a session on Daniel, so that it needs you (danielrispler/Falinks#434, decisions 24 to 26): what asks,
/// and the calls it holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Block {
    pub(crate) cause: Cause,
    /// The tool it asks for, when its cause is a call.
    pub(crate) tool: String,
    pub(crate) input: Value,
    /// The agent that asks: a subagent, or `None` for the session's own agent or a notice that names
    /// none.
    pub(crate) agent: Option<String>,
    /// Whether the event named its agent. A notice with no agent may be any running agent's.
    named: bool,
    /// The calls it holds, by `tool_use_id`, each with its agent; it clears when the last
    /// resolves.
    pub(crate) calls: Vec<(String, Option<String>)>,
    pub(crate) at: u64,
    /// A permission notice already echoed it, so the next notice is another agent's.
    pub(crate) noticed: bool,
    /// For a blocked command, the denied call, which Daniel's choice names.
    #[serde(default)]
    pub(crate) denied: Option<String>,
}

/// What a block waits on.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum Cause {
    /// A tool call that asks, or a permission request for one.
    Call,
    /// A notice that a permission waits, which names no call.
    Notice,
    /// An external server's request for input.
    Elicitation,
    /// A blocked command, which waits on Daniel until its agent moves on (#572).
    Denied,
}

impl Block {
    /// Whether it waits on tool call `call`: a permission request on the call it names, of its own
    /// agent; an MCP server's request for input on an MCP tool; a permission notice on any call
    /// but an open spawn, which asks for no permission while its subagent runs.
    pub(crate) fn takes(&self, call: &Call) -> bool {
        let tool = match self.cause {
            Cause::Elicitation => call.kind == ToolKind::External,
            Cause::Notice => call.kind != ToolKind::Spawn,
            Cause::Call => self.tool == call.tool && self.input == call.input,
            // The denied call never ran, so no call is its.
            Cause::Denied => false,
        };
        tool && (!self.named || self.agent == call.agent)
    }

    /// Who it names: its agent, or with no agent named, the agent of its one call. `None` is the
    /// session's own agent, or no name when several agents may ask.
    pub(crate) fn asker(&self) -> Option<String> {
        match &self.calls[..] {
            _ if self.named => self.agent.clone(),
            [(_, agent)] => agent.clone(),
            _ => None,
        }
    }
}

/// A session's state (danielrispler/Falinks#369, decision 4). Each has a named clearer: an event, or the tick for idle.
/// The order is the session list's: needs you first, then the groups the list collapses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting on Daniel (danielrispler/Falinks#328, module 3, "Needs you"), until the step that asks resolves, a prompt
    /// of Daniel's, `Stop` or `SessionEnd`.
    NeedsYou,
    /// In a turn, from a prompt until `Stop` or `SessionEnd`.
    #[default]
    Running,
    /// The turn ended, or the session started and has had no prompt yet, until a prompt.
    YourTurn,
    /// At your turn with no events for `IDLE_AFTER_MS`, until any event.
    Idle,
    /// From `SessionEnd` until the session starts again under its id.
    Ended,
}

/// How long a Claude session's hook state and its OSC 7501 state may disagree before the mismatch log
/// gets a line (danielrispler/Falinks#560); a starting guess.
pub(crate) const MISMATCH_AFTER_MS: u64 = 5_000;

/// The state an OSC 7501 root state shows (danielrispler/Falinks#560): `working` is running, `blocked` and `error` need
/// Daniel, and a turn that ended (`done`) or a program waiting (`idle`) is his turn; `None` for a
/// state the protocol does not list.
pub(crate) fn shown_state(reported: &str) -> Option<State> {
    match reported {
        "working" => Some(State::Running),
        "blocked" | "error" => Some(State::NeedsYou),
        "done" | "idle" => Some(State::YourTurn),
        _ => None,
    }
}

/// 2 hours, a starting guess (danielrispler/Falinks#369, decision 4).
pub(crate) const IDLE_AFTER_MS: u64 = 2 * 60 * 60 * 1000;

/// A tool call the agent made, as the "now" line names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub tool: String,
    /// What it works on (`step_detail`): a command's `description`, else its first line.
    pub detail: Option<String>,
    /// A command's full text, one click away in the session view (danielrispler/Falinks#461, decision 12).
    #[serde(default)]
    pub command: Option<String>,
}

impl Step {
    pub(crate) fn new(tool: &str, input: &Value, cwd: &str) -> Step {
        Step {
            tool: tool.into(),
            detail: step_detail(input, cwd),
            command: input["command"].as_str().map(Into::into),
        }
    }
}

/// The id of `owner`'s card written at `at`, a worker's or else a session's (`CardRequest::id`).
pub(crate) fn card_id(owner: &str, at: u64) -> String {
    format!("{owner}@{at}")
}

/// When the hook received the event card `id` was written at (`card_id`); 0 for an id of another form.
pub(crate) fn card_time(id: &str) -> u64 {
    let at = id.rsplit_once('@').and_then(|(_, at)| at.parse().ok());
    at.unwrap_or_default()
}

/// The first `n` characters of an id.
pub(crate) fn short(id: &str, n: usize) -> &str {
    id.get(..n).unwrap_or(id)
}
