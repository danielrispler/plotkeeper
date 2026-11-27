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
use studio_test_support::LauncherGuard;

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
    _guard: LauncherGuard,
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
    // The person's own git identity, which a draft PR's commit carries.
    git(&project, &["config", "user.name", "Daniel"]);
    git(&project, &["config", "user.email", "daniel@example.com"]);
    git(&project, &["config", "maintenance.auto", "false"]);
    fs::create_dir_all(root.join("answers")).unwrap();
    stub_gh(root);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_plotkeeper-run"), root.join("launcher"))
        .unwrap();
    let (runner, drafts, changes) = open(root);
    Bench {
        _guard: LauncherGuard::new(root.join("runs")),
        dir,
        runner,
        drafts,
        changes,
    }
}

/// The recorded `gh`; no test reaches GitHub. It logs each call's arguments to `gh.log`. `auth status` fails while `gh.logged-out` is
/// there. `pr create` keeps its folder, the branch there and the body it was given, and answers with PR `gh.next`'s address (7 when
/// none). `pr view` prints `gh.view`, or fails while `gh.view-fails` is there.
fn stub_gh(root: &Path) {
    let gh = root.join("gh");
    fs::write(
        &gh,
        format!(
            "#!/bin/sh\necho \"$@\" >> '{0}/gh.log'\ncase \"$1 $2\" in\n\
             'auth status') [ ! -e '{0}/gh.logged-out' ] || {{ echo 'You are not logged into any GitHub hosts.' >&2; exit 1; }} ;;\n\
             'pr create') pwd -P > '{0}/gh.cwd'; git branch --show-current > '{0}/gh.branch'; cat > '{0}/gh.body'\n\
               echo \"https://github.com/someone/app/pull/$(cat '{0}/gh.next' 2>/dev/null || echo 7)\" ;;\n\
             'pr view') [ ! -e '{0}/gh.view-fails' ] || {{ echo 'HTTP 502' >&2; exit 1; }}; cat '{0}/gh.view' ;;\n\
             esac\n",
            root.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
}

/// The runner and Map drafts over `root`, as a start of plotkeeper makes them: the runs there and the Map drafts picked up. The Map
/// drafts are kept where the Map view reads the map waiting on a draft PR, the test's home's data folder.
fn open(root: &Path) -> (Arc<Runner>, Arc<MapDrafts>, mpsc::Receiver<String>) {
    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    let main = studio_workbench::main_checkout(&root.join("project")).unwrap();
    let store = studio_server::map::pending_file(&main).unwrap();
    let runner = Runner::new(Config {
        data: data.clone(),
        runs: root.join("runs"),
        settings: data.join("settings.json"),
        // The real launcher, through a link a test removes to keep a run from starting.
        launcher: root.join("launcher"),
        harness: Arc::new(Stub {
            root: root.to_path_buf(),
        }),
    });
    let drafts = MapDrafts::new(
        DraftConfig {
            store: store.parent().unwrap().to_path_buf(),
            gh: Some(root.join("gh")),
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
        self.pending().with_file_name(format!("{name}.json"))
    }

    /// Where the map waiting on a draft PR is kept, as the Map view reads it.
    fn pending(&self) -> PathBuf {
        studio_server::map::pending_file(&self.main()).unwrap()
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
    // The first writer answers no JSON; the second group's reviewer cannot start, its launcher gone by then.
    fs::write(
        bench.root().join("answers/writer-a.json"),
        result(&"I could not tell.".into()),
    )
    .unwrap();
    bench.answer("writer-b", &wrote(&["b", "c"]));
    bench.hold("writer-b");
    bench.start();
    bench.until(|view| view.groups[0].state == GroupState::WriterFailed);
    let launcher = bench.root().join("launcher");
    fs::remove_file(&launcher).unwrap();
    bench.release("writer-b");
    let view = bench.until(|view| view.groups[1].state == GroupState::ReviewerFailed);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_plotkeeper-run"), &launcher).unwrap();
    assert_eq!(
        view.groups[0].why.as_deref(),
        Some("The writer's answer held no JSON")
    );
    let why = view.groups[1].why.clone().unwrap_or_default();
    assert!(
        why.starts_with("The reviewer failed: the launcher") && why.contains("did not start"),
        "{why}"
    );
    // The rest stand: the second group's text shows.
    assert_eq!(purpose_of(&view, "b"), "b does its job.");
    assert!(!view.ready);

    // Retry runs the failed step of that group alone, as a new run. A purpose answered twice has its one verdict.
    bench.answer("writer-a", &wrote(&["a"]));
    bench.answer(
        "reviewer-a",
        &judged(&[("a", "supported"), ("a", "supported")]),
    );
    let retried = bench.drafts.retry(&bench.project(), 0).unwrap();
    assert_eq!(retried.groups[0].why, None);
    bench.until(|view| view.groups[0].state == GroupState::Done);
    // A reviewer that answers one purpose twice and the other never leaves that one without a verdict.
    bench.answer(
        "reviewer-b",
        &judged(&[("b", "supported"), ("b", "supported")]),
    );
    bench.drafts.retry(&bench.project(), 1).unwrap();
    let view = bench.until(|view| view.groups[1].state == GroupState::ReviewerFailed);
    assert_eq!(
        view.groups[1].why.as_deref(),
        Some("The review gave no verdict for 1 of 2 purposes")
    );
    assert!(!view.ready);
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
        [2, 1, 1, 3]
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

    // A Discard whose cancel of the first writer is refused (its process is not the one plotkeeper started) keeps the draft,
    // and the group whose reviewer it did stop is failed, retryable, never left being checked.
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(bench.file()).unwrap()).unwrap();
    let writer = written["groups"][0]["run"].as_str().unwrap().to_string();
    let launch = bench.root().join("runs").join(&writer).join("launch.json");
    let started = fs::read(&launch).unwrap();
    let mut other: serde_json::Value = serde_json::from_slice(&started).unwrap();
    other["start"] = "another process".into();
    fs::write(&launch, other.to_string()).unwrap();
    let refused = bench.drafts.discard(&bench.project());
    assert!(
        refused
            .unwrap_err()
            .contains("not the one plotkeeper started")
    );
    let view = bench.until(|view| view.groups[1].state == GroupState::ReviewerFailed);
    assert_eq!(
        view.groups[1].why.as_deref(),
        Some("The reviewer was cancelled: retry it")
    );
    assert_eq!(view.groups[0].state, GroupState::Writing);
    assert_eq!(state(&later), Some(RunState::Queued));
    fs::write(&launch, started).unwrap();

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
        _guard,
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
        _guard,
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

/// What `git args` prints in `dir`, trimmed.
fn out(dir: &Path, args: &[&str]) -> String {
    let done = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        done.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&done.stderr)
    );
    String::from_utf8_lossy(&done.stdout).trim().to_string()
}

/// Every file of `dir` but git's own, with its bytes.
fn tree(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut left = vec![dir.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in fs::read_dir(&at).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                left.push(path);
            } else {
                found.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    found.sort();
    found
}

impl Bench {
    /// Gives the repository an `origin`, a bare repository beside it, holding its `main` as the default branch.
    fn with_origin(&self) -> PathBuf {
        let origin = self.root().join("origin.git");
        git(
            self.root(),
            &["init", "-q", "--bare", "-b", "main", "origin.git"],
        );
        let project = self.root().join("project");
        git(
            &project,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        git(&project, &["push", "-q", "origin", "main"]);
        git(&project, &["remote", "set-head", "origin", "main"]);
        origin
    }

    fn read_map(&self) -> studio_server::map::MapState {
        studio_server::map::read_map(Path::new(&self.main()))
    }

    /// The pending file as JSON.
    fn pending_json(&self) -> serde_json::Value {
        serde_json::from_slice(&fs::read(self.pending()).unwrap()).unwrap()
    }

    /// What `gh` was asked, a line a call.
    fn gh_asked(&self) -> Vec<String> {
        fs::read_to_string(self.root().join("gh.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    /// The branches `origin` holds under `first-map/`.
    fn pushed(&self, origin: &Path) -> Vec<String> {
        out(
            origin,
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                "refs/heads/first-map/",
            ],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// The map waiting on PR `number`, as #757's reader expects the file: its number, branch and map, `closed` when it closed.
    fn seed_pending(&self, number: u64, closed: bool, map: &str) {
        let mut file = serde_json::json!({ "number": number, "branch": "first-map/1", "map": map });
        if closed {
            file["closed"] = true.into();
        }
        fs::create_dir_all(self.pending().parent().unwrap()).unwrap();
        fs::write(self.pending(), file.to_string()).unwrap();
    }
}

#[test]
fn open_a_draft_pr_pushes_the_map_from_a_temporary_worktree_writes_nothing_to_the_checkout_and_keeps_it_pending()
 {
    let bench = bench();
    let origin = bench.with_origin();
    bench.ready();
    let main = PathBuf::from(bench.main());
    let files = tree(&main);
    let head = out(&main, &["rev-parse", "HEAD"]);

    let opened = bench.drafts.open_pr(&bench.project()).unwrap();
    assert_eq!(
        (opened.number, opened.url.as_str()),
        (7, "https://github.com/someone/app/pull/7")
    );

    // One branch on origin, one commit on main holding the map alone, by the person's own git identity.
    let pushed = bench.pushed(&origin);
    let [branch] = pushed.as_slice() else {
        panic!("origin holds {pushed:?}");
    };
    assert_eq!(
        out(&origin, &["rev-parse", &format!("{branch}^")]),
        out(&origin, &["rev-parse", "main"])
    );
    assert_eq!(
        out(&origin, &["diff", "--name-only", "main", branch]),
        "system-map.json"
    );
    assert_eq!(
        out(&origin, &["log", "-1", "--format=%an <%ae>", branch]),
        "Daniel <daniel@example.com>"
    );
    let map_text = out(&origin, &["show", &format!("{branch}:system-map.json")]);
    let map: serde_json::Value = serde_json::from_str(&map_text).unwrap();
    assert_eq!(map["parts"].as_array().map(Vec::len), Some(3));
    assert_eq!(
        map["links"],
        serde_json::json!([{ "id": "a-b", "from": "a", "to": "b", "kind": "import" }])
    );

    // A draft PR, made from the temporary worktree on that branch.
    let created = bench
        .gh_asked()
        .into_iter()
        .find(|line| line.starts_with("pr create"))
        .unwrap();
    assert!(created.starts_with("pr create --draft"), "{created}");
    assert!(created.contains(&format!("--head {branch}")), "{created}");
    assert!(created.contains("--base main"), "{created}");
    assert_eq!(
        fs::read_to_string(bench.root().join("gh.branch"))
            .unwrap()
            .trim(),
        branch
    );
    let body = fs::read_to_string(bench.root().join("gh.body")).unwrap();
    assert!(body.contains("system-map.json"), "{body}");
    assert!(!body.contains('@'), "a mention in the body: {body}");

    // Nothing is written to the checkout, its branch is as it was, and the temporary worktree and branch are gone.
    let made = PathBuf::from(
        fs::read_to_string(bench.root().join("gh.cwd"))
            .unwrap()
            .trim(),
    );
    assert!(!made.exists(), "the temporary worktree is removed");
    assert_eq!(tree(&main), files);
    assert_eq!(out(&main, &["status", "--porcelain"]), "");
    assert_eq!(out(&main, &["rev-parse", "HEAD"]), head);
    assert_eq!(out(&main, &["branch", "--show-current"]), "main");
    assert_eq!(out(&main, &["branch", "--list", "first-map/*"]), "");
    assert_eq!(
        out(&main, &["worktree", "list", "--porcelain"])
            .matches("worktree ")
            .count(),
        1
    );

    // The accepted map is kept with the PR's number and branch, and the Map view shows it waiting on the PR; the Map draft is gone.
    let pending = bench.pending_json();
    assert_eq!(pending["number"], 7);
    assert_eq!(pending["branch"], branch.as_str());
    assert_eq!(pending.get("closed"), None);
    assert_eq!(
        pending["map"].as_str().map(str::trim_end),
        Some(map_text.as_str())
    );
    let state = bench.read_map();
    assert_eq!(state.from, Some(studio_server::map::ReadFrom::PendingPr));
    assert_eq!(state.status, studio_server::map::Status::Loaded);
    assert_eq!(
        state.pending.as_ref().map(|pr| (pr.number, pr.closed)),
        Some((7, false))
    );
    assert_eq!(bench.view(), None);
    assert!(!bench.file().exists());
}

#[test]
fn open_a_draft_pr_is_muted_with_why_when_gh_is_missing_or_not_logged_in() {
    /// A case: its name, what is done to `gh`, and a word of why the button is muted, or none.
    type Case = (&'static str, fn(&Bench), Option<&'static str>);
    let cases: [Case; 3] = [
        ("logged in", |_| {}, None),
        (
            "not logged in",
            |bench| fs::write(bench.root().join("gh.logged-out"), "").unwrap(),
            Some("gh auth login"),
        ),
        (
            "missing",
            |bench| fs::remove_file(bench.root().join("gh")).unwrap(),
            Some("not found"),
        ),
    ];
    for (name, setup, muted) in cases {
        let bench = bench();
        let origin = bench.with_origin();
        setup(&bench);
        let view = bench.ready();
        match muted {
            None => assert_eq!(view.pr_muted, None, "{name}"),
            Some(word) => {
                let why = view.pr_muted.clone().unwrap_or_default();
                assert!(why.contains(word), "{name}: {why:?}");
                let refused = bench.drafts.open_pr(&bench.project()).unwrap_err();
                assert_eq!(refused, why, "{name}");
                assert_eq!(bench.pushed(&origin), Vec::<String>::new(), "{name}");
                assert!(bench.view().is_some(), "{name}: the draft went");
            }
        }
    }
}

const PENDING_MAP: &str =
    r#"{"version":2,"parts":[],"actors":[],"links":[],"ignored":[],"laws":[]}"#;

#[test]
fn the_pending_pr_is_followed_merged_deletes_its_file_closed_keeps_it_and_anything_else_changes_nothing()
 {
    /// A case: its name, what `gh pr view` prints (none: it fails), whether the file stays, and whether it then says closed.
    type Case = (&'static str, Option<&'static str>, bool, bool);
    let cases: [Case; 6] = [
        (
            "merged",
            Some(r#"{"state":"MERGED","mergedAt":"2026-11-27T10:00:00Z"}"#),
            false,
            false,
        ),
        (
            "closed",
            Some(r#"{"state":"CLOSED","mergedAt":null}"#),
            true,
            true,
        ),
        (
            "open",
            Some(r#"{"state":"OPEN","mergedAt":null}"#),
            true,
            false,
        ),
        ("a gh error", None, true, false),
        (
            "an unreadable answer",
            Some("<html>rate limited</html>"),
            true,
            false,
        ),
        (
            "a state it does not know",
            Some(r#"{"state":"LOCKED"}"#),
            true,
            false,
        ),
    ];
    for (name, view, kept, closed) in cases {
        let bench = bench();
        bench.seed_pending(12, false, PENDING_MAP);
        match view {
            Some(text) => fs::write(bench.root().join("gh.view"), text).unwrap(),
            None => fs::write(bench.root().join("gh.view-fails"), "").unwrap(),
        }
        bench.drafts.follow(&bench.project());
        assert!(
            bench
                .gh_asked()
                .contains(&"pr view 12 --json state,mergedAt".to_string()),
            "{name}: {:?}",
            bench.gh_asked()
        );
        assert_eq!(bench.pending().exists(), kept, "{name}");
        let state = bench.read_map();
        let shown = state.pending.as_ref().map(|pr| (pr.number, pr.closed));
        assert_eq!(shown, kept.then_some((12, closed)), "{name}");
        if kept {
            assert_eq!(bench.pending_json()["map"], PENDING_MAP, "{name}");
        }
    }
    // With no map waiting, nothing is asked.
    let bench = bench();
    bench.drafts.follow(&bench.project());
    assert_eq!(bench.gh_asked(), Vec::<String>::new());
}

/// A bench whose accepted map's draft PR, #7, closed unmerged, with its `origin`.
fn closed_bench() -> (Bench, PathBuf) {
    let bench = bench();
    let origin = bench.with_origin();
    bench.ready();
    bench.drafts.open_pr(&bench.project()).unwrap();
    fs::write(
        bench.root().join("gh.view"),
        r#"{"state":"CLOSED","mergedAt":null}"#,
    )
    .unwrap();
    bench.drafts.follow(&bench.project());
    assert_eq!(
        bench.read_map().pending.map(|pr| (pr.number, pr.closed)),
        Some((7, true))
    );
    (bench, origin)
}

#[test]
fn from_a_closed_pr_write_open_a_draft_pr_and_discard_each_work() {
    use studio_server::map::{ReadFrom, Status};
    // Write puts the accepted map into the checkout, uncommitted, and the copy goes.
    let (bench, _) = closed_bench();
    let map = bench.pending_json()["map"].as_str().unwrap().to_string();
    let written = bench.drafts.pending_write(&bench.project(), None).unwrap();
    let path = bench.map_file().display().to_string();
    assert_eq!(written, Accepted::Written { path: path.clone() });
    assert_eq!(fs::read_to_string(&path).unwrap(), map);
    assert!(!bench.pending().exists());
    assert_eq!(bench.read_map().from, Some(ReadFrom::Checkout));
    assert_eq!(
        out(Path::new(&bench.main()), &["status", "--porcelain"]),
        "?? system-map.json"
    );

    // Open a draft PR opens a new one on a new branch, and the map waits on it; the checkout stays as it was.
    let (bench, origin) = closed_bench();
    let main = PathBuf::from(bench.main());
    let files = tree(&main);
    fs::write(bench.root().join("gh.next"), "8").unwrap();
    let opened = bench.drafts.pending_pr(&bench.project()).unwrap();
    assert_eq!(opened.number, 8);
    assert_eq!(bench.pushed(&origin).len(), 2);
    let pending = bench.pending_json();
    assert_eq!(
        (pending["number"].as_u64(), pending.get("closed")),
        (Some(8), None)
    );
    assert_eq!(pending["map"].as_str(), Some(map.as_str()));
    assert_eq!(
        bench.read_map().pending.map(|pr| (pr.number, pr.closed)),
        Some((8, false))
    );
    assert_eq!(tree(&main), files);

    // Discard deletes plotkeeper's copy, and the repository has no map again.
    let (bench, _) = closed_bench();
    bench.drafts.pending_discard(&bench.project()).unwrap();
    assert!(!bench.pending().exists());
    assert_eq!(bench.read_map().status, Status::None);
}

#[test]
fn a_map_whose_pr_is_still_open_is_not_written_reopened_or_discarded() {
    let bench = bench();
    bench.seed_pending(12, false, PENDING_MAP);
    let before = fs::read(bench.pending()).unwrap();
    let refusals = [
        bench.drafts.pending_write(&bench.project(), None).map(drop),
        bench.drafts.pending_pr(&bench.project()).map(drop),
        bench.drafts.pending_discard(&bench.project()),
    ];
    for refused in refusals {
        let why = refused.unwrap_err();
        assert!(why.contains("PR #12 is still open"), "{why}");
    }
    assert_eq!(fs::read(bench.pending()).unwrap(), before);
    assert!(!bench.map_file().exists());
}
