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

/// What a scan of a repository of `files` found: part paths, loose files and tests, candidates, read and unread languages, extractors and ignored folders.
struct Found {
    parts: Vec<String>,
    loose: Vec<String>,
    tests: Vec<String>,
    candidates: Vec<(String, bool)>,
    read: Vec<String>,
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
        read: scan.read_languages,
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
        (
            "files that are not code, hidden ones too, are loose when no part holds them",
            files(&[
                code("src/a", 2),
                code("src/b", 2),
                vec![
                    ("src/a/notes.md".to_string(), "x\n"),
                    ("docs/guide.md".to_string(), "x\n"),
                    (".claude/settings.json".to_string(), "{}\n"),
                    ("README.md".to_string(), "x\n"),
                ],
            ]),
            vec!["src/a/", "src/b/"],
            vec![".claude/settings.json", "README.md", "docs/guide.md"],
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
    let with = |file: &'static str, text: &'static str, more: &[&str]| {
        let mut all = members.clone();
        all.push((file.to_string(), text));
        for folder in more {
            all.extend(code(folder, 1));
        }
        all
    };
    let all: &[&str] = &["crates/one/", "crates/two/", "tools/"];
    /// A case: its name, the files, and the parts' paths.
    type Case<'a> = (&'a str, Vec<(String, &'a str)>, &'a [&'a str]);
    let cases: Vec<Case> = vec![
        (
            "cargo",
            with("Cargo.toml", "[workspace]\nmembers = [\"crates/*\"]\n", &[]),
            all,
        ),
        (
            "npm",
            with("package.json", "{\"workspaces\": [\"crates/*\"]}", &[]),
            all,
        ),
        (
            "npm packages object",
            with(
                "package.json",
                "{\"workspaces\": {\"packages\": [\"crates/*\"]}}",
                &[],
            ),
            all,
        ),
        (
            "pnpm",
            with(
                "pnpm-workspace.yaml",
                "packages:\n  - 'crates/*'\n  - '!crates/two'\n",
                &[],
            ),
            &["crates/one/", "tools/"],
        ),
        (
            "go",
            with(
                "go.work",
                "go 1.22\n\nuse (\n\t./crates/one\n\t./crates/two\n)\n",
                &[],
            ),
            all,
        ),
        (
            "a member named in letters past ASCII",
            with(
                "package.json",
                "{\"workspaces\": [\"crates/*\"]}",
                &["crates/café"],
            ),
            &["crates/café/", "crates/one/", "crates/two/", "tools/"],
        ),
        (
            "a pattern with two stars",
            with(
                "package.json",
                "{\"workspaces\": [\"crates/*o*\"]}",
                &["crates/o"],
            ),
            &["crates/o/", "crates/one/", "crates/two/", "tools/"],
        ),
    ];
    for (name, tree, expected) in cases {
        let f = found(&tree);
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
fn reference_copies_are_listed_ticked_only_when_sure_and_only_a_ticked_one_is_ignored() {
    let manifest = |dir: &str| (format!("{dir}/package.json"), "{}");
    let importer = |dir: &str, name: &str| (format!("{dir}/uses.ts"), name.to_string());
    /// A case: its name, the files, the candidates as listed and ticked, the folders ignored, and folders that are parts and that
    /// are not.
    struct Case<'a> {
        name: &'a str,
        files: Vec<(String, String)>,
        candidates: Vec<(&'a str, bool)>,
        ignored: Vec<&'a str>,
        parts: Vec<&'a str>,
        not_parts: Vec<&'a str>,
    }
    let own = |tree: Vec<(String, &str)>| -> Vec<(String, String)> {
        tree.into_iter().map(|(f, t)| (f, t.to_string())).collect()
    };
    let mut cases: Vec<Case> = vec![Case {
        name: "the names a copy goes by",
        files: own(files(&[
            code("src", 6),
            code("vendor", 1),
            code("third_party", 1),
            code("third-party", 1),
            code("external", 1),
            code("deps", 1),
            code("old-copy", 1),
            code("reference-core", 1),
            code("snapshots", 1),
        ])),
        candidates: vec![
            ("deps/", false),
            ("external/", false),
            ("old-copy/", false),
            ("reference-core/", false),
            ("snapshots/", false),
            ("third-party/", true),
            ("third_party/", true),
            ("vendor/", true),
        ],
        ignored: vec!["third-party/", "third_party/", "vendor/"],
        parts: vec!["external/"],
        not_parts: vec!["vendor/"],
    }];
    // chery-platform-personal's shape, a repository of the 2026-11-27 trial (#753): a top-level copy of another system's code that no
    // manifest builds and nothing imports, asked about unticked, and a part until it is ignored.
    let mut chery = files(&[
        code("relevant-code-from-system/core", 4),
        code("apps/ui", 3),
    ]);
    chery.push(("pnpm-workspace.yaml".into(), "packages:\n  - apps/*\n"));
    chery.push(manifest("apps/ui"));
    cases.push(Case {
        name: "chery-like: no manifest, nothing imports it",
        files: own(chery.clone()),
        candidates: vec![("relevant-code-from-system/", false)],
        ignored: vec![],
        parts: vec!["apps/ui/", "relevant-code-from-system/"],
        not_parts: vec![],
    });
    let mut imported = own(chery.clone());
    imported.push(importer(
        "apps/ui",
        "import x from '../../relevant-code-from-system/core/f0'",
    ));
    cases.push(Case {
        name: "another folder names it",
        files: imported,
        candidates: vec![],
        ignored: vec![],
        parts: vec![],
        not_parts: vec![],
    });
    let mut small = own(files(&[code("misc", 1), code("apps/ui", 9)]));
    small.push(("apps/ui/package.json".into(), "{}".into()));
    cases.push(Case {
        name: "holds under a fifth of the code",
        files: small,
        candidates: vec![],
        ignored: vec![],
        parts: vec![],
        not_parts: vec![],
    });
    cases.push(Case {
        name: "no manifest anywhere lists no folder",
        files: own(files(&[code("a", 4), code("b", 4)])),
        candidates: vec![],
        ignored: vec![],
        parts: vec![],
        not_parts: vec![],
    });
    for case in cases {
        let name = case.name;
        let tree: Vec<(String, &str)> = case
            .files
            .iter()
            .map(|(f, t)| (f.clone(), t.as_str()))
            .collect();
        let f = found(&tree);
        let listed: Vec<(&str, bool)> =
            f.candidates.iter().map(|(p, t)| (p.as_str(), *t)).collect();
        assert_eq!(listed, case.candidates, "{name}");
        assert_eq!(strings(&f.ignored), case.ignored, "{name}");
        for part in case.parts {
            assert!(
                f.parts.iter().any(|p| p == part),
                "{name}: {part} in {:?}",
                f.parts
            );
        }
        for part in case.not_parts {
            assert!(
                !f.parts.iter().any(|p| p == part),
                "{name}: {part} in {:?}",
                f.parts
            );
        }
    }
}

#[test]
fn observability_sdks_shape_keeps_its_loose_files_and_its_tests_apart() {
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
            "package.json",
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
    tree.push(("src/main.go".into(), "package main\n"));
    let f = found(&tree);
    assert_eq!(f.read, ["Go", "TypeScript"]);
    assert_eq!(f.unread, ["Kotlin"]);
    assert_eq!(f.extractors, ["typescript-imports"]);
}

#[test]
fn the_proposed_map_fails_the_check_only_on_the_loose_files_and_tests() {
    /// A case: its name, the files, and the files the check finds no part for.
    type Case = (&'static str, Vec<(String, &'static str)>, Vec<&'static str>);
    let cases: Vec<Case> = vec![
        (
            "code only: it passes",
            files(&[code("src/a", 2), code("src/b", 2)]),
            vec![],
        ),
        (
            "files that are not code, outside every part",
            files(&[
                code("src/a", 2),
                code("src/b", 2),
                vec![
                    ("docs/guide.md".to_string(), "x\n"),
                    (".changeset/x.md".to_string(), "x\n"),
                    ("tests/data.json".to_string(), "{}\n"),
                ],
            ]),
            vec![".changeset/x.md", "docs/guide.md", "tests/data.json"],
        ),
    ];
    for (name, tree, unowned) in cases {
        let listed: Vec<(&str, &str)> = tree.iter().map(|(f, t)| (f.as_str(), *t)).collect();
        let (_dir, root) = tracked(&listed);
        let scan = system_map::scan(Path::new(&root)).unwrap();
        let names = tree.iter().map(|(f, _)| f.clone()).collect();
        let report = system_map::check_text(Path::new(&root), names, &scan.map.to_string());
        let mut failed: Vec<&str> = report
            .problems
            .iter()
            .map(|p| {
                p.split_once(": tracked, but no part owns it")
                    .map_or(p.as_str(), |(f, _)| f)
            })
            .collect();
        failed.sort_unstable();
        assert_eq!(failed, unowned, "{name}: {:?}", report.problems);
        let mut loose: Vec<&str> = strings(&scan.loose_files);
        loose.extend(strings(&scan.loose_tests));
        loose.sort_unstable();
        assert_eq!(loose, unowned, "{name}: the scan lists them as loose");
        assert_eq!(scan.map["parts"][0]["tier"], "interface", "{name}");
    }
}
