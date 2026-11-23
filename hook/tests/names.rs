//! Every name a user can see says plotkeeper (#353): the plugins' files, and the hook's messages
//! and the environment variables it reads. The event log's `studio_*` fields, its `Studio*` event
//! names and the old data folder's name are stored names, and stay (#320, Names).
use std::{fs, path::Path};

fn old_name(text: &str) -> bool {
    // VS Code's app is "Visual Studio Code", which is no name of plotkeeper's (#361).
    let text = text.to_lowercase().replace("visual studio code", "");
    text.contains("studio") || text.contains("falinks")
}

/// Every file under `dir`, with its text.
fn files(dir: &Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files(&path));
        } else {
            found.push((
                path.display().to_string(),
                fs::read_to_string(&path).unwrap(),
            ));
        }
    }
    found
}

#[test]
fn the_plugins_name_no_studio() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for dir in ["plugin", "plugin-agy", "plugin-codex", "plugin-copilot"] {
        for (path, text) in files(&root.join(dir)) {
            let lines = text.lines().filter(|l| old_name(l));
            found.extend(lines.map(|line| format!("{path}: {line}")));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn the_hook_names_no_studio_but_its_stored_names() {
    let stored = |s: &str| {
        s.starts_with("studio_")
            || s.starts_with("Studio")
            || s == "dev.falinks.studio"
            || s == "studio-state.json"
    };
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for (path, text) in files(&src) {
        let code = text.lines().filter(|l| !l.trim_start().starts_with("//"));
        for line in code {
            // The string literals of the line: every second piece between quotes.
            let literals = line.split('"').skip(1).step_by(2);
            let old = literals.filter(|s| old_name(s) && !stored(s));
            found.extend(old.map(|literal| format!("{path}: {literal:?} in {line}")));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}
