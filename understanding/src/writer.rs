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

/// Which program writes: `claude` by default, or `agy`, chosen by `summaries.writer` in `settings.json`; or `anthropic`,
/// the Anthropic Messages API called with the key pasted on the page, which runs no program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriterKind {
    #[default]
    Claude,
    Agy,
    Anthropic,
}

/// How long a logged-out `agy` waits for a pasted OAuth code before it gives up.
pub const AGY_LOGIN_WAIT: Duration = Duration::from_secs(60);

impl WriterKind {
    /// Every writer, in the order the page presets the first found (spec #320, "Writer settings on the page").
    /// An API key comes first: it is found only when one is set.
    pub const ALL: [WriterKind; 3] = [WriterKind::Anthropic, WriterKind::Claude, WriterKind::Agy];

    /// Whether the writer takes the contract's HTTP path with an API key, instead of running a program.
    pub fn http(self) -> bool {
        matches!(self, WriterKind::Anthropic)
    }

    /// The writer's name on the page.
    pub fn name(self) -> &'static str {
        match self {
            WriterKind::Claude => "Claude",
            WriterKind::Agy => "agy",
            WriterKind::Anthropic => "Anthropic API key",
        }
    }

    /// The efforts the writer takes, the default ("") first; none for `agy`, whose effort is part of each model id.
    pub fn efforts(self) -> &'static [&'static str] {
        match self {
            WriterKind::Claude | WriterKind::Anthropic => &["", "low", "medium", "high"],
            WriterKind::Agy => &[],
        }
    }

    /// The program's name, as the login shell finds it, and the writer's name in `summaries.writer`. An HTTP writer's
    /// is no program.
    pub fn program(self) -> &'static str {
        match self {
            WriterKind::Claude => "claude",
            WriterKind::Agy => "agy",
            WriterKind::Anthropic => "anthropic",
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
            WriterKind::Anthropic => &["claude-haiku-5-5", "claude-sonnet-5-5", "claude-opus-5-5"],
        }
    }

    /// The model a call asked for `asked`, as this writer takes it. `agy` takes only its own list, so a model it does
    /// not know (the default `haiku` after `summaries.writer` moved to `agy`) is its first one. `claude` takes any
    /// id, a full one included. The API takes any full model id, so an alias such as `haiku` is its first one.
    pub fn model(self, asked: &str) -> &str {
        match self {
            WriterKind::Agy if !self.models().contains(&asked) => self.models()[0],
            WriterKind::Anthropic if !asked.starts_with("claude-") => self.models()[0],
            _ => asked,
        }
    }

    /// How long a call may run before it fails and is stopped. `agy`'s covers its login wait and then some.
    pub fn timeout(self) -> Duration {
        match self {
            WriterKind::Claude => Duration::from_secs(5 * 60),
            WriterKind::Agy => Duration::from_secs(3 * 60),
            WriterKind::Anthropic => Duration::from_secs(2 * 60),
        }
    }

    /// The least a call that is not to be waited for (the risk judge) may be given.
    pub fn floor(self) -> Duration {
        match self {
            WriterKind::Claude | WriterKind::Anthropic => Duration::ZERO,
            WriterKind::Agy => AGY_LOGIN_WAIT + Duration::from_secs(15),
        }
    }

    /// The name of the neutral folder, beside the event log, the writer runs in. An HTTP writer runs in none, and no
    /// folder of its name is made.
    pub fn folder(self) -> &'static str {
        match self {
            WriterKind::Claude => "card-writer",
            WriterKind::Agy => "card-writer-agy",
            WriterKind::Anthropic => "card-writer-anthropic",
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

/// Today's writers in the order the page presets the first found (#320): an API key, Claude, then agy. An HTTP writer is
/// listed as not found: no program is looked for, and the server says whether its key is set.
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
            (FailureKind::NotLoggedIn, _) => "Run `claude` in a terminal and log in with /login.",
            (FailureKind::Quota, _) => {
                "Wait for the limit to reset, or pick another model or writer under Writer, at the top of the page."
            }
            (FailureKind::Timeout, _) => "Check the connection; it tries again at the next turn.",
            (FailureKind::NotInstalled, "agy") => "Install the Antigravity CLI, or fix its path.",
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
            FailureKind::NotLoggedIn if program == "anthropic" => "has no working API key",
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
                .args(["--print-timeout", &format!("{minutes}m"), "-p"])
                .arg(prompt.replace('\0', ""));
            String::new()
        }
        WriterKind::Anthropic => unreachable!("an HTTP writer runs no program"),
    };
    command.env("PLOTKEEPER_INTERNAL", "1").current_dir(folder);
    (command, stdin.replace('\0', ""))
}

/// What a run printed, read into the contract: `kind`'s extractor and failure classifier. `schema` says the call
/// had one, so the reply is the program's JSON envelope; `status` is how the program exited.
pub fn read_reply(
    kind: WriterKind,
    schema: bool,
    status: &ExitStatus,
    stdout: &str,
) -> Result<WriterReply, WriterError> {
    let head: String = stdout.trim().chars().take(300).collect();
    let fail = |kind_of, detail: &str| Err(WriterError::new(kind_of, kind, detail));
    let envelope = match kind {
        WriterKind::Agy => Some(crate::parse_object(stdout).unwrap_or(Value::Null)),
        WriterKind::Claude if schema => Some(crate::parse_object(stdout).unwrap_or(Value::Null)),
        WriterKind::Claude => None,
        WriterKind::Anthropic => unreachable!("an HTTP writer's reply is read by `http_writer`"),
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
