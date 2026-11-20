//! Turn writing (danielrispler/plotkeeper#250): what a turn's end writes, in one place. The shell hands this module the requests the
//! core emits and an [`Effects`] for what only it can do; the offer, the writer calls, the store and
//! the order of a session's pieces are behind it.
//!
//! Each session has a queue of its own: its jobs run one at a time, first in first out, so a parent's
//! card reads its workers' cards and two writes never update one brief at once; sessions write in
//! parallel. [`TurnWriting::stepped`] has no threads, for tests that run the jobs in order themselves.

use crate::{
    CardStore, CardWriter, Changed, Decision, Offer, Offers, StoredSummary, SummarySetup,
    backfilled, changed_in, offer_stop, offer_summary, settle, turn_of, turns_card, write_design,
    write_goal, write_name,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{Arc, Mutex, mpsc},
    thread,
};
use studio_core::{Backfill, CardRequest, NameRequest, Session, Studio, SummaryRequest};

/// What only the shell can do, called from the module's jobs with no lock of the core or the store held.
pub trait Effects: Send + Sync {
    /// Sends the session list: a row's line changed with no line read.
    fn list(&self);
    /// Raises the "New card" notice on `session_id`'s line, naming its subagent `agent`, if any.
    fn new_card(&self, session_id: &str, agent: Option<&str>);
    /// Settles the guesses on the cards of the checks that ended, before a new card is drawn.
    fn settle_checks(&self);
    /// Appends the line that names session `session_id` `name` to the event log.
    fn name(&self, session_id: &str, name: &str);
    /// Logs a problem, as one line, as the understanding layer does everywhere.
    fn log(&self, line: String);
}

/// A writer call, run on its session's queue.
type Job = Box<dyn FnOnce() + Send>;

/// The card writer, as the shell holds it.
pub type Writer = Arc<dyn CardWriter + Send + Sync>;

/// The core, which the shell and this module lock for one statement at a time.
pub type Core = Arc<Mutex<Studio>>;

enum Queues {
    /// One thread for each session with jobs.
    Threads(Mutex<HashMap<String, mpsc::Sender<Job>>>),
    /// Jobs wait for [`TurnWriting::run_queued`].
    Stepped(Mutex<VecDeque<Job>>),
}

/// Turn writing: see the module's description.
pub struct TurnWriting {
    state: Arc<State>,
    queues: Queues,
}

struct State {
    core: Core,
    store: Arc<Mutex<CardStore>>,
    writer: Writer,
    offers: Offers,
    effects: Arc<dyn Effects>,
    /// The summary writer's setup, from the settings the window sends.
    setup: Mutex<SummarySetup>,
    /// The turns whose summary waits for Daniel to open it while summaries are written on open, by
    /// turn id.
    // shortcut: the waiting turns live in memory, so a restart loses them; keep them in the store if
    // that bites.
    waiting: Mutex<HashMap<String, SummaryRequest>>,
    /// The files of each turn card's own change, for a summary of an agent that reports no trees: it is
    /// judged on the same change as its card (danielrispler/plotkeeper#209).
    changes: Mutex<HashMap<String, Vec<Changed>>>,
    /// What the card path decided for each live turn card, by turn id, until the turn's summary takes it:
    /// one decision for a turn (danielrispler/plotkeeper#209), however late its summary runs.
    decided: Mutex<HashMap<String, Decision>>,
    /// The turns whose decision was a lone Quiz, by turn id, until their summary puts it on record.
    lone: Mutex<HashMap<String, Decision>>,
}

impl TurnWriting {
    /// Turn writing over `core`, `store` and `writer`, with a thread for each session's queue.
    pub fn new(
        core: Core,
        store: Arc<Mutex<CardStore>>,
        writer: Writer,
        offers: Offers,
        effects: Arc<dyn Effects>,
    ) -> TurnWriting {
        let queues = Queues::Threads(Mutex::default());
        Self::build(core, store, writer, offers, effects, queues)
    }

    /// Turn writing with no threads: a job waits until [`TurnWriting::run_queued`] runs it, through
    /// the code a queue's thread runs.
    pub fn stepped(
        core: Core,
        store: Arc<Mutex<CardStore>>,
        writer: Writer,
        offers: Offers,
        effects: Arc<dyn Effects>,
    ) -> TurnWriting {
        let queues = Queues::Stepped(Mutex::default());
        Self::build(core, store, writer, offers, effects, queues)
    }

    fn build(
        core: Core,
        store: Arc<Mutex<CardStore>>,
        writer: Writer,
        offers: Offers,
        effects: Arc<dyn Effects>,
        queues: Queues,
    ) -> TurnWriting {
        let state = State {
            core,
            store,
            writer,
            offers,
            effects,
            setup: Mutex::default(),
            waiting: Mutex::default(),
            changes: Mutex::default(),
            decided: Mutex::default(),
            lone: Mutex::default(),
        };
        TurnWriting {
            state: Arc::new(state),
            queues,
        }
    }

    /// Runs the queued jobs, oldest first, on this thread; returns how many ran. Only a stepped
    /// module holds jobs for it.
    pub fn run_queued(&self) -> usize {
        let Queues::Stepped(jobs) = &self.queues else {
            return 0;
        };
        let mut ran = 0;
        loop {
            let job = jobs.lock().unwrap().pop_front();
            let Some(job) = job else { return ran };
            job();
            ran += 1;
        }
    }

    /// The card `request` asks for, on its session's queue; `live` is false for a card of a turn that
    /// ended while the app was down. `claude -p` takes seconds, so the caller goes on.
    pub fn card(&self, request: CardRequest, live: bool) {
        self.write(request, true, live);
    }

    /// Only the Goal of `request`'s owner, with no card.
    pub fn brief(&self, request: CardRequest) {
        self.write(request, false, true);
    }

    /// The cards of the turns that ended while the app was down (danielrispler/Falinks#486), but for those already on
    /// record: a turn decided, a refused one too, is not asked again. No summary follows such a card,
    /// so nothing is kept for one.
    pub fn backfill(&self, turns: Vec<Backfill>) {
        let recorded = &self.state.offers;
        let turns = turns
            .into_iter()
            .filter(|b| !recorded.settled(&b.request.session_id, &b.request.id));
        for request in turns.filter_map(backfilled) {
            self.write(request, true, false);
        }
    }

    /// The summary of `request`'s turn, on its session's queue after the turn's card. While summaries are
    /// written on open, the turn waits for Daniel instead ([`TurnWriting::opened`]).
    pub fn summary(&self, request: SummaryRequest) {
        let on_open = self.state.core.lock().unwrap().summaries_on_open();
        let id = request.session_id.clone();
        let state = self.state.clone();
        let job: Job = match on_open {
            true => Box::new(move || state.park(request)),
            false => Box::new(move || state.write_summary(request)),
        };
        self.queue(id, job);
    }

    /// The name of the session of `request`, from Daniel's first prompt, on its queue.
    pub fn name(&self, request: NameRequest) {
        let state = self.state.clone();
        let id = request.session_id.clone();
        self.queue(id, Box::new(move || state.write_name(&request)));
    }

    /// Daniel opened turn `id`: its summary, written now when it waits for him, which takes seconds, so
    /// the caller must run off its main thread. `None` when the turn has none. A failed write keeps the
    /// turn waiting and is an error, so the window asks again only at the next opening.
    pub fn opened(&self, id: &str) -> Result<Option<StoredSummary>, String> {
        self.state.opened(id)
    }

    /// The summary settings changed: the writer's model and effort, and whether a summary is written
    /// when Daniel opens its turn instead of at the turn's end.
    pub fn settings(&self, setup: SummarySetup, on_open: bool) {
        *self.state.setup.lock().unwrap() = setup;
        self.state
            .core
            .lock()
            .unwrap()
            .set_summaries_on_open(on_open);
    }

    /// The summary writer's setup, for the writes that stay the shell's calls.
    pub fn setup(&self) -> SummarySetup {
        self.state.setup.lock().unwrap().clone()
    }

    /// The sessions that are live: a queue of any other lets its jobs finish, then ends its thread.
    pub fn prune(&self, live: &HashSet<String>) {
        if let Queues::Threads(queues) = &self.queues {
            queues.lock().unwrap().retain(|key, _| live.contains(key));
        }
    }

    fn write(&self, mut request: CardRequest, card: bool, live: bool) {
        let state = self.state.clone();
        let id = request.session_id.clone();
        let Some(session) = state.core.lock().unwrap().session(&id).cloned() else {
            return;
        };
        // Only a live turn's own card has a summary to come, which takes what is kept here and clears it.
        let turn_card = card && live && turns_card(&session, &request);
        if turn_card {
            let files = changed_in(request.diff());
            let mut changes = state.changes.lock().unwrap();
            changes.insert(request.id.clone(), files);
        }
        let job = move || state.write_stop(&mut request, &session, card, turn_card);
        self.queue(id, Box::new(job));
    }

    /// Adds `job` to session `id`'s queue, which starts when it has none.
    fn queue(&self, id: String, job: Job) {
        match &self.queues {
            Queues::Stepped(jobs) => jobs.lock().unwrap().push_back(job),
            Queues::Threads(queues) => {
                let mut queues = queues.lock().unwrap();
                let queue = queues.entry(id).or_insert_with(|| {
                    let (queue, jobs) = mpsc::channel::<Job>();
                    thread::spawn(move || jobs.into_iter().for_each(|job| job()));
                    queue
                });
                let _ = queue.send(job);
            }
        }
    }
}

impl State {
    /// A card job: the card, or only the Goal when `card` is false, then Design so far.
    fn write_stop(
        &self,
        request: &mut CardRequest,
        session: &Session,
        card: bool,
        turn_card: bool,
    ) {
        let log = |line| self.effects.log(line);
        // Where the card's code is pulled from: its turn's end trees (danielrispler/plotkeeper#121).
        request.ends = self.core.lock().unwrap().card_ends(request);
        let agent = request.agent.clone();
        // The list and the notice go out once the card is stored, before a brief rewrite.
        let stored = || {
            // A check that ended while its card was written settles it before the pane draws it.
            self.effects.settle_checks();
            self.effects.list();
            self.effects.new_card(&session.agent.id, agent.as_deref());
        };
        let card_id = request.id.clone();
        // A turn the table gives no card is written none, but still feeds the design.
        let written = match card {
            true => {
                let turn = turn_of(&self.core, &request.id, &mut |w| log(w));
                let (written, decided) = offer_stop(
                    &self.store,
                    &*self.writer,
                    request.clone(),
                    session,
                    &turn,
                    &self.offers,
                    turn_card,
                    stored,
                    log,
                );
                if let Some(decided) = decided.filter(|_| turn_card) {
                    self.decided
                        .lock()
                        .unwrap()
                        .insert(card_id.clone(), decided);
                }
                written
            }
            false => write_goal(&self.store, &*self.writer, request, session, log),
        };
        // A Quiz is never beside a decision (danielrispler/plotkeeper#119).
        if card && session.needs_you() {
            self.unsaved(self.store.lock().unwrap().drop_card_quiz(&card_id));
        }
        let designed = self.design(&session.agent.id);
        // The brief, which a row's line may show, changed again.
        if written || designed {
            self.effects.list();
        }
    }

    /// Writes "Design so far" of session `id` when its sources changed (danielrispler/plotkeeper#123). Returns whether it did.
    fn design(&self, id: &str) -> bool {
        let turns: Vec<String> = match self.core.lock().unwrap().session(id) {
            Some(session) => session.turns.iter().filter_map(|t| t.id.clone()).collect(),
            None => return false,
        };
        write_design(&self.store, &*self.writer, id, &turns, |l| {
            self.effects.log(l);
        })
    }

    fn unsaved(&self, saved: std::io::Result<()>) {
        if let Err(e) = saved {
            self.effects.log(format!("save the card store: {e}"));
        }
    }

    /// Fills in `request`'s facts and, by the one offer function (danielrispler/plotkeeper#209), whether its summary may
    /// keep a Quiz. Returns whether the summary is written: one asked for the sake of a lone Quiz is dropped when the
    /// turn gets none.
    fn settle(&self, request: &mut SummaryRequest) -> bool {
        let change = self.changes.lock().unwrap().remove(&request.id);
        let decided = self.decided.lock().unwrap().remove(&request.id);
        let turn = turn_of(&self.core, &request.id, &mut |w| self.effects.log(w));
        let settled = settle(
            request,
            &turn,
            &change.unwrap_or_default(),
            decided.as_ref(),
        );
        settled.record(&self.offers, request);
        if settled.decision.offer == Offer::Quiz {
            let mut lone = self.lone.lock().unwrap();
            lone.insert(request.id.clone(), settled.decision);
        }
        settled.write
    }

    /// The summary job of a summary written at the turn's end.
    fn write_summary(&self, mut request: SummaryRequest) {
        let setup = self.setup.lock().unwrap().clone();
        if !self.settle(&mut request) {
            return;
        }
        let session = request.session_id.clone();
        let lone = self.lone.lock().unwrap().remove(&request.id);
        let written = offer_summary(
            &self.store,
            &*self.writer,
            &setup,
            request,
            lone.as_ref(),
            &self.offers,
            |l| self.effects.log(l),
        );
        if written.is_some() {
            self.design(&session);
        }
    }

    /// The summary job of a summary that waits for Daniel to open its turn.
    fn park(&self, mut request: SummaryRequest) {
        if self.settle(&mut request) {
            self.waiting
                .lock()
                .unwrap()
                .insert(request.id.clone(), request);
            self.store.lock().unwrap().waiting_summary();
        }
    }

    fn opened(&self, id: &str) -> Result<Option<StoredSummary>, String> {
        if let Some(kept) = self.store.lock().unwrap().turn_summary(id) {
            return Ok(Some(kept.clone()));
        }
        let Some(request) = self.waiting.lock().unwrap().remove(id) else {
            return Ok(None);
        };
        let setup = self.setup.lock().unwrap().clone();
        let lone = self.lone.lock().unwrap().remove(id);
        let written = offer_summary(
            &self.store,
            &*self.writer,
            &setup,
            request.clone(),
            lone.as_ref(),
            &self.offers,
            |l| self.effects.log(l),
        );
        if written.is_none() {
            if let Some(lone) = lone {
                self.lone.lock().unwrap().insert(id.to_owned(), lone);
            }
            self.waiting.lock().unwrap().insert(id.to_owned(), request);
            return Err(format!("no summary on {id}"));
        }
        self.design(&request.session_id);
        Ok(self.store.lock().unwrap().turn_summary(id).cloned())
    }

    fn write_name(&self, request: &NameRequest) {
        let setup = self.setup.lock().unwrap().clone();
        let response = request.response.as_deref();
        let name = write_name(&*self.writer, &setup, &request.prompt, response, |l| {
            self.effects.log(l);
        });
        if let Some(name) = name {
            self.effects.name(&request.session_id, &name);
        }
    }
}
