//! The free scan as the Map view shows it (spec #753, "The free scan", ticket #760): a git repository with no map gets the scan's
//! proposed parts as grey plates, the import links the readers compute between them, and the languages line. It is worked out in
//! memory on each ask (`system_map::scan`, then `system_map::check_text` on the skeleton), so nothing is written and no token is spent.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};

/// What the scan screen draws for one project (`MapScan` in `ui/types.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapScan {
    /// The skeleton: the scan's version 2 map, with the computed edges as `import` links, as text the page parses.
    pub map: String,
    /// The tracked files each part owns, by part id.
    pub files: BTreeMap<String, usize>,
    /// The languages of the code that a reader reads, so the links cover them.
    pub read_languages: Vec<String>,
    /// The languages of the code that no reader reads yet.
    pub unread_languages: Vec<String>,
    /// Whether `claude` was found, so Draft the map can start; else the copy-prompt leads.
    pub claude: bool,
    /// The folders that look like copies or vendored code, each ticked while the skeleton ignores it.
    pub ignore_candidates: Vec<system_map::IgnoreCandidate>,
    /// The code files no part owns, tests apart.
    pub loose_files: Vec<String>,
    /// The tests no part owns.
    pub loose_tests: Vec<String>,
    /// The subfolders each part that Split by subfolders can split would split into, by part id.
    pub splits: BTreeMap<String, Vec<String>>,
}

/// One change of the skeleton on the scan screen (`ScanEdit` in `ui/types.ts`; #753, stories 7-9).
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Edit {
    /// Ignore `folder` (a reference-copy candidate), or stop ignoring it.
    Ignore { folder: String, ignored: bool },
    /// Give the loose `files` to the part `part`, or to a new part when it is none.
    Home {
        files: Vec<String>,
        part: Option<String>,
    },
    /// Merge the part `part` into the part `into`.
    Merge { part: String, into: String },
    /// Split the part `part` by its subfolders.
    Split { part: String },
}

/// `map_scan_edit`: the skeleton `map` the page holds with `edit` made, and the scan screen for it, its lines computed again.
pub fn map_scan_edit(project: String, map: String, edit: Edit) -> Result<MapScan, String> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let claude = studio_understanding::find(shell.as_ref(), "claude").is_some();
    edit_of(Path::new(&project), &map, edit, claude)
}

/// The skeleton `map` of the main checkout of `repo` with `edit` made, and its scan screen. The parts' paths stay apart (none inside
/// another), so which part owns a file never hangs on the parts' order: Ignore drops the paths inside the folder, a loose folder is
/// given whole only when no part's path is inside it, and Split leaves the folder's other files to the part by their own paths.
fn edit_of(repo: &Path, map: &str, edit: Edit, claude: bool) -> Result<MapScan, String> {
    let main = studio_workbench::main_checkout(repo)?;
    let root = Path::new(&main);
    let found = system_map::scan(root)?;
    let files = system_map::tracked_files(root)?;
    let mut map: Value =
        serde_json::from_str(map).map_err(|e| format!("the skeleton does not read: {e}"))?;
    if !map["parts"].is_array() {
        return Err("the skeleton has no parts".into());
    }
    match edit {
        Edit::Ignore { folder, ignored } => {
            if !found.ignore_candidates.iter().any(|c| c.path == folder) {
                return Err(format!("{folder} is not a folder the scan asks about"));
            }
            ignore(&mut map, &folder, ignored, &files);
        }
        Edit::Home { files: loose, part } => home(&mut map, &loose, part.as_deref(), &files)?,
        Edit::Merge { part, into } => {
            if part == into || index(&map, &into).is_none() {
                return Err(format!("{part} cannot merge into {into}"));
            }
            let at = index(&map, &part).ok_or_else(|| format!("no part {part}"))?;
            let gone = parts_mut(&mut map).remove(at);
            let to = index(&map, &into).unwrap_or_default();
            let paths = parts_mut(&mut map)[to]["paths"]
                .as_array_mut()
                .ok_or("a part has no paths")?;
            paths.extend(gone["paths"].as_array().cloned().unwrap_or_default());
        }
        Edit::Split { part } => {
            let at = index(&map, &part).ok_or_else(|| format!("no part {part}"))?;
            let (keep, folders) = split_of(&map, &part, &files)
                .ok_or_else(|| format!("{part} has no subfolders to split by"))?;
            let mut new: Vec<Value> = Vec::new();
            for folder in &folders {
                let id = fresh_id(&map, &new, folder);
                new.push(skeleton_part(
                    &id,
                    &system_map::folder_title(folder),
                    vec![folder.clone()],
                ));
            }
            let parts = parts_mut(&mut map);
            let after = if keep.is_empty() {
                parts.remove(at);
                at
            } else {
                parts[at]["paths"] = keep.into();
                at + 1
            };
            parts.splice(after..after, new);
        }
    }
    Ok(screen_of(root, map, found, files, claude))
}

/// Whether the path `pattern` of a skeleton (a folder ending in a slash, or a file) covers `file`.
fn covers(pattern: &str, file: &str) -> bool {
    if pattern.ends_with('/') {
        file.starts_with(pattern)
    } else {
        file == pattern
    }
}

fn strings(value: &Value) -> impl Iterator<Item = &str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

fn parts_of(map: &Value) -> &[Value] {
    map["parts"].as_array().map_or(&[], Vec::as_slice)
}

fn parts_mut(map: &mut Value) -> &mut Vec<Value> {
    map["parts"]
        .as_array_mut()
        .expect("the skeleton's parts were checked")
}

fn index(map: &Value, id: &str) -> Option<usize> {
    parts_of(map).iter().position(|part| part["id"] == id)
}

fn ignored(map: &Value, file: &str) -> bool {
    strings(&map["ignored"]).any(|pattern| covers(pattern, file))
}

/// The id of the part that owns `file`, by `check`'s rule: none when it is ignored, else the last part with a path covering it.
fn owner<'a>(map: &'a Value, file: &str) -> Option<&'a str> {
    if ignored(map, file) {
        return None;
    }
    let part = parts_of(map)
        .iter()
        .rev()
        .find(|part| strings(&part["paths"]).any(|path| covers(path, file)))?;
    part["id"].as_str()
}

/// A part of the skeleton as the scan writes one, with its placeholder words.
fn skeleton_part(id: &str, name: &str, paths: Vec<String>) -> Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "caption": "Not yet described",
        "purpose": "Not yet described.",
        "holds": [],
        "tier": "interface",
        "paths": paths,
    })
}

/// An id for a new part at `path`, as the scan gives one: its last folder's words, with its parents' added while the id is taken by a
/// part of `map` or of `new`, then a number.
fn fresh_id(map: &Value, new: &[Value], path: &str) -> String {
    let taken = |id: &str| parts_of(map).iter().chain(new).any(|part| part["id"] == id);
    let words: Vec<String> = path
        .trim_end_matches('/')
        .split('/')
        .map(|segment| {
            let slug: String = segment
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            let slug = slug
                .split('-')
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            if slug.is_empty() { "part".into() } else { slug }
        })
        .collect();
    (1..=words.len())
        .map(|take| words[words.len() - take..].join("-"))
        .chain((2..).map(|n| format!("{}-{n}", words.join("-"))))
        .find(|id| !taken(id))
        .unwrap_or_default()
}

/// Ignores `folder` and drops the paths inside it, a part left with none going; or stops ignoring it, and a folder no part covers that
/// holds code becomes a part of its own.
fn ignore(map: &mut Value, folder: &str, on: bool, files: &[String]) {
    let mut list: Vec<String> = strings(&map["ignored"])
        .filter(|p| *p != folder)
        .map(String::from)
        .collect();
    if on {
        list.push(folder.to_string());
        list.sort();
        map["ignored"] = list.into();
        for part in parts_mut(map).iter_mut() {
            let kept: Vec<String> = strings(&part["paths"])
                .filter(|path| !path.starts_with(folder))
                .map(String::from)
                .collect();
            part["paths"] = kept.into();
        }
        parts_mut(map).retain(|part| {
            part["paths"]
                .as_array()
                .is_some_and(|paths| !paths.is_empty())
        });
        return;
    }
    map["ignored"] = list.into();
    let covered = parts_of(map)
        .iter()
        .any(|part| strings(&part["paths"]).any(|path| covers(path, folder)));
    let code = files
        .iter()
        .any(|file| file.starts_with(folder) && system_map::is_code(file) && !ignored(map, file));
    if !covered && code {
        let id = fresh_id(map, &[], folder);
        parts_mut(map).push(skeleton_part(
            &id,
            &system_map::folder_title(folder),
            vec![folder.to_string()],
        ));
    }
}

/// Gives the loose files `loose` to the part `to`, or to a new part when it is none: their folder whole when they share one that no
/// part's path is inside, else each by its own path.
fn home(
    map: &mut Value,
    loose: &[String],
    to: Option<&str>,
    files: &[String],
) -> Result<(), String> {
    if loose.is_empty() {
        return Err("no loose files to give".into());
    }
    for file in loose {
        if !files.contains(file)
            || !system_map::is_code(file)
            || ignored(map, file)
            || owner(map, file).is_some()
        {
            return Err(format!("{file} is not a loose file"));
        }
    }
    let parent = |file: &str| {
        file.rsplit_once('/')
            .map_or(String::new(), |(dir, _)| format!("{dir}/"))
    };
    let folder = parent(&loose[0]);
    let shared = !folder.is_empty() && loose.iter().all(|file| parent(file) == folder);
    let whole = shared
        && !parts_of(map)
            .iter()
            .any(|part| strings(&part["paths"]).any(|path| path.starts_with(&folder)));
    let paths: Vec<String> = if whole {
        vec![folder.clone()]
    } else {
        loose.to_vec()
    };
    match to {
        Some(id) => {
            let at = index(map, id).ok_or_else(|| format!("no part {id}"))?;
            let list = parts_mut(map)[at]["paths"]
                .as_array_mut()
                .ok_or("a part has no paths")?;
            list.extend(paths.into_iter().map(Value::from));
        }
        None => {
            let (at, name) = if shared {
                (folder.as_str(), system_map::folder_title(&folder))
            } else {
                ("root", "Root".to_string())
            };
            let id = fresh_id(map, &[], at);
            parts_mut(map).push(skeleton_part(&id, &name, paths));
        }
    }
    Ok(())
}

/// How the part `id` splits by subfolders, when it does: the paths it keeps (its file paths, and of each of its folders the files right
/// in it and the subfolders with no code) and the subfolders of code that become parts. None unless that leaves two parts owning code.
fn split_of(map: &Value, id: &str, files: &[String]) -> Option<(Vec<String>, Vec<String>)> {
    let part = &parts_of(map)[index(map, id)?];
    let owned: Vec<&String> = files
        .iter()
        .filter(|file| owner(map, file) == Some(id))
        .collect();
    let (mut keep, mut folders) = (Vec::new(), Vec::new());
    for path in strings(&part["paths"]) {
        if !path.ends_with('/') {
            keep.push(path.to_string());
            continue;
        }
        let mut children: BTreeMap<String, bool> = BTreeMap::new();
        for file in owned.iter().filter(|file| file.starts_with(path)) {
            match file[path.len()..].split_once('/') {
                Some((child, _)) => {
                    *children.entry(format!("{path}{child}/")).or_default() |=
                        system_map::is_code(file);
                }
                None => keep.push(file.to_string()),
            }
        }
        for (child, code) in children {
            if code {
                folders.push(child);
            } else {
                keep.push(child);
            }
        }
    }
    let keeps_code = owned
        .iter()
        .any(|file| system_map::is_code(file) && keep.iter().any(|path| covers(path, file)));
    (folders.len() + usize::from(keeps_code) >= 2).then_some((keep, folders))
}

/// `map_scan`: the scan of `project`'s main checkout, and whether the runner would find `claude`.
pub fn map_scan(project: String) -> Result<MapScan, String> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let claude = studio_understanding::find(shell.as_ref(), "claude").is_some();
    scan_of(Path::new(&project), claude)
}

/// The scan of the main checkout of `repo`, with `claude` as found.
pub(crate) fn scan_of(repo: &Path, claude: bool) -> Result<MapScan, String> {
    let main = studio_workbench::main_checkout(repo)?;
    let root = Path::new(&main);
    let found = system_map::scan(root)?;
    let files = system_map::tracked_files(root)?;
    let map = found.map.clone();
    Ok(screen_of(root, map, found, files, claude))
}

/// The edges `report` computed, as `import` links: one per pair of parts in the edge's direction, `<from>-<to>`, with no reason, which an
/// `import` link needs none of (`system-map/src/edges.rs`).
pub(crate) fn import_links(report: &system_map::Report) -> Value {
    report
        .edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "id": format!("{}-{}", edge.from, edge.to),
                "from": edge.from,
                "to": edge.to,
                "kind": "import",
            })
        })
        .collect()
}

/// The scan screen of the skeleton `map`: it gets the edges `check_text` computes on it as `import` links, one per pair of parts in the
/// edge's direction, with no reason, as Accept writes them (#753, "The map written"); its loose files, ticks and splits are its own.
fn screen_of(
    root: &Path,
    mut map: Value,
    found: system_map::Scan,
    files: Vec<String>,
    claude: bool,
) -> MapScan {
    let report = system_map::check_text(root, files.clone(), &map.to_string());
    map["links"] = import_links(&report);
    let loose = |tests: bool| -> Vec<String> {
        let free = |file: &&String| {
            system_map::is_code(file)
                && system_map::is_test(file) == tests
                && !ignored(&map, file)
                && owner(&map, file).is_none()
        };
        files.iter().filter(free).cloned().collect()
    };
    let ids = parts_of(&map).iter().filter_map(|part| part["id"].as_str());
    let splits = ids
        .filter_map(|id| split_of(&map, id, &files).map(|(_, folders)| (id.to_string(), folders)))
        .collect();
    let ignore_candidates = found
        .ignore_candidates
        .into_iter()
        .map(|c| system_map::IgnoreCandidate {
            ticked: strings(&map["ignored"]).any(|p| p == c.path),
            path: c.path,
        })
        .collect();
    MapScan {
        files: report
            .parts
            .into_iter()
            .map(|part| (part.id, part.files.len()))
            .collect(),
        read_languages: found.read_languages,
        unread_languages: found.unread_languages,
        claude,
        ignore_candidates,
        loose_files: loose(false),
        loose_tests: loose(true),
        splits,
        map: map.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_test_support::{git, repo};

    fn write(root: &Path, file: &str, text: &str) {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn the_scan_screen_gets_the_parts_their_file_counts_the_computed_links_and_the_languages() {
        let (dir, _main) = repo();
        write(
            dir.path(),
            "src/app/main.ts",
            "import { b } from \"../lib/b\";\n",
        );
        write(dir.path(), "src/app/more.ts", "export const x = 1;\n");
        write(dir.path(), "src/lib/b.ts", "export const b = 1;\n");
        write(dir.path(), "src/lib/tool.rb", "puts 1\n");
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "-m", "Code"]);
        let scan = scan_of(dir.path(), true).unwrap();
        let map: serde_json::Value = serde_json::from_str(&scan.map).unwrap();
        let paths: Vec<&str> = map["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|part| part["paths"][0].as_str().unwrap())
            .collect();
        assert_eq!(paths, ["src/app/", "src/lib/"]);
        assert_eq!(
            scan.files,
            BTreeMap::from([("app".to_string(), 2), ("lib".to_string(), 2)])
        );
        assert_eq!(
            map["links"],
            serde_json::json!([{ "id": "app-lib", "from": "app", "to": "lib", "kind": "import" }])
        );
        assert_eq!(scan.read_languages, ["TypeScript"]);
        assert_eq!(scan.unread_languages, ["Ruby"]);
        assert!(scan.claude);
    }

    /// The skeleton's parts as (id, paths), its links as (from, to), the loose files and tests, the ticked candidates and the splits.
    type Seen = (
        Vec<(String, Vec<String>)>,
        Vec<(String, String)>,
        Vec<String>,
        Vec<String>,
        Vec<String>,
        BTreeMap<String, Vec<String>>,
    );
    fn seen(scan: &MapScan) -> Seen {
        let map: serde_json::Value = serde_json::from_str(&scan.map).unwrap();
        let strings = |value: &serde_json::Value| -> Vec<String> {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        };
        let parts = map["parts"].as_array().unwrap().iter();
        let ends = |link: &serde_json::Value| {
            (
                link["from"].as_str().unwrap().to_string(),
                link["to"].as_str().unwrap().to_string(),
            )
        };
        (
            parts
                .map(|part| {
                    (
                        part["id"].as_str().unwrap().to_string(),
                        strings(&part["paths"]),
                    )
                })
                .collect(),
            map["links"].as_array().unwrap().iter().map(ends).collect(),
            scan.loose_files.clone(),
            scan.loose_tests.clone(),
            scan.ignore_candidates
                .iter()
                .filter(|c| c.ticked)
                .map(|c| c.path.clone())
                .collect(),
            scan.splits.clone(),
        )
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "a table test: the fixture repository, then one row an edit with all it changes"
    )]
    fn each_edit_of_the_skeleton_changes_its_parts_and_the_lines_are_computed_again() {
        let (dir, _main) = repo();
        let code = [
            (
                "src/app/main.ts",
                "import { b } from \"../lib/b\";\nimport { c } from \"../lib/core/c\";\n",
            ),
            ("src/app/more.ts", "export const x = 1;\n"),
            ("src/lib/b.ts", "export const b = 1;\n"),
            ("src/lib/core/c.ts", "export const c = 1;\n"),
            ("src/lib/edge/e.ts", "export const e = 1;\n"),
            (
                "src/snapshot-copy/s.ts",
                "import { b } from \"../lib/b\";\n",
            ),
            ("src/index.ts", "export const i = 1;\n"),
            ("tests/b.test.ts", "import { b } from \"../src/lib/b\";\n"),
            ("vendor/x/v.ts", "import { b } from \"../../src/lib/b\";\n"),
        ];
        for (file, text) in code {
            write(dir.path(), file, text);
        }
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "-m", "Code"]);
        let scan = scan_of(dir.path(), true).unwrap();
        let s = |items: &[&str]| -> Vec<String> { items.iter().map(ToString::to_string).collect() };
        let part = |id: &str, paths: &[&str]| (id.to_string(), s(paths));
        let link = |from: &str, to: &str| (from.to_string(), to.to_string());
        let lib_splits =
            BTreeMap::from([("lib".to_string(), s(&["src/lib/core/", "src/lib/edge/"]))]);
        let (app, lib, copy) = (
            part("app", &["src/app/"]),
            part("lib", &["src/lib/"]),
            part("snapshot-copy", &["src/snapshot-copy/"]),
        );
        let first_links = vec![link("app", "lib"), link("snapshot-copy", "lib")];
        let cases: Vec<(&str, Option<Edit>, Seen)> = vec![
            (
                "the scan as it came: a sure candidate ticked, a loose file and a loose test, lib splittable",
                None,
                (
                    vec![app.clone(), lib.clone(), copy.clone()],
                    first_links.clone(),
                    s(&["src/index.ts"]),
                    s(&["tests/b.test.ts"]),
                    s(&["vendor/"]),
                    lib_splits.clone(),
                ),
            ),
            (
                "ignoring a candidate drops its part and its line",
                Some(Edit::Ignore {
                    folder: "src/snapshot-copy/".into(),
                    ignored: true,
                }),
                (
                    vec![app.clone(), lib.clone()],
                    vec![link("app", "lib")],
                    s(&["src/index.ts"]),
                    s(&["tests/b.test.ts"]),
                    s(&["src/snapshot-copy/", "vendor/"]),
                    lib_splits.clone(),
                ),
            ),
            (
                "no longer ignoring a candidate makes it a part, with its lines",
                Some(Edit::Ignore {
                    folder: "vendor/".into(),
                    ignored: false,
                }),
                (
                    vec![
                        app.clone(),
                        lib.clone(),
                        copy.clone(),
                        part("vendor", &["vendor/"]),
                    ],
                    vec![
                        link("app", "lib"),
                        link("snapshot-copy", "lib"),
                        link("vendor", "lib"),
                    ],
                    s(&["src/index.ts"]),
                    s(&["tests/b.test.ts"]),
                    s(&[]),
                    lib_splits.clone(),
                ),
            ),
            (
                "a loose file given to a part joins it by name, since its folder holds other parts",
                Some(Edit::Home {
                    files: s(&["src/index.ts"]),
                    part: Some("app".into()),
                }),
                (
                    vec![
                        part("app", &["src/app/", "src/index.ts"]),
                        lib.clone(),
                        copy.clone(),
                    ],
                    first_links.clone(),
                    s(&[]),
                    s(&["tests/b.test.ts"]),
                    s(&["vendor/"]),
                    lib_splits.clone(),
                ),
            ),
            (
                "a loose test given a new part makes one of its folder; a test's imports draw no line",
                Some(Edit::Home {
                    files: s(&["tests/b.test.ts"]),
                    part: None,
                }),
                (
                    vec![
                        app.clone(),
                        lib.clone(),
                        copy.clone(),
                        part("tests", &["tests/"]),
                    ],
                    first_links.clone(),
                    s(&["src/index.ts"]),
                    s(&[]),
                    s(&["vendor/"]),
                    lib_splits.clone(),
                ),
            ),
            (
                "a merge leaves one part with both folders, and the line between them goes",
                Some(Edit::Merge {
                    part: "app".into(),
                    into: "lib".into(),
                }),
                (
                    vec![part("lib", &["src/lib/", "src/app/"]), copy.clone()],
                    vec![link("snapshot-copy", "lib")],
                    s(&["src/index.ts"]),
                    s(&["tests/b.test.ts"]),
                    s(&["vendor/"]),
                    lib_splits.clone(),
                ),
            ),
            (
                "a split makes a part of each subfolder of code; the folder's own files stay, and the lines follow",
                Some(Edit::Split { part: "lib".into() }),
                (
                    vec![
                        app.clone(),
                        part("lib", &["src/lib/b.ts"]),
                        part("core", &["src/lib/core/"]),
                        part("edge", &["src/lib/edge/"]),
                        copy.clone(),
                    ],
                    vec![
                        link("app", "core"),
                        link("app", "lib"),
                        link("snapshot-copy", "lib"),
                    ],
                    s(&["src/index.ts"]),
                    s(&["tests/b.test.ts"]),
                    s(&["vendor/"]),
                    BTreeMap::new(),
                ),
            ),
        ];
        for (name, edit, want) in cases {
            let got = match edit {
                None => scan.clone(),
                Some(edit) => edit_of(dir.path(), &scan.map, edit, true).unwrap(),
            };
            assert_eq!(seen(&got), want, "{name}");
        }
    }

    #[test]
    fn an_edit_of_what_the_skeleton_does_not_offer_is_refused() {
        let (dir, _main) = repo();
        write(dir.path(), "src/app/main.ts", "export const a = 1;\n");
        write(dir.path(), "src/lib/b.ts", "export const b = 1;\n");
        git(dir.path(), &["add", "-A"]);
        git(dir.path(), &["commit", "-q", "-m", "Code"]);
        let scan = scan_of(dir.path(), true).unwrap();
        for edit in [
            Edit::Merge {
                part: "app".into(),
                into: "nothing".into(),
            },
            Edit::Merge {
                part: "app".into(),
                into: "app".into(),
            },
            Edit::Split {
                part: "nothing".into(),
            },
            Edit::Home {
                files: vec!["src/app/main.ts".into()],
                part: Some("lib".into()),
            },
            Edit::Ignore {
                folder: "src/app/".into(),
                ignored: true,
            },
        ] {
            let name = format!("{edit:?}");
            assert!(
                edit_of(dir.path(), &scan.map, edit, true).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn a_folder_in_no_git_repository_has_no_scan() {
        let dir = tempfile::tempdir().unwrap();
        let refused = scan_of(dir.path(), true).err().unwrap_or_default();
        assert!(refused.contains("not in a git repository"), "{refused:?}");
    }
}
