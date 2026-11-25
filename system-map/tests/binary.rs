//! The thin wrapper: exit status, one line per problem, and `--json`.

use std::process::{Command, Output};

use serde_json::Value;

use crate::common::{FILES, base, project};

fn run(root: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg("check")
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}

#[test]
fn a_passing_repository_exits_zero() {
    let (_dir, root) = project(&base(), &FILES);
    let out = run(&root, &[]);
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "ok: 2 parts own 2 files (2 ignored); 1 link; 1 law: 0 check, 1 trust\n"
    );
}

#[test]
fn the_pass_line_counts_in_the_plural_and_by_backing() {
    let mut map = base();
    map["links"] = serde_json::json!([
        { "id": "alpha-beta", "from": "alpha", "to": "beta" },
        { "id": "writer-alpha", "from": "writer", "to": "alpha" }
    ]);
    map["laws"] = serde_json::json!([
        { "id": "stay-true", "on": "alpha", "backing": "trust", "text": "It stays true." },
        { "id": "held", "on": "beta", "backing": "check", "text": "It is held." }
    ]);
    let (dir, root) = project(
        &map,
        &["alpha/a.rs", "alpha/b.rs", "beta/b.rs", "docs/d.md"],
    );
    crate::common::write(dir.path(), "beta/b.rs", &format!("// @{}: held\n", "law"));
    studio_test_support::git(dir.path(), &["commit", "-qam", "Tag"]);
    let out = run(&root, &[]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "ok: 2 parts own 3 files (2 ignored); 2 links; 2 laws: 1 check, 1 trust\n"
    );
}

#[test]
fn a_failing_repository_exits_non_zero_with_a_line_per_problem() {
    let mut map = base();
    map["ignored"] = serde_json::json!(["docs/", "old/", "system-map.json"]);
    let (_dir, root) = project(&map, &["alpha/a.rs", "beta/b", "docs/d", "stray"]);
    let out = run(&root, &[]);
    assert!(!out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
}

#[test]
fn json_prints_data_that_parses() {
    let (_dir, root) = project(&base(), &FILES);
    let out = run(&root, &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["parts"][0]["id"], "alpha");
    assert_eq!(value["problems"].as_array().unwrap().len(), 0);
    assert_eq!(
        value["counts"],
        serde_json::json!({
            "parts": 2, "files": 2, "ignored": 2, "links": 1,
            "laws": { "check": 0, "trust": 1 }
        })
    );
}

#[test]
fn json_lists_each_law_with_its_backing_and_its_tags_by_file_and_line() {
    let mut map = base();
    map["laws"][0]["backing"] = serde_json::json!("check");
    let (dir, root) = project(&map, &FILES);
    let tag = format!("// @{}: stay-true\n", "law");
    crate::common::write(dir.path(), "alpha/a.rs", &format!("fn a() {{}}\n{tag}"));
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Tag"]);
    let out = run(&root, &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["problems"].as_array().unwrap().len(), 0);
    let law = &value["laws"][0];
    assert_eq!(law["id"], "stay-true");
    assert_eq!(law["backing"], "check");
    assert_eq!(law["tags"][0]["file"], "alpha/a.rs");
    assert_eq!(law["tags"][0]["line"], 2);
}
