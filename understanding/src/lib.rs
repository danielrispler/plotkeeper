//! The understanding layer's I/O, which the core leaves to the shell: the card writer (#328,
//! module 4), the worktree fingerprint (module 5), and the card store and its weekly rates (module 6).
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use studio_core::{CardRequest, Kind, Sender, Session};

mod drift;
mod fingerprint;
mod store;
mod week;
pub use drift::{Drift, Versions, drift};
pub use fingerprint::Fingerprints;
pub use store::{
    Brief, CardStore, Guess, Level, Rating, Reason, Row, SessionCards, StoredCard, millis,
};
pub use week::{Rate, Ratings, Week, week};

/// A card on a change, in Hebrew; code stays in English, and code inside the Hebrew text is in
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
}

/// Every 5th update of a brief rewrites it from scratch (#369, decision 16), a starting guess.
const REWRITE_EVERY: u32 = 5;

/// The card on a session's or a worker's change and its brief (#369, decisions 11, 12 and 16), from
/// `session`'s state. `writer` writes the card on `request` with the summaries of the earlier cards
/// of its prompt and the brief of its owner, the worker or else the session, which the same reply
/// updates, and both go in `store`. A card with no change is a session's that changed nothing
/// itself: it is written from its workers' card summaries instead (`workers`). Every 5th update then
/// rewrites the brief from scratch (`rebrief`). The store is not locked while the writer runs,
/// which takes seconds. Each problem is one line for `log`. Returns whether a card was added.
pub fn write_stop(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    mut request: CardRequest,
    session: &Session,
    mut log: impl FnMut(String),
) -> bool {
    let owner = request.owner().to_owned();
    let summary = |kept: &StoredCard| Some(kept.card.summary.clone()).filter(|s| !s.is_empty());
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
    if request.change.is_empty() {
        let Some(workers) = workers else {
            log(format!(
                "no card on {}: its workers' cards are not in the store",
                request.id
            ));
            return false;
        };
        request.change = workers;
    }
    let Some((card, updated)) = write_card(writer, &request, &earlier, &brief.text, &mut log)
    else {
        return false;
    };
    if let Err(e) = store.lock().unwrap().add(request.clone(), card, millis()) {
        log(format!("save the card store: {e}"));
    }
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
        let brief = store.brief(&session.id).cloned().unwrap_or_default();
        (brief, workers(&store, &request, session))
    };
    let reply = workers
        .ok_or("its workers' cards are not in the store".to_owned())
        .and_then(|workers| writer.update(&brief.text, &workers));
    match reply.and_then(|reply| brief_in(&reply).ok_or("the reply holds none".into())) {
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
        let of = cards.iter().filter(|kept| kept.request.owner() == agent.id);
        let summaries: Vec<&str> = of.map(|kept| kept.card.summary.as_str()).collect();
        let running = agent.status == "running" && agent.kind == Kind::Worker;
        let name = agent.description.as_deref().unwrap_or(&agent.id);
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
        let summary = |kept: &StoredCard| Some(kept.card.summary.clone()).filter(|s| !s.is_empty());
        let read = |kept: &&StoredCard| match agent {
            Some(_) => kept.request.agent.as_deref() == agent,
            None => kept.request.session_id == session.id,
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
        let worker = session.agents.iter().find(|a| Some(&*a.id) == agent);
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
                let daniels = daniels.filter(|p| !matches!(p.sender, Sender::WakeUp(_)));
                daniels.map(|p| p.text.as_str()).collect()
            }
        };
        let rewritten = writer.rewrite(&prompts, &summaries, &diffstat);
        match rewritten.and_then(|reply| brief_in(&reply).ok_or("the reply holds none".into())) {
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

/// The JSON object in a writer's reply, perhaps in a code fence: from its first `{` to its last `}`.
fn object(reply: &str) -> &str {
    let object = reply.find('{').zip(reply.rfind('}'));
    let object = object.filter(|(start, end)| start < end);
    object.map_or("", |(start, end)| &reply[start..=end])
}

/// The brief in a writer's reply, when it holds one that is not empty.
fn brief_in(reply: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(object(reply)).ok()?;
    let brief = value["brief"].as_str()?.trim();
    (!brief.is_empty()).then(|| brief.into())
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
        Card::from_reply(&reply).map(|(card, problem)| (card, problem, brief_in(&reply)))
    };
    match card().or_else(|_| card()) {
        Ok((card, problem, brief)) => {
            if let Some(problem) = problem {
                log(format!("no quiz on {}: {problem}", request.id));
            }
            if brief.is_none() {
                log(format!("no brief on {}: the reply holds none", request.id));
            }
            Some((card, brief))
        }
        Err(line) => {
            log(format!("no card on {}: {line}", request.id));
            None
        }
    }
}

/// Writes a fixed Hebrew card that shows the change, and a fixed brief, so the app can run without
/// `claude -p`.
pub struct FakeWriter;

impl CardWriter for FakeWriter {
    fn reply(&self, request: &CardRequest, _: &[String], _: &str) -> Result<String, String> {
        let paths: Vec<&str> = request
            .change
            .lines()
            .filter_map(|line| line.strip_prefix("+++ b/"))
            .collect();
        let named = format!("`{}`", paths.join("`, `"));
        // A change to several files gets a section, so the app shows one.
        let architecture = match paths.len() {
            0 | 1 => NOT_STRUCTURAL.into(),
            _ => format!("סעיף ארכיטקטורה לדוגמה: השינוי נוגע ב־{named}."),
        };
        let option = |text: &str, why: &str| serde_json::json!({ "text": text, "why": why });
        let card = serde_json::json!({
            "summary": format!("שינוי לדוגמה ב־{named}"),
            "explanation": "כרטיס לדוגמה של הכותב המזויף, שלא קורא את הקוד. השינוי עצמו למטה.",
            "paths": paths,
            "code": request.change,
            "architecture": architecture,
            "brief": format!("תקציר לדוגמה של הכותב המזויף.\nהשינוי האחרון נגע ב־{named}."),
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

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Ok("הסבר ארוך יותר לדוגמה של הכותב המזויף, שלא קורא את הקוד. כאן יופיע איך הקוד שהשתנה עובד, צעד אחר צעד, ולמה נבחרה הדרך הזו, עם קוד כמו `if a <= b {`.".into())
    }

    fn rewrite(&self, _: &[&str], _: &[String], _: &str) -> Result<String, String> {
        let brief = "תקציר לדוגמה של הכותב המזויף, שנכתב מחדש.\nהוא לא קורא את הסשן.";
        Ok(serde_json::json!({ "brief": brief }).to_string())
    }

    fn update(&self, brief: &str, _: &str) -> Result<String, String> {
        let brief = format!("{brief}\nהעובדים חזרו, והסשן עצמו לא שינה דבר.");
        Ok(serde_json::json!({ "brief": brief }).to_string())
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

    /// The `claude -p` run for one change's card, read with the summaries of the prompt's `earlier`
    /// cards and the session's `brief` so far.
    pub fn command(&self, request: &CardRequest, earlier: &[String], brief: &str) -> Command {
        let earlier: String = match earlier {
            [] => "none\n".into(),
            summaries => summaries.iter().map(|s| format!("- {s}\n")).collect(),
        };
        let brief = if brief.is_empty() { "none yet" } else { brief };
        let head = format!(
            "{PROMPT}\nThe summaries of this prompt's earlier cards, oldest first:\n{earlier}\n\
             The brief so far:\n{brief}\n"
        );
        self.claude(&head, &request.change)
    }

    /// The `claude -p` run that rewrites a session's brief from Daniel's `prompts`, its card
    /// `summaries` and `diffstat`. Past `MAX_CHANGE`, the oldest prompts are cut first; the
    /// summaries stay.
    pub fn rewrite_command(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Command {
        let summaries: String = summaries.iter().map(|s| format!("- {s}\n")).collect();
        let prompts: Vec<String> = prompts
            .iter()
            .map(|prompt| format!("<prompt>\n{prompt}\n</prompt>\n"))
            .collect();
        let mut size =
            summaries.len() + diffstat.len() + prompts.iter().map(String::len).sum::<usize>();
        let mut cut = 0;
        while size > MAX_CHANGE && cut < prompts.len() {
            size -= prompts[cut].len();
            cut += 1;
        }
        let note = match cut {
            0 => String::new(),
            cut => format!(" (the {cut} oldest cut)"),
        };
        let kept = prompts[cut..].concat();
        self.run(&format!(
            "{REWRITE}\nThe developer's prompts, oldest first{note}:\n{kept}\n\
             The card summaries, oldest first:\n{summaries}\n\
             `git diff --stat` since the session began:\n{diffstat}"
        ))
    }

    /// The `claude -p` run that updates a session's `brief` from `workers`, what its workers did.
    pub fn update_command(&self, brief: &str, workers: &str) -> Command {
        let brief = if brief.is_empty() { "none yet" } else { brief };
        self.run(&format!(
            "{UPDATE}\nThe brief so far:\n{brief}\n\n{workers}"
        ))
    }

    /// The `claude -p` run for "explain more" on `card`, which the writer reads with its change.
    pub fn more_command(&self, request: &CardRequest, card: &Card) -> Command {
        let head = format!("{MORE}\nThe card's explanation:\n{}\n", card.explanation);
        self.claude(&head, &request.change)
    }

    /// The `claude -p` run on `head`, then `change`, cut at `MAX_CHANGE`.
    fn claude(&self, head: &str, change: &str) -> Command {
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
    /// session sends no hook events.
    fn run(&self, prompt: &str) -> Command {
        let mut command = Command::new(&self.program);
        command
            // `--tools` takes several values, so an option must follow it.
            .args(["--tools", "", "--strict-mcp-config", "-p", prompt])
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
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        stdout(self.command(request, earlier, brief))
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        stdout(self.more_command(request, card))
    }

    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String> {
        stdout(self.rewrite_command(prompts, summaries, diffstat))
    }

    fn update(&self, brief: &str, workers: &str) -> Result<String, String> {
        stdout(self.update_command(brief, workers))
    }
}

/// What `command` printed, or one line when it did not start.
fn stdout(mut command: Command) -> Result<String, String> {
    match command.output() {
        Ok(output) => Ok(String::from_utf8_lossy(&output.stdout).into()),
        Err(error) => Err(format!("`claude -p` did not start: {error}")),
    }
}

/// The writer's architecture section on a change that is not structural.
const NOT_STRUCTURAL: &str = "structural: none";

/// The most of a change the writer gets, and of a brief rewrite's input: 64 KiB. The lines that
/// matter fit, a huge new file or log costs no more, and the prompt stays far under macOS's 1 MiB
/// limit on a command's arguments.
const MAX_CHANGE: usize = 64 << 10;

/// The card writer's instructions. Its input is the change, the summaries of the prompt's earlier
/// cards and the brief, not the payloads (v0 note, proposal 8).
const PROMPT: &str = r#"You write one card that teaches a developer about the change a coding agent made to his code while he waited on it, since the previous card. The card opens with a multiple-choice question on the change's logic or design. He taps an option, and the card shows at once whether he was right, why each option is right or wrong, and the explanation. You also update the session's brief, which he reads to resume the session.

Answer from the input below: the summaries of the earlier cards on the same prompt of his, the brief so far, and the diff. Use only this input: do not use tools or read files.

Write every text field in Hebrew, in plain words. Code, code terms, file paths and commands stay in English; inside Hebrew text, wrap each of them in backticks.

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
- quiz.options: 3 or 4 options, the right one at a random place. Each wrong option is something the code could plausibly have done instead, of the right option's length and tone, so the wording gives nothing away.
- quiz.options[].why: one line on why that option is right or wrong.
- quiz.key: the place of the right option in quiz.options, counting from 0.
- brief: the brief so far, updated with this change: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Keep what still holds. Its last line says what this change did.

When the change is not a diff but the agent's workers with the summaries of their cards, the agent changed nothing itself and handed the work to them: the card explains what it split up among them, what came back and what is still open, paths and code are empty, and the quiz is on how the work was split.
"#;

/// The writer's instructions for a brief updated from a session's workers' cards when it changed
/// nothing itself (#369, decision 11): no card, so a worker's return adds no thin card.
const UPDATE: &str = r#"You update the brief of a coding agent's session, which the developer who runs it reads to resume the session: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Since the brief, the agent changed nothing itself; its workers did.

Answer from the input below: the brief so far, and the agent's workers, each with its status and the summaries of its cards since then. Use only this input: do not use tools or read files. Keep what still holds. Its last line says what came back.

Write in Hebrew, in plain words. Code, code terms, file paths and commands stay in English; inside Hebrew text, wrap each of them in backticks.

Reply with one JSON object and nothing else: {"brief": string}
"#;

/// The writer's instructions for a brief rewritten from scratch (#369, decision 16): from Daniel's
/// prompts, the card summaries and the diffstat, never a diff, so errors in the rolling brief do not
/// compound.
const REWRITE: &str = r#"You rewrite from scratch the brief of a coding agent's session, which the developer who runs it reads to resume the session: what the session set out to do, what it has done, oldest first, and what is still open, in a few short lines. Its last line says what the latest change did.

Answer from the input below: the developer's prompts, the one-line summaries of the session's cards, and `git diff --stat` since the session began. Use only this input: do not use tools or read files.

Write in Hebrew, in plain words. Code, code terms, file paths and commands stay in English; inside Hebrew text, wrap each of them in backticks.

Reply with one JSON object and nothing else: {"brief": string}
"#;

/// The writer's instructions for "explain more" (#369, decision 15): one more call on the card's
/// change, which also reads the card's explanation, so it goes deeper instead of repeating it.
const MORE: &str = r#"A developer read a card that explains a change a coding agent made to his code, and asked for more. Explain the change in more depth than the card: how the changed code works, step by step, and why it was done this way and not another. Do not repeat the card.

Answer from the diff below. Use only this input: do not use tools or read files.

Write in Hebrew, in plain words. Code, code terms, file paths and commands stay in English; inside Hebrew text, wrap each of them in backticks.

Reply with the explanation only, in at most three short paragraphs: no JSON and no heading.
"#;
