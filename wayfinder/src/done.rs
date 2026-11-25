//! `--done`: remove a merged ticket's worktree and branch.

use crate::claim::live_worktrees;
use crate::{REPO, Result, gh, require, stdout};
use serde_json::Value;
use std::env;

#[derive(Debug, PartialEq)]
pub(crate) struct DonePlan {
    pub(crate) branch: String,
    /// The live worktree that has the branch checked out, if any.
    pub(crate) worktree: Option<String>,
}
/// Decide whether `--done` may delete the ticket's local branch and worktree: only when a
/// merged PR ended on the branch's current commit, so nothing unmerged is lost.
/// `branches` is `<name> <sha>` per local `issue-<n>-*` branch; `fetch_merged` lists the branch's
/// merged PRs as `headRefName,headRefOid`; `cwd` must lie outside the worktree.
pub(crate) fn done_plan(
    number: u64,
    worktrees: &str,
    branches: &str,
    fetch_merged: impl FnOnce(&str) -> Result<Value>,
    cwd: &std::path::Path,
) -> Result<DonePlan> {
    let prefix = format!("issue-{number}-");
    let ours = branches
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(name, _)| name.starts_with(&prefix))
        .collect::<Vec<_>>();
    let [(branch, head)] = ours.as_slice() else {
        let names = ours.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        return Err(format!(
            "#{number}: expected one local {prefix}* branch, found [{}]",
            names.join(", ")
        )
        .into());
    };
    let merged_here = fetch_merged(branch)?
        .as_array()
        .ok_or("expected a merged PR list")?
        .iter()
        .any(|pr| pr["headRefName"] == *branch && pr["headRefOid"] == *head);
    require(
        merged_here,
        &format!("{branch}: no merged PR ends at {head}; it has unmerged work or its PR is open"),
    )?;
    let mut worktree = None;
    for (path, _) in live_worktrees(worktrees).filter(|(_, name)| name == branch) {
        require(
            !cwd.starts_with(path),
            &format!("run --done from outside {path}"),
        )?;
        worktree = Some(path.to_owned());
    }
    Ok(DonePlan {
        branch: (*branch).to_owned(),
        worktree,
    })
}
/// Files `git worktree remove` would lose: `git status --porcelain --ignored` lines other
/// than an ignored Cargo `target/` build directory, at the root or in a nested crate, or the
/// ignored `dist/`, `node_modules/`, `archive/src-tauri/gen/` and `scripts/wire/<name>.ts` that `npm ci`
/// and the check rebuild.
pub(crate) fn leftovers(status: &str) -> Vec<&str> {
    status
        .lines()
        .filter(|line| {
            !line.strip_prefix("!! ").is_some_and(|path| {
                path == "target/"
                    || path.ends_with("/target/")
                    || ["dist/", "node_modules/", "archive/src-tauri/gen/"].contains(&path)
                    || path
                        .strip_prefix("scripts/wire/")
                        .is_some_and(|name| name.ends_with(".ts") && !name.contains('/'))
            })
        })
        .collect()
}
/// Refuse while an open PR is based on `branch`: deleting the branch would close it. `open_prs`
/// is `gh pr list --base <branch> --state open --json number`.
pub(crate) fn stacked_refusal(branch: &str, open_prs: &Value) -> Result<()> {
    let numbers = open_prs
        .as_array()
        .ok_or("expected an open PR list")?
        .iter()
        .filter_map(|pr| pr["number"].as_u64())
        .collect::<Vec<_>>();
    require(
        numbers.is_empty(),
        &format!(
            "open PRs are based on {branch}; deleting it would close them. Retarget each first:\n{}",
            numbers
                .iter()
                .map(|m| format!("#{m}: gh pr edit {m} --base main"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
}
/// After merge: remove the ticket's worktree and local branch once `done_plan` allows it.
pub(crate) fn done(number: u64) -> Result<()> {
    let branches = stdout(
        "git",
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname)",
            &format!("refs/heads/issue-{number}-*"),
        ],
    )?;
    let plan = done_plan(
        number,
        &stdout("git", &["worktree", "list", "--porcelain"])?,
        &branches,
        |branch| {
            gh(&[
                "pr",
                "list",
                "--repo",
                REPO,
                "--state",
                "merged",
                "--head",
                branch,
                "--json",
                "headRefName,headRefOid",
            ])
        },
        // Resolved, so a symlinked path into the worktree is still caught.
        &env::current_dir()?.canonicalize()?,
    )?;
    stacked_refusal(
        &plan.branch,
        &gh(&[
            "pr",
            "list",
            "--repo",
            REPO,
            "--state",
            "open",
            "--base",
            &plan.branch,
            "--json",
            "number",
        ])?,
    )?;
    if let Some(path) = &plan.worktree {
        let status = stdout("git", &["-C", path, "status", "--porcelain", "--ignored"])?;
        let left = leftovers(&status);
        require(
            left.is_empty(),
            &format!(
                "{path} has uncommitted, untracked or ignored files:\n{}",
                left.join("\n")
            ),
        )?;
        stdout("git", &["worktree", "remove", path])?;
        println!("removed worktree {path}");
    }
    // A prunable worktree record still holds the branch; drop it so the branch can go.
    stdout("git", &["worktree", "prune"])?;
    // -D: a squash merge leaves the branch unmerged in Git's eyes; done_plan checked the PR head.
    stdout("git", &["branch", "-D", &plan.branch])?;
    println!("deleted branch {}", plan.branch);
    Ok(())
}
#[cfg(test)]
mod done_tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;
    const WORKTREES: &str = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo/.claude/worktrees/issue-47-x\nHEAD aaa\nbranch refs/heads/issue-47-x\n";
    const BRANCH: &str = "issue-47-x aaa\n";
    fn merged_at(sha: &str) -> Value {
        json!([{"headRefName":"issue-47-x","headRefOid":sha}])
    }
    fn plan(worktrees: &str, branches: &str, merged: &Value, cwd: &str) -> Result<DonePlan> {
        done_plan(
            47,
            worktrees,
            branches,
            |_| Ok(merged.clone()),
            Path::new(cwd),
        )
    }
    #[test]
    fn branch_merged_at_its_head_loses_worktree_and_branch() {
        assert_eq!(
            plan(WORKTREES, BRANCH, &merged_at("aaa"), "/repo").unwrap(),
            DonePlan {
                branch: "issue-47-x".into(),
                worktree: Some("/repo/.claude/worktrees/issue-47-x".into()),
            }
        );
    }
    #[test]
    fn branch_without_worktree_is_still_deleted() {
        let plan = plan(
            "worktree /repo\nbranch refs/heads/main\n",
            BRANCH,
            &merged_at("aaa"),
            "/repo",
        );
        assert_eq!(plan.unwrap().worktree, None);
    }
    /// Commits after the merged head, or no merged PR at all, keep the branch.
    #[test]
    fn unmerged_or_newer_branch_is_kept() {
        for merged in [merged_at("old"), json!([])] {
            let error = plan(WORKTREES, BRANCH, &merged, "/repo").unwrap_err();
            assert!(
                error.to_string().contains("no merged PR ends at aaa"),
                "{error}"
            );
        }
    }
    #[test]
    fn running_inside_the_worktree_is_refused() {
        let error = plan(
            WORKTREES,
            BRANCH,
            &merged_at("aaa"),
            "/repo/.claude/worktrees/issue-47-x/src",
        )
        .unwrap_err();
        assert!(error.to_string().contains("from outside"), "{error}");
    }
    #[test]
    fn missing_or_ambiguous_branch_is_refused_before_asking_github() {
        for branches in ["issue-470-y bbb\n", "issue-47-x aaa\nissue-47-y bbb\n"] {
            let fetch = |_: &str| -> Result<Value> { panic!("fetched merged PRs") };
            assert!(done_plan(47, WORKTREES, branches, fetch, Path::new("/repo")).is_err());
        }
    }
    #[test]
    fn prunable_worktree_is_not_removed() {
        let worktrees =
            format!("{WORKTREES}prunable gitdir file points to non-existent location\n");
        let plan = plan(&worktrees, BRANCH, &merged_at("aaa"), "/repo").unwrap();
        assert_eq!(plan.worktree, None);
    }
    #[test]
    fn rebuilt_output_directories_may_be_left_behind() {
        let ignored = "!! target/\n!! dist/\n!! node_modules/\n!! archive/src-tauri/gen/\n";
        assert!(leftovers(ignored).is_empty());
        assert_eq!(
            leftovers(
                "?? dist/\n M dist/index.html\n?? node_modules/\n!! archive/src-tauri/gen/x\n"
            ),
            [
                "?? dist/",
                " M dist/index.html",
                "?? node_modules/",
                "!! archive/src-tauri/gen/x"
            ]
        );
    }
    #[test]
    fn wire_files_the_check_writes_may_be_left_behind() {
        let written = "!! scripts/wire/core.ts\n!! scripts/wire/understanding.ts\n";
        assert!(leftovers(written).is_empty());
        assert_eq!(
            leftovers("?? scripts/wire/new.ts\n!! scripts/wire/x/a.ts\n!! scripts/wire/a.rs\n"),
            [
                "?? scripts/wire/new.ts",
                "!! scripts/wire/x/a.ts",
                "!! scripts/wire/a.rs"
            ]
        );
    }
    #[test]
    fn open_prs_based_on_the_branch_refuse_with_the_retarget_command() {
        let listed = serde_json::json!([{"number": 272}, {"number": 290}]);
        let message = stacked_refusal("issue-253-x", &listed)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("#272") && message.contains("#290"),
            "{message}"
        );
        assert!(message.contains("gh pr edit 272 --base main"), "{message}");
        assert!(message.contains("gh pr edit 290 --base main"), "{message}");
        assert!(stacked_refusal("issue-253-x", &serde_json::json!([])).is_ok());
    }
    #[test]
    fn only_the_build_directory_may_be_left_behind() {
        assert!(leftovers("!! evaluation/target/\n!! target/\n").is_empty());
        assert_eq!(
            leftovers("!! target/\n!! .env\n!! target.bak/\n?? target/\n M src/lib.rs\n"),
            ["!! .env", "!! target.bak/", "?? target/", " M src/lib.rs"]
        );
    }
}
