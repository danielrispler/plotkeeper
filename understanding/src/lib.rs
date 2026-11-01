//! The understanding layer's I/O, which the core leaves to the shell: the card writer (#328,
//! module 4), the worktree fingerprint (module 5), and the card store and its weekly rates (module 6).
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
use studio_core::CardRequest;

mod drift;
mod fingerprint;
mod store;
mod week;
pub use drift::{Drift, Versions, drift};
pub use fingerprint::Fingerprints;
pub use store::{CardStore, Guess, SessionCards, StoredCard, millis};
pub use week::{Rate, Week, week};

/// A card on a change, in Hebrew; code stays in English, and code inside the Hebrew text is in
/// backticks. It opens with its quiz; the rest shows when Daniel taps an option.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// What the change does and why. Cards stored before #375 call it `answer`.
    #[serde(alias = "answer")]
    pub explanation: String,
    // The fields below default to empty, so cards stored before they existed still load.
    /// The files the change touched.
    #[serde(default)]
    pub paths: Vec<String>,
    /// The change's real code: the lines that matter most.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub why_it_matters: String,
    #[serde(default)]
    pub not_checked: String,
    /// `None` when the writer's quiz was missing or unusable, and in cards stored before #375.
    #[serde(default)]
    pub quiz: Option<Quiz>,
}

/// A card's multiple-choice question on the change's logic or design (#369, decision 13), named
/// apart from an agent's question (`AskUserQuestion`). The writer's key grades Daniel's tap.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quiz {
    pub text: String,
    pub options: Vec<Choice>,
    /// The right option, counting from 0.
    pub key: usize,
}

/// One of a quiz's options, with one line on why it is right or wrong.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub text: String,
    pub why: String,
}

impl Quiz {
    /// The quiz in `value`, or why it is unusable: it does not parse, or its key names none of two
    /// or more options.
    fn from_value(value: serde_json::Value) -> Result<Quiz, String> {
        let quiz: Quiz = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let options = quiz.options.len();
        if options < 2 || quiz.key >= options {
            return Err(format!("key {} of {options} options", quiz.key));
        }
        Ok(quiz)
    }
}

impl Card {
    /// The card in a writer's reply, which must hold one JSON object, perhaps in a code fence. When
    /// it does not parse, the error is one line for the log. A card whose quiz is missing or
    /// unusable keeps its explanation, with one line on why it has no quiz.
    pub fn from_reply(reply: &str) -> Result<(Card, Option<String>), String> {
        let object = reply.find('{').zip(reply.rfind('}'));
        let object = object.filter(|(start, end)| start < end);
        let object = object.map_or("", |(start, end)| &reply[start..=end]);
        let fail = |error: serde_json::Error| {
            let head: String = reply.chars().take(300).collect();
            format!("the card writer's reply does not parse ({error}): {head:?}")
        };
        let mut value: serde_json::Value = serde_json::from_str(object).map_err(fail)?;
        let quiz = value.get_mut("quiz").map(serde_json::Value::take);
        let mut card: Card = serde_json::from_value(value).map_err(fail)?;
        match Quiz::from_value(quiz.unwrap_or_default()) {
            Ok(quiz) => {
                card.quiz = Some(quiz);
                Ok((card, None))
            }
            Err(error) => Ok((card, Some(format!("its quiz is unusable ({error})")))),
        }
    }
}

/// Writes the card on a session's change since its last card.
pub trait CardWriter {
    /// The writer's reply on `request`'s change, or one line for the log when it gave none.
    fn reply(&self, request: &CardRequest) -> Result<String, String>;
}

/// The card on `request` from `writer`'s reply, which is asked once more when it gives no card
/// (#369, decision 12). Each problem is one line for `log`: no card after the retry, or a card with
/// no quiz, which is kept without asking again.
pub fn write_card(
    writer: &(impl CardWriter + ?Sized),
    request: &CardRequest,
    mut log: impl FnMut(String),
) -> Option<Card> {
    let card = || {
        writer
            .reply(request)
            .and_then(|reply| Card::from_reply(&reply))
    };
    match card().or_else(|_| card()) {
        Ok((card, problem)) => {
            if let Some(problem) = problem {
                log(format!("no quiz on {}: {problem}", request.id));
            }
            Some(card)
        }
        Err(line) => {
            log(format!("no card on {}: {line}", request.id));
            None
        }
    }
}

/// Writes a fixed Hebrew card that shows the change, so the app can run without `claude -p`.
pub struct FakeWriter;

impl CardWriter for FakeWriter {
    fn reply(&self, request: &CardRequest) -> Result<String, String> {
        let paths: Vec<&str> = request
            .change
            .lines()
            .filter_map(|line| line.strip_prefix("+++ b/"))
            .collect();
        let option = |text: &str, why: &str| serde_json::json!({ "text": text, "why": why });
        let card = serde_json::json!({
            "explanation": "כרטיס לדוגמה של הכותב המזויף, שלא קורא את הקוד. השינוי עצמו למטה.",
            "paths": paths,
            "code": request.change,
            "why_it_matters": "כאן יופיע למה השינוי חשוב, עם קוד בתוך הטקסט כמו `if a <= b {`.",
            "not_checked": "לא נבדק: כלום, זה כרטיס לדוגמה.",
            "quiz": {
                "text": format!("מה הסוכן שינה ב־`{}`?", paths.join("`, `")),
                "options": [
                    option("את מה שמופיע בקוד למטה.", "נכון: הקוד למטה הוא השינוי עצמו."),
                    option("רק את התיעוד, בלי לגעת בקוד.", "לא: הכותב המזויף לא יודע מה השתנה."),
                    option("כלום, הוא רק הריץ בדיקות.", "לא: כרטיס נכתב רק על שינוי."),
                ],
                "key": 0,
            },
        });
        Ok(card.to_string())
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

    /// The `claude -p` run for one change. Without the two variables and without `--bare`, `claude`
    /// uses the subscription login (#320, "Fact"); the studio never reads that login itself. The
    /// change is untrusted text, so the run has no tools and no MCP servers, whatever Daniel's
    /// settings allow. It also runs without the studio plugin, so its session sends no hook events.
    pub fn command(&self, request: &CardRequest) -> Command {
        let change = &request.change;
        let kept = &change[..change.floor_char_boundary(MAX_CHANGE)];
        let cut = match change.len() - kept.len() {
            0 => String::new(),
            bytes => format!("\n[{bytes} more bytes cut]"),
        };
        let prompt = format!("{PROMPT}\nChange:\n{kept}{cut}");
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
    /// Runs `claude -p`, which takes seconds, so the shell calls it off its log thread.
    // shortcut: no timeout, as in v0; add one if a stuck `claude -p` ever holds a card back.
    fn reply(&self, request: &CardRequest) -> Result<String, String> {
        match self.command(request).output() {
            Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).into()),
            Err(error) => Err(format!("`claude -p` did not start: {error}")),
        }
    }
}

/// The most of a change the writer gets: 64 KiB. The lines that matter fit, a huge new file or log
/// costs no more, and the prompt stays far under macOS's 1 MiB limit on a command's arguments.
const MAX_CHANGE: usize = 64 << 10;

/// The card writer's instructions. Its input is only the change, not the payloads (v0 note,
/// proposal 8).
const PROMPT: &str = r#"You write one card that teaches a developer about the change a coding agent made to his code while he waited on it, since the previous card. The card opens with a multiple-choice question on the change's logic or design. He taps an option, and the card shows at once whether he was right, why each option is right or wrong, and the explanation.

Answer from the diff below. Use only this input: do not use tools or read files.

Write every text field in Hebrew, in plain words. Code, code terms, file paths and commands stay in English; inside Hebrew text, wrap each of them in backticks.

Reply with one JSON object and nothing else:
{"explanation": string, "paths": [string], "code": string, "why_it_matters": string, "not_checked": string, "quiz": {"text": string, "options": [{"text": string, "why": string}], "key": number}}
- explanation: what the change does and why, in at most three sentences.
- paths: the files the change touched.
- code: the lines of the change that matter most, copied from it, at most 12.
- why_it_matters: what the change does for the code, in one or two sentences.
- not_checked: one thing the diff does not show.
- quiz.text: one question on the change's logic or design that the diff answers, such as why the code works this way, what a line does, or what would break without it. Never ask whether a command, test or check will pass.
- quiz.options: 3 or 4 options, the right one at a random place. Each wrong option is something the code could plausibly have done instead, of the right option's length and tone, so the wording gives nothing away.
- quiz.options[].why: one line on why that option is right or wrong.
- quiz.key: the place of the right option in quiz.options, counting from 0.
"#;
