//! Which parts a set of paths touches, by pattern alone.

use std::path::Path;

use serde_json::{Value, json};

use crate::common::{base, project, project_text};

fn touched(map: &Value, paths: &[&str]) -> system_map::Touched {
    let (_dir, root) = project(map, &["alpha/a.rs"]);
    let paths: Vec<String> = paths.iter().map(ToString::to_string).collect();
    system_map::touched(Path::new(&root), &paths).unwrap()
}

fn parts(got: &system_map::Touched) -> Vec<(&str, Vec<&str>)> {
    got.parts
        .iter()
        .map(|p| (p.id.as_str(), p.files.iter().map(String::as_str).collect()))
        .collect()
}

#[test]
fn each_owning_part_comes_once_with_its_paths() {
    let got = touched(&base(), &["alpha/a.rs", "beta/b.rs", "alpha/deep/x.rs"]);
    assert_eq!(
        parts(&got),
        vec![
            ("alpha", vec!["alpha/a.rs", "alpha/deep/x.rs"]),
            ("beta", vec!["beta/b.rs"])
        ]
    );
}

#[test]
fn a_path_missing_from_the_repository_is_placed_by_its_pattern() {
    let got = touched(&base(), &["beta/deleted/gone.rs"]);
    assert_eq!(parts(&got), vec![("beta", vec!["beta/deleted/gone.rs"])]);
}

#[test]
fn star_and_double_star_and_exact_patterns_place_paths() {
    let mut map = base();
    map["parts"][0]["paths"] = json!(["alpha/*.rs", "alpha/**/deep.rs"]);
    map["parts"][1]["paths"] = json!(["beta/exact.txt"]);
    let got = touched(
        &map,
        &[
            "alpha/a.rs",
            "alpha/x/y/deep.rs",
            "beta/exact.txt",
            "alpha/sub/a.rs",
            "beta/other.txt",
        ],
    );
    assert_eq!(
        parts(&got),
        vec![
            ("alpha", vec!["alpha/a.rs", "alpha/x/y/deep.rs"]),
            ("beta", vec!["beta/exact.txt"])
        ]
    );
    assert_eq!(got.unowned, vec!["alpha/sub/a.rs", "beta/other.txt"]);
}

#[test]
fn the_last_matching_part_wins() {
    let mut map = base();
    map["parts"][1]["paths"] = json!(["beta/", "alpha/shared/"]);
    let got = touched(&map, &["alpha/shared/s.rs", "alpha/a.rs"]);
    assert_eq!(
        parts(&got),
        vec![
            ("alpha", vec!["alpha/a.rs"]),
            ("beta", vec!["alpha/shared/s.rs"])
        ]
    );
}

#[test]
fn an_ignored_match_wins_over_every_part() {
    let mut map = base();
    map["ignored"] = json!(["alpha/gen/", "docs/", "system-map.json"]);
    let got = touched(&map, &["alpha/gen/g.rs", "docs/d.md", "alpha/a.rs"]);
    assert_eq!(parts(&got), vec![("alpha", vec!["alpha/a.rs"])]);
    assert_eq!(got.ignored, vec!["alpha/gen/g.rs", "docs/d.md"]);
    assert!(got.unowned.is_empty());
}

#[test]
fn the_map_file_is_ignored_with_no_ignored_entry_for_it() {
    let mut map = base();
    map["ignored"] = json!(["docs/"]);
    map["parts"][0]["paths"] = json!(["alpha/", "*.json"]);
    let got = touched(&map, &["system-map.json"]);
    assert_eq!(got.ignored, vec!["system-map.json"]);
    assert!(got.parts.is_empty() && got.unowned.is_empty(), "{got:?}");
}

#[test]
fn paths_no_pattern_matches_are_listed_apart() {
    let got = touched(&base(), &["stray.txt", "alpha/a.rs"]);
    assert_eq!(parts(&got), vec![("alpha", vec!["alpha/a.rs"])]);
    assert_eq!(got.unowned, vec!["stray.txt"]);
    assert!(got.ignored.is_empty());
}

#[test]
fn a_map_that_fails_its_shape_check_gives_its_problem_not_an_answer() {
    let (_dir, root) = project_text("{ \"version\": 1 }", &["a"]);
    let error = system_map::touched(Path::new(&root), &["a".to_string()]).unwrap_err();
    assert!(error[0].contains("system-map.json"), "{error:?}");
}

mod binary {
    use std::{
        io::Write,
        process::{Command, Output, Stdio},
    };

    use serde_json::Value;

    use crate::common::{base, project, project_text};

    fn run(root: &str, args: &[&str], stdin: Option<&str>) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_system-map"))
            .arg("touched")
            .args(args)
            .args(["--root", root])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut pipe = child.stdin.take().unwrap();
        pipe.write_all(stdin.unwrap_or("").as_bytes()).unwrap();
        drop(pipe);
        child.wait_with_output().unwrap()
    }

    #[test]
    fn arguments_and_standard_input_give_the_same_answer() {
        let (_dir, root) = project(&base(), &["alpha/a.rs"]);
        let by_args = run(&root, &["--json", "alpha/a.rs", "beta/b.rs"], None);
        let by_stdin = run(&root, &["--json"], Some("alpha/a.rs\nbeta/b.rs\n"));
        assert!(by_args.status.success());
        assert_eq!(by_args.stdout, by_stdin.stdout);
        let value: Value = serde_json::from_slice(&by_args.stdout).unwrap();
        assert_eq!(value["parts"][1]["id"], "beta");
        assert_eq!(value["parts"][1]["files"][0], "beta/b.rs");
        let plain = run(&root, &["alpha/a.rs", "beta/b.rs"], None);
        assert_eq!(
            plain.stdout,
            run(&root, &[], Some("alpha/a.rs\nbeta/b.rs\n")).stdout
        );
        assert!(!plain.stdout.is_empty());
    }

    #[test]
    fn a_bad_map_fails_with_its_problem() {
        let (_dir, root) = project_text("{}", &["a"]);
        let out = run(&root, &["a"], None);
        assert!(!out.status.success());
        assert!(out.stdout.is_empty());
        assert!(
            String::from_utf8(out.stderr)
                .unwrap()
                .contains("system-map.json")
        );
    }
}

#[test]
fn a_map_that_fails_the_shape_check_makes_touched_fail_with_every_problem() {
    let mut map = base();
    map["parts"][1]["id"] = json!("alpha");
    map["actors"][0]["id"] = json!("Bad Id");
    map["links"][0]["to"] = json!("nowhere");
    map["laws"][0]["on"] = json!("nothing");
    let (_dir, root) = project(&map, &["alpha/a.rs"]);
    let problems = system_map::touched(Path::new(&root), &["alpha/a.rs".to_string()])
        .expect_err("a map with problems must not answer");
    let check = system_map::check(Path::new(&root)).problems;
    for needle in ["Bad Id", "more than once", "nowhere", "nothing"] {
        assert!(
            problems.iter().any(|p| p.contains(needle)),
            "{needle}: {problems:?}"
        );
    }
    assert!(problems.len() >= 4 && problems.iter().all(|p| check.contains(p)));
}

#[test]
fn a_hop_on_a_missing_link_makes_touched_fail() {
    let mut map = base();
    map["flows"] =
        json!([{ "id": "go", "name": "Go", "hops": [{ "link": "ghost", "text": "t" }] }]);
    let (_dir, root) = project(&map, &["alpha/a.rs"]);
    let problems = system_map::touched(Path::new(&root), &["alpha/a.rs".to_string()])
        .expect_err("a hop on a missing link must not answer");
    assert!(problems.iter().any(|p| p.contains("ghost")), "{problems:?}");
}

#[test]
fn many_double_stars_against_a_path_that_does_not_match_finish_at_once() {
    let mut map = base();
    map["parts"][0]["paths"] = json!(["a/**/**/**/**/**/**/**/**/**/**/**/**/c"]);
    let path = format!("a/{}d", "x/".repeat(34));
    let got = touched(&map, &[&path]);
    assert_eq!(got.unowned, vec![path]);
}

#[test]
fn a_version_other_than_two_makes_touched_fail_and_names_it() {
    let mut map = base();
    map["version"] = json!(3);
    let (_dir, root) = project(&map, &["alpha/a.rs"]);
    let problems = system_map::touched(Path::new(&root), &["alpha/a.rs".to_string()])
        .expect_err("a map of another version must not answer");
    assert!(
        problems
            .iter()
            .any(|p| p.contains("version") && p.contains('3') && p.contains("version 2")),
        "{problems:?}"
    );
}

#[test]
fn touched_for_map_places_paths_without_reading_the_working_tree() {
    let map = system_map::parse(&base().to_string()).unwrap();
    let paths = ["beta/b.rs", "docs/d.md", "stray.txt", "beta/b.rs"].map(String::from);
    let got = system_map::touched_for_map(&map, &paths);
    assert_eq!(parts(&got), vec![("beta", vec!["beta/b.rs"])]);
    assert_eq!(got.ignored, vec!["docs/d.md"]);
    assert_eq!(got.unowned, vec!["stray.txt"]);
    let (_dir, root) = project(&base(), &["alpha/a.rs"]);
    let by_root = system_map::touched(Path::new(&root), &paths).unwrap();
    assert_eq!(parts(&by_root), parts(&got));
    assert_eq!(by_root.ignored, got.ignored);
    assert_eq!(by_root.unowned, got.unowned);
}

#[test]
fn a_map_over_a_cap_still_answers() {
    let mut map = base();
    map["parts"][0]["purpose"] = json!("a".repeat(241));
    let got = touched(&map, &["alpha/a.rs"]);
    assert_eq!(parts(&got), vec![("alpha", vec!["alpha/a.rs"])]);
}

#[test]
fn the_held_items_a_list_of_files_reaches_are_those_whose_paths_match_one() {
    let mut map = base();
    map["parts"][0]["holds"] = json!([
        { "name": "File", "text": "One file.", "paths": ["alpha/a.rs"] },
        { "name": "Folder", "text": "A folder.", "paths": ["alpha/deep/"] },
        { "name": "Star", "text": "Any test.", "paths": ["alpha/**/*_test.rs"] },
        { "name": "Anywhere", "text": "No paths." },
        { "name": "Empty", "text": "No paths either.", "paths": [] }
    ]);
    let map = system_map::parse(&map.to_string()).unwrap();
    let cases: [(&str, &[&str], &[usize]); 6] = [
        ("an exact path", &["alpha/a.rs"], &[0]),
        ("a file in a folder", &["alpha/deep/x.rs"], &[1]),
        (
            "a double star",
            &["alpha/x/y/z_test.rs", "alpha/deep/x.rs"],
            &[1, 2],
        ),
        ("a file no held item names", &["alpha/b.rs"], &[]),
        ("another part's file", &["beta/b.rs"], &[]),
        ("no files", &[], &[]),
    ];
    for (case, files, reached) in cases {
        let files: Vec<String> = files.iter().map(ToString::to_string).collect();
        assert_eq!(
            system_map::held_reached(&map, "alpha", &files),
            reached,
            "{case}"
        );
    }
    assert!(system_map::held_reached(&map, "nothing", &["alpha/a.rs".into()]).is_empty());
}
