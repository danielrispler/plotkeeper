//! `plotkeeper map check` as it runs in a repository, in CI or at the prompt (spec #753, "Keep it current"): what `system-map check`
//! prints, compared with that binary's own output, and exit 0 on a pass, 1 on a fail and 2 on an error.

use std::{
    path::Path,
    process::{Command, Output},
};

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
    run(env!("CARGO_BIN_EXE_plotkeeper").as_ref(), args)
}

/// `system-map`, the checker's own binary, which a workspace build (`npm run check`'s `cargo nextest run --workspace`) puts beside
/// `plotkeeper`.
fn system_map(args: &[&str]) -> Output {
    let binary = Path::new(env!("CARGO_BIN_EXE_plotkeeper")).with_file_name("system-map");
    assert!(
        binary.exists(),
        "{} is not built: run this test in a workspace build, `cargo nextest run --workspace`",
        binary.display()
    );
    run(&binary, args)
}

fn run(binary: &Path, args: &[&str]) -> Output {
    let home = tempfile::tempdir().unwrap();
    Command::new(binary)
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn map_check_prints_what_system_map_check_prints_with_its_exit_codes() {
    /// A case: its name, a stray file the map does not own, the arguments after `--root <repo>`, and the exit code.
    type Case = (
        &'static str,
        Option<&'static str>,
        &'static [&'static str],
        i32,
    );
    let cases: [Case; 4] = [
        ("a pass", None, &[], 0),
        ("a fail", Some("stray.txt"), &[], 1),
        ("JSON at a commit", None, &["--json", "--commit", "HEAD"], 0),
        ("an unknown argument", None, &["--bogus"], 2),
    ];
    for (name, extra, args, code) in cases {
        let (_dir, root) = project(extra);
        let args: Vec<&str> = ["check", "--root", &root]
            .into_iter()
            .chain(args.iter().copied())
            .collect();
        let ours = plotkeeper(&[["map"].as_slice(), &args].concat());
        let theirs = system_map(&args);
        assert_eq!(ours.status.code(), Some(code), "{name}: {ours:?}");
        assert_eq!(ours.status.code(), theirs.status.code(), "{name}");
        assert_eq!(
            String::from_utf8_lossy(&ours.stdout),
            String::from_utf8_lossy(&theirs.stdout),
            "{name}"
        );
        // An error names the program it came from.
        let said = |out: &Output, prog: &str| {
            String::from_utf8_lossy(&out.stderr).replace(&format!("{prog}: "), "<prog>: ")
        };
        assert_eq!(
            said(&ours, "plotkeeper map"),
            said(&theirs, "system-map"),
            "{name}"
        );
    }
}

#[test]
fn the_other_map_commands_stay_the_checkers() {
    let (_dir, root) = project(None);
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
