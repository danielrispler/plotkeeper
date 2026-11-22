//! Which pull requests are ready for review (spec danielrispler/plotkeeper#334, ticket #336). The shell asks `gh pr view
//! <branch>` for each session's branch on Landed's triggers, never on a timer: a turn's end at once, and window focus and the
//! Map view's opening through one per-repository cooldown with an in-flight guard, as the Map view's fetch has. A PR is ready
//! when it is open and not a draft; its head is kept so a later ticket can tell a new push.

use crate::map::{Fetched, Fetches};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};

/// What `gh pr view` is asked for.
pub const FIELDS: &str = "number,title,isDraft,state,headRefOid,body,closingIssuesReferences";

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

/// A ready PR as the Map view's tray lists it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ready {
    pub number: u64,
    pub title: String,
    pub head: String,
    /// What the PR's brief is doing (`pr_briefs.rs`); `prs.ready` leaves it empty and the Map view's payload fills it.
    pub brief: Option<crate::pr_briefs::BriefInfo>,
}

/// The PRs found for each project's branches, and the guard of the lookups.
#[derive(Default)]
pub struct Prs {
    guard: Fetches,
    found: Mutex<HashMap<String, BTreeMap<String, Pr>>>,
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
        view: impl Fn(&str) -> Option<String>,
    ) -> Option<Vec<(String, Option<u64>)>> {
        let cooldown = match trigger {
            Trigger::TurnEnd => Duration::ZERO,
            Trigger::Focus | Trigger::MapOpen => crate::map::COOLDOWN,
        };
        let mut asked = Vec::new();
        let outcome = self
            .guard
            .run_after(&format!("prs:{project}"), now, cooldown, || {
                for branch in branches {
                    let pr = view(branch).and_then(|json| parse(&json));
                    asked.push((branch.clone(), pr));
                }
                Ok(())
            });
        if outcome == Fetched::Skipped {
            return None;
        }
        let mut found = self.found.lock().unwrap();
        let of_project = found.entry(project.to_string()).or_default();
        let mut numbers = Vec::new();
        for (branch, pr) in asked {
            numbers.push((branch.clone(), pr.as_ref().map(|p| p.number)));
            match pr {
                Some(pr) => {
                    of_project.insert(branch, pr);
                }
                None => {
                    of_project.remove(&branch);
                }
            }
        }
        Some(numbers)
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
                brief: None,
            })
            .collect();
        ready.sort_by_key(|r| r.number);
        ready
    }
}

/// Runs `gh pr view <branch>` in `cwd`, as Daniel with its own login; no credential file is read.
fn gh_view(cwd: &str, branch: &str) -> Option<String> {
    let output = std::process::Command::new(studio_workbench::gh_path())
        .args(["pr", "view", branch, "--json", FIELDS])
        .current_dir(cwd)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Asks about the PRs of every listed session's branch on `trigger`, on a thread of its own so no loop waits on the network.
/// A session's name takes the PR number found; the Map view is told (`map-landed`) when a project's ready PRs changed.
pub fn ask_later(app: &AppHandle, trigger: Trigger) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut projects = BTreeMap::<String, Vec<(String, String)>>::new();
        for (cwd, branch) in app.state::<crate::Core>().lock().unwrap().branches() {
            if let Ok(project) = studio_workbench::main_checkout(Path::new(&cwd)) {
                projects.entry(project).or_default().push((cwd, branch));
            }
        }
        let prs = app.state::<Prs>();
        for (project, sessions) in projects {
            let before = prs.ready(&project);
            let branches: Vec<String> = sessions.iter().map(|(_, b)| b.clone()).collect();
            let cwd_of = |branch: &str| {
                sessions
                    .iter()
                    .find(|(_, b)| b == branch)
                    .map(|(cwd, _)| cwd.clone())
            };
            let asked = prs.ask(&project, trigger, Instant::now(), &branches, |branch| {
                gh_view(&cwd_of(branch)?, branch)
            });
            let Some(asked) = asked else { continue };
            for (branch, number) in asked {
                if app
                    .state::<crate::Core>()
                    .lock()
                    .unwrap()
                    .set_pr(&branch, number)
                {
                    crate::LIST_CHANGED.store(true, std::sync::atomic::Ordering::Release);
                }
            }
            let ready = prs.ready(&project);
            if ready != before {
                let _ = app.emit("map-landed", project.clone());
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

    fn recorded(branch: &str) -> Option<String> {
        match branch {
            "fog" => Some(OPEN.into()),
            "wip" => Some(DRAFT.into()),
            "gone" => Some(CLOSED.into()),
            _ => None,
        }
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
        prs.ask("/p", Trigger::TurnEnd, start, &["fog".into()], recorded);
        assert_eq!(prs.ready("/p")[0].head, "aaa111");
        let pushed = |_: &str| Some(PUSHED.to_string());
        prs.ask("/p", Trigger::TurnEnd, start, &["fog".into()], pushed);
        assert_eq!(prs.ready("/p")[0].head, "bbb222");
    }

    #[test]
    fn a_pr_that_is_no_longer_ready_leaves_the_tray() {
        let prs = Prs::default();
        let start = Instant::now();
        prs.ask("/p", Trigger::TurnEnd, start, &["fog".into()], recorded);
        prs.ask("/p", Trigger::TurnEnd, start, &["fog".into()], |_| {
            Some(DRAFT.into())
        });
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
        assert!(prs.ask("/p", Trigger::MapOpen, start, &one, view).is_some());
        // Focus within the cooldown asks nothing.
        let soon = start + Duration::from_secs(30);
        assert!(prs.ask("/p", Trigger::Focus, soon, &one, view).is_none());
        assert!(prs.ask("/p", Trigger::MapOpen, soon, &one, view).is_none());
        assert_eq!(asks.get(), 1);
        // A turn's end asks at once, whatever the cooldown.
        assert!(prs.ask("/p", Trigger::TurnEnd, soon, &one, view).is_some());
        assert_eq!(asks.get(), 2);
        // Another repository has its own cooldown; after a minute focus asks again.
        assert!(prs.ask("/q", Trigger::Focus, soon, &one, view).is_some());
        let later = soon + crate::map::COOLDOWN;
        assert!(prs.ask("/p", Trigger::Focus, later, &one, view).is_some());
        assert_eq!(asks.get(), 4);
    }

    #[test]
    fn a_lookup_in_flight_blocks_every_trigger() {
        let prs = Prs::default();
        let start = Instant::now();
        let inner = std::cell::Cell::new(None);
        prs.ask("/p", Trigger::TurnEnd, start, &["fog".into()], |b| {
            inner.set(Some(prs.ask(
                "/p",
                Trigger::TurnEnd,
                start,
                &["fog".into()],
                recorded,
            )));
            recorded(b)
        });
        assert_eq!(inner.take(), Some(None));
    }

    #[test]
    fn output_that_is_not_a_pr_is_none() {
        assert_eq!(parse("no pull requests found"), None);
    }
}
