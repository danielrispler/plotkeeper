//! What the understanding tests share: `Scripted`, a `CardWriter` whose answer to each method is
//! set by the test, and which keeps what it was asked.

use std::sync::Mutex;
use studio_core::{CardRequest, SummaryRequest};
use studio_understanding::{
    Card, CardWriter, ClaudeWriter, FakeWriter, RetestAsk, Risk, SummarySetup, TurnSummary,
};

pub type Reply = Result<String, String>;

/// What `Answer::Run` runs.
pub type Run = dyn Fn(&str) -> Option<Reply> + Send + Sync;

/// What a `Scripted` method answers.
pub enum Answer {
    /// What `FakeWriter` answers.
    Fake,
    /// This, every time.
    Fixed(Reply),
    /// Each in turn, the last again after that; `None` is what `FakeWriter` answers.
    Sequence(Vec<Option<Reply>>),
    /// Decided by the call's subject, a card's id or a name's prompt (any other method's is
    /// empty); `None` is what `FakeWriter` answers. It runs with nothing locked, so it may take
    /// its time.
    Run(Box<Run>),
}

impl Answer {
    pub fn ok(reply: impl Into<String>) -> Self {
        Answer::Fixed(Ok(reply.into()))
    }

    pub fn err(reply: &str) -> Self {
        Answer::Fixed(Err(reply.into()))
    }

    /// These replies in turn, the last again after that.
    pub fn replies(replies: impl IntoIterator<Item = String>) -> Self {
        let replies: Vec<_> = replies.into_iter().map(|r| Some(Ok(r))).collect();
        assert!(!replies.is_empty(), "a sequence needs at least one reply");
        Answer::Sequence(replies)
    }

    /// `FakeWriter`'s answer, after the calls that `run` takes on.
    pub fn run(run: impl Fn(&str) -> Option<Reply> + Send + Sync + 'static) -> Self {
        Answer::Run(Box::new(run))
    }
}

/// What a writer was asked on a side question: the item, the turn's intentions and the question.
pub type Asking = (String, Vec<String>, String);

/// What a `Scripted` writer keeps of its calls.
#[derive(Default)]
pub struct Kept {
    /// The name of each method called, in order.
    calls: Mutex<Vec<&'static str>>,
    requests: Mutex<Vec<CardRequest>>,
    asked: Mutex<Vec<Asking>>,
    /// The command and folder of each risk note asked for.
    judged: Mutex<Vec<(String, String)>>,
    inputs: Mutex<Vec<String>>,
}

/// A writer that answers each method as its slot says. `fake()` has `FakeWriter`'s answer in every
/// slot, `bare()` the answer of a writer that writes nothing; a test sets the slots it cares about.
pub struct Scripted {
    pub reply: Answer,
    pub more: Answer,
    pub goal: Answer,
    pub design: Answer,
    pub ask: Answer,
    pub retest: Answer,
    pub summarize: Answer,
    pub summarize_agent: Answer,
    pub name: Answer,
    /// The judge's raw replies, read as the real writer reads them; the subject is the command.
    pub risk: Answer,
    /// What it keeps of its calls, for `calls`, `requests`, `asked` and `inputs`.
    pub kept: Kept,
}

impl Scripted {
    fn with(slot: impl Fn(&str) -> Answer) -> Self {
        Scripted {
            reply: slot("reply"),
            more: slot("more"),
            goal: slot("goal"),
            design: slot("design"),
            ask: slot("ask"),
            retest: slot("retest"),
            summarize: slot("summarize"),
            summarize_agent: slot("summarize_agent"),
            name: slot("name"),
            risk: slot("risk"),
            kept: Kept::default(),
        }
    }

    pub fn fake() -> Self {
        Self::with(|_| Answer::Fake)
    }

    pub fn bare() -> Self {
        Self::with(|method| Answer::err(&format!("this writer writes no {method}")))
    }

    /// How many times `method` was called.
    pub fn calls(&self, method: &str) -> usize {
        self.kept
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|m| **m == method)
            .count()
    }

    /// The requests the writer was asked to write a card on.
    pub fn requests(&self) -> Vec<CardRequest> {
        self.kept.requests.lock().unwrap().clone()
    }

    /// The side questions it was asked.
    pub fn asked(&self) -> Vec<Asking> {
        self.kept.asked.lock().unwrap().clone()
    }

    /// The command and folder of each risk note it was asked for.
    pub fn judged(&self) -> Vec<(String, String)> {
        self.kept.judged.lock().unwrap().clone()
    }

    /// What the real `claude -p` would have been given for each summary it wrote, as text.
    pub fn inputs(&self) -> Vec<String> {
        self.kept.inputs.lock().unwrap().clone()
    }

    fn answer(
        &self,
        method: &'static str,
        slot: &Answer,
        subject: &str,
        fake: impl FnOnce() -> Reply,
    ) -> Reply {
        let earlier = {
            let mut calls = self.kept.calls.lock().unwrap();
            let earlier = calls.iter().filter(|m| **m == method).count();
            calls.push(method);
            earlier
        };
        match slot {
            Answer::Fake => fake(),
            Answer::Fixed(reply) => reply.clone(),
            Answer::Sequence(replies) => match &replies[earlier.min(replies.len() - 1)] {
                Some(reply) => reply.clone(),
                None => fake(),
            },
            Answer::Run(run) => run(subject).unwrap_or_else(fake),
        }
    }
}

/// The `claude` the inputs of `Scripted::inputs` are made with.
fn claude() -> ClaudeWriter {
    ClaudeWriter::new("/opt/homebrew/bin/claude".into(), "/card-writer".into())
}

impl CardWriter for Scripted {
    fn reply(&self, request: &CardRequest, earlier: &[String]) -> Reply {
        self.kept.requests.lock().unwrap().push(request.clone());
        let fake = || FakeWriter.reply(request, earlier);
        self.answer("reply", &self.reply, &request.id, fake)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Reply {
        self.answer("more", &self.more, "", || FakeWriter.more(request, card))
    }

    fn goal(&self, prompts: &[&str]) -> Reply {
        self.answer("goal", &self.goal, "", || FakeWriter.goal(prompts))
    }

    fn design(&self, sources: &[(String, String)]) -> Reply {
        self.answer("design", &self.design, "", || FakeWriter.design(sources))
    }

    fn ask(&self, item: &str, intentions: &[String], question: &str) -> Reply {
        let asking = (item.into(), intentions.to_vec(), question.into());
        self.kept.asked.lock().unwrap().push(asking);
        let fake = || FakeWriter.ask(item, intentions, question);
        self.answer("ask", &self.ask, question, fake)
    }

    fn retest(&self, _: &RetestAsk) -> Reply {
        self.answer("retest", &self.retest, "", || {
            Err("this writer writes no retests".into())
        })
    }

    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Reply {
        let (_, input) = claude().summary_command(setup, request, earlier);
        self.kept.inputs.lock().unwrap().push(input);
        let fake = || FakeWriter.summarize(setup, request, earlier);
        self.answer("summarize", &self.summarize, "", fake)
    }

    fn summarize_agent(&self, setup: &SummarySetup, prompt: &str, message: &str) -> Reply {
        let (_, input) = claude().agent_summary_command(setup, prompt, message);
        self.kept.inputs.lock().unwrap().push(input);
        let fake = || FakeWriter.summarize_agent(setup, prompt, message);
        self.answer("summarize_agent", &self.summarize_agent, prompt, fake)
    }

    fn name(&self, setup: &SummarySetup, prompt: &str, response: Option<&str>) -> Reply {
        let fake = || FakeWriter.name(setup, prompt, response);
        self.answer("name", &self.name, prompt, fake)
    }

    fn risk(&self, command: &str, folder: &str) -> Result<Risk, String> {
        self.kept
            .judged
            .lock()
            .unwrap()
            .push((command.into(), folder.into()));
        let fake = || {
            let risk = FakeWriter.risk(command, folder)?;
            Ok(serde_json::json!({ "level": risk.level, "line": risk.line }).to_string())
        };
        let reply = self.answer("risk", &self.risk, command, fake)?;
        Risk::from_reply(&reply)
    }
}
