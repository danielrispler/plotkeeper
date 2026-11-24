//! Seam 2 (danielrispler/Falinks#328 and danielrispler/Falinks#461, Testing Decisions): what removing a task's worktree
//! would lose (danielrispler/Falinks#468), with real `git` in temporary repositories that have a bare `origin`.

use std::path::Path;
use studio_test_support::{git, origin};
use studio_workbench::{Losses, losses};

#[test]
fn losses_are_empty_for_a_clean_pushed_task_and_list_new_work() {
    let repos = origin();
    let path = &repos.main.join(".claude/worktrees/shipped");
    let at = path.to_str().unwrap();
    git(&repos.main, &["worktree", "add", "-q", "-b", "shipped", at]);
    git(path, &["commit", "-q", "--allow-empty", "-m", "Work"]);
    git(path, &["push", "-q", "origin", "shipped"]);
    let nothing = Losses {
        uncommitted: vec![],
        unpushed: vec![],
    };
    assert_eq!(losses(path), Ok(nothing));

    // New work after the push shows at once: a commit, then a file.
    git(path, &["commit", "-q", "--allow-empty", "-m", "More"]);
    assert_eq!(losses(path).unwrap().unpushed.len(), 1);
    git(path, &["push", "-q", "origin", "shipped"]);
    std::fs::write(path.join("new.txt"), "x").unwrap();
    let found = losses(path).unwrap();
    assert_eq!(
        (found.uncommitted, found.unpushed),
        (vec!["?? new.txt".into()], vec![])
    );
}

/// No code path forces git (danielrispler/Falinks#461, decision 9): the studio's own sources, the workbench, the server
/// and the UI, never spell git's force option.
#[test]
fn no_code_path_passes_force() {
    let studio = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut checked = 0;
    for dir in ["workbench/src", "server/src", "ui"] {
        for entry in std::fs::read_dir(studio.join(dir)).unwrap() {
            let file = entry.unwrap().path();
            if !matches!(file.extension().and_then(|e| e.to_str()), Some("rs" | "ts")) {
                continue;
            }
            let source = std::fs::read_to_string(&file).unwrap();
            for force in ["--force", "\"-f\"", "'-f'"] {
                assert!(!source.contains(force), "{} has {force}", file.display());
            }
            checked += 1;
        }
    }
    assert!(checked > 5, "only {checked} files checked");
}
