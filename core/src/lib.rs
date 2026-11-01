//! The studio core (#328, module 3): plain Rust with no I/O. It takes hook events one at a time, in
//! event log order, and keeps the state the app shows. App start replays the whole log through
//! `replay`; live events go through `apply`, which updates state the same way and also returns what
//! the shell must do.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
};

#[derive(Default)]
pub struct Studio {
    sessions: HashMap<String, Session>,
    /// The session Daniel clicked last, which the session view shows.
    selected: Option<String>,
    /// The shell's latest tick, in Unix milliseconds.
    now: u64,
    /// Sessions in a wait that has not raised its guess prompt yet.
    unguessed: HashSet<String>,
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
    /// The open guess prompt: its check started and its result has not arrived.
    pub guess: Option<GuessPrompt>,
    /// The checks that ran, oldest first: pass or fail lines in the session's timeline (#369,
    /// decision 14).
    pub checks: Vec<CheckLine>,
    /// The tool calls whose `PreToolUse` landed and whose `PostToolUse` has not.
    #[serde(skip)]
    running: Vec<Call>,
    /// The tool calls blocked on Daniel.
    #[serde(skip)]
    blocks: Vec<Call>,
    /// A block no tool call clears: a question in the turn's final message.
    #[serde(skip)]
    asked: bool,
    #[serde(skip)]
    turn_ended: bool,
    #[serde(skip)]
    ended: bool,
    /// Its subagents at any depth, in the order they were first seen (#369, decision 5).
    pub agents: Vec<Agent>,
    /// What it waits on while it needs Daniel.
    pub waiting: Option<Wait>,
    /// The questions its agents asked, oldest first (#369, decision 21).
    pub questions: Vec<Question>,
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
pub struct Agent {
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
    /// Daniel opened its line.
    pub opened: bool,
    /// A helper that succeeded, or one that failed and Daniel opened, folds into its parent's line
    /// (#369, story 18). A failed child stays visible until he opens it.
    pub faded: bool,
    #[serde(skip)]
    last_event_at: u64,
}

impl Agent {
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

    fn fades(&self) -> bool {
        let succeeded = self.status == "completed";
        self.kind == Kind::Helper && self.status != "running" && (succeeded || self.opened)
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
    fn agent(&mut self, id: &str, at: u64) -> &mut Agent {
        let index = match self.agents.iter().position(|a| a.id == id) {
            Some(index) => index,
            None => {
                self.agents.push(Agent {
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
                    opened: false,
                    faded: false,
                    last_event_at: at,
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
                for notice in event.prompt.split("<task-notification>").skip(1) {
                    let (Some(id), Some(status)) = (tag(notice, "task-id"), tag(notice, "status"))
                    else {
                        continue;
                    };
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

    fn needs_you(&self) -> bool {
        self.asked || !self.blocks.is_empty()
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

    /// Clears the blocks of the session's own agent (#328, module 3, "Needs you": `UserPromptSubmit`,
    /// `Stop`, `SessionEnd`). The turn's calls end with it, so a call that never reported back takes
    /// no later block. Subagents run past their session's turn, so their blocks stay.
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
    /// Blocked on Daniel (#328, module 3, "Needs you"), until the step that asks resolves, a prompt,
    /// `Stop` or `SessionEnd`.
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
    /// A session's worktree changed since its last card; ask the card writer.
    Card(CardRequest),
    /// The wait's first check started: ask "Will it pass?".
    Guess(GuessPrompt),
    /// A session newly needs Daniel.
    NeedsYou { session_id: String },
}

/// "Will it pass?" on the wait's first check (#328, module 3, "Guesses"). It is open from the
/// check's `PreToolUse` until its result arrives.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GuessPrompt {
    pub session_id: String,
    pub tool_use_id: String,
    /// The check's command, which the prompt shows.
    pub step: Step,
    /// Daniel's tap: whether he guessed the check will pass. `None` until he taps.
    pub guess: Option<bool>,
}

/// A session's change since its last card (#369, decision 10), with what the card writer needs: the
/// diff, not the payloads. The card store keeps it with its card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardRequest {
    pub session_id: String,
    /// Names the card: its session, `@`, and when the hook received the `Stop` it was written at.
    pub id: String,
    pub base: Base,
    /// The diff of each file that changed since the base, commits included.
    pub change: String,
    /// What settled a guess on a check's card. Checks are lines now, so it is `None`.
    // shortcut: kept for the store's settling and `weekly` until #375 replaces the guess.
    #[serde(default)]
    pub check: Option<Check>,
}

/// A check that ran, shown as a pass or fail line.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheckLine {
    /// The command's first line.
    pub command: String,
    /// Whether it passed, when its exit code tells, and Daniel's tap.
    pub check: Check,
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
}

/// A check's guess and exit code (#328, module 4, "Settling a guess").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    /// Daniel's tap: whether he guessed it would pass. `None` when he did not tap before the result.
    pub guess: Option<bool>,
    /// Whether the command exited 0, when that says whether the check passed: nothing in the command
    /// masks the check's exit code (`exit_code_counts`), and it ran. `None` otherwise.
    pub exit_passed: Option<bool>,
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

/// What a session that needs Daniel waits on: its oldest block's asker, a subagent or `None` for the
/// session's own agent, and since when, in Unix milliseconds (#369, decision 21).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Wait {
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
    /// The session, and the folder whose worktree the shell fingerprints for it (#328, module 5),
    /// before it applies this live line, or `None` (#369, decision 10). At a prompt, the fingerprint
    /// is the prompt's base, so what Daniel changed himself since the last card does not count. A
    /// prompt inside a turn still open takes none: an interrupt fires no `Stop`, and a wake-up can
    /// land mid-turn, so that turn's change stays in its card. At `Stop`, the fingerprint's change
    /// since the base is the card's, and it is the next card's base.
    pub fn fingerprint_folder(&self, line: &str) -> Option<(String, String)> {
        let event = serde_json::from_str::<Event>(line).ok()?;
        let at = match event.hook_event_name.as_str() {
            "UserPromptSubmit" => !self.in_turn(&event.session_id),
            "Stop" => true,
            _ => false,
        };
        // A child's folder is its own worktree, not its session's.
        let own = event.agent_id.is_none();
        (at && own && !event.cwd.is_empty()).then_some((event.session_id, event.cwd))
    }

    /// Whether session `id` is in a turn: prompted, and neither stopped nor ended since.
    fn in_turn(&self, id: &str) -> bool {
        let session = self.sessions.get(id);
        session.is_some_and(|s| !s.turn_ended && !s.ended)
    }

    /// The sessions in a turn, with their folders. After a replay the shell fingerprints them once,
    /// so a restart mid-turn leaves the turn's `Stop` a base to compare with.
    pub fn waiting_folders(&self) -> impl Iterator<Item = (&str, &str)> {
        let waiting = self.sessions.values().filter(|s| self.in_turn(&s.id));
        waiting.map(|s| (s.id.as_str(), s.cwd.as_str()))
    }

    /// Takes one live event log line that the shell took no fingerprint for, and returns what the
    /// shell must do.
    pub fn apply(&mut self, line: &str) -> Vec<Output> {
        self.apply_with_change(line, None)
    }

    /// Takes one live event log line, with what the shell's fingerprint before it found: the
    /// worktree's change since the session's base, or `None` when nothing changed or the shell took
    /// none. Returns what the shell must do. A line that is not a hook event is skipped.
    pub fn apply_with_change(&mut self, line: &str, change: Option<Change>) -> Vec<Output> {
        let Ok(event) = serde_json::from_str::<Event>(line) else {
            return Vec::new();
        };
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
                guess: None,
                checks: Vec::new(),
                running: Vec::new(),
                blocks: Vec::new(),
                asked: false,
                turn_ended: false,
                ended: false,
                agents: Vec::new(),
                waiting: None,
                questions: Vec::new(),
            });
        // The hooks are async, so the latest event can land before an earlier one.
        session.last_event_at = session.last_event_at.max(event.studio_received_ms);
        session.track_agents(&event, change.is_some());
        // A child's events still keep its session from going idle, but its folder and steps are its
        // own.
        let own = event.agent_id.is_none();
        if own && !event.cwd.is_empty() {
            session.cwd.clone_from(&event.cwd);
        }
        let mut outputs = Vec::new();
        let mut blocked_before = session.needs_you();
        let tool = event.tool_name.unwrap_or_default();
        let id = Some(event.tool_use_id.clone()).filter(|id| !id.is_empty());
        let at = event.studio_received_ms;
        match event.hook_event_name.as_str() {
            // A prompt opens a wait, also after an interrupt, which fires no `Stop`.
            "UserPromptSubmit" => {
                self.unguessed.insert(event.session_id.clone());
                session.unblock();
                session.turn_ended = false;
                session.guess = None;
            }
            // A turn that ended ran all its calls, so a prompt still open lost its result.
            "Stop" => {
                self.unguessed.remove(&event.session_id);
                session.guess = None;
                // The order on `Stop`: clear its own blocks first, then ask if the message asks.
                session.unblock();
                session.turn_ended = true;
                // A subagent's wait outlasts the turn, so it gives no second notice.
                blocked_before = session.needs_you();
                session.asked = asks(&event.last_assistant_message);
                if let Some(Change { base, diff }) = change {
                    outputs.push(Output::Card(CardRequest {
                        session_id: event.session_id.clone(),
                        id: format!("{}@{}", event.session_id, event.studio_received_ms),
                        base,
                        change: diff,
                        check: None,
                    }));
                }
            }
            "SessionEnd" => {
                self.unguessed.remove(&event.session_id);
                session.unblock();
                // Its subagents end with it.
                session.blocks.clear();
                session.running.clear();
                session.turn_ended = true;
                session.ended = true;
                session.guess = None;
            }
            // A resumed session keeps its id; a compaction keeps the turn it happens in.
            "SessionStart" => {
                session.ended = false;
                session.turn_ended |= event.source != "compact";
            }
            "PreToolUse" => {
                let step = Step {
                    tool: tool.clone(),
                    detail: step_detail(&event.tool_input, &event.cwd),
                };
                if own {
                    session.last_step = Some(step.clone());
                }
                // The prompt comes at the check's start, before its result. A child's checks are its
                // own, so they take no guess and add no line.
                if own
                    && runs_check(&tool, &event.tool_input)
                    && self.unguessed.remove(&event.session_id)
                {
                    let prompt = GuessPrompt {
                        session_id: event.session_id.clone(),
                        tool_use_id: event.tool_use_id.clone(),
                        step,
                        guess: None,
                    };
                    session.guess = Some(prompt.clone());
                    outputs.push(Output::Guess(prompt));
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
                let guessed = session
                    .guess
                    .take_if(|p| p.tool_use_id == event.tool_use_id);
                if !ran {
                    // A check that did not run leaves the wait's guess to a later check.
                    if guessed.is_some() {
                        self.unguessed.insert(event.session_id.clone());
                    }
                // A check gets its line whether it passed or failed. The hooks are async, so a fast
                // check's result can land before its start: it takes the wait's guess with none.
                } else if own && runs_check(&tool, input) {
                    self.unguessed.remove(&event.session_id);
                    let command = input["command"].as_str().unwrap_or_default();
                    session.checks.push(CheckLine {
                        command: command.lines().next().unwrap_or_default().into(),
                        check: Check {
                            guess: guessed.and_then(|prompt| prompt.guess),
                            exit_passed: exit_code_counts(command).then_some(!failed),
                        },
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
        session.waiting = session.blocks.first().map(|block| Wait {
            agent: block.agent.clone(),
            since: block.at,
        });
        if !blocked_before && session.state == State::NeedsYou {
            outputs.push(Output::NeedsYou {
                session_id: event.session_id,
            });
        }
        outputs
    }

    /// Takes one event log line replayed at app start. It updates state exactly like `apply` and
    /// requests nothing, so turns that ended while the app was down get no cards and no notices.
    pub fn replay(&mut self, line: &str) {
        self.apply(line);
    }

    /// Daniel's tap, yes or no, on the guess prompt of check `tool_use_id`, a fact from the shell. A
    /// tap after the check's result arrived finds no open prompt, so that guess stays none. The
    /// first tap counts.
    pub fn guess(&mut self, tool_use_id: &str, will_pass: bool) {
        let open = self.sessions.values_mut().filter_map(|s| s.guess.as_mut());
        for prompt in open.filter(|p| p.tool_use_id == tool_use_id) {
            prompt.guess.get_or_insert(will_pass);
        }
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

    /// Daniel clicked session `id`'s row. Nothing else changes the selection.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.into());
    }

    /// Daniel opened subagent `id`'s line, so a failed helper folds into its parent's line.
    pub fn open(&mut self, id: &str) {
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
}
