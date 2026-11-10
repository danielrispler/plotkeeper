//! The skills browser (#528): every skill in Daniel's skills folder, the project's `.claude/skills`
//! and the installed plugins, with its `SKILL.md`. It reads only `SKILL.md` files and lists folders,
//! never a JSON file under `~/.claude`, which may hold tokens (`apps/studio/AGENTS.md`).

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The skills of `home` and of the project at `project`: user and project first, then each plugin's,
/// each with its source, its `SKILL.md`'s path and text, and whether "Edit" may open it. Plugin
/// skills are read-only: the plugin's next update replaces them.
pub fn list(home: &Path, project: &Path) -> Vec<Value> {
    let mut skills = Vec::new();
    let mut add = |folder: &Path, source: &str, editable: bool| {
        for (name, path) in skill_files(folder) {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let path = path.display().to_string();
            skills.push(json!({ "name": name, "source": source, "path": path, "text": text, "editable": editable }));
        }
    };
    add(&home.join(".claude/skills"), "user", true);
    if project != home {
        add(&project.join(".claude/skills"), "project", true);
    }
    for (plugin, root) in plugins(&home.join(".claude/plugins/cache")) {
        add(&root.join("skills"), &format!("plugin {plugin}"), false);
    }
    skills
}

/// Each `<folder>/<name>/SKILL.md`, by name; a symlinked one is skipped, so nothing outside the
/// source's root is read.
fn skill_files(folder: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<_> = dirs(folder)
        .into_iter()
        .map(|dir| (name(&dir), dir.join("SKILL.md")))
        .filter(|(_, path)| fs::symlink_metadata(path).is_ok_and(|m| m.is_file()))
        .collect();
    found.sort();
    found
}

/// Each plugin in the cache (`<marketplace>/<plugin>/<version>`) at its newest version.
/// shortcut: the newest version by modification time, not `installed_plugins.json`'s, which is not
/// read; a disabled plugin is listed too. Upgrade if stale or disabled plugins confuse.
fn plugins(cache: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<_> = dirs(cache)
        .iter()
        .flat_map(|market| dirs(market))
        .filter_map(|plugin| {
            let newest = dirs(&plugin)
                .into_iter()
                .max_by_key(|v| v.metadata().and_then(|m| m.modified()).ok())?;
            Some((name(&plugin), newest))
        })
        .collect();
    found.sort();
    found
}

/// The folders in `folder`, never a symlink, which could lead out of it.
fn dirs(folder: &Path) -> Vec<PathBuf> {
    let entries = fs::read_dir(folder).into_iter().flatten().flatten();
    let real = |e: &fs::DirEntry| e.file_type().is_ok_and(|t| t.is_dir());
    entries.filter(real).map(|e| e.path()).collect()
}

fn name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// The skills, for the selected session's folder, else the configured repository.
#[tauri::command(async)]
pub fn skills(project: Option<String>) -> Result<Vec<Value>, String> {
    let home = std::env::home_dir().ok_or("no home folder")?;
    Ok(list(
        &home,
        Path::new(&project.unwrap_or_else(crate::terminal::repo)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(dir: &Path, text: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), text).unwrap();
    }

    #[test]
    fn lists_skills_from_all_three_sources_each_with_its_source_and_only_plugins_read_only() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        skill(&home.path().join(".claude/skills/mine"), "# Mine");
        skill(&project.path().join(".claude/skills/ours"), "# Ours");
        let plugin = home.path().join(".claude/plugins/cache/market/tools/1.0.0");
        skill(&plugin.join("skills/theirs"), "# Theirs");
        // Not a skill: a folder with no `SKILL.md`, and the cache's JSON, which is never read.
        fs::create_dir_all(home.path().join(".claude/skills/empty")).unwrap();
        fs::write(
            home.path().join(".claude/plugins/installed_plugins.json"),
            "{}",
        )
        .unwrap();

        let got: Vec<_> = list(home.path(), project.path())
            .iter()
            .map(|s| {
                (
                    s["name"].as_str().unwrap().to_owned(),
                    s["source"].as_str().unwrap().to_owned(),
                    s["editable"] == true,
                    s["text"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let want = [
            ("mine", "user", true, "# Mine"),
            ("ours", "project", true, "# Ours"),
            ("theirs", "plugin tools", false, "# Theirs"),
        ];
        assert_eq!(
            got,
            want.map(|(n, s, e, t)| (n.to_owned(), s.to_owned(), e, t.to_owned()))
        );
    }

    #[test]
    fn a_symlink_out_of_a_source_is_never_followed() {
        let home = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        skill(&outside.path().join("secret"), "# Secret");
        let skills = home.path().join(".claude/skills");
        fs::create_dir_all(skills.join("linked-file")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), skills.join("linked")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret/SKILL.md"),
            skills.join("linked-file/SKILL.md"),
        )
        .unwrap();
        assert!(list(home.path(), home.path()).is_empty());
    }

    #[test]
    fn a_plugin_shows_its_newest_version_only() {
        let home = tempfile::tempdir().unwrap();
        let tools = home.path().join(".claude/plugins/cache/market/tools");
        skill(&tools.join("1.0.0/skills/old"), "old");
        std::thread::sleep(std::time::Duration::from_millis(20));
        skill(&tools.join("2.0.0/skills/new"), "new");
        let names: Vec<_> = list(home.path(), home.path())
            .iter()
            .map(|s| s["name"].clone())
            .collect();
        assert_eq!(names, ["new"]);
    }
}
