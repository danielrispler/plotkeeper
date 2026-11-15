//! The neutral events the session rules react to (danielrispler/plotkeeper#43, decisions 2 and 3).
//! An agent adapter (`claude.rs`) turns one hook line into an `Event`: the context every line
//! shares, and what happened. The rules read these, never an agent's own event or tool names.
//!
//! The shared events are those every agent reports. What only one agent reports is a variant of
//! its own, which only that agent's adapter emits, never an optional field of a shared event.

use serde_json::Value;
use std::collections::BTreeMap;

/// One hook line, as the session rules read it.
pub(crate) struct Event {
    pub session_id: String,
    /// The subagent that sent the event, or `None` for the session's own agent.
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub cwd: String,
    /// The session's transcript.
    pub transcript_path: String,
    /// The agent's id of the turn the event belongs to, absent until the first user input.
    pub prompt_id: Option<String>,
    pub studio_received_ms: u64,
    /// The terminal the session runs in, which the hook records from `STUDIO_TERMINAL_ID`.
    pub studio_terminal_id: Option<String>,
    /// The trees the hook recorded at a turn's first touch of each worktree, by root.
    pub studio_turn_start: BTreeMap<String, String>,
    /// The trees the hook recorded at the end of the turn this event ends, by root.
    pub studio_turn_end: BTreeMap<String, String>,
    /// What the adapter that read the line reports.
    pub capabilities: Capabilities,
    /// What happened, the line's main event first. A line the adapter does not know has none.
    pub happened: Vec<Happened>,
}

/// What a line reports.
pub(crate) enum Happened {
    // Shared by every agent.
    SessionStart,
    SessionEnd,
    /// A turn starts, by Daniel's prompt or a wake-up.
    TurnStart {
        prompt: String,
        /// Where the turn came from, when the adapter can tell from the agent's own record. `None`
        /// leaves it to the prompt's text, as Claude's wake-ups name a task the session knows.
        origin: Option<Origin>,
    },
    /// A turn ends; `background_pending` when work goes on in the background.
    TurnEnd {
        background_pending: bool,
    },
    ToolStart {
        tool: Tool,
        id: String,
    },
    ToolEnd {
        tool: Tool,
        id: String,
        failed: bool,
    },
    /// A subagent was spawned.
    SubagentSpawned(Spawned),
    /// The subagent that sent the line ended.
    SubagentEnd,
    /// The model the session runs at, as the agent last said.
    Model(String),
    /// The git branch of the session's workspace.
    Branch(String),

    // Claude only.
    /// How a session started.
    StartedFrom(Start),
    /// The session ended by a `/clear`.
    EndedByClear,
    PermissionAsked {
        tool: Tool,
    },
    /// A notice that a permission waits on Daniel.
    PermissionNotice,
    /// A notice that the agent waits for input.
    IdlePrompt,
    /// Any other notice.
    OtherNotice,
    ElicitationAsked,
    /// What the agent's last message said, on a turn's or a subagent's end.
    Reply(String),
    /// The tasks running in the background, by id, when a turn ends.
    BackgroundTasks(Vec<String>),
    /// The id of the background task a tool call started.
    BackgroundTask(String),
    /// The tasks a wake-up reports on, each with how it ended.
    TasksReported(Vec<(String, String)>),
    /// A failed command that ran and exited with a code.
    ExitedNonZero,
    /// A failed call that Daniel rejected.
    Rejected,
    /// A question's answers, as strings by question.
    Answers(Value),
    /// How long a call took, in milliseconds.
    Took(u64),
    PermissionMode(String),

    // The studio's own lines.
    Renamed(String),
    /// A name the studio made from Daniel's first prompt.
    PromptNamed(String),
    Deleted,
    /// Daniel answered from the studio's card.
    Answered {
        tool: Tool,
        id: String,
        answer: Value,
    },
}

/// Where a turn came from, where an adapter can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Daniel,
    WakeUp,
}

/// How a session started, where an agent says.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Start {
    Clear,
    Fork,
    Resume,
    Compact,
    Other,
}

/// A tool call: what it is called, its input, and what the rules treat it as.
#[derive(Clone, Default)]
pub(crate) struct Tool {
    pub name: String,
    pub input: Value,
    pub kind: ToolKind,
    /// The command it runs, when its input has one.
    pub command: Option<String>,
    /// The file it edits, when it edits one.
    pub path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) enum ToolKind {
    #[default]
    Other,
    Edit,
    Command,
    /// Spawns a subagent.
    Spawn,
    /// Runs a skill, by name.
    Skill(Option<String>),
    /// Asks Daniel a question, in any permission mode.
    Question,
    /// Asks Daniel to approve a plan, in any permission mode.
    Plan,
    /// A tool of an external server.
    External,
}

impl Tool {
    /// Whether it asks Daniel, before any permission request.
    pub fn asks(&self) -> bool {
        matches!(self.kind, ToolKind::Question | ToolKind::Plan)
    }
}

/// A spawn's response: the child it started.
pub(crate) struct Spawned {
    pub child: String,
    /// The spawn call's id.
    pub call: String,
    pub name: Option<String>,
    pub prompt: String,
    pub agent_type: Option<String>,
    /// It works in a worktree of its own.
    pub worktree: bool,
    /// It ran in the foreground and has ended.
    pub finished: bool,
}

/// What an adapter reports, which decides what its sessions' views show (decision 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct Capabilities {
    /// It reports a permission or a question that waits on Daniel, with its text.
    pub asks: bool,
    /// It names the tasks running in the background.
    pub background_tasks: bool,
    /// It reports a subagent's last message.
    pub replies: bool,
    /// It reports a session's permission mode.
    pub permission_mode: bool,
    /// It reports how long a call took.
    pub durations: bool,
    /// Its transcript carries the agent's version.
    pub transcript_version: bool,
}

impl Event {
    pub fn has(&self, f: impl Fn(&Happened) -> bool) -> bool {
        self.happened.iter().any(f)
    }

    fn find<'a, T>(&'a self, f: impl Fn(&'a Happened) -> Option<T>) -> Option<T> {
        self.happened.iter().find_map(f)
    }

    pub fn session_starts(&self) -> bool {
        self.has(|h| matches!(h, Happened::SessionStart))
    }

    pub fn turn_ends(&self) -> bool {
        self.has(|h| matches!(h, Happened::TurnEnd { .. }))
    }

    /// The prompt of the turn it starts.
    pub fn prompt(&self) -> Option<&str> {
        self.find(|h| match h {
            Happened::TurnStart { prompt, .. } => Some(prompt.as_str()),
            _ => None,
        })
    }

    pub fn started_from(&self) -> Option<Start> {
        self.find(|h| match h {
            Happened::StartedFrom(start) => Some(*start),
            _ => None,
        })
    }

    pub fn is_start_from(&self, start: Start) -> bool {
        self.session_starts() && self.started_from() == Some(start)
    }

    /// The tool of its call, whatever the call's event.
    pub fn tool(&self) -> Option<&Tool> {
        self.find(|h| match h {
            Happened::ToolStart { tool, .. }
            | Happened::ToolEnd { tool, .. }
            | Happened::PermissionAsked { tool }
            | Happened::Answered { tool, .. } => Some(tool),
            _ => None,
        })
    }

    /// The id of its call, empty when it names none.
    pub fn call_id(&self) -> &str {
        self.find(|h| match h {
            Happened::ToolStart { id, .. }
            | Happened::ToolEnd { id, .. }
            | Happened::Answered { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .unwrap_or_default()
    }

    /// Whether it starts a tool call.
    pub fn tool_starts(&self) -> bool {
        self.has(|h| matches!(h, Happened::ToolStart { .. }))
    }

    /// Whether it ends a tool call, in success or failure.
    pub fn tool_ends(&self) -> bool {
        self.has(|h| matches!(h, Happened::ToolEnd { .. }))
    }

    /// Whether it ends a call that succeeded.
    pub fn tool_succeeds(&self) -> bool {
        self.has(|h| matches!(h, Happened::ToolEnd { failed: false, .. }))
    }

    /// Whether it ends a call that failed.
    pub fn tool_fails(&self) -> bool {
        self.has(|h| matches!(h, Happened::ToolEnd { failed: true, .. }))
    }

    /// Whether the command it ends ran: it succeeded, or failed with an exit code.
    pub fn ran(&self) -> bool {
        !self.tool_fails() || self.has(|h| matches!(h, Happened::ExitedNonZero))
    }

    pub fn rejected(&self) -> bool {
        self.has(|h| matches!(h, Happened::Rejected))
    }

    /// A start, an end or a notice: one that never starts a turn or touches a worktree, since in
    /// the day log they carry ids no turn ran under.
    pub fn outside_turns(&self) -> bool {
        self.has(|h| {
            matches!(
                h,
                Happened::SessionStart
                    | Happened::SessionEnd
                    | Happened::PermissionNotice
                    | Happened::IdlePrompt
                    | Happened::OtherNotice
            )
        })
    }

    pub fn reply(&self) -> &str {
        self.find(|h| match h {
            Happened::Reply(reply) => Some(reply.as_str()),
            _ => None,
        })
        .unwrap_or_default()
    }

    pub fn background_tasks(&self) -> Option<&[String]> {
        self.find(|h| match h {
            Happened::BackgroundTasks(tasks) => Some(tasks.as_slice()),
            _ => None,
        })
    }

    pub fn background_task(&self) -> Option<&str> {
        self.find(|h| match h {
            Happened::BackgroundTask(task) => Some(task.as_str()),
            _ => None,
        })
    }

    /// The tasks a wake-up reports on, each with how it ended.
    pub fn reported(&self) -> &[(String, String)] {
        self.find(|h| match h {
            Happened::TasksReported(tasks) => Some(tasks.as_slice()),
            _ => None,
        })
        .unwrap_or_default()
    }

    pub fn answers(&self) -> Option<&Value> {
        self.find(|h| match h {
            Happened::Answers(answers) => Some(answers),
            _ => None,
        })
    }

    pub fn took(&self) -> Option<u64> {
        self.find(|h| match h {
            Happened::Took(ms) => Some(*ms),
            _ => None,
        })
    }

    pub fn permission_mode(&self) -> Option<&str> {
        self.find(|h| match h {
            Happened::PermissionMode(mode) => Some(mode.as_str()),
            _ => None,
        })
    }
}

impl Event {
    pub fn model(&self) -> Option<&str> {
        self.find(|h| match h {
            Happened::Model(model) => Some(model.as_str()),
            _ => None,
        })
    }

    pub fn branch(&self) -> Option<&str> {
        self.find(|h| match h {
            Happened::Branch(branch) => Some(branch.as_str()),
            _ => None,
        })
    }
}
