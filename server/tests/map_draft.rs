//! First map's drafting run (spec danielrispler/plotkeeper#753, ticket #762) through the runner's real launcher, with a stub
//! `claude` that answers canned JSON: each writer and reviewer waits while its `hold-<key>` file is there, then prints
//! `answers/<key>.json` as Claude Code's result, and exits 1 when `fail-<key>` is there. A key is the run's role and its
//! first part (`writer-a`, `reviewer-b`, `redraft-b`). No test runs `claude`.

mod isolated_home;
use std::io::Write as _;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use studio_server::map_draft::{
    Accepted, Config as DraftConfig, Edit, GroupState, MapDraftView, MapDrafts,
};
use studio_server::runner::{
    Config, Harness, HarnessCommand, Kind, Limits, Plan, Request, RunState, Runner,
};

/// The skeleton the page gives: part `a` in zone `x`, `b` and `c` in zone `y`, so two writer groups, `[a]` and `[b, c]`;
/// `vendor/` is a reference copy, ignored. `a` imports `b`, an edge the scan's links leave out, as Accept computes them again.
const SKELETON: &str = r#"{"version":2,"extractors":["typescript-imports"],"parts":[
  {"id":"a","name":"A","caption":"Not yet described","purpose":"Not yet described.","holds":[],"tier":"interface","paths":["a/"],"zone":"x"},
  {"id":"b","name":"B","caption":"Not yet described","purpose":"Not yet described.","holds":[],"tier":"interface","paths":["b/"],"zone":"y"},
  {"id":"c","name":"C","caption":"Not yet described","purpose":"Not yet described.","holds":[],"tier":"interface","paths":["c/"],"zone":"y"}],
  "actors":[],"links":[],"ignored":["vendor/"],"laws":[]}"#;

/// The stub harness: the run's key from its prompt, and a script that answers for that key.
struct Stub {
    root: PathBuf,
}

/// The key of a run from its prompt: `writer-<first part>`, `redraft-<part>` or `reviewer-<first part>`.
fn key_of(prompt: &str) -> String {
    let first = |after: &str, end: char| {
        let from = prompt.find(after).map(|at| at + after.len());
        from.and_then(|from| prompt[from..].split(end).next())
            .unwrap_or_default()
            .to_string()
    };
    if prompt.contains("You review a project's system map") {
        format!("reviewer-{}", first("\n- part:", '/'))
    } else if prompt.contains("This is a redraft") {
        format!("redraft-{}", first("Your parts: `", '`'))
    } else {
        format!("writer-{}", first("Your parts: `", '`'))
    }
}

impl Harness for Stub {
    fn command(&self, plan: &Plan) -> Result<HarnessCommand, String> {
        let key = key_of(plan.prompt);
        // What each run was asked and the map its folder holds, one line each.
        let line = serde_json::json!({
            "key": key,
            "prompt": plan.prompt,
            "map": fs::read_to_string(plan.folder.join("system-map.json")).unwrap_or_default(),
            "has_code": plan.folder.join("a/x.ts").exists(),
        });
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.root.join("asked.jsonl"))
            .unwrap();
        writeln!(log, "{line}").unwrap();
        let root = self.root.display();
        Ok(HarnessCommand {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                format!(
                    "while [ -e '{root}/hold-{key}' ]; do sleep 0.05; done\ncat '{root}/answers/{key}.json' 2>/dev/null\n[ ! -e '{root}/fail-{key}' ]"
                ),
            ],
            ..HarnessCommand::default()
        })
    }
}

struct Bench {
    dir: tempfile::TempDir,
    runner: Arc<Runner>,
    drafts: Arc<MapDrafts>,
    changes: mpsc::Receiver<String>,
}

fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A repository with code in `a/`, `b/` and `c/` and no map, and the runner and Map drafts over it.
fn bench() -> Bench {
    isolated_home::set();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    git(&project, &["init", "-q", "-b", "main"]);
    for file in ["b/y.ts", "c/z.ts", "vendor/v.ts"] {
        write(&project, file, "export const x = 1;\n");
    }
    write(&project, "a/x.ts", "import { x } from \"../b/y\";\n");
    git(&project, &["add", "-A"]);
    git(&project, &["commit", "-qm", "code"]);
    fs::create_dir_all(root.join("answers")).unwrap();
    let (runner, drafts, changes) = open(root);
    Bench {
        dir,
        runner,
        drafts,
        changes,
    }
}

/// The runner and Map drafts over `root`, as a start of plotkeeper makes them: the runs there and the Map drafts picked up.
fn open(root: &Path) -> (Arc<Runner>, Arc<MapDrafts>, mpsc::Receiver<String>) {
    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    let runner = Runner::new(Config {
        data: data.clone(),
        runs: root.join("runs"),
        settings: data.join("settings.json"),
        launcher: env!("CARGO_BIN_EXE_plotkeeper-run").into(),
        harness: Arc::new(Stub {
            root: root.to_path_buf(),
        }),
    });
    let drafts = MapDrafts::new(
        DraftConfig {
            store: data.join("first-map"),
        },
        &runner,
    );
    let (tell, changes) = mpsc::channel();
    let runs = tell.clone();
    drafts.on_change(move |project| drop(tell.send(project.to_string())));
    runner.on_change(move |project| drop(runs.send(project.to_string())));
    runner.pick_up();
    drafts.pick_up();
    (runner, drafts, changes)
}

/// Claude Code's result printing `answer`.
fn result(answer: &serde_json::Value) -> String {
    serde_json::json!({
        "type": "result", "subtype": "success", "is_error": false, "result": answer.to_string(), "total_cost_usd": 0.1,
    })
    .to_string()
}

/// A writer's answer naming each of `parts` `<Id> part`, with the purpose `<id> does its job.`.
fn wrote(parts: &[&str]) -> serde_json::Value {
    let parts: serde_json::Map<String, serde_json::Value> = parts
        .iter()
        .map(|id| {
            let entry = serde_json::json!({
                "name": format!("{} part", id.to_uppercase()), "caption": format!("Does {id}"),
                "purpose": format!("{id} does its job."), "purpose_cites": [format!("{id}/x.ts:1-1")],
            });
            ((*id).to_string(), entry)
        })
        .collect();
    serde_json::json!({ "parts": parts, "not_written": [], "guesses": [], "files_opened": 1 })
}

/// A reviewer's answer: each of `verdicts`, a part and its verdict.
fn judged(verdicts: &[(&str, &str)]) -> serde_json::Value {
    let items: Vec<serde_json::Value> = verdicts
        .iter()
        .map(|(part, verdict)| {
            serde_json::json!({
                "id": format!("part:{part}/purpose"), "verdict": verdict, "why": format!("{part} is {verdict}."),
                "cites": [format!("{part}/x.ts:1")],
            })
        })
        .collect();
    serde_json::json!({ "items": items })
}

impl Bench {
    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn project(&self) -> String {
        self.root().join("project").display().to_string()
    }

    /// The main checkout as git names it: the runs' project.
    fn main(&self) -> String {
        studio_workbench::main_checkout(&self.root().join("project")).unwrap()
    }

    fn answer(&self, key: &str, answer: &serde_json::Value) {
        fs::write(
            self.root().join("answers").join(format!("{key}.json")),
            result(answer),
        )
        .unwrap();
    }

    fn hold(&self, key: &str) {
        fs::write(self.root().join(format!("hold-{key}")), "").unwrap();
    }

    fn release(&self, key: &str) {
        fs::remove_file(self.root().join(format!("hold-{key}"))).unwrap();
    }

    fn start(&self) -> MapDraftView {
        self.drafts
            .start(&self.project(), Some(SKELETON.into()))
            .unwrap()
    }

    fn view(&self) -> Option<MapDraftView> {
        self.drafts.view(&self.project())
    }

    /// Waits, on the change notices, until the Map draft's view meets `what`.
    fn until(&self, what: impl Fn(&MapDraftView) -> bool) -> MapDraftView {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(view) = self.view()
                && what(&view)
            {
                return view;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            self.changes
                .recv_timeout(left)
                .expect("the Map draft never reached the state the test waits for");
        }
    }

    /// What each run was asked, in order: its key, prompt and folder's map.
    fn asked(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(self.root().join("asked.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    fn asked_of(&self, key: &str) -> Vec<serde_json::Value> {
        self.asked()
            .into_iter()
            .filter(|asked| asked["key"] == key)
            .collect()
    }

    /// `system-map.json` in the main checkout, where Accept writes.
    fn map_file(&self) -> PathBuf {
        Path::new(&self.main()).join("system-map.json")
    }

    /// A Map draft every reviewer answered on: each part written and its purpose supported.
    fn ready(&self) -> MapDraftView {
        self.answer("writer-a", &wrote(&["a"]));
        self.answer("writer-b", &wrote(&["b", "c"]));
        self.answer("reviewer-a", &judged(&[("a", "supported")]));
        self.answer(
            "reviewer-b",
            &judged(&[("b", "supported"), ("c", "supported")]),
        );
        self.start();
        self.until(|view| view.ready)
    }

    fn file(&self) -> PathBuf {
        let name: String = self
            .main()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        self.root()
            .join("data/first-map")
            .join(format!("{name}.json"))
    }
}

fn states(view: &MapDraftView) -> Vec<GroupState> {
    view.groups.iter().map(|group| group.state).collect()
}

/// The parts of `view`'s map, as `(id, name, purpose, tier)`.
fn parts(view: &MapDraftView) -> Vec<(String, String, String, String)> {
    let map: serde_json::Value = serde_json::from_str(&view.map).unwrap();
    map["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| {
            let field = |key: &str| part[key].as_str().unwrap().to_string();
            (field("id"), field("name"), field("purpose"), field("tier"))
        })
        .collect()
}

fn purpose_of(view: &MapDraftView, id: &str) -> String {
    parts(view).into_iter().find(|part| part.0 == id).unwrap().2
}

#[test]
fn the_groups_land_in_order_and_each_reviewer_checks_its_groups_purposes_alone() {
    let bench = bench();
    for key in ["writer-a", "writer-b", "reviewer-b"] {
        bench.hold(key);
    }
    bench.answer("writer-a", &wrote(&["a"]));
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.answer("reviewer-a", &judged(&[("a", "supported")]));
    bench.answer(
        "reviewer-b",
        &judged(&[("b", "supported"), ("c", "contradicted")]),
    );
    let view = bench.start();
    let groups: Vec<Vec<String>> = view.groups.iter().map(|g| g.parts.clone()).collect();
    assert_eq!(groups, [vec!["a"], vec!["b", "c"]]);
    assert_eq!(states(&view), [GroupState::Writing, GroupState::Writing]);
    assert!(!view.ready);
    // Each writer reads the repository with the skeleton, its parts named and the reference copy left out.
    let writer = &bench.asked_of("writer-b")[0];
    let prompt = writer["prompt"].as_str().unwrap();
    assert!(prompt.contains("Your parts: `b`, `c`"), "{prompt}");
    assert!(prompt.contains("don't read `vendor/`"), "{prompt}");
    assert_eq!(writer["has_code"], true);
    assert_eq!(writer["map"], SKELETON);
    assert!(bench.file().exists());

    // The second group lands first: its text shows, being checked, and its reviewer reads that group's text alone.
    bench.release("writer-b");
    let view = bench.until(|view| view.groups[1].state == GroupState::Checking);
    assert_eq!(states(&view), [GroupState::Writing, GroupState::Checking]);
    assert_eq!(purpose_of(&view, "b"), "b does its job.");
    assert_eq!(purpose_of(&view, "a"), "Not yet described.");
    assert!(view.verdicts.is_empty());
    let reviewer = &bench.asked_of("reviewer-b")[0];
    let prompt = reviewer["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("the repository, with a draft map"),
        "{prompt}"
    );
    assert!(
        prompt.contains("- part:b/purpose\n- part:c/purpose\n"),
        "{prompt}"
    );
    assert!(!prompt.contains("part:b/name"), "{prompt}");
    let map: serde_json::Value = serde_json::from_str(reviewer["map"].as_str().unwrap()).unwrap();
    let purposes: Vec<&str> = map["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["purpose"].as_str().unwrap())
        .collect();
    assert_eq!(
        purposes,
        ["Not yet described.", "b does its job.", "c does its job."]
    );

    // The first group lands and is checked; Accept unlocks only once the last reviewer answers.
    bench.release("writer-a");
    let view = bench.until(|view| view.groups[0].state == GroupState::Done);
    assert_eq!(view.groups[1].state, GroupState::Checking);
    assert!(!view.ready);
    bench.release("reviewer-b");
    let view = bench.until(|view| view.ready);
    assert_eq!(states(&view), [GroupState::Done, GroupState::Done]);
    let flagged: Vec<(&str, &str, bool)> = view
        .verdicts
        .iter()
        .map(|(id, v)| (id.as_str(), v.verdict.as_str(), v.flagged))
        .collect();
    assert_eq!(
        flagged,
        [
            ("a", "supported", false),
            ("b", "supported", false),
            ("c", "contradicted", true)
        ]
    );
    assert_eq!(view.verdicts["c"].why, "c is contradicted.");
    // The ended runs are dismissed: their folders held only a copy of the repository.
    assert!(bench.runner.runs(&bench.main()).runs.is_empty());

    // An edit shows over the writers' text, and is kept.
    let edit = Edit {
        name: Some("Alpha".into()),
        tier: Some("core".into()),
        ..Edit::default()
    };
    let view = bench.drafts.edit(&bench.project(), "a", edit).unwrap();
    assert_eq!(
        parts(&view)[0],
        (
            "a".into(),
            "Alpha".into(),
            "a does its job.".into(),
            "core".into()
        )
    );
    let refused = bench.drafts.edit(
        &bench.project(),
        "a",
        Edit {
            tier: Some("gold".into()),
            ..Edit::default()
        },
    );
    assert_eq!(refused.unwrap_err(), "gold is not a tier");
    // A second draft is refused while this one is there.
    let again = bench.drafts.start(&bench.project(), None);
    assert!(again.unwrap_err().contains("already"));
}

#[test]
fn a_failed_group_is_kept_apart_and_retried_alone() {
    let bench = bench();
    // The first writer answers no JSON; the second group's reviewer leaves a purpose without a verdict.
    fs::write(
        bench.root().join("answers/writer-a.json"),
        result(&"I could not tell.".into()),
    )
    .unwrap();
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.answer("reviewer-b", &judged(&[("b", "supported")]));
    bench.start();
    let view = bench.until(|view| {
        view.groups[0].state == GroupState::WriterFailed
            && view.groups[1].state == GroupState::ReviewerFailed
    });
    assert_eq!(
        view.groups[0].why.as_deref(),
        Some("The writer's answer held no JSON")
    );
    assert_eq!(
        view.groups[1].why.as_deref(),
        Some("The review gave no verdict for 1 of 2 purposes")
    );
    // The rest stand: the second group's text shows.
    assert_eq!(purpose_of(&view, "b"), "b does its job.");
    assert!(!view.ready);

    // Retry runs the failed step of that group alone, as a new run.
    bench.answer("writer-a", &wrote(&["a"]));
    bench.answer("reviewer-a", &judged(&[("a", "supported")]));
    let retried = bench.drafts.retry(&bench.project(), 0).unwrap();
    assert_eq!(retried.groups[0].why, None);
    bench.until(|view| view.groups[0].state == GroupState::Done);
    bench.answer(
        "reviewer-b",
        &judged(&[("b", "supported"), ("c", "supported")]),
    );
    bench.drafts.retry(&bench.project(), 1).unwrap();
    let view = bench.until(|view| view.ready);
    assert_eq!(purpose_of(&view, "a"), "a does its job.");
    let asked: Vec<String> = bench
        .asked()
        .iter()
        .map(|asked| asked["key"].as_str().unwrap().to_string())
        .collect();
    let count = |key: &str| asked.iter().filter(|each| *each == key).count();
    assert_eq!(
        [
            count("writer-a"),
            count("reviewer-a"),
            count("writer-b"),
            count("reviewer-b")
        ],
        [2, 1, 1, 2]
    );
    let refused = bench.drafts.retry(&bench.project(), 0);
    assert_eq!(refused.unwrap_err(), "That group has not failed");
}

#[test]
fn redraft_this_part_rewrites_one_part_with_its_flag_then_checks_it_again() {
    let bench = bench();
    bench.answer("writer-a", &wrote(&["a"]));
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.answer("reviewer-a", &judged(&[("a", "supported")]));
    bench.answer(
        "reviewer-b",
        &judged(&[("b", "supported"), ("c", "contradicted")]),
    );
    bench.start();
    bench.until(|view| view.ready);
    bench
        .drafts
        .edit(
            &bench.project(),
            "c",
            Edit {
                purpose: Some("Mine.".into()),
                tier: Some("core".into()),
                ..Edit::default()
            },
        )
        .unwrap();

    bench.hold("redraft-c");
    let mut redrafted = wrote(&["c"]);
    redrafted["parts"]["c"]["purpose"] = "c does its job, again.".into();
    bench.answer("redraft-c", &redrafted);
    bench.answer("reviewer-c", &judged(&[("c", "supported")]));
    let view = bench.drafts.redraft(&bench.project(), "c").unwrap();
    // Accept is locked again until the redraft's reviewer answers.
    assert!(!view.ready);
    let early = bench.drafts.accept(&bench.project(), None);
    assert_eq!(
        early.unwrap_err(),
        "Accept unlocks once a reviewer has checked every part"
    );
    assert!(!bench.map_file().exists());
    assert_eq!(
        view.groups
            .last()
            .map(|group| (&group.parts, group.state, group.redraft)),
        Some((&vec!["c".to_string()], GroupState::Writing, true))
    );
    // One writer over the one part, with the flag's verdict and reason.
    let asked = &bench.asked_of("redraft-c")[0];
    let prompt = asked["prompt"].as_str().unwrap();
    assert!(prompt.contains("Your parts: `c`\n"), "{prompt}");
    assert!(
        prompt.contains("judged this part's last purpose `contradicted`: c is contradicted. It cited `c/x.ts:1`."),
        "{prompt}"
    );
    let refused = bench.drafts.redraft(&bench.project(), "c");
    assert_eq!(
        refused.unwrap_err(),
        "This part is being written or checked"
    );

    bench.release("redraft-c");
    let view = bench.until(|view| view.ready);
    // The redraft's text replaces the person's edit of it; their tier stays.
    let c = parts(&view).into_iter().find(|part| part.0 == "c").unwrap();
    assert_eq!(
        (c.2.as_str(), c.3.as_str()),
        ("c does its job, again.", "core")
    );
    assert_eq!(view.verdicts["c"].verdict, "supported");
    assert_eq!(bench.asked_of("reviewer-c").len(), 1);
    let prompt = bench.asked_of("reviewer-c")[0]["prompt"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(prompt.contains("- part:c/purpose\n"), "{prompt}");
    assert!(!prompt.contains("part:b/purpose"), "{prompt}");
}

#[test]
fn discard_mid_batch_stops_every_run_frees_the_slot_and_deletes_the_draft() {
    let bench = bench();
    bench.hold("writer-a");
    bench.hold("reviewer-b");
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.start();
    bench.until(|view| view.groups[1].state == GroupState::Checking);
    let main = bench.main();
    // A request behind the batch waits for its slot.
    let later = bench
        .runner
        .submit(Request {
            kind: Kind::FirstMap,
            project: main.clone(),
            folder: bench.runner.new_folder().unwrap(),
            prompt: "later".into(),
            limits: Limits::of(Kind::FirstMap),
        })
        .unwrap();
    let state = |id: &str| {
        bench
            .runner
            .runs(&main)
            .runs
            .into_iter()
            .find(|run| run.id == id)
            .map(|run| run.state)
    };
    assert_eq!(state(&later), Some(RunState::Queued));
    let running = bench.runner.runs(&main).runs.len();
    assert_eq!(running, 3, "{:?}", bench.runner.runs(&main));

    bench.drafts.discard(&bench.project()).unwrap();
    assert_eq!(bench.view(), None);
    assert!(!bench.file().exists());
    let left: Vec<String> = bench
        .runner
        .runs(&main)
        .runs
        .into_iter()
        .map(|run| run.id)
        .collect();
    assert_eq!(left, std::slice::from_ref(&later));
    let deadline = Instant::now() + Duration::from_secs(30);
    while state(&later) != Some(RunState::Ended) {
        assert!(
            Instant::now() < deadline,
            "the request behind the batch never ran"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // A new draft can start once the old one is gone.
    bench.release("writer-a");
    assert!(
        bench
            .drafts
            .start(&bench.project(), Some(SKELETON.into()))
            .is_ok()
    );
}

/// Claude Code's result on a usage limit, which the runner reads as a rate limit.
fn limited() -> String {
    serde_json::json!({
        "type": "result", "subtype": "success", "is_error": true,
        "result": "Claude usage limit reached. Your limit will reset at 5pm.", "total_cost_usd": 0,
    })
    .to_string()
}

#[test]
fn a_limit_mid_batch_pauses_the_queues_keeps_the_landed_groups_and_retries_after_resume() {
    let bench = bench();
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.answer(
        "reviewer-b",
        &judged(&[("b", "supported"), ("c", "supported")]),
    );
    fs::write(bench.root().join("answers/writer-a.json"), limited()).unwrap();
    fs::write(bench.root().join("fail-writer-a"), "").unwrap();
    bench.hold("writer-a");
    bench.start();
    bench.until(|view| view.groups[1].state == GroupState::Done);
    bench.release("writer-a");
    let view = bench.until(|view| view.groups[0].state == GroupState::WriterFailed);
    assert!(
        view.groups[0]
            .why
            .as_deref()
            .unwrap()
            .contains("usage or rate limit"),
        "{:?}",
        view.groups[0].why
    );
    assert!(bench.runner.runs(&bench.main()).paused.is_some());
    assert_eq!(purpose_of(&view, "b"), "b does its job.");

    // A retry while paused waits for Resume.
    fs::remove_file(bench.root().join("fail-writer-a")).unwrap();
    bench.answer("writer-a", &wrote(&["a"]));
    bench.answer("reviewer-a", &judged(&[("a", "supported")]));
    let view = bench.drafts.retry(&bench.project(), 0).unwrap();
    assert_eq!(view.groups[0].state, GroupState::Writing);
    let runs = bench.runner.runs(&bench.main()).runs;
    assert_eq!(
        runs.iter().map(|run| run.state).collect::<Vec<_>>(),
        [RunState::Queued]
    );
    bench.runner.resume();
    let view = bench.until(|view| view.ready);
    assert_eq!(purpose_of(&view, "a"), "a does its job.");
}

/// Kills the process group `pid` leads.
fn kill_group(pid: u64) {
    let pid = i32::try_from(pid).unwrap();
    // SAFETY: a signal to the test's own launcher's process group.
    unsafe { libc::killpg(pid, libc::SIGKILL) };
}

#[test]
fn a_restart_mid_batch_reads_the_draft_back_and_a_dead_runs_group_is_retryable() {
    let bench = bench();
    bench.hold("writer-a");
    bench.hold("reviewer-b");
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.answer(
        "reviewer-b",
        &judged(&[("b", "supported"), ("c", "overreach")]),
    );
    bench.start();
    bench.until(|view| view.groups[1].state == GroupState::Checking);
    let (main, root) = (bench.main(), bench.root().to_path_buf());
    // The draft written mid-batch: the second group landed, the first still writing.
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(bench.file()).unwrap()).unwrap();
    assert_eq!(
        written["groups"][1]["text"]["b"]["purpose"],
        "b does its job."
    );
    assert_eq!(written["groups"][0]["state"], "writing");
    let writer = written["groups"][0]["run"].as_str().unwrap().to_string();
    let reviewer = written["groups"][1]["run"].as_str().unwrap().to_string();
    let pid = |id: &str| {
        let launch = fs::read_to_string(root.join("runs").join(id).join("launch.json")).unwrap();
        serde_json::from_str::<serde_json::Value>(&launch).unwrap()["pid"]
            .as_u64()
            .unwrap()
    };
    let pids = [pid(&writer), pid(&reviewer)];

    // plotkeeper stops; meanwhile the first writer dies and the second group's reviewer answers.
    let Bench {
        dir,
        runner,
        drafts,
        changes,
    } = bench;
    drop((runner, drafts, changes));
    kill_group(pids[0]);
    fs::remove_file(root.join("hold-reviewer-b")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while pids.iter().any(|pid| alive(*pid)) {
        assert!(Instant::now() < deadline, "the runs never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));

    let (runner, drafts, changes) = open(&root);
    let bench = Bench {
        dir,
        runner,
        drafts,
        changes,
    };
    // The draft reads back with its landed group, the reviewer's verdicts read at the pick-up, and the dead run's group failed.
    let view = bench.view().unwrap();
    assert_eq!(purpose_of(&view, "b"), "b does its job.");
    assert_eq!(
        states(&view),
        [GroupState::WriterFailed, GroupState::Done],
        "{view:?}"
    );
    assert_eq!(view.verdicts["c"].verdict, "overreach");
    assert_eq!(
        view.groups[0].why.as_deref(),
        Some("The writer's process went away before it finished")
    );
    // No run is left holding the slot.
    assert!(bench.runner.runs(&main).runs.is_empty());
    bench.answer("writer-a", &wrote(&["a"]));
    bench.answer("reviewer-a", &judged(&[("a", "supported")]));
    bench.release("writer-a");
    bench.drafts.retry(&bench.project(), 0).unwrap();
    let view = bench.until(|view| view.ready);
    assert_eq!(purpose_of(&view, "a"), "a does its job.");
}

fn alive(pid: u64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .unwrap()
        .status
        .success()
}

#[test]
fn accept_writes_the_map_with_its_import_links_into_the_checkout_uncommitted_and_deletes_the_draft()
{
    let bench = bench();
    let view = bench.ready();
    let path = bench.map_file().display().to_string();
    assert_eq!(view.path, path);
    let edit = Edit {
        name: Some("Alpha".into()),
        tier: Some("core".into()),
        ..Edit::default()
    };
    bench.drafts.edit(&bench.project(), "a", edit).unwrap();

    let accepted = bench.drafts.accept(&bench.project(), None).unwrap();
    assert_eq!(accepted, Accepted::Written { path: path.clone() });
    // The skeleton with the written text and the edits, the computed edge as an `import` link with no reason, no laws and no flows.
    let map: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    let view = MapDraftView {
        map: map.to_string(),
        ..view
    };
    assert_eq!(
        parts(&view),
        [
            (
                "a".into(),
                "Alpha".into(),
                "a does its job.".into(),
                "core".into()
            ),
            (
                "b".into(),
                "B part".into(),
                "b does its job.".into(),
                "interface".into()
            ),
            (
                "c".into(),
                "C part".into(),
                "c does its job.".into(),
                "interface".into()
            ),
        ]
    );
    assert_eq!(
        map["links"],
        serde_json::json!([{ "id": "a-b", "from": "a", "to": "b", "kind": "import" }])
    );
    assert_eq!(map["laws"], serde_json::json!([]));
    assert_eq!(map["flows"], serde_json::json!([]));
    assert_eq!(map["ignored"], serde_json::json!(["vendor/"]));
    let report = system_map::check(Path::new(&bench.main()));
    assert_eq!(report.problems, Vec::<String>::new());
    // It is left uncommitted, for the person to commit.
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(bench.main())
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&status.stdout),
        "?? system-map.json\n"
    );
    // The Map draft is deleted.
    assert_eq!(bench.view(), None);
    assert!(!bench.file().exists());
    let again = bench.drafts.accept(&bench.project(), None);
    assert_eq!(again.unwrap_err(), "There is no Map draft to accept");
}

#[test]
fn accept_is_refused_with_why_writing_nothing_and_keeping_the_draft() {
    /// A case: its name, what is done to the checkout once the draft is ready, the file it leaves to stay as it is, and the refusal.
    type Case = (&'static str, fn(&Bench) -> PathBuf, &'static str);
    let cases: [Case; 5] = [
        (
            "the map fails its check",
            |bench| {
                let edit = Edit {
                    purpose: Some("A".repeat(300)),
                    ..Edit::default()
                };
                bench.drafts.edit(&bench.project(), "b", edit).unwrap();
                bench.map_file()
            },
            "The map does not pass its check: part \"b\": purpose is 300 characters, the cap is 240",
        ),
        (
            "the checkout's system-map.json is a symbolic link",
            |bench| {
                let outside = bench.root().join("outside.json");
                fs::write(&outside, "kept").unwrap();
                std::os::unix::fs::symlink(&outside, bench.map_file()).unwrap();
                outside
            },
            "system-map.json in the checkout is a symbolic link",
        ),
        (
            "a symbolic link to nothing",
            |bench| {
                let nowhere = bench.root().join("nowhere.json");
                std::os::unix::fs::symlink(&nowhere, bench.map_file()).unwrap();
                nowhere
            },
            "system-map.json in the checkout is a symbolic link",
        ),
        (
            "the checkout has a map that reads",
            |bench| {
                fs::write(bench.map_file(), SKELETON).unwrap();
                bench.map_file()
            },
            "This repository has a map already",
        ),
        (
            "the default branch has a map",
            |bench| {
                fs::write(bench.map_file(), SKELETON).unwrap();
                git(Path::new(&bench.main()), &["add", "system-map.json"]);
                git(Path::new(&bench.main()), &["commit", "-qm", "map"]);
                bench.map_file()
            },
            "This repository has a map already",
        ),
    ];
    for (name, setup, refusal) in cases {
        let bench = bench();
        bench.ready();
        let kept = setup(&bench);
        let before = fs::read(&kept).ok();
        let refused = bench.drafts.accept(&bench.project(), None);
        let why = refused.expect_err(name);
        assert!(why.starts_with(refusal), "{name}: {why}");
        assert_eq!(fs::read(&kept).ok(), before, "{name}");
        assert!(bench.view().is_some(), "{name}: the draft went");
    }
    // A link is left a link, never written through.
    let bench = bench();
    bench.ready();
    std::os::unix::fs::symlink(bench.root().join("nowhere.json"), bench.map_file()).unwrap();
    bench
        .drafts
        .accept(&bench.project(), Some("0"))
        .unwrap_err();
    assert!(
        fs::symlink_metadata(bench.map_file())
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn an_unreadable_system_map_json_is_replaced_only_on_a_yes_to_the_file_as_it_was_asked_about() {
    let bench = bench();
    bench.ready();
    let path = bench.map_file().display().to_string();
    fs::write(bench.map_file(), "{ \"parts\": 1").unwrap();
    // Write asks first, naming the file's first problem as the Map view says it.
    let asked = bench.drafts.accept(&bench.project(), None).unwrap();
    let Accepted::Replace {
        path: asked_path,
        problem,
        seen,
    } = asked
    else {
        panic!("Write did not ask: {asked:?}");
    };
    assert_eq!(asked_path, path);
    let shown = studio_server::map::read_map(Path::new(&bench.main())).note;
    assert_eq!(
        Some(format!("system-map.json does not read: {problem}")),
        shown
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "{ \"parts\": 1");
    // A yes to another file than the one there asks again.
    let stale = bench.drafts.accept(&bench.project(), Some("0")).unwrap();
    assert!(
        matches!(&stale, Accepted::Replace { seen: again, .. } if *again == seen),
        "{stale:?}"
    );
    // The file changed between the question and the yes: it is asked about again.
    fs::write(bench.map_file(), "not a map").unwrap();
    let changed = bench.drafts.accept(&bench.project(), Some(&seen)).unwrap();
    let Accepted::Replace { seen: now, .. } = changed else {
        panic!("a changed file was replaced: {changed:?}");
    };
    assert_ne!(now, seen);
    assert_eq!(fs::read_to_string(&path).unwrap(), "not a map");
    assert!(bench.view().is_some());
    // A yes to the file as it is replaces it.
    let written = bench.drafts.accept(&bench.project(), Some(&now)).unwrap();
    assert_eq!(written, Accepted::Written { path: path.clone() });
    let map: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(map["parts"].as_array().map(Vec::len), Some(3));
    assert_eq!(bench.view(), None);
}
