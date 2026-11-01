//! The card store (#328, module 6): the cards, Daniel's guesses and their records, and each
//! session's brief, on disk in the studio's data directory, so a restart neither loses nor re-asks
//! them.

use crate::Card;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use studio_core::{CardRequest, Session};

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

/// Daniel's tap on a card's quiz, graded by the writer's key (#369, decision 13).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Guess {
    /// The option he tapped, counting from 0.
    pub option: usize,
    /// Whether it is the key.
    pub right: bool,
    /// When, in Unix milliseconds.
    pub at: u64,
}

/// A session list row: the session, and what its row says while its turn is over (`CardStore::line`).
#[derive(Clone, Debug, Serialize)]
pub struct Row {
    #[serde(flatten)]
    pub session: Session,
    pub line: Option<String>,
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
    pub recent: Vec<StoredCard>,
    /// The cards added before, or before the store kept the time.
    pub older: Vec<StoredCard>,
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
        Ok(CardStore {
            path,
            cards,
            briefs,
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
        SessionCards {
            brief,
            recent,
            older,
        }
    }

    /// What `session`'s row says while its turn is over (#369, decision 19): the summary of the card
    /// on its `Stop`; the first line of the agent's last message until that card arrives; the brief's
    /// latest line when the `Stop` requested no card. `None` in a turn.
    pub fn line(&self, session: &Session) -> Option<String> {
        let stopped = session.stopped.as_ref()?;
        let card = self
            .cards
            .iter()
            .find(|kept| kept.request.id == stopped.card);
        let summary = card.map(|kept| kept.card.summary.clone());
        let brief = self.brief(&session.id).and_then(|b| b.text.lines().last());
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
        });
        self.save()
    }

    /// Marks the quiz of card `id` shown at `at`, in Unix milliseconds, and saves the store. A card
    /// keeps its first time, and one guessed before gets none, so a guess never comes before its
    /// quiz.
    pub fn show(&mut self, id: &str, at: u64) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id)
            && kept.shown_at.is_none()
            && kept.guess.is_none()
        {
            kept.shown_at = Some(at);
            return self.save();
        }
        Ok(())
    }

    /// Records Daniel's tap on option `option` of card `id`'s quiz at `at`, in Unix milliseconds,
    /// graded by its key, and saves the store. The first tap counts; a card with no quiz, or no such
    /// option, records none.
    pub fn guess(&mut self, id: &str, option: usize, at: u64) -> io::Result<()> {
        if let Some(kept) = self.card_mut(id)
            && kept.guess.is_none()
            && let Some(quiz) = &kept.card.quiz
            && option < quiz.options.len()
        {
            let right = option == quiz.key;
            kept.guess = Some(Guess { option, right, at });
            return self.save();
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

    /// Card `id`, to record on.
    fn card_mut(&mut self, id: &str) -> Option<&mut StoredCard> {
        self.cards.iter_mut().find(|kept| kept.request.id == id)
    }

    fn save(&self) -> io::Result<()> {
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
