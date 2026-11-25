//! `go-imports`: Go repositories in temporary folders, read through `check` and `imports` (spec #569, "Testing
//! Decisions").

use std::{path::Path, process::Command};

use serde_json::{Value, json};

use crate::common::{has, project_text, write};

/// A map naming `go-imports` whose parts are `parts`, each an id with its paths, and whose import links are `links`.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)]) -> Value {
    json!({
        "version": 2,
        "extractors": ["go-imports"],
        "parts": parts.iter().map(|(id, paths)| json!({
            "id": id, "name": id, "caption": id, "purpose": "A part.", "holds": [], "tier": "core", "paths": paths
        })).collect::<Vec<_>>(),
        "actors": [],
        "links": links.iter().map(|(from, to)| json!({
            "id": format!("{from}-{to}"), "from": from, "to": to, "kind": "import"
        })).collect::<Vec<_>>(),
        "ignored": ["system-map.json"],
        "laws": []
    })
}

/// A repository holding `map` and `files` with their texts, committed.
fn repo(map: &Value, files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let (dir, root) = project_text(&map.to_string(), &[]);
    for (file, text) in files {
        write(dir.path(), file, text);
    }
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Code"]);
    (dir, root)
}

fn report(root: &str) -> system_map::Report {
    system_map::check(Path::new(root))
}

/// Every file pair of the computed edges, as `from>to`.
fn pairs(report: &system_map::Report) -> Vec<String> {
    report
        .edges
        .iter()
        .flat_map(|e| e.pairs.iter().map(|p| format!("{}>{}", p.from, p.to)))
        .collect()
}

/// The file pairs a repository of `files` gives, its parts `a` owning everything under `a/` and `b` the rest, with no
/// link, so that only the pairs matter.
fn go_pairs(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[]), files);
    pairs(&report(&root))
}

fn run(root: &str, command: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg(command)
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}

const MOD: (&str, &str) = ("go.mod", "module example.com/m\n\ngo 1.22\n");

#[test]
fn an_import_pairs_with_every_package_file_of_its_folder_and_none_below() {
    let found = go_pairs(&[
        MOD,
        (
            "a/main.go",
            "package main\n\nimport \"example.com/m/lib\"\n\nfunc main() { lib.X() }\n",
        ),
        ("lib/x.go", "package lib\n\nfunc X() {}\n"),
        ("lib/y.go", "package lib\n"),
        ("lib/sub/z.go", "package sub\n"),
        ("lib/notes.md", "x\n"),
    ]);
    assert_eq!(found, ["a/main.go>lib/x.go", "a/main.go>lib/y.go"]);
}

#[test]
fn every_module_is_indexed_by_its_declared_path_whatever_its_folder() {
    let found = go_pairs(&[
        ("a/go.mod", "module example.com/a\n"),
        (
            "a/main.go",
            "package main\n\nimport (\n\t\"example.com/etcd/server/v3/store\"\n\t\"example.org/lib\"\n)\n",
        ),
        ("server/go.mod", "module example.com/etcd/server/v3\n"),
        ("server/store/s.go", "package store\n"),
        ("lib/go.mod", "module example.org/lib\n"),
        ("lib/lib.go", "package lib\n"),
    ]);
    assert_eq!(
        found,
        ["a/main.go>lib/lib.go", "a/main.go>server/store/s.go"]
    );
}

#[test]
fn a_nested_module_stops_the_outer_one_at_its_folder() {
    let found = go_pairs(&[
        MOD,
        (
            "a/main.go",
            "package main\n\nimport (\n\t\"example.com/m/inner/x\"\n\t\"example.com/m/inner/y\"\n)\n",
        ),
        ("inner/go.mod", "module example.com/other\n"),
        ("inner/x/x.go", "package x\n"),
        // `example.com/other/y` names this folder; the outer module's path does not reach it.
        ("inner/y/y.go", "package y\n"),
    ]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_module_block_and_a_backquoted_path_are_read() {
    for go_mod in [
        "// The module.\nmodule (\n\texample.com/m // the path\n)\n",
        "module `example.com/m`\n",
        "module \"example.com/m\"\n",
    ] {
        let found = go_pairs(&[
            ("go.mod", go_mod),
            ("a/main.go", "package main\n\nimport `example.com/m/lib`\n"),
            ("lib/x.go", "package lib\n"),
        ]);
        assert_eq!(found, ["a/main.go>lib/x.go"], "{go_mod}");
    }
}

#[test]
fn std_third_party_cgo_relative_and_vendor_paths_give_no_pair() {
    let found = go_pairs(&[
        MOD,
        (
            "a/main.go",
            "package main\n\n// #include \"stdio.h\"\nimport \"C\"\n\nimport (\n\t\"fmt\"\n\t\"github.com/x/y\"\n\t\
             \"./lib\"\n\t\"../lib\"\n\t\"/lib\"\n\t\"example.com/m/vendor/lib\"\n\t\"example.com/m/lib\\x00\"\n)\n",
        ),
        ("lib/x.go", "package lib\n"),
        ("vendor/lib/v.go", "package lib\n"),
        ("C/c.go", "package c\n"),
    ]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn named_dot_and_blank_imports_count_each_at_its_own_line() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[]),
        &[
            MOD,
            (
                "a/main.go",
                "package main\n\nimport (\n\tx \"example.com/m/x\"\n\t. \"example.com/m/y\"\n\t_ \"example.com/m/z\"\n)\n\
                 import w \"example.com/m/w\"; import \"example.com/m/v\"\n",
            ),
            ("x/x.go", "package x\n"),
            ("y/y.go", "package y\n"),
            ("z/z.go", "package z\n"),
            ("w/w.go", "package w\n"),
            ("v/v.go", "package v\n"),
        ],
    );
    assert_eq!(
        sites(&root),
        [
            "a/main.go>v/v.go a/main.go:8",
            "a/main.go>w/w.go a/main.go:8",
            "a/main.go>x/x.go a/main.go:4",
            "a/main.go>y/y.go a/main.go:5",
            "a/main.go>z/z.go a/main.go:6",
        ]
    );
}

/// Each file pair's import sites in `check --json`, as `from>to file:line`.
fn sites(root: &str) -> Vec<String> {
    let out = run(root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut found = Vec::new();
    for edge in value["edges"].as_array().into_iter().flatten() {
        for pair in edge["pairs"].as_array().unwrap() {
            for site in pair["sites"].as_array().unwrap() {
                found.push(format!(
                    "{}>{} {}:{}",
                    pair["from"].as_str().unwrap(),
                    pair["to"].as_str().unwrap(),
                    site["file"].as_str().unwrap(),
                    site["line"]
                ));
            }
        }
    }
    found.sort();
    found
}

const IMPORTS_LIB: &str = "package main\n\nimport \"example.com/m/lib\"\n";

#[test]
fn a_skipped_file_gives_no_pair_and_does_not_make_its_part_read() {
    for (file, text) in [
        ("s/x_test.go", IMPORTS_LIB),
        ("testdata/t.go", IMPORTS_LIB),
        ("s/testdata/t.go", IMPORTS_LIB),
        ("vendor/v.go", IMPORTS_LIB),
        ("_x/t.go", IMPORTS_LIB),
        (".x/t.go", IMPORTS_LIB),
        ("s/_name.go", IMPORTS_LIB),
        ("s/.name.go", IMPORTS_LIB),
        (
            "s/gen.go",
            "// Generates the tables.\n\n//go:build ignore\n\npackage main\n\nimport \"example.com/m/lib\"\n",
        ),
        (
            "s/old.go",
            "// +build ignore\n\npackage main\n\nimport \"example.com/m/lib\"\n",
        ),
        (
            "s/doc.go",
            "// Package documentation.\npackage documentation\n\nimport \"example.com/m/lib\"\n",
        ),
    ] {
        let (_dir, root) = repo(
            &map(
                &[("lib", &["go.mod", "lib/"]), ("s", &[file])],
                &[("s", "lib")],
            ),
            &[MOD, ("lib/x.go", "package lib\n"), (file, text)],
        );
        let found = report(&root);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{file}");
        assert_eq!(found.problems, Vec::<String>::new(), "{file}");
        assert_eq!(found.counts.links.unproven, 1, "{file}");
    }
}

#[test]
fn a_pair_into_a_package_names_only_its_package_files() {
    let found = go_pairs(&[
        MOD,
        ("a/main.go", IMPORTS_LIB),
        ("lib/x.go", "package lib\n"),
        ("lib/x_test.go", "package lib\n"),
        ("lib/_y.go", "package lib\n"),
        ("lib/gen.go", "//go:build ignore\n\npackage main\n"),
        ("lib/old.go", "// +build ignore\n\npackage main\n"),
        ("lib/doc.go", "package documentation\n"),
        ("lib/testdata/t.go", "package t\n"),
    ]);
    assert_eq!(found, ["a/main.go>lib/x.go"]);
}

#[test]
fn a_file_under_no_go_mod_gives_no_pair_and_no_note_but_its_part_is_read() {
    let (_dir, root) = repo(
        &map(&[("lib", &["lib/"]), ("s", &["s/"])], &[("s", "lib")]),
        &[
            ("lib/go.mod", "module example.com/m/lib\n"),
            ("lib/x.go", "package lib\n"),
            ("s/main.go", IMPORTS_LIB),
        ],
    );
    let out = run(&root, "check", &[]);
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    let found = report(&root);
    assert_eq!(pairs(&found), Vec::<String>::new());
    assert!(
        has(&found.problems, &["import link \"s-lib\"", "no dependency"]),
        "{:?}",
        found.problems
    );
}

/// `a/` and `b/` both declare `example.com/x`, and only `a/` holds the package `sub`, which `app/main.go` imports.
fn declared_twice(links: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    repo(
        &map(
            &[("app", &["app/"]), ("xa", &["a/"]), ("xb", &["b/"])],
            links,
        ),
        &[
            ("app/go.mod", "module example.com/app\n"),
            (
                "app/main.go",
                "package main\n\nimport \"example.com/x/sub\"\n",
            ),
            ("a/go.mod", "module example.com/x\n"),
            ("a/sub/s.go", "package sub\n"),
            ("a/sub/t.go", "package sub\n"),
            ("b/go.mod", "module example.com/x\n"),
            ("b/b.go", "package x\n"),
        ],
    )
}

#[test]
fn a_path_declared_twice_makes_the_importer_unsure_and_relaxes_only_the_link_to_its_candidates() {
    let (_dir, root) = declared_twice(&[("app", "xa"), ("app", "xb")]);
    let out = run(&root, "check", &[]);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "note: app/main.go is unsure of example.com/x/sub; its links to xa pass as unproven\n"
    );
    let problems = String::from_utf8_lossy(&out.stdout);
    assert_eq!(problems.lines().count(), 1, "{problems}");
    assert!(
        problems.starts_with(
            "import link \"app-xb\" runs from \"app\" to \"xb\", but the code shows no dependency"
        ),
        "{problems}"
    );
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["unsure"],
        json!([{
            "file": "app/main.go",
            "name": "example.com/x/sub",
            "reason": "declared twice",
            "candidates": ["a/sub/s.go", "a/sub/t.go"]
        }])
    );
    assert_eq!(value["edges"], json!([]));
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("note: app/main.go is unsure of example.com/x/sub")
    );
}

#[test]
fn imports_json_lists_the_unsure_files_and_the_pass_line_counts_the_relaxed_link_unproven() {
    let (_dir, root) = declared_twice(&[("app", "xa")]);
    let out = run(&root, "imports", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["unsure"][0]["file"], "app/main.go");
    assert_eq!(value["unsure"][0]["reason"], "declared twice");
    let out = run(&root, "check", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 import (0 proven, 1 unproven)"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// The report of [`declared_twice`]'s repository with `work` added, its `go.work` files, which a part `rest` owns, and
/// an import link from `app` to `xa`.
fn worked(work: &[(&str, &str)]) -> system_map::Report {
    let (dir, root) = declared_twice(&[]);
    let map = map(
        &[
            ("rest", &["**"]),
            ("app", &["app/"]),
            ("xa", &["a/"]),
            ("xb", &["b/"]),
        ],
        &[("app", "xa")],
    );
    write(dir.path(), "system-map.json", &map.to_string());
    for (file, text) in work {
        write(dir.path(), file, text);
    }
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Work"]);
    report(&root)
}

/// The pairs `declared_twice` gives when `a/` wins the tie.
const A_WINS: [&str; 2] = ["app/main.go>a/sub/s.go", "app/main.go>a/sub/t.go"];

#[test]
fn a_go_work_using_exactly_one_declarer_breaks_the_tie() {
    let found = worked(&[(
        "go.work",
        "go 1.22\n\nuse (\n\t./app\n\t./a // the real one\n)\n",
    )]);
    assert_eq!(pairs(&found), A_WINS);
    assert_eq!(found.unsure, []);
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn two_go_work_files_picking_different_winners_leave_the_path_unsure() {
    let found = worked(&[
        ("go.work", "use ./a\n"),
        ("app/go.work", "use (\n\t.\n\t../b\n)\n"),
    ]);
    assert_eq!(pairs(&found), Vec::<String>::new());
    let reasons: Vec<_> = found.unsure.iter().map(|u| u.reason.as_str()).collect();
    assert_eq!(reasons, ["declared twice"]);
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn a_go_work_under_an_excluded_folder_is_never_read() {
    let found = worked(&[
        ("go.work", "use ./a\n"),
        ("testdata/go.work", "use ../b\n"),
        ("_x/go.work", "frobnicate (\n"),
    ]);
    assert_eq!(pairs(&found), A_WINS);
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn a_use_path_is_joined_to_its_go_work_folder_and_cleaned() {
    for (file, used) in [
        ("go.work", "./a"),
        ("go.work", "a/"),
        ("go.work", "x/../a"),
        ("go.work", "\"./a/\""),
        ("w/go.work", "../a"),
    ] {
        let found = worked(&[(file, &format!("use {used}\n"))]);
        assert_eq!(pairs(&found), A_WINS, "{file}: {used}");
        assert_eq!(found.problems, Vec::<String>::new(), "{file}: {used}");
    }
}

#[test]
fn a_use_outside_the_repository_absolute_or_of_a_folder_with_no_go_mod_declares_nothing() {
    // Read as `b/`, `/b` would make the root's `go.work` use both declarers, and `../../b` disagree with its pick.
    let found = worked(&[
        (
            "go.work",
            "use (\n\t./a\n\t/b\n\t./missing\n\t./app/sub\n)\n",
        ),
        ("w/go.work", "use ../../b\n"),
    ]);
    assert_eq!(pairs(&found), A_WINS);
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn a_malformed_go_work_is_a_problem_naming_its_file_and_line() {
    for (text, says) in [
        (
            "go 1.22\n\nuse (\n\t./a\n",
            "go.work:3: a block opened with `(` is never closed",
        ),
        (
            "go 1.22\nrequire example.com/x v1.0.0\n",
            "go.work:2: unknown directive `require`",
        ),
        ("require (\n)\n", "go.work:1: unknown block kind `require`"),
        ("use \"./a\n", "go.work:1: the line does not tokenise"),
        (
            "use ./app\nuse ./a\nuse ./b\n",
            "go.work:3: uses both a/ and b/, which declare example.com/x",
        ),
    ] {
        let found = worked(&[("go.work", text)]);
        assert!(
            has(&found.problems, &[says]),
            "{text}: {:?}",
            found.problems
        );
    }
}

#[test]
fn a_files_own_module_path_means_its_own_module_even_when_a_copy_declares_it() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["m/main.go"])], &[]),
        &[
            ("m/go.mod", "module example\n"),
            ("m/main.go", "package main\n\nimport \"example/lib\"\n"),
            ("m/lib/x.go", "package lib\n"),
            ("copy/go.mod", "module example\n"),
            ("copy/lib/y.go", "package lib\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), ["m/main.go>m/lib/x.go"]);
    assert_eq!(found.unsure, []);
}

#[test]
fn a_doubtful_import_whose_folders_hold_no_package_file_gives_no_finding() {
    let (_dir, root) = repo(
        &map(&[("app", &["app/"]), ("xa", &["a/"]), ("xb", &["b/"])], &[]),
        &[
            ("app/go.mod", "module example.com/app\n"),
            (
                "app/main.go",
                "package main\n\nimport \"example.com/x/sub\"\n",
            ),
            ("a/go.mod", "module example.com/x\n"),
            ("a/sub/s_test.go", "package sub\n"),
            ("b/go.mod", "module example.com/x\n"),
            ("b/sub/_s.go", "package sub\n"),
        ],
    );
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value.get("unsure"), None);
    assert_eq!(value["problems"], json!([]));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}

#[test]
fn two_modules_providing_one_folder_make_the_importer_unsure() {
    let (_dir, root) = repo(
        &map(
            &[("app", &["app/"]), ("x", &["x/"]), ("sub", &["sub/"])],
            &[],
        ),
        &[
            ("app/go.mod", "module example.com/app\n"),
            (
                "app/main.go",
                "package main\n\nimport \"example.com/x/sub\"\n",
            ),
            ("x/go.mod", "module example.com/x\n"),
            ("x/sub/a.go", "package sub\n"),
            ("sub/go.mod", "module example.com/x/sub\n"),
            ("sub/b.go", "package sub\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), Vec::<String>::new());
    let unsure: Vec<_> = found
        .unsure
        .iter()
        .map(|u| {
            (
                u.file.as_str(),
                u.name.as_str(),
                u.reason.as_str(),
                u.candidates.clone(),
            )
        })
        .collect();
    assert_eq!(
        unsure,
        [(
            "app/main.go",
            "example.com/x/sub",
            "two modules provide it",
            vec!["sub/b.go".to_string(), "x/sub/a.go".to_string()]
        )]
    );
}

#[test]
fn a_nested_module_that_alone_provides_a_folder_gives_pairs_beside_a_doubtful_path() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["app/"])], &[]),
        &[
            ("app/go.mod", "module example.com/app\n"),
            (
                "app/main.go",
                "package main\n\nimport \"example.com/x/y/z\"\n",
            ),
            ("a/go.mod", "module example.com/x\n"),
            ("b/go.mod", "module example.com/x\n"),
            ("c/go.mod", "module example.com/x/y\n"),
            ("c/z/z.go", "package z\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), ["app/main.go>c/z/z.go"]);
    assert_eq!(found.unsure, []);
}

#[test]
fn an_excluded_go_mod_is_never_indexed_but_still_bounds_a_module() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["app/"])], &[("a", "b")]),
        &[
            ("app/go.mod", "module example.com/app\n"),
            (
                "app/main.go",
                "package main\n\nimport (\n\t\"example.com/x/lib\"\n\t\"example.com/x/fixed\"\n)\n",
            ),
            ("x/go.mod", "module example.com/x\n"),
            ("x/lib/l.go", "package lib\n"),
            // A fixture module declaring the same path, and one inside `x`.
            ("_fixture/go.mod", "module example.com/x\n"),
            ("_fixture/lib/f.go", "package lib\n"),
            ("x/fixed/testdata/go.mod", "module example.com/x/fixed\n"),
            ("x/fixed/f.go", "package fixed\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(
        pairs(&found),
        ["app/main.go>x/fixed/f.go", "app/main.go>x/lib/l.go"]
    );
    assert_eq!(found.unsure, []);
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn a_go_mod_without_exactly_one_module_line_is_a_problem_naming_it() {
    for (text, says) in [
        ("go 1.22\n", "a/go.mod: no readable module line"),
        (
            "module example.com/a\nmodule example.com/b\n",
            "a/go.mod: more than one module line",
        ),
        (
            "module ( example.com/a\n",
            "a/go.mod: a block opened with `(` is never closed",
        ),
    ] {
        let (_dir, root) = repo(
            &map(&[("a", &["**"])], &[]),
            &[MOD, ("a/go.mod", text), ("a/a.go", "package a\n")],
        );
        assert!(
            has(&report(&root).problems, &[says]),
            "{text}: {:?}",
            report(&root).problems
        );
    }
}

#[test]
fn a_repository_with_no_go_mod_outside_the_excluded_folders_gets_one_problem_saying_so() {
    for files in [
        &[("a/a.go", "package a\n")][..],
        &[
            ("testdata/go.mod", "module example.com/t\n"),
            ("a/a.go", "package a\n"),
        ],
        &[
            ("_x/go.mod", "module example.com/t\n"),
            ("vendor/go.mod", "module v\n"),
        ],
    ] {
        let (_dir, root) = repo(&map(&[("a", &["**"])], &[]), files);
        assert_eq!(
            report(&root).problems,
            [
                "go-imports found no go.mod outside testdata/, vendor/ and folders starting with _ or .; add one with \
              a module line, or remove go-imports from extractors"
            ],
            "{files:?}"
        );
    }
}

#[test]
fn check_suggests_go_imports_only_for_a_repository_with_a_go_mod_it_would_index() {
    let note = "note: go-imports could read this repository; add it to extractors\n";
    for (files, expected) in [
        (&[("a/a.go", "package a\n")][..], ""),
        (
            &[
                ("testdata/go.mod", "module example.com/t\n"),
                ("a/a.go", "package a\n"),
            ],
            "",
        ),
        (
            &[
                ("go.mod", "module example.com/m\n"),
                ("a/a.go", "package a\n"),
            ],
            note,
        ),
    ] {
        let mut map = map(&[("a", &["**"])], &[]);
        map["extractors"] = json!([]);
        let (_dir, root) = repo(&map, files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

#[test]
fn import_like_text_in_comments_and_after_the_imports_gives_no_pair() {
    let found = go_pairs(&[
        MOD,
        (
            "a/main.go",
            "// Command main. import \"example.com/m/x\"\npackage main // import \"example.com/m/x\"\n\n\
             /* import \"example.com/m/x\" */\n\n/*\n#include \"example.com/m/x\"\nimport \"example.com/m/x\"\n*/\n\
             import \"C\"\n\nimport (\n\t// \"example.com/m/x\"\n\t\"example.com/m/y\" /* \"example.com/m/x\" */\n)\n\n\
             var s = \"import \\\"example.com/m/z\\\"\"\n\nconst r = `\nimport \"example.com/m/z\"\n`\n\n\
             import \"example.com/m/z\"\n",
        ),
        ("x/x.go", "package x\n"),
        ("y/y.go", "package y\n"),
        ("z/z.go", "package z\n"),
    ]);
    assert_eq!(found, ["a/main.go>y/y.go"]);
}

fn imports_json(root: &str) -> Value {
    let out = run(root, "imports", &["--json"]);
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn an_import_of_a_package_of_several_files_counts_as_one_import() {
    let (_dir, root) = repo(
        &map(
            &[("lib", &["go.mod", "lib/"]), ("a", &["a/"])],
            &[("a", "lib")],
        ),
        &[
            MOD,
            ("a/main.go", IMPORTS_LIB),
            ("lib/x.go", "package lib\n"),
            ("lib/y.go", "package lib\n"),
            ("lib/z.go", "package lib\n"),
        ],
    );
    let value = imports_json(&root);
    assert_eq!(value["total"], json!({"raw": 1, "weighted": 1.0}));
    assert_eq!(value["across"], json!({"raw": 1, "weighted": 1.0}));
    assert_eq!(value.get("unsure"), None);
}

#[test]
fn a_split_package_counts_once_per_part_and_its_hubs_name_their_part_in_the_text() {
    let mut files = vec![
        MOD,
        ("lib/x.go", "package lib\n"),
        ("lib/y.go", "package lib\n"),
    ];
    let importers: Vec<String> = (0..10).map(|i| format!("a/c{i}/main.go")).collect();
    files.extend(importers.iter().map(|f| (f.as_str(), IMPORTS_LIB)));
    let (_dir, root) = repo(
        &map(
            &[
                ("p", &["go.mod", "lib/x.go"]),
                ("q", &["lib/y.go"]),
                ("a", &["a/"]),
            ],
            &[("a", "p"), ("a", "q")],
        ),
        &files,
    );
    let value = imports_json(&root);
    // Ten importing packages, each one import of `lib/` split across `p` and `q`: twenty units over twelve slices.
    assert_eq!(value["total"]["raw"], 20);
    assert_eq!(value["mean_degree"], 3.33);
    assert_eq!(
        value["hubs"],
        json!([
            {"file": "lib/", "part": "p", "degree": 10, "fan_in": 10, "fan_out": 0},
            {"file": "lib/", "part": "q", "degree": 10, "fan_in": 10, "fan_out": 0}
        ])
    );
    let text = String::from_utf8(run(&root, "imports", &[]).stdout).unwrap();
    assert!(
        text.contains("  lib/ (p): degree 10, 10 in, 0 out\n  lib/ (q): degree 10, 10 in, 0 out\n"),
        "{text}"
    );
}

#[test]
fn an_importing_package_nothing_imports_is_one_node_counted_as_its_slice() {
    // A `main` package of six files, each importing its own one-file package.
    let libs: Vec<(String, String)> = (1..=6)
        .map(|i| {
            (
                format!("a/m{i}.go"),
                format!("package main\n\nimport \"example.com/m/lib{i}\"\n"),
            )
        })
        .collect();
    let lib_files: Vec<String> = (1..=6).map(|i| format!("lib{i}/l.go")).collect();
    let mut files = vec![MOD];
    files.extend(libs.iter().map(|(f, t)| (f.as_str(), t.as_str())));
    files.extend(lib_files.iter().map(|f| (f.as_str(), "package lib\n")));
    let (_dir, root) = repo(
        &map(&[("lib", &["**"]), ("a", &["a/"])], &[("a", "lib")]),
        &files,
    );
    let value = imports_json(&root);
    // Six units over seven nodes: `a/` and the six packages it imports, not six files of `a/`.
    assert_eq!(value["total"]["raw"], 6);
    assert_eq!(value["mean_degree"], 1.71);
    assert_eq!(
        value["hubs"],
        json!([{"file": "a/", "part": "a", "degree": 6, "fan_in": 0, "fan_out": 6}])
    );
    let text = String::from_utf8(run(&root, "imports", &[]).stdout).unwrap();
    assert!(text.contains("  a/: degree 6, 0 in, 6 out\n"), "{text}");
}

#[test]
fn commit_mode_gives_the_edges_and_doubts_of_the_working_tree() {
    let (dir, root) = declared_twice(&[("app", "xa")]);
    write(
        dir.path(),
        "app/lib/l.go",
        "package lib\n\nimport \"example.com/x\"\n",
    );
    write(
        dir.path(),
        "app/main.go",
        "package main\n\nimport (\n\t\"example.com/x/sub\"\n\t\"example.com/app/lib\"\n)\n",
    );
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Lib"]);
    let tree = run(&root, "check", &["--json"]);
    let commit = run(&root, "check", &["--json", "--commit", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&commit.stderr),
        String::from_utf8_lossy(&tree.stderr)
    );
    let value: Value = serde_json::from_slice(&commit.stdout).unwrap();
    assert_eq!(value["unsure"].as_array().map(Vec::len), Some(2), "{value}");
    // An edit not committed changes nothing at the commit.
    write(dir.path(), "app/main.go", "package main\n");
    assert_eq!(
        String::from_utf8_lossy(&run(&root, "check", &["--json", "--commit", "HEAD"]).stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
}
