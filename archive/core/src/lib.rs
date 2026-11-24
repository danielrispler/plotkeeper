// Removed from core/src/lib.rs in #415: `Session::wants_you` (the Dock badge), `Trees`, and `Studio`'s `turn_merged`, `turn_workers`, `summary_facts` and `set_task_safe`, which only their tests called. Fragments, kept verbatim.

    /// Whether the Dock badge counts it: it needs you, or a blocked command waits, unless it has ended.
    pub fn wants_you(&self) -> bool {
        self.state == State::NeedsYou || (self.state != State::Ended && self.blocked_waiting() > 0)
    }


/// The git trees the hook recorded for worktree `root` at a turn's start and end, which differ.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trees {
    pub root: String,
    pub start: String,
    pub end: String,
}
    /// Whether the turn named `id` ran a merge, by `git merge` or `gh pr merge`, which the offer of a
    /// card's offer leaves out (danielrispler/plotkeeper#119).
    pub fn turn_merged(&self, id: &str) -> bool {
        let found = self.sessions.values().find_map(|s| {
            let at = s.turns.iter().position(|t| t.id.as_deref() == Some(id))?;
            Some(
                s.ledger
                    .turn(at)
                    .filter_map(|r| r.command.as_deref())
                    .any(merges),
            )
        });
        found.unwrap_or(false)
    }

    /// The worktrees that were the home of one of the session's workers during the turn named `id`: a worker
    /// spawned in an earlier turn that still worked during it counts.
    pub fn turn_workers(&self, id: &str) -> Vec<String> {
        let found = self.sessions.values().find_map(|s| {
            let at = s.turns.iter().position(|t| t.id.as_deref() == Some(id))?;
            let turn = &s.turns[at];
            let (since, until) = (turn.started_at, turn.ended_at.unwrap_or(u64::MAX));
            // Spawned in this turn or an earlier one, and working at some time during it.
            let working = s.agents.iter().filter(|a| {
                a.turn.is_some_and(|spawned| spawned <= at)
                    && a.agent.started_at <= until
                    && a.ended_at.is_none_or(|end| end >= since)
            });
            let homes = working.filter(|a| s.has_cards(&a.agent.id));
            let mut roots: Vec<String> = homes.map(|a| a.root.clone()).collect();
            roots.dedup();
            Some(roots)
        });
        found.unwrap_or_default()
    }

    /// The facts of the turn `ask` names, from the numstat the shell found (`None` when git could
    /// not diff it): its files without their hunks, which the shell adds, and its checks.
    pub fn summary_facts(&self, ask: &TurnDiff, diffs: Option<Vec<RootDiff>>) -> Facts {
        let found = diffs.map(|diffs| {
            let snapshot = self.turn_files_snapshot(ask, &diffs);
            compute_turn_files(&snapshot, diffs)
        });
        self.facts_of(ask, found)
    }
    /// The git half of a task's facts, read again with its workspace's git state (no GitHub): a new
    /// commit or file clears the mark at once. A task never looked up has no facts to change.
    /// Returns whether the facts changed.
    pub fn set_task_safe(&mut self, path: &str, safe: bool) -> bool {
        let Some(facts) = self.tasks.get_mut(path) else {
            return false;
        };
        std::mem::replace(&mut facts.safe, safe) != safe
    }
// And `merges`, which only `turn_merged` called:

/// Whether a shell command merges: `git merge` or `gh pr merge`, alone or in a chain.
fn merges(command: &str) -> bool {
    let parts = command.split(['&', ';', '|', '\n']);
    // The word ends there: `git merge-base` is no merge.
    let word = |c: &str, start: &str| {
        let rest = c.strip_prefix(start);
        rest.is_some_and(|r| r.is_empty() || r.starts_with(char::is_whitespace))
    };
    parts
        .map(str::trim)
        .any(|c| word(c, "git merge") || word(c, "gh pr merge"))
}
