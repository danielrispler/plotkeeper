//! Seam 2 (danielrispler/Falinks#328 and danielrispler/Falinks#461, Testing Decisions): `fetch` with real `git` in
//! temporary repositories, as the engine's tests do (`src/lib.rs`, `retention_checks_every_blob_of_a_snapshot`).

use std::process::Command;
use studio_test_support::{git, repo};
use studio_workbench::fetch;

/// A repository whose `main` follows `origin/main`, a bare repository in the temp folder.
fn pushed() -> (tempfile::TempDir, String, std::path::PathBuf) {
    let (dir, main) = repo();
    let origin = dir.path().join(".git/origin.git");
    git(
        dir.path(),
        &["clone", "-q", "--bare", ".", origin.to_str().unwrap()],
    );
    git(
        dir.path(),
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    git(dir.path(), &["fetch", "-q", "origin"]);
    git(dir.path(), &["branch", "-q", "-u", "origin/main"]);
    (dir, main, origin)
}

/// A fetch brings `origin`'s new commits (#33).
#[test]
fn a_fetch_moves_origins_default_branch() {
    let (dir, _main, origin) = pushed();
    let other = tempfile::tempdir().unwrap();
    git(
        other.path(),
        &["clone", "-q", origin.to_str().unwrap(), "c"],
    );
    let clone = other.path().join("c");
    git(&clone, &["commit", "-q", "--allow-empty", "-m", "Theirs"]);
    git(&clone, &["push", "-q", "origin", "main"]);
    let theirs = git(&clone, &["rev-parse", "HEAD"]);
    let tip = || git(dir.path(), &["rev-parse", "origin/main"]);
    assert_ne!(tip(), theirs);

    fetch(dir.path()).unwrap();

    assert_eq!(tip(), theirs);
}

/// A failed fetch says why and leaves the refs as they were (#33).
#[test]
fn a_fetch_from_a_missing_remote_fails_with_gits_message_and_keeps_the_refs() {
    let (dir, _main, origin) = pushed();
    let refs = || {
        let out = Command::new("git")
            .args(["for-each-ref"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap()
    };
    let before = refs();
    std::fs::rename(&origin, dir.path().join(".git/gone.git")).unwrap();

    let error = fetch(dir.path()).unwrap_err();

    assert!(error.contains("origin.git"), "{error}");
    assert_eq!(refs(), before);
}
