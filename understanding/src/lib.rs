//! The understanding layer's I/O, which the core leaves to the shell: the card writer (danielrispler/Falinks#328,
//! module 4), which also writes the briefs (danielrispler/Falinks#369, decision 16), the worktree fingerprint (module 5),
//! the card store with its briefs and its weekly rates (module 6), and the drift in Claude Code
//! (danielrispler/Falinks#369, decisions 7, 9 and 23).
use serde::{Deserialize, Serialize};
use std::{
    hash::{BuildHasher, Hasher, RandomState},
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
mod card_code;
mod drift;
mod fingerprint;
mod grounding;
mod offer;
mod retest;
mod self_test;
mod store;
mod summary;
mod turn_trees;
mod turn_writing;
mod week;
pub use brief::{
    Answered, DesignLine, OpenItem, OpenKind, answered, design_lines, design_sources, open_items,
    write_design,
};
pub use card_code::{CodeRange, pull_code};
pub use drift::{AGY_FIXTURE_VERSION, AgyVersions, Drift, Versions, drift, dropped_events};
pub use fingerprint::{Fingerprints, backfilled, fingerprint_changes, live_roots, worktree_root};
use grounding::words;
pub use offer::{
    Changed, Decision, Inputs, Offer, OfferKind, Offers, Outcome, Settled, Shape, TurnFacts,
    changed_in, decide, is_logic, offer_stop, offer_summary, settle, turns_card,
};
pub use retest::{retest_quiz, write_retests};
pub use self_test::{ArmReport, SelfTest, self_test};
pub use store::{
    Arm, Brief, CardStore, CheckResult, Guess, Learning, Level, PlacedCard, QuizTap, Rating,
    Reason, Retest, RetestAsk, Row, SessionCards, StoredCard, StoredSummary, Surface, Tap, millis,
};
pub use store::{
    Place, QUIZZES_PER_HOUR, RETEST_AFTER_MS, RETEST_REACH_MS, RETESTS_PER_DAY, RETESTS_WAITING,
};
pub use summary::{
    Line, LineKind, SummarySetup, TurnSummary, write_agent_summary, write_name, write_summary,
};
pub use turn_trees::{
    Hunk, MAX_TURN_FILES, add_hunks, anchor, diff_turn, found_files, hunk, hunk_view, numstat,
    turn_diffs, turn_facts, turn_hunk, turn_of,
};
pub use turn_writing::{Core, Effects, TurnWriting, Writer};
pub use week::{Rate, Ratings, Week, week};

/// A card on a change, in the explanation language; code inside its text is in
/// backticks. It opens with its quiz; the rest shows when Daniel taps an option.
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

/// A card's multiple-choice question on the change's logic or design (danielrispler/Falinks#369, decision 13), named
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

/// Marks a writer may put on the right option, which would give the answer away (danielrispler/Falinks#517, decision
/// 14).
const MARKS: [&str; 7] = ["✅", "✔️", "✔", "✓", "☑️", "(correct)", "(Correct)"];

/// The most words an option has (danielrispler/plotkeeper#118).
pub const OPTION_WORDS: usize = 10;

impl Quiz {
    /// The quiz in `value`, its options' marks removed and shuffled with the key and reasons moving
    /// with them, as the writer places the right one unevenly (danielrispler/Falinks#434, decision 36); or why it is
    /// rejected (danielrispler/plotkeeper#118, `docs/studio/question-rules.md`): it does not parse, its key names none
    /// of two or more options, an option has no reason or no `right` mark, the key is not the one
    /// option marked right, an option has over `OPTION_WORDS` words, or the right option is the
    /// longest.
    pub(crate) fn from_value(value: serde_json::Value) -> Result<Quiz, String> {
        let marked: Vec<Option<bool>> = (value["options"].as_array().into_iter().flatten())
            .map(|option| option["right"].as_bool())
            .collect();
        let mut quiz: Quiz = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let options = quiz.options.len();
        if options < 2 || quiz.key >= options {
            return Err(format!("key {} of {options} options", quiz.key));
        }
        if quiz.options.iter().any(|o| o.why.trim().is_empty()) {
            return Err("an option has no reason".into());
        }
        if marked.contains(&None) {
            return Err("an option is not marked right or wrong".into());
        }
        let right: Vec<usize> = (marked.iter().enumerate())
            .filter(|(_, m)| **m == Some(true))
            .map(|(i, _)| i)
            .collect();
        if right != [quiz.key] {
            return Err(format!(
                "its key {} is not the one option marked right ({right:?})",
                quiz.key
            ));
        }
        for option in &mut quiz.options {
            let text = MARKS
                .iter()
                .fold(option.text.clone(), |t, m| t.replace(m, ""));
            option.text = text.trim().into();
        }
        if let Some(long) = quiz.options.iter().find(|o| words(&o.text) > OPTION_WORDS) {
            return Err(format!(
                "an option has {} words: {:?}",
                words(&long.text),
                long.text
            ));
        }
        let length = |o: &Choice| o.text.chars().count();
        let wrong = (quiz.options.iter().enumerate())
            .filter(|(i, _)| *i != quiz.key)
            .map(|(_, o)| length(o))
            .max();
        if wrong.is_some_and(|wrong| length(&quiz.options[quiz.key]) > wrong) {
            return Err("its right option is the longest".into());
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
        // The writer names ranges, never code: a copy it made anyway is dropped.
        card.code.clear();
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

/// Writes the card on a session's change since its last card.
pub trait CardWriter {
    /// The writer's reply on `request`'s change, or one line for the log when it gave none. It reads
    /// the change with `earlier`, the summaries of the earlier cards of the prompt the change belongs
    /// to, oldest first (danielrispler/Falinks#369, decision 12).
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String>;

    /// The writer's longer explanation of `card` on `request`'s change, for "explain more" (danielrispler/Falinks#369,
    /// decision 15), or one line for the log when it gave none.
    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String>;

    /// The writer's reply with the Goal of a session, or of a worker, from `prompts`, Daniel's or its
    /// spawn's, oldest first (danielrispler/plotkeeper#122), or one line for the log when it gave none.
    /// A writer that writes no goals says so.
    fn goal(&self, _prompts: &[&str]) -> Result<String, String> {
        Err("this writer writes no goals".into())
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

    /// The writer's reply with a fresh Quiz on the decision of `ask`'s first Quiz (danielrispler/plotkeeper#129), or one line
    /// for the log when it gave none. A writer that writes no retests says so.
    fn retest(&self, _ask: &RetestAsk) -> Result<String, String> {
        Err("this writer writes no retests".into())
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

/// The card on a session's or a worker's change, from `session`'s state, and its Goal (danielrispler/Falinks#369,
/// decisions 11 and 12; danielrispler/plotkeeper#122). `writer` writes the card on `request` with the
/// summaries of the earlier cards of its prompt, and it goes in `store`. A request with no change
/// writes no card. The Goal is written after `stored` runs on the card stored, so the shell sends the
/// list and the "New card" notice first (danielrispler/Falinks#434, decision 32). The store is not locked while
/// the writer runs, which takes seconds. Each problem is one line for `log`. Returns whether a card
/// was added.
pub fn write_stop(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    request: CardRequest,
    session: &Session,
    stored: impl FnOnce(),
    mut log: impl FnMut(String),
) -> bool {
    let owner = request.owner().to_owned();
    let earlier = {
        let store = store.lock().unwrap();
        let cards = store
            .cards()
            .iter()
            .filter(|kept| kept.request.owner() == owner);
        let same_prompt =
            cards.filter(|kept| request.prompt.is_some() && kept.request.prompt == request.prompt);
        same_prompt.filter_map(summary).collect::<Vec<String>>()
    };
    // A session that changed nothing itself gets no card, whatever its workers wrote.
    if request.change.is_none() {
        return false;
    }
    let Some(mut card) = write_card(writer, &request, &earlier, &mut log) else {
        return false;
    };
    // The cap check and the store are one step: another session's card cannot slip between them.
    {
        let mut store = store.lock().unwrap();
        place_card_quiz(&mut store, &request, &mut card, &mut log);
        if let Err(e) = store.add(request.clone(), card, millis()) {
            log(format!("save the card store: {e}"));
        }
    }
    stored();
    write_goal(store, writer, &request, session, log);
    true
}

/// Holds `card`'s Quiz to the rules for when a Quiz shows (danielrispler/plotkeeper#119, #257): none beside a
/// decision, which `request` says by how its turn ended and never by the session's state now; then at most
/// 3 an hour: past it, a session's card shows without its Quiz, and takes the place of the oldest unanswered lone
/// Quiz of the hour when there is one. A worker's card and a check's are no turn's and are left alone.
fn place_card_quiz(
    store: &mut CardStore,
    request: &CardRequest,
    card: &mut Card,
    mut log: impl FnMut(String),
) {
    if card.quiz.is_none() || request.agent.is_some() || request.check.is_some() {
        return;
    }
    if request.asks {
        card.quiz = None;
        return;
    }
    match store.place_quiz(true, millis()) {
        Place::Offer => {}
        Place::Withhold => card.quiz = None,
        Place::Replace(turn) => {
            if let Err(e) = store.drop_summary_quiz(&turn) {
                log(format!("save the summaries: {e}"));
            }
        }
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
    let value: serde_json::Value = serde_json::from_str(object(&reply)).unwrap_or_default();
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

/// Why a reply gave no goal, for the log.
const HOLDS_NONE: &str = "the reply holds none";

/// A stored card's summary, when it has one.
fn summary(kept: &StoredCard) -> Option<String> {
    Some(kept.card.summary.clone()).filter(|s| !s.is_empty())
}

/// The Goal in a writer's reply, when it holds one that is not empty, or else why not, for the log.
fn goal_in(reply: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(object(reply)).unwrap_or_default();
    match value["goal"].as_str().map(str::trim) {
        Some(goal) if !goal.is_empty() => Ok(goal.into()),
        _ => Err(HOLDS_NONE.into()),
    }
}

/// The longer explanation of card `id` in `store`: the one kept, or else one more call to `writer`,
/// whose text is kept with the card (danielrispler/Falinks#369, decision 15). The store is not locked while the writer
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
/// (danielrispler/Falinks#369, decision 12). The writer reads `earlier`, the summaries of the
/// earlier cards of the prompt `request` belongs to. Each problem is one line for `log`: no card
/// after the retry, or a card with no quiz, which is kept without asking again.
pub fn write_card(
    writer: &(impl CardWriter + ?Sized),
    request: &CardRequest,
    earlier: &[String],
    mut log: impl FnMut(String),
) -> Option<Card> {
    let card = || {
        let reply = writer.reply(request, earlier)?;
        let (mut card, mut problem) = Card::from_reply(&reply)?;
        // A check's card asks whether the check passes, which its result settles (danielrispler/Falinks#461, decision
        // 15), so its quiz is the app's, with the writer's reasons.
        if let Some(check) = &request.check {
            card.quiz = Some(check_quiz(check, &reply));
            problem = None;
        }
        Ok::<_, String>((card, problem))
    };
    // A reply with no card, or whose quiz breaks a rule (danielrispler/plotkeeper#118), is asked for once more; the
    // second stands, with no quiz when it breaks one again, or the first when the second gives no card.
    let first = card();
    let retry = !matches!(&first, Ok((_, None)));
    match if retry { card().or(first) } else { first } {
        Ok((card, problem)) => {
            if let Some(problem) = problem {
                log(format!("no quiz on {}: {problem}", request.id));
            }
            Some(Card {
                code: pull_code(&card.ranges, &request.ends),
                ..card
            })
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

/// Writes a fixed card that shows the change, and a fixed goal, so the app can run without
/// `claude -p`.
pub struct FakeWriter;

impl CardWriter for FakeWriter {
    fn reply(&self, request: &CardRequest, _: &[String]) -> Result<String, String> {
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
        let option = |text: &str, why: &str, right: bool| serde_json::json!({ "text": text, "why": why, "right": right });
        let card = serde_json::json!({
            "summary": format!("Sample change to {named}"),
            "explanation": "A sample card from the fake writer, which does not read the code. The change itself is below.",
            "paths": paths,
            "ranges": paths.iter().map(|path| serde_json::json!({"path": path, "from": 1, "to": 1})).collect::<Vec<_>>(),
            "architecture": architecture,
            "why_it_matters": "Why the change matters goes here, with code inside the text such as `if a <= b {`.",
            "not_checked": "Not checked: nothing, this is a sample card.",
            "if_it_passes": "It passes when the change is right: a sample reason from the fake writer.",
            "if_it_fails": "It fails when the change breaks a test: a sample reason from the fake writer.",
            "quiz": {
                "text": format!("What did the agent change in `{}`?", paths.join("`, `")),
                "options": [
                    option("What the code below shows.", "Right: the code below is the change itself.", true),
                    option("Only the docs, without touching the code.", "No: the fake writer does not know what changed.", false),
                    option("Nothing, it only ran tests.", "No: a card is written only on a change.", false),
                ],
                "key": 0,
            },
        });
        Ok(card.to_string())
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Ok("A longer sample explanation from the fake writer, which does not read the code. How the changed code works goes here, step by step, and why it was done this way, with code such as `if a <= b {`.".into())
    }

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
}

/// Writes the card with `claude -p` on Daniel's subscription, in a neutral folder that holds none of
/// his projects, so it loads no repository's instructions and has nothing to wander into.
pub struct ClaudeWriter {
    /// The `claude` program, from `find_claude`.
    program: PathBuf,
    folder: PathBuf,
    /// How long a call may run before it fails and is stopped (danielrispler/Falinks#434, decision 30).
    timeout: Duration,
    /// Told when no call can start and when one works again (danielrispler/plotkeeper#51).
    availability: Availability,
}

/// Whether the writer's program starts, and who hears when that changes.
#[derive(Default)]
struct Availability {
    off: AtomicBool,
    on_change: Option<Box<dyn Fn(Option<String>) + Send + Sync>>,
}

impl Availability {
    /// The program did not start: tells once until a call works.
    fn failed(&self, error: &std::io::Error) {
        if !self.off.swap(true, Ordering::SeqCst) {
            self.tell(Some(format!(
                "Cards and briefs are off: `claude` did not start ({error}). Install Claude Code, or fix its path."
            )));
        }
    }

    /// A call worked: clears the notice, if one was told.
    fn worked(&self) {
        if self.off.swap(false, Ordering::SeqCst) {
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
            availability: Availability::default(),
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
    /// cards, with the prompt it reads on standard input, as are all the runs below.
    pub fn command(&self, request: &CardRequest, earlier: &[String]) -> (Command, String) {
        let earlier: String = match earlier {
            [] => "none\n".into(),
            summaries => summaries.iter().map(|s| format!("- {s}\n")).collect(),
        };
        // A check's card is on what the check verifies against the change (danielrispler/Falinks#461, decision 15).
        let instructions = match &request.check {
            Some(check) => format!("{CHECK}\nThe check's command:\n{check}\n"),
            None => PROMPT.into(),
        };
        let head = format!(
            "{instructions}\nThe summaries of this prompt's earlier cards, oldest first:\n{earlier}\n"
        );
        self.claude(&head, request.diff())
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
    /// uses the subscription login (danielrispler/Falinks#320, "Fact"); the studio never reads that login itself. The
    /// prompt holds untrusted text, a change or Daniel's prompts, so the run has no tools and no MCP
    /// servers, whatever Daniel's settings allow. It also runs without the studio plugin, so its
    /// session sends no hook events, and in safe mode, so none of Daniel's plugins, hooks, skills or
    /// CLAUDE.md reach it (danielrispler/Falinks#517, decision 9). The prompt goes on standard input without its NUL bytes, which
    /// `claude` could not take in an argument either (danielrispler/Falinks#434, decision 31).
    fn run(&self, prompt: &str) -> (Command, String) {
        self.run_with(&[], prompt)
    }

    /// `run`, with `options` before `-p`.
    pub fn run_with(&self, options: &[&str], prompt: &str) -> (Command, String) {
        let mut command = Command::new(&self.program);
        command
            // `--tools` takes several values, so an option must follow it.
            .args(["--tools", "", "--strict-mcp-config", "--safe-mode"])
            .args(options)
            .arg("-p")
            .env_remove("ANTHROPIC_API_KEY")
            .env_remove("ANTHROPIC_AUTH_TOKEN")
            .env_remove("CLAUDE_CODE_PLUGIN_DIRS")
            .current_dir(&self.folder);
        (command, prompt.replace('\0', ""))
    }

    /// What a run printed, or one line for the log when the call failed: it did not start, exited
    /// with an error, printed nothing, or ran past the timeout, which stops it and every process it
    /// started, all in its own process group (danielrispler/Falinks#434, decision 30).
    fn stdout(&self, (mut command, prompt): (Command, String)) -> Result<String, String> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .process_group(0)
            .spawn()
            .map_err(|e| {
                self.availability.failed(&e);
                format!("`claude -p` did not start: {e}")
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
            (true, false) => {
                self.availability.worked();
                Ok(out.into())
            }
        }
    }
}

impl CardWriter for ClaudeWriter {
    /// Runs `claude -p`, which takes seconds, so the shell calls it off its log thread.
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Result<String, String> {
        self.stdout(self.command(request, earlier))
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        self.stdout(self.more_command(request, card))
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

    fn retest(&self, ask: &RetestAsk) -> Result<String, String> {
        self.stdout(self.retest_command(ask))
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
pub(crate) use explanation_language;

/// The Quiz's rules (danielrispler/plotkeeper#118), one paragraph in both writers' instructions; `docs/studio/question-rules.md`
/// holds them too. `headline` names what the writer's own headline is called in that prompt.
macro_rules! quiz_rules {
    ($headline:literal) => {
        concat!(
            "The quiz asks why this design over the alternative, what must stay true, or what happens in a concrete \
             case in this codebase. It never asks what the agent did, a name or syntax, or bookkeeping. Its stem \
             cannot be answered from ",
            $headline,
            " or from the evidence line (the recorded files, their line counts and the checks), which the developer \
             sees beside it: if they answer it, ask something else or leave the quiz out. Each wrong option comes from \
             the alternative that was considered, the previous behaviour, or a common wrong model of the code. Each \
             option has at most 10 words, and the right option is never the longest. Every option carries its reason \
             (`why`) and whether it is right (`right`): exactly one option is right, and the key is its place. Never \
             mark the right option in its text, with a sign, an emoji or a word: the options are shown in another \
             order."
        )
    };
}
pub(crate) use quiz_rules;

/// The card writer's instructions. Its input is the change, the summaries of the prompt's earlier
/// cards and the brief, not the payloads (v0 note, proposal 8).
const PROMPT: &str = concat!(
    r#"You write one card that teaches a developer about the change a coding agent made to his code while he waited on it, since the previous card. The card opens with a multiple-choice question on the change's logic or design. He taps an option, and the card shows at once whether he was right, why each option is right or wrong, and the explanation.

Answer from the input below: the summaries of the earlier cards on the same prompt of his, and the diff. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string, "explanation": string, "paths": [string], "ranges": [{"path": string, "from": number, "to": number}], "why_it_matters": string, "not_checked": string, "architecture": string, "quiz": {"text": string, "options": [{"text": string, "why": string, "right": boolean}], "key": number}}
- summary: what the change did, in one short line.
- explanation: what the change does and why, in at most three sentences.
- paths: the files the change touched.
- ranges: the lines of the change that matter most, as at most 3 of {"path", "from", "to"}: a file of the change and the first and last line, counted from 1, in the file as the change leaves it (the diff's + side), at most 12 lines each. Name the lines, never write them: the studio reads them from git. A range of lines the file does not have is dropped.
- why_it_matters: what the change does for the code, in one or two sentences.
- not_checked: one thing the diff does not show.
- architecture: when the change is structural, a short section on how it changes the code's structure, before and after, and why: modules, types, interfaces or files added, removed, moved or split, or a change in which parts call or depend on which. Read it with the earlier cards' summaries, so that a structural change spread over several cards counts. Otherwise exactly "structural: none".
- quiz.text: the quiz's question, under the rules below. Never ask whether a command, test or check will pass.
- quiz.options: 3 or 4 options, each {"text", "why", "right"}. why: one line on why that option is right or wrong; the options are shown in another order, so never refer to an option by its letter or place. right: true for the one right option, false for the others.
- quiz.key: the place of the right option in quiz.options, counting from 0.

"#,
    quiz_rules!("your `summary`"),
    r#""#
);

/// The card writer's instructions for a check's card (danielrispler/Falinks#461, decision 15), written while the check
/// runs: on what the check verifies against the change so far. The app asks whether it passes.
const CHECK: &str = concat!(
    r#"A coding agent just started a check, a command that tests or verifies its code, which usually runs for minutes. You write one card that teaches the developer who waits on it what this check verifies against the change the agent made since the previous card, so he can guess whether it will pass. The card asks him "Will it pass?"; once the check ends, it shows whether he was right and your two reasons.

Answer from the input below: the check's command, the summaries of the earlier cards on the same prompt of his, and the diff. Use only this input: do not use tools or read files. You do not know the check's result.

"#,
    explanation_language!(),
    r#"

Reply with one JSON object and nothing else:
{"summary": string, "explanation": string, "paths": [string], "why_it_matters": string, "not_checked": string, "architecture": string, "if_it_passes": string, "if_it_fails": string}
- summary: what the change did, in one short line.
- explanation: what the check runs and what of the change it verifies, in at most three sentences.
- paths: the files the change touched.
- why_it_matters: what the check's result tells about the change, in one or two sentences.
- not_checked: one thing in the change the check does not verify.
- architecture: when the change is structural, a short section on how it changes the code's structure, before and after, and why. Otherwise exactly "structural: none".
- if_it_passes: one line on what a pass would show about the change.
- if_it_fails: one line on the likeliest reason in the change it could fail.
"#
);

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

/// The writer's instructions for "explain more" (danielrispler/Falinks#369, decision 15): one more call on the card's
/// change, which also reads the card's explanation, so it goes deeper instead of repeating it.
const MORE: &str = concat!(
    r#"A developer read a card that explains a change a coding agent made to his code, and asked for more. Explain the change in more depth than the card: how the changed code works, step by step, and why it was done this way and not another. Do not repeat the card.

Answer from the change below. Use only this input: do not use tools or read files.

"#,
    explanation_language!(),
    r#"

Reply with the explanation only, in at most three short paragraphs: no JSON and no heading.
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
