//! Temporary git repositories with a map in them.

use std::{fs, path::Path};

use serde_json::{Value, json};
use studio_test_support::{git, repo};

/// A valid map: two parts, an actor, an import link (unproven, since the map names no extractor), an ignored folder and
/// a law.
pub fn base() -> Value {
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
pub fn base_v1() -> Value {
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
pub const FILES: [&str; 3] = ["alpha/a.rs", "beta/b.rs", "docs/d.md"];

/// A repository that tracks `files` and `system-map.json` holding `map`, and its root.
pub fn project(map: &Value, files: &[&str]) -> (tempfile::TempDir, String) {
    project_text(&map.to_string(), files)
}

pub fn project_text(map: &str, files: &[&str]) -> (tempfile::TempDir, String) {
    let (dir, root) = repo();
    write(dir.path(), "system-map.json", map);
    for file in files {
        write(dir.path(), file, "x\n");
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "Files"]);
    (dir, root)
}

pub fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// The problems `check` reports for a repository.
pub fn problems(root: &str) -> Vec<String> {
    system_map::check(Path::new(root)).problems
}

/// Whether some problem mentions every one of `needles`.
pub fn has(problems: &[String], needles: &[&str]) -> bool {
    problems
        .iter()
        .any(|line| needles.iter().all(|needle| line.contains(needle)))
}
