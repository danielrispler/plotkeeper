//! The wire's agreement check (#246) for the workbench's types: the picker's workspaces, a workspace's git state, the
//! branches, what removing a task would lose and the plan usage, written into `scripts/wire/workbench.ts`
//! (`scripts/wire/samples.rs`). Each optional field is filled once and left empty once; an optional field no sample
//! fills is not checked.

#[path = "../../scripts/wire/samples.rs"]
mod samples;

use samples::{Sample, json, write};
use studio_workbench::{
    AgentUsage, Branch, GitState, LimitGroup, Losses, UsageWindow, Workspace, Workspaces,
};

#[test]
fn the_wire_samples_of_the_workbench_are_written_for_tsc() {
    let task = |branch: Option<&str>| Workspace {
        path: "/repo/.claude/worktrees/fix-typo".into(),
        branch: branch.map(Into::into),
    };
    let workspaces = Workspaces {
        main: Workspace {
            path: "/repo".into(),
            branch: Some("main".into()),
        },
        tasks: vec![task(Some("fix-typo")), task(None)],
        hidden: 2,
    };
    let state = GitState {
        branch: Some("fix-typo".into()),
        upstream: Some((1, 2)),
        changed: 3,
        operation: Some("rebase".into()),
        main_behind: Some(4),
        on_default: false,
        would_conflict: true,
        would_overwrite: true,
    };
    let detached = GitState {
        branch: None,
        upstream: None,
        operation: None,
        main_behind: None,
        ..state
    };
    let branch = |task: Option<&str>| Branch {
        name: "fix-typo".into(),
        remote: task.is_none(),
        task: task.map(Into::into),
        current: false,
    };
    let losses = Losses {
        uncommitted: vec![" M src/lib.rs".into()],
        unpushed: vec!["5f0c1e2 Fix the typo".into()],
    };
    let usage = AgentUsage {
        agent: "claude".into(),
        groups: vec![LimitGroup {
            name: String::new(),
            windows: vec![UsageWindow {
                label: "5h".into(),
                used_percentage: 23.5,
                resets_at: 1_738_425_600,
            }],
        }],
        read_at: Some(1_738_420_000),
        source: Some("last Claude session".into()),
        reason: None,
    };

    let samples: Vec<Sample> = vec![
        ("Workspaces", json(&workspaces)),
        ("Workspace", json(&workspaces.main)),
        ("GitState", json(&state)),
        ("GitState", json(&detached)),
        ("Branch", json(branch(Some("fix-typo")))),
        ("Branch", json(branch(None))),
        ("Losses", json(&losses)),
        ("AgentUsage", json(&usage)),
        (
            "AgentUsage",
            json(AgentUsage::failed("codex", "Codex is not installed")),
        ),
    ];
    let types = [
        "Workspaces",
        "Workspace",
        "GitState",
        "Branch",
        "Losses",
        "AgentUsage",
    ];
    write(env!("CARGO_MANIFEST_DIR"), "workbench", &types, &samples);
}
