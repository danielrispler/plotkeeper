use super::*;
use system_map::Tag;

const NEXTEST: &str = include_str!("../../tests/ci-logs/nextest-failed.log");
const NEXTEST_FAIL_FAST: &str = include_str!("../../tests/ci-logs/nextest-fail-fast.log");
const NODE_TAP: &str = include_str!("../../tests/ci-logs/node-tap-failed.log");
const NODE_SPEC: &str = include_str!("../../tests/ci-logs/node-spec-failed.log");
const NEXTEST_REAL: &str = include_str!("../../tests/ci-logs/nextest-real-binaries.log");
const FMT: &str = include_str!("../../tests/ci-logs/fmt-failed.log");

fn tagged(law: &str, file: &str, test: &str) -> LawTest {
    LawTest {
        law: law.into(),
        tag: Tag {
            file: file.into(),
            line: 1,
        },
        test: Some(test.into()),
    }
}
fn names(failures: &[Failure]) -> Vec<&str> {
    failures.iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn the_runners_prefix_and_colour_are_stripped_from_each_line() {
    let log = "macos\tRun npm run check\t2026-11-22T09:12:01.1234567Z \u{1b}[31mFAIL\u{1b}[0m [   0.1s] a b\nplain line";
    assert_eq!(clean_lines(log), vec!["FAIL [   0.1s] a b", "plain line"]);
}

#[test]
fn a_nextest_log_names_every_failing_test_once_with_its_binary() {
    let found = parse_failures(NEXTEST);
    assert_eq!(
        names(&found),
        [
            "unrelated_in_same_file",
            "tests::unit_fails",
            "wire_types_match"
        ]
    );
    assert_eq!(found[0].binary.as_deref(), Some("studio-nx::wire"));
    assert_eq!(found[1].binary.as_deref(), Some("studio-nx"));
}

#[test]
fn a_node_tap_log_names_each_failing_test_with_its_file() {
    let found = parse_failures(NODE_TAP);
    assert_eq!(
        names(&found),
        ["a delete keeps the live terminal", "an unrelated one fails"]
    );
    assert_eq!(
        found[0].file.as_deref(),
        Some("/Users/runner/work/plotkeeper/plotkeeper/ui/tests/t.test.mjs")
    );
    assert_eq!(found[0].binary, None);
}

#[test]
fn a_tap_failure_titled_like_an_earlier_one_does_not_panic_on_its_location() {
    let log = "not ok 1 - dup\nnot ok 2 - dup\n  location: 'ui/tests/a.test.mjs:5:1'\n";
    let found = parse_failures(log);
    assert_eq!(names(&found), ["dup"]);
}

#[test]
fn a_node_spec_log_names_each_failing_test_with_its_file() {
    let found = parse_failures(NODE_SPEC);
    assert_eq!(
        names(&found),
        ["a delete keeps the live terminal", "an unrelated one fails"]
    );
    assert_eq!(found[1].file.as_deref(), Some("ui/tests/t.test.mjs"));
}

#[test]
fn a_formatting_failure_names_no_test() {
    assert!(parse_failures(FMT).is_empty());
}

#[test]
fn only_the_tagged_test_of_a_nextest_log_breaks_its_law() {
    let tests = [tagged(
        "wire-types-agree",
        "studio-nx/tests/wire.rs",
        "wire_types_match",
    )];
    assert_eq!(
        broken_laws(&parse_failures(NEXTEST), &tests),
        ["wire-types-agree"]
    );
    assert_eq!(
        broken_laws(&parse_failures(NEXTEST_FAIL_FAST), &tests),
        ["wire-types-agree"]
    );
}

#[test]
fn a_failure_in_this_workspaces_it_binary_breaks_the_law_tagged_in_its_test_file() {
    let failures = parse_failures(NEXTEST_REAL);
    let tests = [tagged(
        "wire-types-agree",
        "core/tests/wire.rs",
        "the_wire_samples_of_the_core_are_written_for_tsc",
    )];
    assert_eq!(broken_laws(&failures, &tests), ["wire-types-agree"]);
    // The same function in another test file, another crate, or a passing one breaks nothing.
    for (file, test) in [
        (
            "core/tests/other.rs",
            "the_wire_samples_of_the_core_are_written_for_tsc",
        ),
        (
            "understanding/tests/wire.rs",
            "the_wire_samples_of_the_core_are_written_for_tsc",
        ),
        ("core/tests/wire.rs", "passes"),
    ] {
        assert!(
            broken_laws(&failures, &[tagged("l", file, test)]).is_empty(),
            "{file} {test}"
        );
    }
    // A unit test of the crate's own source: bare crate binary, module path.
    let unit = [tagged("u", "core/src/ids.rs", "unit_fails")];
    assert_eq!(broken_laws(&failures, &unit), ["u"]);
    let other = [tagged("u", "core/src/other.rs", "unit_fails")];
    assert!(broken_laws(&failures, &other).is_empty());
}

#[test]
fn an_unrelated_failing_test_in_the_same_file_breaks_no_law() {
    let tests = [tagged(
        "wire-types-agree",
        "studio-nx/tests/wire.rs",
        "passes",
    )];
    assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
    let tests = [tagged(
        "delete-keeps-live-terminal",
        "ui/tests/t.test.mjs",
        "a delete keeps the live terminal and more",
    )];
    assert!(broken_laws(&parse_failures(NODE_TAP), &tests).is_empty());
}

#[test]
fn a_test_of_the_same_name_in_another_binary_breaks_no_law() {
    let tests = [tagged("l", "studio-nx/tests/other.rs", "wire_types_match")];
    assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
    // A Node test never matches a Rust failure, nor the other way round.
    let tests = [
        tagged("l", "ui/tests/t.test.mjs", "wire_types_match"),
        tagged("m", "src/lib.rs", "a delete keeps the live terminal"),
    ];
    assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
    assert!(broken_laws(&parse_failures(NODE_TAP), &tests).is_empty());
}

#[test]
fn a_failing_node_test_breaks_the_law_tagged_above_it() {
    let tests = [tagged(
        "delete-keeps-live-terminal",
        "ui/tests/t.test.mjs",
        "a delete keeps the live terminal",
    )];
    assert_eq!(
        broken_laws(&parse_failures(NODE_TAP), &tests),
        ["delete-keeps-live-terminal"]
    );
    assert_eq!(
        broken_laws(&parse_failures(NODE_SPEC), &tests),
        ["delete-keeps-live-terminal"]
    );
    // The same title in another file is another test.
    let elsewhere = [tagged(
        "delete-keeps-live-terminal",
        "ui/tests/home.test.mjs",
        "a delete keeps the live terminal",
    )];
    assert!(broken_laws(&parse_failures(NODE_TAP), &elsewhere).is_empty());
}

#[test]
fn a_tag_with_no_test_after_it_breaks_nothing() {
    let tests = [LawTest {
        law: "l".into(),
        tag: Tag {
            file: "a.rs".into(),
            line: 1,
        },
        test: None,
    }];
    assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
}

#[test]
fn a_failed_law_test_is_broken_and_keeps_the_log_tail() {
    let tests = [tagged(
        "wire-types-agree",
        "studio-nx/tests/wire.rs",
        "wire_types_match",
    )];
    let found = assess(Some(NEXTEST), &tests);
    assert_eq!(found.status, Status::Broken);
    assert_eq!(found.laws, ["wire-types-agree"]);
    assert!(found.tail.contains("Summary") && found.tail.lines().count() <= TAIL);
    assert!(!found.tail.contains('\t'), "the columns are stripped");
}

#[test]
fn a_run_that_failed_before_the_tests_ran_says_the_law_tests_did_not_run() {
    let tests = [tagged(
        "wire-types-agree",
        "studio-nx/tests/wire.rs",
        "wire_types_match",
    )];
    let found = assess(Some(FMT), &tests);
    assert_eq!(found.status, Status::NotRun);
    assert!(found.laws.is_empty());
    assert!(found.tail.contains("Diff in"));
}

#[test]
fn a_run_that_failed_only_in_untagged_tests_marks_nothing() {
    let tests = [tagged(
        "wire-types-agree",
        "studio-nx/tests/wire.rs",
        "passes",
    )];
    let found = assess(Some(NEXTEST), &tests);
    assert_eq!(found.status, Status::NotRun);
    assert!(found.laws.is_empty());
}

#[test]
fn a_log_that_cannot_be_fetched_or_is_empty_is_unreadable_and_marks_nothing() {
    let tests = [tagged(
        "wire-types-agree",
        "studio-nx/tests/wire.rs",
        "wire_types_match",
    )];
    for log in [None, Some(""), Some("  \n\n")] {
        let found = assess(log, &tests);
        assert_eq!(found.status, Status::Unreadable);
        assert!(found.laws.is_empty());
    }
}

fn run(id: u64, status: &str, conclusion: Option<&str>) -> Run {
    Run {
        database_id: id,
        status: status.into(),
        conclusion: conclusion.map(String::from),
        head_sha: format!("sha{id}"),
        url: format!("https://github.com/o/r/actions/runs/{id}"),
        updated_at: None,
    }
}

#[test]
fn gh_run_list_json_reads_into_runs() {
    let json = r#"[{"conclusion":"failure","databaseId":42,"headSha":"abc","status":"completed","url":"https://github.com/o/r/actions/runs/42","updatedAt":"2026-11-24T10:00:00Z"},
                   {"conclusion":"","databaseId":41,"headSha":"abd","status":"in_progress","url":"u"}]"#;
    let runs = parse_runs(json).unwrap();
    let mut finished = run(42, "completed", Some("failure")).with("abc");
    finished.updated_at = Some("2026-11-24T10:00:00Z".into());
    assert_eq!(runs[0], finished);
    assert_eq!(
        runs[1].updated_at, None,
        "an answer without the field reads"
    );
    assert_eq!(runs[1].conclusion.as_deref(), Some(""));
    assert!(parse_runs("not json").is_err());
}
impl Run {
    fn with(mut self, sha: &str) -> Run {
        self.head_sha = sha.into();
        self.url = format!("https://github.com/o/r/actions/runs/{}", self.database_id);
        self
    }
}

#[test]
fn the_latest_finished_run_is_read_and_a_newer_one_in_progress_says_it_is_for_an_earlier_commit() {
    let runs = [
        run(5, "in_progress", None),
        run(4, "completed", Some("cancelled")),
        run(3, "completed", Some("failure")),
        run(2, "completed", Some("success")),
    ];
    let (picked, earlier) = pick(&runs).unwrap();
    assert_eq!((picked.database_id, earlier), (3, true));
    let (picked, earlier) = pick(&runs[2..]).unwrap();
    assert_eq!((picked.database_id, earlier), (3, false));
    assert!(pick(&runs[..2]).is_none());
    assert!(pick(&[]).is_none());
}

const RUST_TESTS: &str = "use x;\n\n// @law: wire-types-agree\n#[test]\nfn wire_types_match() {\n    assert_eq!(1, 1);\n}\n\n#[test]\nfn another() {}\n";

#[test]
fn the_proof_is_the_tag_and_the_test_after_it_up_to_its_closing_line() {
    assert_eq!(
        snippet(RUST_TESTS, 3),
        (
            3,
            "// @law: wire-types-agree\n#[test]\nfn wire_types_match() {\n    assert_eq!(1, 1);\n}"
                .to_string()
        )
    );
    let node =
        "// @law: a\ntest(\"one\", () => {\n  assert.ok(1);\n});\ntest(\"two\", () => {});\n";
    assert_eq!(
        snippet(node, 1),
        (
            1,
            "// @law: a\ntest(\"one\", () => {\n  assert.ok(1);\n});".to_string()
        )
    );
    assert_eq!(
        snippet("// @law: a\n#[test]\nfn one() {}\nfn two() {}", 1).1,
        "// @law: a\n#[test]\nfn one() {}"
    );
}

#[test]
fn a_proof_that_never_closes_stops_at_the_limit() {
    let text = format!("// @law: a\nfn long() {{\n{}", "    x;\n".repeat(200));
    assert_eq!(snippet(&text, 1).1.lines().count(), SNIPPET);
}

fn commit_file(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    studio_test_support::git(dir, &["add", name]);
    studio_test_support::git(dir, &["commit", "-q", "-m", name]);
}

#[test]
fn the_tagged_tests_are_read_from_the_default_branch_not_the_working_tree() {
    let o = studio_test_support::origin();
    std::fs::create_dir_all(o.seed.join("tests")).unwrap();
    commit_file(&o.seed, "tests/wire.rs", RUST_TESTS);
    studio_test_support::git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    studio_test_support::git(&o.main, &["fetch", "-q", "origin"]);
    std::fs::create_dir_all(o.main.join("tests")).unwrap();
    commit_file(&o.main, "tests/wire.rs", "// @law: other\nfn nope() {}\n");
    let found = law_tests_at(&o.main, "origin/trunk");
    assert_eq!(
        found,
        [LawTest {
            law: "wire-types-agree".into(),
            tag: system_map::Tag {
                file: "tests/wire.rs".into(),
                line: 3
            },
            test: Some("wire_types_match".into())
        }]
    );
    assert!(law_tests_at(&o.main, "origin/nowhere").is_empty());
    let map = r#"{"version":1,"parts":[{"id":"shell","name":"S","caption":"","description":"","tier":"core","paths":["tests/"]}],"actors":[],"links":[],"ignored":[],"laws":[{"id":"wire-types-agree","on":"shell","backing":"check","text":"t"}]}"#;
    commit_file(&o.seed, "system-map.json", map);
    studio_test_support::git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    studio_test_support::git(&o.main, &["fetch", "-q", "origin"]);
    let proof = proofs(&o.main, "wire-types-agree");
    assert_eq!(
        (
            proof.len(),
            proof[0].file.as_str(),
            proof[0].line,
            proof[0].first
        ),
        (1, "tests/wire.rs", 3, 3)
    );
    assert!(proof[0].snippet.contains("fn wire_types_match"));
    assert!(proofs(&o.main, "no-such-law").is_empty());
    assert_eq!(
        parts_of(&o.main, &["wire-types-agree".to_string()]),
        ["shell"]
    );
    assert!(parts_of(&o.main, &["other".to_string()]).is_empty());
}

fn ci(status: Status, parts: &[&str]) -> Ci {
    Ci {
        status,
        run_url: None,
        commit: None,
        earlier: false,
        parts: parts.iter().map(ToString::to_string).collect(),
        laws: Vec::new(),
        tail: String::new(),
        note: None,
        at: None,
    }
}

#[test]
fn the_last_finished_result_survives_a_restart_and_older_files_read() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("ci-last.json");
    let cache = CiCache::load(file.clone());
    assert!(cache.put("/p", ci(Status::Broken, &["shell"])));
    assert!(cache.put("/q", ci(Status::Unknown, &[])));
    let again = CiCache::load(file.clone());
    assert!(again.broken("/p"));
    assert!(!again.has("/q"), "an unknown result is not kept");
    // An entry missing newer fields still reads; one that does not read is skipped.
    std::fs::write(
        &file,
        r#"{"/a":{"status":"broken","parts":["x"]},"/b":{"nonsense":1}}"#,
    )
    .unwrap();
    let older = CiCache::load(file.clone());
    assert!(older.broken("/a") && !older.has("/b"));
    std::fs::write(&file, "not json").unwrap();
    assert!(!CiCache::load(file).has("/a"));
}

#[test]
fn the_last_result_is_kept_in_a_data_folder_that_does_not_exist_yet() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("fresh").join("ci-last.json");
    let cache = CiCache::load(file.clone());
    assert!(cache.put("/p", ci(Status::Broken, &["shell"])));
    assert!(CiCache::load(file).broken("/p"));
}

#[test]
fn a_panicking_read_still_frees_its_project() {
    let cache = CiCache::default();
    cache.running.lock().unwrap().insert("/p".into());
    let freed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _running = Running(&cache.running, "/p".into());
        panic!("read panicked");
    }));
    assert!(freed.is_err());
    assert!(!cache.running.lock().unwrap().contains("/p"));
}

#[test]
fn a_failed_read_never_replaces_a_real_result_and_a_mark_needs_a_part() {
    let cache = CiCache::default();
    assert!(!cache.broken("/p"));
    assert!(cache.put("/p", ci(Status::Broken, &["shell"])));
    assert!(
        !cache.put("/p", ci(Status::Broken, &["shell"])),
        "the same result changes nothing"
    );
    assert!(cache.broken("/p"));
    assert!(
        !cache.put("/p", ci(Status::Unknown, &[])),
        "gh failing keeps what was known"
    );
    assert!(cache.broken("/p"));
    assert!(cache.put("/p", ci(Status::NotRun, &[])));
    assert!(!cache.broken("/p"));
    assert!(!cache.has("/q") && cache.put("/q", ci(Status::Unknown, &[])) && cache.has("/q"));
}

const MAP: &str = r#"{"version":1,"parts":[
  {"id":"core","name":"Core","caption":"c","description":"d","tier":"core","paths":["core/"]},
  {"id":"understanding","name":"Understanding","caption":"c","description":"d","tier":"core","paths":["understanding/"]},
  {"id":"window","name":"Window","caption":"c","description":"d","tier":"core","paths":["ui/"]}],
  "actors":[],"links":[],"ignored":[],"laws":[]}"#;

#[test]
fn a_failed_check_names_its_one_test_and_the_part_that_owns_it() {
    assert_eq!(
        name_failed_check(NODE_TAP, "npm run check", Some(MAP)),
        "Window tests failed: a delete keeps the live terminal (+1 more)"
    );
    let one =
        "not ok 1 - a delete keeps the live terminal\n  location: 'ui/tests/t.test.mjs:5:1'\n";
    assert_eq!(
        name_failed_check(one, "npm run check", Some(MAP)),
        "Window tests failed: a delete keeps the live terminal"
    );
}

#[test]
fn several_failures_across_parts_name_the_first_and_count_the_rest() {
    assert_eq!(
        name_failed_check(NEXTEST_REAL, "cargo nextest run", Some(MAP)),
        "Core tests failed: the wire samples of the core are written for tsc (+2 more)"
    );
}

#[test]
fn a_failure_with_no_test_parsed_names_the_step() {
    assert_eq!(
        name_failed_check(FMT, "npm run check", Some(MAP)),
        "formatting check failed"
    );
    assert_eq!(
        name_failed_check("error: could not compile `x`", "cargo build", None),
        "build failed"
    );
    assert_eq!(name_failed_check("", "go vet", None), "go vet failed");
}

#[test]
fn a_project_without_a_map_names_the_test_alone() {
    assert_eq!(
        name_failed_check(NODE_SPEC, "npm run check", None),
        "Test failed: a delete keeps the live terminal (+1 more)"
    );
    // A test no part owns is named the same way.
    assert_eq!(
        name_failed_check(NEXTEST, "cargo nextest run", Some(MAP)),
        "Test failed: unrelated in same file (+2 more)"
    );
}
