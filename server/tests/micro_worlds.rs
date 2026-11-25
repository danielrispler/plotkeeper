//! Micro-worlds (spec danielrispler/plotkeeper#378, ticket #399) with the runner's real launcher, a fake harness that writes the
//! page a run would, and the fake dsh-visual (`fake-dsh-visual/`) as the host check's scripts. No test runs `claude` or Chrome.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use studio_server::micro_worlds::{CSP, Explored, MicroWorlds, Phase, Subject, Topic, flow_topic};
use studio_server::runner::{Config, Harness, HarnessCommand, Plan, Runner};

/// The fake harness: `sh -c <script>` in the run's working folder, with the run's prompt in `PROMPT`.
struct Writes(String);

impl Harness for Writes {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        Ok(HarnessCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), self.0.clone()],
            env: [("PROMPT".to_string(), plan.prompt.to_string())].into(),
            ..HarnessCommand::default()
        })
    }
}

/// A script that writes `page` as the run's template.
fn writes(page: &str) -> String {
    let quoted = page.replace('\'', r"'\''");
    format!("printf '%s' '{quoted}' > .micro-world/template.html")
}

const GOOD: &str = "<!doctype html><title>World</title><p>the turn's journey</p>";
const BAD: &str = "<!doctype html><p>VERIFY-FAIL</p>";

const MAP: &str = r#"{
  "version": 1,
  "parts": [
    { "id": "core", "name": "Core", "caption": "Turns events into state", "description": "Plain logic.", "tier": "core", "paths": ["core/"] },
    { "id": "server", "name": "Server", "caption": "Serves the page", "description": "The shell.", "tier": "core", "paths": ["server/"] },
    { "id": "ui", "name": "Page", "caption": "Draws the map", "description": "The page.", "tier": "interface", "paths": ["ui/"] }
  ],
  "actors": [{ "id": "daniel", "name": "Daniel" }],
  "links": [
    { "id": "core-server", "from": "core", "to": "server" },
    { "id": "server-ui", "from": "server", "to": "ui" },
    { "id": "daniel-ui", "from": "daniel", "to": "ui" }
  ],
  "laws": [{ "id": "one-writer", "on": "core", "backing": "trust", "text": "Only core writes the state." }],
  "flows": [
    { "id": "turn", "name": "A turn", "hops": [
      { "link": "core-server", "text": "Core sees the turn end." },
      { "link": "server-ui", "text": "The server tells the page." }
    ] },
    { "id": "look", "name": "A look", "hops": [{ "link": "daniel-ui", "text": "Daniel opens the page." }] }
  ]
}"#;

struct Bench {
    dir: tempfile::TempDir,
    runner: Arc<Runner>,
    worlds: Arc<MicroWorlds>,
    changes: mpsc::Receiver<String>,
    repo: PathBuf,
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Commits `file` with `text` in the bench's repository: the new commit.
fn commit(repo: &Path, file: &str, text: &str) -> String {
    let path = repo.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-qm", file]);
    git(repo, &["rev-parse", "HEAD"])
}

/// The fake dsh-visual `scripts/` folder, with its playwright-core where Node finds it.
fn fake_scripts(dir: &Path) -> PathBuf {
    let fake = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-dsh-visual");
    let scripts = dir.join("dsh-visual/scripts");
    let modules = scripts.join("node_modules/playwright-core");
    fs::create_dir_all(&modules).unwrap();
    for name in ["build.py", "verify.mjs"] {
        fs::copy(fake.join(name), scripts.join(name)).unwrap();
    }
    for name in ["index.js", "index.mjs", "package.json"] {
        fs::copy(fake.join("playwright-core").join(name), modules.join(name)).unwrap();
    }
    scripts
}

/// A bench whose harness runs `script`, with `microWorlds.dshVisual` set to the fake when `dsh` holds.
fn bench(script: &str, dsh: bool) -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    fs::create_dir_all(&data).unwrap();
    let scripts = fake_scripts(dir.path());
    let settings = match dsh {
        true => serde_json::json!({ "microWorlds": { "dshVisual": scripts } }),
        false => serde_json::json!({}),
    };
    fs::write(data.join("settings.json"), settings.to_string()).unwrap();
    let repo = dir.path().join("repo");
    fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    commit(&repo, "system-map.json", MAP);
    commit(&repo, "core/lib.rs", "fn turn() {}\n");
    let runner = Runner::new(Config {
        data: data.clone(),
        runs: dir.path().join("runs"),
        settings: data.join("settings.json"),
        launcher: env!("CARGO_BIN_EXE_plotkeeper-run").into(),
        harness: Arc::new(Writes(script.into())),
    });
    let worlds = MicroWorlds::new(
        data.join("micro-worlds"),
        data.join("settings.json"),
        &runner,
    );
    let (tell, changes) = mpsc::channel();
    worlds.on_change(move |project| drop(tell.send(project.to_string())));
    let told = Arc::downgrade(&worlds);
    runner.on_change(move |project| {
        if let Some(worlds) = told.upgrade() {
            worlds.runs_changed(project);
        }
    });
    runner.pick_up();
    Bench {
        dir,
        runner,
        worlds,
        changes,
        repo,
    }
}

impl Bench {
    fn project(&self) -> String {
        self.repo.to_string_lossy().into_owned()
    }

    fn head(&self) -> String {
        git(&self.repo, &["rev-parse", "HEAD"])
    }

    fn turn(&self) -> Topic {
        flow_topic(MAP, "turn").unwrap()
    }

    fn explore(&self) -> Explored {
        self.worlds
            .explore(&self.project(), self.turn(), &self.head())
    }

    /// Waits, on the micro-worlds' own change notices, until the world of `key` is no longer building.
    fn settled(&self, key: &str) -> studio_server::micro_worlds::View {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let state = self.worlds.state(&self.project(), Some(&self.head()));
            if let Some(view) = state.worlds.get(key)
                && view.phase != Phase::Building
            {
                return view.clone();
            }
            let left = deadline.saturating_duration_since(Instant::now());
            self.changes
                .recv_timeout(left)
                .expect("the micro-world never settled");
        }
    }
}

#[test]
fn a_flows_topic_holds_only_map_text_its_parts_and_their_paths() {
    let topic = flow_topic(MAP, "turn").unwrap();
    assert_eq!(topic.key, "flow:turn");
    assert_eq!(topic.name, "A turn");
    // The parts the hops run through, in order; an actor is no part.
    assert_eq!(topic.parts, ["core", "server", "ui"]);
    assert_eq!(topic.paths, ["core/", "server/", "ui/"]);
    for words in [
        "Core sees the turn end.",
        "The server tells the page.",
        "Only core writes the state.",
        "Turns events into state",
    ] {
        assert!(topic.text.contains(words), "{words} in {}", topic.text);
    }
    assert!(flow_topic(MAP, "nowhere").is_none());
    assert_eq!(Subject::Flow { id: "turn".into() }.key(), "flow:turn");
}

#[test]
fn a_flows_first_hop_run_backwards_is_told_and_ordered_the_way_it_runs() {
    let map = MAP
        .replace(
            r#"{ "id": "core-server", "from": "core", "to": "server" }"#,
            r#"{ "id": "server-core", "from": "server", "to": "core" }"#,
        )
        .replace(r#"{ "link": "core-server""#, r#"{ "link": "server-core""#);
    let topic = flow_topic(&map, "turn").unwrap();
    assert_eq!(topic.parts, ["core", "server", "ui"]);
    assert!(
        topic
            .text
            .contains("1. Core sees the turn end. (Core to Server)\n"),
        "{}",
        topic.text
    );
}

#[test]
fn a_topic_quotes_each_parts_purpose_and_held_items_on_either_version() {
    let topic = flow_topic(MAP, "turn").unwrap();
    assert!(
        topic
            .text
            .contains("(core/): Turns events into state. Plain logic.\n"),
        "{}",
        topic.text
    );
    let v2 = MAP
        .replace(r#""version": 1"#, r#""version": 2"#)
        .replace(r#""description": "The shell.""#, r#""purpose": "The shell.", "holds": [{ "name": "Routes", "text": "One per command.", "paths": ["server/api.rs"] }]"#)
        .replace(r#""description""#, r#""holds": [], "purpose""#);
    let topic = flow_topic(&v2, "turn").unwrap();
    assert!(
        topic.text.contains("(server/): Serves the page. The shell.\n  - Routes: One per command. (key files: server/api.rs)\n"),
        "{}",
        topic.text
    );
}

#[test]
fn a_page_that_passes_the_host_checks_is_stored_and_served_with_the_csp() {
    let bench = bench(&writes(GOOD), true);
    let explored = bench.explore();
    assert!(explored.started, "{explored:?}");
    let view = bench.settled("flow:turn");
    assert_eq!(view.phase, Phase::Ready, "{view:?}");
    assert!(view.page && !view.stale);
    let page = bench.worlds.page(&bench.project(), "flow:turn").unwrap();
    // The policy is the page's first element, before anything the run wrote.
    let csp = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#);
    assert!(page.starts_with(&format!("<!doctype html>{csp}")), "{page}");
    assert!(page.contains("the turn's journey"));
    // The run read the project's files at the commit: they were in its working folder.
    let runs = bench.dir.path().join("runs");
    let work = fs::read_dir(&runs)
        .unwrap()
        .flatten()
        .find(|e| e.path().is_dir())
        .unwrap()
        .path()
        .join("work");
    assert!(work.join("core/lib.rs").exists() && work.join("system-map.json").exists());
}

#[test]
fn the_api_serves_only_a_stored_checked_page() {
    let bench = bench(&writes(BAD), true);
    assert!(
        bench.worlds.page(&bench.project(), "flow:turn").is_err(),
        "nothing built yet"
    );
    bench.explore();
    assert_eq!(bench.settled("flow:turn").phase, Phase::Failed);
    assert!(
        bench.worlds.page(&bench.project(), "flow:turn").is_err(),
        "a page that failed its checks is never served"
    );
}

#[test]
fn a_failing_page_is_rewritten_once_with_what_failed() {
    // The first run writes a failing page; the rewrite, which finds it and is told what failed, writes a good one.
    let script = format!(
        "if [ -e .micro-world/template.html ] && printf '%s' \"$PROMPT\" | grep -q 'no horizontal overflow'; then {}; else {}; fi",
        writes(GOOD),
        writes(BAD)
    );
    let bench = bench(&script, true);
    bench.explore();
    let view = bench.settled("flow:turn");
    assert_eq!(view.phase, Phase::Ready, "{view:?}");
    assert_eq!(
        bench.runner.runs(&bench.project()).runs.len(),
        2,
        "one run and one rewrite"
    );
}

#[test]
fn a_page_that_fails_twice_falls_back_to_the_prompt_to_copy() {
    let bench = bench(&writes(BAD), true);
    bench.explore();
    let view = bench.settled("flow:turn");
    assert_eq!(view.phase, Phase::Failed);
    assert!(!view.page);
    assert!(
        view.line
            .as_deref()
            .unwrap_or("")
            .contains("no horizontal overflow"),
        "{view:?}"
    );
    let prompt = view.prompt.expect("a failed world offers its prompt");
    assert!(
        prompt.contains("A turn") && prompt.contains("dsh-visual"),
        "{prompt}"
    );
    assert_eq!(
        bench.runner.runs(&bench.project()).runs.len(),
        2,
        "rewritten once, never more"
    );
}

#[test]
fn without_dsh_visual_explore_offers_only_the_prompt_to_copy() {
    let bench = bench(&writes(GOOD), false);
    let explored = bench.explore();
    assert!(!explored.started);
    let prompt = explored.prompt.expect("the prompt to copy");
    assert!(
        prompt.contains("A turn") && prompt.contains("Core sees the turn end."),
        "{prompt}"
    );
    assert!(
        bench.runner.runs(&bench.project()).runs.is_empty(),
        "no run starts"
    );
    let state = bench.worlds.state(&bench.project(), Some(&bench.head()));
    assert!(!state.on && state.worlds.is_empty());
}

#[test]
fn a_change_to_a_part_on_the_flow_marks_the_world_out_of_date_and_rebuild_starts_a_run() {
    let bench = bench(&writes(GOOD), true);
    bench.explore();
    assert_eq!(bench.settled("flow:turn").phase, Phase::Ready);
    // A file of no part on the flow changes nothing.
    commit(&bench.repo, "docs/readme.md", "words\n");
    assert!(!bench.settled("flow:turn").stale);
    commit(&bench.repo, "server/main.rs", "fn main() {}\n");
    let view = bench.settled("flow:turn");
    assert!(view.stale && view.page, "{view:?}");
    // Rebuild is Explore again: a run starts at the new commit, and the page it checks replaces the old one.
    assert!(bench.explore().started);
    assert_eq!(bench.runner.runs(&bench.project()).runs.len(), 2);
    let view = bench.settled("flow:turn");
    assert_eq!(view.phase, Phase::Ready);
    assert!(!view.stale);
}

#[test]
fn dismiss_waits_until_the_host_check_is_done_with_the_runs_folder() {
    let bench = bench(&writes("<!doctype html><p>VERIFY-SLOW</p>"), true);
    bench.explore();
    // The run ends, and its page is being checked on the host: the check's log is in the run's folder.
    let deadline = Instant::now() + Duration::from_secs(60);
    let id = loop {
        let runs = bench.runner.runs(&bench.project()).runs;
        let checking = runs.iter().find(|run| {
            run.outcome.is_some()
                && bench
                    .dir
                    .path()
                    .join("runs")
                    .join(&run.id)
                    .join("build.log")
                    .exists()
        });
        if let Some(run) = checking {
            break run.id.clone();
        }
        assert!(
            Instant::now() < deadline,
            "the run's page was never checked"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        bench.runner.cancel(&id).is_err(),
        "Dismiss is refused while the page is checked"
    );
    assert_eq!(bench.settled("flow:turn").phase, Phase::Ready);
    bench.runner.cancel(&id).unwrap();
}

#[test]
fn a_host_check_plotkeeper_stopped_in_fails_with_why_at_the_next_start() {
    let bench = bench(&writes("<!doctype html><p>VERIFY-SLOW</p>"), true);
    bench.explore();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !fs::read_dir(bench.dir.path().join("runs"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|run| run.path().join("build.log").exists())
    {
        assert!(
            Instant::now() < deadline,
            "the run's page was never checked"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // plotkeeper stops mid-check and starts again.
    let data = bench.dir.path().join("data");
    let runner = Runner::new(Config {
        data: data.clone(),
        runs: bench.dir.path().join("runs"),
        settings: data.join("settings.json"),
        launcher: env!("CARGO_BIN_EXE_plotkeeper-run").into(),
        harness: Arc::new(Writes(String::new())),
    });
    let worlds = MicroWorlds::new(
        data.join("micro-worlds"),
        data.join("settings.json"),
        &runner,
    );
    runner.pick_up();
    worlds.pick_up();
    let state = worlds.state(&bench.project(), Some(&bench.head()));
    let view = &state.worlds["flow:turn"];
    assert_eq!(view.phase, Phase::Failed, "{view:?}");
    assert!(
        view.line
            .as_deref()
            .unwrap_or("")
            .contains("plotkeeper stopped"),
        "{view:?}"
    );
}

#[test]
fn a_project_whose_path_differs_only_in_punctuation_has_worlds_of_its_own() {
    let bench = bench(&writes(GOOD), true);
    bench.explore();
    assert_eq!(bench.settled("flow:turn").phase, Phase::Ready);
    // Another project whose path differs from this one's only where a character is not a letter or a digit.
    let project = bench.project();
    let at = project.rfind('/').unwrap();
    let other = format!("{}_{}", &project[..at], &project[at + 1..]);
    assert!(
        bench.worlds.state(&other, None).worlds.is_empty(),
        "{other} shows none of {project}'s worlds"
    );
    assert!(bench.worlds.page(&other, "flow:turn").is_err());
}
