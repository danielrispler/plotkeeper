//! `--claim`: assign a ticket and set up its worktree.

use crate::issues::logins;
use crate::touches::{Overlap, Stale, in_checkout, overlaps, stale_touches};
use crate::{REPO, Result, blockers_of, gh, open_issue_pages, require, stdout};
use serde_json::Value;

/// Path and branch of each live worktree in `git worktree list --porcelain`. A prunable
/// worktree's directory is gone, so it is skipped: it cannot be reused or removed.
pub(crate) fn live_worktrees(porcelain: &str) -> impl Iterator<Item = (&str, &str)> {
    porcelain.split("\n\n").filter_map(|stanza| {
        let field = |key: &str| stanza.lines().find_map(|line| line.strip_prefix(key));
        match (
            field("worktree "),
            field("branch refs/heads/"),
            field("prunable"),
        ) {
            (Some(path), Some(branch), None) => Some((path, branch)),
            _ => None,
        }
    })
}
#[derive(Debug, PartialEq)]
pub(crate) enum Workspace {
    /// A worktree already has the ticket branch checked out; use it as is.
    Reuse,
    /// The ticket branch exists without a worktree; check it out.
    CheckoutBranch,
    /// Only `origin` has the ticket branch (pushed from another clone); track it.
    TrackRemote,
    /// Neither exists; branch from up-to-date `origin/main`.
    Create,
}
#[derive(Debug, PartialEq)]
pub(crate) struct ClaimPlan {
    assign: bool,
    branch: String,
    path: String,
    workspace: Workspace,
}
/// Decide whether `--claim` may take `issue` and how to set up its worktree.
/// `worktrees` is `git worktree list --porcelain`; `branches` lists local and `origin/` branch names one per line.
fn claim_plan(
    issue: &Value,
    open_blockers: &[u64],
    caller: &str,
    root: &str,
    worktrees: &str,
    branches: &str,
) -> Result<ClaimPlan> {
    let number = issue["number"].as_u64().ok_or("missing issue number")?;
    let title = issue["title"].as_str().ok_or("missing issue title")?;
    let slug = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_ascii_lowercase();
    require(issue["state"] == "OPEN", &format!("#{number} is closed"))?;
    let assignees = logins(&issue["assignees"])?;
    let others = assignees
        .iter()
        .filter(|login| **login != caller)
        .copied()
        .collect::<Vec<_>>();
    require(
        others.is_empty(),
        &format!("#{number} is assigned to {}", others.join(", ")),
    )?;
    let blockers = open_blockers
        .iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>();
    require(
        blockers.is_empty(),
        &format!("#{number} is blocked by {}", blockers.join(", ")),
    )?;
    let prefix = format!("issue-{number}-");
    let ours = |name: &str| name.starts_with(&prefix);
    let assign = assignees.is_empty();
    if let Some((path, branch)) = live_worktrees(worktrees).find(|(_, branch)| ours(branch)) {
        return Ok(ClaimPlan {
            assign,
            branch: branch.into(),
            path: path.into(),
            workspace: Workspace::Reuse,
        });
    }
    let names = branches.lines().map(str::trim);
    let local = names.clone().find(|name| ours(name));
    let remote = names
        .filter_map(|name| name.strip_prefix("origin/"))
        .find(|name| ours(name));
    let (branch, workspace) = match (local, remote) {
        (Some(existing), _) => (existing.to_owned(), Workspace::CheckoutBranch),
        (None, Some(pushed)) => (pushed.to_owned(), Workspace::TrackRemote),
        (None, None) => (format!("{prefix}{slug}"), Workspace::Create),
    };
    Ok(ClaimPlan {
        assign,
        path: format!("{root}/.claude/worktrees/{branch}"),
        branch,
        workspace,
    })
}
/// Assign the ticket, then create or reuse its worktree.
pub(crate) fn claim(number: u64) -> Result<()> {
    let id = number.to_string();
    let issue = gh(&[
        "issue",
        "view",
        &id,
        "--repo",
        REPO,
        "--json",
        "number,state,title,assignees",
    ])?;
    let caller = gh(&["api", "user"])?["login"]
        .as_str()
        .ok_or("missing caller login")?
        .to_owned();
    let common = stdout(
        "git",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let root = std::path::Path::new(common.trim())
        .parent()
        .ok_or("git common dir has no parent")?
        .to_str()
        .ok_or("non-UTF-8 repo path")?
        .to_owned();
    // Fetch first so a ticket branch pushed from another clone is seen.
    stdout("git", &["fetch", "origin"])?;
    let plan = claim_plan(
        &issue,
        &blockers_of(number)?,
        &caller,
        &root,
        &stdout("git", &["worktree", "list", "--porcelain"])?,
        &stdout(
            "git",
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                &format!("refs/heads/issue-{number}-*"),
                &format!("refs/remotes/origin/issue-{number}-*"),
            ],
        )?,
    )?;
    if plan.assign {
        stdout(
            "gh",
            &[
                "issue",
                "edit",
                &id,
                "--repo",
                REPO,
                "--add-assignee",
                "@me",
            ],
        )?;
    }
    make_worktree(&plan)?;
    warn_touches(number, &plan.path);
    println!("{}", plan.path);
    Ok(())
}

/// Reuses the ticket's worktree, or makes it from its local branch, its origin branch or a new one.
fn make_worktree(plan: &ClaimPlan) -> Result<()> {
    match plan.workspace {
        Workspace::Reuse => {}
        Workspace::CheckoutBranch => {
            // A prunable worktree may still hold the branch; drop its stale record.
            stdout("git", &["worktree", "prune"])?;
            stdout("git", &["worktree", "add", &plan.path, &plan.branch])?;
        }
        Workspace::TrackRemote => {
            stdout(
                "git",
                &[
                    "worktree",
                    "add",
                    "--track",
                    "-b",
                    &plan.branch,
                    &plan.path,
                    &format!("origin/{}", plan.branch),
                ],
            )?;
        }
        Workspace::Create => {
            // --no-track: a plain `git push` must not target main.
            stdout(
                "git",
                &[
                    "worktree",
                    "add",
                    "--no-track",
                    "-b",
                    &plan.branch,
                    &plan.path,
                    "origin/main",
                ],
            )?;
        }
    }
    Ok(())
}

/// Warns of another ticket whose Touches overlap this one's, and of a path its Touches names that
/// is not in its worktree at `path`. Only warnings: a failed lookup, an overlap or a stale path never
/// blocks the claim.
fn warn_touches(number: u64, path: &str) {
    let warnings = open_issue_pages().and_then(|pages| {
        let mut lines: Vec<String> = overlaps(&pages)?
            .iter()
            .filter(|overlap| overlap.first == number || overlap.second == number)
            .map(Overlap::warning)
            .collect();
        // The ticket's own worktree is the checkout its implementer will see.
        let stale = stale_touches(&pages, in_checkout(path))?;
        lines.extend(
            stale
                .iter()
                .filter(|stale| stale.ticket == number)
                .map(Stale::warning),
        );
        Ok(lines)
    });
    match warnings {
        Ok(lines) => lines.iter().for_each(|line| eprintln!("{line}")),
        Err(error) => eprintln!("could not check the ticket's Touches: {error}"),
    }
}
#[cfg(test)]
mod claim_tests {
    use super::*;
    use serde_json::json;
    const ROOT: &str = "/repo";
    const WORKTREES: &str = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n";
    fn ticket() -> Value {
        json!({"number":47,"state":"OPEN","title":"wayfinder-startup --claim: assign and set up worktree","assignees":[]})
    }
    #[test]
    fn eligible_ticket_is_assigned_and_gets_a_new_worktree_from_main() {
        let plan = claim_plan(&ticket(), &[], "dev", ROOT, WORKTREES, "").unwrap();
        assert_eq!(
            plan,
            ClaimPlan {
                assign: true,
                branch: "issue-47-wayfinder-startup-claim-assign".into(),
                path: "/repo/.claude/worktrees/issue-47-wayfinder-startup-claim-assign".into(),
                workspace: Workspace::Create,
            }
        );
    }
    fn refusal(issue: &Value, blockers: &[u64]) -> String {
        claim_plan(issue, blockers, "dev", ROOT, WORKTREES, "")
            .unwrap_err()
            .to_string()
    }
    #[test]
    fn ticket_assigned_to_another_user_is_refused() {
        let mut issue = ticket();
        issue["assignees"] = json!([{"login":"other"}]);
        let error = refusal(&issue, &[]);
        assert!(error.contains("#47 is assigned to other"), "{error}");
    }
    #[test]
    fn ticket_already_assigned_to_caller_is_not_reassigned() {
        let mut issue = ticket();
        issue["assignees"] = json!([{"login":"dev"}]);
        let plan = claim_plan(&issue, &[], "dev", ROOT, WORKTREES, "").unwrap();
        assert!(!plan.assign);
    }
    #[test]
    fn blocked_ticket_is_refused_naming_open_blockers() {
        let error = refusal(&ticket(), &[46, 48]);
        assert!(error.contains("#47 is blocked by #46, #48"), "{error}");
    }
    #[test]
    fn existing_worktree_for_the_ticket_is_reused_wherever_it_lives() {
        let worktrees = format!(
            "{WORKTREES}\nworktree /repo/.claude/worktrees/issue-470-other\nHEAD def\nbranch refs/heads/issue-470-other\n\nworktree /elsewhere/agent-1\nHEAD 123\nbranch refs/heads/issue-47-older-title\n"
        );
        let plan = claim_plan(&ticket(), &[], "dev", ROOT, &worktrees, "").unwrap();
        assert_eq!(plan.workspace, Workspace::Reuse);
        assert_eq!(plan.path, "/elsewhere/agent-1");
        assert_eq!(plan.branch, "issue-47-older-title");
    }
    #[test]
    fn existing_branch_without_worktree_is_checked_out() {
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            WORKTREES,
            "main\nissue-470-other\nissue-47-older-title\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::CheckoutBranch);
        assert_eq!(plan.branch, "issue-47-older-title");
        assert_eq!(plan.path, "/repo/.claude/worktrees/issue-47-older-title");
    }
    #[test]
    fn prunable_worktree_is_not_reused() {
        let worktrees = format!(
            "{WORKTREES}\nworktree /gone/agent-1\nHEAD 123\nbranch refs/heads/issue-47-older-title\nprunable gitdir file points to non-existent location\n"
        );
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            &worktrees,
            "issue-47-older-title\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::CheckoutBranch);
        assert_eq!(plan.path, "/repo/.claude/worktrees/issue-47-older-title");
    }
    #[test]
    fn remote_only_branch_is_tracked_into_a_new_worktree() {
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            WORKTREES,
            "origin/issue-470-other\norigin/issue-47-pushed-elsewhere\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::TrackRemote);
        assert_eq!(plan.branch, "issue-47-pushed-elsewhere");
        assert_eq!(
            plan.path,
            "/repo/.claude/worktrees/issue-47-pushed-elsewhere"
        );
    }
    #[test]
    fn local_branch_wins_over_remote_branch() {
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            WORKTREES,
            "origin/issue-47-older-title\nissue-47-older-title\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::CheckoutBranch);
    }
    #[test]
    fn closed_ticket_is_refused() {
        let mut issue = ticket();
        issue["state"] = json!("CLOSED");
        let error = refusal(&issue, &[]);
        assert!(error.contains("#47 is closed"), "{error}");
    }
}
