//! Turn writing (danielrispler/plotkeeper#250): what a turn's end writes, in one place. The shell hands this module the requests the
//! core emits and an [`Effects`] for what only it can do; the writer calls, the store and the order of a
//! session's pieces are behind it. A turn's end writes its summary, the session's Goal and Design so far, and no
//! card: cards are retired (danielrispler/plotkeeper#348).
//!
//! Each session has a queue of its own: its jobs run one at a time, first in first out, so two writes never
//! update one brief at once; sessions write in parallel. [`TurnWriting::stepped`] has no threads, for tests that run the
//! jobs in order themselves.

use crate::{
    CardStore, CardWriter, StoredSummary, SummarySetup, turn_facts, write_design, write_goal,
    write_name, write_summary,
};
use std::{
    collections::{HashMap, VecDeque},
    panic::{self, AssertUnwindSafe},
    sync::{Arc, Mutex},
    thread,
};
use studio_core::{CardRequest, NameRequest, Session, Studio, SummaryRequest};

/// What only the shell can do, called from the module's jobs with no lock of the core or the store held.
pub trait Effects: Send + Sync {
    /// Sends the session list: a row's line changed with no line read.
    fn list(&self);
    /// Appends the line that names session `session_id` `name` to the event log.
    fn name(&self, session_id: &str, name: &str);
    /// Logs a problem, as one line, as the understanding layer does everywhere.
    fn log(&self, line: String);
}

/// A writer call, run on its session's queue.
type Job = Box<dyn FnOnce() + Send>;

/// The writer, as the shell holds it.
pub type Writer = Arc<dyn CardWriter + Send + Sync>;

/// The core, which the shell and this module lock for one statement at a time.
pub type Core = Arc<Mutex<Studio>>;

enum Queues {
    /// The jobs waiting for each session, which has one thread while it has any: the thread ends when
    /// its session's jobs are done, under the lock that adds a job, so no job is lost to an ending thread.
    Threads(Arc<Mutex<HashMap<String, VecDeque<Job>>>>),
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
    effects: Arc<dyn Effects>,
    /// The summary writer's setup, from the settings the window sends.
    setup: Mutex<SummarySetup>,
    /// The turns whose summary waits for Daniel to open it while summaries are written on open, by
    /// turn id.
    // shortcut: the waiting turns live in memory, so a restart loses them; keep them in the store if
    // that bites.
    waiting: Mutex<HashMap<String, SummaryRequest>>,
}

impl TurnWriting {
    /// Turn writing over `core`, `store` and `writer`, with a thread for each session's queue.
    pub fn new(
        core: Core,
        store: Arc<Mutex<CardStore>>,
        writer: Writer,
        effects: Arc<dyn Effects>,
    ) -> TurnWriting {
        let queues = Queues::Threads(Arc::default());
        Self::build(core, store, writer, effects, queues)
    }

    /// Turn writing with no threads: a job waits until [`TurnWriting::run_queued`] runs it, through
    /// the code a queue's thread runs.
    pub fn stepped(
        core: Core,
        store: Arc<Mutex<CardStore>>,
        writer: Writer,
        effects: Arc<dyn Effects>,
    ) -> TurnWriting {
        let queues = Queues::Stepped(Mutex::default());
        Self::build(core, store, writer, effects, queues)
    }

    fn build(
        core: Core,
        store: Arc<Mutex<CardStore>>,
        writer: Writer,
        effects: Arc<dyn Effects>,
        queues: Queues,
    ) -> TurnWriting {
        let state = State {
            core,
            store,
            writer,
            effects,
            setup: Mutex::default(),
            waiting: Mutex::default(),
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

    /// The Goal and Design so far of `request`'s owner, on its session's queue: all a change's request writes now that
    /// cards are retired (danielrispler/plotkeeper#348). `claude -p` takes seconds, so the caller goes on.
    pub fn brief(&self, request: CardRequest) {
        let state = self.state.clone();
        let id = request.session_id.clone();
        // The job reads its session when it runs, not now: it may wait behind earlier writer calls.
        let job = move || {
            if let Some(session) = state.present(&request.session_id) {
                state.write_brief(&request, &session);
            }
        };
        self.queue(id, Box::new(job));
    }

    /// The summary of `request`'s turn, on its session's queue after the turn's Goal. While summaries are
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
        let (summary, designed) = self.state.opened(id)?;
        // Design so far is written on the session's queue, after the summary is on its way back.
        if let Some(session) = designed {
            let state = self.state.clone();
            let owner = session.clone();
            self.queue(
                session,
                Box::new(move || {
                    if state.design(&owner) {
                        state.effects.list();
                    }
                }),
            );
        }
        Ok(summary)
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

    /// How many sessions have a queue, with its thread, now.
    pub fn queues(&self) -> usize {
        match &self.queues {
            Queues::Threads(queues) => queues.lock().unwrap().len(),
            Queues::Stepped(_) => 0,
        }
    }

    /// Adds `job` to session `id`'s queue, which starts when it has none.
    fn queue(&self, id: String, job: Job) {
        match &self.queues {
            Queues::Stepped(jobs) => jobs.lock().unwrap().push_back(job),
            Queues::Threads(queues) => {
                let mut waiting = queues.lock().unwrap();
                if let Some(jobs) = waiting.get_mut(&id) {
                    jobs.push_back(job);
                    return;
                }
                waiting.insert(id.clone(), VecDeque::from([job]));
                let queues = queues.clone();
                let effects = self.state.effects.clone();
                thread::spawn(move || {
                    loop {
                        let job = {
                            let mut waiting = queues.lock().unwrap();
                            match waiting.get_mut(&id).and_then(VecDeque::pop_front) {
                                Some(job) => job,
                                None => {
                                    waiting.remove(&id);
                                    return;
                                }
                            }
                        };
                        // A job that panics is logged and the session's later jobs still run.
                        if panic::catch_unwind(AssertUnwindSafe(job)).is_err() {
                            effects.log(format!("a turn writing job of session {id} panicked"));
                        }
                    }
                });
            }
        }
    }
}

impl State {
    /// Session `id` when it is there and not deleted: a job whose session is gone when it runs ends
    /// without writing. An ended session is not gone.
    fn present(&self, id: &str) -> Option<Session> {
        let session = self.core.lock().unwrap().session(id).cloned();
        session.filter(|s| !s.is_deleted())
    }

    /// A brief job: the Goal, then Design so far.
    fn write_brief(&self, request: &CardRequest, session: &Session) {
        let log = |line| self.effects.log(line);
        let goal = write_goal(&self.store, &*self.writer, request, session, log);
        let designed = self.design(&session.agent.id);
        // The brief, which a row's line may show, changed.
        if goal || designed {
            self.effects.list();
        }
    }

    /// Writes "Design so far" of session `id` when its sources changed (danielrispler/plotkeeper#123). Returns whether it did.
    fn design(&self, id: &str) -> bool {
        let turns: Vec<String> = match self.present(id) {
            Some(session) => session.turns.iter().filter_map(|t| t.id.clone()).collect(),
            None => return false,
        };
        write_design(&self.store, &*self.writer, id, &turns, |l| {
            self.effects.log(l);
        })
    }

    /// Fills in `request`'s facts from its turn's trees. Returns whether the summary is written: only one that is wanted for
    /// itself.
    fn settle(&self, request: &mut SummaryRequest) -> bool {
        request.facts = turn_facts(&self.core, &request.id, &mut |w| self.effects.log(w));
        request.wanted
    }

    /// The summary job of a summary written at the turn's end.
    fn write_summary(&self, mut request: SummaryRequest) {
        if self.present(&request.session_id).is_none() {
            return;
        }
        let setup = self.setup.lock().unwrap().clone();
        if !self.settle(&mut request) {
            return;
        }
        let session = request.session_id.clone();
        let written = write_summary(&self.store, &*self.writer, &setup, request, |l| {
            self.effects.log(l);
        });
        if written.is_some() {
            self.design(&session);
        }
    }

    /// The summary job of a summary that waits for Daniel to open its turn.
    fn park(&self, mut request: SummaryRequest) {
        if self.present(&request.session_id).is_none() {
            return;
        }
        if self.settle(&mut request) {
            self.waiting
                .lock()
                .unwrap()
                .insert(request.id.clone(), request);
            self.store.lock().unwrap().waiting_summary();
        }
    }

    /// The summary of turn `id`, written when it waits, and the session whose design is due.
    fn opened(&self, id: &str) -> Result<(Option<StoredSummary>, Option<String>), String> {
        if let Some(kept) = self.store.lock().unwrap().turn_summary(id) {
            return Ok((Some(kept.clone()), None));
        }
        let Some(request) = self.waiting.lock().unwrap().remove(id) else {
            return Ok((None, None));
        };
        let setup = self.setup.lock().unwrap().clone();
        let written = write_summary(&self.store, &*self.writer, &setup, request.clone(), |l| {
            self.effects.log(l);
        });
        if written.is_none() {
            self.waiting.lock().unwrap().insert(id.to_owned(), request);
            return Err(format!("no summary on {id}"));
        }
        let summary = self.store.lock().unwrap().turn_summary(id).cloned();
        Ok((summary, Some(request.session_id)))
    }

    fn write_name(&self, request: &NameRequest) {
        if self.present(&request.session_id).is_none() {
            return;
        }
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
