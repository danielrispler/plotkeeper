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

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Session {
    pub id: String,
    /// The folder the session works in, from its latest event.
    pub cwd: String,
    pub last_step: Option<Step>,
    pub state: State,
    /// When the hook received its first and its latest event (`studio_received_ms`), in Unix
    /// milliseconds.
    #[serde(skip)]
    started_at: u64,
    #[serde(skip)]
    last_event_at: u64,
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
    #[serde(skip)]
    since: Since,
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
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Subagent {
    /// Its `agent_id`.
    pub id: String,
    pub parent: Parent,
    pub kind: Kind,
    /// Its spawn's description and its type, such as `Explore`.
    pub description: Option<String>,
    pub agent_type: Option<String>,
    /// `running`, then how it ended: its wake-up's `<status>`, such as `completed` or `killed`, or
    /// `completed` when only its `SubagentStop`, its spawn's response or its session's `Stop` says it
    /// ended.
    pub status: String,
    /// When its first event and its end were received, in Unix milliseconds.
    pub started_at: u64,
    pub ended_at: Option<u64>,
    pub last_step: Option<Step>,
    /// The first line of its last message, from its `SubagentStop`.
    pub result: Option<String>,
    /// It ended with a status other than `completed`, such as `killed`.
    pub failed: bool,
    /// Daniel opened its line.
    pub opened: bool,
    /// A helper that succeeded, or one that failed and Daniel opened, folds into its parent's line
    /// (#369, story 18). A failed child stays visible until he opens it.
    pub faded: bool,
    #[serde(skip)]
    last_event_at: u64,
    /// The folder of its first event that has one: its worktree, or its parent's folder.
    #[serde(skip)]
    pub folder: Option<String>,
    /// What its spawn asked it to do, which its brief's rewrite reads; the pane never shows it.
    #[serde(skip)]
    pub prompt: String,
    #[serde(skip)]
    since: Since,
}

impl Subagent {
    /// Ends it at `at`, with its wake-up's `status`, or with none from any other signal. A wake-up's
    /// status overrides the others.
    fn end(&mut self, status: Option<&str>, at: u64) {
        if let Some(status) = status {
            self.status = status.into();
        } else if self.status == "running" {
            self.status = "completed".into();
        }
        self.ended_at.get_or_insert(at);
    }

    fn fails(&self) -> bool {
        !["running", "completed"].contains(&self.status.as_str())
    }

    fn fades(&self) -> bool {
        let succeeded = self.status == "completed";
        self.kind == Kind::Helper && (succeeded || self.failed && self.opened)
    }
}

/// Who spawned a subagent: the session's own agent, another subagent, or no known spawn.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Parent {
    Session,
    Agent(String),
    Unattached,
}

/// A worker is spawned with its own worktree, or changes files; any other subagent is a helper
/// (#369, decision 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Worker,
    Helper,
}

impl Session {
    /// Subagent `id`, added in "unattached" when this is the first event that names it.
    fn agent(&mut self, id: &str, at: u64) -> &mut Subagent {
        let index = match self.agents.iter().position(|a| a.id == id) {
            Some(index) => index,
            None => {
                self.agents.push(Subagent {
                    id: id.into(),
                    parent: Parent::Unattached,
                    kind: Kind::Helper,
                    description: None,
                    agent_type: None,
                    status: "running".into(),
                    started_at: at,
                    ended_at: None,
                    last_step: None,
                    result: None,
                    failed: false,
                    opened: false,
                    faded: false,
                    last_event_at: at,
                    folder: None,
                    prompt: String::new(),
                    since: Since::default(),
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
            agent.last_event_at = agent.last_event_at.max(at);
            if agent.agent_type.is_none() {
                agent.agent_type.clone_from(&event.agent_type);
            }
            // Its first folder is its first base (`Studio::look`).
            if agent.folder.is_none() && !event.cwd.is_empty() {
                agent.folder = Some(event.cwd.clone());
                agent.since.at = at;
            }
            // A changed file promotes a helper at that step.
            let changed = matches!(tool, "Edit" | "Write") || (tool == "Bash" && worktree_changed);
            match event.hook_event_name.as_str() {
                // A step after its end means its parent continued it (`SendMessage`).
                "PreToolUse" => {
                    agent.last_step = Some(Step {
                        tool: tool.into(),
                        detail: step_detail(&event.tool_input, &event.cwd),
                    });
                    agent.status = "running".into();
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
        if let Some(task) = event.tool_response["backgroundTaskId"].as_str() {
            self.tasks.insert(task.into());
        }
        let listed = event.background_tasks.as_array().into_iter().flatten();
        let ids = listed.filter_map(|task| task["id"].as_str());
        self.tasks.extend(ids.map(Into::into));
        match event.hook_event_name.as_str() {
            "PostToolUse"
                if tool == "Agent"
                    && let Some(child) = event.tool_response["agentId"].as_str() =>
            {
                let parent = event
                    .agent_id
                    .clone()
                    .map_or(Parent::Session, Parent::Agent);
                let input = &event.tool_input;
                let agent = self.agent(child, at);
                agent.parent = parent;
                agent.description = input["description"].as_str().map(Into::into);
                agent.prompt = input["prompt"].as_str().unwrap_or_default().into();
                if agent.agent_type.is_none() {
                    agent.agent_type = input["subagent_type"].as_str().map(Into::into);
                }
                if input["isolation"] == "worktree" {
                    agent.kind = Kind::Worker;
                }
                // A foreground spawn's response comes at its end.
                if event.tool_response["status"] == "completed" {
                    agent.end(None, at);
                }
            }
            // A wake-up names the task it reports on, and how it ended (#370 note).
            "UserPromptSubmit" => {
                for (id, status) in reported(&event.prompt) {
                    if let Some(agent) = self.agents.iter_mut().find(|a| a.id == id) {
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
                    a.parent == Parent::Session && a.status == "running" && !listed(&a.id)
                });
                for agent in ended {
                    agent.end(None, agent.last_event_at);
                }
            }
            _ => {}
        }
        for agent in &mut self.agents {
            agent.failed = agent.fails();
            agent.faded = agent.fades();
        }
        // A subagent that ended waits on nobody.
        let agents = &self.agents;
        let ended = |call: &Call| {
            let agent = agents.iter().find(|a| Some(&a.id) == call.agent.as_ref());
            agent.is_some_and(|a| a.ended_at.is_some())
        };
        self.blocks.retain(|call| !ended(call));
        self.running.retain(|call| !ended(call));
    }

    /// Who sent `prompt`. A wake-up names a known task id of the session in markup the studio
    /// knows: `<agent-message from="…">` or `<task-id>…</task-id>`. A known id in other markup is
    /// drift, and one in plain text is Daniel's.
    fn sender(&self, prompt: &str) -> Sender {
        let known = self.agents.iter().map(|a| &a.id).chain(&self.tasks);
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

    /// The subagent whose cards cover subagent `id`'s changes in `folder`: `id` itself when its parent
    /// works in another folder, else its parent's; `None` for the session's own agent's cards.
    fn owner(&self, id: &str, folder: &str) -> Option<String> {
        let find = |id: &str| self.agents.iter().find(|a| a.id == id);
        match find(id).map(|a| &a.parent) {
            Some(Parent::Agent(parent)) => {
                if find(parent).and_then(|a| a.folder.as_deref()) != Some(folder) {
                    return Some(id.into());
                }
                self.owner(parent, folder)
            }
            _ => (folder != self.cwd).then(|| id.into()),
        }
    }

    /// Whether subagent `id` gets cards of its own: a worker in a folder of its own.
    fn has_cards(&self, id: &str) -> bool {
        let agent = self.agents.iter().find(|a| a.id == id);
        agent.is_some_and(|a| {
            let folder = a.folder.as_deref().unwrap_or_default();
            a.kind == Kind::Worker && self.owner(id, folder).as_deref() == Some(id)
        })
    }

    /// What happened in the worktree of `owner`, a subagent, or else the session's own agent, since
    /// its last card.
    fn since(&mut self, owner: Option<&str>) -> &mut Since {
        match owner.and_then(|id| self.agents.iter_mut().find(|a| a.id == id)) {
            Some(agent) => &mut agent.since,
            None => &mut self.since,
        }
    }

    /// The card on `owner`'s change since its last card, as `Since` says, which starts the next one
    /// at `at`: `change`, or else its workers' cards.
    fn card(&mut self, owner: Option<&str>, change: Option<Change>, at: u64) -> CardRequest {
        // A worker's cards belong to its task, and a session's to Daniel's latest prompt.
        let daniels = |p: &&Prompt| !matches!(p.sender, Sender::WakeUp(_));
        let prompt = match owner {
            Some(id) => self
                .agents
                .iter()
                .find(|a| a.id == id)
                .map(|a| a.started_at),
            None => self.prompts.iter().rfind(daniels).map(|p| p.at),
        };
        let since = std::mem::take(self.since(owner));
        self.since(owner).at = at;
        let (base, change) = change.map_or_default(|c| (c.base, c.diff));
        CardRequest {
            session_id: self.id.clone(),
            id: card_id(owner.unwrap_or(&self.id), at),
            base,
            change,
            prompt,
            agent: owner.map(Into::into),
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
            .filter(|a| a.kind == Kind::Worker && a.failed)
            .map(|a| a.id.clone())
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Blocked on Daniel (#328, module 3, "Needs you"), until the step that asks resolves, a prompt
    /// of Daniel's, `Stop` or `SessionEnd`.
    NeedsYou,
    /// In a turn, from a prompt until `Stop` or `SessionEnd`.
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
    /// The diff of each file that changed since the base, commits included. Empty on a card built
    /// from `workers`' cards.
    pub change: String,
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
    pub keep: Keep,
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

/// When a fingerprint becomes the base of the next change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    Always,
    /// Only when `HEAD` moved: a worker's step looks for a commit, and the change it saw otherwise
    /// stays for the card at its next commit, its end or its fallback (#369, decision 11).
    OnCommit,
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
    /// On an `Agent` call's `PostToolUse`, the child's `agentId` and status.
    #[serde(default)]
    tool_response: Value,
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
        let relative = path.strip_prefix(cwd).and_then(|p| p.strip_prefix('/'));
        return Some(relative.unwrap_or(path).into());
    }
    ["pattern", "url", "query", "description"]
        .iter()
        .find_map(|key| input[key].as_str())
        .map(Into::into)
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
                let agent = session.agents.iter().find(|a| a.id == id);
                let agent = agent.filter(|a| a.ended_at.is_none() && session.has_cards(id));
                looks.extend(agent.and_then(|a| {
                    Some(Fingerprint {
                        key: id.into(),
                        folder: a.folder.clone()?,
                        keep: Keep::Always,
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
        let session_due = |s: &Session| s.in_turn() && due(&s.since);
        let look = |key: &str, folder: &str, keep| {
            let look = Fingerprint {
                key: key.into(),
                folder: folder.into(),
                keep,
            };
            (!folder.is_empty()).then_some(look)
        };
        let Some(id) = &event.agent_id else {
            let at = match name {
                "UserPromptSubmit" => !self.in_turn(&event.session_id),
                "Stop" => true,
                _ => session.is_some_and(session_due),
            };
            return look(&event.session_id, &event.cwd, Keep::Always).filter(|_| at);
        };
        let session = session?;
        let agent = session.agents.iter().find(|a| &a.id == id);
        let Some(folder) = agent.and_then(|a| a.folder.as_deref()) else {
            return look(id, &event.cwd, Keep::Always);
        };
        let worker = |a: &Subagent| a.kind == Kind::Worker;
        let bash = event.tool_name.as_deref() == Some("Bash");
        // A helper's base right before its command, so only a change across it promotes it.
        if bash && name == "PreToolUse" && agent.is_some_and(|a| !worker(a)) {
            return look(id, folder, Keep::Always);
        }
        let bash = step && bash;
        let owner = session.owner(id, folder);
        let owner = owner.and_then(|o| session.agents.iter().find(|a| a.id == o));
        match owner {
            Some(own)
                if &own.id == id && worker(own) && (name == "SubagentStop" || due(&own.since)) =>
            {
                look(id, folder, Keep::Always)
            }
            Some(own) if &own.id == id => look(id, folder, Keep::OnCommit).filter(|_| bash),
            // It works in the worktree of a worker above it, or of its session.
            Some(above) if worker(above) && due(&above.since) => {
                look(&above.id, above.folder.as_deref()?, Keep::Always)
            }
            None if session_due(session) => look(&session.id, &session.cwd, Keep::Always),
            _ => look(id, folder, Keep::OnCommit)
                .filter(|_| bash && agent.is_some_and(|a| !worker(a))),
        }
    }

    /// Whether session `id` is in a turn: prompted, and neither stopped nor ended since.
    fn in_turn(&self, id: &str) -> bool {
        self.sessions.get(id).is_some_and(Session::in_turn)
    }

    /// The sessions in a turn and the running subagents, with their folders, by their fingerprints'
    /// keys. After a replay the shell fingerprints them once, so a restart mid-turn leaves the
    /// turn's `Stop` and each subagent's next look a base to compare with.
    pub fn waiting_folders(&self) -> impl Iterator<Item = (&str, &str)> {
        let waiting = self.sessions.values().filter(|s| self.in_turn(&s.id));
        let agents = self.sessions.values().flat_map(|s| &s.agents);
        let running = agents.filter(|a| a.status == "running");
        let running = running.filter_map(|a| Some((a.id.as_str(), a.folder.as_deref()?)));
        waiting
            .map(|s| (s.id.as_str(), s.cwd.as_str()))
            .chain(running)
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
            .or_insert_with(|| Session {
                id: event.session_id.clone(),
                cwd: String::new(),
                last_step: None,
                state: State::Running,
                started_at: event.studio_received_ms,
                last_event_at: 0,
                checks: Vec::new(),
                running: Vec::new(),
                blocks: Vec::new(),
                asked: false,
                turn_ended: false,
                ended: false,
                versioned: false,
                agents: Vec::new(),
                asker: None,
                prompts: Vec::new(),
                tasks: BTreeSet::new(),
                questions: Vec::new(),
                stopped: None,
                woken: false,
                since: Since {
                    at: event.studio_received_ms,
                    ..Since::default()
                },
            });
        // The hooks are async, so the latest event can land before an earlier one.
        session.last_event_at = session.last_event_at.max(event.studio_received_ms);
        let failed_before = session.failed_workers();
        let its_own = |id: &String| changes.contains_key(id);
        session.track_agents(&event, event.agent_id.as_ref().is_some_and(its_own));
        // A child's events still keep its session from going idle, but its folder and steps are its
        // own.
        let own = event.agent_id.is_none();
        if own && !event.cwd.is_empty() {
            session.cwd.clone_from(&event.cwd);
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
            let folder = session.agents.iter().find(|a| &a.id == agent);
            let folder = folder.and_then(|a| a.folder.clone()).unwrap_or_default();
            let owner = session.owner(agent, &folder);
            if owner.as_ref() != Some(agent) {
                let relative = path.strip_prefix(&folder).and_then(|p| p.strip_prefix('/'));
                let by = &mut session.since(owner.as_deref()).by;
                by.entry(agent.clone())
                    .or_default()
                    .insert(relative.unwrap_or(path).into());
            }
        }
        // A worker's card at its commit, seen after its Bash step, or at its end, and any worktree's
        // after 20 minutes: when its worktree changed since its last card (#369, decision 11). The
        // session's own look at a prompt is its base, and at `Stop` its card's change, below.
        for look in &looks {
            if look.key == session.id && matches!(name, "UserPromptSubmit" | "Stop") {
                continue;
            }
            let found = changes.remove(&look.key);
            let Some(found) = found.filter(|c| look.keep == Keep::Always || c.head != c.base.head)
            else {
                continue;
            };
            let owner = (look.key != session.id).then_some(look.key.as_str());
            if owner.is_none_or(|id| session.has_cards(id)) {
                let request = session.card(owner, Some(found), at);
                if owner.is_some() {
                    session.since.cards.push(request.id.clone());
                }
                outputs.push(Output::Card(request));
            }
        }
        match name {
            // A prompt opens a wait, also after an interrupt, which fires no `Stop`.
            "UserPromptSubmit" => {
                let sender = session.sender(&event.prompt);
                let wake_up = matches!(sender, Sender::WakeUp(_));
                session.woken = wake_up && (session.woken || !session.in_turn());
                // A base for the prompt: what Daniel changed himself before it is no one's.
                if looks.iter().any(|look| look.key == session.id) {
                    session.since.at = at;
                    session.since.by.clear();
                }
                match &sender {
                    // The row says which worker returned until the session's next step.
                    Sender::WakeUp(_) => session.last_step = None,
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
                let workers = !session.since.cards.is_empty();
                let output = match changes.remove(&session.id) {
                    Some(change) => Some(Output::Card(session.card(None, Some(change), at))),
                    None if workers && session.woken => {
                        Some(Output::Brief(session.card(None, None, at)))
                    }
                    None if workers => Some(Output::Card(session.card(None, None, at))),
                    None => None,
                };
                session.since.by.clear();
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
                    session.last_step = Some(Step {
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
                    question.answer = Answer::Answered(event.tool_response["answers"].clone());
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
                && now.saturating_sub(session.last_event_at) >= IDLE_AFTER_MS
            {
                session.state = State::Idle;
                changed = true;
            }
        }
        changed
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
        sessions.sort_by_key(|s| (s.state, Reverse(s.started_at), &s.id));
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
        for agent in agents.filter(|a| a.id == id) {
            agent.opened = true;
            agent.faded = agent.fades();
        }
    }

    /// The session Daniel selected, which the session view shows; `None` until his first click.
    pub fn selected(&self) -> Option<&Session> {
        self.session(self.selected.as_deref()?)
    }

    /// Whose cards and brief the session view shows: the subagent Daniel opened since he selected
    /// its session, else that session; `None` until his first click.
    pub fn viewed(&self) -> Option<&str> {
        self.viewed.as_deref().or(self.selected.as_deref())
    }

    /// The body of a notice on session `id`'s line: its folder and short id, as its row shows them,
    /// and the subagent `agent` it names, by its spawn's description, else its short id.
    pub fn notice_body(&self, id: &str, agent: Option<&str>) -> String {
        let session = self.session(id);
        let folder = session.and_then(|s| s.cwd.rsplit('/').next());
        let mut body = format!("{} {}", folder.unwrap_or_default(), short(id, 8));
        if let Some(agent) = agent {
            let child = session.and_then(|s| s.agents.iter().find(|a| a.id == agent));
            let name = child.and_then(|a| a.description.as_deref());
            body = format!("{body} · {}", name.unwrap_or(short(agent, 5)));
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
