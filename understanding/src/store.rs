//! The card store (#328, module 6): the cards, Daniel's guesses and their records, and each
//! session's brief, on disk in the studio's data directory, so a restart neither loses nor re-asks
//! them.

use crate::{Card, TurnSummary};
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
    /// Daniel flagged that the writer's key is wrong (#369, decision 13), from the quiz or from his
    /// rating: one record.
    #[serde(default)]
    pub key_wrong: bool,
    /// The writer's longer explanation, from Daniel's first "explain more" (#369, decision 15).
    #[serde(default)]
    pub more: Option<String>,
    /// Daniel's rating of the explanation (#369, decision 15); `None` until he rates it.
    #[serde(default)]
    pub rating: Option<Rating>,
    /// The result of the check whose start requested the card (#461, decision 15), which settles
    /// its guess; `None` while it runs, and on a card no check requested.
    #[serde(default)]
    pub check_result: Option<CheckResult>,
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

/// A turn summary in the store (#526), with the turn it is on and when it arrived.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredSummary {
    pub request: SummaryRequest,
    pub summary: TurnSummary,
    pub added_at: u64,
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

/// Daniel's tap on a card's quiz, graded by the writer's key (#369, decision 13), or on a check's card
/// by the check's result (#461, decision 15).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guess {
    /// The option he tapped, counting from 0.
    pub option: usize,
    /// Whether it is the key. On a check's card, whether it called the check's result: `None` until
    /// the result settles it, and for good when the result does not say (`StoredCard::check_result`).
    /// A store from before #476 holds a plain `true` or `false`, which loads as `Some`.
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

/// A session's or a worker's rolling brief (#369, decision 16).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Brief {
    pub text: String,
    /// How many times it was updated, so that every 5th update rewrites it.
    pub updates: u32,
}

/// One session's brief and its cards in the session view's two groups (#369, decision 18), each
/// oldest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SessionCards {
    pub brief: Option<String>,
    /// The cards added in the last 30 minutes.
    #[serde(serialize_with = "without_diffs")]
    pub recent: Vec<StoredCard>,
    /// The cards added before, or before the store kept the time.
    #[serde(serialize_with = "without_diffs")]
    pub older: Vec<StoredCard>,
    /// Its decision history, oldest first (#461, decision 17).
    pub decisions: Vec<Decision>,
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

fn without_diffs<S: serde::Serializer>(cards: &[StoredCard], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(cards.iter().map(|kept| ShownCard {
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
    }))
}

/// The last 30 minutes (#369, decision 18).
const RECENT_MS: u64 = 30 * 60 * 1000;

/// Now, in Unix milliseconds, as the store's records keep time.
pub fn millis() -> u64 {
    UNIX_EPOCH.elapsed().unwrap_or_default().as_millis() as u64
}

/// The store's cards, and its briefs by session or worker, kept in memory and written to their files on each
/// change: the briefs to `briefs.json` beside the cards (#369, decision 17).
// shortcut: rewrites the whole file on each change, which suits a few cards a day; switch to
// appended lines if it grows to thousands of cards.
pub struct CardStore {
    path: PathBuf,
    cards: Vec<StoredCard>,
    briefs: BTreeMap<String, Brief>,
    /// What the live app learned that the event log does not hold, in `live-facts.json` beside the
    /// cards (#434, decision 37).
    facts: LiveFacts,
    /// Each session's decision history, by its row's id, in `decisions.json` beside the cards (#461,
    /// decision 17).
    decisions: BTreeMap<String, Vec<Decision>>,
    /// The turn summaries, oldest first, in `summaries.json` beside the cards (#526).
    summaries: Vec<StoredSummary>,
    /// The subagent summaries, by subagent id, in `agent-summaries.json` beside the cards (#535).
    agent_summaries: BTreeMap<String, String>,
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
        let agent_summaries = read(&path.with_file_name("agent-summaries.json"))?;
        Ok(CardStore {
            path,
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
    /// store from before cards covered a prompt's whole change (#369, decision 17) is moved aside to
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

    /// The cards, oldest first.
    pub fn cards(&self) -> &[StoredCard] {
        &self.cards
    }

    /// The cards and brief of `owner`, a session's id, or a worker's for its own (#369, decision 11),
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
        let brief = self.brief(owner).map(|brief| brief.text.clone());
        // A session's history holds its workers' decisions; a worker's, its own.
        let decisions = self.decisions.iter().flat_map(|(session, kept)| {
            let its = move |d: &&Decision| session == owner || d.agent.as_deref() == Some(owner);
            kept.iter().filter(its)
        });
        SessionCards {
            brief,
            recent,
            older,
            decisions: decisions.cloned().collect(),
        }
    }

    /// What `session`'s row says at `now`, in Unix milliseconds (#410, decision 27): while it needs
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

    /// What `session`'s row says while its turn is over (#369, decision 19): the summary of the card
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

    /// Updates `owner`'s brief to `text`, and saves the briefs.
    pub fn set_brief(&mut self, owner: &str, text: String) -> io::Result<()> {
        let brief = self.briefs.entry(owner.into()).or_default();
        brief.text = text;
        brief.updates += 1;
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

    /// Settles the guess on card `id`, a check's, with the check's result (#461, decision 15):
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
            // shortcut: an exit code that does not say leaves the guess not counted, where #343 let
            // the writer's `passed` settle it: the core skips a check's output (#410), and the
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
        self.summaries.push(StoredSummary {
            request,
            summary,
            added_at: millis(),
        });
        self.revision += 1;
        write(&self.path.with_file_name("summaries.json"), &self.summaries)
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

    /// Applies the live-only facts and the cards to `studio` after its replay (#434, decisions 37
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

/// Writes `value` to a file beside `path` and renames it over `path`, so an app that dies mid-write
/// leaves the previous file whole.
fn write(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let written = path.with_extension("json.tmp");
    let mut file = File::create(&written)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(written, path)
}
