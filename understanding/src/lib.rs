//! The understanding layer's I/O, which the core leaves to the shell: the card writer (#328,
//! module 4), the worktree fingerprint (module 5), and the card store and its weekly rates (module 6).

use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use studio_core::CardRequest;

mod fingerprint;
mod store;
mod week;
pub use fingerprint::Fingerprints;
pub use store::{CardStore, Outcome, SessionCards, StoredCard, millis};
pub use week::{Rate, Week, week};

/// A card on one step, in Hebrew; code stays in English, and code inside the Hebrew text is in
/// backticks. It opens with the question; the rest shows when Daniel taps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub question: String,
    pub answer: String,
    // The fields below default to empty, so cards stored before they existed still load.
    /// The files the step changed.
    #[serde(default)]
    pub paths: Vec<String>,
    /// The step's real code: the lines of its change that matter most.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub why_it_matters: String,
    #[serde(default)]
    pub not_checked: String,
    /// For a check: whether its output shows it passed. `None` when the writer answers "unknown",
    /// and for other steps.
    #[serde(default, deserialize_with = "verdict")]
    pub passed: Option<bool>,
}

/// `true` or `false`; anything else, such as `"unknown"`, is no verdict.
fn verdict<'de, D: serde::Deserializer<'de>>(value: D) -> Result<Option<bool>, D::Error> {
    Ok(serde_json::Value::deserialize(value)?.as_bool())
}

impl Card {
    /// The card in a writer's reply, which must hold one JSON object, perhaps in a code fence. When
    /// it does not parse, the error is one line for the log.
    pub fn from_reply(reply: &str) -> Result<Card, String> {
        let object = reply.find('{').zip(reply.rfind('}'));
        let object = object.filter(|(start, end)| start < end);
        let object = object.map_or("", |(start, end)| &reply[start..=end]);
        serde_json::from_str(object).map_err(|error| {
            let head: String = reply.chars().take(300).collect();
            format!("the card writer's reply does not parse ({error}): {head:?}")
        })
    }
}

/// Writes the card on a finished card step.
pub trait CardWriter {
    /// The card, or `None` when the writer's output does not parse.
    fn write(&self, request: &CardRequest) -> Option<Card>;
}

/// Writes a fixed Hebrew card that shows the step's change, so the app can run without `claude -p`.
pub struct FakeWriter;

impl CardWriter for FakeWriter {
    fn write(&self, request: &CardRequest) -> Option<Card> {
        let what = request.step.detail.as_deref().unwrap_or(&request.step.tool);
        Some(Card {
            question: format!("למה הסוכן שינה את `{what}`?"),
            answer: "כרטיס לדוגמה של הכותב המזויף, שלא קורא את הקוד. השינוי עצמו למטה.".into(),
            paths: request.step.detail.iter().cloned().collect(),
            code: request.change.clone(),
            why_it_matters: "כאן יופיע למה הצעד חשוב, עם קוד בתוך הטקסט כמו `if a <= b {`.".into(),
            not_checked: "לא נבדק: כלום, זה כרטיס לדוגמה.".into(),
            passed: None,
        })
    }
}

/// Writes the card with `claude -p` on Daniel's subscription, in a neutral folder that holds none of
/// his projects, so it loads no repository's instructions and has nothing to wander into.
pub struct ClaudeWriter {
    /// The `claude` program, from `find_claude`.
    program: PathBuf,
    folder: PathBuf,
}

/// The `claude` program, as login shell `shell` finds it, or `claude` when it answers no path. An
/// app started from Finder or `open` gets launchd's `PATH`, which lacks Homebrew's folder. Only the
/// path is kept: the shell's environment, which a profile may give the variables the writer removes,
/// never reaches the writer.
pub fn find_claude(shell: &Path) -> PathBuf {
    let output = Command::new(shell)
        .args(["-lc", "command -v claude"])
        .output();
    let stdout = output.ok().filter(|o| o.status.success()).map(|o| o.stdout);
    let stdout = stdout.as_deref().map(String::from_utf8_lossy);
    // A profile may print before the answer, which is the last line.
    match stdout.as_deref().and_then(|out| out.lines().last()) {
        Some(path) if path.starts_with('/') => path.into(),
        _ => "claude".into(),
    }
}

impl ClaudeWriter {
    pub fn new(program: PathBuf, folder: PathBuf) -> Self {
        Self { program, folder }
    }

    /// The `claude -p` run for one step. Without the two variables and without `--bare`, `claude`
    /// uses the subscription login (#320, "Fact"); the studio never reads that login itself. The
    /// change is untrusted text, so the run has no tools and no MCP servers, whatever Daniel's
    /// settings allow. It also runs without the studio plugin, so its session sends no hook events.
    pub fn command(&self, request: &CardRequest) -> Command {
        let step = &request.step;
        let detail = step.detail.as_deref().unwrap_or_default();
        // A check's input is its command and its result, not a change.
        let (check, input) = match request.check {
            Some(_) => (CHECK, "Result"),
            None => ("", "Change"),
        };
        let change = &request.change;
        let kept = &change[..change.floor_char_boundary(MAX_CHANGE)];
        let cut = match change.len() - kept.len() {
            0 => String::new(),
            bytes => format!("\n[{bytes} more bytes cut]"),
        };
        let prompt = format!(
            "{PROMPT}{check}\nStep: {} {detail}\n{input}:\n{kept}{cut}",
            step.tool
        );
        let mut command = Command::new(&self.program);
        command
            // `--tools` takes several values, so an option must follow it.
            .args(["--tools", "", "--strict-mcp-config", "-p", &prompt])
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("ANTHROPIC_AUTH_TOKEN")
            .env_remove("CLAUDE_CODE_PLUGIN_DIRS")
            .current_dir(&self.folder);
        command
    }
}

impl CardWriter for ClaudeWriter {
    /// Runs `claude -p`, which takes seconds, so the shell calls it off its log thread. A step with
    /// no card logs one line on stderr.
    // shortcut: no timeout, as in v0; add one if a stuck `claude -p` ever holds a card back.
    fn write(&self, request: &CardRequest) -> Option<Card> {
        let card = match self.command(request).output() {
            Ok(output) => Card::from_reply(&String::from_utf8_lossy(&output.stdout)),
            Err(error) => Err(format!("`claude -p` did not start: {error}")),
        };
        card.inspect_err(|line| eprintln!("no card on {}: {line}", request.tool_use_id))
            .ok()
    }
}

/// The most of a step's change the writer gets: 64 KiB. The lines that matter fit, a huge new file
/// or log costs no more, and the prompt stays far under macOS's 1 MiB limit on a command's arguments.
const MAX_CHANGE: usize = 64 << 10;

/// The card writer's instructions. Its input is only the step and its change, not the whole payload
/// (v0 note, proposal 8).
const PROMPT: &str = r#"You write one card that teaches a developer about one step a coding agent took while he waited on it. The card opens with a question he tries to answer before he taps to see the answer.

Ask why the agent took this step, and answer from the change below. Use only this input: do not use tools or read files.

Write question, answer, why_it_matters and not_checked in Hebrew, in plain words. Code, code terms, file paths and commands stay in English; inside Hebrew text, wrap each of them in backticks.

Reply with one JSON object and nothing else:
{"question": string, "answer": string, "paths": [string], "code": string, "why_it_matters": string, "not_checked": string}
- question: why the agent took this step, in one sentence.
- answer: the reason, in at most three sentences.
- paths: the files the step changed.
- code: the lines of the change that matter most, copied from it, at most 12.
- why_it_matters: what the step changes for the code, in one or two sentences.
- not_checked: one thing the step's result does not show.
"#;

/// What the card writer adds for a check: its verdict, which settles Daniel's guess (#328, module 4).
const CHECK: &str = r#"
This step is a check: a command that tests the code. The input below is its command and its output, not a change. For paths, give the files the output names; for code, the output lines that matter most.
Add one more key, "passed": true if the output shows the check passed, false if it shows it failed, or "unknown" if it shows neither.
"#;
