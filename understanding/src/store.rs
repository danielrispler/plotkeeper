//! The card store (danielrispler/Falinks#328, module 6): the cards, Daniel's guesses and their records, and each
//! session's brief, on disk in the studio's data directory, so a restart neither loses nor re-asks
//! them.

use crate::risk::RiskView;
use crate::{Answered, Card, DesignLine, OpenItem, Quiz, TurnSummary, answered};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
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
/// does not say (`CheckCard::passed`), so the guess is not counted.
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

/// A turn's arm in the self-test (danielrispler/plotkeeper#130), retired in danielrispler/plotkeeper#348: the store only reads
/// the ones it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arm {
    QuizFirst,
    BodyOnly,
}

/// Where a confidence tap was made (danielrispler/plotkeeper#242): Home's learning slot, or the rail, which a tap stored before
/// the field existed reads as. The rail's taps are written without it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    #[default]
    Rail,
    Home,
}

impl Surface {
    #[allow(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `skip_serializing_if` passes the field by reference"
    )]
    fn is_rail(&self) -> bool {
        *self == Surface::Rail
    }
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
        #[serde(default, skip_serializing_if = "Surface::is_rail")]
        surface: Surface,
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
    /// "Not now" (danielrispler/plotkeeper#240): Daniel set the Quiz aside from Home's learning slot. It is not an answer, and
    /// each tap is kept, one per day it was tapped.
    NotNow,
    /// A tap of a kind this build does not know, such as one a later build wrote: it reads, and counts as nothing, so a later
    /// tap kind never makes this build refuse the store.
    #[serde(other)]
    Unknown,
}

/// A tap and when, in Unix milliseconds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuizTap {
    #[serde(flatten)]
    pub tap: Tap,
    pub at: u64,
}

/// A fresh Quiz on the decision of an earlier one, which Home offered a week after it (danielrispler/plotkeeper#129). Retests
/// are retired (danielrispler/plotkeeper#347): none is written or served now, and the store only reads the ones it holds.
/// `quiz` is `None` when the writer's replies broke the rules twice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Retest {
    /// `retest ` and the id of the Quiz it retests, `of`.
    pub id: String,
    pub of: String,
    pub session_id: String,
    pub added_at: u64,
    pub quiz: Option<Quiz>,
    #[serde(default)]
    pub taps: Vec<QuizTap>,
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

/// One session's brief, its decision history and its blocked commands' risk notes, which the session view shows.
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
    /// Its decision history, oldest first (danielrispler/Falinks#461, decision 17).
    pub decisions: Vec<Decision>,
    /// What each blocked command's card shows of its risk note (danielrispler/plotkeeper#298), which the shell joins
    /// to the cards when it reads them (`RiskNotes::views`); empty in a worker's own view.
    pub risk_notes: Vec<RiskView>,
    /// The note of each Bash decision in the history, by the decision's call, for the window to compare with its time
    /// (danielrispler/plotkeeper#299); the stored decision holds none.
    pub decision_notes: Vec<RiskView>,
}

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
    /// Rises on every change, briefs included, so the shell sends the cards only when it moves.
    revision: u64,
}

/// The JSON file at `path`, or the default when it is missing. One that does not parse is an
/// error, so it is never overwritten.
fn read<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> io::Result<T> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        }),
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

    /// The brief, decisions and risk-note slots of `owner`, a session's id, or a worker's for its own
    /// (danielrispler/Falinks#369, decision 11).
    pub fn session_cards(&self, owner: &str) -> SessionCards {
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
            decisions,
            risk_notes: Vec::new(),
            decision_notes: Vec::new(),
        }
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

    /// The retests the store holds, oldest first: none is written now (danielrispler/plotkeeper#347).
    pub fn retests(&self) -> &[Retest] {
        &self.retests
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
        let hunk = (request.facts.files.iter())
            .map(|f| format!("--- {}\n{}", f.file.path, f.hunk))
            .collect();
        self.summaries.push(StoredSummary {
            request,
            summary,
            added_at: at,
            taps: Vec::new(),
            hunk,
            arm: None,
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
