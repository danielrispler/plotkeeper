//! The card store (#328, module 6): the cards, Daniel's guesses and their records, on disk in the
//! studio's data directory, so a restart neither loses nor re-asks them.

use crate::Card;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
use studio_core::CardRequest;

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

/// One session's cards in the session view's two groups (#369, decision 18), each oldest first.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SessionCards {
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

/// The store's cards, kept in memory and written to its file on each change.
// shortcut: rewrites the whole file on each change, which suits a few cards a day; switch to
// appended lines if it grows to thousands of cards.
pub struct CardStore {
    path: PathBuf,
    cards: Vec<StoredCard>,
}

impl CardStore {
    /// Reads the store at `path`. A missing store is empty. One that does not parse is an error, so
    /// it is never overwritten.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let cards = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(CardStore { path, cards })
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

    /// Session `session_id`'s cards, split 30 minutes before `now`, the shell's tick.
    pub fn session_cards(&self, session_id: &str, now: u64) -> SessionCards {
        let cards = self
            .cards
            .iter()
            .filter(|kept| kept.request.session_id == session_id);
        let (recent, older) = cards.cloned().partition(|kept| {
            kept.added_at
                .is_some_and(|at| now.saturating_sub(at) < RECENT_MS)
        });
        SessionCards { recent, older }
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
        let mut cards = self.cards.iter_mut();
        if let Some(kept) = cards.find(|kept| kept.request.id == id)
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
        let mut cards = self.cards.iter_mut();
        if let Some(kept) = cards.find(|kept| kept.request.id == id)
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
        let mut cards = self.cards.iter_mut();
        if let Some(kept) = cards.find(|kept| kept.request.id == id)
            && !kept.key_wrong
        {
            kept.key_wrong = true;
            return self.save();
        }
        Ok(())
    }

    /// Records Daniel's rating of card `id`, which replaces the one before, and saves the store.
    pub fn rate(&mut self, id: &str, rating: Rating) -> io::Result<()> {
        let mut cards = self.cards.iter_mut();
        if let Some(kept) = cards.find(|kept| kept.request.id == id) {
            kept.rating = Some(rating);
            return self.save();
        }
        Ok(())
    }

    /// Keeps `more`, the writer's longer explanation of card `id`, unless it has one, and saves the
    /// store.
    pub(crate) fn more(&mut self, id: &str, more: String) -> io::Result<()> {
        let mut cards = self.cards.iter_mut();
        if let Some(kept) = cards.find(|kept| kept.request.id == id)
            && kept.more.is_none()
        {
            kept.more = Some(more);
            return self.save();
        }
        Ok(())
    }

    /// Writes a file beside the store and renames it over the store, so an app that dies mid-write
    /// leaves the previous store whole.
    fn save(&self) -> io::Result<()> {
        let written = self.path.with_extension("json.tmp");
        let mut file = File::create(&written)?;
        file.write_all(&serde_json::to_vec(&self.cards)?)?;
        file.sync_all()?;
        fs::rename(written, &self.path)
    }
}
