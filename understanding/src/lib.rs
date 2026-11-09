//! The understanding layer's I/O, which the core leaves to the shell: the card writer (#328,
//! module 4), which also writes the briefs (#369, decision 16), the worktree fingerprint (module 5),
//! the card store with its briefs and its weekly rates (module 6), and the drift in Claude Code
//! (#369, decisions 7, 9 and 23).
use serde::{Deserialize, Serialize};
use std::{
    hash::{BuildHasher, Hasher, RandomState},
    io::{Read, Write},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use studio_core::{CardRequest, Kind, Session, Status, Transcript};

mod drift;
mod fingerprint;
mod store;
mod week;
pub use drift::{Drift, Versions, drift};
pub use fingerprint::{Fingerprints, backfilled, fingerprint_changes, worktree_root};
pub use store::{
    Brief, CardStore, CheckResult, Guess, Level, Rating, Reason, Row, SessionCards, StoredCard,
    millis,
};
pub use week::{Rate, Ratings, Week, week};

/// A card on a change, in the explanation language; code inside its text is in
/// backticks. It opens with its quiz; the rest shows when Daniel taps an option.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Card {
    /// What the change does and why. Cards stored before #375 call it `answer`.
    #[serde(alias = "answer")]
    pub explanation: String,
    // The fields below default to empty, so cards stored before they existed still load.
    /// What the change did, in one line: the session's row while its turn is over, and the brief's
    /// rewrite (#369, decisions 16 and 19).
    #[serde(default)]
    pub summary: String,
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
    /// How the change alters the code's structure, when it does (#369, decision 12); `None` when the
    /// writer answered `structural: none`.
    #[serde(default)]
    pub architecture: Option<String>,
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
    /// The quiz in `value`, its options shuffled with the key and reasons moving with them, as the
    /// writer places the right one unevenly (#434, decision 36); or why it is unusable: it does not
    /// parse, or its key names none of two or more options.
    fn from_value(value: serde_json::Value) -> Result<Quiz, String> {
        let mut quiz: Quiz = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let options = quiz.options.len();
        if options < 2 || quiz.key >= options {
            return Err(format!("key {} of {options} options", quiz.key));
        }
        let right = quiz.options[quiz.key].clone();
        // Fisher-Yates, on the random keys std gives each hasher.
        for last in (1..options).rev() {
            let random = RandomState::new().build_hasher().finish();
            quiz.options
                .swap(last, (random % (last as u64 + 1)) as usize);
        }
        quiz.key = quiz.options.iter().position(|o| *o == right).unwrap();
        Ok(quiz)
    }
}

impl Card {
    /// The card in a writer's reply, which must hold one JSON object, perhaps in a code fence. When
    /// it does not parse, the error is one line for the log. A card whose quiz is missing or
    /// unusable keeps its explanation, with one line on why it has no quiz.
    pub fn from_reply(reply: &str) -> Result<(Card, Option<String>), String> {
        let fail = |error: serde_json::Error| {
            let head: String = reply.chars().take(300).collect();
            format!("the card writer's reply does not parse ({error}): {head:?}")
        };
        let mut value: serde_json::Value = serde_json::from_str(object(reply)).map_err(fail)?;
        let quiz = value.get_mut("quiz").map(serde_json::Value::take);
        let mut card: Card = serde_json::from_value(value).map_err(fail)?;
        let structural = |section: &String| !["", NOT_STRUCTURAL].contains(&section.trim());
        card.architecture = card.architecture.filter(structural);
        match Quiz::from_value(quiz.unwrap_or_default()) {
            Ok(quiz) => {
                card.quiz = Some(quiz);
                Ok((card, None))
            }
            Err(error) => Ok((card, Some(format!("its quiz is unusable ({error})")))),
        }
    }
}

/// Writes the card on a session's change since its last card, and its brief.
pub trait CardWriter {
    /// The writer's reply on `request`'s change, or one line for the log when it gave none. It reads
    /// the change with `earlier`, the summaries of the earlier cards of the prompt the change belongs
    /// to, oldest first, and `brief`, the session's brief so far, which it updates (#369, decision
    /// 12).
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String>;

    /// The writer's longer explanation of `card` on `request`'s change, for "explain more" (#369,
    /// decision 15), or one line for the log when it gave none.
    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String>;

    /// The writer's reply with a session's brief rewritten from scratch (#369, decision 16), or one
    /// line for the log when it gave none. It reads only `prompts`, Daniel's, `summaries`, those of
    /// the session's cards, both oldest first, and `diffstat`, `git diff --stat` since the session
    /// began.
    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String>;

    /// The writer's reply with `brief` updated from `workers`, what a session's workers did since its
    /// last card, from their cards' summaries (#369, decision 11), or one line for the log when it
    /// gave none.
    fn update(&self, brief: &str, workers: &str) -> Result<String, String>;

    /// The writer's reply to Daniel's side `question` on `item`, a response, card or intention, read
    /// with `intentions`, those of the item's turn, oldest first (#461, decision 20), or one line
    /// when it gave none. A writer that answers no questions says so.
    fn ask(&self, _item: &str, _intentions: &[String], _question: &str) -> Result<String, String> {
        Err("this writer answers no questions".into())
    }
}

/// Every 5th update of a brief rewrites it from scratch (#369, decision 16), a starting guess.
const REWRITE_EVERY: u32 = 5;

/// The card on a session's or a worker's change and its brief (#369, decisions 11, 12 and 16), from
/// `session`'s state. `writer` writes the card on `request` with the summaries of the earlier cards
/// of its prompt and the brief of its owner, the worker or else the session, which the same reply
/// updates, and both go in `store`. A card with no change is a session's that changed nothing
/// itself: it is written from its workers' card summaries instead (`workers`). Every 5th update then
/// rewrites the brief from scratch (`rebrief`), after `stored` runs on the card stored, so the
/// shell sends the list and the "New card" notice first (#434, decision 32). The store is not
/// locked while the writer runs, which takes seconds. Each problem is one line for `log`. Returns
/// whether a card was added.
pub fn write_stop(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    mut request: CardRequest,
    session: &Session,
    stored: impl FnOnce(),
    mut log: impl FnMut(String),
) -> bool {
    let owner = request.owner().to_owned();
    let (earlier, brief, workers) = {
        let store = store.lock().unwrap();
        let cards = store
            .cards()
            .iter()
            .filter(|kept| kept.request.owner() == owner);
        let same_prompt =
            cards.filter(|kept| request.prompt.is_some() && kept.request.prompt == request.prompt);
        let earlier: Vec<String> = same_prompt.filter_map(summary).collect();
        let brief = store.brief(&owner).cloned().unwrap_or_default();
        (earlier, brief, workers(&store, &request, session))
    };
    if request.change.is_none() {
        let Some(workers) = workers else {
            log(format!(
                "no card on {}: its workers' cards are not in the store",
                request.id
            ));
            return false;
        };
        request.change = Some(workers);
    }
    let Some((card, updated)) = write_card(writer, &request, &earlier, &brief.text, &mut log)
    else {
        return false;
    };
    if let Err(e) = store.lock().unwrap().add(request.clone(), card, millis()) {
        log(format!("save the card store: {e}"));
    }
    stored();
    rebrief(
        store,
        writer,
        &request,
        brief.updates,
        updated,
        session,
        log,
    );
    true
}

/// A session's brief updated from its workers' cards since its last card, `request.workers`, when a
/// turn a wake-up opened changed nothing itself: no card (#369, decision 11). Every 5th update
/// rewrites it from scratch (`rebrief`). Each problem is one line for `log`. Returns whether the
/// brief changed.
pub fn write_brief(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    request: CardRequest,
    session: &Session,
    mut log: impl FnMut(String),
) -> bool {
    let (brief, workers) = {
        let store = store.lock().unwrap();
        let brief = store.brief(&session.agent.id).cloned().unwrap_or_default();
        (brief, workers(&store, &request, session))
    };
    let reply = workers
        .ok_or("its workers' cards are not in the store".to_owned())
        .and_then(|workers| writer.update(&brief.text, &workers));
    match reply.and_then(|reply| brief_in(&reply)) {
        Ok(updated) => {
            rebrief(
                store,
                writer,
                &request,
                brief.updates,
                Some(updated),
                session,
                log,
            );
            true
        }
        Err(line) => {
            log(format!("no brief on {}: {line}", request.id));
            false
        }
    }
}

/// What the workers of `session` did since its last card, for its card or its brief, from the
/// summaries of their cards among `request.workers`, not their diffs: each worker that has one of
/// those cards or still runs, with its task, its status and those summaries. `None` when none of
/// those cards is in the store.
fn workers(store: &CardStore, request: &CardRequest, session: &Session) -> Option<String> {
    let cards: Vec<&StoredCard> = store
        .cards()
        .iter()
        .filter(|kept| request.workers.contains(&kept.request.id))
        .collect();
    if cards.is_empty() {
        return None;
    }
    let lines = session.agents.iter().filter_map(|agent| {
        let of = cards
            .iter()
            .filter(|kept| kept.request.owner() == agent.agent.id);
        // A card with no summary adds nothing (#434, decision 35).
        let summaries: Vec<String> = of.filter_map(|kept| summary(kept)).collect();
        let running = agent.status == Status::Running && agent.kind == Kind::Worker;
        let name = &agent.name;
        let did = match &summaries[..] {
            [] if running => "no card yet".into(),
            [] => return None,
            summaries => summaries.join(" "),
        };
        Some(format!("- `{name}` ({}): {did}\n", agent.status))
    });
    Some(format!("{WORKERS}{}", lines.collect::<String>()))
}

/// The head of a change built from workers' cards, which the writer's instructions name.
const WORKERS: &str = "No diff: the agent changed nothing itself since its last card. Its workers, \
                       each with its status and the summaries of its cards since then:\n";

/// Sets the brief of `request`'s owner, its worker or else `session`, to `updated`, the brief after
/// its `updates` updates so far updated once more. Every 5th update first rewrites it from scratch
/// from the owner's prompts (a session's with its wake-ups left out, a worker's spawn), the
/// summaries of its cards (a session's with its workers'), and `git diff --stat` in its folder since
/// its first card's base: never the raw diff. A failed rewrite keeps `updated`.
fn rebrief(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    request: &CardRequest,
    updates: u32,
    mut updated: Option<String>,
    session: &Session,
    mut log: impl FnMut(String),
) {
    let (owner, agent) = (request.owner(), request.agent.as_deref());
    if (updates + 1).is_multiple_of(REWRITE_EVERY) {
        let read = |kept: &&StoredCard| match agent {
            Some(_) => kept.request.agent.as_deref() == agent,
            None => kept.request.session_id == session.agent.id,
        };
        let (summaries, since) = {
            let store = store.lock().unwrap();
            let cards = store.cards().iter();
            let summaries: Vec<String> = cards.clone().filter(read).filter_map(summary).collect();
            let mut own = cards.filter(|kept| kept.request.owner() == owner);
            (
                summaries,
                own.find_map(|kept| kept.request.base.head.clone()),
            )
        };
        let worker = agent.and_then(|id| session.subagent(id));
        let folder = worker
            .and_then(|a| a.folder.as_deref())
            .unwrap_or(&session.cwd);
        let diffstat = since.map_or_else(String::new, |head| {
            fingerprint::diff_stat(Path::new(folder), &head)
        });
        let prompts: Vec<&str> = match worker {
            Some(worker) => vec![&worker.prompt],
            None => {
                let daniels = session.prompts.iter();
                let daniels = daniels.filter(|p| p.sender.is_daniels());
                daniels.map(|p| p.text.as_str()).collect()
            }
        };
        let rewritten = writer.rewrite(&prompts, &summaries, &diffstat);
        match rewritten.and_then(|reply| brief_in(&reply)) {
            Ok(brief) => updated = Some(brief),
            Err(line) => log(format!("no brief rewrite on {}: {line}", request.id)),
        }
    }
    if let Some(text) = updated
        && let Err(e) = store.lock().unwrap().set_brief(owner, text)
    {
        log(format!("save the card store: {e}"));
    }
}

/// The answer to Daniel's side `question` on `item` (#461, decision 20), from one call to `writer`
/// with the intentions of `transcript`'s turn, one per response. It stays in the studio: the session
/// never sees it. Why there is none when the call failed or its reply holds no answer.
pub fn ask_about(
    writer: &(impl CardWriter + ?Sized),
    transcript: &Transcript,
    item: &str,
    question: &str,
) -> Result<String, String> {
    let intentions: Vec<String> = transcript
        .responses
        .iter()
        .filter_map(|text| text.lines().map(str::trim).rfind(|l| !l.is_empty()))
        .map(Into::into)
        .collect();
    let reply = writer.ask(item, &intentions, question)?;
    let value: serde_json::Value = serde_json::from_str(object(&reply)).unwrap_or_default();
    match value["answer"].as_str().map(str::trim) {
        Some(answer) if !answer.is_empty() => Ok(answer.into()),
        _ => Err("the writer's reply holds no answer".into()),
    }
}

/// The JSON object in a writer's reply, perhaps in a code fence: from its first `{` to its last `}`.
fn object(reply: &str) -> &str {
    let object = reply.find('{').zip(reply.rfind('}'));
    let object = object.filter(|(start, end)| start < end);
    object.map_or("", |(start, end)| &reply[start..=end])
}

/// Why a reply gave no brief, for the log.
const HOLDS_NONE: &str = "the reply holds none";

/// A stored card's summary, when it has one.
fn summary(kept: &StoredCard) -> Option<String> {
    Some(kept.card.summary.clone()).filter(|s| !s.is_empty())
}

/// A brief as the writer reads it: "none yet" before its first update.
fn so_far(brief: &str) -> &str {
    if brief.is_empty() { "none yet" } else { brief }
}

/// The brief in a writer's reply, when it holds one that is not empty, or else why not, for the log.
fn brief_in(reply: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(object(reply)).unwrap_or_default();
    match value["brief"].as_str().map(str::trim) {
        Some(brief) if !brief.is_empty() => Ok(brief.into()),
        _ => Err(HOLDS_NONE.into()),
    }
}

/// The longer explanation of card `id` in `store`: the one kept, or else one more call to `writer`,
/// whose text is kept with the card (#369, decision 15). The store is not locked while the writer
/// runs, which takes seconds. A writer that gives no text logs one line to `log`, and nothing is kept.
pub fn explain_more(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    id: &str,
    mut log: impl FnMut(String),
) -> Option<String> {
    // The lock ends with the statement.
    let kept = store
        .lock()
        .unwrap()
        .cards()
        .iter()
        .find(|kept| kept.request.id == id)
        .cloned()?;
    if kept.more.is_some() {
        return kept.more;
    }
    let more = writer
        .more(&kept.request, &kept.card)
        .and_then(|text| match text.trim() {
            "" => Err("the writer's reply is empty".to_owned()),
            text => Ok(text.to_owned()),
        });
    match more {
        Ok(more) => {
            if let Err(e) = store.lock().unwrap().more(id, more.clone()) {
                log(format!("save the card store: {e}"));
            }
            Some(more)
        }
        Err(line) => {
            log(format!("no longer explanation on {id}: {line}"));
            None
        }
    }
}

/// The card on `request` from `writer`'s reply, which is asked once more when it gives no card
/// (#369, decision 12), and `brief` updated with it. The writer reads `earlier`, the summaries of the
/// earlier cards of the prompt `request` belongs to. Each problem is one line for `log`: no card
/// after the retry, or a card with no quiz or no brief, which is kept without asking again.
pub fn write_card(
    writer: &(impl CardWriter + ?Sized),
    request: &CardRequest,
    earlier: &[String],
    brief: &str,
    mut log: impl FnMut(String),
) -> Option<(Card, Option<String>)> {
    let card = || {
        let reply = writer.reply(request, earlier, brief)?;
        let (mut card, mut problem) = Card::from_reply(&reply)?;
        // A check's card asks whether the check passes, which its result settles (#461, decision
        // 15), so its quiz is the app's, with the writer's reasons.
        if let Some(check) = &request.check {
            card.quiz = Some(check_quiz(check, &reply));
            problem = None;
        }
        Ok::<_, String>((card, problem, brief_in(&reply).ok()))
    };
    match card().or_else(|_| card()) {
        Ok((card, problem, brief)) => {
            if let Some(problem) = problem {
                log(format!("no quiz on {}: {problem}", request.id));
            }
            if brief.is_none() {
                log(format!("no brief on {}: {HOLDS_NONE}", request.id));
            }
            Some((card, brief))
        }
        Err(line) => {
            log(format!("no card on {}: {line}", request.id));
            None
        }
    }
}

/// The quiz on a check's card: whether `check` passes, with the writer's `if_it_passes` and
/// `if_it_fails` as the reasons, in that order. Its key is "It passes" until the result settles it
/// (`CardStore::settle`).
fn check_quiz(check: &str, reply: &str) -> Quiz {
    let value: serde_json::Value = serde_json::from_str(object(reply)).unwrap_or_default();
    let why = |field: &str| value[field].as_str().unwrap_or_default().to_owned();
    let first = check.lines().next().unwrap_or_default();
    Quiz {
        text: format!("Will `{first}` pass?"),
        options: vec![
            Choice {
                text: "It passes.".into(),
                why: why("if_it_passes"),
            },
            Choice {
                text: "It fails.".into(),
                why: why("if_it_fails"),
            },
        ],
        key: 0,
    }
}

/// Writes a fixed card that shows the change, and a fixed brief, so the app can run without
/// `claude -p`.
pub struct FakeWriter;

impl CardWriter for FakeWriter {
    fn reply(&self, request: &CardRequest, _: &[String], _: &str) -> Result<String, String> {
        let paths: Vec<&str> = request
            .diff()
            .lines()
            .filter_map(|line| line.strip_prefix("+++ b/"))
            .collect();
        let named = format!("`{}`", paths.join("`, `"));
        // A change to several files gets a section, so the app shows one.
        let architecture = match paths.len() {
            0 | 1 => NOT_STRUCTURAL.into(),
            _ => format!("Sample architecture section: the change touches {named}."),
        };
        let option = |text: &str, why: &str| serde_json::json!({ "text": text, "why": why });
        let card = serde_json::json!({
            "summary": format!("Sample change to {named}"),
            "explanation": "A sample card from the fake writer, which does not read the code. The change itself is below.",
            "paths": paths,
            "code": request.diff(),
            "architecture": architecture,
            "brief": format!("A sample brief from the fake writer.\nThe latest change touched {named}."),
            "why_it_matters": "Why the change matters goes here, with code inside the text such as `if a <= b {`.",
            "not_checked": "Not checked: nothing, this is a sample card.",
            "if_it_passes": "It passes when the change is right: a sample reason from the fake writer.",
            "if_it_fails": "It fails when the change breaks a test: a sample reason from the fake writer.",
            "quiz": {
                "text": format!("What did the agent change in `{}`?", paths.join("`, `")),
                "options": [
                    option("What the code below shows.", "Right: the code below is the change itself."),
                    option("Only the docs, without touching the code.", "No: the fake writer does not know what changed."),
                    option("Nothing, it only ran tests.", "No: a card is written only on a change."),
                ],
                "key": 0,
            },
        });
        Ok(card.to_string())
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Ok("A longer sample explanation from the fake writer, which does not read the code. How the changed code works goes here, step by step, and why it was done this way, with code such as `if a <= b {`.".into())
    }

    fn rewrite(&self, _: &[&str], _: &[String], _: &str) -> Result<String, String> {
        let brief =
            "A sample brief from the fake writer, rewritten.\nIt does not read the session.";
        Ok(serde_json::json!({ "brief": brief }).to_string())
    }

    fn update(&self, brief: &str, _: &str) -> Result<String, String> {
        let brief =
            format!("{brief}\nThe workers came back, and the session itself changed nothing.");
        Ok(serde_json::json!({ "brief": brief }).to_string())
    }

    fn ask(&self, _: &str, intentions: &[String], question: &str) -> Result<String, String> {
        // `STUDIO_FAKE_ASK=fail` makes it fail, so the error and its retry can be seen.
        if std::env::var("STUDIO_FAKE_ASK").as_deref() == Ok("fail") {
            return Ok("Shall I read the file?".into());
        }
        let answer = format!(
            "A sample answer from the fake writer, which does not read the session, to `{question}`. It was given {} intentions of the turn.",
            intentions.len()
        );
        Ok(serde_json::json!({ "answer": answer }).to_string())
    }
}

/// Writes the card with `claude -p` on Daniel's subscription, in a neutral folder that holds none of
/// his projects, so it loads no repository's instructions and has nothing to wander into.
pub struct ClaudeWriter {
    /// The `claude` program, from `find_claude`.
    program: PathBuf,
    folder: PathBuf,
    /// How long a call may run before it fails and is stopped (#434, decision 30).
    timeout: Duration,
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
        let timeout = Duration::from_secs(5 * 60);
        Self {
            program,
            folder,
            timeout,
        }
    }

    /// The writer with each call stopped after `timeout` instead of 5 minutes.
    pub fn with_timeout(self, timeout: Duration) -> Self {
        Self { timeout, ..self }
    }

    /// The writer for event log `log`: its neutral folder, `card-writer` beside the log, made here,
    /// and `claude` as the login shell finds it. An error when the folder cannot be made.
    pub fn beside(log: &Path) -> std::io::Result<Self> {
        let folder = log.with_file_name("card-writer");
        std::fs::create_dir_all(&folder)?;
        let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
        Ok(Self::new(find_claude(shell.as_ref()), folder))
    }

    /// The `claude -p` run for one change's card, read with the summaries of the prompt's `earlier`
    /// cards and the session's `brief` so far, with the prompt it reads on standard input, as are
    /// all the runs below.
    pub fn command(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> (Command, String) {
        let earlier: String = match earlier {
            [] => "none\n".into(),
            summaries => summaries.iter().map(|s| format!("- {s}\n")).collect(),
        };
        let brief = so_far(brief);
        // A check's card is on what the check verifies against the change (#461, decision 15).
        let instructions = match &request.check {
            Some(check) => format!("{CHECK}\nThe check's command:\n{check}\n"),
            None => PROMPT.into(),
        };
        let head = format!(
            "{instructions}\nThe summaries of this prompt's earlier cards, oldest first:\n{earlier}\n\
             The brief so far:\n{brief}\n"
        );
        self.claude(&head, request.diff())
    }

    /// The `claude -p` run that rewrites a session's brief from Daniel's `prompts`, its card
    /// `summaries` and `diffstat`. Past `MAX_CHANGE`, the oldest prompts are cut first, and the
    /// newest is shortened rather than cut; the summaries stay.
    pub fn rewrite_command(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> (Command, String) {
        let summaries: String = summaries.iter().map(|s| format!("- {s}\n")).collect();
        let newest = prompts.last().copied();
        let prompts: Vec<String> = prompts
            .iter()
            .map(|prompt| format!("<prompt>\n{prompt}\n</prompt>\n"))
            .collect();
        let mut size =
            summaries.len() + diffstat.len() + prompts.iter().map(String::len).sum::<usize>();
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
        // The newest prompt alone over the budget is shortened, not dropped (#434, decision 33),
        // keeping its start even when the summaries fill the budget.
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
            "{REWRITE}\nThe developer's prompts, oldest first{note}:\n{kept}\n\
             The card summaries, oldest first:\n{summaries}\n\
             `git diff --stat` since the session began:\n{diffstat}"
        ))
    }

    /// The `claude -p` run that updates a session's `brief` from `workers`, what its workers did.
    pub fn update_command(&self, brief: &str, workers: &str) -> (Command, String) {
        let brief = so_far(brief);
        self.run(&format!(
            "{UPDATE}\nThe brief so far:\n{brief}\n\n{workers}"
        ))
    }

    /// The `claude -p` run for "explain more" on `card`, which the writer reads with its change.
    pub fn more_command(&self, request: &CardRequest, card: &Card) -> (Command, String) {
        let head = format!("{MORE}\nThe card's explanation:\n{}\n", card.explanation);
        self.claude(&head, request.diff())
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

    /// The `claude -p` run on `head`, then `change`, cut at `MAX_CHANGE`.
    fn claude(&self, head: &str, change: &str) -> (Command, String) {
        let kept = &change[..change.floor_char_boundary(MAX_CHANGE)];
        let cut = match change.len() - kept.len() {
            0 => String::new(),
            bytes => format!("\n[{bytes} more bytes cut]"),
        };
        self.run(&format!("{head}\nChange:\n{kept}{cut}"))
    }

    /// The `claude -p` run on `prompt`. Without the two variables and without `--bare`, `claude`
    /// uses the subscription login (#320, "Fact"); the studio never reads that login itself. The
    /// prompt holds untrusted text, a change or Daniel's prompts, so the run has no tools and no MCP
    /// servers, whatever Daniel's settings allow. It also runs without the studio plugin, so its
    /// session sends no hook events, and in safe mode, so none of Daniel's plugins, hooks, skills or
    /// CLAUDE.md reach it (#517, decision 9). The prompt goes on standard input without its NUL bytes, which
    /// `claude` could not take in an argument either (#434, decision 31).
    fn run(&self, prompt: &str) -> (Command, String) {
        let mut command = Command::new(&self.program);
        command
            // `--tools` takes several values, so an option must follow it.
            .args(["--tools", "", "--strict-mcp-config", "--safe-mode", "-p"])
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("ANTHROPIC_AUTH_TOKEN")
            .env_remove("CLAUDE_CODE_PLUGIN_DIRS")
            .current_dir(&self.folder);
        (command, prompt.replace('\0', ""))
    }

    /// What a run printed, or one line for the log when the call failed: it did not start, exited
    /// with an error, printed nothing, or ran past the timeout, which stops it and every process it
    /// started, all in its own process group (#434, decision 30).
    fn stdout(&self, (mut command, prompt): (Command, String)) -> Result<String, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| format!("`claude -p` did not start: {e}"))?;
        let (mut stdin, mut stdout) = (child.stdin.take(), child.stdout.take());
        // Threads, so a call that reads no input or never ends cannot hold back the timeout.
        thread::spawn(move || stdin.as_mut().map(|i| i.write_all(prompt.as_bytes())));
        let (sender, read) = mpsc::channel();
        thread::spawn(move || {
            let mut out = Vec::new();
            let _ = stdout.as_mut().map(|o| o.read_to_end(&mut out));
            let _ = sender.send(out);
        });
        let deadline = Instant::now() + self.timeout;
        let out = read.recv_timeout(self.timeout).ok();
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
            return Err(format!(
                "`claude -p` ran past {:?} and was stopped",
                self.timeout
            ));
        };
        let out = String::from_utf8_lossy(&out);
        let head: String = out.trim().chars().take(300).collect();
        match (status.success(), head.is_empty()) {
            (false, _) => Err(format!("`claude -p` failed ({status}): {head:?}")),
            (true, true) => Err("`claude -p` printed nothing".into()),
            (true, false) => Ok(out.into()),
        }
    }
}

impl CardWriter for ClaudeWriter {
    /// Runs `claude -p`, which takes seconds, so the shell calls it off its log thread.
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        self.stdout(self.command(request, earlier, brief))
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        self.stdout(self.more_command(request, card))
    }

    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String> {
        self.stdout(self.rewrite_command(prompts, summaries, diffstat))
    }

    fn update(&self, brief: &str, workers: &str) -> Result<String, String> {
        self.stdout(self.update_command(brief, workers))
    }

    fn ask(&self, item: &str, intentions: &[String], question: &str) -> Result<String, String> {
        self.stdout(self.ask_command(item, intentions, question))
    }
}

/// The writer's architecture section on a change that is not structural.
const NOT_STRUCTURAL: &str = "structural: none";

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

/// The card writer's instructions. Its input is the change, the summaries of the prompt's earlier
/// cards and the brief, not the payloads (v0 note, proposal 8).
const PROMPT: &str = concat!(
    r#"You write one card that teaches a developer about the change a coding agent made to his code while he waited on it, since the previous card. The card opens with a multiple-choice question on the change's logic or design. He taps an option, and the card shows at once whether he was right, why each option is right or wrong, and the explanation. You also update the session's brief, which he reads to resume the session.

Answer from the input below: the summaries of the earlier cards on the same prompt of his, the brief so far, and the diff. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string, "explanation": string, "paths": [string], "code": string, "why_it_matters": string, "not_checked": string, "architecture": string, "quiz": {"text": string, "options": [{"text": string, "why": string}], "key": number}, "brief": string}
- summary: what the change did, in one short line.
- explanation: what the change does and why, in at most three sentences.
- paths: the files the change touched.
- code: the lines of the change that matter most, copied from it, at most 12.
- why_it_matters: what the change does for the code, in one or two sentences.
- not_checked: one thing the diff does not show.
- architecture: when the change is structural, a short section on how it changes the code's structure, before and after, and why: modules, types, interfaces or files added, removed, moved or split, or a change in which parts call or depend on which. Read it with the earlier cards' summaries, so that a structural change spread over several cards counts. Otherwise exactly "structural: none".
- quiz.text: one question on the change's logic or design that the diff answers, such as why the code works this way, what a line does, or what would break without it. Never ask whether a command, test or check will pass.
- quiz.options: 3 or 4 options. Each wrong option is something the code could plausibly have done instead, of the right option's length and tone, so the wording gives nothing away.
- quiz.options[].why: one line on why that option is right or wrong. The options are shown in another order, so never refer to an option by its letter or place.
- quiz.key: the place of the right option in quiz.options, counting from 0.
- brief: the brief so far, updated with this change: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Keep what still holds. Its last line says what this change did.

When the change is not a diff but the agent's workers with the summaries of their cards, the agent changed nothing itself and handed the work to them: the card explains what it split up among them, what came back and what is still open, paths and code are empty, and the quiz is on how the work was split.
"#
);

/// The card writer's instructions for a check's card (#461, decision 15), written while the check
/// runs: on what the check verifies against the change so far. The app asks whether it passes.
const CHECK: &str = concat!(
    r#"A coding agent just started a check, a command that tests or verifies its code, which usually runs for minutes. You write one card that teaches the developer who waits on it what this check verifies against the change the agent made since the previous card, so he can guess whether it will pass. The card asks him "Will it pass?"; once the check ends, it shows whether he was right and your two reasons. You also update the session's brief, which he reads to resume the session.

Answer from the input below: the check's command, the summaries of the earlier cards on the same prompt of his, the brief so far, and the diff. Use only this input: do not use tools or read files. You do not know the check's result.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string, "explanation": string, "paths": [string], "code": string, "why_it_matters": string, "not_checked": string, "architecture": string, "if_it_passes": string, "if_it_fails": string, "brief": string}
- summary: what the change did, in one short line.
- explanation: what the check runs and what of the change it verifies, in at most three sentences.
- paths: the files the change touched.
- code: the lines of the change the check exercises most, copied from it, at most 12.
- why_it_matters: what the check's result tells about the change, in one or two sentences.
- not_checked: one thing in the change the check does not verify.
- architecture: when the change is structural, a short section on how it changes the code's structure, before and after, and why. Otherwise exactly "structural: none".
- if_it_passes: one line on what a pass would show about the change.
- if_it_fails: one line on the likeliest reason in the change it could fail.
- brief: the brief so far, updated with this change: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Keep what still holds. Its last line says what this change did.
"#
);

/// The writer's instructions for a brief updated from a session's workers' cards when it changed
/// nothing itself (#369, decision 11): no card, so a worker's return adds no thin card.
const UPDATE: &str = concat!(
    r#"You update the brief of a coding agent's session, which the developer who runs it reads to resume the session: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Since the brief, the agent changed nothing itself; its workers did.

Answer from the input below: the brief so far, and the agent's workers, each with its status and the summaries of its cards since then. Use only this input: do not use tools or read files. Keep what still holds. Its last line says what came back.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else: {"brief": string}
"#
);

/// The writer's instructions for a brief rewritten from scratch (#369, decision 16): from Daniel's
/// prompts, the card summaries and the diffstat, never a diff, so errors in the rolling brief do not
/// compound.
const REWRITE: &str = concat!(
    r#"You rewrite from scratch the brief of a coding agent's session, which the developer who runs it reads to resume the session: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Its last line says what the latest change did.

Answer from the input below: the developer's prompts, the one-line summaries of the session's cards, and `git diff --stat` since the session began. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else: {"brief": string}
"#
);

/// The writer's instructions for "explain more" (#369, decision 15): one more call on the card's
/// change, which also reads the card's explanation, so it goes deeper instead of repeating it.
const MORE: &str = concat!(
    r#"A developer read a card that explains a change a coding agent made to his code, and asked for more. Explain the change in more depth than the card: how the changed code works, step by step, and why it was done this way and not another. Do not repeat the card.

Answer from the change below. Use only this input: do not use tools or read files.

When the change is not a diff but the agent's workers with the summaries of their cards, the agent changed nothing itself and handed the work to them: explain how the work was split and what came back, and do not invent code you were not shown.

"#,
    explanation_language!(),
    r#"

Reply with the explanation only, in at most three short paragraphs: no JSON and no heading.
"#
);

/// The writer's instructions for "ask about this" (#461, decision 20): a side question on one item
/// of a session, which the session never sees.
const ASK: &str = concat!(
    r#"A developer watches a coding agent work and asks a side question about one item of its session: one of the agent's responses, one of its intentions (the last line of a response, which says what it does next and why), or a card that explains a change it made. The agent never sees the question or your answer. Answer so that he understands the agent's work.

Answer from the input below: the intentions of the item's turn, oldest first, the item itself, and the question. Use only this input: do not use tools or read files. When the input does not answer the question, say so, and say what would.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else: {"answer": string}, the answer in at most three short paragraphs.
"#
);
