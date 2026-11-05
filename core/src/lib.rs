//! The studio core (#328, module 3): plain Rust with no I/O but the transcript reader
//! (`transcript.rs`), which reads the sessions' transcripts. It takes hook events one at a time, in
//! event log order, and keeps the state the app shows. App start replays the whole log through
//! `replay`; live events go through `apply`, which updates state the same way and also returns what
//! the shell must do.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Duration,
};

mod transcript;
mod work;
pub use transcript::{Context, Transcript};
pub use work::{Estimate, MIN_HISTORY, Phase, PhaseStart, Work, range};

#[derive(Default)]
pub struct Studio {
    sessions: HashMap<String, Session>,
    /// The session Daniel clicked last, which the session view shows.
    selected: Option<String>,
    /// The subagent of it whose line he opened since, whose own view shows instead.
    viewed: Option<String>,
    /// The shell's latest tick, in Unix milliseconds.
    now: u64,
    roots: Roots,
    /// The turns the replay saw end with both trees, for `backfill`.
    ended: Vec<Backfill>,
    /// How long each check ran when it did not fail, by `check_key`, oldest first (#461, decision 13).
    durations: HashMap<String, Vec<u64>>,
    /// The row of each session that continues another after a `/clear`, by its own id (#461,
    /// decision 18). A session not here is its own row.
    rows: HashMap<String, String>,
}

type FindRoot = dyn Fn(&str) -> Option<String> + Send;

/// Each folder's worktree root (#434, decision 11), found once per folder by the shell's lookup. With
/// none found, a folder is its own root.
#[derive(Default)]
struct Roots {
    find: Option<Box<FindRoot>>,
    known: std::cell::RefCell<HashMap<String, String>>,
}

impl Roots {
    fn of(&self, folder: &str) -> String {
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
}

/// What the session view shows: the session Daniel selected, and the subagent of it he opened since,
/// if any. The pane draws the pressed row and the view from it, and keeps no selection of its own.
#[derive(Debug, PartialEq, Serialize)]
pub struct Selection<'a> {
    pub session: &'a Session,
    pub agent: Option<&'a str>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Session {
    /// Its own agent: the session id, last step, first and latest events, and worktree since.
    #[serde(flatten)]
    pub agent: Agent,
    /// The folder the session works in, from its latest event.
    pub cwd: String,
    /// The last segment of `cwd`, or the whole `cwd` when that segment is empty.
    pub folder_name: String,
    /// The studio terminal it runs in, its `STUDIO_TERMINAL_ID` from its latest event, `None` when
    /// it runs outside the studio (#461, decision 8).
    pub terminal: Option<String>,
    /// Daniel's name for it, from the studio's latest rename line in the log (#469); `None` for its
    /// folder's.
    pub name: Option<String>,
    /// The id `claude --resume` takes: its latest conversation's, which a `/clear` changes while the
    /// row keeps its first id (#461, decision 18).
    pub resume_id: String,
    /// When each `/clear` continued it, oldest first: the "cleared" markers in its history.
    pub cleared: Vec<u64>,
    /// Its latest `SessionEnd` was a `/clear`, so the next `SessionStart` from `clear` on its
    /// terminal, or in its folder, continues it.
    #[serde(skip)]
    clearing: bool,
    pub state: State,
    /// Whether closing its terminal asks first: it is in a turn, or its latest `Stop` listed
    /// background tasks (#469).
    pub asks_before_closing: bool,
    /// Daniel deleted it: the list leaves it out until it starts again.
    #[serde(skip)]
    deleted: bool,
    /// Its latest `Stop` listed background tasks.
    #[serde(skip)]
    background: bool,
    /// The checks that ran, oldest first: pass or fail lines in the session's timeline (#369,
    /// decision 14).
    pub checks: Vec<CheckLine>,
    /// The tool calls whose `PreToolUse` landed and whose `PostToolUse` has not.
    #[serde(skip)]
    running: Vec<Call>,
    /// What blocks it on Daniel, so that it needs you, oldest first (#434, decision 24).
    #[serde(skip)]
    blocks: Vec<Block>,
    /// Its turn's final message asks Daniel something: a block no tool call clears.
    #[serde(skip)]
    asked: bool,
    #[serde(skip)]
    turn_ended: bool,
    #[serde(skip)]
    ended: bool,
    /// Whether a `Stop` of it requested Claude Code's version.
    #[serde(skip)]
    versioned: bool,
    /// Its subagents at any depth, in the order they were first seen (#369, decision 5).
    #[serde(serialize_with = "serialize_agents")]
    pub agents: Vec<Subagent>,
    /// Who asks Daniel, and since when, while it needs him.
    pub asker: Option<Asker>,
    /// Its prompts, oldest first, each marked with who sent it (#369, decision 7).
    pub prompts: Vec<Prompt>,
    /// The ids of its background tasks, commands included, from their responses and `Stop`'s
    /// `background_tasks`. With its subagents' ids, they are the task ids a wake-up can name.
    #[serde(skip)]
    tasks: BTreeSet<String>,
    /// The questions its agents asked, oldest first (#369, decision 21).
    pub questions: Vec<Question>,
    /// Its answered questions, plans and permissions, oldest first: its decision history (#461,
    /// decision 17). The card store keeps them across restarts and gives them to the pane.
    #[serde(skip)]
    pub decisions: Vec<Decision>,
    /// The questions answered on the studio card, by call, whose result has not landed yet.
    #[serde(skip)]
    carded: BTreeSet<String>,
    /// When its latest turn started and ended (#461, decision 13).
    pub turn_span: Option<Span>,
    /// Its latest `Stop`, until its next prompt (#369, decision 19).
    pub stopped: Option<Stopped>,
    /// When its latest turn ended, which took its workers' cards since its last card.
    #[serde(skip)]
    stopped_at: u64,
    /// Whether its turn was opened by a wake-up and has had no prompt of Daniel's since.
    #[serde(skip)]
    woken: bool,
    /// Claude Code's prompt id of its latest turn (#434, decision 3), `None` until an event of its
    /// own agent carries one.
    #[serde(skip)]
    turn: Option<String>,
    /// The prompts that arrived during its turn, queued or typed after an interrupt: the next turn's
    /// (#434, decision 4).
    #[serde(skip)]
    queued: Vec<Prompt>,
    /// The worktree roots its latest turn touched, each with its base from the first touch (#434,
    /// decision 9).
    #[serde(skip)]
    touched: BTreeSet<String>,
    /// The tree the hook recorded at its latest turn's first touch of each worktree, by root (#486).
    #[serde(skip)]
    trees: BTreeMap<String, String>,
    /// "worker <name> returned", from its latest wake-up until its own next step or Daniel's next
    /// prompt (#410, decision 27).
    pub returned: Option<String>,
    /// What its transcript says of its latest turn: the intention, the agent's texts, or why it
    /// cannot be read (#461, decision 12).
    pub transcript: Transcript,
    #[serde(skip)]
    transcript_file: transcript::TranscriptFile,
    /// Its issue, PR and worktree, which name it unless Daniel renamed it (#461, decision 19).
    pub identity: Identity,
    /// Its work on its ticket: its kind, its phases and how long its agent worked (#461, decision
    /// 14).
    pub work: Work,
}

/// What names a session (#461, decision 19): the issue of its `issue-<n>-…` branch, else the one
/// it claimed with `wayfinder-startup --claim <n>`, the PR the shell finds for its branch, and its
/// worktree's folder.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Identity {
    pub issue: Option<u64>,
    /// Its folder's branch, from its transcript.
    pub branch: Option<String>,
    pub pr: Option<u64>,
    /// The last segment of its folder's worktree root.
    pub worktree: String,
    /// The derived name: "#480 · PR #490 · agent-a1b2", "fix-typo · agent-a1b2" or "main · Falinks".
    pub name: String,
    #[serde(skip)]
    claim: Option<u64>,
}

impl Identity {
    fn named(&mut self) {
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

/// The issue a `wayfinder-startup … --claim <n>` command claims.
fn claimed(command: &str) -> Option<u64> {
    if !command.contains("wayfinder-startup") {
        return None;
    }
    let mut words = command.split_whitespace();
    words.find(|w| *w == "--claim")?;
    words.next()?.parse().ok()
}

/// A terminal of the daemon, as its `list` gives it, or as its record of the terminals it ran gives
/// them after it died (`terminal-daemon/src/lib.rs`). Fields the record lacks are empty.
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
    /// What it reruns: its spawn's argv, empty for a shell.
    pub argv: Vec<String>,
    /// The session to reopen with `claude --resume`, for a Claude terminal whose session is known.
    pub session: Option<String>,
    /// What runs in it that a restart would end: a program, "a turn" or "background tasks".
    pub busy: Option<String>,
}

/// What happened in a worktree since the last card on it: a session's own, or a subagent's that
/// works in a folder of its own (#369, decisions 10 and 11).
#[derive(Clone, Debug, Default, PartialEq)]
struct Since {
    /// When that card, or else the first base, was taken.
    at: u64,
    /// The files that other subagents edited or wrote there, relative to its folder, by subagent.
    by: BTreeMap<String, BTreeSet<String>>,
    /// A session's only: its workers' cards since, at any depth.
    cards: Vec<String>,
}

/// 20 minutes, a starting guess (#369, decision 11 and Further Notes).
const FALLBACK_MS: u64 = 20 * 60 * 1000;

/// 2 minutes, a starting guess (#461, decision 22): a check that usually runs longer gets a card at
/// its start (decision 15).
const LONG_CHECK_MS: u64 = 2 * 60 * 1000;

/// A check a `PreToolUse` starts (`Studio::check_start`): its whole command, and its typical duration
/// (`Studio::typical`).
struct CheckStart {
    command: String,
    typical: Option<u64>,
}

impl CheckStart {
    /// It usually takes over 2 minutes, so it gets a card at its start (#461, decision 15).
    fn long(&self) -> bool {
        self.typical.is_some_and(|ms| ms > LONG_CHECK_MS)
    }
}

/// A prompt of a session (#369, decision 7), received at `at`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Prompt {
    pub at: u64,
    pub sender: Sender,
    /// What it says, which the brief's rewrite reads (#369, decision 16); the pane never shows it.
    #[serde(skip)]
    pub text: String,
}

/// A session's `Stop`, which its row tells about while its turn is over (#369, decision 19).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stopped {
    /// The id a card on it has (`CardRequest::id`).
    pub card: String,
    /// Whether it requested a card (`Output::Card`). Only a live `Stop` does.
    pub requested: bool,
    /// The first line of the agent's last message.
    pub message: Option<String>,
}

/// Who sent a prompt: Daniel, or Claude Code waking the session when one of its tasks returns.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Sender {
    Daniel,
    /// A wake-up, with the id of the task it names.
    WakeUp(String),
    /// A known task id in markup the studio does not know: drift, counted as Daniel's.
    Unrecognised(String),
}

impl Sender {
    /// Whether the prompt counts as Daniel's: anything but a wake-up, drift included.
    pub fn is_daniels(&self) -> bool {
        !matches!(self, Sender::WakeUp(_))
    }
}

/// An `AskUserQuestion` call, which Daniel answers in the terminal (#369, decision 21).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Question {
    /// The call's id, which the answer hook's request names (#461, decision 16).
    tool_use_id: String,
    /// The subagent that asks, `None` for the session's own agent.
    pub agent: Option<String>,
    /// Its questions and their options, as the tool input gives them.
    pub questions: Value,
    pub answer: Answer,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Asked, with no result yet.
    Open,
    /// Daniel's choice for each question, keyed by its text: the `PostToolUse`'s `answers`, kept only
    /// when it is an object of strings (#434, decision 42).
    Answered(Value),
    /// Its call failed or was interrupted, or its turn or its agent ended first.
    NotAnswered,
}

/// An answered question, plan or permission in its session's decision history (#461, decision 17).
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

/// Where Daniel answered: the studio card, through the answer hook, or the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Card,
    Terminal,
}

/// A subagent of a session (#369, decisions 5, 6 and 8).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
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
    /// The first line of its last message, from its `SubagentStop`.
    pub result: Option<String>,
    /// Daniel opened its line after it failed (#434, decision 39).
    pub opened: bool,
    /// The folder of its first event that has one: its worktree, or its parent's folder.
    #[serde(skip)]
    pub folder: Option<String>,
    /// Its folder's worktree root, and the owner key of the cards on its changes there, decided at
    /// its first folder and never again (#434, decision 11).
    #[serde(skip)]
    root: String,
    #[serde(skip)]
    owner: String,
    /// What its spawn asked it to do, which its brief's rewrite reads; the pane never shows it.
    #[serde(skip)]
    pub prompt: String,
    /// Its spawn's call, by `tool_use_id`, and the agent that made it, `None` for the session's own.
    #[serde(skip)]
    spawn: Option<(String, Option<String>)>,
}

/// The state a session's own agent and each subagent share (#410, decision 22). Its id is the owner
/// key of its card stream.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Agent {
    pub id: String,
    pub last_step: Option<Step>,
    /// When the hook received its first and its latest event (`studio_received_ms`), in Unix
    /// milliseconds.
    pub started_at: u64,
    pub last_event_at: u64,
    /// When its last step started, and ended at its result or its turn's end (#461, decision 13).
    pub step_span: Option<Span>,
    /// The last step's `tool_use_id`, whose result ends it.
    #[serde(skip)]
    step_call: String,
    #[serde(skip)]
    since: Since,
}

/// When something started and ended, in Unix milliseconds as the hook received them; `end` is `None`
/// while it runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Span {
    pub start: u64,
    pub end: Option<u64>,
}

impl Agent {
    fn new(id: &str, at: u64) -> Self {
        Agent {
            id: id.into(),
            started_at: at,
            last_event_at: at,
            ..Agent::default()
        }
    }

    /// Its step of call `call` starts at `at`.
    fn step(&mut self, step: Step, call: &str, at: u64) {
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
    fn step_done(&mut self, call: &str, at: u64) {
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
        self.ended_at.get_or_insert(at);
    }

    /// It ended with a status other than `completed`, such as `killed`.
    pub fn failed(&self) -> bool {
        matches!(self.status, Status::Ended(_))
    }

    /// A helper that succeeded, or one that failed and Daniel opened, folds into its parent's line
    /// (#369, story 18). A failed child stays visible until he opens it.
    pub fn faded(&self) -> bool {
        let succeeded = self.status == Status::Completed;
        self.kind == Kind::Helper && (succeeded || self.failed() && self.opened)
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

impl Serialize for Status {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Each subagent with its computed `failed` and `faded`, which the pane reads.
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
    }
    let wire = agents.iter().map(|agent| Wire {
        agent,
        failed: agent.failed(),
        faded: agent.faded(),
    });
    serializer.collect_seq(wire)
}

/// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Parent {
    Session,
    Agent(String),
    #[default]
    Unattached,
}

/// A worker is spawned with its own worktree, or changes files; any other subagent is a helper
/// (#369, decision 6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Worker,
    #[default]
    Helper,
}

impl Session {
    /// Its subagent `id`, at any depth.
    pub fn subagent(&self, id: &str) -> Option<&Subagent> {
        self.agents.iter().find(|a| a.agent.id == id)
    }

    /// The name of subagent `id`: its own, or else the first 5 characters of `id` (#410, decision 26).
    pub fn agent_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.subagent(id).map_or(short(id, 5), |a| &a.name)
    }

    /// Subagent `id`, added in "unattached" when this is the first event that names it.
    fn agent(&mut self, id: &str, at: u64) -> &mut Subagent {
        let index = match self.agents.iter().position(|a| a.agent.id == id) {
            Some(index) => index,
            None => {
                self.agents.push(Subagent {
                    agent: Agent::new(id, at),
                    name: short(id, 5).into(),
                    status: Status::Running,
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

    /// The running subagents that `event` ends (#434, decision 18), each with its status, `None`
    /// when the signal tells none, and when it ended.
    fn ends(&self, event: &Event) -> Vec<(String, Option<String>, u64)> {
        let at = event.studio_received_ms;
        let tool = event.tool_name.as_deref().unwrap_or_default();
        let mut ends: Vec<(String, Option<String>, u64)> = Vec::new();
        let mut end = |id: &str, status: Option<&str>, at| {
            ends.push((id.into(), status.map(Into::into), at));
        };
        let spawned_by = |call: &str| {
            let agents = self.agents.iter();
            let call = call.to_string();
            agents.filter(move |a| a.spawn.as_ref().is_some_and(|(id, _)| *id == call))
        };
        match event.hook_event_name.as_str() {
            "SubagentStop" => event.agent_id.iter().for_each(|id| end(id, None, at)),
            // A wake-up names the task it reports on, and how it ended (#370 note).
            "UserPromptSubmit" => reported(&event.prompt).for_each(|(id, s)| end(id, Some(s), at)),
            // A foreground spawn's response comes at its end.
            "PostToolUse"
                if tool == "Agent"
                    && event.tool_response.status.as_deref() == Some("completed") =>
            {
                let child = event.tool_response.agent_id.iter();
                child.for_each(|id| end(id, None, at));
            }
            "PostToolUseFailure" if tool == "Agent" => {
                let children = spawned_by(&event.tool_use_id);
                children.for_each(|a| end(&a.agent.id, Some("interrupted"), at));
            }
            // `background_tasks` lists only the running tasks, so a child of the session's own agent
            // that it no longer lists has ended, at its last event.
            // shortcut: only the session's own children, since whether it lists their children is
            // unseen; widen it if a recorded log shows it does.
            "Stop"
                if event.agent_id.is_none()
                    && let Some(tasks) = event.background_tasks.as_array() =>
            {
                let listed = |id: &str| tasks.iter().any(|task| task["id"] == id);
                let children = self.agents.iter().filter(|a| a.parent == Parent::Session);
                let gone = children.filter(|a| !listed(&a.agent.id));
                gone.for_each(|a| end(&a.agent.id, None, a.agent.last_event_at));
            }
            "SessionEnd" => self.agents.iter().for_each(|a| end(&a.agent.id, None, at)),
            _ => {}
        }
        // A turn that ends without its `Stop` ends the subagents it runs in the foreground: their
        // spawn calls are still open. A background spawn's call closes at its launch.
        if self.ends_turn_early(event) {
            let own = self
                .running
                .iter()
                .filter(|c| c.agent.is_none() && c.tool == "Agent");
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

    /// Updates the subagent tree with one event. A spawn's response names its child (`agentId`),
    /// and every child event names its agent (`agent_id`).
    fn track_agents(&mut self, event: &Event, worktree_changed: bool, roots: &Roots) {
        let at = event.studio_received_ms;
        let tool = event.tool_name.as_deref().unwrap_or_default();
        for (id, status, at) in self.ends(event) {
            self.agent(&id, at).end(status.as_deref(), at);
        }
        if let Some(id) = &event.agent_id {
            // A child's first event matches it to the oldest open spawn call with no child yet.
            // shortcut: no event names both a child and its spawn's call until the spawn's response,
            // so two spawns open at once may swap; match by a shared field if a recording shows one.
            if self.subagent(id).is_none() {
                let bound = |call: &Option<String>| {
                    let mut spawns = self.agents.iter().filter_map(|a| a.spawn.as_ref());
                    spawns.any(|(id, _)| Some(id) == call.as_ref())
                };
                let open = self.running.iter().filter(|c| c.tool == "Agent");
                let call = open
                    .filter(|c| c.agent.as_ref() != Some(id))
                    .find(|c| !bound(&c.id));
                let spawn = call.and_then(|c| Some((c.id.clone()?, c.agent.clone())));
                self.agent(id, at).spawn = spawn;
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
            // A changed file promotes a helper at that step: an `Edit` or `Write` that succeeded, or
            // a command that changed its worktree and either succeeded or ran and failed (#434,
            // decision 21).
            let ran =
                event.hook_event_name == "PostToolUse" || event.error.starts_with("Exit code");
            let changed = matches!(tool, "Edit" | "Write")
                && event.hook_event_name == "PostToolUse"
                || tool == "Bash" && worktree_changed && ran;
            match event.hook_event_name.as_str() {
                // A step after its end means its parent continued it (`SendMessage`).
                "PreToolUse" => {
                    let step = Step::new(tool, &event.tool_input, &event.cwd);
                    agent.agent.step(step, &event.tool_use_id, at);
                    agent.status = Status::Running;
                    agent.ended_at = None;
                }
                "PostToolUse" | "PostToolUseFailure" => {
                    agent.agent.step_done(&event.tool_use_id, at);
                    if changed {
                        agent.kind = Kind::Worker;
                    }
                }
                "SubagentStop" => {
                    let mut lines = event.last_assistant_message.lines().map(str::trim);
                    agent.result = lines.find(|line| !line.is_empty()).map(Into::into);
                    agent.end(None, at);
                }
                _ => {}
            }
        }
        if let Some(task) = event.tool_response.background_task_id.as_deref() {
            self.tasks.insert(task.into());
        }
        let listed = event.background_tasks.as_array().into_iter().flatten();
        let ids = listed.filter_map(|task| task["id"].as_str());
        self.tasks.extend(ids.map(Into::into));
        match event.hook_event_name.as_str() {
            "PostToolUse"
                if tool == "Agent"
                    && let Some(child) = event.tool_response.agent_id.as_deref() =>
            {
                let parent = event
                    .agent_id
                    .clone()
                    .map_or(Parent::Session, Parent::Agent);
                let input = &event.tool_input;
                let call = event.tool_use_id.clone();
                for agent in &mut self.agents {
                    if agent.spawn.as_ref().is_some_and(|(id, _)| *id == call) {
                        agent.spawn = None;
                    }
                }
                let agent = self.agent(child, at);
                agent.spawn = Some((call, event.agent_id.clone()));
                agent.parent = parent;
                if let Some(description) = input["description"].as_str() {
                    agent.name = description.into();
                }
                agent.prompt = input["prompt"].as_str().unwrap_or_default().into();
                if agent.agent_type.is_none() {
                    agent.agent_type = input["subagent_type"].as_str().map(Into::into);
                }
                if input["isolation"] == "worktree" {
                    agent.kind = Kind::Worker;
                }
                // A foreground spawn's response comes at its end.
                if event.tool_response.status.as_deref() == Some("completed") {
                    agent.end(None, at);
                }
            }
            // A wake-up's status overrides how another signal ended its task.
            "UserPromptSubmit" => {
                for (id, status) in reported(&event.prompt) {
                    if let Some(agent) = self.agents.iter_mut().find(|a| a.agent.id == id) {
                        agent.end(Some(status), at);
                    }
                }
            }
            _ => {}
        }
        // A subagent that ended asks Daniel nothing, and its calls never resolve.
        let ended = self.agents.iter().filter(|a| a.ended_at.is_some());
        let ended: BTreeSet<String> = ended.map(|a| a.agent.id.clone()).collect();
        let ended = |agent: &Option<String>| agent.as_ref().is_some_and(|a| ended.contains(a));
        self.blocks.retain(|w| !(w.named && ended(&w.agent)));
        self.running.retain(|call| !ended(&call.agent));
        self.resolve(|(_, agent)| ended(agent));
    }

    /// Who sent `prompt`. A wake-up names a known task id of the session in markup the studio
    /// knows: `<agent-message from="…">` or `<task-id>…</task-id>`. A known id in other markup is
    /// drift, and one in plain text is Daniel's.
    fn sender(&self, prompt: &str) -> Sender {
        let known = self.agents.iter().map(|a| &a.agent.id).chain(&self.tasks);
        let mut unrecognised = None;
        for id in known {
            if prompt.contains(&format!("<agent-message from=\"{id}\""))
                || prompt.contains(&format!("<task-id>{id}</task-id>"))
            {
                return Sender::WakeUp(id.clone());
            }
            if prompt.contains(&format!("=\"{id}\"")) || prompt.contains(&format!(">{id}</")) {
                unrecognised.get_or_insert_with(|| id.clone());
            }
        }
        unrecognised.map_or(Sender::Daniel, Sender::Unrecognised)
    }

    /// Whether subagent `id` gets cards of its own: a worker in a worktree of its own.
    fn has_cards(&self, id: &str) -> bool {
        self.subagent(id)
            .is_some_and(|a| a.kind == Kind::Worker && a.owner == id)
    }

    /// Whether worktree `root` is a running worker's with cards of its own, which the session's
    /// cards leave out (#434, decision 10).
    fn workers_own(&self, root: &str) -> bool {
        let running = self.agents.iter().filter(|a| a.ended_at.is_none());
        running
            .filter(|a| a.root == root)
            .any(|a| self.has_cards(&a.agent.id))
    }

    /// The worktrees its turn touched, but those of running workers with cards.
    fn closing(&self) -> Vec<String> {
        let touched = self.touched.iter().filter(|root| !self.workers_own(root));
        touched.cloned().collect()
    }

    /// Its subagents in its own worktree that ran since its last card and its turn's end: the
    /// between-turn card's tags (#434, decisions 16 and 17).
    fn between_turns(&self, at: u64) -> Vec<String> {
        let after = self.agent.since.at.max(self.stopped_at);
        let own = self.agents.iter().filter(|a| a.owner == self.agent.id);
        let ran = own.filter(|a| a.agent.last_event_at > after || a.ended_at == Some(at));
        ran.map(|a| a.agent.id.clone()).collect()
    }

    /// The agent record of owner key `key`: subagent `key`'s, or else the session's own.
    fn record(&self, key: &str) -> &Agent {
        self.subagent(key).map_or(&self.agent, |a| &a.agent)
    }

    fn record_mut(&mut self, key: &str) -> &mut Agent {
        match self.agents.iter_mut().find(|a| a.agent.id == key) {
            Some(a) => &mut a.agent,
            None => &mut self.agent,
        }
    }

    /// The card on `owner`'s change since its last card, as `Since` says, which starts the next one
    /// at `at`: `change`, or else its workers' cards.
    fn card(&mut self, owner: &str, change: Option<Change>, at: u64) -> CardRequest {
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
        }
    }

    /// Whether it is in a turn: prompted, and neither stopped nor ended since.
    fn in_turn(&self) -> bool {
        !self.turn_ended && !self.ended
    }

    /// Whether `event` starts a turn by its prompt id (#434, decision 3): an event of the session's
    /// own agent with a new prompt id, or a prompt with the last turn's id once that turn ended. A
    /// start, an end or a notice never starts one: in the day log they carry ids no turn ran under.
    /// An event with no prompt id starts none; a prompt then opens a wait by the old rule (decision 6).
    fn starts_turn(&self, event: &Event) -> bool {
        let name = event.hook_event_name.as_str();
        let Some(id) = &event.prompt_id else {
            return false;
        };
        event.agent_id.is_none()
            && !outside_turns(name)
            && (self.turn.as_ref() != Some(id) || name == "UserPromptSubmit" && !self.in_turn())
    }

    /// Whether `event` opens a turn: it starts one by its prompt id, or it is a prompt outside one.
    fn opens_turn(&self, event: &Event) -> bool {
        self.starts_turn(event) || event.hook_event_name == "UserPromptSubmit" && !self.in_turn()
    }

    /// Puts it back in a turn, with no stopped line.
    fn reopen_turn(&mut self) {
        self.turn_ended = false;
        self.stopped = None;
    }

    /// Whether `event` ends its turn, keyed by a prompt id, without a `Stop` (#434, decision 5): the
    /// next turn starts, or Claude Code says the agent waits for input. Esc fires no `Stop`.
    fn ends_turn_early(&self, event: &Event) -> bool {
        let idle = event.agent_id.is_none()
            && event.hook_event_name == "Notification"
            && event.notification_type == "idle_prompt";
        let keyed = self.turn.is_some();
        keyed
            && self.in_turn()
            && event.hook_event_name != "Stop"
            && (idle || self.starts_turn(event))
    }

    /// Takes `prompt` as a prompt of its turn; `opens` when it opened the turn.
    fn take_prompt(&mut self, prompt: Prompt, opens: bool) {
        self.woken = !prompt.sender.is_daniels() && (self.woken || opens);
        self.prompts.push(prompt);
    }

    /// Ends its turn at `at`, with the change since the turn's base: a card on it, else on its
    /// workers' cards since its last card, or only its brief when a wake-up opened the turn.
    fn end_turn(&mut self, change: Option<Change>, at: u64) -> Option<Output> {
        self.unblock();
        self.close_spans(at);
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

    /// The backfill of its turn ended at `at` with the hook's trees `ends`: `None` when no worktree
    /// has a start tree and another end tree.
    fn backfill(&self, ends: &BTreeMap<String, String>, at: u64) -> Option<Backfill> {
        let trees: Vec<Trees> = ends
            .iter()
            .filter_map(|(root, end)| {
                let start = self.trees.get(root).filter(|start| *start != end)?;
                let (root, start, end) = (root.clone(), start.clone(), end.clone());
                Some(Trees { root, start, end })
            })
            .collect();
        let daniels = self.prompts.iter().rfind(|p| p.sender.is_daniels());
        let request = CardRequest {
            session_id: self.agent.id.clone(),
            id: card_id(&self.agent.id, at),
            prompt: daniels.map(|p| p.at),
            ..CardRequest::default()
        };
        (!trees.is_empty()).then_some(Backfill { request, trees })
    }

    /// Ends its turn's span and its step's at `at`, when they run.
    fn close_spans(&mut self, at: u64) {
        let spans = [&mut self.turn_span, &mut self.agent.step_span];
        for span in spans.into_iter().flatten().filter(|s| s.end.is_none()) {
            span.end = Some(at);
        }
    }

    fn needs_you(&self) -> bool {
        self.asked || !self.blocks.is_empty()
    }

    /// Its workers that failed: they ended with a status other than `completed`, such as `killed`.
    fn failed_workers(&self) -> Vec<String> {
        self.agents
            .iter()
            .filter(|a| a.kind == Kind::Worker && a.failed())
            .map(|a| a.agent.id.clone())
            .collect()
    }

    /// Adds a block by `agent` on the running calls it holds (#434, decisions 25 and 26), so that
    /// their `PostToolUse` clears it. A named block takes its agent's latest such call. A notice or a
    /// request for input with no agent names none: it takes every such call of the agents still
    /// running. With none, its call's `PreToolUse` may still land (the hooks are async); otherwise
    /// it lasts until its turn ends.
    fn block(&mut self, tool: String, input: Value, agent: Option<String>, at: u64) {
        let named = agent.is_some() || !matches!(tool.as_str(), "Notification" | "Elicitation");
        let mut block = Block {
            tool,
            input,
            agent,
            named,
            calls: Vec::new(),
            at,
            noticed: false,
        };
        let mut calls = self.running.iter().filter(|c| block.takes(c));
        let calls: Vec<&Call> = match named {
            true => calls.next_back().into_iter().collect(),
            false => calls.collect(),
        };
        let calls = calls
            .into_iter()
            .filter_map(|c| Some((c.id.clone()?, c.agent.clone())));
        block.calls = calls.collect();
        self.blocks.push(block);
    }

    /// Takes the calls that `gone` names off every block; a block whose last call went clears.
    fn resolve(&mut self, gone: impl Fn(&(String, Option<String>)) -> bool) {
        self.blocks.retain_mut(|w| {
            let had = !w.calls.is_empty();
            w.calls.retain(|c| !gone(c));
            !had || !w.calls.is_empty()
        });
    }

    /// Clears the blocks of the session's own agent and those that name no agent (#328, module 3,
    /// "Needs you": Daniel's `UserPromptSubmit`, not a wake-up; a turn's end; `SessionEnd`). The
    /// turn's calls end with it, so a call that never reported back takes no later block. Subagents run
    /// past their session's turn, so their own blocks stay.
    fn unblock(&mut self) {
        self.blocks.retain(|w| w.named && w.agent.is_some());
        self.running.retain(|c| c.agent.is_some());
        self.asked = false;
    }
}

/// Tools that ask Daniel in any permission mode, before any `PermissionRequest`.
const ASKING_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

/// Whether hook event `name` is a start, an end or a notice: one that never starts a turn or
/// touches a worktree, since in the day log they carry ids no turn ran under.
fn outside_turns(name: &str) -> bool {
    matches!(name, "SessionStart" | "SessionEnd" | "Notification")
}

/// What blocks a session on Daniel, so that it needs you (#434, decisions 24 to 26): what asks,
/// and the calls it holds.
#[derive(Clone, Debug, PartialEq)]
struct Block {
    tool: String,
    input: Value,
    /// The agent that asks: a subagent, or `None` for the session's own agent or a notice that names
    /// none.
    agent: Option<String>,
    /// Whether the event named its agent. A notice with no agent may be any running agent's.
    named: bool,
    /// The calls it holds, by `tool_use_id`, each with its agent; it clears when the last
    /// resolves.
    calls: Vec<(String, Option<String>)>,
    at: u64,
    /// A permission notice already echoed it, so the next notice is another agent's.
    noticed: bool,
}

impl Block {
    /// Whether it waits on tool call `call`: a permission request on the call it names, of its own
    /// agent; an MCP server's request for input on an MCP tool; a permission notice on any call
    /// but an open spawn, which asks for no permission while its subagent runs.
    fn takes(&self, call: &Call) -> bool {
        let tool = match self.tool.as_str() {
            "Elicitation" => call.tool.starts_with("mcp__"),
            "Notification" => call.tool != "Agent",
            tool => tool == call.tool && self.input == call.input,
        };
        tool && (!self.named || self.agent == call.agent)
    }

    /// Who it names: its agent, or with no agent named, the agent of its one call. `None` is the
    /// session's own agent, or no name when several agents may ask.
    fn asker(&self) -> Option<String> {
        match &self.calls[..] {
            _ if self.named => self.agent.clone(),
            [(_, agent)] => agent.clone(),
            _ => None,
        }
    }
}

/// A session's state (#369, decision 4). Each has a named clearer: an event, or the tick for idle.
/// The order is the session list's: needs you first, then the groups the list collapses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Waiting on Daniel (#328, module 3, "Needs you"), until the step that asks resolves, a prompt
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

/// 2 hours, a starting guess (#369, decision 4).
const IDLE_AFTER_MS: u64 = 2 * 60 * 60 * 1000;

/// A tool call the agent made, as the "now" line names it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub tool: String,
    /// What it works on (`step_detail`): a command's `description`, else its first line.
    pub detail: Option<String>,
    /// A command's full text, one click away in the session view (#461, decision 12).
    #[serde(default)]
    pub command: Option<String>,
}

impl Step {
    fn new(tool: &str, input: &Value, cwd: &str) -> Step {
        Step {
            tool: tool.into(),
            detail: step_detail(input, cwd),
            command: input["command"].as_str().map(Into::into),
        }
    }
}

/// What the shell must act on after a live event.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum Output {
    /// A session's or a worker's worktree changed since its last card, or a session's workers'
    /// did; ask the card writer.
    Card(CardRequest),
    /// A session woken by its workers changed nothing itself since its last card: its brief only
    /// takes in its workers' cards (#369, decision 11).
    Brief(CardRequest),
    /// A session newly needs Daniel: its own agent asks, or `agent`, a subagent at any depth, whose
    /// notice goes on its session's line (#369, decision 22).
    NeedsYou {
        session_id: String,
        agent: Option<String>,
    },
    /// Worker `agent` of a session, at any depth, newly failed. A failed helper raises none.
    Failed { session_id: String, agent: String },
    /// Helper `agent` became a worker, which the card store keeps, since a replay takes no
    /// fingerprint to see its command's change (#434, decision 37).
    Promoted { agent: String },
    /// A prompt named a known task id in markup the studio does not know: drift in Claude Code's
    /// wake-ups, for stderr (#369, decision 7).
    Unrecognised { session_id: String, task_id: String },
    /// A session's first `Stop`: read Claude Code's version from its transcript, and nothing else
    /// (#369, decision 9).
    Version { transcript_path: String },
}

/// What the live app learned that the event log does not hold, which the card store keeps as it
/// happens and the core applies after the replay (#434, decision 37).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LiveFacts {
    /// The helpers promoted to workers (`Output::Promoted`).
    #[serde(default)]
    pub promoted: BTreeSet<String>,
    /// The failed lines Daniel opened (`Studio::open`).
    #[serde(default)]
    pub opened: BTreeSet<String>,
}

/// A session's change since its last card (#369, decision 10), with what the card writer needs: the
/// diff, not the payloads. The card store keeps it with its card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CardRequest {
    pub session_id: String,
    /// Names the card: its worker, else its session, `@`, and when the hook received the event it
    /// was written at.
    pub id: String,
    pub base: Base,
    /// The diff of each file that changed since the base, commits included. The core gives none on
    /// a card built from `workers`' cards; the understanding layer fills it with their text before
    /// it asks the writer and stores the card.
    #[serde(default)]
    pub change: Option<String>,
    /// When the hook received the prompt of Daniel's that the card belongs to: its session's latest
    /// prompt that is not a wake-up; for a worker's card, when the worker started. `None` in cards
    /// stored before #377.
    #[serde(default)]
    pub prompt: Option<u64>,
    /// The worker whose card it is, `None` for its session's own.
    #[serde(default)]
    pub agent: Option<String>,
    /// The files other subagents edited or wrote in its worktree, by subagent (#369, story 20).
    #[serde(default)]
    pub changed_by: BTreeMap<String, BTreeSet<String>>,
    /// A session's workers' cards since its last card, at any depth.
    #[serde(default)]
    pub workers: Vec<String>,
    /// The command of the check whose start requested it, which the card is on (#461, decision 15);
    /// `None` for any other card.
    #[serde(default)]
    pub check: Option<String>,
}

/// A turn that ended with no card, as the replay saw it (#486): its card request with no change,
/// and its worktrees' trees, whose diffs the shell joins into the change.
#[derive(Clone, Debug, PartialEq)]
pub struct Backfill {
    pub request: CardRequest,
    pub trees: Vec<Trees>,
}

/// The git trees the hook recorded for worktree `root` at a turn's start and end, which differ.
#[derive(Clone, Debug, PartialEq)]
pub struct Trees {
    pub root: String,
    pub start: String,
    pub end: String,
}

impl CardRequest {
    /// The change the writer reads: the diff, or the workers' text; empty when there is none.
    pub fn diff(&self) -> &str {
        self.change.as_deref().unwrap_or_default()
    }

    /// Whose card it is, and whose brief it updates: its worker's id, else its session's.
    pub fn owner(&self) -> &str {
        self.agent.as_deref().unwrap_or(&self.session_id)
    }
}

/// A worktree the shell fingerprints before it applies a live line, and the card stream it is for:
/// `key` is a session's id, or a subagent's.
#[derive(Clone, Debug, PartialEq)]
pub struct Fingerprint {
    pub key: String,
    pub folder: String,
    pub purpose: Purpose,
}

/// A check that runs or ran, shown as a pass or fail line once it ends.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheckLine {
    /// The command's first line.
    pub command: String,
    /// Whether the command exited 0, when that says whether the check passed: nothing in the
    /// command masks the check's exit code (`exit_code_counts`). `None` otherwise, and while it runs.
    pub passed: Option<bool>,
    /// When it started, and when it ended: its result, or for a command in the background its
    /// `<task-notification>`; `None` while it runs (#461, decision 13).
    pub started_at: u64,
    pub ended_at: Option<u64>,
    /// Its typical duration when it started, from the checks that ran before (`Studio::typical`).
    pub typical_ms: Option<u64>,
    /// The card its start requested, when it usually takes over 2 minutes (#461, decision 15),
    /// whose guess its result settles.
    pub card: Option<String>,
    /// Its call's `tool_use_id`, and its background task's id once it moved to the background.
    #[serde(skip)]
    call: String,
    #[serde(skip)]
    task: Option<String>,
    /// Whether its exit code says whether it passed (`exit_code_counts`).
    #[serde(skip)]
    counts: bool,
}

impl CheckLine {
    fn new(command: &str, call: &str, at: u64, typical_ms: Option<u64>) -> Self {
        CheckLine {
            command: command.lines().next().unwrap_or_default().into(),
            passed: None,
            started_at: at,
            ended_at: None,
            typical_ms,
            card: None,
            call: call.into(),
            task: None,
            counts: exit_code_counts(command),
        }
    }

    /// It ended at `at`, failed or not, which its typical duration counts only when it did not fail.
    fn end(&mut self, failed: bool, at: u64, durations: &mut HashMap<String, Vec<u64>>) {
        self.ended_at = Some(at);
        self.passed = self.counts.then_some(!failed);
        if !failed {
            let runs = durations.entry(check_key(&self.command)).or_default();
            runs.push(at.saturating_sub(self.started_at));
        }
    }
}

/// What typical durations group check runs by: the command's first line, up to any pipe or output
/// redirection, so `cargo test 2>&1 | tail` counts with `cargo test`.
fn check_key(command: &str) -> String {
    let first = command.lines().next().unwrap_or_default();
    let first = first.split('|').next().unwrap_or_default();
    let first = first.replace("2>&1", "");
    first.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The lower median of `runs`, `None` when there is none.
fn median(runs: &[u64]) -> Option<u64> {
    let mut runs = runs.to_vec();
    runs.sort_unstable();
    runs.get(runs.len().checked_sub(1)? / 2).copied()
}

/// The worktree a card's change starts from (#369, decision 10): the session's previous card, or its
/// prompt.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Base {
    /// `HEAD`'s sha, or `None` before the first commit.
    pub head: Option<String>,
    /// The files `git status` listed, whose content the fingerprint kept in memory.
    pub files: Vec<String>,
}

/// A worktree's change since its base, as the shell's fingerprint finds it.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub base: Base,
    /// The diff of each file that changed, commits included.
    pub diff: String,
    /// `HEAD`'s sha now, so that a commit since the base shows as `head` other than `base.head`.
    pub head: Option<String>,
}

/// What a fingerprint is for (#410, decision 23). Its record becomes the next change's base for
/// every purpose but `Commit`, which keeps it only when `HEAD` moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// A base, whose change is never a card: a turn's first touch of a worktree, a subagent's first
    /// folder, a helper's look before its command, and the start-up bases.
    Base,
    /// After a worker's or a helper's command: a card only when `HEAD` moved; the change it saw
    /// otherwise stays for the card at its next commit, its end or its fallback.
    Commit,
    /// A card when the worktree changed, such as a worker's end, or a session's change between its
    /// turns (#434, decisions 16 and 17).
    Card,
    /// 20 minutes or more after the worktree's last card or its last quiet fallback: a card when
    /// it changed, else a new 20-minute wait (#369, decision 11, as #410 changes it).
    Fallback,
    /// A turn's end, at its `Stop` or without one (#434, decision 5): its change, or else its
    /// workers' cards, makes its card or brief.
    TurnEnd,
}

/// One tool call: its `tool_use_id`, and its tool and input, which are all `PermissionRequest`
/// names it by. Also the subagent that made it, `None` for the session's own agent.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    id: Option<String>,
    tool: String,
    input: Value,
    agent: Option<String>,
}

/// Who a session that needs Daniel waits for: its oldest block's asker, a subagent or `None` for the
/// session's own agent, and since when, in Unix milliseconds (#369, decision 21).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Asker {
    pub agent: Option<String>,
    pub since: u64,
}

/// The four fields of a tool response the core reads (#410, decision 20). The rest, file contents
/// included, is skipped without building a JSON tree.
#[derive(Default, Deserialize)]
struct ToolResponse {
    #[serde(rename = "backgroundTaskId")]
    background_task_id: Option<String>,
    /// On an `Agent` call's `PostToolUse`, the child's id and status.
    #[serde(rename = "agentId")]
    agent_id: Option<String>,
    status: Option<String>,
    #[serde(default)]
    answers: Value,
}

/// A response that is not a JSON object yields none of the four, and keeps its event.
fn tool_response<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ToolResponse, D::Error> {
    let raw = <&serde_json::value::RawValue>::deserialize(d)?;
    Ok(serde_json::from_str(raw.get()).unwrap_or_default())
}

/// The fields of a hook payload the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Event {
    session_id: String,
    hook_event_name: String,
    /// The subagent that sent the event, or `None` for the session's own agent.
    agent_id: Option<String>,
    agent_type: Option<String>,
    #[serde(default)]
    prompt: String,
    /// On `Stop`, the session's running tasks.
    #[serde(default)]
    background_tasks: Value,
    #[serde(default)]
    cwd: String,
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
    #[serde(default, deserialize_with = "tool_response")]
    tool_response: ToolResponse,
    /// A failed call's error, which for a command that ran holds its exit code and output.
    #[serde(default)]
    error: String,
    #[serde(default)]
    last_assistant_message: String,
    /// The session's transcript, whose lines carry Claude Code's version.
    #[serde(default)]
    transcript_path: String,
    #[serde(default)]
    notification_type: String,
    /// Claude Code's id of the prompt the event belongs to, absent until the first user input.
    prompt_id: Option<String>,
    /// The start of a session: `startup`, `resume`, `clear` or `compact`.
    #[serde(default)]
    source: String,
    /// The end of a session: `clear`, `logout`, `prompt_input_exit`, `other` and the like.
    #[serde(default)]
    reason: String,
    #[serde(default)]
    studio_received_ms: u64,
    /// On a tool's result, how long its call took.
    duration_ms: Option<u64>,
    /// The terminal the session runs in, which the hook records from `STUDIO_TERMINAL_ID`.
    studio_terminal_id: Option<String>,
    /// The trees the hook recorded at a turn's first touch of each worktree, by root (#486).
    #[serde(default)]
    studio_turn_start: BTreeMap<String, String>,
    /// The trees the hook recorded at the end of the turn this event ends, by root.
    #[serde(default)]
    studio_turn_end: BTreeMap<String, String>,
    /// On the studio's own `StudioRename` line, the session's new name; empty for none.
    #[serde(default)]
    studio_name: String,
    /// On the answer hook's `StudioAnswer` line, Daniel's answer from the card: `answers` for a
    /// question, `decision` for a plan or a permission.
    #[serde(default)]
    studio_answer: Value,
}

/// The event the shell appends to the log when Daniel renames a session (#469). The log is what a
/// restart replays, so the name survives it.
pub const RENAME: &str = "StudioRename";

/// The event the shell appends to the log when Daniel deletes a session: its row leaves the list,
/// at once and after a restart, until a `SessionStart` of it brings it back. Its cards stay.
pub const DELETE: &str = "StudioDelete";

/// The line the answer hook appends when Daniel answers from the studio card (#478): the call's hook
/// payload under this name, with `studio_asked_by` (the event that asked) and `studio_answer`. An
/// answer in the terminal leaves none.
pub const STUDIO_ANSWER: &str = "StudioAnswer";

/// Whether the turn's final message asks Daniel something: a sentence ending in `?`, after removing
/// fenced code, inline code, blockquotes, headings and table rows (#321's evidence, "Transcripts").
pub fn asks(message: &str) -> bool {
    let mut in_fence = false;
    message.lines().any(|line| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            return false;
        }
        // Inline code sits between pairs of backticks; an unpaired last one starts no code.
        let parts: Vec<&str> = line.split('`').collect();
        let prose: String = parts
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0 || i + 1 == parts.len())
            .map(|(_, part)| *part)
            .collect();
        let prose = prose
            .trim()
            .trim_end_matches([' ', '\t', '*', '_', ')', '"', '\'', '”', '’']);
        // A heading is `#`s and a space, so a line that opens with an issue number such as `#341` is
        // prose.
        let heading = prose.starts_with('#') && prose.trim_start_matches('#').starts_with(' ');
        !in_fence
            && !heading
            && !prose.starts_with(['>', '|'])
            && prose.split_whitespace().any(|word| word.ends_with('?'))
    })
}

/// The tasks a wake-up reports on, each with how it ended: its `<task-id>` and its `<status>`
/// (#370 note).
fn reported(prompt: &str) -> impl Iterator<Item = (&str, &str)> {
    let notices = prompt.split("<task-notification>").skip(1);
    notices.filter_map(|notice| Some((tag(notice, "task-id")?, tag(notice, "status")?)))
}

/// The text of the first `<name>` element in `text`.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(&format!("<{name}>"))?;
    Some(rest.split_once(&format!("</{name}>"))?.0)
}

/// The tool input that says what a step works on: a command's `description`, which the agent sets
/// on every Bash call (#461, decision 12), else its first line; a path relative to the session's
/// folder, the first question asked, or a search pattern, URL, query or task description.
fn step_detail(input: &Value, cwd: &str) -> Option<String> {
    if let Some(command) = input["command"].as_str() {
        if let Some(description) = input["description"]
            .as_str()
            .filter(|d| !d.trim().is_empty())
        {
            return Some(description.into());
        }
        return command.lines().next().map(Into::into);
    }
    if let Some(question) = input["questions"][0]["question"].as_str() {
        return Some(question.into());
    }
    if let Some(path) = input["file_path"].as_str() {
        return Some(relative(path, cwd).into());
    }
    ["pattern", "url", "query", "description"]
        .iter()
        .find_map(|key| input[key].as_str())
        .map(Into::into)
}

/// `path` relative to `folder`, or else `path` as it is.
fn relative<'a>(path: &'a str, folder: &str) -> &'a str {
    let relative = path.strip_prefix(folder).and_then(|p| p.strip_prefix('/'));
    relative.unwrap_or(path)
}

/// A command that runs a check from the check list (#328, module 3): `cargo
/// test|nextest|clippy|check|build`, `ci-checks`, `go test|vet`, with any flags or toolchain before
/// the subcommand (`cargo +nightly test`). Quoted text is not run, so a PR body that names `cargo
/// test` is no check.
// shortcut: every session uses the Falinks list; take each project's list from the shell once
// projects exist.
fn is_check(command: &str) -> bool {
    let mut quote = None;
    let unquoted: String = command
        .chars()
        .map(|c| {
            match quote {
                None if c == '\'' || c == '"' => quote = Some(c),
                None => return c,
                Some(open) if c == open => quote = None,
                Some(_) => {}
            }
            ' '
        })
        .collect();
    let words: Vec<&str> = unquoted
        .split(|c: char| c.is_whitespace() || ";&|()".contains(c))
        .filter(|word| !word.is_empty())
        .collect();
    words
        .iter()
        .any(|word| word.rsplit('/').next() == Some("ci-checks"))
        || words.iter().enumerate().any(|(i, word)| {
            let subcommand = words[i + 1..].iter().find(|w| !w.starts_with(['-', '+']));
            matches!(
                (*word, subcommand.copied()),
                (
                    "cargo",
                    Some("test" | "nextest" | "clippy" | "check" | "build")
                ) | ("go", Some("test" | "vet"))
            )
        })
}

/// Whether a check's exit code says whether it passed (#328, module 4). A command exits with its
/// last statement's code, so the check must be the last statement, and nothing in it may mask the
/// code: `|| true` exits 0, `!` flips it, `&` runs the check in the background, and a pipe exits
/// with its last command's code unless an earlier `set` turns on `pipefail`.
fn exit_code_counts(command: &str) -> bool {
    let statements = command.split([';', '\n']).map(str::trim);
    let Some(last) = statements.clone().rfind(|s| !s.is_empty()) else {
        return false;
    };
    let pipefail = statements
        .flat_map(|s| s.split("&&"))
        .any(|s| s.trim().starts_with("set ") && s.contains("pipefail"));
    let background = ["&&", ">&", "&>"]
        .iter()
        .fold(last.to_string(), |rest, and| rest.replace(and, ""))
        .contains('&');
    is_check(last)
        && !last.contains("||")
        && !last.starts_with('!')
        && !background
        && (pipefail || !last.contains('|'))
}

impl Studio {
    /// A studio that finds a folder's worktree root with `find` (#434, decision 11), such as the
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

    /// The worktree roots that `event` of a session's own agent touches (#434, decision 9): its
    /// folder's, and an `Edit` or `Write` path's, but those of its running workers with cards. A
    /// start, an end or a notice touches none.
    // shortcut: a Bash write through a path, without a `cd`, is not seen; parse commands for paths
    // if a live run shows cards missing such writes.
    fn touches(&self, event: &Event) -> Vec<String> {
        let name = event.hook_event_name.as_str();
        if event.agent_id.is_some() || outside_turns(name) {
            return Vec::new();
        }
        let tool = event.tool_name.as_deref();
        let path = event.tool_input["file_path"].as_str();
        let path = path.filter(|p| p.starts_with('/') && matches!(tool, Some("Edit" | "Write")));
        let folder = path.and_then(|p| std::path::Path::new(p).parent()?.to_str());
        // With no lookup, a path under the event's folder is in its worktree.
        let below = |f: &&str| f.starts_with(&format!("{}/", event.cwd));
        let folder = folder.filter(|f| self.roots.find.is_some() || !below(f));
        let session = self.sessions.get(&event.session_id);
        let mut roots: Vec<String> = Vec::new();
        for folder in [Some(event.cwd.as_str()), folder].into_iter().flatten() {
            let root = self.roots.of(folder);
            if !root.is_empty()
                && !roots.contains(&root)
                && !session.is_some_and(|s| s.workers_own(&root))
            {
                roots.push(root);
            }
        }
        roots
    }

    /// The worktrees the shell fingerprints before it applies this live line (#328, module 5), each
    /// with its own key: none, one, or at a wake-up also its workers' ends (#369, decisions 6, 8, 10
    /// and 11).
    pub fn fingerprint_folders(&self, line: &str) -> Vec<Fingerprint> {
        let event = serde_json::from_str::<Event>(line);
        let event = event
            .ok()
            .filter(|e| ![RENAME, DELETE].contains(&e.hook_event_name.as_str()));
        event.map_or_else(Vec::new, |mut event| {
            event.session_id = self.row(&event);
            let long_check = self.check_start(&event).is_some_and(|start| start.long());
            self.looks(&event, long_check)
        })
    }

    /// The row `event` belongs to: its session's, or for a start from `/clear`, the cleared session's
    /// on the same terminal, or in the same folder outside studio terminals (#461, decision 18).
    // shortcut: a start that lands before its clear's `SessionEnd` makes a new row; match both ways
    // if the async hooks show it.
    fn row(&self, event: &Event) -> String {
        if let Some(row) = self.rows.get(&event.session_id) {
            return row.clone();
        }
        let fresh = !self.sessions.contains_key(&event.session_id);
        if fresh && event.hook_event_name == "SessionStart" && event.source == "clear" {
            let same = |s: &&Session| match &event.studio_terminal_id {
                Some(terminal) => s.terminal.as_ref() == Some(terminal),
                None => s.terminal.is_none() && s.cwd == event.cwd,
            };
            let cleared = self.sessions.values().filter(|s| s.clearing).filter(same);
            if let Some(s) = cleared.max_by_key(|s| s.agent.last_event_at) {
                return s.agent.id.clone();
            }
        }
        event.session_id.clone()
    }

    /// The fingerprints before `event`: its own (`look`), and the end of each worker with cards that
    /// it ends, by any signal (#434, decision 19).
    fn looks(&self, event: &Event, long_check: bool) -> Vec<Fingerprint> {
        let mut looks = self.look(event, long_check);
        let Some(session) = self.sessions.get(&event.session_id) else {
            return looks;
        };
        for (id, ..) in session.ends(event) {
            let Some(agent) = session.subagent(&id) else {
                continue;
            };
            // One in its session's worktree, outside a turn, ends with the session's card (#434,
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
                }),
            }
        }
        looks
    }

    /// The fingerprints before `event`. A session's own, one per worktree (#434, decisions 8 to
    /// 10): at a turn's first touch of a worktree, its base, so what Daniel changed himself before
    /// does not count; at a turn's end, its `Stop` or the next turn's first event (#434, decision 5),
    /// every worktree it touched, whose change since its base is the card's, and whose record is
    /// the next turn's base. A turn that starts after the last one ended first cards what its
    /// subagents changed between them (decision 17). A subagent's: its first event's, its base; a
    /// helper's before and after each Bash step, for a change across it that makes it a worker; a
    /// worker's after each Bash step, for a commit; and a worker's end. On a step 20 minutes after
    /// the last card of the worktree it is in, that worktree's, for the fallback card; a session's
    /// only in a turn. A helper's look after its Bash step comes beside such a fallback's.
    fn look(&self, event: &Event, long_check: bool) -> Vec<Fingerprint> {
        let name = event.hook_event_name.as_str();
        let step = matches!(name, "PostToolUse" | "PostToolUseFailure");
        let due = |since: &Since| step && event.studio_received_ms >= since.at + FALLBACK_MS;
        let session = self.sessions.get(&event.session_id);
        // A session's own fallback is on a prompt's steps only, not between its turns.
        let session_due = |s: &Session| s.in_turn() && due(&s.agent.since);
        let look = |key: &str, folder: &str, purpose| {
            let look = Fingerprint {
                key: key.into(),
                folder: folder.into(),
                purpose,
            };
            (!folder.is_empty()).then_some(look)
        };
        let Some(id) = &event.agent_id else {
            let blank = Session::default();
            let s = session.unwrap_or(&blank);
            let key = &event.session_id;
            let ends = s.ends_turn_early(event) || name == "Stop";
            let opens = s.opens_turn(event);
            // A turn's end, and a due fallback, look at every worktree it touched (#434, decision
            // 10).
            let mut closing = s.closing();
            // A turn seen only from its end, such as one the log starts in, ends where it ends.
            if closing.is_empty() && ends {
                closing = vec![self.roots.of(&event.cwd)];
            }
            let purpose = match ends {
                true => Some(Purpose::TurnEnd),
                // A long check's start cards the change so far, as a fallback would (#461,
                // decision 15).
                false => {
                    (session.is_some_and(session_due) || long_check).then_some(Purpose::Fallback)
                }
            };
            let mut looks: Vec<Fingerprint> = purpose
                .into_iter()
                .flat_map(|purpose| closing.iter().filter_map(move |r| look(key, r, purpose)))
                .collect();
            // Each worktree a turn touches first gets its base, which a turn's end just took. A
            // turn that starts after the last one ended first cards what subagents changed
            // between them (#434, decision 17).
            if opens || s.in_turn() && name != "Stop" {
                let between = match opens && !s.in_turn() {
                    true => s.between_turns(event.studio_received_ms),
                    false => Vec::new(),
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
        let Some(folder) = agent.and_then(|a| a.folder.as_deref()) else {
            return look(id, &event.cwd, Purpose::Base).into_iter().collect();
        };
        let worker = |a: &Subagent| a.kind == Kind::Worker;
        let bash = event.tool_name.as_deref() == Some("Bash");
        // A helper's base right before its command, so only a change across it promotes it.
        // shortcut: the hooks are async, so a command that ends before the shell reads its
        // `PreToolUse` line is absorbed into this base and never promotes its helper; make the
        // hook synchronous if a live run shows missed promotions matter (#434, decision 23).
        if bash && name == "PreToolUse" && agent.is_some_and(|a| !worker(a)) {
            return look(id, folder, Purpose::Base).into_iter().collect();
        }
        let bash = step && bash;
        let owner = agent.map_or_default(|a| a.owner.as_str());
        let since = &session.record(owner).since;
        let fallback: Vec<Fingerprint> = match session.subagent(owner) {
            Some(own) if &own.agent.id == id => match worker(own) {
                true if due(since) => look(id, folder, Purpose::Fallback),
                _ => look(id, folder, Purpose::Commit).filter(|_| bash),
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
        // A due fallback's look comes beside a helper's own (#434, decision 22).
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
        let Ok(mut event) = serde_json::from_str::<Event>(line) else {
            return Vec::new();
        };
        let own_id = event.session_id.clone();
        event.session_id = self.row(&event);
        let continues = event.session_id != own_id && !self.rows.contains_key(&own_id);
        if event.session_id != own_id {
            self.rows.insert(own_id.clone(), event.session_id.clone());
        }
        // A rename names a known session and is none of its events.
        if event.hook_event_name == RENAME {
            if let Some(session) = self.sessions.get_mut(&event.session_id) {
                session.name = Some(event.studio_name).filter(|name| !name.is_empty());
            }
            return Vec::new();
        }
        // So does a delete, which also drops the selection when it was this session.
        if event.hook_event_name == DELETE {
            if let Some(session) = self.sessions.get_mut(&event.session_id) {
                session.deleted = true;
            }
            if self.selected.as_deref() == Some(&event.session_id) {
                self.selected = None;
                self.viewed = None;
            }
            return Vec::new();
        }
        let start = self.check_start(&event);
        let long_check = start.as_ref().filter(|start| start.long());
        let looks = self.looks(&event, long_check.is_some());
        let long_check = long_check.map(|start| start.command.clone());
        // The card a long check's start requests, which its line keeps.
        let mut check_card = None;
        let touches = self.touches(&event);
        let typical = self.typical(event.tool_input["command"].as_str().unwrap_or_default());
        let roots = &self.roots;
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
        // A deleted session that starts again, by a resume, is back in the list; a late end is not.
        session.deleted &= event.hook_event_name != "SessionStart";
        // The hooks are async, so the latest event can land before an earlier one.
        session.agent.last_event_at = session.agent.last_event_at.max(event.studio_received_ms);
        // Every event of a `claude` process carries its terminal, so a resume elsewhere moves it.
        session.terminal.clone_from(&event.studio_terminal_id);
        let working = session.state == State::Running;
        session.work.passes(event.studio_received_ms, working);
        let phases_before = session.work.phases.len();
        session.resume_id = own_id;
        if continues {
            session.cleared.push(event.studio_received_ms);
        }
        let failed_before = session.failed_workers();
        let in_turn = session.in_turn();
        let ends_early = session.ends_turn_early(&event);
        let starts = session.starts_turn(&event);
        let opens = session.opens_turn(&event);
        let helpers: Vec<String> = session
            .agents
            .iter()
            .filter(|a| a.kind == Kind::Helper)
            .map(|a| a.agent.id.clone())
            .collect();
        let its_own = |id: &String| changes.contains_key(id);
        let changed = event.agent_id.as_ref().is_some_and(its_own);
        session.track_agents(&event, changed, roots);
        // A child's events still keep its session from going idle, but its folder and steps are its
        // own.
        let own = event.agent_id.is_none();
        if own && !event.cwd.is_empty() {
            session.cwd.clone_from(&event.cwd);
            let last = event.cwd.rsplit('/').next().unwrap_or_default();
            session.folder_name = if last.is_empty() { &event.cwd } else { last }.into();
            let root = roots.of(&event.cwd);
            let last = root.rsplit('/').next().unwrap_or_default();
            session.identity.worktree = if last.is_empty() { &root } else { last }.into();
            let command = event.tool_input["command"].as_str().unwrap_or_default();
            if let Some(issue) = claimed(command) {
                session.identity.claim = Some(issue);
            }
            session.identity.named();
        }
        // A resume or `/clear` may move it to another transcript, read from its start.
        if own
            && !event.transcript_path.is_empty()
            && session.transcript_file.path != event.transcript_path
        {
            session.transcript_file = transcript::TranscriptFile::default();
            session
                .transcript_file
                .path
                .clone_from(&event.transcript_path);
            session.transcript = Transcript::default();
        }
        let failed = session.failed_workers().into_iter();
        let mut outputs: Vec<Output> = failed
            .filter(|agent| !failed_before.contains(agent))
            .map(|agent| Output::Failed {
                session_id: event.session_id.clone(),
                agent,
            })
            .collect();
        let worker = |id: &String| session.subagent(id).is_some_and(|a| a.kind == Kind::Worker);
        let promoted = helpers.into_iter().filter(worker);
        outputs.extend(promoted.map(|agent| Output::Promoted { agent }));
        let mut blocked_before = session.needs_you();
        let tool = event.tool_name.unwrap_or_default();
        let id = Some(event.tool_use_id.clone()).filter(|id| !id.is_empty());
        let at = event.studio_received_ms;
        let name = event.hook_event_name.as_str();
        // The turn this event ends, then the one it starts, with the prompts queued for it.
        if ends_early {
            let change = changes.remove(&session.agent.id);
            outputs.extend(session.end_turn(change, at));
            if replay {
                self.ended
                    .extend(session.backfill(&event.studio_turn_end, at));
            }
        }
        if starts {
            session.turn.clone_from(&event.prompt_id);
            session.reopen_turn();
            session.woken = false;
            let queued = std::mem::take(&mut session.queued);
            for (index, prompt) in queued.into_iter().enumerate() {
                session.take_prompt(prompt, index == 0);
            }
        }
        if opens {
            session.turn_span = Some(Span {
                start: at,
                end: None,
            });
            session.touched.clear();
            session.trees.clear();
        }
        if own {
            for (root, tree) in &event.studio_turn_start {
                session
                    .trees
                    .entry(root.clone())
                    .or_insert_with(|| tree.clone());
            }
        }
        // Who changed what (#369, story 20): a subagent's edit in a worktree whose cards are another's.
        if let Some(agent) = &event.agent_id
            && name == "PostToolUse"
            && matches!(tool.as_str(), "Edit" | "Write")
            && let Some(path) = event.tool_input["file_path"].as_str()
        {
            let (owner, root) = session
                .subagent(agent)
                .map_or_default(|a| (a.owner.clone(), a.root.clone()));
            // Only a path inside the owner's worktree (#434, decision 15).
            let inside = path.strip_prefix(&root).and_then(|p| p.strip_prefix('/'));
            if owner != *agent
                && let Some(inside) = inside.filter(|_| !root.is_empty())
            {
                let by = &mut session.record_mut(&owner).since.by;
                by.entry(agent.clone()).or_default().insert(inside.into());
            }
        }
        // A worker's card at its commit, seen after its Bash step, or at its end, and any worktree's
        // after 20 minutes: when its worktree changed since its last card (#369, decision 11). The
        // session's own look at a prompt is its base, and at `Stop` its card's change, below.
        for look in &looks {
            let found = match look.purpose {
                Purpose::Base | Purpose::TurnEnd => continue,
                Purpose::Commit => changes.remove(&look.key).filter(|c| c.head != c.base.head),
                Purpose::Card | Purpose::Fallback => changes.remove(&look.key),
            };
            let Some(found) = found else {
                // A quiet fallback restarts its worktree's 20-minute wait.
                if look.purpose == Purpose::Fallback {
                    session.record_mut(&look.key).since.at = at;
                }
                continue;
            };
            let worker = look.key != session.agent.id;
            // A session's card between turns is tagged with the subagents that ran then (#434,
            // decisions 16 and 17).
            if !worker && look.purpose == Purpose::Card {
                for agent in session.between_turns(at) {
                    session.agent.since.by.entry(agent).or_default();
                }
            }
            if !worker || session.has_cards(&look.key) {
                let mut request = session.card(&look.key, Some(found), at);
                // At a long check's start, the session's own look is the check's card.
                if !worker && long_check.is_some() {
                    request.check.clone_from(&long_check);
                    check_card.get_or_insert_with(|| request.id.clone());
                }
                if worker {
                    session.agent.since.cards.push(request.id.clone());
                }
                outputs.push(Output::Card(request));
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
        match name {
            // A prompt opens a wait, also after an interrupt, which fires no `Stop`.
            "UserPromptSubmit" => {
                let sender = session.sender(&event.prompt);
                let wake_up = !sender.is_daniels();
                match &sender {
                    // The row says which worker returned until the session's next step.
                    Sender::WakeUp(task_id) => {
                        let returned = format!("worker {} returned", session.agent_name(task_id));
                        session.returned = Some(returned);
                    }
                    Sender::Unrecognised(task_id) => outputs.push(Output::Unrecognised {
                        session_id: event.session_id.clone(),
                        task_id: task_id.clone(),
                    }),
                    Sender::Daniel => {}
                }
                // A background check ends at the notification that names its task.
                for (task, status) in reported(&event.prompt) {
                    let running = session.checks.iter_mut().filter(|l| l.ended_at.is_none());
                    let mut lines = running.filter(|l| l.task.as_deref() == Some(task));
                    if let Some(line) = lines.next() {
                        line.end(status != "completed", at, &mut self.durations);
                        if !matches!(status, "completed" | "failed") {
                            line.passed = None;
                        }
                    }
                }
                session.work.prompt(&event.prompt, at);
                let prompt = Prompt {
                    at,
                    sender,
                    text: event.prompt,
                };
                // A prompt with its running turn's id is the next turn's (#434, decision 4).
                if event.prompt_id.is_some() && !starts && in_turn {
                    session.queued.push(prompt);
                } else {
                    session.take_prompt(prompt, starts || !in_turn);
                    // Daniel's prompt answers what asked him; a wake-up is not his, so it answers
                    // nothing.
                    if !wake_up {
                        session.returned = None;
                        session.unblock();
                    }
                    session.reopen_turn();
                }
            }
            "Stop" => {
                // The order on `Stop`: clear its own blocks first, then ask if the message asks.
                let in_turn = session.in_turn();
                let tasks = event.background_tasks.as_array();
                session.background = tasks.is_some_and(|tasks| !tasks.is_empty());
                let output = session.end_turn(changes.remove(&session.agent.id), at);
                if replay && in_turn {
                    self.ended
                        .extend(session.backfill(&event.studio_turn_end, at));
                }
                // A subagent's block outlasts the turn, so it gives no second notice.
                blocked_before = session.needs_you();
                session.asked = asks(&event.last_assistant_message);
                if !session.versioned && !event.transcript_path.is_empty() {
                    session.versioned = true;
                    outputs.push(Output::Version {
                        transcript_path: event.transcript_path,
                    });
                }
                let mut lines = event.last_assistant_message.lines().map(str::trim);
                session.stopped = Some(Stopped {
                    card: card_id(&event.session_id, at),
                    requested: matches!(output, Some(Output::Card(_))),
                    message: lines.find(|line| !line.is_empty()).map(Into::into),
                });
                outputs.extend(output);
            }
            "SessionEnd" => {
                session.unblock();
                // Its subagents end with it.
                session.blocks.clear();
                session.running.clear();
                if session.in_turn() {
                    session.close_spans(at);
                }
                session.turn_ended = true;
                session.ended = true;
                session.clearing = event.reason == "clear";
            }
            // A resumed session keeps its id; a compaction keeps the turn it happens in.
            "SessionStart" => {
                session.ended = false;
                session.clearing = false;
                session.turn_ended |= event.source != "compact";
            }
            "PreToolUse" => {
                if own {
                    session.returned = None;
                    let step = Step::new(&tool, &event.tool_input, &event.cwd);
                    session.agent.step(step, &event.tool_use_id, at);
                    session.work.call(&tool, &event.tool_input, at);
                    // A check's line runs from its start (#461, decision 13).
                    if let Some(start) = &start {
                        let call = &event.tool_use_id;
                        let mut line = CheckLine::new(&start.command, call, at, start.typical);
                        line.card = check_card.take();
                        session.checks.push(line);
                    }
                }
                let call = Call {
                    id,
                    tool,
                    input: event.tool_input,
                    agent: event.agent_id.clone(),
                };
                if call.tool == "AskUserQuestion" {
                    session.questions.push(Question {
                        tool_use_id: event.tool_use_id.clone(),
                        agent: call.agent.clone(),
                        questions: call.input["questions"].clone(),
                        answer: Answer::Open,
                    });
                }
                // The hooks are async, so a block on this call may have landed first.
                // shortcut: a block whose call ended before the block landed stays until the turn
                // ends; remember ended calls if auto-denied calls (#322) show it.
                if let Some(id) = &call.id {
                    let unbound = session.blocks.iter_mut();
                    let unbound = unbound.filter(|w| w.calls.is_empty() && w.takes(&call));
                    unbound.for_each(|w| w.calls.push((id.clone(), call.agent.clone())));
                }
                let asks = ASKING_TOOLS.contains(&call.tool.as_str());
                let (tool, input, agent) =
                    (call.tool.clone(), call.input.clone(), call.agent.clone());
                session.running.push(call);
                if asks {
                    session.block(tool, input, agent, at);
                }
            }
            // It has no `tool_use_id`, so it names its call by tool and input. With no agent, it is
            // the session's own agent's.
            "PermissionRequest" if !ASKING_TOOLS.contains(&tool.as_str()) => {
                session.block(tool, event.tool_input, event.agent_id, at);
            }
            // A permission notice echoes a block, of the agent it names if any, that has not had its
            // notice yet. A sandboxed command's network request, and a subagent's prompt, fire
            // only the notice.
            "Notification" if event.notification_type == "permission_prompt" => {
                let agent = &event.agent_id;
                let mut blocks = session.blocks.iter_mut();
                let echoed = |w: &&mut Block| {
                    !w.noticed && w.tool != "Notification" && (agent.is_none() || w.agent == *agent)
                };
                match blocks.find(echoed) {
                    Some(block) => block.noticed = true,
                    None => session.block("Notification".into(), Value::Null, event.agent_id, at),
                }
            }
            "Elicitation" => session.block("Elicitation".into(), Value::Null, event.agent_id, at),
            // Daniel decided a plan or a permission from the studio card (#478): it waits on him no
            // more, though its result comes only when the call ends, or for a denial maybe never. A
            // question's result carries its answer, so its block waits for it.
            STUDIO_ANSWER if event.studio_answer.get("decision").is_some() => {
                let input = &event.tool_input;
                let decided =
                    |w: &Block| w.tool == tool && w.input == *input && w.agent == event.agent_id;
                if let Some(block) = session.blocks.iter().find(|w| decided(w)) {
                    let call = block.calls.first().map(|(call, _)| call.clone());
                    session.decisions.push(Decision {
                        call: call.unwrap_or(event.tool_use_id),
                        agent: block.agent.clone(),
                        tool: tool.clone(),
                        input: block.input.clone(),
                        answer: event.studio_answer["decision"].clone(),
                        at,
                        source: Source::Card,
                    });
                }
                session.blocks.retain(|w| !decided(w));
            }
            // A question answered on the card: its result, which carries the answer, is the card's.
            STUDIO_ANSWER => {
                session.carded.insert(event.tool_use_id);
            }
            "PostToolUse" | "PostToolUseFailure" => {
                session.running.retain(|c| c.id != id);
                let failed = event.hook_event_name == "PostToolUseFailure";
                // A plan or a permission that still waited on Daniel was decided in the terminal:
                // allowed if its call went on, denied if Daniel rejected it.
                // shortcut: a call that failed for another reason, such as an Edit whose text was
                // not found, records no decision; read its error if that loses real ones.
                let held = session.blocks.iter().find(|w| {
                    w.tool == tool && w.calls.iter().any(|(call, _)| Some(call) == id.as_ref())
                });
                let answer = match failed {
                    false => Some("allow"),
                    true if event.error.contains("doesn't want to proceed") => Some("deny"),
                    true => None,
                };
                if let (Some(block), Some(answer), Some(call)) = (held, answer, &id)
                    && tool != "AskUserQuestion"
                {
                    session.decisions.push(Decision {
                        call: call.clone(),
                        agent: block.agent.clone(),
                        tool: tool.clone(),
                        input: block.input.clone(),
                        answer: answer.into(),
                        at,
                        source: Source::Terminal,
                    });
                }
                session.resolve(|(call, _)| Some(call) == id.as_ref());
                let input = &event.tool_input;
                // The hooks are async, so an answer that lands after its turn's `Stop` still counts.
                let mut asked = session.questions.iter_mut();
                if !failed
                    && let Some(question) = asked.find(|q| q.tool_use_id == event.tool_use_id)
                {
                    // Only an object of strings is kept, so the pane can read it (#434, decision 42).
                    let answers = &event.tool_response.answers;
                    let strings = answers
                        .as_object()
                        .is_some_and(|a| a.values().all(Value::is_string));
                    question.answer = match strings {
                        true => Answer::Answered(answers.clone()),
                        false => Answer::NotAnswered,
                    };
                    if strings {
                        let card = session.carded.remove(&event.tool_use_id);
                        session.decisions.push(Decision {
                            call: event.tool_use_id.clone(),
                            agent: question.agent.clone(),
                            tool: tool.clone(),
                            input: input.clone(),
                            answer: answers.clone(),
                            at,
                            source: if card { Source::Card } else { Source::Terminal },
                        });
                    }
                }
                // A denied or interrupted call, or a failed Edit or Write, changed nothing: only a
                // command that ran and failed has its exit code in its error.
                let ran = !failed || event.error.starts_with("Exit code");
                if own {
                    session.agent.step_done(&event.tool_use_id, at);
                }
                // A check gets its line whether it passed or failed; one that did not run loses
                // the line its start gave it. A child's checks are its own, so they add no line.
                let command = input["command"].as_str().unwrap_or_default();
                if own && tool == "Bash" && is_check(command) {
                    let index = session
                        .checks
                        .iter()
                        .position(|l| l.call == event.tool_use_id);
                    let index = match index {
                        _ if !ran => {
                            session.checks.retain(|l| l.call != event.tool_use_id);
                            None
                        }
                        Some(index) => Some(index),
                        // The hooks are async, so its result can land before its start.
                        None => {
                            let start = at.saturating_sub(event.duration_ms.unwrap_or_default());
                            let line = CheckLine::new(command, &event.tool_use_id, start, typical);
                            session.checks.push(line);
                            Some(session.checks.len() - 1)
                        }
                    };
                    let task = event.tool_response.background_task_id.clone();
                    if let Some(line) = index.map(|i| &mut session.checks[i]) {
                        // In the background it runs on until its notification.
                        match task {
                            Some(task) => line.task = Some(task),
                            None => line.end(failed, at, &mut self.durations),
                        }
                    }
                }
            }
            _ => {}
        }
        // The worktrees its turn touched, each with its base from `looks` (#434, decision 9).
        if session.in_turn() {
            session.touched.extend(touches);
        }
        // An open question whose block cleared with no answer was not answered: its call failed, or
        // its turn or its agent ended first.
        let blocks = &session.blocks;
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
        session.asks_before_closing = session.in_turn() || !session.ended && session.background;
        session.work.estimate.depends_on_you = session.state == State::NeedsYou;
        session.asker = session.blocks.first().map(|block| Asker {
            agent: block.asker(),
            since: block.at,
        });
        let phase_started = session.work.phases.len() != phases_before;
        if !blocked_before && session.state == State::NeedsYou {
            outputs.push(Output::NeedsYou {
                session_id: event.session_id.clone(),
                agent: session.asker.as_ref().and_then(|w| w.agent.clone()),
            });
        }
        if phase_started {
            self.estimate(&event.session_id);
        }
        outputs
    }

    /// Sets session `id`'s estimate from the start of its phase, from the sessions of its kind that
    /// started before it and have ended (#461, decision 14): a paused session's time may be cut short.
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
    /// requests nothing, so turns that ended while the app was down get no cards and no notices
    /// here; `backfill` then asks for their cards.
    pub fn replay(&mut self, line: &str) {
        self.take(line, BTreeMap::new(), true);
    }

    /// After the replay, the turns that ended in the 24 hours before `now` with a change between
    /// the hook's trees and no card among `cards` (#486). Each is returned once.
    pub fn backfill(&mut self, now: u64, cards: &[CardRequest]) -> Vec<Backfill> {
        let since = now.saturating_sub(24 * 60 * 60 * 1000);
        let ended = std::mem::take(&mut self.ended).into_iter();
        let ended = ended.filter(|b| card_time(&b.request.id) >= since);
        ended
            .filter(|b| cards.iter().all(|c| c.id != b.request.id))
            .collect()
    }

    /// Applies, after the replay, what the live app learned before a restart (#434, decisions 37 and
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
            if session.state == State::YourTurn
                && now.saturating_sub(session.agent.last_event_at) >= IDLE_AFTER_MS
            {
                session.state = State::Idle;
                changed = true;
            }
        }
        changed
    }

    /// The keys of the sessions that have not ended and of their subagents: the shell drops the
    /// fingerprint records and writer queues of every other key (#410, decision 18).
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
    /// not the hangup ended it; a shell restarts in its folder. `busy` says what runs in it now.
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
                    // The rule a close asks by (#469).
                    Some(s) if s.in_turn() => Some("a turn".into()),
                    Some(s) if s.asks_before_closing => Some("background tasks".into()),
                    Some(s) if !s.ended => None,
                    // Started, with no event yet, or its session ended while the program runs on.
                    _ => program(),
                }
            };
            Revive {
                id: t.id.clone(),
                cwd: t.cwd.clone(),
                argv: t.argv.clone(),
                session: (!t.argv.is_empty())
                    .then(|| latest.map(|s| s.resume_id.clone()))
                    .flatten(),
                busy,
            }
        };
        running.map(revive).collect()
    }

    /// Reads what was added to the transcripts of the sessions that have not ended, and of the
    /// selected one, since the last call (#461, decision 12). `log` gets one line when a session's
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

    /// The folder and branch of each session that has not ended, and of the selected one, one per
    /// branch: what the shell asks `gh pr view` about, off the poll (#461, decision 19).
    pub fn branches(&self) -> Vec<(String, String)> {
        let mut seen = BTreeMap::new();
        for session in self.sessions.values() {
            let shown = self.selected.as_ref() == Some(&session.agent.id);
            if let Some(branch) = &session.identity.branch
                && (session.state != State::Ended || shown)
            {
                seen.entry(branch.clone()).or_insert(session.cwd.clone());
            }
        }
        seen.into_iter()
            .map(|(branch, cwd)| (cwd, branch))
            .collect()
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
    /// runs took when they did not fail, its backgrounded runs included (#461, decisions 13 to 15).
    /// `None` when it never ran. Runs group by `check_key`.
    pub fn typical(&self, command: &str) -> Option<u64> {
        median(self.durations.get(&check_key(command))?)
    }

    /// The check `event` starts: a new Bash call of its session's own agent that runs a check. A
    /// subagent's checks are not recorded, so they get no line and no card.
    fn check_start(&self, event: &Event) -> Option<CheckStart> {
        let command = event.tool_input["command"].as_str()?;
        let session = self.sessions.get(&event.session_id);
        let known = session.is_some_and(|s| s.checks.iter().any(|l| l.call == event.tool_use_id));
        let start = event.hook_event_name == "PreToolUse" && event.agent_id.is_none();
        (start && event.tool_name.as_deref() == Some("Bash") && is_check(command) && !known).then(
            || CheckStart {
                command: command.into(),
                typical: self.typical(command),
            },
        )
    }

    /// Session `id`'s row: its own, or the row it continues after a `/clear`.
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions
            .get(self.rows.get(id).map_or(id, String::as_str))
    }

    /// The session list: every session Daniel has not deleted, grouped in `State` order, the newest first in each group.
    pub fn sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().filter(|s| !s.deleted).collect();
        sessions.sort_by_key(|s| (s.state, Reverse(s.agent.started_at), &s.agent.id));
        sessions
    }

    /// Daniel clicked session `id`'s row, whose own view shows. Nothing else changes the selection.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.into());
        self.viewed = None;
    }

    /// Daniel opened subagent `id`'s line, so its view shows, with its own cards and brief, and a
    /// failed helper folds into its parent's line. Returns whether the opening counts, which the
    /// card store records: only a line whose agent had already failed (#434, decision 39).
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

/// The id of `owner`'s card written at `at`, a worker's or else a session's (`CardRequest::id`).
fn card_id(owner: &str, at: u64) -> String {
    format!("{owner}@{at}")
}

/// When the hook received the event card `id` was written at (`card_id`); 0 for an id of another form.
fn card_time(id: &str) -> u64 {
    let at = id.rsplit_once('@').and_then(|(_, at)| at.parse().ok());
    at.unwrap_or_default()
}

/// The first `n` characters of an id.
fn short(id: &str, n: usize) -> &str {
    id.get(..n).unwrap_or(id)
}

/// 3 seconds, a starting guess for "a few seconds" (#369, decision 22 and Further Notes).
const COMBINE: Duration = Duration::from_secs(3);

/// A notice not shown yet: its title and body, and for "needs you" the session it is about.
struct Pending {
    title: String,
    body: String,
    needs_you: Option<String>,
}

/// The notices the shell has not shown yet. Those that come within `COMBINE` of the first show as
/// one notification, so a burst never floods Daniel and no notice waits longer than that. Times are
/// on a monotonic clock, time since the app started, so a clock change never holds them back
/// (#434, decision 28).
#[derive(Default)]
pub struct Notifications {
    pending: Vec<Pending>,
    first_at: Duration,
}

impl Notifications {
    pub fn push(&mut self, title: &str, body: String, now: Duration) {
        self.queue(title, body, None, now);
    }

    /// A "needs you" notice on session `session_id`, which shows only if the session still needs
    /// Daniel when it is due (#434, decision 27).
    pub fn push_needs_you(&mut self, session_id: &str, body: String, now: Duration) {
        self.queue("Needs you", body, Some(session_id.into()), now);
    }

    fn queue(&mut self, title: &str, body: String, needs_you: Option<String>, now: Duration) {
        if self.pending.is_empty() {
            self.first_at = now;
        }
        let title = title.into();
        self.pending.push(Pending {
            title,
            body,
            needs_you,
        });
    }

    /// The notification due at `now`, as its title and body: the pending notices, once `COMBINE`
    /// has passed since the first, less the "needs you" ones whose session `needs_you` says no
    /// longer needs Daniel. Only then is `focused` asked, and a focused window drops them, since
    /// Daniel sees the list.
    pub fn due(
        &mut self,
        now: Duration,
        focused: impl FnOnce() -> bool,
        needs_you: impl Fn(&str) -> bool,
    ) -> Option<(String, String)> {
        if self.pending.is_empty() || now < self.first_at + COMBINE {
            return None;
        }
        let mut pending = std::mem::take(&mut self.pending);
        pending.retain(|p| p.needs_you.as_deref().is_none_or(&needs_you));
        if pending.is_empty() || focused() {
            return None;
        }
        let pending: Vec<(String, String)> =
            pending.into_iter().map(|p| (p.title, p.body)).collect();
        Some(match &pending[..] {
            [one] => one.clone(),
            many => {
                let lines: Vec<String> = many.iter().map(|(t, b)| format!("{t}: {b}")).collect();
                (format!("{} notices", many.len()), lines.join("\n"))
            }
        })
    }
}
