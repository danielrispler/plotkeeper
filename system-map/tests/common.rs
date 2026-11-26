//! Temporary git repositories with a map in them.

use std::{fs, path::Path, process::Command};

use serde_json::{Value, json};
use studio_test_support::git;

/// A valid map: two parts, an actor, an import link (unproven, since the map names no extractor), an ignored folder and
/// a law.
pub(crate) fn base() -> Value {
    json!({
        "version": 2,
        "parts": [
            { "id": "alpha", "name": "Alpha", "caption": "First", "purpose": "The first part.", "holds": [],
              "tier": "core", "paths": ["alpha/"] },
            { "id": "beta", "name": "Beta", "caption": "Second", "purpose": "The second part.", "holds": [],
              "tier": "interface", "paths": ["beta/"] }
        ],
        "actors": [{ "id": "writer", "name": "Writer" }],
        "links": [{ "id": "alpha-beta", "from": "alpha", "to": "beta", "kind": "import" }],
        "ignored": ["docs/", "system-map.json"],
        "laws": [{ "id": "stay-true", "on": "alpha", "backing": "trust", "text": "It stays true." }]
    })
}

/// `base` as a version 1 map holds it: each part's text is one `description`, with no `holds`.
pub(crate) fn base_v1() -> Value {
    json!({
        "version": 1,
        "parts": [
            { "id": "alpha", "name": "Alpha", "caption": "First", "description": "The first part.",
              "tier": "core", "paths": ["alpha/"] },
            { "id": "beta", "name": "Beta", "caption": "Second", "description": "The second part.",
              "tier": "interface", "paths": ["beta/"] }
        ],
        "actors": [{ "id": "writer", "name": "Writer" }],
        "links": [{ "id": "alpha-beta", "from": "alpha", "to": "beta" }],
        "ignored": ["docs/", "system-map.json"],
        "laws": [{ "id": "stay-true", "on": "alpha", "backing": "trust", "text": "It stays true." }]
    })
}

/// The files `base` places: one per part and one ignored.
pub(crate) const FILES: [&str; 3] = ["alpha/a.rs", "beta/b.rs", "docs/d.md"];

/// A repository that tracks `files` and `system-map.json` holding `map`, and its root.
pub(crate) fn project(map: &Value, files: &[&str]) -> (tempfile::TempDir, String) {
    project_text(&map.to_string(), files)
}

pub(crate) fn project_text(map: &str, files: &[&str]) -> (tempfile::TempDir, String) {
    let (dir, root) = studio_test_support::repo();
    write(dir.path(), "system-map.json", map);
    for file in files {
        write(dir.path(), file, "x\n");
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "Files"]);
    (dir, root)
}

pub(crate) fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The problems `check` reports for a repository.
pub(crate) fn problems(root: &str) -> Vec<String> {
    system_map::check(Path::new(root)).problems
}

/// Whether some problem mentions every one of `needles`.
pub(crate) fn has(problems: &[String], needles: &[&str]) -> bool {
    problems
        .iter()
        .any(|line| needles.iter().all(|needle| line.contains(needle)))
}

/// A map naming `extractor` whose parts are `parts`, each an id with its paths, whose import links are `links`, and
/// which ignores `ignored` besides its own file.
pub(crate) fn extractor_map(
    extractor: &str,
    parts: &[(&str, &[&str])],
    links: &[(&str, &str)],
    ignored: &[&str],
) -> Value {
    let mut ignored = ignored.to_vec();
    ignored.push("system-map.json");
    json!({
        "version": 2,
        "extractors": [extractor],
        "parts": parts.iter().map(|(id, paths)| json!({
            "id": id, "name": id, "caption": id, "purpose": "A part.", "holds": [], "tier": "core", "paths": paths
        })).collect::<Vec<_>>(),
        "actors": [],
        "links": links.iter().map(|(from, to)| json!({
            "id": format!("{from}-{to}"), "from": from, "to": to, "kind": "import"
        })).collect::<Vec<_>>(),
        "ignored": ignored,
        "laws": []
    })
}

/// A repository holding `map` and `files` with their texts, committed.
pub(crate) fn repo(map: &Value, files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let (dir, root) = project_text(&map.to_string(), &[]);
    for (file, text) in files {
        write(dir.path(), file, text);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "Code"]);
    (dir, root)
}

/// What `check` reports for a repository.
pub(crate) fn report(root: &str) -> system_map::Report {
    system_map::check(Path::new(root))
}

/// Every file pair of the computed edges, as `from>to`.
pub(crate) fn pairs(report: &system_map::Report) -> Vec<String> {
    report
        .edges
        .iter()
        .flat_map(|e| e.pairs.iter().map(|p| format!("{}>{}", p.from, p.to)))
        .collect()
}

/// The `system-map` binary run as `command` with `args` on the repository at `root`.
pub(crate) fn run(root: &str, command: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg(command)
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}
