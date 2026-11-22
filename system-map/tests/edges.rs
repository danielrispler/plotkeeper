//! Computed edges: dependencies between parts that no link joins, read by the named extractors.

use std::{path::Path, process::Command};

use serde_json::{Value, json};

use crate::support::{base, has, project_text, write};

/// The map `base` makes, naming `extractors` and with `links` instead of its own.
fn map(extractors: &[&str], links: Value) -> Value {
    let mut map = base();
    map["extractors"] = json!(extractors);
    map["links"] = links;
    map["ignored"] = json!(["docs/", "Cargo.toml", "system-map.json", "ign/"]);
    map
}

fn manifest(name: &str, deps: &str, dev_deps: &str) -> String {
    format!(
        "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [dependencies]\n{deps}\n[dev-dependencies]\n{dev_deps}\n"
    )
}

/// A repository holding a map and a cargo workspace of the crates `alpha`, `beta` and `ign`
/// (`ign` is an ignored folder); `deps` and `dev_deps` map a crate to its dependency lines.
fn workspace(
    map: &Value,
    deps: &[(&str, &str)],
    dev_deps: &[(&str, &str)],
) -> (tempfile::TempDir, String) {
    let line = |list: &[(&str, &str)], name: &str| {
        list.iter()
            .filter(|(from, _)| *from == name)
            .map(|(_, to)| format!("{to} = {{ path = \"../{to}\" }}\n"))
            .collect::<String>()
    };
    let (dir, root) = project_text(&map.to_string(), &["docs/d.md"]);
    let crates = [("alpha", "alpha"), ("beta", "beta"), ("ign", "ign")];
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"alpha\", \"beta\", \"ign\"]\n",
    );
    for (name, folder) in crates {
        let text = manifest(name, &line(deps, name), &line(dev_deps, name));
        write(dir.path(), &format!("{folder}/Cargo.toml"), &text);
        write(dir.path(), &format!("{folder}/src/lib.rs"), "");
    }
    let git = |args: &[&str]| studio_test_support::git(dir.path(), args);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "Workspace"]);
    (dir, root)
}

fn report(root: &str) -> system_map::Report {
    system_map::check(Path::new(root))
}

/// The unlinked edges as `from>to` strings.
fn edges(report: &system_map::Report) -> Vec<String> {
    report
        .unlinked
        .iter()
        .map(|e| format!("{}>{}", e.from, e.to))
        .collect()
}

#[test]
fn a_dependency_across_parts_without_a_link_is_reported_and_the_check_passes() {
    let (_dir, root) = workspace(&map(&["cargo"], json!([])), &[("alpha", "beta")], &[]);
    let report = report(&root);
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(edges(&report), ["alpha>beta"]);
}

#[test]
fn a_link_in_either_direction_silences_the_edge() {
    for (from, to) in [("alpha", "beta"), ("beta", "alpha")] {
        let links = json!([{ "id": "joined", "from": from, "to": to }]);
        let (_dir, root) = workspace(&map(&["cargo"], links), &[("alpha", "beta")], &[]);
        let report = report(&root);
        assert_eq!(report.problems, Vec::<String>::new());
        assert_eq!(edges(&report), Vec::<String>::new(), "link {from}>{to}");
    }
}

#[test]
fn a_dev_dependency_is_not_reported() {
    let (_dir, root) = workspace(&map(&["cargo"], json!([])), &[], &[("alpha", "beta")]);
    assert_eq!(edges(&report(&root)), Vec::<String>::new());
}

#[test]
fn a_dependency_inside_one_part_is_not_reported() {
    let mut map = map(&["cargo"], json!([]));
    map["parts"][0]["paths"] = json!(["alpha/", "beta/"]);
    map["parts"][1]["paths"] = json!(["other/"]);
    let (_dir, root) = workspace_in_one_part(&map);
    assert_eq!(edges(&report(&root)), Vec::<String>::new());
}

fn workspace_in_one_part(map: &Value) -> (tempfile::TempDir, String) {
    let (dir, root) = project_text(&map.to_string(), &["docs/d.md", "other/o.rs"]);
    write(
        dir.path(),
        "Cargo.toml",
        "[workspace]\nresolver = \"2\"\nmembers = [\"alpha\", \"beta\"]\n",
    );
    write(
        dir.path(),
        "alpha/Cargo.toml",
        &manifest("alpha", "beta = { path = \"../beta\" }\n", ""),
    );
    write(dir.path(), "beta/Cargo.toml", &manifest("beta", "", ""));
    for f in ["alpha/src/lib.rs", "beta/src/lib.rs"] {
        write(dir.path(), f, "");
    }
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Workspace"]);
    (dir, root)
}

#[test]
fn a_dependency_on_or_from_an_ignored_crate_is_not_reported() {
    let (_dir, root) = workspace(
        &map(&["cargo"], json!([])),
        &[("alpha", "ign"), ("ign", "beta")],
        &[],
    );
    assert_eq!(edges(&report(&root)), Vec::<String>::new());
}

#[test]
fn a_map_naming_no_extractor_reports_no_edge_and_runs_no_tool() {
    // No Cargo.toml at all: running cargo here would be a problem.
    let mut map = base();
    map["links"] = json!([]);
    let (_dir, root) = project_text(&map.to_string(), &["alpha/a.rs", "beta/b.rs", "docs/d.md"]);
    let report = report(&root);
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(edges(&report), Vec::<String>::new());
}

#[test]
fn a_map_naming_no_extractor_ignores_a_workspace_dependency() {
    let mut map = map(&[], json!([]));
    map.as_object_mut().unwrap().remove("extractors");
    let (_dir, root) = workspace(&map, &[("alpha", "beta")], &[]);
    assert_eq!(edges(&report(&root)), Vec::<String>::new());
}

#[test]
fn naming_cargo_in_a_repository_without_a_cargo_toml_fails_and_says_so() {
    let mut map = map(&["cargo"], json!([]));
    map["ignored"] = json!(["docs/", "system-map.json"]);
    let (_dir, root) = project_text(&map.to_string(), &["alpha/a.rs", "beta/b.rs", "docs/d.md"]);
    let problems = report(&root).problems;
    assert!(has(&problems, &["Cargo.toml", "cargo"]), "{problems:?}");
}

#[test]
fn an_unknown_extractor_name_fails_and_is_named() {
    let (_dir, root) = workspace(&map(&["cargo", "gradle"], json!([])), &[], &[]);
    let problems = report(&root).problems;
    assert!(has(&problems, &["gradle", "extractor"]), "{problems:?}");
}

/// A repository holding `map` and the tracked `files` with their text.
fn tree(map: &Value, files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let (dir, root) = project_text(&map.to_string(), &[]);
    for (file, text) in files {
        write(dir.path(), file, text);
    }
    let git = |args: &[&str]| studio_test_support::git(dir.path(), args);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "Tree"]);
    (dir, root)
}

/// The map naming `extractors`, with `alpha` owning `alpha_paths` and `beta` owning `beta_paths`.
fn split_map(extractors: &[&str], alpha_paths: &[&str], beta_paths: &[&str]) -> Value {
    let mut map = map(extractors, json!([]));
    map["parts"][0]["paths"] = json!(alpha_paths);
    map["parts"][1]["paths"] = json!(beta_paths);
    map["ignored"] = json!(["system-map.json"]);
    map
}

fn rust_edges(files: &[(&str, &str)], alpha: &[&str], beta: &[&str]) -> Vec<String> {
    let (_dir, root) = tree(&split_map(&["rust-modules"], alpha, beta), files);
    let report = report(&root);
    assert_eq!(report.problems, Vec::<String>::new());
    edges(&report)
}

#[test]
fn a_use_crate_path_between_modules_of_two_parts_is_reported() {
    // Only the `use` lines make the edge from beta to alpha; the `mod` lines make the other.
    for text in ["use crate::a::A;\n", "use crate::{b::B, a::{self, A}};\n"] {
        let found = rust_edges(
            &[
                ("k/src/lib.rs", "mod a;\nmod b;\n"),
                ("k/src/a.rs", "pub struct A;\n"),
                ("k/src/b.rs", text),
            ],
            &["k/src/lib.rs", "k/src/a.rs"],
            &["k/src/b.rs"],
        );
        assert_eq!(found, ["alpha>beta", "beta>alpha"], "{text}");
    }
}

#[test]
fn a_use_of_the_crate_root_reaches_the_root_file() {
    let mut map = split_map(&["rust-modules"], &["k/src/lib.rs"], &["k/src/b.rs"]);
    map["ignored"] = json!(["system-map.json"]);
    let (_dir, root) = tree(
        &map,
        &[
            ("k/src/lib.rs", "mod b;\npub struct Studio;\n"),
            ("k/src/b.rs", "use crate::Studio;\n"),
        ],
    );
    let report = report(&root);
    assert_eq!(edges(&report), ["alpha>beta", "beta>alpha"]);
}

#[test]
fn a_mod_declaration_resolves_to_name_rs_and_to_name_mod_rs() {
    let found = rust_edges(
        &[
            ("k/src/lib.rs", "mod flat;\nmod nested;\n"),
            ("k/src/flat.rs", ""),
            ("k/src/nested/mod.rs", "mod deep;\n"),
            ("k/src/nested/deep.rs", ""),
        ],
        &["k/src/lib.rs"],
        &["k/src/flat.rs", "k/src/nested/"],
    );
    assert_eq!(found, ["alpha>beta"]);
    let found = rust_edges(
        &[
            ("k/src/lib.rs", "mod nested;\n"),
            ("k/src/nested/mod.rs", ""),
        ],
        &["k/src/lib.rs"],
        &["k/src/nested/mod.rs"],
    );
    assert_eq!(found, ["alpha>beta"]);
    let found = rust_edges(
        &[("k/src/lib.rs", "mod flat;\n"), ("k/src/flat.rs", "")],
        &["k/src/lib.rs"],
        &["k/src/flat.rs"],
    );
    assert_eq!(found, ["alpha>beta"]);
}

#[test]
fn a_nested_module_file_is_found_beside_its_parent_and_a_use_group_is_read() {
    let found = rust_edges(
        &[
            ("k/src/lib.rs", "mod a;\nmod c;\n"),
            (
                "k/src/a.rs",
                "mod inner;\nuse crate::{c::{self, X}, a::inner::Y};\n",
            ),
            ("k/src/a/inner.rs", ""),
            ("k/src/c.rs", ""),
        ],
        &["k/src/lib.rs", "k/src/a.rs", "k/src/a/inner.rs"],
        &["k/src/c.rs"],
    );
    assert_eq!(found, ["alpha>beta"]);
}

#[test]
fn unresolvable_rust_paths_are_skipped_without_a_problem() {
    let found = rust_edges(
        &[
            ("k/src/lib.rs", "mod missing;\nmod a;\n// mod b;\n"),
            (
                "k/src/a.rs",
                "use std::fmt;\nuse serde::Serialize;\nuse crate::nowhere::Thing;\nuse super::x;\n\
                 let s = \"use crate::b::X;\";\n",
            ),
            ("k/src/b.rs", ""),
        ],
        &["k/src/lib.rs", "k/src/a.rs"],
        &["k/src/b.rs"],
    );
    assert_eq!(found, Vec::<String>::new());
}

fn ts_edges(files: &[(&str, &str)]) -> Vec<String> {
    let map = split_map(&["typescript-imports"], &["alpha/"], &["beta/"]);
    let (_dir, root) = tree(&map, files);
    let report = report(&root);
    assert_eq!(report.problems, Vec::<String>::new());
    edges(&report)
}

#[test]
fn a_relative_import_of_another_part_is_reported_and_js_resolves_to_ts() {
    for spec in ["../beta/b.ts", "../beta/b.js", "../beta/b", "../beta"] {
        let (from, to) = if spec == "../beta" {
            ("alpha/a.ts", "beta/index.ts")
        } else {
            ("alpha/a.ts", "beta/b.ts")
        };
        let text = format!("import {{ x }} from \"{spec}\";\n");
        let found = ts_edges(&[(from, &text), (to, "export const x = 1;\n")]);
        assert_eq!(found, ["alpha>beta"], "{spec}");
    }
}

#[test]
fn other_import_forms_are_read() {
    for text in [
        "import '../beta/b.ts';",
        "export * from '../beta/b.ts';",
        "const m = await import('../beta/b.ts');",
        "import {\n  x,\n} from '../beta/b.ts';",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, ["alpha>beta"], "{text}");
    }
}

#[test]
fn an_untracked_target_and_a_package_import_are_skipped_without_a_problem() {
    let found = ts_edges(&[
        (
            "alpha/a.ts",
            "import { h } from '../../dist/home.js';\nimport x from 'react';\n\
             import y from '@scope/pkg';\nimport z from '../beta/gone.ts';\n\
             import w from '../../../outside.ts';\n",
        ),
        ("beta/b.ts", ""),
    ]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn the_new_extractors_do_not_run_when_the_map_does_not_name_them() {
    let mut map = split_map(&[], &["k/src/lib.rs", "alpha/"], &["k/src/b.rs", "beta/"]);
    map.as_object_mut().unwrap().remove("extractors");
    let (_dir, root) = tree(
        &map,
        &[
            ("k/src/lib.rs", "mod b;\n"),
            ("k/src/b.rs", ""),
            ("alpha/a.ts", "import '../beta/b.ts';"),
            ("beta/b.ts", ""),
        ],
    );
    assert_eq!(edges(&report(&root)), Vec::<String>::new());
}

fn run_check(root: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg("check")
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}

#[test]
fn check_prints_an_edge_on_a_line_no_problem_looks_like_and_json_lists_it() {
    let (_dir, root) = workspace(&map(&["cargo"], json!([])), &[("alpha", "beta")], &[]);
    let out = run_check(&root, &[]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(
        text.starts_with("note: ") && text.contains("alpha") && text.contains("beta"),
        "{text}"
    );

    let out = run_check(&root, &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["unlinked"],
        json!([{ "from": "alpha", "to": "beta" }])
    );
}
