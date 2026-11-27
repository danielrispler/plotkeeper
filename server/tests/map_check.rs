//! `plotkeeper map check` as it runs in a repository, in CI or at the prompt (spec #753, "Keep it current"): the output
//! of `system-map check`, and exit 0 on a pass, 1 on a fail and 2 on an error.

use std::process::{Command, Output};

use studio_test_support::{git, repo};

const MAP: &str = r#"{
  "version": 2,
  "parts": [
    { "id": "alpha", "name": "Alpha", "caption": "First", "purpose": "The first part.", "holds": [], "tier": "core", "paths": ["alpha/"] }
  ],
  "actors": [],
  "links": [],
  "ignored": ["system-map.json"],
  "laws": []
}"#;

/// A repository tracking `alpha/a.rs`, `extra` when given, and the map.
fn project(extra: Option<&str>) -> (tempfile::TempDir, String) {
    let (dir, root) = repo();
    std::fs::create_dir_all(dir.path().join("alpha")).unwrap();
    std::fs::write(dir.path().join("alpha/a.rs"), "x\n").unwrap();
    std::fs::write(dir.path().join("system-map.json"), MAP).unwrap();
    if let Some(extra) = extra {
        std::fs::write(dir.path().join(extra), "x\n").unwrap();
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "Files"]);
    (dir, root)
}

fn plotkeeper(args: &[&str]) -> Output {
    let home = tempfile::tempdir().unwrap();
    Command::new(env!("CARGO_BIN_EXE_plotkeeper"))
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn a_passing_map_prints_the_pass_line_and_exits_zero() {
    let (_dir, root) = project(None);
    let out = plotkeeper(&["map", "check", "--root", &root]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "ok: 1 part own 1 file (1 ignored); 0 links: 0 import (0 proven, 0 unproven), 0 http, 0 file, 0 spawn; 0 laws: 0 check, 0 trust\n"
    );
}

#[test]
fn a_failing_map_prints_its_problems_and_exits_one() {
    let (_dir, root) = project(Some("stray.txt"));
    let out = plotkeeper(&["map", "check", "--root", &root]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    assert!(String::from_utf8(out.stdout).unwrap().contains("stray.txt"));
}

#[test]
fn json_and_a_commit_are_the_checkers_own() {
    let (_dir, root) = project(None);
    let out = plotkeeper(&[
        "map", "check", "--json", "--commit", "HEAD", "--root", &root,
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["parts"][0]["id"], "alpha");
}

#[test]
fn an_error_exits_two_and_the_other_map_commands_stay_the_checkers() {
    let (_dir, root) = project(None);
    let out = plotkeeper(&["map", "check", "--bogus", "--root", &root]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("unknown argument")
    );
    for command in ["touched", "imports", "bogus"] {
        let out = plotkeeper(&["map", command, "--root", &root]);
        assert_eq!(out.status.code(), Some(1), "{command}: {out:?}");
        assert!(
            String::from_utf8(out.stderr)
                .unwrap()
                .contains("unknown command"),
            "{command}"
        );
    }
}

#[test]
fn scan_prints_the_free_scan_as_text_or_json_and_exits_zero() {
    let (_dir, root) = project(None);
    let text = plotkeeper(&["map", "scan", "--root", &root]);
    assert_eq!(text.status.code(), Some(0), "{text:?}");
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.starts_with("1 part\n  alpha: alpha/\n"), "{text}");
    let json = plotkeeper(&["map", "scan", "--json", "--root", &root]);
    assert_eq!(json.status.code(), Some(0), "{json:?}");
    let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(value["map"]["parts"][0]["paths"][0], "alpha/");
    assert_eq!(value["loose_files"], serde_json::json!([]));
}
