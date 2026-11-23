//! The understanding layer's I/O, which the core leaves to the shell: the card writer (danielrispler/Falinks#328,
//! module 4), which also writes the briefs (danielrispler/Falinks#369, decision 16), the worktree fingerprint (module 5),
//! the card store with its briefs and its weekly rates (module 6), and the drift in Claude Code
//! (danielrispler/Falinks#369, decisions 7, 9 and 23).
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use studio_core::{CardRequest, Session, SummaryRequest, Transcript};

mod brief;
mod drift;
mod fingerprint;
mod grounding;
mod logic_file;
pub mod pr_brief;
pub mod pr_brief_check;
mod risk;
mod store;
mod summary;
mod turn_trees;
mod turn_writing;
mod writer;
pub use brief::{
    Answered, DesignLine, OpenItem, OpenKind, answered, design_lines, design_sources, open_items,
    write_design,
};
pub use drift::{AGY_FIXTURE_VERSION, AgyVersions, Drift, Versions, drift, dropped_events};
pub use fingerprint::{Fingerprints, fingerprint_changes, live_roots, worktree_root};
pub use risk::{RISK_TIMEOUT, Risk, RiskLevel, RiskNote, RiskNotes, RiskRequest, RiskView, judge};
pub use store::{
    Arm, Brief, CardStore, CheckResult, Guess, Level, QuizTap, Rating, Reason, Retest, Row,
    SessionCards, StoredCard, StoredSummary, Surface, Tap, millis,
};
pub use summary::{
    Line, LineKind, SummarySetup, TurnSummary, write_agent_summary, write_name, write_summary,
};
pub use turn_trees::{
    Hunk, MAX_TURN_FILES, add_hunks, anchor, diff_turn, found_files, hunk, hunk_view, numstat,
    turn_diffs, turn_facts, turn_hunk,
};
pub use turn_writing::{Core, Effects, TurnWriting, Writer};
pub use writer::{
    AGY_LOGIN_WAIT, FailureKind, FoundWriter, Usage, WriterError, WriterKind, WriterReply,
    WriterRequest, build_command, found_writers, read_reply,
};

/// Lines `from` to `to`, counted from 1 and both included, of a file in a card's end tree, as a card stored before
/// danielrispler/plotkeeper#348 names them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodeRange {
    pub path: String,
    pub from: usize,
    pub to: usize,
}

/// A card on a change, as the store keeps the ones written before cards were retired (danielrispler/plotkeeper#348): the
/// frozen samples load it, and none is written now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// What the change does and why. Cards stored before danielrispler/Falinks#375 call it `answer`.
    #[serde(alias = "answer")]
    pub explanation: String,
    // The fields below default to empty, so cards stored before they existed still load.
    /// What the change did, in one line: the session's row while its turn is over, and the brief's
    /// rewrite (danielrispler/Falinks#369, decisions 16 and 19).
    #[serde(default)]
    pub summary: String,
    /// The files the change touched.
    #[serde(default)]
    pub paths: Vec<String>,
    /// The change's real code: the lines that matter most, pulled from git by `ranges` when the card was
    /// written (danielrispler/plotkeeper#121). Cards stored before that keep the writer's own copy.
    #[serde(default)]
    pub code: String,
    /// Where the writer said `code` is; `code` is what they hold in the turn's end tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ranges: Vec<CodeRange>,
    #[serde(default)]
    pub why_it_matters: String,
    #[serde(default)]
    pub not_checked: String,
    /// How the change alters the code's structure, when it does (danielrispler/Falinks#369, decision 12); `None` when the
    /// writer answered `structural: none`.
    #[serde(default)]
    pub architecture: Option<String>,
    /// `None` when the writer's quiz was missing or unusable, and in cards stored before danielrispler/Falinks#375.
    #[serde(default)]
    pub quiz: Option<Quiz>,
}

/// A multiple-choice question as the store kept it, on a card or a turn's summary written before the Quiz was retired
/// (danielrispler/plotkeeper#347). Named apart from an agent's question (`AskUserQuestion`).
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

/// The writers of the turn: its Goal, Design so far, summaries and names, and Daniel's side questions.
pub trait CardWriter {
    /// The writer's reply with the Goal of a session, or of a worker, from `prompts`, Daniel's or its
    /// spawn's, oldest first (danielrispler/plotkeeper#122), or one line for the log when it gave none.
    /// A writer that writes no goals says so.
    fn goal(&self, _prompts: &[&str]) -> Result<String, String> {
        Err("this writer writes no goals".into())
    }

    /// Chooses which program writes from now on, from the settings (#327). A writer with one program ignores it.
    fn use_writer(&self, _kind: WriterKind) {}

    /// Turns the writer's calls on or off: off, until a writer passes Test, for a user who arrived with none configured
    /// (spec #320, "The Test gate"). A Test runs either way.
    fn summaries_on(&self, _on: bool) {}

    /// The page's Test: one tiny call to writer `kind` at `model` and `effort` (empty: its default), and its typed outcome.
    /// A writer that runs no program passes.
    fn test(&self, _kind: WriterKind, _model: &str, _effort: &str) -> Result<(), WriterError> {
        Ok(())
    }

    /// The writer's reply with "Design so far" of a session from `sources`, each a turn's or a card's
    /// id and a line of it, oldest first (danielrispler/plotkeeper#123), or one line for the log when it gave none.
    /// A writer that writes no design says so.
    fn design(&self, _sources: &[(String, String)]) -> Result<String, String> {
        Err("this writer writes no design".into())
    }

    /// The writer's reply to Daniel's side `question` on `item`, a response, card or intention, read
    /// with `intentions`, those of the item's turn, oldest first (danielrispler/Falinks#461, decision 20), or one line
    /// when it gave none. A writer that answers no questions says so.
    fn ask(&self, _item: &str, _intentions: &[String], _question: &str) -> Result<String, String> {
        Err("this writer answers no questions".into())
    }

    /// The writer's reply with the summary of `request`'s turn, read with `earlier`, the session's
    /// latest summaries, oldest first, at `setup`'s model and effort (danielrispler/Falinks#526), or one line when it
    /// gave none. A writer that writes no summaries says so.
    fn summarize(
        &self,
        _setup: &SummarySetup,
        _request: &SummaryRequest,
        _earlier: &[TurnSummary],
    ) -> Result<String, String> {
        Err("this writer writes no summaries".into())
    }

    /// The writer's reply with the summary of a subagent's purpose and result, from its spawn's
    /// `prompt` and its last `message`, at `setup`'s model and effort (danielrispler/Falinks#535), or one line when it
    /// gave none. A writer that writes no summaries says so.
    fn summarize_agent(
        &self,
        _setup: &SummarySetup,
        _prompt: &str,
        _message: &str,
    ) -> Result<String, String> {
        Err("this writer writes no summaries".into())
    }

    /// The writer's risk note on a blocked `command` that was asked to run in `folder`
    /// (danielrispler/plotkeeper#298), or one line for the log when it gave none. A writer that
    /// writes no risk notes says so.
    fn risk(&self, _command: &str, _folder: &str) -> Result<Risk, String> {
        Err("this writer writes no risk notes".into())
    }

    /// The writer's reply with a session's name, from Daniel's first `prompt` and, when it was a
    /// bare slash command, the turn's final `response`, at `setup`'s model and effort, or one line
    /// when it gave none. A writer that writes no summaries says so.
    fn name(
        &self,
        _setup: &SummarySetup,
        _prompt: &str,
        _response: Option<&str>,
    ) -> Result<String, String> {
        Err("this writer writes no summaries".into())
    }
}

/// The Goal of `request`'s owner, its worker or else `session`, written again when its prompts
/// changed since it was written: one or two lines from Daniel's prompts in the session (a worker's
/// from its spawn), the only part of the brief a model writes (danielrispler/plotkeeper#122). Each
/// problem is one line for `log`. Returns whether the Goal changed.
pub fn write_goal(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    request: &CardRequest,
    session: &Session,
    mut log: impl FnMut(String),
) -> bool {
    let (owner, agent) = (request.owner(), request.agent.as_deref());
    let prompts: Vec<&str> = match agent.and_then(|id| session.subagent(id)) {
        Some(worker) => vec![&worker.prompt],
        None => {
            let daniels = session.prompts.iter();
            daniels
                .filter(|p| p.sender.is_daniels())
                .map(|p| p.text.as_str())
                .collect()
        }
    };
    let written = store.lock().unwrap().brief(owner).map(|b| b.goal_prompts);
    if prompts.is_empty() || written == Some(prompts.len() as u32) {
        return false;
    }
    let reply = writer.goal(&prompts).and_then(|reply| goal_in(&reply));
    match reply {
        Ok(goal) => match store
            .lock()
            .unwrap()
            .set_goal(owner, goal, prompts.len() as u32)
        {
            Ok(()) => true,
            Err(e) => {
                log(format!("save the card store: {e}"));
                false
            }
        },
        Err(line) => {
            log(format!("no goal on {}: {line}", request.id));
            false
        }
    }
}

/// The answer to Daniel's side `question` on `item` (danielrispler/Falinks#461, decision 20), from one call to `writer`
/// with the intentions of `transcript`'s turn, one per response. It stays in the studio: the session
/// never sees it. Why there is none when the call failed or its reply holds no answer.
pub fn ask_about(
    writer: &(impl CardWriter + ?Sized),
    transcript: &Transcript,
    item: &str,
    question: &str,
) -> Result<String, String> {
    let reply = writer.ask(item, &transcript.intentions(), question)?;
    let value = parse_object(&reply).unwrap_or_default();
    match value["answer"].as_str().map(str::trim) {
        Some(answer) if !answer.is_empty() => Ok(answer.into()),
        _ => Err("the writer's reply holds no answer".into()),
    }
}

/// The writer's input item for a question on `what` (danielrispler/plotkeeper#126): the recorded `facts`, and the
/// `hunk` of a file, as recorded and apart from anything the writer says.
pub fn ask_item(what: &str, facts: &studio_core::AskFacts, hunk: Option<&str>) -> String {
    let mut item = format!("About {what}. What was recorded:\n{}", facts.text());
    if let Some(hunk) = hunk {
        item.push_str(&format!("\n\nThe hunk:\n{hunk}"));
    }
    item
}

/// What "why didn't it touch `name`" gives: what was recorded about it, and the writer's possible reasons, which are
/// guesses and never recorded.
#[derive(Debug, PartialEq, Serialize)]
pub struct WhyNot {
    pub facts: studio_core::AskFacts,
    pub reasons: Option<String>,
}

/// "Why didn't it touch `name`?" on its recorded `facts`: the writer is asked for possible reasons only when something was
/// recorded about `name`. When the session never read, searched or mentioned it, the facts say so and there is no call.
pub fn why_not(
    writer: &(impl CardWriter + ?Sized),
    transcript: &Transcript,
    name: &str,
    facts: studio_core::AskFacts,
) -> Result<WhyNot, String> {
    if facts.sections.is_empty() {
        return Ok(WhyNot {
            facts,
            reasons: None,
        });
    }
    let item = ask_item(&format!("`{name}`"), &facts, None);
    let question = format!(
        "Why didn't the agent change {name}? Give possible reasons, each as a guess from what was recorded."
    );
    let reasons = ask_about(writer, transcript, &item, &question)?;
    Ok(WhyNot {
        facts,
        reasons: Some(reasons),
    })
}

/// The JSON object in a writer's reply, perhaps in a code fence: from its first `{` to its last `}`.
fn object(reply: &str) -> &str {
    let object = reply.find('{').zip(reply.rfind('}'));
    let object = object.filter(|(start, end)| start < end);
    object.map_or("", |(start, end)| &reply[start..=end])
}

/// The JSON object in a writer's reply, parsed: the one place a reply is read.
pub(crate) fn parse_object(reply: &str) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(object(reply))
}

/// Asks a writer once more when `retry` says the `first` result cannot stand, and `pick` chooses between the second
/// result and the first. Each writer keeps its own `retry` and `pick`.
pub(crate) fn once_more<T, E>(
    first: Result<T, E>,
    retry: impl FnOnce(&Result<T, E>) -> bool,
    ask: impl FnOnce() -> Result<T, E>,
    pick: impl FnOnce(Result<T, E>, Result<T, E>) -> Result<T, E>,
) -> Result<T, E> {
    if retry(&first) {
        pick(ask(), first)
    } else {
        first
    }
}

/// Why a reply gave no goal, for the log.
const HOLDS_NONE: &str = "the reply holds none";

/// The Goal in a writer's reply, when it holds one that is not empty, or else why not, for the log.
fn goal_in(reply: &str) -> Result<String, String> {
    let value = parse_object(reply).unwrap_or_default();
    match value["goal"].as_str().map(str::trim) {
        Some(goal) if !goal.is_empty() => Ok(goal.into()),
        _ => Err(HOLDS_NONE.into()),
    }
}

/// Writes a fixed card that shows the change, and a fixed goal, so the app can run without
/// `claude -p`.
pub struct FakeWriter;

impl CardWriter for FakeWriter {
    fn goal(&self, prompts: &[&str]) -> Result<String, String> {
        let goal = format!(
            "A sample goal from the fake writer, which read {} prompts.",
            prompts.len()
        );
        Ok(serde_json::json!({ "goal": goal }).to_string())
    }

    fn design(&self, sources: &[(String, String)]) -> Result<String, String> {
        let design: Vec<_> = sources
            .iter()
            .take(1)
            .map(|(id, _)| serde_json::json!({"text": "A sample decision from the fake writer, because it reads nothing.", "cite": id}))
            .collect();
        Ok(serde_json::json!({ "design": design }).to_string())
    }

    fn ask(&self, _: &str, intentions: &[String], question: &str) -> Result<String, String> {
        // `PLOTKEEPER_FAKE_ASK=fail` makes it fail, so the error and its retry can be seen.
        if std::env::var("PLOTKEEPER_FAKE_ASK").as_deref() == Ok("fail") {
            return Ok("Shall I read the file?".into());
        }
        let answer = format!(
            "A sample answer from the fake writer, which does not read the session, to `{question}`. It was given {} intentions of the turn.",
            intentions.len()
        );
        Ok(serde_json::json!({ "answer": answer }).to_string())
    }

    fn summarize(
        &self,
        _: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        let option = |text: &str, why: &str, right: bool| serde_json::json!({ "text": text, "why": why, "right": right });
        let headline = "A sample headline from the fake writer, which does not read the turn.";
        let line = format!(
            "It was given {} responses and {} earlier summaries.",
            request.responses.len(),
            earlier.len()
        );
        Ok(serde_json::json!({
            "headline": headline,
            "lines": [{ "kind": "reported", "text": line }],
            "quiz": {
                "text": "What did the fake writer read?",
                "options": [
                    option("Nothing of the turn.", "Right: it writes a fixed sample.", true),
                    option("The whole transcript.", "No: it reads nothing.", false),
                    option("Only the final reply.", "No: it reads nothing.", false),
                ],
                "key": 0,
            },
        })
        .to_string())
    }

    fn summarize_agent(&self, _: &SummarySetup, prompt: &str, _: &str) -> Result<String, String> {
        let summary = format!(
            "A sample subagent summary from the fake writer, which read a {}-byte prompt.",
            prompt.len()
        );
        Ok(serde_json::json!({ "summary": summary }).to_string())
    }

    fn name(&self, _: &SummarySetup, _: &str, _: Option<&str>) -> Result<String, String> {
        Ok(serde_json::json!({ "name": "Sample session name" }).to_string())
    }

    fn risk(&self, _: &str, _: &str) -> Result<Risk, String> {
        Ok(Risk {
            level: RiskLevel::NotSure,
            line: "A sample risk note from the fake writer, which does not read the command."
                .into(),
        })
    }
}

/// Writes the card with `claude -p` on Daniel's subscription, in a neutral folder that holds none of
/// his projects, so it loads no repository's instructions and has nothing to wander into.
pub struct ClaudeWriter {
    /// The `claude` program, from `find_claude`.
    program: PathBuf,
    folder: PathBuf,
    /// The `agy` program and its own neutral folder, for when the settings choose it (#327).
    agy_program: PathBuf,
    agy_folder: PathBuf,
    /// The same for `codex` (#364).
    codex_program: PathBuf,
    codex_folder: PathBuf,
    /// The program that writes now.
    kind: Mutex<WriterKind>,
    /// How long a call may run before it fails and is stopped (danielrispler/Falinks#434, decision 30); unset, the
    /// program's own (`WriterKind::timeout`).
    timeout: Option<Duration>,
    /// The same for the risk note's judge, which is not to be waited for (danielrispler/plotkeeper#298).
    risk_timeout: Duration,
    /// Told when no call can start and when one works again (danielrispler/plotkeeper#51).
    availability: Availability,
    /// Whether calls run: off until a writer passes Test for a user who arrived with none configured (spec #320).
    on: AtomicBool,
}

/// The Test's one tiny call.
const TEST_PROMPT: &str = "Reply with the word ok and nothing else.";

/// The notice now shown, and who hears when it changes.
#[derive(Default)]
struct Availability {
    shown: Mutex<Option<String>>,
    on_change: Option<Box<dyn Fn(Option<String>) + Send + Sync>>,
}

impl Availability {
    /// A call failed: tells its notice, once until it changes or a call works.
    fn failed(&self, error: &WriterError) {
        let notice = error.notice();
        let mut shown = self.shown.lock().unwrap();
        if shown.as_ref() != Some(&notice) {
            *shown = Some(notice.clone());
            self.tell(Some(notice));
        }
    }

    /// A call worked: clears the notice, if one was told.
    fn worked(&self) {
        if self.shown.lock().unwrap().take().is_some() {
            self.tell(None);
        }
    }

    fn tell(&self, notice: Option<String>) {
        if let Some(on_change) = &self.on_change {
            on_change(notice);
        }
    }
}

/// The `claude` program, as login shell `shell` finds it, or `claude` when it answers no path. An
/// app started from Finder or `open` gets launchd's `PATH`, which lacks Homebrew's folder. Only the
/// path is kept: the shell's environment, which a profile may give the variables the writer removes,
/// never reaches the writer.
pub fn find_claude(shell: &Path) -> PathBuf {
    find_program(shell, "claude")
}

/// `find_claude` for any `program`, such as `agy`.
pub fn find_program(shell: &Path, program: &str) -> PathBuf {
    find(shell, program).unwrap_or_else(|| program.into())
}

/// `program`'s path as login shell `shell`'s `command -v` answers it, or `None` when it finds none.
pub fn find(shell: &Path, program: &str) -> Option<PathBuf> {
    let output = Command::new(shell)
        .args(["-lc", &format!("command -v {program}")])
        .output();
    let stdout = output.ok().filter(|o| o.status.success()).map(|o| o.stdout);
    let stdout = stdout.as_deref().map(String::from_utf8_lossy);
    // A profile may print before the answer, which is the last line.
    match stdout.as_deref().and_then(|out| out.lines().last()) {
        Some(path) if path.starts_with('/') => Some(path.into()),
        _ => None,
    }
}

impl ClaudeWriter {
    pub fn new(program: PathBuf, folder: PathBuf) -> Self {
        Self {
            agy_program: WriterKind::Agy.program().into(),
            agy_folder: folder.with_file_name(WriterKind::Agy.folder()),
            codex_program: WriterKind::Codex.program().into(),
            codex_folder: folder.with_file_name(WriterKind::Codex.folder()),
            kind: Mutex::new(WriterKind::Claude),
            program,
            folder,
            timeout: None,
            risk_timeout: RISK_TIMEOUT,
            availability: Availability::default(),
            on: AtomicBool::new(true),
        }
    }

    /// The writer that calls `on_change` with a notice when no call can start (a missing `claude`),
    /// once, and with `None` when a call works again. A call that starts and fails is no notice.
    pub fn on_availability(
        mut self,
        on_change: impl Fn(Option<String>) + Send + Sync + 'static,
    ) -> Self {
        self.availability.on_change = Some(Box::new(on_change));
        self
    }

    /// The writer with each call stopped after `timeout` instead of 5 minutes.
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self {
            timeout: Some(timeout),
            ..self
        }
    }

    /// The writer that runs `agy` as `program`, in the neutral folder `folder`.
    pub fn with_agy(self, program: PathBuf, folder: PathBuf) -> Self {
        Self {
            agy_program: program,
            agy_folder: folder,
            ..self
        }
    }

    /// The writer that runs `codex` as `program`, in the neutral folder `folder`.
    pub fn with_codex(self, program: PathBuf, folder: PathBuf) -> Self {
        Self {
            codex_program: program,
            codex_folder: folder,
            ..self
        }
    }

    /// The writer that starts as `kind`; the settings change it later with `use_writer`.
    pub fn with_kind(self, kind: WriterKind) -> Self {
        *self.kind.lock().unwrap() = kind;
        self
    }

    /// Whether the login shell found `claude`, which PR briefs need whatever writer writes summaries (spec #320).
    /// `find_claude` keeps the bare name when it finds no path.
    pub fn claude_found(&self) -> bool {
        self.program.is_absolute()
    }

    /// The program that writes now.
    pub fn kind(&self) -> WriterKind {
        *self.kind.lock().unwrap()
    }

    /// How long a call may run now.
    pub fn timeout(&self) -> Duration {
        self.timeout.unwrap_or(self.kind().timeout())
    }

    /// The writer for event log `log`: its neutral folder, `card-writer` beside the log, made here,
    /// and `claude` as the login shell finds it. An error when the folder cannot be made.
    pub fn beside(log: &Path) -> std::io::Result<Self> {
        let folder = log.with_file_name(WriterKind::Claude.folder());
        let agy_folder = log.with_file_name(WriterKind::Agy.folder());
        let codex_folder = log.with_file_name(WriterKind::Codex.folder());
        std::fs::create_dir_all(&folder)?;
        std::fs::create_dir_all(&agy_folder)?;
        std::fs::create_dir_all(&codex_folder)?;
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        let agy = find_program(shell.as_ref(), WriterKind::Agy.program());
        let codex = find_program(shell.as_ref(), WriterKind::Codex.program());
        Ok(Self::new(find_claude(shell.as_ref()), folder)
            .with_agy(agy, agy_folder)
            .with_codex(codex, codex_folder))
    }

    /// The `claude -p` run that writes a session's Goal from Daniel's `prompts`. Past `MAX_CHANGE`,
    /// the oldest prompts are cut first, and the newest is shortened rather than cut.
    pub fn goal_command(&self, prompts: &[&str]) -> (Command, String) {
        let newest = prompts.last().copied();
        let prompts: Vec<String> = prompts
            .iter()
            .map(|prompt| format!("<prompt>\n{prompt}\n</prompt>\n"))
            .collect();
        let mut size = prompts.iter().map(String::len).sum::<usize>();
        let mut cut = 0;
        while size > MAX_CHANGE && cut + 1 < prompts.len() {
            size -= prompts[cut].len();
            cut += 1;
        }
        let note = match cut {
            0 => String::new(),
            cut => format!(" (the {cut} oldest cut)"),
        };
        let mut kept = prompts[cut..].concat();
        // The newest prompt alone over the budget is shortened, not dropped (danielrispler/Falinks#434, decision 33),
        // keeping its start.
        if size > MAX_CHANGE
            && let Some(newest) = newest
        {
            let keep = newest
                .len()
                .saturating_sub(size - MAX_CHANGE)
                .max(MIN_PROMPT);
            let end = newest.floor_char_boundary(keep);
            let over = newest.len() - end;
            kept = match over {
                0 => kept,
                _ => format!(
                    "<prompt>\n{}\n[{over} more bytes cut]\n</prompt>\n",
                    &newest[..end]
                ),
            };
        }
        self.run(&format!(
            "{GOAL}\nThe developer's prompts, oldest first{note}:\n{kept}"
        ))
    }

    /// The `claude -p` run that writes a session's "Design so far" from `sources`. Past `MAX_CHANGE`,
    /// the oldest are cut first.
    pub fn design_command(&self, sources: &[(String, String)]) -> (Command, String) {
        let lines: Vec<String> = sources
            .iter()
            .map(|(id, line)| format!("[{id}] {}\n", line.replace('\n', " ")))
            .collect();
        let mut size = lines.iter().map(String::len).sum::<usize>();
        let mut cut = 0;
        while size > MAX_CHANGE && cut + 1 < lines.len() {
            size -= lines[cut].len();
            cut += 1;
        }
        self.run(&format!(
            "{DESIGN}\nThe turns' reasons and the cards' summaries, oldest first:\n{}",
            lines[cut..].concat()
        ))
    }

    /// The `claude -p` run for Daniel's side `question` on `item`, read with its turn's `intentions`.
    /// The item is cut at `MAX_CHANGE`, as a response may be long.
    pub fn ask_command(
        &self,
        item: &str,
        intentions: &[String],
        question: &str,
    ) -> (Command, String) {
        let intentions: String = match intentions {
            [] => "none\n".into(),
            lines => lines.iter().map(|line| format!("- {line}\n")).collect(),
        };
        let item = &item[..item.floor_char_boundary(MAX_CHANGE)];
        self.run(&format!(
            "{ASK}\nThe intentions of the turn, oldest first:\n{intentions}\n\
             <item>\n{item}\n</item>\n\nThe developer's question:\n{question}"
        ))
    }

    /// The `claude -p` run on `prompt`. Without the two variables and without `--bare`, `claude`
    /// uses the subscription login (danielrispler/Falinks#320, "Fact"); the studio never reads that login itself. The
    /// prompt holds untrusted text, a change or Daniel's prompts, so the run has no tools and no MCP
    /// servers, whatever Daniel's settings allow. It also runs without the studio plugin, so its
    /// session sends no hook events, and in safe mode, so none of Daniel's plugins, hooks, skills or
    /// CLAUDE.md reach it (danielrispler/Falinks#517, decision 9). The prompt goes on standard input without its NUL bytes, which
    /// `claude` could not take in an argument either (danielrispler/Falinks#434, decision 31).
    fn run(&self, prompt: &str) -> (Command, String) {
        self.run_with(None, None, prompt)
    }

    /// `run`, with `setup`'s model and effort and the studio's own `system` prompt.
    pub fn run_with(
        &self,
        setup: Option<&SummarySetup>,
        system: Option<&str>,
        prompt: &str,
    ) -> (Command, String) {
        self.build(&WriterRequest {
            system: system.map(Into::into),
            user: prompt.into(),
            model: setup.map(|s| s.model.clone()),
            effort: setup.map(|s| s.effort.clone()),
            timeout: self.timeout(),
            ..WriterRequest::default()
        })
    }

    /// The command that makes `request` with the program that writes now (`build_command`).
    pub fn build(&self, request: &WriterRequest) -> (Command, String) {
        match self.kind() {
            kind @ WriterKind::Claude => build_command(kind, &self.program, &self.folder, request),
            kind @ WriterKind::Agy => {
                build_command(kind, &self.agy_program, &self.agy_folder, request)
            }
            kind @ WriterKind::Codex => {
                build_command(kind, &self.codex_program, &self.codex_folder, request)
            }
        }
    }

    /// What `claude -p --output-format json` printed for a PR brief's call at `model` on `prompt`: always Claude, whatever
    /// `summaries.writer` says (danielrispler/plotkeeper#334, port decision Q4). One line for the log when the call failed.
    pub(crate) fn brief_stdout(&self, model: &str, prompt: &str) -> Result<String, String> {
        let request = WriterRequest {
            user: prompt.into(),
            model: Some(model.into()),
            timeout: self.timeout.unwrap_or(WriterKind::Claude.timeout()),
            ..WriterRequest::default()
        };
        let (mut command, stdin) =
            build_command(WriterKind::Claude, &self.program, &self.folder, &request);
        command.args(["--output-format", "json"]);
        match self.call(request.timeout, (command, stdin), false) {
            Ok(reply) => Ok(reply.text),
            Err(error) => Err(error.to_string()),
        }
    }

    /// The contract's one call: `request` made and its reply read, or why not.
    pub fn complete(&self, request: &WriterRequest) -> Result<WriterReply, WriterError> {
        self.on_or(self.kind())?;
        let run = self.build(request);
        self.call(request.timeout, run, request.schema.is_some())
    }

    /// What a run printed, or one line for the log when the call failed: it did not start, exited
    /// with an error, printed nothing, or ran past the timeout, which stops it and every process it
    /// started, all in its own process group (danielrispler/Falinks#434, decision 30).
    fn stdout(&self, run: (Command, String)) -> Result<String, String> {
        self.stdout_within(self.timeout(), run)
    }

    /// `stdout`, with the call stopped after `timeout`.
    fn stdout_within(&self, timeout: Duration, run: (Command, String)) -> Result<String, String> {
        self.on_or(self.built_for(&run.0))
            .map_err(|error| error.to_string())?;
        match self.call(timeout, run, false) {
            Ok(reply) => Ok(reply.text),
            Err(error) => Err(error.to_string()),
        }
    }

    /// The writer whose program `command` runs.
    fn built_for(&self, command: &Command) -> WriterKind {
        if command.get_program() == self.agy_program.as_os_str() {
            WriterKind::Agy
        } else if command.get_program() == self.codex_program.as_os_str() {
            WriterKind::Codex
        } else {
            WriterKind::Claude
        }
    }

    /// Nothing, or why no call runs: summaries are off until a writer passes Test. Being off is no failure notice.
    fn on_or(&self, kind: WriterKind) -> Result<(), WriterError> {
        match self.on.load(Ordering::Acquire) {
            true => Ok(()),
            false => Err(WriterError::new(
                FailureKind::Other,
                kind,
                "(summaries are off until a writer passes Test)",
            )),
        }
    }

    /// A run, read into the contract: its reply, or a typed failure.
    fn call(
        &self,
        timeout: Duration,
        run: (Command, String),
        schema: bool,
    ) -> Result<WriterReply, WriterError> {
        // The writer the command was built for, not the one set now: the settings may switch between the two.
        let kind = self.built_for(&run.0);
        self.call_as(kind, timeout, run, schema, true)
    }

    /// `call` for writer `kind`; `tell`: its failure or success changes the notice.
    fn call_as(
        &self,
        kind: WriterKind,
        timeout: Duration,
        (mut command, prompt): (Command, String),
        schema: bool,
        tell: bool,
    ) -> Result<WriterReply, WriterError> {
        let tell_failed = |error: &WriterError| {
            if tell {
                self.availability.failed(error)
            }
        };
        let timeout = stop_after(kind, timeout);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| {
                let not_found = e.kind() == std::io::ErrorKind::NotFound;
                let kind_of = if not_found {
                    FailureKind::NotInstalled
                } else {
                    FailureKind::Other
                };
                let error = WriterError::new(kind_of, kind, format!("({e})"));
                tell_failed(&error);
                error
            })?;
        let (mut stdin, mut stdout) = (child.stdin.take(), child.stdout.take());
        // Threads, so a call that reads no input or never ends cannot hold back the timeout.
        thread::spawn(move || stdin.as_mut().map(|i| i.write_all(prompt.as_bytes())));
        let (sender, read) = mpsc::channel();
        thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.as_mut().map(|o| o.read_to_end(&mut out));
            let _ = sender.send(out);
        });
        let deadline = Instant::now() + timeout;
        let out = read.recv_timeout(timeout).ok();
        let status = loop {
            match child.try_wait() {
                Ok(None) if out.is_some() && Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10))
                }
                Ok(status) => break status,
                Err(_) => break None,
            }
        };
        let (Some(out), Some(status)) = (out, status) else {
            // The group's id stays taken while the unreaped call or a process holding its output
            // lives, so this stops no one else.
            // SAFETY: `kill` takes no pointers.
            unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.wait();
            let error = WriterError::new(
                FailureKind::Timeout,
                kind,
                format!("(ran past {timeout:?})"),
            );
            // A call kept short on purpose (the risk judge) timing out is no fault of the writer.
            if timeout >= kind.timeout() {
                tell_failed(&error);
            }
            return Err(error);
        };
        let reply = read_reply(kind, schema, &status, &String::from_utf8_lossy(&out));
        match &reply {
            Ok(_) if tell => self.availability.worked(),
            Ok(_) => {}
            Err(error) => tell_failed(error),
        }
        reply
    }
}

impl CardWriter for ClaudeWriter {
    fn use_writer(&self, kind: WriterKind) {
        *self.kind.lock().unwrap() = kind;
    }

    fn summaries_on(&self, on: bool) {
        self.on.store(on, Ordering::Release);
    }

    fn test(&self, kind: WriterKind, model: &str, effort: &str) -> Result<(), WriterError> {
        let request = WriterRequest {
            user: TEST_PROMPT.into(),
            model: (!model.is_empty()).then(|| model.into()),
            effort: Some(effort.into()),
            timeout: self.timeout.unwrap_or(kind.timeout()),
            ..WriterRequest::default()
        };
        let (program, folder) = match kind {
            WriterKind::Claude => (&self.program, &self.folder),
            WriterKind::Agy => (&self.agy_program, &self.agy_folder),
            WriterKind::Codex => (&self.codex_program, &self.codex_folder),
        };
        let run = build_command(kind, program, folder, &request);
        self.call_as(kind, request.timeout, run, false, false)
            .map(|_| ())
    }

    fn goal(&self, prompts: &[&str]) -> Result<String, String> {
        self.stdout(self.goal_command(prompts))
    }

    fn design(&self, sources: &[(String, String)]) -> Result<String, String> {
        self.stdout(self.design_command(sources))
    }

    fn ask(&self, item: &str, intentions: &[String], question: &str) -> Result<String, String> {
        self.stdout(self.ask_command(item, intentions, question))
    }

    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        self.stdout(self.summary_command(setup, request, earlier))
    }

    fn summarize_agent(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        message: &str,
    ) -> Result<String, String> {
        self.stdout(self.agent_summary_command(setup, prompt, message))
    }

    fn name(
        &self,
        setup: &SummarySetup,
        prompt: &str,
        response: Option<&str>,
    ) -> Result<String, String> {
        self.stdout(self.name_command(setup, prompt, response))
    }

    fn risk(&self, command: &str, folder: &str) -> Result<Risk, String> {
        ClaudeWriter::risk(self, command, folder)
    }
}

/// The most of a change the writer gets, and of a brief rewrite's input: 64 KiB. The lines that
/// matter fit, and a huge new file or log costs no more.
const MAX_CHANGE: usize = 64 << 10;

/// The least of the newest prompt a brief rewrite keeps, past `MAX_CHANGE` if it must: 4 KiB.
const MIN_PROMPT: usize = 4 << 10;

/// The explanation language, one paragraph in each of the writer's instructions below. A macro, as
/// `concat!` takes literals and no `const`.
macro_rules! explanation_language {
    () => {
        "Write in English, in plain words a developer new to this code follows. Wrap code, code terms, file paths \
         and commands in backticks."
    };
}
pub(crate) use explanation_language;

/// The writer's instructions for a session's Goal (danielrispler/plotkeeper#122): from Daniel's prompts
/// only, so it names what he wants done and not what the agent did.
const GOAL: &str = concat!(
    r#"You write the Goal of a coding agent's session, which the developer who runs it reads to resume the session: one or two short lines on the outcome he wants, such as "Show what the agent did on the rail from recorded facts", not a restatement of his first prompt and not what the agent did so far. When his later prompts changed the aim, the Goal follows the latest aim.

Answer from the input below: the developer's prompts, oldest first. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else: {"goal": string}
"#
);

/// The writer's instructions for a session's "Design so far" (danielrispler/plotkeeper#123): from the turns' reasons
/// and the cards' summaries only, each line citing the id it came from.
const DESIGN: &str = concat!(
    r#"You write "Design so far" of a coding agent's session, which the developer who runs it reads to build his model of the program: at most five lines, each one design decision of this session with its reason, in one line of at most 25 words, such as "Anchor turn trees under a ref, because git gc would otherwise drop old hunks". Each line cites the id, in square brackets in the input, of the one line it comes from. A decision is a choice between ways to build the program, not a task done. Leave out what the input does not give a reason for. When the input holds no design decision, reply with an empty list.

Answer from the input below: each line is `[id] text`. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else: {"design": [{"text": string, "cite": string}]}, where cite is an id exactly as in the input, without the brackets.
"#
);

/// The writer's instructions for "ask about this" (danielrispler/Falinks#461, decision 20): a side question on one item
/// of a session, which the session never sees.
const ASK: &str = concat!(
    r#"A developer watches a coding agent work and asks a side question about one item of its session: one of the agent's responses, one of its intentions (the last line of a response, which says what it does next and why, or the first line of a finished turn's final response, which says what it did), or a card that explains a change it made. The agent never sees the question or your answer. Answer so that he understands the agent's work.

Answer from the input below: the intentions of the item's turn, oldest first, the item itself with what the studio recorded about it, and the question. Use only this input: do not use tools or read files. Build the answer on the recorded facts, and call what you infer from them an inference. When the input does not answer the question, say so, and say what would.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else: {"answer": string}, the answer in at most three short paragraphs.
"#
);

#[cfg(test)]
mod reply_tests {
    use super::{once_more, parse_object};

    #[test]
    fn a_reply_in_a_code_fence_parses_to_its_object() {
        let value = parse_object("Here:\n```json\n{\"goal\": \"x\"}\n```").unwrap();
        assert_eq!(value["goal"], "x");
        assert!(parse_object("no object here").is_err());
    }

    #[test]
    fn a_first_result_that_can_stand_is_not_asked_again() {
        let mut asked = false;
        let got = once_more(
            Ok::<_, String>(1),
            |_| false,
            || {
                asked = true;
                Ok(2)
            },
            |second, _| second,
        );
        assert_eq!((got, asked), (Ok(1), false));
    }

    #[test]
    fn the_picked_result_stands_after_a_second_ask() {
        let keep_first_when_second_fails = once_more(
            Ok::<_, String>(1),
            |_| true,
            || Err("no".into()),
            |second, first| second.or(first),
        );
        assert_eq!(keep_first_when_second_fails, Ok(1));
        let second_stands = once_more(
            Err::<i32, _>("a".to_string()),
            |_| true,
            || Err("b".into()),
            |second, _| second,
        );
        assert_eq!(second_stands, Err("b".into()));
    }
}

/// How long a call of `kind` asked to stop after `asked` runs: never less than the writer's floor, so a logged-out
/// `agy` reports its login failure instead of being killed as a timeout.
fn stop_after(kind: WriterKind, asked: Duration) -> Duration {
    asked.max(kind.floor())
}

#[cfg(test)]
mod call_tests {
    use super::{CardWriter, ClaudeWriter, WriterKind, stop_after};
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;

    /// A call is read by the writer its command was built for, even when the settings switch writers in between.
    #[test]
    fn a_call_is_read_by_the_writer_it_was_built_for() {
        let dir = tempfile::tempdir().unwrap();
        let agy = dir.path().join("agy");
        std::fs::write(
            &agy,
            "#!/bin/sh\necho '{\"status\":\"SUCCESS\",\"response\":\"plain words\"}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&agy, std::fs::Permissions::from_mode(0o755)).unwrap();
        let writer = ClaudeWriter::new(dir.path().join("claude"), dir.path().into())
            .with_agy(agy, dir.path().into())
            .with_kind(WriterKind::Agy);

        let run = writer.goal_command(&["a prompt"]);
        writer.use_writer(WriterKind::Claude);

        assert_eq!(writer.stdout(run), Ok("plain words".into()));
    }

    /// The risk judge's 15 seconds are too short for a logged-out `agy`, which waits a minute before it says so.
    #[test]
    fn a_short_call_of_agy_is_given_its_floor_and_claudes_is_kept() {
        let fifteen = Duration::from_secs(15);
        assert_eq!(
            stop_after(WriterKind::Agy, fifteen),
            WriterKind::Agy.floor()
        );
        assert_eq!(stop_after(WriterKind::Claude, fifteen), fifteen);
        let long = Duration::from_secs(180);
        assert_eq!(stop_after(WriterKind::Agy, long), long);
    }
}
