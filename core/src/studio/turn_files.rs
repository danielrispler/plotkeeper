//! The turns whose files the shell diffs, and what it found.

use super::*;
use crate::turn_files;

impl Studio {
    /// The trees the hook recorded of worktree `root` for turn `turn` of `session`: its start, and its end once the
    /// turn ended (danielrispler/plotkeeper#114) and no worker it spawned runs on there (danielrispler/plotkeeper#142).
    pub fn turn_trees(
        &self,
        session: &str,
        turn: usize,
        root: &str,
    ) -> Option<(String, Option<String>)> {
        let session = self.session(session)?;
        let (start, end) = session.turns.get(turn)?.trees.get(root)?.clone();
        let open = session.open_roots().contains(&(turn, root.into()));
        Some((start, end.filter(|_| !open)))
    }

    /// The turns whose files the shell should diff now, newest first, at most `limit`
    /// (danielrispler/plotkeeper#111): a running turn each 5 seconds, its end tree taken on demand,
    /// and an ended one of the last 7 days once, from the trees the hook recorded.
    pub fn turn_diffs(&mut self, now: u64, limit: usize) -> Vec<TurnDiff> {
        let mut asks = Vec::new();
        for session in self.sessions.values_mut() {
            if !session.capabilities.turn_trees || session.deleted {
                continue;
            }
            let running = session.open_roots();
            let windows: Vec<(u64, u64)> = (0..session.turns.len())
                .map(|index| session.trees_window(index))
                .collect();
            let (turns, session_ended, last_event) = (
                session.turns.len(),
                session.ended,
                session.agent.last_event_at,
            );
            for (index, turn) in session.turns.iter_mut().enumerate() {
                // A turn with no end of an ended session, or before a later turn, is over all
                // the same.
                let stale = turn.ended_at.is_none() && (session_ended || index + 1 < turns);
                let ended = turn.ended_at.is_some() || stale;
                let open = |root: &String| !ended || running.contains(&(index, root.clone()));
                // Final once it ended and no worker it spawned runs on in one of its worktrees.
                let last = ended && !turn.trees.keys().any(open);
                // A turn that never ended (its session was killed) ages from its start.
                let recent = turn.ended_at.unwrap_or(turn.started_at) + turn_files::SINCE_MS >= now;
                let due = match turn.files_asked {
                    None => true,
                    Some((_, true)) => false,
                    // The worktree only changes through the agent's own events, so a turn still open is asked again only
                    // once its session has had one since the last ask: a killed session's turn keeps its last result.
                    Some((at, false)) => {
                        now.saturating_sub(at) >= turn_files::RUNNING_REFRESH_MS
                            && (last || session.agent.last_event_at >= at)
                    }
                };
                if !recent || !due || turn.trees.is_empty() {
                    continue;
                }
                let trees: Vec<TurnTrees> = turn
                    .trees
                    .iter()
                    // An ended turn's worktree with no end tree has nothing to compare.
                    .filter(|(root, (_, end))| open(root) || end.is_some())
                    .map(|(root, (start, end))| TurnTrees {
                        root: root.clone(),
                        start: start.clone(),
                        end: end.clone().filter(|_| !open(root)),
                    })
                    .collect();
                if trees.is_empty() {
                    turn.files_asked = Some((now, last));
                } else {
                    let session_id = session.agent.id.clone();
                    let ask = TurnDiff {
                        session_id,
                        turn: index,
                        ended: last,
                        trees,
                        window: windows[index],
                    };
                    // A stale turn sorts by its start, or the session's last event, never as now.
                    let at = match turn.ended_at {
                        Some(at) => at,
                        None if !stale => now,
                        None if session_ended => last_event,
                        None => turn.started_at,
                    };
                    asks.push((at, ask));
                }
            }
        }
        asks.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        asks.truncate(limit);
        // Only the asks that survive the cut are asked, so the rest are due again in the next pass.
        for (_, ask) in &asks {
            let turn = self
                .sessions
                .get_mut(&ask.session_id)
                .and_then(|s| s.turns.get_mut(ask.turn));
            if let Some(turn) = turn {
                turn.files_asked = Some((now, ask.ended));
            }
        }
        asks.into_iter().map(|(_, ask)| ask).collect()
    }

    /// The diff the shell is asked for to write the summary of the turn named `id`
    /// (`SummaryRequest::id`): its trees as they ended. `None` for a turn with no trees.
    pub fn summary_ask(&self, id: &str) -> Option<TurnDiff> {
        let session = self
            .sessions
            .values()
            .find(|s| s.turns.iter().any(|t| t.id.as_deref() == Some(id)))?;
        let (index, turn) = session
            .turns
            .iter()
            .enumerate()
            .find(|(_, t)| t.id.as_deref() == Some(id))?;
        let trees: Vec<TurnTrees> = (turn.trees.iter())
            .filter_map(|(root, (start, end))| {
                let end = end.clone()?;
                Some(TurnTrees {
                    root: root.clone(),
                    start: start.clone(),
                    end: Some(end),
                })
            })
            .collect();
        (session.capabilities.turn_trees && !trees.is_empty()).then(|| TurnDiff {
            session_id: session.agent.id.clone(),
            turn: index,
            ended: true,
            trees,
            window: session.trees_window(index),
        })
    }

    /// The facts of the turn `ask` names, from the files `compute_turn_files` found for it.
    pub fn facts_of(&self, ask: &TurnDiff, found: Option<TurnFiles>) -> Facts {
        let Some(session) = self.sessions.get(&ask.session_id) else {
            return Facts::default();
        };
        let Some(turn) = session.turns.get(ask.turn) else {
            return Facts::default();
        };
        let found = found.unwrap_or_default();
        Facts {
            files: found
                .files
                .into_iter()
                .map(|file| FactFile {
                    file,
                    hunk: String::new(),
                })
                .collect(),
            checks: checks::ended(&turn.checks),
            external: 0,
        }
    }

    /// Marks the turn of `ask` as asked for the last time, for one whose diff the shell skipped as
    /// too large: it is not asked again until a new tree lands.
    pub fn settle_turn_files(&mut self, ask: &TurnDiff) {
        let turn = self
            .sessions
            .get_mut(&ask.session_id)
            .and_then(|s| s.turns.get_mut(ask.turn));
        if let Some(turn) = turn {
            turn.files_asked = Some((0, true));
        }
    }

    /// Takes what the shell found for `ask`: the numstat of each of its worktrees, `None` when git
    /// could not diff them. The turn's files are every file that differs, but those another
    /// session's or turn's call names (danielrispler/plotkeeper#207). The shell does the three
    /// steps apart, the matching with the core unlocked: `turn_files_snapshot`,
    /// `compute_turn_files`, `store_turn_files`.
    pub fn set_turn_files(&mut self, ask: &TurnDiff, diffs: Option<Vec<RootDiff>>) {
        let found = diffs.map(|diffs| {
            let snapshot = self.turn_files_snapshot(ask, &diffs);
            compute_turn_files(&snapshot, diffs)
        });
        self.store_turn_files(ask, found);
    }

    /// Stores the files `compute_turn_files` found for `ask`, `None` when git could not diff the turn.
    pub fn store_turn_files(&mut self, ask: &TurnDiff, found: Option<TurnFiles>) {
        let Some(session) = self.sessions.get_mut(&ask.session_id) else {
            return;
        };
        if let Some(turn) = session.turns.get_mut(ask.turn) {
            turn.files = found;
        }
        session.update_unseen();
    }

    /// What the matching of the files of `ask` among `diffs` reads: the calls that touched the same
    /// worktrees while its trees ran, which name the files that differ but are not the turn's, and
    /// the turn's own, which name those that are. Another call is any of another session's, and any
    /// of its own session's other turns.
    pub fn turn_files_snapshot(&self, ask: &TurnDiff, diffs: &[RootDiff]) -> TurnFilesSnapshot {
        let (session_id, turn) = (&ask.session_id, ask.turn);
        let Some(session) = self.sessions.get(session_id) else {
            return TurnFilesSnapshot::default();
        };
        let (since, until) = session.trees_window(turn);
        let made = |r: &&Row| {
            let rejected = r.outcome == Some(Outcome::Rejected);
            !rejected && !(r.edits() && r.failed())
        };
        let others: Vec<&Row> = (self.sessions.values())
            .flat_map(|s| {
                s.ledger
                    .rows()
                    .iter()
                    .map(move |r| (s.agent.id == *session_id, r))
            })
            .filter(|(own, r)| !*own || r.turn != Some(turn))
            .map(|(_, r)| r)
            .filter(made)
            .filter(|r| r.at().is_some_and(|at| (since..=until).contains(&at)))
            .collect();
        let ours: Vec<&Row> = (session.ledger.turn(turn)).filter(made).collect();
        let name = |agent: &Option<String>| match agent {
            Some(id) => session.agent_name(id).to_string(),
            None => session.capabilities.agent.to_string(),
        };
        let worker = |root: &str| {
            let spawned = session.agents.iter().filter(|a| a.turn == Some(turn));
            let mut own = spawned.filter(|a| a.root == root && session.has_cards(&a.agent.id));
            own.next().map(|a| name(&Some(a.agent.id.clone())))
        };
        TurnFilesSnapshot::of(
            &self.roots,
            diffs.iter().map(|d| d.root.as_str()),
            &others,
            &ours,
            |row| name(&row.agent),
            worker,
        )
    }
}
