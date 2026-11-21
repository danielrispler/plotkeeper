//! What the crates' tests share: `git` run away from the developer's own configuration, and the
//! temporary repositories the tests start from.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Runs `git` in `dir` as the studio's own identity, away from the developer's git config, and
/// returns its output without the trailing newline.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}");
    String::from_utf8_lossy(&output.stdout).trim_end().into()
}

/// A repository with one empty commit on `main`, and its real path as `git` spells it.
pub fn repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().canonicalize().unwrap().display().to_string();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "Start"],
    );
    (dir, main)
}

/// A bare `origin` whose default branch is `trunk`, with the commit `First` pushed to it from the
/// clone `seed`, and a second clone `main` made after that.
pub struct Origin {
    /// Keeps the folders alive.
    pub dir: tempfile::TempDir,
    pub origin: PathBuf,
    pub seed: PathBuf,
    pub main: PathBuf,
    /// The sha of `First`.
    pub first: String,
}

pub fn origin() -> Origin {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let origin = root.join("origin.git");
    let seed = root.join("seed");
    let main = root.join("main");
    git(
        &root,
        &["init", "-q", "--bare", "-b", "trunk", "origin.git"],
    );
    git(&root, &["clone", "-q", "origin.git", "seed"]);
    git(&seed, &["commit", "-q", "--allow-empty", "-m", "First"]);
    git(&seed, &["push", "-q", "origin", "trunk"]);
    git(&root, &["clone", "-q", "origin.git", "main"]);
    let first = git(&main, &["rev-parse", "HEAD"]);
    Origin {
        dir,
        origin,
        seed,
        main,
        first,
    }
}
