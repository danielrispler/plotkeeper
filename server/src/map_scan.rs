//! The free scan as the Map view shows it (spec #753, "The free scan", ticket #760): a git repository with no map gets the scan's
//! proposed parts as grey plates, the import links the readers compute between them, and the languages line. It is worked out in
//! memory on each ask (`system_map::scan`, then `system_map::check_text` on the skeleton), so nothing is written and no token is spent.

use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

/// What the scan screen draws for one project (`MapScan` in `ui/types.ts`).
#[derive(Debug, PartialEq, Serialize)]
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
}

/// `map_scan`: the scan of `project`'s main checkout, and whether the runner would find `claude`.
pub fn map_scan(project: String) -> Result<MapScan, String> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    let claude = studio_understanding::find(shell.as_ref(), "claude").is_some();
    scan_of(Path::new(&project), claude)
}

/// The scan of the main checkout of `repo`, with `claude` as found: the skeleton gets the edges `check_text` computes on it as
/// `import` links, one per pair of parts in the edge's direction, with no reason, as Accept writes them (#753, "The map written").
fn scan_of(repo: &Path, claude: bool) -> Result<MapScan, String> {
    let main = studio_workbench::main_checkout(repo)?;
    let root = Path::new(&main);
    let found = system_map::scan(root)?;
    let mut map = found.map;
    let files = system_map::tracked_files(root)?;
    let report = system_map::check_text(root, files, &map.to_string());
    map["links"] = report
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
        .collect();
    Ok(MapScan {
        map: map.to_string(),
        files: report
            .parts
            .into_iter()
            .map(|part| (part.id, part.files.len()))
            .collect(),
        read_languages: found.read_languages,
        unread_languages: found.unread_languages,
        claude,
    })
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

    #[test]
    fn a_folder_in_no_git_repository_has_no_scan() {
        let dir = tempfile::tempdir().unwrap();
        let refused = scan_of(dir.path(), true).err().unwrap_or_default();
        assert!(refused.contains("not in a git repository"), "{refused:?}");
    }
}
