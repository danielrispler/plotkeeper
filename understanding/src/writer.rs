//! The one card writer contract (#327, spec #319, "The writer"). A call takes a system prompt, a user prompt, an
//! optional JSON schema, a model, an effort and a timeout, and gives text, parsed JSON and usage, or a typed failure.
//! Per writer only four things differ, all here: a command template (`build_command`), an output extractor and a
//! failure classifier (`read_reply`) and a model list (`WriterKind::models`). The process handling is shared, in
//! `ClaudeWriter`. An API-key writer takes the contract's HTTP path instead (`http_writer`, spec #320, "The contract gains
//! an HTTP path"); both paths end in `finish`, so a caller cannot tell which one wrote.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt,
    path::Path,
    process::{Command, ExitStatus},
    time::Duration,
};

/// Which program writes: `claude` by default, `agy`, `codex`, `copilot` or Cursor's `agent`, chosen by `summaries.writer` in
/// `settings.json`; or an API-key writer, which runs no program: `anthropic`, the Anthropic Messages API called with the key
/// pasted on the page, or `openai`, any OpenAI-compatible chat API (OpenAI, OpenRouter, Groq, a local Ollama) at the base URL
/// set on the page, with its key when it needs one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriterKind {
    #[default]
    Claude,
    Agy,
    Codex,
    Copilot,
    Cursor,
    Anthropic,
    OpenAI,
}

/// How long a logged-out `agy` waits for a pasted OAuth code before it gives up.
pub const AGY_LOGIN_WAIT: Duration = Duration::from_secs(60);

impl WriterKind {
    /// Every writer, in the order the page presets the first found (spec #320, "Writer settings on the page").
    /// An API key comes first: it is found only when one is set.
    pub const ALL: [WriterKind; 7] = [
        WriterKind::Anthropic,
        WriterKind::OpenAI,
        WriterKind::Claude,
        WriterKind::Codex,
        WriterKind::Agy,
        WriterKind::Copilot,
        WriterKind::Cursor,
    ];

    /// Whether the writer takes the contract's HTTP path with an API key, instead of running a program.
    pub fn http(self) -> bool {
        matches!(self, WriterKind::Anthropic | WriterKind::OpenAI)
    }

    /// The writer's name on the page.
    pub fn name(self) -> &'static str {
        match self {
            WriterKind::Claude => "Claude",
            WriterKind::Agy => "agy",
            WriterKind::Codex => "Codex",
            WriterKind::Copilot => "Copilot",
            WriterKind::Cursor => "Cursor",
            WriterKind::Anthropic => "Anthropic API key",
            WriterKind::OpenAI => "OpenAI-compatible API",
        }
    }

    /// The efforts the writer takes, the default ("") first; none for `agy`, whose effort is part of each model id.
    pub fn efforts(self) -> &'static [&'static str] {
        match self {
            WriterKind::Claude => &["", "low", "medium", "high"],
            WriterKind::Agy => &[],
            WriterKind::Codex | WriterKind::Anthropic | WriterKind::OpenAI => {
                &["", "low", "medium", "high"]
            }
            WriterKind::Copilot | WriterKind::Cursor => &[],
        }
    }

    /// The program's name, as the login shell finds it.
    pub fn program(self) -> &'static str {
        match self {
            WriterKind::Claude => "claude",
            WriterKind::Agy => "agy",
            WriterKind::Codex => "codex",
            WriterKind::Copilot => "copilot",
            WriterKind::Cursor => "agent",
            WriterKind::Anthropic => "anthropic",
            WriterKind::OpenAI => "openai",
        }
    }

    /// The models the writer takes, the ones its settings offer (`agy models`; its effort is part of each id).
    pub fn models(self) -> &'static [&'static str] {
        match self {
            WriterKind::Claude => &["haiku", "sonnet", "opus", "fable"],
            WriterKind::Agy => &[
                "gemini-4-neon-low",
                "gemini-4-neon-medium",
                "gemini-4-neon-high",
                "gemini-4-argon-low",
                "gemini-4-hydrogen-low",
            ],
            // `codex debug models`, the listed ones, the cheapest first.
            WriterKind::Codex => &["gpt-6-luna", "gpt-5.6-luna", "gpt-5.6-terra"],
            // Copilot's and Cursor's from their docs, not run (`delivery-writer-facts.md`): the cheapest first, and
            // Cursor's own `auto`.
            WriterKind::Copilot => &["claude-haiku-4.5", "gpt-6-astra", "gpt-5.3-codex"],
            WriterKind::Cursor => &["auto", "claude-haiku-5-5", "gpt-5.6-luna"],
            WriterKind::Anthropic => &["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"],
            // Suggestions only: the endpoint takes any id it serves, such as a local Ollama's `llama3.2`.
            WriterKind::OpenAI => &["gpt-6-luna", "gpt-5.6-luna", "gpt-5.6-terra"],
        }
    }

    /// The model a call asked for `asked`, as this writer takes it. Every program writer but `claude` takes only its own
    /// list, so a model one does not know (the default `haiku` after `summaries.writer` moved to it) is its first one.
    /// `claude` takes any id, a full one included. The API takes any full model id, so an alias such as `haiku` is its
    /// first one.
    pub fn model(self, asked: &str) -> &str {
        match self {
            WriterKind::Claude => asked,
            WriterKind::Anthropic if !asked.starts_with("claude-") => self.models()[0],
            WriterKind::Anthropic => asked,
            // Any id the endpoint serves, but none of Claude's aliases, which no OpenAI-compatible endpoint does.
            WriterKind::OpenAI
                if asked.is_empty() || WriterKind::Claude.models().contains(&asked) =>
            {
                self.models()[0]
            }
            WriterKind::OpenAI => asked,
            _ if !self.models().contains(&asked) => self.models()[0],
            _ => asked,
        }
    }

    /// Whether the program says why it failed on standard error only, which is then read apart from its reply, to tell
    /// a failure apart: Cursor's `agent` (its output-format docs).
    pub(crate) fn fails_on_stderr(self) -> bool {
        self == WriterKind::Cursor
    }

    /// How long a call may run before it fails and is stopped. `agy`'s covers its login wait and then some.
    pub fn timeout(self) -> Duration {
        match self {
            WriterKind::Claude => Duration::from_secs(5 * 60),
            WriterKind::Agy => Duration::from_secs(3 * 60),
            WriterKind::Codex | WriterKind::Copilot | WriterKind::Cursor => {
                Duration::from_secs(3 * 60)
            }
            WriterKind::Anthropic | WriterKind::OpenAI => Duration::from_secs(2 * 60),
        }
    }

    /// The least a call that is not to be waited for (the risk judge) may be given.
    pub fn floor(self) -> Duration {
        match self {
            WriterKind::Claude
            | WriterKind::Codex
            | WriterKind::Copilot
            | WriterKind::Cursor
            | WriterKind::Anthropic
            | WriterKind::OpenAI => Duration::ZERO,
            WriterKind::Agy => AGY_LOGIN_WAIT + Duration::from_secs(15),
        }
    }

    /// The name of the neutral folder, beside the event log, the writer runs in. An HTTP writer runs in none, and no
    /// folder of its name is made.
    pub fn folder(self) -> &'static str {
        match self {
            WriterKind::Claude => "card-writer",
            WriterKind::Agy => "card-writer-agy",
            WriterKind::Codex => "card-writer-codex",
            WriterKind::Copilot => "card-writer-copilot",
            WriterKind::Cursor => "card-writer-cursor",
            WriterKind::Anthropic => "card-writer-anthropic",
            WriterKind::OpenAI => "card-writer-openai",
        }
    }
}

/// A writer as the page's writer section lists it (spec #320, "Writer settings on the page").
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FoundWriter {
    pub kind: WriterKind,
    pub name: &'static str,
    /// Whether the login shell's `command -v` finds its program. Nothing is run to see whether it is logged in. For an
    /// HTTP writer: whether its API key is set, which the server, which keeps the key, fills in.
    pub found: bool,
    pub models: &'static [&'static str],
    /// The efforts it takes; none where the effort is part of the model id.
    pub efforts: &'static [&'static str],
}

/// Today's writers in the order the page presets the first found (#320): an API key, Claude, Codex, agy, Copilot, then
/// Cursor. An HTTP writer is listed as not found: no program is looked for, and the server says whether its key is set.
pub fn found_writers(shell: &Path) -> Vec<FoundWriter> {
    WriterKind::ALL
        .into_iter()
        .map(|kind| FoundWriter {
            kind,
            name: kind.name(),
            found: !kind.http() && crate::find(shell, kind.program()).is_some(),
            models: kind.models(),
            efforts: kind.efforts(),
        })
        .collect()
}

/// One call of the contract.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WriterRequest {
    pub system: Option<String>,
    pub user: String,
    /// A JSON schema the reply's object must fit; a writer that cannot take one is read with `parse_object` instead.
    pub schema: Option<Value>,
    pub model: Option<String>,
    /// Empty or `None` leaves the model's default.
    pub effort: Option<String>,
    pub timeout: Duration,
}

/// Tokens a call used, when the writer says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// What a call gave.
#[derive(Clone, Debug, PartialEq)]
pub struct WriterReply {
    pub text: String,
    /// The reply's JSON object, parsed, when the call had a schema.
    pub json: Option<Value>,
    pub usage: Option<Usage>,
}

/// Why a call gave nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    NotLoggedIn,
    /// A quota or a rate limit.
    Quota,
    Timeout,
    NotInstalled,
    Empty,
    Other,
}

/// A failed call: its kind, and the head of what the program printed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriterError {
    pub kind: FailureKind,
    pub program: &'static str,
    pub detail: String,
}

impl WriterError {
    pub(crate) fn new(kind: FailureKind, program: WriterKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            program: program.program(),
            detail: detail.into(),
        }
    }
}

impl WriterError {
    /// The line the page shows: why no summary is written, and what to do about it. PR briefs are written by `claude`
    /// whatever writer is chosen, so the line speaks of summaries only (spec #320, "The writer's failure notice").
    pub fn notice(&self) -> String {
        format!("Summaries are off: {}", self.reason())
    }

    /// Why the call failed and what to do about it, each writer with its own fix: what a Test shows, and the notice's
    /// words after its opening.
    pub fn reason(&self) -> String {
        let program = self.program;
        let todo = match (self.kind, program) {
            (FailureKind::NotLoggedIn, "agy") => {
                "Log in to `agy` in a terminal, then it writes again."
            }
            (FailureKind::NotLoggedIn, "anthropic") => {
                "Add or replace the API key under Writer, at the top of the page."
            }
            (FailureKind::Other, "anthropic") => {
                "The API answered with an error; it tries again at the next turn."
            }
            (FailureKind::NotLoggedIn, "openai") => {
                "Add or replace the API key of the OpenAI-compatible API under Writer, at the top of the page."
            }
            (FailureKind::Other, "openai") => {
                "Check the base URL and the model under Writer, at the top of the page, and that the API is running; it tries again at the next turn."
            }
            (FailureKind::NotLoggedIn, "codex") => {
                "Run `codex login` in a terminal, then it writes again."
            }
            (FailureKind::NotLoggedIn, "copilot") => {
                "Run `copilot login` in a terminal, then it writes again."
            }
            (FailureKind::NotLoggedIn, "agent") => {
                "Run `agent login` in a terminal, then it writes again."
            }
            (FailureKind::NotLoggedIn, _) => "Run `claude` in a terminal and log in with /login.",
            (FailureKind::Quota, _) => {
                "Wait for the limit to reset, or pick another model or writer under Writer, at the top of the page."
            }
            (FailureKind::Timeout, _) => "Check the connection; it tries again at the next turn.",
            (FailureKind::NotInstalled, "agy") => "Install the Antigravity CLI, or fix its path.",
            (FailureKind::NotInstalled, "codex") => "Install the Codex CLI, or fix its path.",
            (FailureKind::NotInstalled, "copilot") => {
                "Install the GitHub Copilot CLI, or fix its path."
            }
            (FailureKind::NotInstalled, "agent") => "Install the Cursor CLI, or fix its path.",
            (FailureKind::NotInstalled, _) => "Install Claude Code, or fix its path.",
            (FailureKind::Empty, _) => {
                "Run it by hand to see why; it tries again at the next turn."
            }
            (FailureKind::Other, _) => {
                "Run it by hand to see why; it tries again at the next turn."
            }
        };
        // What the program printed stays out of the page: it may quote the work.
        let mut head = self.clone();
        head.detail = if head.detail.starts_with('(') && self.kind == FailureKind::NotInstalled {
            head.detail
        } else {
            String::new()
        };
        format!("{head}. {todo}")
    }
}

/// One line for the log. What the program printed comes after the first quote mark, which the log keeps out.
impl fmt::Display for WriterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let program = self.program;
        let what = match self.kind {
            FailureKind::NotLoggedIn if program == "anthropic" || program == "openai" => {
                "has no working API key"
            }
            FailureKind::NotLoggedIn => "is not logged in",
            FailureKind::Quota => "hit its quota or rate limit",
            FailureKind::Timeout => "ran past its timeout and was stopped",
            FailureKind::NotInstalled => "did not start",
            FailureKind::Empty => "printed nothing",
            FailureKind::Other => "failed",
        };
        match self.detail.as_str() {
            "" => write!(f, "`{program}` {what}"),
            detail if detail.starts_with('(') => write!(f, "`{program}` {what} {detail}"),
            detail => write!(f, "`{program}` {what}: {detail}"),
        }
    }
}

/// The command that makes `request`, and what goes on its standard input: the program's own template. Both run in
/// `folder`, a neutral empty one, and with `PLOTKEEPER_INTERNAL=1`, so the hook never records the run (#323).
pub fn build_command(
    kind: WriterKind,
    program: &Path,
    folder: &Path,
    request: &WriterRequest,
) -> (Command, String) {
    let mut command = Command::new(program);
    let schema = request.schema.as_ref().map(Value::to_string);
    let effort = request.effort.as_deref().filter(|e| !e.is_empty());
    let stdin = match kind {
        WriterKind::Claude => {
            command
                // `--tools` takes several values, so an option must follow it.
                .args(["--tools", "", "--strict-mcp-config", "--safe-mode"]);
            if let Some(model) = &request.model {
                command.args(["--model", model]);
            }
            if let Some(effort) = effort {
                command.args(["--effort", effort]);
            }
            if let Some(system) = &request.system {
                command.args(["--system-prompt", system]);
            }
            if let Some(schema) = &schema {
                command.args(["--output-format", "json", "--json-schema", schema]);
            }
            command
                .arg("-p")
                .env_remove("ANTHROPIC_API_KEY")
                .env_remove("ANTHROPIC_AUTH_TOKEN")
                .env_remove("CLAUDE_CODE_PLUGIN_DIRS");
            request.user.clone()
        }
        WriterKind::Agy => {
            command.args(["--output-format", "json"]);
            if let Some(schema) = &schema {
                command.args(["--json-schema", schema]);
            }
            if let Some(model) = &request.model {
                command.args(["--model", kind.model(model)]);
            }
            // The effort is part of an `agy` model id, so one that came with another writer's model is dropped.
            let known = request
                .model
                .as_deref()
                .is_none_or(|m| kind.models().contains(&m));
            if let Some(effort) = effort.filter(|_| known) {
                command.args(["--effort", effort]);
            }
            // `agy` has no system prompt flag and takes the prompt as an argument, not on standard input.
            let prompt = match &request.system {
                Some(system) => format!("{system}\n\n{}", request.user),
                None => request.user.clone(),
            };
            let minutes = request.timeout.as_secs().div_ceil(60).max(1);
            command
                // No flag turns `agy`'s tools off, and `--mode plan` writes a plan instead of the reply: its lockout is
                // the sandboxed terminal, with no slash commands or skills, in the empty folder.
                .args(["--sandbox", "--disable-slash-commands"])
                .args(["--print-timeout", &format!("{minutes}m"), "-p"])
                .arg(prompt.replace('\0', ""));
            String::new()
        }
        WriterKind::Codex => {
            // No flag turns Codex's tools off: its lockout is the read-only sandbox, without the user's config (and so
            // without their MCP servers), in the empty folder. `--json` prints events, which hold the reply and usage.
            command.args([
                "exec",
                "--skip-git-repo-check",
                "-s",
                "read-only",
                "--ephemeral",
                "--ignore-user-config",
                "--json",
            ]);
            if let Some(model) = &request.model {
                command.args(["--model", kind.model(model)]);
            }
            if let Some(effort) = effort {
                command.args(["-c", &format!("model_reasoning_effort=\"{effort}\"")]);
            }
            if let Some(schema) = &schema {
                command
                    .arg("--output-schema")
                    .arg(schema_file(folder, schema));
            }
            // `-`: the prompt on standard input. Codex has no system prompt flag, so the system prompt leads it.
            command.arg("-");
            match &request.system {
                Some(system) => format!("{system}\n\n{}", request.user),
                None => request.user.clone(),
            }
        }
        WriterKind::Copilot => {
            // An empty allow-list turns Copilot's tools off (its docs; that an empty one leaves none is not verified),
            // and it never stops to ask. `-s` keeps only the reply; `json` prints it as JSON lines. No schema flag: the
            // object is read from the text.
            command.args([
                "-s",
                "--output-format=json",
                "--available-tools=",
                "--no-ask-user",
            ]);
            if let Some(model) = &request.model {
                command.arg(format!("--model={}", kind.model(model)));
            }
            command.arg("-p").arg(lead(request));
            String::new()
        }
        WriterKind::Cursor => {
            // No flag turns Cursor's tools off: its lockout is ask mode, read-only, in the empty folder, trusted so that
            // no prompt waits. No schema flag: the object is read from its `result`.
            command.args(["-p", "--output-format", "json", "--mode", "ask", "--trust"]);
            if let Some(model) = &request.model {
                command.args(["--model", kind.model(model)]);
            }
            command.arg(lead(request));
            String::new()
        }
        WriterKind::Anthropic | WriterKind::OpenAI => {
            unreachable!("an HTTP writer runs no program")
        }
    };
    command.env("PLOTKEEPER_INTERNAL", "1").current_dir(folder);
    (command, stdin.replace('\0', ""))
}

/// The prompt as one argument, for a writer with no system prompt flag that takes no standard input: the system prompt
/// leads it, and NUL bytes, which no argument can hold, are dropped.
fn lead(request: &WriterRequest) -> String {
    match &request.system {
        Some(system) => format!("{system}\n\n{}", request.user),
        None => request.user.clone(),
    }
    .replace('\0', "")
}

/// The file in `folder` that holds `schema`, which `codex` takes only as a file: one per schema, named by its text, so
/// calls at once never write over each other's. Not written (the folder is missing), the run fails and says why.
fn schema_file(folder: &Path, schema: &str) -> std::path::PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    schema.hash(&mut hasher);
    let file = folder.join(format!("schema-{:016x}.json", hasher.finish()));
    if std::fs::read_to_string(&file).ok().as_deref() != Some(schema) {
        // Written beside and moved in, so a call reading it never sees half of it.
        let part = folder.join(format!(
            ".schema-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::write(&part, schema).and_then(|()| std::fs::rename(&part, &file));
    }
    file
}

/// What a run printed, read into the contract: `kind`'s extractor and failure classifier. `schema` says the call
/// had one, so the reply is the program's JSON envelope; `status` is how the program exited.
pub fn read_reply(
    kind: WriterKind,
    schema: bool,
    status: &ExitStatus,
    stdout: &str,
) -> Result<WriterReply, WriterError> {
    match kind {
        WriterKind::Codex => return read_codex(schema, status, stdout),
        WriterKind::Copilot => return read_copilot(schema, status, stdout),
        _ => {}
    }
    let head: String = stdout.trim().chars().take(300).collect();
    let fail = |kind_of, detail: &str| Err(WriterError::new(kind_of, kind, detail));
    let envelope = match kind {
        WriterKind::Agy => Some(crate::parse_object(stdout).unwrap_or(Value::Null)),
        WriterKind::Claude if schema => Some(crate::parse_object(stdout).unwrap_or(Value::Null)),
        // Cursor's one object, as Claude's; none when it failed, which it says on standard error.
        WriterKind::Cursor => Some(crate::parse_object(stdout).unwrap_or(Value::Null)),
        WriterKind::Claude | WriterKind::Codex | WriterKind::Copilot => None,
        WriterKind::Anthropic | WriterKind::OpenAI => {
            unreachable!("an HTTP writer's reply is read by `http_writer`")
        }
    };
    if let Some(envelope) = &envelope {
        let (error, reply) = match kind {
            WriterKind::Agy => (
                envelope["status"].as_str() == Some("ERROR"),
                envelope["response"].as_str(),
            ),
            _ => (
                envelope["is_error"].as_bool() == Some(true),
                envelope["result"].as_str(),
            ),
        };
        if error {
            let said = match kind {
                WriterKind::Agy => envelope["error"].as_str(),
                _ => reply,
            };
            let said = said.unwrap_or(&head);
            return fail(classify(kind, status, said), &format!("{said:?}"));
        }
        if !status.success() {
            return fail(
                classify(kind, status, &head),
                &format!("({status}): {head:?}"),
            );
        }
        let json = envelope.get("structured_output").filter(|v| !v.is_null());
        let text = match (json, reply) {
            (Some(json), _) => json.to_string(),
            (None, Some(reply)) => reply.to_string(),
            // A schema call with no object in its envelope: the whole output, read as today's replies are.
            (None, None) => stdout.to_string(),
        };
        return finish(
            kind,
            schema,
            text,
            json.cloned(),
            usage_of(&envelope["usage"]),
        );
    }
    // Claude's plain text: today's reading.
    match (status.success(), head.is_empty()) {
        (false, _) => fail(
            classify(kind, status, &head),
            &format!("({status}): {head:?}"),
        ),
        (true, true) => fail(FailureKind::Empty, ""),
        (true, false) => Ok(WriterReply {
            text: stdout.into(),
            json: None,
            usage: None,
        }),
    }
}

/// `read_reply` of `stdout`, the reply, for a writer that may say why it failed on standard error only
/// (`fails_on_stderr`): a failure its output does not tell apart is classified by what `stderr` said.
pub(crate) fn read_reply_and_stderr(
    kind: WriterKind,
    schema: bool,
    status: &ExitStatus,
    stdout: &str,
    stderr: &str,
) -> Result<WriterReply, WriterError> {
    let said: String = stderr.trim().chars().take(300).collect();
    match read_reply(kind, schema, status, stdout) {
        Err(error) if error.kind == FailureKind::Other && !said.is_empty() => {
            Err(WriterError::new(
                classify(kind, status, &said),
                kind,
                format!("({status}): {said:?}"),
            ))
        }
        reply => reply,
    }
}

/// `read_reply` for `codex exec --json`: one JSON event a line. The reply is the last `agent_message` item, the usage the
/// `turn.completed`'s; a `turn.failed`, or the last `error` event of a run that exited with an error, says why it failed.
fn read_codex(schema: bool, status: &ExitStatus, stdout: &str) -> Result<WriterReply, WriterError> {
    let kind = WriterKind::Codex;
    let (mut text, mut usage, mut failed, mut error) = (None, None, None, None);
    for event in stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    {
        match event["type"].as_str() {
            Some("item.completed") if event["item"]["type"] == "agent_message" => {
                text = event["item"]["text"].as_str().map(String::from)
            }
            Some("turn.completed") => usage = Some(event["usage"].clone()),
            Some("turn.failed") => failed = event["error"]["message"].as_str().map(String::from),
            Some("error") => error = event["message"].as_str().map(String::from),
            _ => {}
        }
    }
    let head: String = stdout.trim().chars().take(300).collect();
    let fail = |kind_of, detail: String| Err(WriterError::new(kind_of, kind, detail));
    if let Some(said) = failed.or(error.filter(|_| !status.success())) {
        let said: String = said.chars().take(300).collect();
        return fail(classify(kind, status, &said), format!("{said:?}"));
    }
    if !status.success() {
        return fail(
            classify(kind, status, &head),
            format!("({status}): {head:?}"),
        );
    }
    let Some(text) = text.filter(|t| !t.trim().is_empty()) else {
        return fail(FailureKind::Empty, String::new());
    };
    Ok(WriterReply {
        json: schema.then(|| crate::parse_object(&text).ok()).flatten(),
        usage: usage.and_then(|usage| {
            usage["input_tokens"].as_u64().map(|input_tokens| Usage {
                input_tokens,
                output_tokens: usage["output_tokens"].as_u64().unwrap_or_default(),
            })
        }),
        text,
    })
}

/// `read_reply` for `copilot -p --output-format=json`: JSON lines whose records its docs do not describe, so this reading
/// waits on wave 1's trial (spec #320). The reply is the last `assistant.message`'s `data.content`, and a `session.error`'s
/// `data.message` says why it failed; output with no JSON line (`-s` printing the reply alone) is the reply.
fn read_copilot(
    schema: bool,
    status: &ExitStatus,
    stdout: &str,
) -> Result<WriterReply, WriterError> {
    let kind = WriterKind::Copilot;
    let records: Vec<Value> = stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|r| r["type"].is_string())
        .collect();
    let last = |of: &str, field: &str| {
        records
            .iter()
            .rev()
            .find(|r| r["type"] == of)
            .and_then(|r| r["data"][field].as_str())
    };
    let head: String = stdout.trim().chars().take(300).collect();
    let fail = |kind_of, detail: String| Err(WriterError::new(kind_of, kind, detail));
    if let Some(said) = last("session.error", "message") {
        let said: String = said.chars().take(300).collect();
        return fail(classify(kind, status, &said), format!("{said:?}"));
    }
    if !status.success() {
        return fail(
            classify(kind, status, &head),
            format!("({status}): {head:?}"),
        );
    }
    let text = match records.is_empty() {
        true => stdout,
        false => last("assistant.message", "content").unwrap_or_default(),
    };
    if text.trim().is_empty() {
        return fail(FailureKind::Empty, String::new());
    }
    Ok(WriterReply {
        json: schema.then(|| crate::parse_object(text).ok()).flatten(),
        usage: None,
        text: text.into(),
    })
}

/// The usage a reply's `usage` object gives, when it does: the same field names on both paths.
pub(crate) fn usage_of(usage: &Value) -> Option<Usage> {
    usage["input_tokens"].as_u64().map(|input_tokens| Usage {
        input_tokens,
        output_tokens: usage["output_tokens"].as_u64().unwrap_or_default(),
    })
}

/// The reply both paths end in: its text, an empty one a failure; and, when the call had a schema, its object, the
/// writer's own (`json`) or else read from the text (the schema check).
pub(crate) fn finish(
    kind: WriterKind,
    schema: bool,
    text: String,
    json: Option<Value>,
    usage: Option<Usage>,
) -> Result<WriterReply, WriterError> {
    if text.trim().is_empty() {
        return Err(WriterError::new(FailureKind::Empty, kind, ""));
    }
    Ok(WriterReply {
        json: json.or_else(|| schema.then(|| crate::parse_object(&text).ok()).flatten()),
        usage,
        text,
    })
}

/// Why a failed call failed, from what it printed and how it exited.
fn classify(kind: WriterKind, status: &ExitStatus, said: &str) -> FailureKind {
    let said = said.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| said.contains(w));
    if has(&[
        "not logged in",
        "please run /login",
        "please log in",
        "authentication failed",
        "not authenticated",
        // Codex's, which retries before it fails.
        "401 unauthorized",
    ]) {
        FailureKind::NotLoggedIn
    } else if has(&[
        "quota",
        "resource_exhausted",
        "resource exhausted",
        "rate limit",
        "rate_limit",
        "usage limit",
        "too many requests",
    ]) {
        FailureKind::Quota
    } else if kind == WriterKind::Agy && status.code() == Some(3) {
        // `--print-timeout` was hit.
        FailureKind::Timeout
    } else {
        FailureKind::Other
    }
}
