//! The studio core (#328, module 3): plain Rust with no I/O. It takes hook events one at a time, in
//! event log order, and keeps the state the app shows. App start replays the whole log through
//! `replay`; live events go through `apply`, which updates state the same way and also returns what
//! the shell must do.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashMap},
};

#[derive(Default)]
pub struct Studio {
    sessions: HashMap<String, Session>,
    /// The session Daniel clicked last, which the session view shows.
    selected: Option<String>,
    /// The subagent of it whose line he opened since, whose own view shows instead.
    viewed: Option<String>,
    /// The shell's latest tick, in Unix milliseconds.
    now: u64,
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
    pub state: State,
    /// The checks that ran, oldest first: pass or fail lines in the session's timeline (#369,
    /// decision 14).
    pub checks: Vec<CheckLine>,
    /// The tool calls whose `PreToolUse` landed and whose `PostToolUse` has not.
    #[serde(skip)]
    running: Vec<Call>,
    /// The tool calls blocked on Daniel.
    #[serde(skip)]
    blocks: Vec<Call>,
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
    /// Its latest `Stop`, until its next prompt (#369, decision 19).
    pub stopped: Option<Stopped>,
    /// Whether its turn was opened by a wake-up and has had no prompt of Daniel's since.
    #[serde(skip)]
    woken: bool,
    /// "worker <name> returned", from its latest wake-up until its own next step or Daniel's next
    /// prompt (#410, decision 27).
    pub returned: Option<String>,
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
    #[serde(skip)]
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
    /// Daniel's choice for each question, keyed by its text: the `PostToolUse`'s `answers`.
    Answered(Value),
    /// Its call failed or was interrupted, or its turn or its agent ended first.
    NotAnswered,
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
    /// Daniel opened its line.
    pub opened: bool,
    /// The folder of its first event that has one: its worktree, or its parent's folder.
    #[serde(skip)]
    pub folder: Option<String>,
    /// What its spawn asked it to do, which its brief's rewrite reads; the pane never shows it.
    #[serde(skip)]
    pub prompt: String,
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
    #[serde(skip)]
    last_event_at: u64,
    #[serde(skip)]
    since: Since,
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

    /// Updates the subagent tree with one event. A spawn's response names its child (`agentId`),
    /// and every child event names its agent (`agent_id`).
    fn track_agents(&mut self, event: &Event, worktree_changed: bool) {
        let at = event.studio_received_ms;
        let tool = event.tool_name.as_deref().unwrap_or_default();
        if let Some(id) = &event.agent_id {
            let agent = self.agent(id, at);
            agent.agent.last_event_at = agent.agent.last_event_at.max(at);
            if agent.agent_type.is_none() {
                agent.agent_type.clone_from(&event.agent_type);
            }
            // Its first folder is its first base (`Studio::look`).
            if agent.folder.is_none() && !event.cwd.is_empty() {
                agent.folder = Some(event.cwd.clone());
                agent.agent.since.at = at;
            }
            // A changed file promotes a helper at that step.
            let changed = matches!(tool, "Edit" | "Write") || (tool == "Bash" && worktree_changed);
            match event.hook_event_name.as_str() {
                // A step after its end means its parent continued it (`SendMessage`).
                "PreToolUse" => {
                    agent.agent.last_step = Some(Step {
                        tool: tool.into(),
                        detail: step_detail(&event.tool_input, &event.cwd),
                    });
                    agent.status = Status::Running;
                    agent.ended_at = None;
                }
                "PostToolUse" if changed => agent.kind = Kind::Worker,
                "SubagentStop" => {
                    let message = event.last_assistant_message.lines().next();
                    agent.result = message.map(Into::into);
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
                let agent = self.agent(child, at);
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
            // A wake-up names the task it reports on, and how it ended (#370 note).
            "UserPromptSubmit" => {
                for (id, status) in reported(&event.prompt) {
                    if let Some(agent) = self.agents.iter_mut().find(|a| a.agent.id == id) {
                        agent.end(Some(status), at);
                    }
                }
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
                let ended = self.agents.iter_mut().filter(|a| {
                    a.parent == Parent::Session
                        && a.status == Status::Running
                        && !listed(&a.agent.id)
                });
                for agent in ended {
                    agent.end(None, agent.agent.last_event_at);
                }
            }
            _ => {}
        }
        // A subagent that ended waits on nobody.
        let agents = &self.agents;
        let ended = |call: &Call| {
            let agent = agents
                .iter()
                .find(|a| Some(&a.agent.id) == call.agent.as_ref());
            agent.is_some_and(|a| a.ended_at.is_some())
        };
        self.blocks.retain(|call| !ended(call));
        self.running.retain(|call| !ended(call));
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

    /// The owner key of the cards that cover subagent `id`'s changes in `folder`: `id` itself when its
    /// parent works in another folder, else its parent's, up to the session's own agent's id.
    fn owner(&self, id: &str, folder: &str) -> String {
        let find = |id: &str| self.subagent(id);
        match find(id).map(|a| &a.parent) {
            Some(Parent::Agent(parent)) => {
                if find(parent).and_then(|a| a.folder.as_deref()) != Some(folder) {
                    return id.into();
                }
                self.owner(parent, folder)
            }
            _ if folder != self.cwd => id.into(),
            _ => self.agent.id.clone(),
        }
    }

    /// Whether subagent `id` gets cards of its own: a worker in a folder of its own.
    fn has_cards(&self, id: &str) -> bool {
        self.subagent(id).is_some_and(|a| {
            let folder = a.folder.as_deref().unwrap_or_default();
            a.kind == Kind::Worker && self.owner(id, folder) == id
        })
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
        }
    }

    /// Whether it is in a turn: prompted, and neither stopped nor ended since.
    fn in_turn(&self) -> bool {
        !self.turn_ended && !self.ended
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

    /// Adds a block on the latest running call it waits on, so that the call's `PostToolUse` clears
    /// it, and the call's agent is the one who asks. With none, the call's `PreToolUse` may still land
    /// (the hooks are async); otherwise the block lasts until its agent's turn ends.
    fn block(&mut self, tool: String, input: Value, agent: Option<String>, at: u64) {
        let mut block = Call {
            id: None,
            tool,
            input,
            agent,
            at,
        };
        if let Some(call) = self.running.iter().rfind(|c| waits_on(&block, c)) {
            block.id.clone_from(&call.id);
            block.agent.clone_from(&call.agent);
        }
        self.blocks.push(block);
    }

    /// Clears the blocks of the session's own agent (#328, module 3, "Needs you": Daniel's
    /// `UserPromptSubmit`, not a wake-up; `Stop`; `SessionEnd`). The turn's calls end with it, so a
    /// call that never reported back takes no later block. Subagents run past their session's turn,
    /// so their blocks stay.
    fn unblock(&mut self) {
        self.blocks.retain(|c| c.agent.is_some());
        self.running.retain(|c| c.agent.is_some());
        self.asked = false;
    }
}

/// Tools that wait on Daniel in any permission mode, before any `PermissionRequest`.
const ASKING_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

/// Whether `block` waits on tool call `call`: a permission prompt on the call it names, an MCP
/// server's request for input on an MCP tool, and a sandboxed command's network request on a command.
fn waits_on(block: &Call, call: &Call) -> bool {
    match block.tool.as_str() {
        "Elicitation" => call.tool.starts_with("mcp__"),
        "Notification" => call.tool == "Bash",
        tool => tool == call.tool && block.input == call.input,
    }
}

/// A session's state (#369, decision 4). Each has a named clearer: an event, or the tick for idle.
/// The order is the session list's: needs you first, then the groups the list collapses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Blocked on Daniel (#328, module 3, "Needs you"), until the step that asks resolves, a prompt
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
    pub detail: Option<String>,
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
    /// A prompt named a known task id in markup the studio does not know: drift in Claude Code's
    /// wake-ups, for stderr (#369, decision 7).
    Unrecognised { session_id: String, task_id: String },
    /// A session's first `Stop`: read Claude Code's version from its transcript, and nothing else
    /// (#369, decision 9).
    Version { transcript_path: String },
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

/// A check that ran, shown as a pass or fail line.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheckLine {
    /// The command's first line.
    pub command: String,
    /// Whether the command exited 0, when that says whether the check passed: nothing in the
    /// command masks the check's exit code (`exit_code_counts`). `None` otherwise.
    pub passed: Option<bool>,
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
    /// A base, whose change is never a card: a session's prompt, a subagent's first folder, a
    /// helper's look before its command, and the start-up bases.
    Base,
    /// After a worker's or a helper's command: a card only when `HEAD` moved; the change it saw
    /// otherwise stays for the card at its next commit, its end or its fallback.
    Commit,
    /// A card when the worktree changed, such as a worker's end.
    Card,
    /// 20 minutes or more after the worktree's last card or its last quiet fallback: a card when
    /// it changed, else a new 20-minute wait (#369, decision 11, as #410 changes it).
    Fallback,
    /// A session's `Stop`, whose change, or else its workers' cards, makes its card or brief.
    TurnEnd,
}

/// One tool call: its `tool_use_id`, and its tool and input, which are all `PermissionRequest`
/// names it by. Also the subagent that made it, `None` for the session's own agent, and when it
/// was received.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    id: Option<String>,
    tool: String,
    input: Value,
    agent: Option<String>,
    at: u64,
}

/// Who a session that needs Daniel waits on: its oldest block's asker, a subagent or `None` for the
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
    /// The start of a session: `startup`, `resume`, `clear` or `compact`.
    #[serde(default)]
    source: String,
    #[serde(default)]
    studio_received_ms: u64,
}

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

/// The tool input that says what a step works on: a command's first line, a path relative to the
/// session's folder, the first question asked, or a search pattern, URL, query or task description.
fn step_detail(input: &Value, cwd: &str) -> Option<String> {
    if let Some(command) = input["command"].as_str() {
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

/// Whether a tool call runs a check in the foreground. A background command's result arrives as soon
/// as it starts, before the check has run.
fn runs_check(tool: &str, input: &Value) -> bool {
    tool == "Bash"
        && input["run_in_background"] != true
        && is_check(input["command"].as_str().unwrap_or_default())
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
    /// The worktrees the shell fingerprints before it applies this live line (#328, module 5), each
    /// with its own key: none, one, or at a wake-up also its workers' ends (#369, decisions 6, 8, 10
    /// and 11).
    pub fn fingerprint_folders(&self, line: &str) -> Vec<Fingerprint> {
        let event = serde_json::from_str(line);
        event.map_or_else(|_| Vec::new(), |event| self.looks(&event))
    }

    /// The fingerprints before `event`: its own (`look`), and at a wake-up, the end of each worker
    /// with cards it reports on that no other signal ended, since a killed worker has no
    /// `SubagentStop` (#369, decision 8).
    fn looks(&self, event: &Event) -> Vec<Fingerprint> {
        let mut looks: Vec<Fingerprint> = self.look(event).into_iter().collect();
        let session = self.sessions.get(&event.session_id);
        if event.hook_event_name == "UserPromptSubmit"
            && let Some(session) = session
        {
            for (id, _) in reported(&event.prompt) {
                let agent = session
                    .subagent(id)
                    .filter(|a| a.ended_at.is_none() && session.has_cards(id));
                looks.extend(agent.and_then(|a| {
                    Some(Fingerprint {
                        key: id.into(),
                        folder: a.folder.clone()?,
                        purpose: Purpose::Card,
                    })
                }));
            }
        }
        looks
    }

    /// The fingerprint before `event`. A session's own: at a prompt, the prompt's base, so what Daniel
    /// changed himself since the last card does not count; a prompt inside a turn still open takes
    /// none, since an interrupt fires no `Stop` and a wake-up can land mid-turn. At `Stop`, the change
    /// since the base is the card's. A subagent's: its first event's, its base; a helper's before and
    /// after each Bash step, for a change across it that makes it a worker; a worker's after each
    /// Bash step, for a commit; and a worker's end. On a step 20 minutes after the last card of the
    /// worktree it is in, that worktree's, for the fallback card; a session's only in a turn.
    fn look(&self, event: &Event) -> Option<Fingerprint> {
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
            let purpose = match name {
                "UserPromptSubmit" if !session.is_some_and(Session::in_turn) => Purpose::Base,
                "Stop" => Purpose::TurnEnd,
                _ if session.is_some_and(session_due) => Purpose::Fallback,
                _ => return None,
            };
            return look(&event.session_id, &event.cwd, purpose);
        };
        let session = session?;
        let agent = session.subagent(id);
        let Some(folder) = agent.and_then(|a| a.folder.as_deref()) else {
            return look(id, &event.cwd, Purpose::Base);
        };
        let worker = |a: &Subagent| a.kind == Kind::Worker;
        let bash = event.tool_name.as_deref() == Some("Bash");
        // A helper's base right before its command, so only a change across it promotes it.
        if bash && name == "PreToolUse" && agent.is_some_and(|a| !worker(a)) {
            return look(id, folder, Purpose::Base);
        }
        let bash = step && bash;
        let owner = session.owner(id, folder);
        let since = &session.record(&owner).since;
        match session.subagent(&owner) {
            Some(own) if &own.agent.id == id => match worker(own) {
                true if name == "SubagentStop" => look(id, folder, Purpose::Card),
                true if due(since) => look(id, folder, Purpose::Fallback),
                _ => look(id, folder, Purpose::Commit).filter(|_| bash),
            },
            // It works in the worktree of a worker above it, or of its session.
            Some(above) if worker(above) && due(since) => {
                look(&above.agent.id, above.folder.as_deref()?, Purpose::Fallback)
            }
            None if session_due(session) => {
                look(&session.agent.id, &session.cwd, Purpose::Fallback)
            }
            _ => look(id, folder, Purpose::Commit)
                .filter(|_| bash && agent.is_some_and(|a| !worker(a))),
        }
    }

    /// The sessions in a turn and the running subagents, with their folders, by their fingerprints'
    /// keys. After a replay the shell fingerprints them once, so a restart mid-turn leaves the
    /// turn's `Stop` and each subagent's next look a base to compare with.
    pub fn waiting_folders(&self) -> Vec<Fingerprint> {
        let waiting = self.sessions.values().filter(|s| s.in_turn());
        let agents = self.sessions.values().flat_map(|s| &s.agents);
        let running = agents.filter(|a| a.status == Status::Running);
        let running = running.filter_map(|a| Some((a.agent.id.as_str(), a.folder.as_deref()?)));
        let folders = waiting
            .map(|s| (s.agent.id.as_str(), s.cwd.as_str()))
            .chain(running);
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
        mut changes: BTreeMap<String, Change>,
    ) -> Vec<Output> {
        let Ok(event) = serde_json::from_str::<Event>(line) else {
            return Vec::new();
        };
        let looks = self.looks(&event);
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
        // The hooks are async, so the latest event can land before an earlier one.
        session.agent.last_event_at = session.agent.last_event_at.max(event.studio_received_ms);
        let failed_before = session.failed_workers();
        let its_own = |id: &String| changes.contains_key(id);
        session.track_agents(&event, event.agent_id.as_ref().is_some_and(its_own));
        // A child's events still keep its session from going idle, but its folder and steps are its
        // own.
        let own = event.agent_id.is_none();
        if own && !event.cwd.is_empty() {
            session.cwd.clone_from(&event.cwd);
            let last = event.cwd.rsplit('/').next().unwrap_or_default();
            session.folder_name = if last.is_empty() { &event.cwd } else { last }.into();
        }
        let failed = session.failed_workers().into_iter();
        let mut outputs: Vec<Output> = failed
            .filter(|agent| !failed_before.contains(agent))
            .map(|agent| Output::Failed {
                session_id: event.session_id.clone(),
                agent,
            })
            .collect();
        let mut blocked_before = session.needs_you();
        let tool = event.tool_name.unwrap_or_default();
        let id = Some(event.tool_use_id.clone()).filter(|id| !id.is_empty());
        let at = event.studio_received_ms;
        let name = event.hook_event_name.as_str();
        // Who changed what (#369, story 20): a subagent's edit in a worktree whose cards are another's.
        if let Some(agent) = &event.agent_id
            && name == "PostToolUse"
            && matches!(tool.as_str(), "Edit" | "Write")
            && let Some(path) = event.tool_input["file_path"].as_str()
        {
            let folder = session.subagent(agent);
            let folder = folder.and_then(|a| a.folder.clone()).unwrap_or_default();
            let owner = session.owner(agent, &folder);
            if owner != *agent {
                let by = &mut session.record_mut(&owner).since.by;
                by.entry(agent.clone())
                    .or_default()
                    .insert(relative(path, &folder).into());
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
            if !worker || session.has_cards(&look.key) {
                let request = session.card(&look.key, Some(found), at);
                if worker {
                    session.agent.since.cards.push(request.id.clone());
                }
                outputs.push(Output::Card(request));
            }
        }
        match name {
            // A prompt opens a wait, also after an interrupt, which fires no `Stop`.
            "UserPromptSubmit" => {
                let sender = session.sender(&event.prompt);
                let wake_up = !sender.is_daniels();
                session.woken = wake_up && (session.woken || !session.in_turn());
                // A base for the prompt: what Daniel changed himself before it is no one's.
                if looks
                    .iter()
                    .any(|look| look.purpose == Purpose::Base && look.key == session.agent.id)
                {
                    session.agent.since.at = at;
                    session.agent.since.by.clear();
                }
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
                session.prompts.push(Prompt {
                    at,
                    sender,
                    text: event.prompt,
                });
                // Daniel's prompt answers what asked him; a wake-up is not his, so it answers nothing.
                if !wake_up {
                    session.returned = None;
                    session.unblock();
                }
                session.turn_ended = false;
                session.stopped = None;
            }
            "Stop" => {
                // The order on `Stop`: clear its own blocks first, then ask if the message asks.
                session.unblock();
                session.turn_ended = true;
                // A subagent's wait outlasts the turn, so it gives no second notice.
                blocked_before = session.needs_you();
                session.asked = asks(&event.last_assistant_message);
                if !session.versioned && !event.transcript_path.is_empty() {
                    session.versioned = true;
                    outputs.push(Output::Version {
                        transcript_path: event.transcript_path,
                    });
                }
                // Its own change, else its workers' cards since its last card: a card, or only its
                // brief when a wake-up opened the turn.
                let workers = !session.agent.since.cards.is_empty();
                let output = match changes.remove(&session.agent.id) {
                    Some(change) => Some(Output::Card(session.card(
                        &event.session_id,
                        Some(change),
                        at,
                    ))),
                    None if workers && session.woken => {
                        Some(Output::Brief(session.card(&event.session_id, None, at)))
                    }
                    None if workers => {
                        Some(Output::Card(session.card(&event.session_id, None, at)))
                    }
                    None => None,
                };
                session.agent.since.by.clear();
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
                session.turn_ended = true;
                session.ended = true;
            }
            // A resumed session keeps its id; a compaction keeps the turn it happens in.
            "SessionStart" => {
                session.ended = false;
                session.turn_ended |= event.source != "compact";
            }
            "PreToolUse" => {
                if own {
                    session.returned = None;
                    session.agent.last_step = Some(Step {
                        tool: tool.clone(),
                        detail: step_detail(&event.tool_input, &event.cwd),
                    });
                }
                let call = Call {
                    id,
                    tool,
                    input: event.tool_input,
                    agent: event.agent_id.clone(),
                    at,
                };
                if ASKING_TOOLS.contains(&call.tool.as_str()) {
                    session.blocks.push(call.clone());
                }
                if call.tool == "AskUserQuestion" {
                    session.questions.push(Question {
                        tool_use_id: event.tool_use_id.clone(),
                        agent: call.agent.clone(),
                        questions: call.input["questions"].clone(),
                        answer: Answer::Open,
                    });
                }
                // The hooks are async, so the block this call waits on may have landed first.
                // shortcut: a block whose call ended before the block landed stays until the turn
                // ends; remember ended calls if auto-denied calls (#322) show it.
                if let Some(block) = session
                    .blocks
                    .iter_mut()
                    .find(|b| b.id.is_none() && waits_on(b, &call))
                {
                    block.id.clone_from(&call.id);
                    block.agent.clone_from(&call.agent);
                }
                session.running.push(call);
            }
            // It has no `tool_use_id`, so it names its call by tool and input. The prompt's
            // Notification may have landed first (the hooks are async), and its block names no call.
            "PermissionRequest" if !ASKING_TOOLS.contains(&tool.as_str()) => {
                session.blocks.retain(|b| b.tool != "Notification");
                session.block(tool, event.tool_input, event.agent_id, at);
            }
            // A sandboxed command's network request fires no `PermissionRequest`; any other
            // prompt already has its block.
            "Notification"
                if event.notification_type == "permission_prompt" && !session.needs_you() =>
            {
                session.block("Notification".into(), Value::Null, event.agent_id, at);
            }
            "Elicitation" => session.block("Elicitation".into(), Value::Null, event.agent_id, at),
            "PostToolUse" | "PostToolUseFailure" => {
                session.running.retain(|c| c.id != id);
                session.blocks.retain(|c| c.id != id);
                let input = &event.tool_input;
                let failed = event.hook_event_name == "PostToolUseFailure";
                // The hooks are async, so an answer that lands after its turn's `Stop` still counts.
                let mut asked = session.questions.iter_mut();
                if !failed
                    && let Some(question) = asked.find(|q| q.tool_use_id == event.tool_use_id)
                {
                    question.answer = Answer::Answered(event.tool_response.answers.clone());
                }
                // A denied or interrupted call, or a failed Edit or Write, changed nothing: only a
                // command that ran and failed has its exit code in its error.
                let ran = !failed || event.error.starts_with("Exit code");
                // A check gets its line whether it passed or failed. A child's checks are its own,
                // so they add no line.
                if ran && own && runs_check(&tool, input) {
                    let command = input["command"].as_str().unwrap_or_default();
                    session.checks.push(CheckLine {
                        command: command.lines().next().unwrap_or_default().into(),
                        passed: exit_code_counts(command).then_some(!failed),
                    });
                }
            }
            _ => {}
        }
        // An open question whose block cleared with no answer was not answered: its call failed, or
        // its turn or its agent ended first.
        let blocks = &session.blocks;
        for question in &mut session.questions {
            let open = blocks
                .iter()
                .any(|b| b.id.as_ref() == Some(&question.tool_use_id));
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
        session.asker = session.blocks.first().map(|block| Asker {
            agent: block.agent.clone(),
            since: block.at,
        });
        if !blocked_before && session.state == State::NeedsYou {
            outputs.push(Output::NeedsYou {
                session_id: event.session_id,
                agent: session.asker.as_ref().and_then(|w| w.agent.clone()),
            });
        }
        outputs
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
    /// requests nothing, so turns that ended while the app was down get no cards and no notices.
    pub fn replay(&mut self, line: &str) {
        self.apply(line);
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

    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    /// The session list: every session, grouped in `State` order, the newest first in each group.
    pub fn sessions(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by_key(|s| (s.state, Reverse(s.agent.started_at), &s.agent.id));
        sessions
    }

    /// Daniel clicked session `id`'s row, whose own view shows. Nothing else changes the selection.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.into());
        self.viewed = None;
    }

    /// Daniel opened subagent `id`'s line, so its view shows, with its own cards and brief, and a
    /// failed helper folds into its parent's line.
    pub fn open(&mut self, id: &str) {
        self.viewed = Some(id.into());
        let agents = self.sessions.values_mut().flat_map(|s| &mut s.agents);
        for agent in agents.filter(|a| a.agent.id == id) {
            agent.opened = true;
        }
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

/// The first `n` characters of an id.
fn short(id: &str, n: usize) -> &str {
    id.get(..n).unwrap_or(id)
}

/// 3 seconds, a starting guess for "a few seconds" (#369, decision 22 and Further Notes).
const COMBINE_MS: u64 = 3_000;

/// The notices the shell has not shown yet, each with its title and body. Those that come within
/// `COMBINE_MS` of the first show as one notification, so a burst never floods Daniel and no notice
/// waits longer than that.
#[derive(Default)]
pub struct Notifications {
    pending: Vec<(String, String)>,
    first_at: u64,
}

impl Notifications {
    pub fn push(&mut self, title: &str, body: String, now: u64) {
        if self.pending.is_empty() {
            self.first_at = now;
        }
        self.pending.push((title.into(), body));
    }

    /// The notification due at `now`, as its title and body: the pending notices, once `COMBINE_MS`
    /// has passed since the first. Only then is `focused` asked, and a focused window drops them,
    /// since Daniel sees the list.
    pub fn due(&mut self, now: u64, focused: impl FnOnce() -> bool) -> Option<(String, String)> {
        if self.pending.is_empty() || now < self.first_at + COMBINE_MS {
            return None;
        }
        let pending = std::mem::take(&mut self.pending);
        if focused() {
            return None;
        }
        Some(match &pending[..] {
            [one] => one.clone(),
            many => {
                let lines: Vec<String> = many.iter().map(|(t, b)| format!("{t}: {b}")).collect();
                (format!("{} notices", many.len()), lines.join("\n"))
            }
        })
    }
}
