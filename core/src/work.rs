//! Phases and the estimate (#461, decision 14): which phase of its work on a ticket a session is in,
//! from the skills its agent invokes and the commands it runs, and how long its remaining work
//! typically takes, from the sessions of the same kind before it.

use serde::Serialize;
use serde_json::Value;

/// A recognisable stretch of a session's work on a ticket, in the usual order (`docs/studio/CONTEXT.md`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    #[default]
    Orienting,
    Designing,
    Implementing,
    Checking,
    Reviewing,
    OpeningPr,
    WaitingOnCi,
    /// Its PR merged: the work is done.
    Merged,
}

/// When a phase started: at a time, and after how much of the agent's work.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhaseStart {
    pub phase: Phase,
    pub at: u64,
    pub agent_ms: u64,
}

/// A session's work on its ticket: its kind, its phases and how long its agent has worked.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Work {
    /// The kind of work: the first prompt's slash command, else the first skill its agent invoked,
    /// without any plugin prefix (`implement`, `wayfinder`); `None` before either.
    pub kind: Option<String>,
    /// Its phases, oldest first; empty before its first prompt.
    pub phases: Vec<PhaseStart>,
    /// How long its agent worked: in its turns, not while it needed Daniel, up to the merge.
    pub agent_ms: u64,
    pub estimate: Estimate,
    /// Its latest event's time.
    #[serde(skip)]
    last_at: Option<u64>,
}

/// How long the agent's remaining work will take, set as each phase starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Estimate {
    /// The range of the remaining work, in milliseconds of the agent's time; `None` for "no
    /// estimate".
    pub range: Option<(u64, u64)>,
    /// The session waits on Daniel, which the range leaves out.
    pub depends_on_you: bool,
}

impl Work {
    pub fn phase(&self) -> Option<Phase> {
        self.phases.last().map(|p| p.phase)
    }

    /// An event of the session at `at`; the agent worked since the one before if it was `working`.
    pub(crate) fn passes(&mut self, at: u64, working: bool) {
        if let Some(last) = self.last_at
            && working
            && self.phase() != Some(Phase::Merged)
        {
            self.agent_ms += at.saturating_sub(last);
        }
        self.last_at = Some(self.last_at.unwrap_or_default().max(at));
    }

    /// Daniel's or a wake-up's prompt at `at`.
    pub(crate) fn prompt(&mut self, prompt: &str, at: u64) {
        if self.phases.is_empty() {
            self.kind = slash_command(prompt).map(|c| without_plugin(c).into());
            self.start(Phase::Orienting, at);
        }
    }

    /// The session's own agent started a call of `tool` with `input` at `at`.
    pub(crate) fn call(&mut self, tool: &str, input: &Value, at: u64) {
        let skill = (tool == "Skill").then(|| input["skill"].as_str()).flatten();
        if let Some(skill) = skill
            && self.kind.is_none()
        {
            self.kind = Some(without_plugin(skill).into());
        }
        let Some(current) = self.phase() else {
            return;
        };
        let next = match (tool, skill) {
            (_, Some(skill)) => skill_phase(without_plugin(skill)),
            ("Bash", _) => command_phase(input["command"].as_str().unwrap_or_default(), current),
            _ => None,
        };
        if let Some(next) = next.filter(|next| *next > current) {
            self.start(next, at);
        }
    }

    fn start(&mut self, phase: Phase, at: u64) {
        let agent_ms = self.agent_ms;
        self.phases.push(PhaseStart {
            phase,
            at,
            agent_ms,
        });
    }
}

/// How many past sessions of a kind an estimate needs (#461, decision 22).
pub const MIN_HISTORY: usize = 20;

/// The range of the remaining work from the start of `phase`, from how long it took in `past`
/// sessions that reached it or a later phase: the spread of their remaining agent time, from its
/// lower to its upper sixth. `None` when fewer than `min_history` reached it.
pub fn range(past: &[&Work], phase: Phase, min_history: usize) -> Option<(u64, u64)> {
    let mut remaining: Vec<u64> = past
        .iter()
        .filter_map(|work| {
            let start = work.phases.iter().find(|p| p.phase >= phase)?;
            Some(work.agent_ms - start.agent_ms)
        })
        .collect();
    if remaining.len() < min_history.max(1) {
        return None;
    }
    remaining.sort_unstable();
    let last = remaining.len() - 1;
    Some((remaining[last / 6], remaining[(5 * last).div_ceil(6)]))
}

/// The slash command a prompt starts with, such as `mattpocock-skills:implement`.
fn slash_command(prompt: &str) -> Option<&str> {
    let command = prompt
        .trim_start()
        .strip_prefix('/')?
        .split_whitespace()
        .next()?;
    Some(command).filter(|c| !c.is_empty())
}

/// A skill's or command's name without its plugin.
fn without_plugin(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn skill_phase(skill: &str) -> Option<Phase> {
    match skill {
        "grilling" | "grill-with-docs" | "domain-modeling" | "to-spec" | "to-tickets"
        | "prototype" | "research" => Some(Phase::Designing),
        "tdd" | "implement" | "implement-spec" | "diagnosing-bugs" => Some(Phase::Implementing),
        "code-review" | "simplify" | "ponytail-review" => Some(Phase::Reviewing),
        "pr" => Some(Phase::OpeningPr),
        _ => None,
    }
}

/// A command's phase. Tests run while implementing; the full checks once there is code to check, so a
/// baseline run before any is part of orienting.
fn command_phase(command: &str, current: Phase) -> Option<Phase> {
    let has = |words: &str| command.contains(words);
    if has("gh pr merge") {
        Some(Phase::Merged)
    } else if has("gh pr checks") && has("--watch") {
        Some(Phase::WaitingOnCi)
    } else if has("gh pr create") {
        Some(Phase::OpeningPr)
    } else if has("ci-checks") || has("cargo clippy") {
        (current >= Phase::Implementing).then_some(Phase::Checking)
    } else if crate::is_check(command) || has("npm test") {
        Some(Phase::Implementing)
    } else {
        None
    }
}
