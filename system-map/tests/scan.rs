//! The free scan over small repositories (spec #753, "The free scan"): what it proposes from folders and manifests.

use std::path::Path;

use serde_json::Value;
use studio_test_support::{git, repo};

use crate::common::write;

/// A repository tracking `files`, each with its text.
fn tracked(files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let (dir, root) = repo();
    for (file, text) in files {
        write(dir.path(), file, text);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "Files"]);
    (dir, root)
}

/// `n` files `<folder>/f<i>.ts`.
fn code(folder: &str, n: usize) -> Vec<(String, &'static str)> {
    (0..n)
        .map(|i| (format!("{folder}/f{i}.ts"), "x\n"))
        .collect()
}

fn paths_of(map: &Value) -> Vec<String> {
    map["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| part["paths"][0].as_str().unwrap().to_string())
        .collect()
}

fn strings(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}

/// Scans a repository of `files` and returns `(part paths, loose files, loose tests, candidates, unread, extractors)`.
struct Found {
    parts: Vec<String>,
    loose: Vec<String>,
    tests: Vec<String>,
    candidates: Vec<(String, bool)>,
    unread: Vec<String>,
    extractors: Vec<String>,
    ignored: Vec<String>,
}

fn found(files: &[(String, &str)]) -> Found {
    let files: Vec<(&str, &str)> = files.iter().map(|(f, t)| (f.as_str(), *t)).collect();
    let (_dir, root) = tracked(&files);
    let scan = system_map::scan(Path::new(&root)).unwrap();
    let strs = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    Found {
        parts: paths_of(&scan.map),
        loose: scan.loose_files,
        tests: scan.loose_tests,
        candidates: scan
            .ignore_candidates
            .into_iter()
            .map(|c| (c.path, c.ticked))
            .collect(),
        unread: scan.unread_languages,
        extractors: strs(&scan.map["extractors"]),
        ignored: strs(&scan.map["ignored"]),
    }
}

fn files(groups: &[Vec<(String, &'static str)>]) -> Vec<(String, &'static str)> {
    groups.iter().flatten().cloned().collect()
}

/// A case's name, files, part paths and loose files.
type Case = (
    &'static str,
    Vec<(String, &'static str)>,
    Vec<&'static str>,
    Vec<&'static str>,
);

#[test]
fn parts_come_from_the_folder_that_holds_most_of_the_code() {
    // (name, files, parts, loose files)
    let cases: Vec<Case> = vec![
        (
            "steps into src/ and reports its loose file",
            files(&[code("src/a", 2), code("src/b", 2), code("src", 1)]),
            vec!["src/a/", "src/b/"],
            vec!["src/f0.ts"],
        ),
        (
            "stops at the root when no folder holds 80%",
            files(&[code("a", 3), code("b", 2)]),
            vec!["a/", "b/"],
            vec![],
        ),
    ];
    for (name, tree, parts, loose) in cases {
        let f = found(&tree);
        assert_eq!(strings(&f.parts), parts, "{name}");
        assert_eq!(strings(&f.loose), loose, "{name}");
    }
}

#[test]
fn a_tracked_workspace_manifest_makes_each_member_a_part() {
    let members = files(&[
        code("crates/one", 1),
        code("crates/two", 1),
        code("tools", 1),
    ]);
    let with = |file: &'static str, text: &'static str| {
        let mut all = members.clone();
        all.push((file.to_string(), text));
        all
    };
    let cases = vec![
        (
            "cargo",
            with("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n"),
        ),
        (
            "npm",
            with("package.json", "{\"workspaces\": [\"crates/*\"]}"),
        ),
        (
            "npm packages object",
            with(
                "package.json",
                "{\"workspaces\": {\"packages\": [\"crates/*\"]}}",
            ),
        ),
        (
            "pnpm",
            with(
                "pnpm-workspace.yaml",
                "packages:\n  - 'crates/*'\n  - '!crates/two'\n",
            ),
        ),
        (
            "go",
            with(
                "go.work",
                "go 1.22\n\nuse (\n\t./crates/one\n\t./crates/two\n)\n",
            ),
        ),
    ];
    for (name, tree) in cases {
        let f = found(&tree);
        let expected: &[&str] = if name == "pnpm" {
            &["crates/one/", "tools/"]
        } else {
            &["crates/one/", "crates/two/", "tools/"]
        };
        assert_eq!(strings(&f.parts), expected, "{name}");
    }
}

#[test]
fn each_folder_holding_a_go_main_package_is_a_part() {
    let mut tree = vec![
        ("go.mod".to_string(), "module x\n"),
        ("cmd/api/main.go".to_string(), "package main\n"),
        ("cmd/worker/main.go".to_string(), "package main\n"),
        ("internal/db/db.go".to_string(), "package db\n"),
    ];
    tree.push(("README.md".to_string(), "x\n"));
    let f = found(&tree);
    assert_eq!(strings(&f.parts), ["cmd/api/", "cmd/worker/", "internal/"]);
}

#[test]
fn reference_copies_are_listed_and_ticked_only_when_sure() {
    let tree = files(&[
        code("src", 6),
        code("vendor", 1),
        code("third_party", 1),
        code("third-party", 1),
        code("external", 1),
        code("deps", 1),
        code("old-copy", 1),
        code("reference-core", 1),
        code("snapshots", 1),
    ]);
    let f = found(&tree);
    assert_eq!(
        f.candidates,
        vec![
            ("deps/".to_string(), false),
            ("external/".to_string(), false),
            ("old-copy/".to_string(), false),
            ("reference-core/".to_string(), false),
            ("snapshots/".to_string(), false),
            ("third-party/".to_string(), true),
            ("third_party/".to_string(), true),
            ("vendor/".to_string(), true),
        ]
    );
    assert_eq!(f.ignored, ["third-party/", "third_party/", "vendor/"]);
    assert!(!f.parts.iter().any(|p| p == "vendor/"), "{:?}", f.parts);
    assert!(f.parts.iter().any(|p| p == "external/"), "{:?}", f.parts);
}

#[test]
fn a_top_level_code_folder_nothing_builds_or_imports_is_listed_unticked() {
    let manifest = |dir: &str| (format!("{dir}/package.json"), "{}");
    let importer = |dir: &str, name: &str| (format!("{dir}/uses.ts"), name.to_string());
    // (name, files, candidates)
    type Row<'a> = (&'a str, Vec<(String, String)>, Vec<(&'a str, bool)>);
    let mut cases: Vec<Row> = Vec::new();
    let own = |tree: Vec<(String, &str)>| -> Vec<(String, String)> {
        tree.into_iter().map(|(f, t)| (f, t.to_string())).collect()
    };
    let mut chery = files(&[
        code("relevant-code-from-system/core", 4),
        code("apps/ui", 3),
    ]);
    chery.push(("pnpm-workspace.yaml".into(), "packages:\n  - apps/*\n"));
    chery.push(manifest("apps/ui"));
    cases.push((
        "chery-like: no manifest, nothing imports it",
        own(chery.clone()),
        vec![("relevant-code-from-system/", false)],
    ));
    let mut imported = own(chery.clone());
    imported.push(importer(
        "apps/ui",
        "import x from '../../relevant-code-from-system/core/f0'",
    ));
    cases.push(("another folder names it", imported, vec![]));
    let mut small = own(files(&[code("misc", 1), code("apps/ui", 9)]));
    small.push(("apps/ui/package.json".into(), "{}".into()));
    cases.push(("holds under a fifth of the code", small, vec![]));
    cases.push((
        "no manifest anywhere lists no folder",
        own(files(&[code("a", 4), code("b", 4)])),
        vec![],
    ));
    for (name, tree, expected) in cases {
        let tree: Vec<(&str, &str)> = tree.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
        let (_dir, root) = tracked(&tree);
        let scan = system_map::scan(Path::new(&root)).unwrap();
        let listed: Vec<(&str, bool)> = scan
            .ignore_candidates
            .iter()
            .map(|c| (c.path.as_str(), c.ticked))
            .collect();
        assert_eq!(listed, expected, "{name}");
        assert_eq!(scan.map["ignored"], serde_json::json!([]), "{name}");
    }
}

#[test]
fn a_chery_like_copy_folder_stays_a_part() {
    let mut tree = files(&[
        code("relevant-code-from-system/core", 4),
        code("apps/ui", 3),
    ]);
    tree.push(("pnpm-workspace.yaml".into(), "packages:\n  - apps/*\n"));
    let f = found(&tree);
    assert_eq!(
        strings(&f.parts),
        ["apps/ui/", "relevant-code-from-system/"]
    );
    assert!(f.ignored.is_empty(), "{:?}", f.ignored);
}

#[test]
fn observability_sdks_shape_has_five_loose_files_and_its_tests_apart() {
    let mut tree: Vec<(String, &str)> = [
        "src/index.ts",
        "src/logger.ts",
        "src/config.type.ts",
        "src/global-registry.ts",
        "src/node-version.ts",
        "src/example/e.ts",
        "src/fastify/f.ts",
        "src/fastify/g.ts",
        "src/__tests__/a.test.ts",
        "src/__tests__/b.test.ts",
        "scripts/check-node.cjs",
    ]
    .iter()
    .map(|f| (f.to_string(), "x\n"))
    .collect();
    tree.push(("package.json".into(), "{}"));
    let f = found(&tree);
    assert_eq!(strings(&f.parts), ["src/example/", "src/fastify/"]);
    assert_eq!(
        strings(&f.loose),
        [
            "scripts/check-node.cjs",
            "src/config.type.ts",
            "src/global-registry.ts",
            "src/index.ts",
            "src/logger.ts",
            "src/node-version.ts"
        ]
    );
    assert_eq!(
        strings(&f.tests),
        ["src/__tests__/a.test.ts", "src/__tests__/b.test.ts"]
    );
}

#[test]
fn more_than_fifteen_parts_fold_the_deepest_siblings_into_their_parent() {
    let mut tree = Vec::new();
    for i in 0..10 {
        tree.extend(code(&format!("x/a{i}"), 1));
        tree.extend(code(&format!("y/b{i}"), 2));
    }
    tree.push((
        "pnpm-workspace.yaml".into(),
        "packages:\n  - 'x/*'\n  - 'y/*'\n",
    ));
    let f = found(&tree);
    let mut expected = vec!["x/".to_string()];
    expected.extend((0..10).map(|i| format!("y/b{i}/")));
    assert_eq!(f.parts, expected);
}

#[test]
fn parts_with_the_same_last_folder_take_their_parents_into_the_id() {
    let mut tree = files(&[code("apps/web", 1), code("packages/web", 1)]);
    tree.push((
        "pnpm-workspace.yaml".into(),
        "packages:\n  - 'apps/*'\n  - 'packages/*'\n",
    ));
    let (_dir, root) = tracked(
        &tree
            .iter()
            .map(|(f, t)| (f.as_str(), *t))
            .collect::<Vec<_>>(),
    );
    let scan = system_map::scan(Path::new(&root)).unwrap();
    let ids: Vec<&str> = scan.map["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["apps-web", "packages-web"]);
}

#[test]
fn a_language_no_extractor_reads_is_detected_and_named() {
    let mut tree = files(&[code("src", 1)]);
    tree.push(("src/Main.kt".into(), "x\n"));
    let f = found(&tree);
    assert_eq!(f.unread, ["Kotlin"]);
    assert_eq!(f.extractors, ["typescript-imports"]);
}

#[test]
fn the_proposed_map_passes_the_check() {
    let tree = files(&[code("src/a", 2), code("src/b", 2)]);
    let files: Vec<(&str, &str)> = tree.iter().map(|(f, t)| (f.as_str(), *t)).collect();
    let (_dir, root) = tracked(&files);
    let scan = system_map::scan(Path::new(&root)).unwrap();
    let names = tree.iter().map(|(f, _)| f.clone()).collect();
    let report = system_map::check_text(Path::new(&root), names, &scan.map.to_string());
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(scan.map["parts"][0]["tier"], "interface");
}
