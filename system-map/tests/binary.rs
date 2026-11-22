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
    assert!(out.stdout.is_empty());
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
}
