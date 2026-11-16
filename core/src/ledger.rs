//! The step ledger (danielrispler/plotkeeper#110): one row for every tool call of an agent session,
//! its workers' calls included. A row keeps what the rail needs to rank and count a turn's work, and
//! where in the event log its two events are (`Bytes`). It keeps no output and no hunk text: those
//! are read on demand, by one seek and one read of the event's length (`Row::details`), so a read
//! costs the same on a large log as on a small one.

use crate::event::{Event, Tool};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

/// Where one event's line is in the event log: its byte offset from the file's start, and its byte
/// length without the line's newline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Bytes {
    pub offset: u64,
    pub len: u64,
}

/// How a call ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    /// It failed; `exit_code` is a command's, when its error names one.
    Failed {
        exit_code: Option<i32>,
    },
    /// Daniel rejected it.
    Rejected,
}

/// One tool call of a session.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Row {
    /// The call's id (`tool_use_id`), unique among the rows of its agent.
    pub call: String,
    pub tool: String,
    /// The file it works on, as its input names it.
    pub file: Option<String>,
    pub command: Option<String>,
    pub pattern: Option<String>,
    /// The turn it is in (`Session::turns`): for a subagent's call, the turn that spawned it.
    pub turn: Option<usize>,
    /// The agent's id of the prompt the call ran under.
    pub prompt_id: Option<String>,
    /// The subagent that made it, `None` for the session's own agent.
    pub agent: Option<String>,
    /// When its start was received, in Unix milliseconds; `None` while only its result has landed.
    pub started_at: Option<u64>,
    /// How long it took: what the agent reports, else from its start to its result.
    pub duration_ms: Option<u64>,
    /// Whether the command that failed ran: one Daniel denied or that was interrupted did not.
    pub ran: bool,
    /// Whether its result is a command's start in the background, which has no result yet.
    pub background: bool,
    /// `None` until its result lands.
    pub outcome: Option<Outcome>,
    /// The log lines it came from: its start, and its result.
    pub start: Option<Bytes>,
    pub end: Option<Bytes>,
    /// When its result was received.
    #[serde(skip)]
    ended_at: Option<u64>,
    /// What the agent reported as its duration.
    #[serde(skip)]
    took: Option<u64>,
}

/// The two events of a call, as the hook logged them: the start's line and the result's line, parsed.
/// Everything the hook wrote is there: a command's input, a response's output, an error's text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    pub start: Option<Value>,
    pub end: Option<Value>,
}

impl Row {
    /// Reads the two events of this call from the event log at `log`: one seek and one read of each
    /// line's length. The log must be the file the offsets were taken from.
    pub fn details(&self, log: &Path) -> io::Result<Details> {
        let mut file = File::open(log)?;
        let mut read = |at: Option<Bytes>| -> io::Result<Option<Value>> {
            let Some(at) = at else { return Ok(None) };
            file.seek(SeekFrom::Start(at.offset))?;
            let mut line = Vec::new();
            (&mut file).take(at.len).read_to_end(&mut line)?;
            Ok(serde_json::from_slice(&line).ok())
        };
        Ok(Details {
            start: read(self.start)?,
            end: read(self.end)?,
        })
    }

    /// Whether it ended in failure.
    pub fn failed(&self) -> bool {
        matches!(self.outcome, Some(Outcome::Failed { .. }))
    }
}

/// A session's rows, in the order their first event came.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ledger {
    rows: Vec<Row>,
    /// The row of each call, by its agent and its id. Agy numbers a call by its step in the
    /// conversation, so two agents' ids can be the same.
    index: HashMap<(Option<String>, String), usize>,
}

impl Ledger {
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The rows of turn `turn`, its workers' included.
    pub fn turn(&self, turn: usize) -> impl Iterator<Item = &Row> {
        self.rows.iter().filter(move |r| r.turn == Some(turn))
    }

    /// The rows of calls whose input names file `path`.
    pub fn file<'a>(&'a self, path: &'a str) -> impl Iterator<Item = &'a Row> {
        self.rows
            .iter()
            .filter(move |r| r.file.as_deref() == Some(path))
    }

    /// The row of call `call` of `agent`.
    pub fn row(&self, agent: Option<&str>, call: &str) -> Option<&Row> {
        let key = (agent.map(Into::into), call.into());
        self.index.get(&key).map(|&i| &self.rows[i])
    }

    /// The row of the call `event` starts or ends, made when it is the call's first event: a result
    /// can land before its start, as the hooks are async.
    fn of(&mut self, event: &Event, tool: &Tool, turn: Option<usize>) -> &mut Row {
        let call = event.call_id();
        let key = (event.agent_id.clone(), call.to_string());
        // A call with no id cannot be matched to another event, so each event of it is a row.
        let found = Some(call)
            .filter(|c| !c.is_empty())
            .and_then(|_| self.index.get(&key));
        let index = match found {
            Some(&index) => index,
            None => {
                let input = &tool.input;
                let text = |key: &str| input[key].as_str().map(String::from);
                self.rows.push(Row {
                    call: call.into(),
                    tool: tool.name.clone(),
                    file: ["file_path", "notebook_path", "path"]
                        .iter()
                        .find_map(|k| text(k)),
                    command: tool.command.clone(),
                    pattern: text("pattern"),
                    turn,
                    prompt_id: event.prompt_id.clone(),
                    agent: event.agent_id.clone(),
                    ..Row::default()
                });
                let index = self.rows.len() - 1;
                if !call.is_empty() {
                    self.index.insert(key, index);
                }
                index
            }
        };
        &mut self.rows[index]
    }

    /// Takes the start of `tool`'s call, which `turn` holds.
    pub(crate) fn start(&mut self, event: &Event, tool: &Tool, turn: Option<usize>) {
        let at = event.studio_received_ms;
        let row = self.of(event, tool, turn);
        row.started_at = Some(at);
        row.start = Some(event.line);
        row.settle();
    }

    /// Takes the result of `tool`'s call.
    pub(crate) fn end(&mut self, event: &Event, tool: &Tool, turn: Option<usize>, failed: bool) {
        let at = event.studio_received_ms;
        let outcome = match failed {
            false => Outcome::Succeeded,
            true if event.rejected() => Outcome::Rejected,
            true => Outcome::Failed {
                exit_code: event.exit_code(),
            },
        };
        let took = event.took();
        let (ran, background) = (event.ran(), event.background_task().is_some());
        let row = self.of(event, tool, turn);
        row.ended_at = Some(at);
        row.took = took;
        row.outcome = Some(outcome);
        row.ran = ran;
        row.background = background;
        row.end = Some(event.line);
        row.settle();
    }
}

impl Row {
    /// Its duration, from what the agent reported, else from its start to its result.
    fn settle(&mut self) {
        let span = self.ended_at.zip(self.started_at);
        self.duration_ms = self
            .took
            .or_else(|| span.map(|(end, start)| end.saturating_sub(start)));
    }
}
