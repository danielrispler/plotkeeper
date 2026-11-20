//! The card store (danielrispler/Falinks#328, module 6): the cards, Daniel's guesses and their records, and each
//! session's brief, on disk in the studio's data directory, so a restart neither loses nor re-asks
//! them.

use crate::{Answered, Card, DesignLine, OpenItem, Quiz, TurnSummary, answered};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use studio_core::{CardRequest, Decision, LiveFacts, Session, Step, Studio, SummaryRequest};

/// A card in the store, with the change it explains and its base, and its record: when it was added,
/// when its quiz showed, Daniel's guess, his flag on the key, his rating and the longer explanation he
/// asked for. The card and its request go through serde whole, so fields added to them need no change here.
/// Times are Unix milliseconds; stores from before a time existed load it as `None`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredCard {
    pub request: CardRequest,
    pub card: Card,
    /// When the card reached the store, which places it in a week.
    #[serde(default)]
    pub added_at: Option<u64>,
    /// When the pane first drew its quiz in a visible window; `None` until then.
    #[serde(default)]
    pub shown_at: Option<u64>,
    /// Daniel's tap on the quiz; `None` while the card shows only its quiz.
    #[serde(default)]
    pub guess: Option<Guess>,
    /// Daniel flagged that the writer's key is wrong (danielrispler/Falinks#369, decision 13), from the quiz or from his
    /// rating: one record.
    #[serde(default)]
    pub key_wrong: bool,
    /// The writer's longer explanation, from Daniel's first "explain more" (danielrispler/Falinks#369, decision 15).
    #[serde(default)]
    pub more: Option<String>,
    /// Daniel's rating of the explanation (danielrispler/Falinks#369, decision 15); `None` until he rates it.
    #[serde(default)]
    pub rating: Option<Rating>,
    /// The result of the check whose start requested the card (danielrispler/Falinks#461, decision 15), which settles
    /// its guess; `None` while it runs, and on a card no check requested.
    #[serde(default)]
    pub check_result: Option<CheckResult>,
    /// What Daniel did with the quiz besides the pick (`guess`): a skip and the chips, each with its time
    /// (danielrispler/plotkeeper#120).
    #[serde(default)]
    pub taps: Vec<QuizTap>,
}

/// What a check's result says about the guess on its card: it passed, it failed, or its exit code
/// does not say (`CheckLine::passed`), so the guess is not counted.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckResult {
    Passed,
    Failed,
    NotCounted,
}

/// A turn summary in the store (danielrispler/Falinks#526), with the turn it is on and when it arrived.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredSummary {
    pub request: SummaryRequest,
    pub summary: TurnSummary,
    pub added_at: u64,
    /// What Daniel did with the turn's own Quiz, each with its time (danielrispler/plotkeeper#120); a turn with a card has
    /// its taps on the card.
    #[serde(default)]
    pub taps: Vec<QuizTap>,
    /// The turn's hunks as the writer read them (danielrispler/plotkeeper#129), which its retest is written from; empty
    /// before they were kept, and for a turn with no facts.
    #[serde(default)]
    pub hunk: String,
    /// The self-test's arm (danielrispler/plotkeeper#130), kept only on a turn with a Quiz that was written while the test ran;
    /// `None` is quiz-first.
    #[serde(default)]
    pub arm: Option<Arm>,
}

/// How long the self-test assigns arms: 21 days from its start (danielrispler/plotkeeper#130).
pub const SELF_TEST_MS: u64 = 21 * 24 * 60 * 60 * 1000;

/// A turn's arm in the self-test (danielrispler/plotkeeper#130): quiz-first shows the Quiz open and the Body after a pick;
/// body-only shows the Body open and no Quiz, though the Quiz is kept for the retest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    QuizFirst,
    BodyOnly,
}

/// A random bit, from the random keys std gives each hasher.
fn coin() -> bool {
    use std::hash::{BuildHasher, Hasher};
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
        & 1
        == 1
}

/// What Daniel does with a Quiz (danielrispler/plotkeeper#120): a pick, "Show the rest anyway" (a skip, which reveals as a pick
/// does), and the three chips under the revealed answer. The trial's two (danielrispler/plotkeeper#129): how sure he is
/// before the pick, and an optional line on why after it, which no model reads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Tap {
    Pick {
        option: usize,
    },
    Confidence {
        sure: bool,
    },
    Why {
        text: String,
    },
    /// A retest's first showing, and the first hunk opened on a turn (danielrispler/plotkeeper#130).
    Shown,
    Hunk,
    Skip,
    Understood,
    ExplainMore,
    KeyWrong,
}

/// A tap and when, in Unix milliseconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuizTap {
    #[serde(flatten)]
    pub tap: Tap,
    pub at: u64,
}

impl QuizTap {
    /// Whether it answers the Quiz, as a pick or a skip do.
    fn answers(&self) -> bool {
        matches!(self.tap, Tap::Pick { .. } | Tap::Skip)
    }
}

/// Whether `taps` hold a pick; a skip, a `shown` or a chip is not one.
fn picked(taps: &[QuizTap]) -> bool {
    taps.iter().any(|t| matches!(t.tap, Tap::Pick { .. }))
}

/// Keeps `tap` at `at` in `taps`: the first answer counts, and each chip once. Whether it was kept.
fn keep_tap(taps: &mut Vec<QuizTap>, tap: Tap, at: u64) -> bool {
    let new = QuizTap { tap, at };
    let taken = taps.iter().any(|kept| {
        if new.answers() {
            kept.answers()
        } else {
            std::mem::discriminant(&kept.tap) == std::mem::discriminant(&new.tap)
        }
    });
    if !taken {
        taps.push(new);
    }
    !taken
}

/// How long after a Quiz was offered its retest is due: seven days (danielrispler/plotkeeper#129).
pub const RETEST_AFTER_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// How long after it came due a retest is still served and written: fourteen days (danielrispler/plotkeeper#226).
pub const RETEST_REACH_MS: u64 = 14 * 24 * 60 * 60 * 1000;

/// How many retests may wait unanswered before no more are written (danielrispler/plotkeeper#215).
pub const RETESTS_WAITING: usize = 3;

/// A fresh Quiz on the decision of an earlier one, offered on Home a week after it (danielrispler/plotkeeper#129). `quiz`
/// is `None` when the writer's replies broke the rules twice: the retest is then not asked for again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Retest {
    /// What `quiz_tap` names it by: `retest ` and the id of the Quiz it retests, `of`.
    pub id: String,
    pub of: String,
    pub session_id: String,
    pub added_at: u64,
    pub quiz: Option<Quiz>,
    #[serde(default)]
    pub taps: Vec<QuizTap>,
}

/// What the retest's writer reads: the turn's stored hunk, and the first Quiz with the reason of its key.
#[derive(Clone, Debug, PartialEq)]
pub struct RetestAsk {
    pub of: String,
    pub session_id: String,
    pub hunk: String,
    pub first: Quiz,
}

/// A Quiz that can be retested, as `CardStore::first_quizzes` reads it.
struct FirstQuiz<'a> {
    of: &'a str,
    session_id: &'a str,
    hunk: &'a str,
    quiz: &'a Quiz,
    offered: u64,
    picked: bool,
    key_wrong: bool,
}

impl FirstQuiz<'_> {
    /// What its retest's writer reads.
    fn ask(self) -> RetestAsk {
        RetestAsk {
            of: self.of.into(),
            session_id: self.session_id.into(),
            hunk: self.hunk.into(),
            first: self.quiz.clone(),
        }
    }

    /// Whether its retest can be served at `now`: due at most `RETEST_REACH_MS` before, and its key not flagged wrong.
    fn servable(&self, now: u64) -> bool {
        let due = self.offered.saturating_add(RETEST_AFTER_MS);
        !self.key_wrong && (due..=due.saturating_add(RETEST_REACH_MS)).contains(&now)
    }
}

/// Daniel's rating of a card's explanation: a level and the reasons he picked. His fifth reason, "the
/// key is wrong", is `StoredCard::key_wrong`, the one record the quiz's flag also sets.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    pub level: Level,
    pub reasons: Vec<Reason>,
}

/// A rating's three levels.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Good,
    Fair,
    Poor,
}

/// A rating's short reasons, besides "the key is wrong".
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Unclear,
    Wrong,
    TooShallow,
    TooEasy,
}

/// Daniel's tap on a card's quiz, graded by the writer's key (danielrispler/Falinks#369, decision 13), or on a check's card
/// by the check's result (danielrispler/Falinks#461, decision 15).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guess {
    /// The option he tapped, counting from 0.
    pub option: usize,
    /// Whether it is the key. On a check's card, whether it called the check's result: `None` until
    /// the result settles it, and for good when the result does not say (`StoredCard::check_result`).
    /// A store from before danielrispler/Falinks#476 holds a plain `true` or `false`, which loads as `Some`.
    pub right: Option<bool>,
    /// When, in Unix milliseconds.
    pub at: u64,
}

/// A session list row: the session, and what its row says (`CardStore::line`).
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub session: Session,
    pub line: String,
}

/// A session's or a worker's brief: its Goal, which a model writes from Daniel's prompts
/// (danielrispler/plotkeeper#122). A brief stored before the slots is prose in `text`, which shows
/// until the first Goal.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Brief {
    pub text: String,
    pub goal: Option<String>,
    /// How many prompts the Goal was written from, so that it is written again when one comes.
    pub goal_prompts: u32,
    /// "Design so far" (danielrispler/plotkeeper#123), and how many sources it was written from, so that it is
    /// written again when one came.
    pub design: Vec<DesignLine>,
    pub design_from: u32,
    /// Unused: kept so that the previous build, whose `Brief` requires it, can still read the file.
    #[serde(default)]
    pub updates: u32,
}

/// One session's brief and its cards in the session view's two groups (danielrispler/Falinks#369, decision 18), each
/// oldest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SessionCards {
    /// A brief stored as prose, which shows while there is no Goal.
    pub brief: Option<String>,
    /// The Brief's slots (danielrispler/plotkeeper#122): the Goal, the answered questions and the
    /// computed Open lines, which the shell fills in (`open_items`).
    pub goal: Option<String>,
    pub answers: Vec<Answered>,
    pub open: Vec<OpenItem>,
    /// "Design so far": at most five decisions, each citing its turn or card.
    pub design: Vec<DesignLine>,
    /// The cards added in the last 30 minutes.
    #[serde(serialize_with = "without_diffs")]
    pub recent: Vec<StoredCard>,
    /// The cards added before, or before the store kept the time.
    #[serde(serialize_with = "without_diffs")]
    pub older: Vec<StoredCard>,
    /// A session's workers' cards, each with the turn of the session's rail it belongs to
    /// (danielrispler/plotkeeper#212); empty in a worker's own view.
    pub workers: Vec<PlacedCard>,
    /// Its decision history, oldest first (danielrispler/Falinks#461, decision 17).
    pub decisions: Vec<Decision>,
}

/// A worker's card with the turn of its session's rail it belongs to, `None` when the session has no turn for it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedCard {
    pub card: StoredCard,
    pub turn: Option<usize>,
}

impl Serialize for PlacedCard {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut placed = s.serialize_struct("PlacedCard", 2)?;
        placed.serialize_field("card", &ShownCard::of(&self.card))?;
        placed.serialize_field("turn", &self.turn)?;
        placed.end()
    }
}

/// A stored card as the pane reads it: its request without the diff and base, which the pane never
/// reads.
#[derive(Serialize)]
struct ShownCard<'a> {
    request: ShownRequest<'a>,
    card: &'a Card,
    added_at: Option<u64>,
    shown_at: Option<u64>,
    guess: &'a Option<Guess>,
    key_wrong: bool,
    more: &'a Option<String>,
    rating: &'a Option<Rating>,
    check_result: Option<CheckResult>,
    taps: &'a [QuizTap],
}

#[derive(Serialize)]
struct ShownRequest<'a> {
    session_id: &'a str,
    id: &'a str,
    prompt: Option<u64>,
    agent: &'a Option<String>,
    changed_by: &'a BTreeMap<String, BTreeSet<String>>,
    workers: &'a [String],
    check: &'a Option<String>,
}

impl<'a> ShownCard<'a> {
    fn of(kept: &'a StoredCard) -> Self {
        ShownCard {
            request: ShownRequest {
                session_id: &kept.request.session_id,
                id: &kept.request.id,
                prompt: kept.request.prompt,
                agent: &kept.request.agent,
                changed_by: &kept.request.changed_by,
                workers: &kept.request.workers,
                check: &kept.request.check,
            },
            card: &kept.card,
            added_at: kept.added_at,
            shown_at: kept.shown_at,
            guess: &kept.guess,
            key_wrong: kept.key_wrong,
            more: &kept.more,
            rating: &kept.rating,
            check_result: kept.check_result,
            taps: &kept.taps,
        }
    }
}

fn without_diffs<S: serde::Serializer>(cards: &[StoredCard], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(cards.iter().map(ShownCard::of))
}

/// The last 30 minutes (danielrispler/Falinks#369, decision 18).
const RECENT_MS: u64 = 30 * 60 * 1000;

/// Now, in Unix milliseconds, as the store's records keep time.
pub fn millis() -> u64 {
    UNIX_EPOCH.elapsed().unwrap_or_default().as_millis() as u64
}

/// The store's cards, and its briefs by session or worker, kept in memory and written to their files on each
/// change: the briefs to `briefs.json` beside the cards (danielrispler/Falinks#369, decision 17).
// shortcut: rewrites the whole file on each change, which suits a few cards a day; switch to
// appended lines if it grows to thousands of cards.
pub struct CardStore {
    path: PathBuf,
    cards: Vec<StoredCard>,
    briefs: BTreeMap<String, Brief>,
    /// What the live app learned that the event log does not hold, in `live-facts.json` beside the
    /// cards (danielrispler/Falinks#434, decision 37).
    facts: LiveFacts,
    /// Each session's decision history, by its row's id, in `decisions.json` beside the cards (danielrispler/Falinks#461,
    /// decision 17).
    decisions: BTreeMap<String, Vec<Decision>>,
    /// The turn summaries, oldest first, in `summaries.json` beside the cards (danielrispler/Falinks#526).
    summaries: Vec<StoredSummary>,
    /// The subagent summaries, by subagent id, in `agent-summaries.json` beside the cards (danielrispler/Falinks#535).
    agent_summaries: BTreeMap<String, String>,
    /// The retests, in `retests.json` beside the cards (danielrispler/plotkeeper#129).
    retests: Vec<Retest>,
    /// When the self-test started, in Unix milliseconds, from the settings; kept in memory.
    self_test: Option<u64>,
    /// Rises on every change, briefs included, so the shell sends the cards only when it moves.
    revision: u64,
}

/// The JSON file at `path`, or the default when it is missing. One that does not parse is an
/// error, so it is never overwritten.
fn read<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> io::Result<T> {
    match fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e),
    }
}

impl CardStore {
    /// Reads the store at `path`, and the briefs beside it. A missing store is empty.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let cards = read(&path)?;
        let briefs = read(&path.with_file_name("briefs.json"))?;
        let facts = read(&path.with_file_name("live-facts.json"))?;
        let decisions = read(&path.with_file_name("decisions.json"))?;
        let summaries = read(&path.with_file_name("summaries.json"))?;
        // A cache: a corrupt file costs the summaries again, never the studio's start.
        let agent_summaries =
            read(&path.with_file_name("agent-summaries.json")).unwrap_or_default();
        let retests = read(&path.with_file_name("retests.json"))?;
        Ok(CardStore {
            path,
            retests,
            self_test: None,
            cards,
            briefs,
            facts,
            decisions,
            summaries,
            agent_summaries,
            revision: 0,
        })
    }

    /// Reads the store the app keeps beside the event log `log`, in the studio's data directory. The
    /// store from before cards covered a prompt's whole change (danielrispler/Falinks#369, decision 17) is moved aside to
    /// `cards-v1.json` on first start, and never over one already there.
    pub fn open_beside(log: &Path) -> io::Result<Self> {
        let (old, aside) = (
            log.with_file_name("cards.json"),
            log.with_file_name("cards-v1.json"),
        );
        if old.exists() && !aside.exists() {
            fs::rename(old, aside)?;
        }
        Self::open(log.with_file_name("cards-v2.json"))
    }

    /// A number that every change to the cards or the briefs raises.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A turn's summary now waits for Daniel to open it: the revision rises, so that a window that
    /// asked for it before and got none asks again.
    pub fn waiting_summary(&mut self) {
        self.revision += 1;
    }

    /// The cards, oldest first.
    pub fn cards(&self) -> &[StoredCard] {
        &self.cards
    }

    /// The cards and brief of `owner`, a session's id, or a worker's for its own (danielrispler/Falinks#369, decision 11),
    /// split 30 minutes before `now`, the shell's tick.
    pub fn session_cards(&self, owner: &str, now: u64) -> SessionCards {
        let cards = self
            .cards
            .iter()
            .filter(|kept| kept.request.owner() == owner);
        let (recent, older) = cards.cloned().partition(|kept| {
            kept.added_at
                .is_some_and(|at| now.saturating_sub(at) < RECENT_MS)
        });
        let brief = self.brief(owner);
        let goal = brief.and_then(|brief| brief.goal.clone());
        let design = brief.map(|brief| brief.design.clone()).unwrap_or_default();
        let brief = brief
            .map(|brief| brief.text.clone())
            .filter(|text| !text.is_empty());
        // A session's history holds its workers' decisions; a worker's, its own.
        let decisions = self.decisions.iter().flat_map(|(session, kept)| {
            let its = move |d: &&Decision| session == owner || d.agent.as_deref() == Some(owner);
            kept.iter().filter(its)
        });
        let decisions: Vec<Decision> = decisions.cloned().collect();
        SessionCards {
            brief,
            goal,
            answers: answered(&decisions),
            open: Vec::new(),
            design,
            recent,
            older,
            workers: Vec::new(),
            decisions,
        }
    }

    /// `session`'s cards and brief, and its workers' cards each with its turn: what its rail shows.
    pub fn rail_cards(&self, session: &Session, now: u64) -> SessionCards {
        let mut shown = self.session_cards(&session.agent.id, now);
        shown.workers = self
            .worker_cards(session)
            .map(|kept| PlacedCard {
                turn: session.worker_card_turn(&kept.request),
                card: kept.clone(),
            })
            .collect();
        shown
    }

    /// The cards of `session`'s workers.
    fn worker_cards<'a>(&'a self, session: &'a Session) -> impl Iterator<Item = &'a StoredCard> {
        let own = move |kept: &&StoredCard| kept.request.session_id == session.agent.id;
        let known = move |kept: &&StoredCard| session.subagent(kept.request.owner()).is_some();
        let workers = self
            .cards
            .iter()
            .filter(|kept| kept.request.agent.is_some());
        workers.filter(own).filter(known)
    }

    /// The turn each of `session`'s worker cards is placed in, by card id: the rail redraws when one
    /// moves, as a wake-up that arrives after the card was stored moves it.
    pub fn placements(&self, session: &Session) -> Vec<(String, Option<usize>)> {
        let placed = self.worker_cards(session).map(|kept| {
            let turn = session.worker_card_turn(&kept.request);
            (kept.request.id.clone(), turn)
        });
        placed.collect()
    }

    /// What `session`'s row says at `now`, in Unix milliseconds (danielrispler/Falinks#410, decision 27): while it needs
    /// Daniel, who asks and for how long; once its turn is over, the stopped line; after a wake-up
    /// with no step since, which worker returned; else its last step, else "No step yet".
    pub fn line(&self, session: &Session, now: u64) -> String {
        let step = match &session.agent.last_step {
            Some(Step {
                tool,
                detail: Some(detail),
                ..
            }) => format!("{tool} {detail}"),
            Some(step) => step.tool.clone(),
            None => "No step yet".into(),
        };
        if let Some(asker) = &session.asker {
            let minutes = now.saturating_sub(asker.since) / 60_000;
            let waited = if minutes < 60 {
                format!("{minutes}m")
            } else {
                format!("{}h", minutes / 60)
            };
            let who = match &asker.agent {
                None => step,
                Some(id) => format!("{} asks", session.agent_name(id)),
            };
            return format!("{who} · {waited}");
        }
        self.stopped_line(session)
            .or_else(|| session.returned.clone())
            .unwrap_or(step)
    }

    /// What `session`'s row says while its turn is over (danielrispler/Falinks#369, decision 19): the summary of the card
    /// on its `Stop`; the first line of the agent's last message until that card arrives; the brief's
    /// latest line when the `Stop` requested no card. `None` in a turn.
    fn stopped_line(&self, session: &Session) -> Option<String> {
        let stopped = session.stopped.as_ref()?;
        let card = self
            .cards
            .iter()
            .find(|kept| kept.request.id == stopped.card);
        let summary = card.map(|kept| kept.card.summary.clone());
        let brief = self
            .brief(&session.agent.id)
            .and_then(|b| b.text.lines().last());
        let pending = stopped.requested.then(|| stopped.message.clone()).flatten();
        let latest = pending.or(brief.map(Into::into));
        summary
            .filter(|s| !s.is_empty())
            .or(latest)
            .or_else(|| stopped.message.clone())
    }

    /// The brief of `owner`, a session's id or a worker's, `None` before its first update.
    pub fn brief(&self, owner: &str) -> Option<&Brief> {
        self.briefs.get(owner)
    }

    /// Sets `owner`'s "Design so far", written from `from` sources, and saves the briefs.
    pub fn set_design(
        &mut self,
        owner: &str,
        design: Vec<DesignLine>,
        from: u32,
    ) -> io::Result<()> {
        let brief = self.briefs.entry(owner.into()).or_default();
        brief.design = design;
        brief.design_from = from;
        self.revision += 1;
        write(&self.path.with_file_name("briefs.json"), &self.briefs)
    }

    /// Sets `owner`'s Goal, written from `prompts` prompts, and saves the briefs.
    pub fn set_goal(&mut self, owner: &str, goal: String, prompts: u32) -> io::Result<()> {
        let brief = self.briefs.entry(owner.into()).or_default();
        brief.goal = Some(goal);
        brief.goal_prompts = prompts;
        self.revision += 1;
        write(&self.path.with_file_name("briefs.json"), &self.briefs)
    }

    /// Adds a new card at `at`, in Unix milliseconds, which shows its quiz first, and saves the store.
    pub fn add(&mut self, request: CardRequest, card: Card, at: u64) -> io::Result<()> {
        self.cards.push(StoredCard {
            request,
            card,
            added_at: Some(at),
            shown_at: None,
            guess: None,
            key_wrong: false,
            more: None,
            rating: None,
            check_result: None,
            taps: Vec::new(),
        });
        self.save()
    }

    /// Marks the quizzes of cards `ids` shown at `at`, in Unix milliseconds, and saves the store
    /// once if any changed. A card keeps its first time, and one guessed before gets none, so a
    /// guess never comes before its quiz.
    pub fn show(&mut self, ids: &[impl AsRef<str>], at: u64) -> io::Result<()> {
        let mut changed = false;
        for id in ids {
            if let Some(kept) = self.card_mut(id.as_ref())
                && kept.shown_at.is_none()
                && kept.guess.is_none()
            {
                kept.shown_at = Some(at);
                changed = true;
            }
        }
        if changed { self.save() } else { Ok(()) }
    }

    /// Records Daniel's tap on option `option` of card `id`'s quiz at `at`, in Unix milliseconds,
    /// graded by its key, and saves the store. The first tap counts; a card with no quiz, or no such
    /// option, records none. On a check's card only a tap before the check's result counts, which
    /// the result grades (`settle`).
    pub fn guess(&mut self, id: &str, option: usize, at: u64) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id)
            && kept.guess.is_none()
            && kept.check_result.is_none()
            && let Some(quiz) = &kept.card.quiz
            && option < quiz.options.len()
        {
            let right = kept.request.check.is_none().then_some(option == quiz.key);
            kept.guess = Some(Guess { option, right, at });
            return self.save();
        }
        Ok(())
    }

    /// Settles the guess on card `id`, a check's, with the check's result (danielrispler/Falinks#461, decision 15):
    /// `passed`, or `None` when its exit code does not say. Its quiz's key becomes the option the
    /// result names, "It passes" or "It fails", and saves the store. A card no check requested, or
    /// one settled before, is left as it is.
    pub fn settle(&mut self, id: &str, passed: Option<bool>) -> io::Result<()> {
        let Some(kept) = self.card_mut(id) else {
            return Ok(());
        };
        if kept.request.check.is_none() || kept.check_result.is_some() {
            return Ok(());
        }
        kept.check_result = Some(match passed {
            Some(true) => CheckResult::Passed,
            Some(false) => CheckResult::Failed,
            // shortcut: an exit code that does not say leaves the guess not counted, where danielrispler/Falinks#343 let
            // the writer's `passed` settle it: the core skips a check's output (danielrispler/Falinks#410), and the
            // check's card took the change, so often no later card comes to read it. Ask the writer
            // on the output at the check's end if `cargo test 2>&1 | tail` checks leave many unscored.
            None => CheckResult::NotCounted,
        });
        if let Some(passed) = passed
            && let Some(quiz) = &mut kept.card.quiz
        {
            quiz.key = usize::from(!passed);
            if let Some(guess) = &mut kept.guess {
                guess.right = Some(guess.option == quiz.key);
            }
        }
        self.save()
    }

    /// Settles the cards of `session`'s checks that ended (`settle`). The shell calls it after each
    /// log read, once a card is stored and before a tap, so a tap after the result never counts.
    pub fn settle_checks(&mut self, session: &Session) -> io::Result<()> {
        for line in session.checks.iter().filter(|l| l.ended_at.is_some()) {
            if let Some(card) = &line.card {
                self.settle(card, line.passed)?;
            }
        }
        Ok(())
    }

    /// Records Daniel's `tap` at `at` on the Quiz of `id`, a card's or a turn's own (danielrispler/plotkeeper#120), and saves it.
    /// A card's pick is its guess, and its key flag the same one record; the first answer counts, so a
    /// pick after a skip, a skip after a pick and a tap on a check's card record nothing.
    pub fn quiz_tap(&mut self, id: &str, tap: Tap, at: u64) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id) {
            let answered = kept.guess.is_some() || kept.taps.iter().any(QuizTap::answers);
            if kept.card.quiz.is_none() || kept.request.check.is_some() {
                return Ok(());
            }
            return match tap {
                Tap::Pick { option } if !answered => self.guess(id, option, at),
                Tap::Pick { .. } => Ok(()),
                Tap::Skip if answered => Ok(()),
                _ => {
                    let key_wrong = matches!(tap, Tap::KeyWrong);
                    if keep_tap(&mut kept.taps, tap, at) {
                        if key_wrong {
                            kept.key_wrong = true;
                        }
                        return self.save();
                    }
                    Ok(())
                }
            };
        }
        if let Some(kept) = self.retests.iter_mut().find(|kept| kept.id == id) {
            let options = kept.quiz.as_ref().map_or(0, |quiz| quiz.options.len());
            if options == 0 || matches!(tap, Tap::Pick { option } if option >= options) {
                return Ok(());
            }
            if keep_tap(&mut kept.taps, tap, at) {
                self.revision += 1;
                return write(&self.path.with_file_name("retests.json"), &self.retests);
            }
            return Ok(());
        }
        let Some(kept) = self.summaries.iter_mut().find(|kept| kept.request.id == id) else {
            return Ok(());
        };
        let options = kept
            .summary
            .quiz
            .as_ref()
            .map_or(0, |quiz| quiz.options.len());
        if options == 0 || matches!(tap, Tap::Pick { option } if option >= options) {
            return Ok(());
        }
        if keep_tap(&mut kept.taps, tap, at) {
            self.revision += 1;
            return write(&self.path.with_file_name("summaries.json"), &self.summaries);
        }
        Ok(())
    }

    /// The Quizzes offered `RETEST_AFTER_MS` or more before `now`, a card's or a turn's own, that have no retest yet and
    /// that have a hunk to ask on (danielrispler/plotkeeper#129). `now` is the caller's clock.
    pub fn due_retests(&self, now: u64) -> Vec<RetestAsk> {
        self.due(now).map(FirstQuiz::ask).collect()
    }

    /// The due retests the hourly run writes at `now` (danielrispler/plotkeeper#215), in the order Home serves them
    /// (danielrispler/plotkeeper#226): those whose first Quiz has a pick first, then the rest, the latest due first
    /// within each. None came due more than `RETEST_REACH_MS` before `now`, and none's first Quiz is flagged Key wrong.
    pub fn retests_to_write(&self, now: u64) -> Vec<RetestAsk> {
        let mut due: Vec<_> = self.due(now).filter(|first| first.servable(now)).collect();
        due.sort_by_key(|first| (!first.picked, std::cmp::Reverse(first.offered)));
        due.into_iter().map(FirstQuiz::ask).collect()
    }

    /// The Quizzes offered `RETEST_AFTER_MS` or more before `now` that have a hunk and no retest yet.
    fn due(&self, now: u64) -> impl Iterator<Item = FirstQuiz<'_>> {
        self.first_quizzes().filter(move |first| {
            now >= first.offered.saturating_add(RETEST_AFTER_MS)
                && !first.hunk.trim().is_empty()
                && self.retests.iter().all(|r| r.of != first.of)
        })
    }

    /// How many retests wait unanswered at `now`, as Home could serve them (danielrispler/plotkeeper#215): each holds
    /// a Quiz with no pick, came due at most `RETEST_REACH_MS` before, and its first Quiz is not flagged Key wrong.
    pub fn waiting_retests(&self, now: u64) -> usize {
        (self.retests.iter())
            .filter(|r| r.quiz.is_some() && !picked(&r.taps))
            .filter(|r| {
                self.first_quizzes()
                    .find(|first| first.of == r.of)
                    .is_some_and(|first| first.servable(now))
            })
            .count()
    }

    /// Each Quiz that can be retested, a card's or a turn's own, with what the retests read of it.
    fn first_quizzes(&self) -> impl Iterator<Item = FirstQuiz<'_>> {
        let cards = self.cards.iter().filter_map(|kept| {
            Some(FirstQuiz {
                of: &kept.request.id,
                session_id: &kept.request.session_id,
                hunk: kept.request.diff(),
                quiz: (kept.card.quiz.as_ref()).filter(|_| kept.request.check.is_none())?,
                offered: kept.added_at.or(kept.shown_at)?,
                picked: kept.guess.is_some(),
                key_wrong: kept.key_wrong,
            })
        });
        let summaries = self.summaries.iter().filter_map(|kept| {
            Some(FirstQuiz {
                of: &kept.request.id,
                session_id: &kept.request.session_id,
                hunk: &kept.hunk,
                quiz: kept.summary.quiz.as_ref()?,
                offered: kept.added_at,
                picked: picked(&kept.taps),
                key_wrong: kept.taps.iter().any(|t| t.tap == Tap::KeyWrong),
            })
        });
        cards.chain(summaries)
    }

    /// Keeps the retest of `ask`'s Quiz, `quiz` as the writer wrote it, or `None` when it was rejected, at `at`; and
    /// saves the retests. A Quiz that has one is not asked again.
    pub fn add_retest(&mut self, ask: &RetestAsk, quiz: Option<Quiz>, at: u64) -> io::Result<()> {
        if self.retests.iter().any(|r| r.of == ask.of) {
            return Ok(());
        }
        self.retests.push(Retest {
            id: format!("retest {}", ask.of),
            of: ask.of.clone(),
            session_id: ask.session_id.clone(),
            added_at: at,
            quiz,
            taps: Vec::new(),
        });
        self.revision += 1;
        write(&self.path.with_file_name("retests.json"), &self.retests)
    }

    /// The retests, oldest first.
    pub fn retests(&self) -> &[Retest] {
        &self.retests
    }

    /// Records Daniel's flag that the key of card `id`'s quiz is wrong, and saves the store.
    pub fn key_wrong(&mut self, id: &str) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id)
            && !kept.key_wrong
        {
            kept.key_wrong = true;
            return self.save();
        }
        Ok(())
    }

    /// Records Daniel's rating of card `id`, which replaces the one before, and saves the store.
    pub fn rate(&mut self, id: &str, rating: Rating) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id) {
            kept.rating = Some(rating);
            return self.save();
        }
        Ok(())
    }

    /// Keeps `more`, the writer's longer explanation of card `id`, unless it has one, and saves the
    /// store.
    pub(crate) fn more(&mut self, id: &str, more: String) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id)
            && kept.more.is_none()
        {
            kept.more = Some(more);
            return self.save();
        }
        Ok(())
    }

    /// Keeps the decisions of session `session` (`Session::decisions`) not kept yet, by their call,
    /// and saves the history if any was new.
    pub fn keep_decisions(&mut self, session: &str, decisions: &[Decision]) -> io::Result<()> {
        let kept = self.decisions.entry(session.into()).or_default();
        let before = kept.len();
        for decision in decisions {
            if kept.iter().all(|k| k.call != decision.call) {
                kept.push(decision.clone());
            }
        }
        if kept.len() == before {
            return Ok(());
        }
        self.revision += 1;
        write(&self.path.with_file_name("decisions.json"), &self.decisions)
    }

    /// Keeps `summary` of `request`'s turn, and saves the summaries.
    pub fn add_summary(&mut self, request: SummaryRequest, summary: TurnSummary) -> io::Result<()> {
        self.add_summary_at(request, summary, millis())
    }

    /// `add_summary` at `at`, in Unix milliseconds.
    pub fn add_summary_at(
        &mut self,
        request: SummaryRequest,
        summary: TurnSummary,
        at: u64,
    ) -> io::Result<()> {
        self.add_summary_with(request, summary, at, coin())
    }

    /// Starts the self-test at `start`, in Unix milliseconds, or ends it with `None` (danielrispler/plotkeeper#130).
    pub fn set_self_test(&mut self, start: Option<u64>) {
        self.self_test = start;
    }

    /// `add_summary_at` with the self-test's coin given: heads is body-only. A turn with a Quiz written in the test's 21
    /// days gets an arm; any other turn none, so outside the window no turn is body-only.
    pub fn add_summary_with(
        &mut self,
        request: SummaryRequest,
        summary: TurnSummary,
        at: u64,
        heads: bool,
    ) -> io::Result<()> {
        let arm = (self.self_test)
            .filter(|start| (*start..*start + SELF_TEST_MS).contains(&at))
            .filter(|_| summary.quiz.is_some())
            .map(|_| if heads { Arm::BodyOnly } else { Arm::QuizFirst });
        let hunk = (request.facts.files.iter())
            .map(|f| format!("--- {}\n{}", f.file.path, f.hunk))
            .collect();
        self.summaries.push(StoredSummary {
            request,
            summary,
            added_at: at,
            taps: Vec::new(),
            hunk,
            arm,
        });
        self.revision += 1;
        write(&self.path.with_file_name("summaries.json"), &self.summaries)
    }

    /// The turn summaries, oldest first.
    pub fn summaries(&self) -> &[StoredSummary] {
        &self.summaries
    }

    /// The summary of turn `id` (`SummaryRequest::id`), when it was written.
    pub fn turn_summary(&self, id: &str) -> Option<&StoredSummary> {
        self.summaries.iter().find(|kept| kept.request.id == id)
    }

    /// Keeps `summary` of subagent `id`, and saves the subagent summaries.
    pub fn add_agent_summary(&mut self, id: &str, summary: &str) -> io::Result<()> {
        self.agent_summaries.insert(id.into(), summary.into());
        write(
            &self.path.with_file_name("agent-summaries.json"),
            &self.agent_summaries,
        )
    }

    /// The summary of subagent `id`, when it was written.
    pub fn agent_summary(&self, id: &str) -> Option<&str> {
        self.agent_summaries.get(id).map(String::as_str)
    }

    /// Session `session`'s latest `n` summaries, oldest first.
    pub fn latest_summaries(&self, session: &str, n: usize) -> Vec<TurnSummary> {
        let its = self
            .summaries
            .iter()
            .filter(|kept| kept.request.session_id == session);
        let mut latest: Vec<TurnSummary> =
            its.rev().take(n).map(|kept| kept.summary.clone()).collect();
        latest.reverse();
        latest
    }

    /// The live-only facts kept so far.
    pub fn facts(&self) -> &LiveFacts {
        &self.facts
    }

    /// Records that helper `id` became a worker (`Output::Promoted`), and saves the facts.
    pub fn promoted(&mut self, id: &str) -> io::Result<()> {
        if self.facts.promoted.insert(id.into()) {
            return self.save_facts();
        }
        Ok(())
    }

    /// Records that Daniel opened failed line `id` (`Studio::open`), and saves the facts.
    pub fn opened(&mut self, id: &str) -> io::Result<()> {
        if self.facts.opened.insert(id.into()) {
            return self.save_facts();
        }
        Ok(())
    }

    /// Records the turns of session `id` Daniel has seen (`Studio::see_selected`,
    /// `Studio::mark_unseen`), and saves the facts.
    pub fn seen(&mut self, id: &str, turns: usize) -> io::Result<()> {
        if self.facts.seen.insert(id.into(), turns) != Some(turns) {
            return self.save_facts();
        }
        Ok(())
    }

    /// Applies the live-only facts and the cards to `studio` after its replay (danielrispler/Falinks#434, decisions 37
    /// and 38).
    pub fn restore(&self, studio: &mut Studio) {
        let requests: Vec<CardRequest> = self.cards.iter().map(|k| k.request.clone()).collect();
        studio.restore(&self.facts, &requests);
    }

    fn save_facts(&self) -> io::Result<()> {
        write(&self.path.with_file_name("live-facts.json"), &self.facts)
    }

    /// Card `id`, to record on.
    fn card_mut(&mut self, id: &str) -> Option<&mut StoredCard> {
        self.cards.iter_mut().find(|kept| kept.request.id == id)
    }

    /// Saves the cards after a change, which raises the revision.
    fn save(&mut self) -> io::Result<()> {
        self.revision += 1;
        write(&self.path, &self.cards)
    }
}

/// At most this many Quizzes are offered in any rolling hour, across the studio (danielrispler/plotkeeper#119).
pub const QUIZZES_PER_HOUR: usize = 3;
const HOUR_MS: u64 = 60 * 60 * 1000;

/// Where a Quiz about to be stored goes under the cap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// It is offered.
    Offer,
    /// It is not: the hour holds its 3 already.
    Withhold,
    /// A card's Quiz takes the place of the lone Quiz of the summary of this turn, which loses it.
    Replace(String),
}

impl CardStore {
    /// The Quizzes offered in the hour before `now`: the cards' and the lone ones, a check card's
    /// apart, answered or not.
    fn offered(&self, now: u64) -> usize {
        let since = now.saturating_sub(HOUR_MS);
        let cards = self.cards.iter().filter(|k| k.request.check.is_none());
        let cards =
            cards.filter(|k| k.card.quiz.is_some() && k.added_at.is_some_and(|at| at > since));
        let lone = self.summaries.iter().filter(|k| {
            k.summary.quiz.is_some() && k.arm != Some(Arm::BodyOnly) && k.added_at > since
        });
        cards.count() + lone.count()
    }

    /// Where a Quiz stored at `now` goes: a card's (`card`) comes before a lone one, which is offered
    /// only while fewer than `QUIZZES_PER_HOUR` were offered in the last hour. A card's past the cap
    /// takes the place of the oldest unanswered lone Quiz of that hour, and is otherwise not offered.
    pub fn place_quiz(&self, card: bool, now: u64) -> Place {
        if self.offered(now) < QUIZZES_PER_HOUR {
            return Place::Offer;
        }
        let since = now.saturating_sub(HOUR_MS);
        let open = |k: &&StoredSummary| {
            k.summary.quiz.is_some()
                && k.arm != Some(Arm::BodyOnly)
                && !k.taps.iter().any(QuizTap::answers)
                && k.added_at > since
        };
        match self.summaries.iter().find(open) {
            Some(oldest) if card => Place::Replace(oldest.request.id.clone()),
            _ => Place::Withhold,
        }
    }

    /// Takes the Quiz from the summary of turn `id`, and saves the summaries.
    pub fn drop_summary_quiz(&mut self, id: &str) -> io::Result<()> {
        let found = self.summaries.iter_mut().find(|k| k.request.id == id);
        if let Some(kept) = found.filter(|k| k.summary.quiz.is_some()) {
            kept.summary.quiz = None;
            self.revision += 1;
            write(&self.path.with_file_name("summaries.json"), &self.summaries)?;
        }
        Ok(())
    }

    /// Takes the Quiz from card `id`, which then shows without it, and saves the store.
    pub fn drop_card_quiz(&mut self, id: &str) -> io::Result<()> {
        match self.card_mut(id).filter(|k| k.card.quiz.is_some()) {
            Some(kept) => {
                kept.card.quiz = None;
                self.save()
            }
            None => Ok(()),
        }
    }
}

/// Writes `value` to a file beside `path` and renames it over `path`, so an app that dies mid-write
/// leaves the previous file whole.
fn write(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let written = path.with_extension("json.tmp");
    let mut file = File::create(&written)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(written, path)
}
