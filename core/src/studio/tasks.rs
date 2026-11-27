//! What the shell finds about each task: its branch, PR, files and issue, and whether it landed.

use super::*;

/// What the shell found about a task, for **Landed**. A lookup that failed is `false`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskFacts {
    /// Nothing uncommitted and nothing unpushed.
    pub safe: bool,
    /// Its PR is merged.
    pub merged: bool,
    /// Its issue is closed; only read when a session of the task has an issue.
    pub issue_closed: bool,
}

/// What the shell asks `gh pr view` about: a branch, the folder of a session on it, and that session's project, whose PRs
/// the answer joins and in whose folder `gh` runs (spec #659).
#[derive(Clone, Debug, PartialEq)]
pub struct BranchQuery {
    pub project: String,
    pub cwd: String,
    pub branch: String,
}

/// What the shell looks up about a task: its worktree folder, its branch, its issue.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskQuery {
    pub path: String,
    pub branch: Option<String>,
    pub issue: Option<u64>,
}

impl Studio {
    /// The branch, folder and project of each session that has not ended, and of the selected one, one per
    /// branch: what the shell asks `gh pr view` about, off the poll (danielrispler/Falinks#461, decision 19).
    pub fn branches(&self) -> Vec<BranchQuery> {
        let mut seen = BTreeMap::new();
        for session in self.sessions.values() {
            let shown = self.selected.as_ref() == Some(&session.agent.id);
            if let Some(branch) = &session.identity.branch
                && (session.state != State::Ended || shown)
            {
                seen.entry(branch.clone()).or_insert_with(|| BranchQuery {
                    project: session.project.clone(),
                    cwd: session.cwd.clone(),
                    branch: branch.clone(),
                });
            }
        }
        seen.into_values().collect()
    }

    /// The tasks the shell looks up for **Landed**: each worktree of a listed session that is under
    /// `.claude/worktrees/`, once, with its branch and issue.
    pub fn task_queries(&self) -> Vec<TaskQuery> {
        let mut queries = BTreeMap::<String, TaskQuery>::new();
        for session in self.sessions() {
            let Some(path) = task_path(&session.cwd) else {
                continue;
            };
            let query = queries.entry(path.clone()).or_insert(TaskQuery {
                path,
                branch: None,
                issue: None,
            });
            query.branch = query.branch.take().or(session.identity.branch.clone());
            query.issue = query.issue.or(session.identity.issue);
        }
        queries.into_values().collect()
    }

    /// The shell looked a task up: `None` when it could not, which leaves the task not landed.
    /// Returns whether the facts changed.
    pub fn set_task_facts(&mut self, path: &str, facts: Option<TaskFacts>) -> bool {
        let before = self.tasks.get(path).copied();
        match facts {
            Some(facts) => self.tasks.insert(path.into(), facts),
            None => self.tasks.remove(path),
        };
        before != facts
    }

    /// What the shell last found about the task folder `path`, `None` before its first lookup.
    pub fn task_facts_at(&self, path: &str) -> Option<TaskFacts> {
        self.tasks.get(path).copied()
    }

    /// Drops the facts of tasks no listed session works in any more, whose worktree is gone with them.
    pub fn forget_gone_tasks(&mut self) {
        let kept: Vec<String> = self.task_queries().into_iter().map(|q| q.path).collect();
        self.tasks.retain(|path, _| kept.contains(path));
    }

    /// What the shell last found about the task session `id` works in, `None` in the main checkout
    /// or before the first lookup.
    pub fn task_facts(&self, id: &str) -> Option<TaskFacts> {
        let path = task_path(&self.sessions.get(id)?.cwd)?;
        self.tasks.get(&path).copied()
    }

    /// The task folder session `id` works in, `None` in the main checkout or for no such session.
    pub fn task_of(&self, id: &str) -> Option<String> {
        task_path(&self.sessions.get(id)?.cwd)
    }

    /// Whether `session`'s task has landed: its work has shipped, so removing its worktree loses
    /// nothing. Nothing uncommitted or unpushed, its PR merged, its issue closed when a session
    /// there has one, and none of the sessions listed in the worktree running or needing Daniel.
    pub fn landed(&self, session: &Session) -> bool {
        let Some(path) = task_path(&session.cwd) else {
            return false;
        };
        let Some(facts) = self.tasks.get(&path) else {
            return false;
        };
        let there: Vec<&Session> = (self.sessions().into_iter())
            .filter(|s| task_path(&s.cwd).as_deref() == Some(path.as_str()))
            .collect();
        let has_issue = there.iter().any(|s| s.identity.issue.is_some());
        facts.safe
            && facts.merged
            && (!has_issue || facts.issue_closed)
            && there
                .iter()
                .all(|s| !matches!(s.state, State::Running | State::NeedsYou))
    }

    /// The shell found `pr` for `branch`; returns whether any session's name changed.
    pub fn set_pr(&mut self, branch: &str, pr: Option<u64>) -> bool {
        let mut changed = false;
        for session in self.sessions.values_mut() {
            let identity = &mut session.identity;
            if identity.branch.as_deref() == Some(branch) && identity.pr != pr {
                identity.pr = pr;
                identity.named();
                changed = true;
            }
        }
        changed
    }

    /// The typical duration of check `command`, in milliseconds: the median of how long its earlier
    /// runs took when they did not fail, its backgrounded runs included (danielrispler/Falinks#461, decisions 13 to 15).
    /// `None` when it never ran. Runs group by their check statements (`checks::key`) under the list
    /// of a session working in `cwd`.
    pub fn typical(&self, cwd: &str, command: &str) -> Option<u64> {
        self.durations.typical(self.check_lists.of(cwd), command)
    }
}
