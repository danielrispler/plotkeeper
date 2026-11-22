//! The one card writer contract (#327, spec #319, "The writer"). A call takes a system prompt, a user prompt, an
//! optional JSON schema, a model, an effort and a timeout, and gives text, parsed JSON and usage, or a typed failure.
//! Per writer only four things differ, all here: a command template (`build_command`), an output extractor and a
//! failure classifier (`read_reply`) and a model list (`WriterKind::models`). The process handling is shared, in
//! `ClaudeWriter`.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fmt,
    path::Path,
    process::{Command, ExitStatus},
    time::Duration,
};

/// Which program writes: `claude` by default, or `agy`, chosen by `summaries.writer` in `settings.json`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriterKind {
    #[default]
    Claude,
    Agy,
}

/// How long a logged-out `agy` waits for a pasted OAuth code before it gives up.
pub const AGY_LOGIN_WAIT: Duration = Duration::from_secs(60);

impl WriterKind {
    /// The program's name, as the login shell finds it.
    pub fn program(self) -> &'static str {
        match self {
            WriterKind::Claude => "claude",
            WriterKind::Agy => "agy",
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
        }
    }

    /// How long a call may run before it fails and is stopped. `agy`'s covers its login wait and then some.
    pub fn timeout(self) -> Duration {
        match self {
            WriterKind::Claude => Duration::from_secs(5 * 60),
            WriterKind::Agy => Duration::from_secs(3 * 60),
        }
    }

    /// The least a call that is not to be waited for (the risk judge) may be given.
    pub fn floor(self) -> Duration {
        match self {
            WriterKind::Claude => Duration::ZERO,
            WriterKind::Agy => AGY_LOGIN_WAIT + Duration::from_secs(15),
        }
    }

    /// The name of the neutral folder, beside the event log, the writer runs in.
    pub fn folder(self) -> &'static str {
        match self {
            WriterKind::Claude => "card-writer",
            WriterKind::Agy => "card-writer-agy",
        }
    }
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
    /// The line the page shows: why no card is written, and what to do about it.
    pub fn notice(&self) -> String {
        let program = self.program;
        let todo = match (self.kind, program) {
            (FailureKind::NotLoggedIn, "agy") => {
                "Log in to `agy` in a terminal, then it writes again."
            }
            (FailureKind::NotLoggedIn, _) => "Run `claude` in a terminal and log in with /login.",
            (FailureKind::Quota, _) => {
                "Wait for the limit to reset, or choose another model or writer in settings.json."
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
        format!("Cards and briefs are off: {head}. {todo}")
    }
}

/// One line for the log. What the program printed comes after the first quote mark, which the log keeps out.
impl fmt::Display for WriterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let program = self.program;
        let what = match self.kind {
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
                command.args(["--model", model]);
            }
            if let Some(effort) = effort {
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
    };
    if let Some(envelope) = &envelope {
        let (error, reply) = match kind {
            WriterKind::Agy => (
                envelope["status"].as_str() == Some("ERROR"),
                envelope["response"].as_str(),
            ),
            WriterKind::Claude => (
                envelope["is_error"].as_bool() == Some(true),
                envelope["result"].as_str(),
            ),
        };
        if error {
            let said = match kind {
                WriterKind::Agy => envelope["error"].as_str(),
                WriterKind::Claude => reply,
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
        if text.trim().is_empty() {
            return fail(FailureKind::Empty, "");
        }
        let usage = &envelope["usage"];
        return Ok(WriterReply {
            json: json
                .cloned()
                .or_else(|| schema.then(|| crate::parse_object(&text).ok()).flatten()),
            usage: usage["input_tokens"].as_u64().map(|input_tokens| Usage {
                input_tokens,
                output_tokens: usage["output_tokens"].as_u64().unwrap_or_default(),
            }),
            text,
        });
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
