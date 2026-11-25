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
        let outcome = self
            .guard
            .run_after(&format!("prs:{project}"), now, cooldown, || {
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

    /// The ready PRs of `project`, by number.
    pub fn ready(&self, project: &str) -> Vec<Ready> {
        let found = self.found.lock().unwrap();
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
        drop(found);
        ready.sort_by_key(|r| r.number);
        ready
    }
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

/// Asks about the PRs of every listed session's branch on `trigger`, on a thread of its own so no loop waits on the network.
/// `also` is a project to ask about beyond the sessions' ones. A session's name takes the PR number found; the Map view is told (`map-landed`) when a project's ready PRs changed.
pub fn ask_later(app: &Arc<App>, trigger: Trigger, also: Option<&str>) {
    let (app, also) = (app.clone(), also.map(str::to_string));
    std::thread::spawn(move || {
        let mut projects = BTreeMap::<String, Vec<(String, String)>>::new();
        let branches = app.core().branches();
        for (cwd, branch) in branches {
            if let Ok(project) = studio_workbench::main_checkout(Path::new(&cwd)) {
                projects.entry(project).or_default().push((cwd, branch));
            }
        }
        // The project the Map view shows is asked about even with no session in it, so its hand-made PRs are found.
        if let Some(project) = also {
            projects.entry(project).or_default();
        }
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
            let ready = prs.ready(&project);
            if ready != before {
                app.events
                    .emit("map-landed", serde_json::json!(project.clone()));
            }
            crate::pr_briefs::lookup_later(&app, &project, ready);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: &str = r#"{"number":7,"title":"Fog on the map","isDraft":false,"state":"OPEN","headRefOid":"aaa111","body":"Closes #5","closingIssuesReferences":[{"number":5}]}"#;
    const PUSHED: &str = r#"{"number":7,"title":"Fog on the map","isDraft":false,"state":"OPEN","headRefOid":"bbb222","body":"Closes #5","closingIssuesReferences":[{"number":5}]}"#;
    const DRAFT: &str = r#"{"number":8,"title":"Wip","isDraft":true,"state":"OPEN","headRefOid":"ccc333","body":"","closingIssuesReferences":[]}"#;
    const CLOSED: &str = r#"{"number":9,"title":"Gone","isDraft":false,"state":"CLOSED","headRefOid":"ddd444","body":"","closingIssuesReferences":[]}"#;

    fn recorded(branch: &str) -> Looked {
        match branch {
            "fog" => Looked::Found(OPEN.into()),
            "wip" => Looked::Found(DRAFT.into()),
            "gone" => Looked::Found(CLOSED.into()),
            _ => Looked::NoPr,
        }
    }
    fn no_list() -> Option<String> {
        None
    }
    fn branches() -> Vec<String> {
        ["fog", "wip", "gone", "none"].map(String::from).to_vec()
    }

    #[test]
    fn only_an_open_pr_that_is_not_a_draft_is_ready() {
        let prs = Prs::default();
        let found = prs.ask(
            "/p",
            Trigger::TurnEnd,
            Instant::now(),
            &branches(),
            recorded,
            no_list,
        );
        // Every PR found still names its session; a branch with none names nothing.
        assert_eq!(
            found.unwrap(),
            vec![
                ("fog".into(), Some(7)),
                ("wip".into(), Some(8)),
                ("gone".into(), Some(9)),
                ("none".into(), None)
            ]
        );
        let ready = prs.ready("/p");
        assert_eq!(ready.len(), 1);
        assert_eq!(
            (ready[0].number, ready[0].title.as_str()),
            (7, "Fog on the map")
        );
    }

    #[test]
    fn a_new_head_replaces_the_stored_one() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            recorded,
            no_list,
        );
        assert_eq!(prs.ready("/p")[0].head, "aaa111");
        let pushed = |_: &str| Looked::Found(PUSHED.to_string());
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            pushed,
            no_list,
        );
        assert_eq!(prs.ready("/p")[0].head, "bbb222");
    }

    #[test]
    fn a_pr_that_is_no_longer_ready_leaves_the_tray() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            recorded,
            no_list,
        );
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            |_| Looked::Found(DRAFT.into()),
            no_list,
        );
        assert!(prs.ready("/p").is_empty());
    }

    #[test]
    fn a_turns_end_asks_at_once_but_focus_and_map_open_share_one_cooldown() {
        let prs = Prs::default();
        let start = Instant::now();
        let asks = std::cell::Cell::new(0);
        let view = |b: &str| {
            asks.set(asks.get() + 1);
            recorded(b)
        };
        let one = ["fog".to_string()];
        assert!(
            prs.ask("/p", Trigger::MapOpen, start, &one, view, no_list)
                .is_some()
        );
        // Focus within the cooldown asks nothing.
        let soon = start + Duration::from_secs(30);
        assert!(
            prs.ask("/p", Trigger::Focus, soon, &one, view, no_list)
                .is_none()
        );
        assert!(
            prs.ask("/p", Trigger::MapOpen, soon, &one, view, no_list)
                .is_none()
        );
        assert_eq!(asks.get(), 1);
        // A turn's end asks at once, whatever the cooldown.
        assert!(
            prs.ask("/p", Trigger::TurnEnd, soon, &one, view, no_list)
                .is_some()
        );
        assert_eq!(asks.get(), 2);
        // Another repository has its own cooldown; after a minute focus asks again.
        assert!(
            prs.ask("/q", Trigger::Focus, soon, &one, view, no_list)
                .is_some()
        );
        let later = soon + crate::map::COOLDOWN;
        assert!(
            prs.ask("/p", Trigger::Focus, later, &one, view, no_list)
                .is_some()
        );
        assert_eq!(asks.get(), 4);
    }

    #[test]
    fn a_lookup_in_flight_blocks_every_trigger() {
        let prs = Prs::default();
        let start = Instant::now();
        let inner = std::cell::Cell::new(None);
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            |b| {
                inner.set(Some(prs.ask(
                    "/p",
                    Trigger::TurnEnd,
                    start,
                    &["fog".into()],
                    recorded,
                    no_list,
                )));
                recorded(b)
            },
            no_list,
        );
        assert_eq!(inner.take(), Some(None));
    }

    fn list_of(prs: &[&str]) -> impl Fn() -> Option<String> {
        let json = format!("[{}]", prs.join(","));
        move || Some(json.clone())
    }
    const HAND_MADE: &str = r#"{"number":21,"title":"By hand","isDraft":false,"state":"OPEN","headRefOid":"eee555","headRefName":"by-hand","body":"","closingIssuesReferences":[]}"#;

    #[test]
    fn a_pr_no_session_has_a_branch_for_is_ready_from_the_list_of_open_prs() {
        let prs = Prs::default();
        let start = Instant::now();
        let found = prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &[],
            recorded,
            list_of(&[HAND_MADE]),
        );
        assert_eq!(found, Some(vec![]));
        let ready = prs.ready("/p");
        assert_eq!(ready.len(), 1);
        assert_eq!((ready[0].number, ready[0].head.as_str()), (21, "eee555"));
    }

    #[test]
    fn a_pr_no_session_has_a_branch_for_takes_its_new_head_from_the_next_list() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &[],
            recorded,
            list_of(&[HAND_MADE]),
        );
        let pushed = HAND_MADE.replace("eee555", "fff666");
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &[],
            recorded,
            list_of(&[&pushed]),
        );
        assert_eq!(prs.ready("/p")[0].head, "fff666");
    }

    #[test]
    fn a_pr_that_left_the_list_and_has_no_session_leaves_the_tray() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &[],
            recorded,
            list_of(&[HAND_MADE]),
        );
        // Merged since: the list no longer holds it, and no session asks about its branch.
        prs.ask("/p", Trigger::TurnEnd, start, &[], recorded, list_of(&[]));
        assert!(prs.ready("/p").is_empty());
    }

    #[test]
    fn a_failed_lookup_keeps_what_was_known() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            recorded,
            list_of(&[HAND_MADE]),
        );
        assert_eq!(prs.ready("/p").len(), 2);
        // `gh` fails for the branch and for the list: both PRs stay, and the session's name is not touched.
        let found = prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            |_| Looked::Failed,
            no_list,
        );
        assert_eq!(found, Some(vec![]));
        assert_eq!(prs.ready("/p").len(), 2);
        assert!(prs.listed("/p"), "the list answered once");
        assert!(!prs.listed("/q"));
        // `gh` says the branch has no PR any more: it goes.
        let found = prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            |_| Looked::NoPr,
            no_list,
        );
        assert_eq!(found, Some(vec![("fog".into(), None)]));
        assert_eq!(prs.ready("/p").len(), 1);
    }

    #[test]
    fn a_session_branch_that_is_no_longer_asked_about_is_forgotten() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask(
            "/p",
            Trigger::TurnEnd,
            start,
            &["fog".into()],
            recorded,
            list_of(&[]),
        );
        assert_eq!(prs.ready("/p").len(), 1);
        // The session closed: its branch is not asked about, and the list (which answered) does not hold the PR.
        prs.ask("/p", Trigger::TurnEnd, start, &[], recorded, list_of(&[]));
        assert!(prs.ready("/p").is_empty());
    }

    #[test]
    fn output_that_is_not_a_pr_is_none() {
        assert_eq!(parse("no pull requests found"), None);
    }
}
