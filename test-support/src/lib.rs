//! What the crates' tests share: `git` run away from the developer's own configuration, the
//! temporary repositories the tests start from, and [`LauncherGuard`], which stops the
//! `plotkeeper-run` launchers a test leaves running, and [`fake`], a fake program for tests that need
//! one.

pub mod fake;
pub use fake::{Behaviour, Call, calls, fake};

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
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

/// A repository with one empty commit on `main` in a new folder `name` under `base`, and its real path as `git` spells it: for a
/// test that needs the repository under a folder of its own choosing.
pub fn repo_in(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["commit", "-q", "--allow-empty", "-m", "Start"]);
    dir.canonicalize().unwrap()
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

/// Stops, when dropped, every `plotkeeper-run` launcher a test started under a runs folder, so a test that ends with a
/// run still going leaves no launcher polling for its limit. A panicking test drops it while unwinding.
///
/// A launcher is stopped only when `ps` still gives its pid the start time its `launch.json` recorded (the identity
/// check plotkeeper's Cancel makes); any other pid, and a run folder without a readable `launch.json`, is left alone.
/// Hold it in a variable that is named, never `_` or `..`, which drop it at once. Declare it after every `Runner` over
/// the folder, and anything that holds one, so it drops once no runner is left to start a queued run as it stops a
/// running one; and before the folder it watches, so it drops while the folder is still there.
pub struct LauncherGuard {
    runs: PathBuf,
}

impl LauncherGuard {
    pub fn new(runs: impl Into<PathBuf>) -> Self {
        Self { runs: runs.into() }
    }
}

impl Drop for LauncherGuard {
    fn drop(&mut self) {
        let Ok(entries) = fs::read_dir(&self.runs) else {
            return;
        };
        let launchers: Vec<i32> = entries
            .flatten()
            .filter_map(|run| confirmed_launcher(&run.path().join("launch.json")))
            .collect();
        // SIGTERM first: the launcher then kills its child's process group, which is not its own, and ends.
        for &pid in &launchers {
            // SAFETY: signals the process group a confirmed launcher leads.
            unsafe { libc::killpg(pid, libc::SIGTERM) };
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while launchers.iter().any(|&pid| alive(pid)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        for &pid in launchers.iter().filter(|&&pid| alive(pid)) {
            // SAFETY: as above; the launcher did not end within the grace.
            unsafe { libc::killpg(pid, libc::SIGKILL) };
        }
    }
}

fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only checks.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// The pid `launch` names, when a process with that pid started at the time `launch` records.
fn confirmed_launcher(launch: &Path) -> Option<i32> {
    let launch: serde_json::Value = serde_json::from_str(&fs::read_to_string(launch).ok()?).ok()?;
    let pid = i32::try_from(launch["pid"].as_u64()?)
        .ok()
        .filter(|&p| p > 1)?;
    (launch["start"].as_str()? == start_of(pid)?).then_some(pid)
}

/// When process `pid` started, as `ps` gives it in the C locale; none when no such process runs.
fn start_of(pid: i32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    let start = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!start.is_empty()).then_some(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    #[test]
    fn a_pid_whose_start_time_differs_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut sleeper = Command::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap();
        let run = dir.path().join("a-run");
        fs::create_dir(&run).unwrap();
        fs::write(
            run.join("launch.json"),
            format!(
                r#"{{"pid":{},"start":"Thu Jan  1 00:00:00 1970","at_ms":0}}"#,
                sleeper.id()
            ),
        )
        .unwrap();
        // A run folder with no launch.json beside it, and one with an unreadable one.
        fs::create_dir(dir.path().join("no-launch")).unwrap();
        fs::create_dir(dir.path().join("torn")).unwrap();
        fs::write(dir.path().join("torn/launch.json"), "{").unwrap();
        drop(LauncherGuard::new(dir.path()));
        let still_running = sleeper.try_wait().unwrap().is_none();
        sleeper.kill().unwrap();
        sleeper.wait().unwrap();
        assert!(
            still_running,
            "the guard signalled a pid it could not confirm"
        );
    }

    #[test]
    fn a_launcher_that_still_matches_is_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let mut sleeper = Command::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = sleeper.id();
        let start = start_of(i32::try_from(pid).unwrap()).unwrap();
        let run = dir.path().join("a-run");
        fs::create_dir(&run).unwrap();
        fs::write(
            run.join("launch.json"),
            format!(r#"{{"pid":{pid},"start":"{start}","at_ms":0}}"#),
        )
        .unwrap();
        // The test reaps the sleeper, as init reaps a real launcher, so the guard sees it gone at once.
        let guard = std::thread::spawn({
            let runs = dir.path().to_path_buf();
            move || drop(LauncherGuard::new(runs))
        });
        assert!(
            sleeper.wait().unwrap().code().is_none(),
            "the launcher was not stopped by a signal"
        );
        guard.join().unwrap();
    }
}
