//! Which pull requests are ready for review (spec danielrispler/plotkeeper#334, ticket #336). The server asks `gh pr view
//! <branch>` for each session's branch on Landed's triggers, never on a timer: a turn's end at once, and window focus and the
//! Map view's opening through one per-repository cooldown with an in-flight guard, as the Map view's fetch has. A PR is ready
//! when it is open and not a draft; its head is kept so a later ticket can tell a new push.

use crate::App;
use crate::map::{Fetched, Fetches};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use studio_workbench::run::{self, RunError, run_until};

/// What `gh pr view` is asked for.
pub const FIELDS: &str =
    "number,title,isDraft,state,headRefOid,headRefName,body,closingIssuesReferences";

/// What asking `gh` about one branch's PR found.
#[derive(Debug, Clone, PartialEq)]
pub enum Looked {
    /// `gh pr view`'s JSON.
    Found(String),
    /// `gh` answered that the branch has no PR.
    NoPr,
    /// `gh` did not answer (offline, signed out, a timeout): what was known stays.
    Failed,
}

/// What asks about PRs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Trigger {
    /// An agent's turn ended: asks at once.
    TurnEnd,
    /// The window gained focus.
    Focus,
    /// The Map view opened.
    MapOpen,
}

/// One pull request as `gh pr view` shows it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pr {
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub state: String,
    #[serde(default, rename = "headRefOid")]
    pub head: String,
    /// The PR's branch, which keys it beside the sessions' branches.
    #[serde(default, rename = "headRefName")]
    pub branch: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub closing_issues_references: Vec<Closing>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Closing {
    pub number: u64,
}

impl Pr {
    /// Ready for review: open and not a draft.
    pub fn ready(&self) -> bool {
        self.state == "OPEN" && !self.is_draft
    }
}

/// Reads `gh pr view --json` output; `None` for anything that is not one PR.
pub fn parse(json: &str) -> Option<Pr> {
    serde_json::from_str(json).ok()
}

/// Reads `gh pr list --json` output; `None` for anything that is not a list of PRs.
pub fn parse_list(json: &str) -> Option<Vec<Pr>> {
    serde_json::from_str(json).ok()
}

/// A ready PR as the Map view's tray lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ready {
    pub number: u64,
    pub title: String,
    pub head: String,
    /// The PR's branch, which an overturn's prompt names while the PR is open.
    pub branch: String,
    /// What the PR's brief is doing (`pr_briefs.rs`); `prs.ready` leaves it empty and the Map view's payload fills it.
    pub brief: Option<crate::pr_briefs::BriefInfo>,
}

/// The PRs found for each project's branches, and the guard of the lookups.
#[derive(Default)]
pub struct Prs {
    guard: Fetches,
    found: Mutex<HashMap<String, BTreeMap<String, Pr>>>,
    /// The projects whose `gh pr list` has answered at least once.
    listed: Mutex<HashSet<String>>,
    /// The order of the lookup whose answers `found` holds, per project: a lookup that ends after a later one keeps none of its
    /// own, so what is found never moves back.
    seqs: Mutex<HashMap<String, u64>>,
    /// The lookups started, in order.
    started: std::sync::atomic::AtomicU64,
}

impl Prs {
    /// Asks `gh` (through `view`, which returns the JSON of `gh pr view <branch>`) about each of `branches` of `project`, if the
    /// trigger and the guard allow it. `Some` holds each branch with the PR number found for it (for the session's name), or
    /// `None` when nothing ran.
    pub fn ask(
        &self,
        project: &str,
        trigger: Trigger,
        now: Instant,
        branches: &[String],
        view: impl Fn(&str) -> Looked,
        list: impl Fn() -> Option<String>,
    ) -> Option<Vec<(String, Option<u64>)>> {
        let cooldown = match trigger {
            Trigger::TurnEnd => Duration::ZERO,
            Trigger::Focus | Trigger::MapOpen => crate::map::COOLDOWN,
        };
        let mut asked = Vec::new();
        let mut listed = None;
        let mut seq = 0;
        let outcome = self
            .guard
            .run_after(&format!("prs:{project}"), now, cooldown, || {
                // The guard runs one lookup of a project at a time, so the order they start in is the order of what they saw.
                seq = self
                    .started
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    + 1;
                for branch in branches {
                    asked.push((branch.clone(), view(branch)));
                }
                listed = list().and_then(|json| parse_list(&json));
                Ok(())
            });
        if outcome == Fetched::Skipped {
            return None;
        }
        let mut found = self.found.lock().unwrap();
        let mut seqs = self.seqs.lock().unwrap();
        if seqs.get(project).is_some_and(|seen| *seen > seq) {
            // A later lookup's answers are kept already: this one's are older.
            asked.clear();
            listed = None;
        } else {
            seqs.insert(project.to_string(), seq);
        }
        drop(seqs);
        let of_project = found.entry(project.to_string()).or_default();
        let mut numbers = Vec::new();
        for (branch, looked) in asked {
            match looked {
                Looked::Found(json) => {
                    if let Some(pr) = parse(&json) {
                        numbers.push((branch.clone(), Some(pr.number)));
                        of_project.insert(branch, pr);
                    }
                }
                Looked::NoPr => {
                    numbers.push((branch.clone(), None));
                    of_project.remove(&branch);
                }
                // `gh` did not answer: the last known state stays.
                Looked::Failed => {}
            }
        }
        // The open PRs of the repository, sessions or not. Once the list has answered, a PR that is neither on it nor a
        // session's branch is gone (merged, closed, its session closed with it).
        if let Some(listed) = listed {
            self.listed.lock().unwrap().insert(project.to_string());
            let mut keep: Vec<String> = branches.to_vec();
            for pr in listed {
                let key = if pr.branch.is_empty() {
                    format!("#{}", pr.number)
                } else {
                    pr.branch.clone()
                };
                keep.push(key.clone());
                of_project.insert(key, pr);
            }
            of_project.retain(|key, _| keep.contains(key));
        }
        drop(found);
        Some(numbers)
    }

    /// Whether `gh pr list` has answered for `project` since start, so its ready PRs are all known.
    pub fn listed(&self, project: &str) -> bool {
        self.listed.lock().unwrap().contains(project)
    }

    /// The ready PRs of `project`, by number, with the order of the lookup they are from (`MapReviews::head_now`).
    pub fn ready_seq(&self, project: &str) -> (u64, Vec<Ready>) {
        let found = self.found.lock().unwrap();
        // Read under `found`'s lock, which every change of `seqs` holds: the order is the one of the answers read.
        let seq = self.seqs.lock().unwrap().get(project).copied().unwrap_or(0);
        (seq, ready_of(&found, project))
    }

    /// The ready PRs of `project`, by number.
    pub fn ready(&self, project: &str) -> Vec<Ready> {
        ready_of(&self.found.lock().unwrap(), project)
    }
}

/// The ready PRs of `project` in `found`, by number.
fn ready_of(found: &HashMap<String, BTreeMap<String, Pr>>, project: &str) -> Vec<Ready> {
    let mut ready: Vec<Ready> = found
        .get(project)
        .into_iter()
        .flat_map(|branches| branches.values())
        .filter(|pr| pr.ready())
        .map(|pr| Ready {
            number: pr.number,
            title: pr.title.clone(),
            head: pr.head.clone(),
            branch: pr.branch.clone(),
            brief: None,
        })
        .collect();
    ready.sort_by_key(|r| r.number);
    ready
}

/// Runs `gh pr view <branch>` in `cwd`, as Daniel with its own login; no credential file is read.
fn gh_view(cwd: &str, branch: &str) -> Looked {
    let mut gh = run::gh(Path::new(cwd));
    match run_until(gh.args(["pr", "view", branch, "--json", FIELDS]), GH_LIMIT) {
        Ok(output) => Looked::Found(String::from_utf8_lossy(&output.stdout).into_owned()),
        Err(RunError::Failed(output))
            if String::from_utf8_lossy(&output.stderr).contains("no pull requests found") =>
        {
            Looked::NoPr
        }
        Err(_) => Looked::Failed,
    }
}

/// The longest a lookup at GitHub may take.
const GH_LIMIT: Duration = Duration::from_secs(30);

/// Runs `gh pr list` for the open PRs of the repository in `project`, hand-made ones included.
fn gh_list(project: &str) -> Option<String> {
    let mut gh = run::gh(Path::new(project));
    let output = run_until(
        gh.args([
            "pr", "list", "--state", "open", "--limit", "100", "--json", FIELDS,
        ]),
        GH_LIMIT,
    )
    .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The lookups `ask_later` runs, by project: each branch with the folder its `gh pr view` runs in. Each branch is asked about
/// under its session's project (spec #659), the project its row is listed under, and in that project's folder, the main checkout of
/// the same repository, never the session's folder, which a `cd` may have taken out of it. `also` is asked about even with no session
/// in it, so the Map view's project's hand-made PRs are found.
fn lookups(
    queries: Vec<studio_core::BranchQuery>,
    also: Option<String>,
) -> BTreeMap<String, Vec<(String, String)>> {
    let mut projects = BTreeMap::<String, Vec<(String, String)>>::new();
    for query in queries {
        if !query.project.is_empty() {
            let folder = query.project.clone();
            (projects.entry(query.project).or_default()).push((folder, query.branch));
        }
    }
    if let Some(project) = also {
        projects.entry(project).or_default();
    }
    projects
}

/// Asks about the PRs of every listed session's branch on `trigger`, on a thread of its own so no loop waits on the network.
/// `also` is a project to ask about beyond the sessions' ones. A session's name takes the PR number found; the Map view is told (`map-landed`) when a project's ready PRs changed.
pub fn ask_later(app: &Arc<App>, trigger: Trigger, also: Option<&str>) {
    let (app, also) = (app.clone(), also.map(str::to_string));
    std::thread::spawn(move || {
        let projects = lookups(app.core().branches(), also);
        let prs = &app.prs;
        for (project, sessions) in projects {
            let before = prs.ready(&project);
            let branches: Vec<String> = sessions.iter().map(|(_, b)| b.clone()).collect();
            let cwd_of = |branch: &str| {
                sessions
                    .iter()
                    .find(|(_, b)| b == branch)
                    .map(|(cwd, _)| cwd.clone())
            };
            let asked = prs.ask(
                &project,
                trigger,
                Instant::now(),
                &branches,
                |branch| match cwd_of(branch) {
                    Some(cwd) => gh_view(&cwd, branch),
                    None => Looked::Failed,
                },
                || gh_list(&project),
            );
            let Some(asked) = asked else { continue };
            for (branch, number) in asked {
                if app.core().set_pr(&branch, number) {
                    app.list_changed();
                }
            }
            let (seq, ready) = prs.ready_seq(&project);
            if ready != before {
                app.events
                    .emit("map-landed", serde_json::json!(project.clone()));
            }
            crate::pr_briefs::lookup_later(&app, &project, ready, seq);
        }
    });
}

/// Whether PR `number` of the repository in `repo` was opened by Daniel's own GitHub login (spec #634, Q17): `gh pr view --json
/// author` against `gh api user`, run as his normal `gh`, which reads no credential file of ours. Anything `gh` cannot answer counts
/// as not his, so an unknown author is never trusted.
pub fn by_own_login(repo: &Path, number: u64) -> bool {
    by_own_login_with(studio_workbench::gh_path(), repo, number)
}

/// `by_own_login` with the `gh` at `program`.
pub(crate) fn by_own_login_with(program: &str, repo: &Path, number: u64) -> bool {
    let n = number.to_string();
    let login = |args: &[&str]| {
        let output = run_until(run::gh_at(program, repo).args(args), GH_LIMIT).ok()?;
        let login = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!login.is_empty()).then_some(login)
    };
    let author = login(&[
        "pr",
        "view",
        &n,
        "--json",
        "author",
        "--jq",
        ".author.login",
    ]);
    // With no author there is nothing to compare, so Daniel's own login is not asked.
    author.is_some_and(|author| login(&["api", "user", "--jq", ".login"]) == Some(author))
}

#[cfg(test)]
mod tests;
