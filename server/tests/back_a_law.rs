//! Back a law (spec danielrispler/plotkeeper#378, ticket #398) end to end, against a temporary repository with a bare
//! `origin`: the clone, the fake agent's test and plant patches, the scope rule, plotkeeper's own proof and Discard. The
//! agent is a shell script the fake harness runs in the clone, and `srt` is a fake that runs its command as it is, so no
//! test starts `claude` or the real sandbox; the proof's commands are the project's checks, here Node's test runner.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use studio_server::{
    back_a_law::{self, LawRuns, Opened, PatchState, Started},
    ratify,
    runner::{self, Harness, HarnessCommand, Plan, Runner},
};
use studio_test_support::git;

/// The fake agent: `sh -c <script>` in the run's working folder, whatever the prompt.
struct Agent(String);

impl Harness for Agent {
    fn command(&self, _: &Plan) -> Result<HarnessCommand, String> {
        Ok(HarnessCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), self.0.clone()],
            ..HarnessCommand::default()
        })
    }
}

const LAW: &str = "adds";
const MAP: &str = r#"{
  "version": 1,
  "parts": [
    { "id": "adder", "name": "Adder", "caption": "Adds numbers", "description": "The sums.", "tier": "core", "paths": ["src/"] }
  ],
  "actors": [],
  "links": [],
  "ignored": ["tests/", "system-map.json"],
  "laws": [
    { "id": "adds", "on": "adder", "backing": "trust", "text": "add sums its two numbers." }
  ]
}
"#;

/// The well-formed agent: a test tagged for the law, the law flipped to check, both staged; then the plant, unstaged.
/// The tag line is its own piece, so the map check does not read it as a tag of this file.
const TEST: &str = concat!(
    r#"cat > tests/add.test.mjs <<'EOF'
import { test } from "node:test";
import assert from "node:assert/strict";
import { add } from "../src/add.mjs";

"#,
    "// @law: adds\n",
    r#"test("add sums its two numbers", () => assert.equal(add(2, 3), 5));
EOF
sed -i '' 's/"backing": "trust"/"backing": "check"/' system-map.json
git add tests/add.test.mjs system-map.json
"#
);
const PLANT: &str = "echo 'export const add = (a, b) => a - b;' > src/add.mjs\n";
const DONE: &str = r#"echo '{"type":"result","subtype":"success","is_error":false,"result":"Backed.","permission_denials":[]}'"#;

struct Bench {
    dir: tempfile::TempDir,
    /// Daniel's repository, a clone of `origin` on a branch of his own.
    repo: PathBuf,
    project: String,
    laws: Arc<LawRuns>,
    runner: Arc<Runner>,
    changes: mpsc::Receiver<()>,
}

/// A repository whose `origin`'s default branch `main` holds the map, `src/` and an untagged test, and whose checkout is
/// on Daniel's own branch `mine`; a runner and back a law over it, whose agent runs `agent`.
fn bench(agent: &str) -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    git(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(&root, &["clone", "-q", "origin.git", "seed"]);
    let seed = root.join("seed");
    for (file, text) in [
        ("system-map.json", MAP),
        ("src/add.mjs", "export const add = (a, b) => a + b;\n"),
        ("src/twice.mjs", "export const twice = (a) => a * 2;\n"),
        (
            "tests/twice.test.mjs",
            "import { test } from \"node:test\";\nimport assert from \"node:assert/strict\";\nimport { twice } from \"../src/twice.mjs\";\n\ntest(\"twice doubles\", () => assert.equal(twice(2), 4));\n",
        ),
    ] {
        fs::create_dir_all(seed.join(file).parent().unwrap()).unwrap();
        fs::write(seed.join(file), text).unwrap();
    }
    git(&seed, &["add", "-A"]);
    git(&seed, &["commit", "-q", "-m", "First"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    git(&root, &["clone", "-q", "origin.git", "repo"]);
    let repo = root.join("repo");
    git(&repo, &["switch", "-q", "-c", "mine"]);
    git(&repo, &["config", "user.name", "Daniel"]);
    git(&repo, &["config", "user.email", "daniel@example.com"]);
    fs::write(repo.join("src/add.mjs"), "// Daniel's own work.\n").unwrap();
    git(&repo, &["commit", "-q", "-am", "Mine"]);
    let project = repo.display().to_string();

    let data = root.join("data");
    fs::create_dir_all(&data).unwrap();
    let settings = data.join("settings.json");
    let checks = serde_json::json!({ "checks": { &project: [
        "node --check src/add.mjs",
        "node --test --test-reporter=tap tests/*.test.mjs",
    ] } });
    fs::write(&settings, checks.to_string()).unwrap();
    // The fake `srt`: it insists on its settings file and runs the command as it is.
    let srt = root.join("srt");
    fs::write(
        &srt,
        "#!/bin/sh\n[ \"$1\" = -s ] && [ -f \"$2\" ] && [ \"$3\" = -c ] || { echo \"fake srt: $*\" >&2; exit 64; }\nexec /bin/sh -c \"$4\"\n",
    )
    .unwrap();
    fs::set_permissions(&srt, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    // The recorded `gh`: it keeps its arguments, its folder, the branch there and the body it was given, and answers with a
    // PR's address, as `gh pr create` does. No test reaches GitHub.
    let gh = root.join("gh");
    fs::write(
        &gh,
        format!(
            "#!/bin/sh\necho \"$@\" > '{0}/gh.args'\npwd -P > '{0}/gh.cwd'\ngit branch --show-current > '{0}/gh.branch'\ncat > '{0}/gh.body'\necho https://github.com/someone/app/pull/7\n",
            root.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();

    let runner = Runner::new(runner::Config {
        data: data.clone(),
        runs: root.join("runs"),
        settings: settings.clone(),
        launcher: env!("CARGO_BIN_EXE_plotkeeper-run").into(),
        harness: Arc::new(Agent(agent.into())),
    });
    let laws = LawRuns::new(
        runner.clone(),
        back_a_law::Config {
            data,
            settings,
            sandbox: Some(srt),
            gh: Some(gh),
        },
    );
    let (tell, changes) = mpsc::channel();
    laws.on_change(move |_| {
        let _ = tell.send(());
    });
    runner.pick_up();
    Bench {
        dir,
        repo,
        project,
        laws,
        runner,
        changes,
    }
}

impl Bench {
    fn start(&self) -> String {
        match self.laws.start(&self.project, LAW).unwrap() {
            Started::Run { id } => id,
            Started::Refused { line, .. } => panic!("refused: {line}"),
        }
    }

    /// Waits until run `id`'s patch is proven or rejected.
    fn settled(&self, id: &str) -> back_a_law::LawPatch {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let patch = self
                .laws
                .patches(&self.project)
                .into_iter()
                .find(|p| p.id == id);
            if let Some(patch) = patch.filter(|p| p.state != PatchState::Proving) {
                return patch;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(
                !left.is_zero(),
                "run {id} was never proven or rejected: {:?}",
                self.runner.runs(&self.project)
            );
            let _ = self
                .changes
                .recv_timeout(left.min(Duration::from_millis(500)));
        }
    }

    fn work(&self, id: &str) -> PathBuf {
        self.dir
            .path()
            .canonicalize()
            .unwrap()
            .join("runs")
            .join(id)
            .join("work")
    }

    /// The rejection's line of run `id`.
    fn rejected(&self, id: &str) -> String {
        let patch = self.settled(id);
        assert_eq!(patch.state, PatchState::Rejected, "{patch:?}");
        patch.line.unwrap_or_default()
    }
}

/// Every file under `dir`, with its bytes: Daniel's repository, git data and working tree both.
fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        for entry in fs::read_dir(&at).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    files
}

#[test]
fn a_well_formed_run_is_proven_and_stored_and_the_repository_is_untouched() {
    let bench = bench(&format!("set -e\n{TEST}{PLANT}{DONE}"));
    let before = snapshot(&bench.repo);
    let id = bench.start();
    let patch = bench.settled(&id);
    assert_eq!(patch.state, PatchState::Proven, "{patch:?}");
    assert_eq!(patch.law, LAW);

    // The stored test patch: the tagged test and the flipped law, never the plant.
    let diff = patch.diff.as_deref().unwrap();
    assert!(
        diff.contains("diff --git a/tests/add.test.mjs b/tests/add.test.mjs"),
        "{diff}"
    );
    assert!(diff.contains("+// @law: adds"), "{diff}");
    assert!(
        diff.contains("-    { \"id\": \"adds\", \"on\": \"adder\", \"backing\": \"trust\""),
        "{diff}"
    );
    assert!(
        diff.contains("+    { \"id\": \"adds\", \"on\": \"adder\", \"backing\": \"check\""),
        "{diff}"
    );
    assert!(!diff.contains("diff --git a/src/"), "{diff}");
    // The proof: green with the test, red with the plant on the new test itself.
    let proof = patch.proof.unwrap();
    assert_eq!(proof.test, "add sums its two numbers");
    assert_eq!(proof.file, "tests/add.test.mjs");
    assert!(
        proof.passed.contains("ok 1 - add sums its two numbers"),
        "{}",
        proof.passed
    );
    assert!(!proof.passed.contains("not ok"), "{}", proof.passed);
    assert!(
        proof.failed.contains("not ok 1 - add sums its two numbers"),
        "{}",
        proof.failed
    );

    // Kept in the data folder, and the clone is gone.
    let stored = bench
        .dir
        .path()
        .join("data/law-patches")
        .join(format!("{id}.json"));
    assert!(stored.exists());
    assert!(!bench.work(&id).exists(), "the clone is removed");
    // Daniel's repository: its branches, its git data and its files, byte for byte.
    assert_eq!(snapshot(&bench.repo), before);
    assert_eq!(git(&bench.repo, &["branch", "--show-current"]), "mine");
}

#[test]
fn the_clone_is_of_the_default_branch_with_no_remote() {
    // The agent leaves what it found in the clone where the test can read it, and does nothing.
    let bench = bench(&format!(
        "cp src/add.mjs ../seen; git remote > ../remotes; git rev-parse --abbrev-ref HEAD > ../head; {DONE}"
    ));
    let id = bench.start();
    bench.rejected(&id);
    let run = bench.work(&id).parent().unwrap().to_path_buf();
    assert_eq!(
        fs::read_to_string(run.join("seen")).unwrap(),
        "export const add = (a, b) => a + b;\n"
    );
    assert_eq!(fs::read_to_string(run.join("remotes")).unwrap(), "");
    assert_eq!(fs::read_to_string(run.join("head")).unwrap().trim(), "HEAD");
}

#[test]
fn a_run_that_leaves_no_test_is_rejected() {
    let bench = bench(&format!("{PLANT}{DONE}"));
    let id = bench.start();
    assert!(bench.rejected(&id).contains("no test patch"));
}

#[test]
fn a_test_patch_that_touches_production_code_is_rejected() {
    let bench = bench(&format!(
        "set -e\n{TEST}echo '// tidied' >> src/twice.mjs; git add src/twice.mjs\n{PLANT}{DONE}"
    ));
    let id = bench.start();
    let line = bench.rejected(&id);
    assert!(line.contains("src/twice.mjs"), "{line}");
    assert!(
        !bench.work(&id).exists(),
        "a rejected run's clone is removed too"
    );
}

#[test]
fn a_plant_that_edits_the_test_is_rejected() {
    let bench = bench(&format!(
        "set -e\n{TEST}sed -i '' 's/5)/6)/' tests/add.test.mjs\n{DONE}"
    ));
    let id = bench.start();
    let line = bench.rejected(&id);
    assert!(line.contains("tests/add.test.mjs"), "{line}");
}

#[test]
fn a_plant_that_breaks_the_build_is_rejected() {
    let bench = bench(&format!(
        "set -e\n{TEST}echo 'export const add = (a, b) => {{' > src/add.mjs\n{DONE}"
    ));
    let id = bench.start();
    let line = bench.rejected(&id);
    assert!(line.contains("did not fail"), "{line}");
}

#[test]
fn a_red_run_that_does_not_name_the_new_test_is_rejected() {
    let bench = bench(&format!(
        "set -e\n{TEST}echo 'export const twice = (a) => a * 3;' > src/twice.mjs\n{DONE}"
    ));
    let id = bench.start();
    let line = bench.rejected(&id);
    assert!(line.contains("did not fail"), "{line}");
}

#[test]
fn a_test_that_fails_without_the_plant_is_rejected() {
    let bench = bench(&format!(
        "set -e\n{}{PLANT}{DONE}",
        TEST.replace("add(2, 3), 5", "add(2, 3), 6")
    ));
    let id = bench.start();
    let line = bench.rejected(&id);
    assert!(line.contains("with the test alone"), "{line}");
}

#[test]
fn a_law_left_on_trust_fails_the_map_check_and_is_rejected() {
    let bench = bench(&format!(
        "set -e\n{}{PLANT}{DONE}",
        TEST.replace("sed -i ''", "true")
    ));
    let id = bench.start();
    let line = bench.rejected(&id);
    assert!(line.contains("says trust"), "{line}");
}

#[test]
fn discard_deletes_the_patch() {
    let bench = bench(&format!("set -e\n{TEST}{PLANT}{DONE}"));
    let id = bench.start();
    assert_eq!(bench.settled(&id).state, PatchState::Proven);
    bench.laws.discard(&bench.project, &id).unwrap();
    assert!(bench.laws.patches(&bench.project).is_empty());
    assert!(
        !bench
            .dir
            .path()
            .join("data/law-patches")
            .join(format!("{id}.json"))
            .exists()
    );
    assert!(bench.laws.discard(&bench.project, "../settings").is_err());
}

#[test]
fn a_law_not_on_the_map_starts_no_run() {
    let bench = bench(DONE);
    assert!(bench.laws.start(&bench.project, "no-such-law").is_err());
    assert!(bench.runner.runs(&bench.project).runs.is_empty());
}

#[test]
fn the_prompt_holds_the_law_and_the_parts_paths() {
    let bench = bench("sleep 60");
    let id = bench.start();
    let prompt = fs::read_to_string(bench.work(&id).parent().unwrap().join("run.json")).unwrap();
    let prompt: serde_json::Value = serde_json::from_str(&prompt).unwrap();
    let prompt = prompt["prompt"].as_str().unwrap();
    assert!(prompt.contains("@law: adds"), "{prompt}");
    assert!(prompt.contains("add sums its two numbers."), "{prompt}");
    assert!(prompt.contains("src/"), "{prompt}");
    bench.runner.cancel(&id).unwrap();
}

/// Every file of the working tree under `dir`, with its bytes, leaving out its git data.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    snapshot(dir)
        .into_iter()
        .filter(|(path, _)| !path.strip_prefix(dir).unwrap().starts_with(".git"))
        .collect()
}

#[test]
fn open_draft_pr_pushes_a_branch_from_the_default_one_and_opens_a_draft_in_a_temporary_worktree() {
    let bench = bench(&format!("set -e\n{TEST}{PLANT}{DONE}"));
    let id = bench.start();
    assert_eq!(bench.settled(&id).state, PatchState::Proven);
    let files = tree(&bench.repo);
    let head = git(&bench.repo, &["rev-parse", "HEAD"]);

    let opened = bench.laws.open_pr(&bench.project, &id).unwrap();
    assert_eq!(
        opened,
        Opened::Pr {
            number: 7,
            url: "https://github.com/someone/app/pull/7".into()
        }
    );

    // The branch is on origin: one commit on main, holding the test patch and nothing of Daniel's own branch.
    let root = bench.dir.path().canonicalize().unwrap();
    let origin = root.join("origin.git");
    let branch = format!("back-a-law/{LAW}-{id}");
    let pushed = git(&origin, &["rev-parse", &format!("refs/heads/{branch}")]);
    assert_eq!(
        git(&origin, &["rev-parse", &format!("{pushed}^")]),
        git(&origin, &["rev-parse", "main"])
    );
    assert_eq!(
        git(&origin, &["diff", "--name-only", "main", &pushed]),
        "system-map.json\ntests/add.test.mjs"
    );
    assert_eq!(
        git(&origin, &["show", &format!("{pushed}:src/add.mjs")]),
        "export const add = (a, b) => a + b;"
    );
    assert!(
        git(&origin, &["show", &format!("{pushed}:system-map.json")])
            .contains("\"backing\": \"check\"")
    );

    // The draft PR, made from the temporary worktree on that branch, with the proof in its body.
    let args = fs::read_to_string(root.join("gh.args")).unwrap();
    assert!(args.starts_with("pr create --draft"), "{args}");
    assert!(args.contains(&format!("--head {branch}")), "{args}");
    assert!(args.contains("--base main"), "{args}");
    assert_eq!(
        fs::read_to_string(root.join("gh.branch")).unwrap().trim(),
        branch
    );
    let body = fs::read_to_string(root.join("gh.body")).unwrap();
    assert!(body.contains("add sums its two numbers."), "{body}");
    assert!(body.contains("ok 1 - add sums its two numbers"), "{body}");
    assert!(
        body.contains("not ok 1 - add sums its two numbers"),
        "{body}"
    );
    assert!(
        !body.contains("@law"),
        "a tag in the body would mention a GitHub user: {body}"
    );

    // The temporary worktree is gone, and Daniel's checkout is as it was.
    let made = PathBuf::from(fs::read_to_string(root.join("gh.cwd")).unwrap().trim());
    assert_ne!(made, bench.repo);
    assert!(!made.exists(), "the temporary worktree is removed");
    assert_eq!(
        git(&bench.repo, &["worktree", "list", "--porcelain"])
            .matches("worktree ")
            .count(),
        1
    );
    assert_eq!(git(&bench.repo, &["branch", "--show-current"]), "mine");
    assert_eq!(git(&bench.repo, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&bench.repo, &["status", "--porcelain"]), "");
    assert_eq!(tree(&bench.repo), files);

    // The patch says its PR, and the law change is accepted in ratify, so it is not offered again as unratified.
    let patch = bench
        .laws
        .patches(&bench.project)
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    assert_eq!(patch.state, PatchState::Opened);
    assert_eq!(patch.pr, Some(7));
    let ratified = ratify::read(&root.join("data").join(ratify::FILE), &bench.project);
    assert_eq!(
        ratified.status(&ratify::Item::new(7, "law", LAW), None),
        ratify::Status::Accepted
    );
}

#[test]
fn a_patch_a_later_commit_made_stale_is_out_of_date() {
    let bench = bench(&format!("set -e\n{TEST}{PLANT}{DONE}"));
    let id = bench.start();
    assert_eq!(bench.settled(&id).state, PatchState::Proven);
    // Someone else changes the law's line on main after the run's clone.
    let root = bench.dir.path().canonicalize().unwrap();
    let seed = root.join("seed");
    let map = fs::read_to_string(seed.join("system-map.json")).unwrap();
    fs::write(
        seed.join("system-map.json"),
        map.replace("add sums its two numbers.", "add returns the sum."),
    )
    .unwrap();
    git(&seed, &["commit", "-q", "-am", "Reword"]);
    git(&seed, &["push", "-q", "origin", "main"]);
    let files = tree(&bench.repo);

    assert_eq!(
        bench.laws.open_pr(&bench.project, &id).unwrap(),
        Opened::OutOfDate
    );
    let patch = bench
        .laws
        .patches(&bench.project)
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    assert_eq!(patch.state, PatchState::OutOfDate);
    assert!(!root.join("gh.args").exists(), "no PR is opened");
    let origin = root.join("origin.git");
    assert_eq!(
        git(&origin, &["branch", "--list", "back-a-law/*"]),
        "",
        "nothing is pushed"
    );
    assert_eq!(
        git(&bench.repo, &["worktree", "list", "--porcelain"])
            .matches("worktree ")
            .count(),
        1
    );
    assert_eq!(
        git(&bench.repo, &["branch", "--list", "back-a-law/*"]),
        "",
        "the branch is not left behind"
    );
    assert_eq!(git(&bench.repo, &["branch", "--show-current"]), "mine");
    assert_eq!(tree(&bench.repo), files);
    // Discard still deletes it.
    bench.laws.discard(&bench.project, &id).unwrap();
    assert!(bench.laws.patches(&bench.project).is_empty());
}
