//! The skills browser (#528): every skill in Daniel's skills folder, the project's `.claude/skills`
//! and the installed plugins, with its `SKILL.md`. Besides `SKILL.md` files and folder lists, it reads
//! Claude Code's `installed_plugins.json` and only the `enabledPlugins` key of its settings files,
//! which may hold tokens (`apps/studio/AGENTS.md`).

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
    for (plugin, root) in plugins(home, project) {
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

/// Each enabled plugin in the cache (`<marketplace>/<plugin>/<version>`) at the version Claude Code
/// installed for `project`, from `installed_plugins.json`. If that file is missing or broken, each
/// plugin shows at its newest cached version. A plugin whose installed version is not cached, or
/// which is not installed here, is left out.
fn plugins(home: &Path, project: &Path) -> Vec<(String, PathBuf)> {
    static FALLBACK: std::sync::Once = std::sync::Once::new();
    let plugins = home.join(".claude/plugins");
    let installed = fs::read_to_string(plugins.join("installed_plugins.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|json| json.get("plugins").filter(|p| p.is_object()).cloned());
    if installed.is_none() {
        FALLBACK.call_once(|| {
            eprintln!("studio: no readable installed_plugins.json; skills show each plugin's newest cached version");
        });
    }
    let enabled = enabled(home, project);
    let mut found: Vec<_> = dirs(&plugins.join("cache"))
        .iter()
        .flat_map(|market| dirs(market).into_iter().map(move |p| (name(market), p)))
        .filter_map(|(market, plugin)| {
            let id = format!("{}@{market}", name(&plugin));
            if enabled.get(&id) == Some(&false) {
                return None;
            }
            let versions = dirs(&plugin);
            let version = match &installed {
                Some(installed) => {
                    let version = installed_version(installed.get(&id)?, project)?;
                    // Matched against the cached folders, so the JSON never names a path.
                    versions.into_iter().find(|v| name(v) == version)?
                }
                None => versions
                    .into_iter()
                    .max_by_key(|v| v.metadata().and_then(|m| m.modified()).ok())?,
            };
            Some((name(&plugin), version))
        })
        .collect();
    found.sort();
    found
}

/// The version of one `installed_plugins.json` entry that applies to `project`: an install for this
/// project, else the one for every project. Version 1 of the file has one entry, version 2 a list.
fn installed_version<'a>(entry: &'a Value, project: &Path) -> Option<&'a str> {
    let installs = entry
        .as_array()
        .map_or(std::slice::from_ref(entry), Vec::as_slice);
    let here = |i: &&Value| i["projectPath"].as_str().map(Path::new) == Some(project);
    let everywhere = |i: &&Value| i.get("projectPath").is_none();
    let install = installs
        .iter()
        .find(here)
        .or_else(|| installs.iter().find(everywhere))?;
    install["version"].as_str()
}

/// Each plugin turned on or off in Claude Code's user and project settings, the later file winning.
/// Only `enabledPlugins` is taken; the rest may hold tokens and is never kept or logged. An unreadable
/// file adds nothing, so with none every plugin shows.
fn enabled(home: &Path, project: &Path) -> std::collections::HashMap<String, bool> {
    let files = [
        home.join(".claude/settings.json"),
        project.join(".claude/settings.json"),
        project.join(".claude/settings.local.json"),
    ];
    let mut enabled = std::collections::HashMap::new();
    for file in files {
        let Some(json) = fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            continue;
        };
        let Some(plugins) = json.get("enabledPlugins").and_then(Value::as_object) else {
            continue;
        };
        for (id, on) in plugins {
            if let Some(on) = on.as_bool() {
                enabled.insert(id.clone(), on);
            }
        }
    }
    enabled
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
        // Not a skill: a folder with no `SKILL.md`.
        fs::create_dir_all(home.path().join(".claude/skills/empty")).unwrap();

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

    /// Two cached versions of `tools`, the newer one written last, and the names `list` returns.
    fn two_versions(home: &Path) -> Vec<Value> {
        let tools = home.join(".claude/plugins/cache/market/tools");
        skill(&tools.join("1.0.0/skills/old"), "old");
        std::thread::sleep(std::time::Duration::from_millis(20));
        skill(&tools.join("2.0.0/skills/new"), "new");
        list(home, home).iter().map(|s| s["name"].clone()).collect()
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_plugin_shows_the_version_claude_code_installed_not_the_newest_in_the_cache() {
        let home = tempfile::tempdir().unwrap();
        write(
            &home.path().join(".claude/plugins/installed_plugins.json"),
            r#"{"version": 2, "plugins": {"tools@market": [{"scope": "user", "version": "1.0.0"}]}}"#,
        );
        assert_eq!(two_versions(home.path()), ["old"]);
    }

    #[test]
    fn a_broken_installed_plugins_json_falls_back_to_the_newest_cached_version() {
        let home = tempfile::tempdir().unwrap();
        write(
            &home.path().join(".claude/plugins/installed_plugins.json"),
            "{ not json",
        );
        assert_eq!(two_versions(home.path()), ["new"]);
    }

    #[test]
    fn a_plugin_turned_off_in_claude_code_settings_is_left_out() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let cache = home.path().join(".claude/plugins/cache/market");
        skill(&cache.join("off/1.0.0/skills/hidden"), "");
        skill(&cache.join("back-on/1.0.0/skills/back"), "");
        skill(&cache.join("unlisted/1.0.0/skills/shown"), "");
        write(
            &home.path().join(".claude/settings.json"),
            r#"{"apiKeyHelper": "secret", "enabledPlugins": {"off@market": false, "back-on@market": false}}"#,
        );
        // The project's local settings win over the user's.
        write(
            &project.path().join(".claude/settings.local.json"),
            r#"{"enabledPlugins": {"back-on@market": true}}"#,
        );
        let names: Vec<_> = list(home.path(), project.path())
            .iter()
            .map(|s| s["name"].clone())
            .collect();
        assert_eq!(names, ["back", "shown"]);
    }

    #[test]
    fn with_no_installed_plugins_json_a_plugin_shows_its_newest_cached_version() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(two_versions(home.path()), ["new"]);
    }
}
