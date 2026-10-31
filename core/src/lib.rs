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
    /// Sessions in a wait that has not had its card step yet.
    awaiting_card: HashSet<String>,
    /// Steps that already have a card in the card store, by `tool_use_id`.
    carded: HashSet<String>,
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
}

impl Session {
    fn needs_you(&self) -> bool {
        self.asked || !self.blocks.is_empty()
    }

    /// Adds a block on the latest running call it waits on, so that the call's `PostToolUse` clears
    /// it. With none, the call's `PreToolUse` may still land (the hooks are async); otherwise the
    /// block lasts until the turn ends.
    fn block(&mut self, tool: String, input: Value) {
        let mut block = Call {
            id: None,
            tool,
            input,
        };
        let call = self.running.iter().rfind(|c| waits_on(&block, c));
        block.id = call.and_then(|c| c.id.clone());
        self.blocks.push(block);
    }

    /// Clears every block (#328, module 3, "Needs you": `UserPromptSubmit`, `Stop`, `SessionEnd`).
    /// The turn's calls end with it, so a call that never reported back takes no later block.
    fn unblock(&mut self) {
        self.blocks.clear();
        self.running.clear();
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
    /// A card step finished; ask the card writer.
    Card(CardRequest),
    /// A check that is the wait's card step started: ask "Will it pass?".
    Guess(GuessPrompt),
    /// A session newly needs Daniel.
    NeedsYou { session_id: String },
}

/// "Will it pass?" on a check that is the wait's card step (#328, module 3, "Guesses"). It is open
/// from the check's `PreToolUse` until its result arrives.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct GuessPrompt {
    pub session_id: String,
    pub tool_use_id: String,
    /// The check's command, which the prompt shows.
    pub step: Step,
    /// Daniel's tap: whether he guessed the check will pass. `None` until he taps.
    pub guess: Option<bool>,
}

/// A finished card step, with what the card writer needs: the step's change, not the whole payload.
/// The card store keeps it with its card.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CardRequest {
    pub session_id: String,
    /// The step's tool call, which names the card.
    pub tool_use_id: String,
    pub step: Step,
    /// The diff lines of an edit, a new file's content as added lines, the diff of the files a Bash
    /// step changed, or a check's output.
    pub change: String,
    /// What settles the guess on a check, with the card writer's verdict; `None` for other steps.
    #[serde(default)]
    pub check: Option<Check>,
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
/// names it by.
#[derive(Clone, Debug, PartialEq)]
struct Call {
    id: Option<String>,
    tool: String,
    input: Value,
}

/// The fields of a hook payload the core reads; the rest of the line is ignored.
#[derive(Deserialize)]
struct Event {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Value,
    #[serde(default)]
    tool_use_id: String,
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

/// A card step's change, or `None` for any other step. The card steps this version knows: an Edit
/// or Write outside the plan files (`~/.claude/plans/`), and a Bash command that does not start with
/// `git` and changed the worktree, which the shell's fingerprint tells. A check is the card step from
/// its `PreToolUse` instead (`is_check`).
fn card_change(
    tool: &str,
    input: &Value,
    response: &Value,
    worktree_change: Option<String>,
) -> Option<String> {
    let plan_file = input["file_path"]
        .as_str()
        .is_some_and(|path| path.contains("/.claude/plans/"));
    let git = input["command"]
        .as_str()
        .is_some_and(|command| command.split_whitespace().next() == Some("git"));
    match tool {
        "Edit" | "Write" if !plan_file => Some(change(response)),
        "Bash" if !git => worktree_change,
        _ => None,
    }
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

/// An edit's change from its result: the patch's hunks, or a new file's content, which has none.
fn change(response: &Value) -> String {
    let hunks = response["structuredPatch"].as_array().into_iter().flatten();
    let patch: String = hunks
        .map(|hunk| {
            let lines = hunk["lines"].as_array().into_iter().flatten();
            let lines: String = lines
                .map(|line| format!("{}\n", line.as_str().unwrap_or_default()))
                .collect();
            let (old, old_lines) = (&hunk["oldStart"], &hunk["oldLines"]);
            let (new, new_lines) = (&hunk["newStart"], &hunk["newLines"]);
            format!("@@ -{old},{old_lines} +{new},{new_lines} @@\n{lines}")
        })
        .collect();
    if !patch.is_empty() {
        return patch;
    }
    let content = response["content"].as_str().unwrap_or_default().lines();
    content.map(|line| format!("+{line}\n")).collect()
}

/// A finished command's output, or its error when it failed.
fn output(response: &Value, error: &str) -> String {
    if !error.is_empty() {
        return error.into();
    }
    let streams = [&response["stdout"], &response["stderr"]];
    let streams = streams
        .iter()
        .filter_map(|s| s.as_str().filter(|s| !s.is_empty()));
    streams.collect::<Vec<_>>().join("\n")
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
    /// The folder whose worktree the shell fingerprints (#328, module 5) before it applies this live
    /// line, or `None`. It takes one at a prompt, so the wait's first step is compared with the
    /// worktree as Daniel left it rather than with what he changed himself since the last wait, and
    /// at each step's end while the wait has not had its card step, so a Bash step is compared with
    /// the step before it.
    pub fn fingerprint_folder(&self, line: &str) -> Option<String> {
        let event = serde_json::from_str::<Event>(line).ok()?;
        let at = match event.hook_event_name.as_str() {
            "UserPromptSubmit" => true,
            "PostToolUse" | "PostToolUseFailure" => self.awaiting_card.contains(&event.session_id),
            _ => false,
        };
        (at && !event.cwd.is_empty()).then_some(event.cwd)
    }

    /// The folders of the sessions in a wait that has not had its card step. After a replay the
    /// shell fingerprints them once, so a restart mid-wait leaves the wait's next Bash step a record
    /// to compare with.
    pub fn waiting_folders(&self) -> impl Iterator<Item = &str> {
        let waiting = self.awaiting_card.iter();
        waiting.filter_map(|id| Some(self.sessions.get(id)?.cwd.as_str()))
    }

    /// Takes one live event log line that the shell took no fingerprint for, and returns what the
    /// shell must do.
    pub fn apply(&mut self, line: &str) -> Vec<Output> {
        self.apply_with_change(line, None)
    }

    /// Takes one live event log line, with what the shell's fingerprint after it found: the diff of
    /// the files that changed since the previous fingerprint, or `None` when nothing changed or the
    /// shell took none. Returns what the shell must do. A line that is not a hook event is skipped.
    pub fn apply_with_change(
        &mut self,
        line: &str,
        worktree_change: Option<String>,
    ) -> Vec<Output> {
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
                running: Vec::new(),
                blocks: Vec::new(),
                asked: false,
                turn_ended: false,
                ended: false,
            });
        // The hooks are async, so the latest event can land before an earlier one.
        session.last_event_at = session.last_event_at.max(event.studio_received_ms);
        if !event.cwd.is_empty() {
            session.cwd.clone_from(&event.cwd);
        }
        let mut outputs = Vec::new();
        let mut blocked_before = session.needs_you();
        let tool = event.tool_name.unwrap_or_default();
        let id = Some(event.tool_use_id.clone()).filter(|id| !id.is_empty());
        match event.hook_event_name.as_str() {
            // A prompt opens a wait, also after an interrupt, which fires no `Stop`.
            "UserPromptSubmit" => {
                self.awaiting_card.insert(event.session_id.clone());
                session.unblock();
                session.turn_ended = false;
                session.guess = None;
            }
            // A turn that ended ran all its calls, so a prompt still open lost its result.
            "Stop" => {
                self.awaiting_card.remove(&event.session_id);
                session.guess = None;
                // The order on `Stop`: clear every block first, then ask if the message asks.
                session.unblock();
                session.turn_ended = true;
                blocked_before = false;
                session.asked = asks(&event.last_assistant_message);
            }
            "SessionEnd" => {
                self.awaiting_card.remove(&event.session_id);
                session.unblock();
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
                session.last_step = Some(step.clone());
                // A check is the wait's card step from its start, so its prompt comes before its
                // result.
                if runs_check(&tool, &event.tool_input)
                    && self.awaiting_card.remove(&event.session_id)
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
                };
                if ASKING_TOOLS.contains(&call.tool.as_str()) {
                    session.blocks.push(call.clone());
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
                }
                session.running.push(call);
            }
            // It has no `tool_use_id`, so it names its call by tool and input. The prompt's
            // Notification may have landed first (the hooks are async), and its block names no call.
            "PermissionRequest" if !ASKING_TOOLS.contains(&tool.as_str()) => {
                session.blocks.retain(|b| b.tool != "Notification");
                session.block(tool, event.tool_input);
            }
            // A sandboxed command's network request fires no `PermissionRequest`; any other
            // prompt already has its block.
            "Notification"
                if event.notification_type == "permission_prompt" && !session.needs_you() =>
            {
                session.block("Notification".into(), Value::Null);
            }
            "Elicitation" => session.block("Elicitation".into(), Value::Null),
            "PostToolUse" | "PostToolUseFailure" => {
                session.running.retain(|c| c.id != id);
                session.blocks.retain(|c| c.id != id);
                let input = &event.tool_input;
                let failed = event.hook_event_name == "PostToolUseFailure";
                // A denied or interrupted call, or a failed Edit or Write, changed nothing: only a
                // command that ran and failed has its exit code in its error.
                let ran = !failed || event.error.starts_with("Exit code");
                let guessed = session
                    .guess
                    .take_if(|p| p.tool_use_id == event.tool_use_id);
                let card = if !ran {
                    // A check that did not run leaves the wait's card step to a later step.
                    if guessed.is_some() {
                        self.awaiting_card.insert(event.session_id.clone());
                    }
                    None
                // A check gets its card whether it passed or failed. The hooks are async, so a fast
                // check's result can land before its start: it takes the card step with no guess.
                } else if guessed.is_some()
                    || (runs_check(&tool, input) && self.awaiting_card.remove(&event.session_id))
                {
                    let command = input["command"].as_str().unwrap_or_default();
                    let check = Check {
                        guess: guessed.and_then(|prompt| prompt.guess),
                        exit_passed: exit_code_counts(command).then_some(!failed),
                    };
                    Some((output(&event.tool_response, &event.error), Some(check)))
                } else if let Some(change) =
                    card_change(&tool, input, &event.tool_response, worktree_change)
                        .or_else(|| self.carded.remove(&event.tool_use_id).then(String::new))
                    && self.awaiting_card.remove(&event.session_id)
                {
                    Some((change, None))
                } else {
                    None
                };
                if let Some((change, check)) = card {
                    let detail = step_detail(input, &event.cwd);
                    outputs.push(Output::Card(CardRequest {
                        session_id: event.session_id.clone(),
                        tool_use_id: event.tool_use_id,
                        step: Step { tool, detail },
                        change,
                        check,
                    }));
                }
            }
            _ => {}
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
        if !blocked_before && session.state == State::NeedsYou {
            outputs.push(Output::NeedsYou {
                session_id: event.session_id,
            });
        }
        outputs
    }

    /// Takes one event log line replayed at app start. It updates state exactly like `apply` and
    /// requests nothing, so steps that ran while the app was down get no cards and no notices.
    pub fn replay(&mut self, line: &str) {
        self.apply(line);
    }

    /// The steps that already have a card in the card store, by `tool_use_id`, a fact from the shell
    /// at start. A replayed Bash step gets no fingerprint, so this is how one that had its card still
    /// takes its wait's card step, and a restart mid-wait gives no second card.
    pub fn set_carded_steps(&mut self, tool_use_ids: impl IntoIterator<Item = String>) {
        self.carded.extend(tool_use_ids);
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

    /// The session Daniel selected, which the session view shows; `None` until his first click.
    pub fn selected(&self) -> Option<&Session> {
        self.session(self.selected.as_deref()?)
    }
}
