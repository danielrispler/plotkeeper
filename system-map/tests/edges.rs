//! Computed edges, read by the named extractors, and the link check that holds the map's import links to them.

use std::{path::Path, process::Command};

use serde_json::{Value, json};

use crate::common::{base, has, project_text, write};

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

/// The computed edges no import link declares, as `from>to` strings, read from their problems.
fn edges(report: &system_map::Report) -> Vec<String> {
    report
        .problems
        .iter()
        .filter_map(|line| {
            let (from, rest) = line
                .strip_prefix("part \"")?
                .split_once("\" depends on part \"")?;
            Some(format!("{from}>{}", rest.split_once('"')?.0))
        })
        .collect()
}

/// The problems that are not undeclared edges.
fn other_problems(report: &system_map::Report) -> Vec<String> {
    report
        .problems
        .iter()
        .filter(|line| !(line.starts_with("part \"") && line.contains("\" depends on part \"")))
        .cloned()
        .collect()
}

fn import(id: &str, from: &str, to: &str) -> Value {
    json!({ "id": id, "from": from, "to": to, "kind": "import" })
}

#[test]
fn a_dependency_across_parts_without_an_import_link_fails() {
    let (_dir, root) = workspace(&map(&["cargo"], json!([])), &[("alpha", "beta")], &[]);
    let report = report(&root);
    assert_eq!(edges(&report), ["alpha>beta"]);
    assert_eq!(other_problems(&report), Vec::<String>::new());
}

#[test]
fn an_import_link_in_the_edge_s_direction_declares_it_and_is_proven() {
    let links = json!([import("alpha-beta", "alpha", "beta")]);
    let (_dir, root) = workspace(&map(&["cargo"], links), &[("alpha", "beta")], &[]);
    let report = report(&root);
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(
        (report.counts.links.proven, report.counts.links.unproven),
        (1, 0)
    );
}

#[test]
fn a_dead_import_link_between_read_parts_fails() {
    let links = json!([import("alpha-beta", "alpha", "beta")]);
    let (_dir, root) = workspace(&map(&["cargo"], links), &[], &[]);
    let problems = report(&root).problems;
    assert!(
        has(
            &problems,
            &[
                "import link \"alpha-beta\"",
                "no dependency of \"alpha\" on \"beta\""
            ]
        ),
        "{problems:?}"
    );
}

#[test]
fn a_backwards_import_link_fails_and_so_does_the_edge_it_misses() {
    let links = json!([import("beta-alpha", "beta", "alpha")]);
    let (_dir, root) = workspace(&map(&["cargo"], links), &[("alpha", "beta")], &[]);
    let report = report(&root);
    assert_eq!(edges(&report), ["alpha>beta"]);
    assert!(
        has(
            &report.problems,
            &["import link \"beta-alpha\"", "turn it around"]
        ),
        "{:?}",
        report.problems
    );
}

#[test]
fn a_run_time_link_beside_an_undeclared_edge_does_not_declare_it() {
    for kind in ["http", "spawn", "file"] {
        let links = json!([{ "id": "joined", "from": "alpha", "to": "beta", "kind": kind, "reason": "Data crosses." }]);
        let (_dir, root) = workspace(&map(&["cargo"], links), &[("alpha", "beta")], &[]);
        let report = report(&root);
        assert_eq!(edges(&report), ["alpha>beta"], "{kind}");
        assert_eq!(other_problems(&report), Vec::<String>::new(), "{kind}");
    }
    // Beside the import link, a run-time link on the same pair is fine.
    let links = json!([
        import("alpha-beta", "alpha", "beta"),
        { "id": "beta-alpha", "from": "beta", "to": "alpha", "kind": "file", "reason": "Beta writes a file alpha reads." }
    ]);
    let (_dir, root) = workspace(&map(&["cargo"], links), &[("alpha", "beta")], &[]);
    assert_eq!(report(&root).problems, Vec::<String>::new());
}

#[test]
fn an_import_link_with_an_unread_end_passes_as_unproven() {
    // No extractor named: no part is read.
    let mut map = base();
    map["links"] = json!([import("alpha-beta", "alpha", "beta")]);
    let (_dir, root) = project_text(&map.to_string(), &["alpha/a.rs", "beta/b.rs", "docs/d.md"]);
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (0, 1)
    );
    // `rust-modules` named, but beta owns no Rust file: alpha is read, beta is not.
    let mut map = split_map(&["rust-modules"], &["alpha/"], &["beta/"]);
    map["links"] = json!([import("alpha-beta", "alpha", "beta")]);
    let (_dir, root) = tree(
        &map,
        &[("alpha/src/lib.rs", ""), ("beta/main.go", "package main\n")],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (0, 1)
    );
}

#[test]
fn an_import_link_with_an_actor_at_an_end_fails() {
    for (from, to) in [("alpha", "writer"), ("writer", "alpha")] {
        let mut map = base();
        map["links"] = json!([import("joined", from, to)]);
        let (_dir, root) =
            project_text(&map.to_string(), &["alpha/a.rs", "beta/b.rs", "docs/d.md"]);
        let problems = report(&root).problems;
        assert!(
            has(
                &problems,
                &["import link \"joined\"", "actor \"writer\"", "owns no code"]
            ),
            "{problems:?}"
        );
    }
}

#[test]
fn a_link_needs_a_kind_and_a_run_time_link_a_reason_of_at_most_140_characters() {
    let check = |link: Value| {
        let mut map = base();
        map["links"] = json!([link]);
        let (_dir, root) =
            project_text(&map.to_string(), &["alpha/a.rs", "beta/b.rs", "docs/d.md"]);
        report(&root).problems
    };
    let problems = check(json!({ "id": "joined", "from": "alpha", "to": "beta" }));
    assert!(
        has(
            &problems,
            &["link \"joined\"", "no kind", "import, http, spawn or file"]
        ),
        "{problems:?}"
    );
    for reason in [None, Some(""), Some("  ")] {
        let mut link = json!({ "id": "joined", "from": "alpha", "to": "writer", "kind": "http" });
        if let Some(reason) = reason {
            link["reason"] = json!(reason);
        }
        let problems = check(link);
        assert!(
            has(&problems, &["link \"joined\"", "http", "no reason"]),
            "{reason:?}: {problems:?}"
        );
    }
    let long = check(
        json!({ "id": "joined", "from": "alpha", "to": "writer", "kind": "spawn", "reason": "x".repeat(141) }),
    );
    assert!(
        has(
            &long,
            &[
                "link \"joined\"",
                "reason is 141 characters, the cap is 140",
                "the area's doc under docs/studio/behaviour/"
            ]
        ),
        "{long:?}"
    );
    let at_cap = check(
        json!({ "id": "joined", "from": "alpha", "to": "writer", "kind": "file", "reason": "é".repeat(140) }),
    );
    assert_eq!(at_cap, Vec::<String>::new());
    let import_without = check(import("joined", "alpha", "beta"));
    assert_eq!(
        import_without,
        Vec::<String>::new(),
        "an import link needs no reason"
    );
    let unknown = check(
        json!({ "id": "joined", "from": "alpha", "to": "beta", "kind": "grpc", "reason": "r" }),
    );
    assert!(
        has(&unknown, &["does not match the map's shape"]),
        "{unknown:?}"
    );
}

#[test]
fn a_dev_dependency_is_not_reported() {
    let (_dir, root) = workspace(&map(&["cargo"], json!([])), &[], &[("alpha", "beta")]);
    assert_eq!(edges(&report(&root)), Vec::<String>::new());
}

#[test]
fn a_registry_dependency_sharing_a_workspace_crate_name_is_not_reported() {
    let (dir, root) = workspace(&map(&["cargo"], json!([])), &[], &[]);
    write(
        dir.path(),
        "alpha/Cargo.toml",
        &manifest("alpha", "beta = \"0.4\"\n", ""),
    );
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Registry beta"]);
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
    assert_eq!(other_problems(&report), Vec::<String>::new());
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
fn a_use_group_naming_only_self_reaches_the_module() {
    for text in ["use crate::a::{self};\n", "use crate::a::{self as m};\n"] {
        let found = rust_edges(
            &[
                ("k/src/lib.rs", "mod a;\nmod b;\n"),
                ("k/src/a.rs", ""),
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

#[test]
fn a_mod_line_naming_a_deleted_or_symlinked_module_file_still_pairs_and_does_not_panic() {
    for symlink in [false, true] {
        let map = split_map(
            &["rust-modules"],
            &["k/src/lib.rs"],
            &["k/src/a.rs", "k/src/target.rs"],
        );
        let (dir, root) = tree(
            &map,
            &[
                ("k/src/lib.rs", "mod a;\n"),
                ("k/src/a.rs", "mod inner;\n"),
                ("k/src/target.rs", ""),
            ],
        );
        let a = dir.path().join("k/src/a.rs");
        std::fs::remove_file(&a).unwrap();
        if symlink {
            std::os::unix::fs::symlink("target.rs", &a).unwrap();
            studio_test_support::git(dir.path(), &["add", "-A"]);
        }
        let report = report(&root);
        assert_eq!(edges(&report), ["alpha>beta"], "symlink {symlink}");
    }
}

#[test]
fn a_module_file_under_tests_or_bin_finds_its_submodules_in_its_own_folder() {
    // `tests/main.rs` is a crate root; `tests/common.rs` is a module it names, so its `mod helpers;`
    // reads `tests/common/helpers.rs`. Likewise `src/tests/foo.rs`, named by `src/tests/mod.rs`.
    let cases = [
        (
            "k/tests/main.rs",
            "k/tests/common.rs",
            "k/tests/common/helpers.rs",
        ),
        (
            "k/src/lib.rs",
            "k/src/tests/foo.rs",
            "k/src/tests/foo/helpers.rs",
        ),
    ];
    for (root_file, module, helpers) in cases {
        let mut files = vec![(module, "mod helpers;\n"), (helpers, "")];
        if root_file.ends_with("lib.rs") {
            files.push((root_file, "mod tests;\n"));
            files.push(("k/src/tests/mod.rs", "mod foo;\n"));
        } else {
            files.push((root_file, "mod common;\n"));
        }
        let mut alpha = vec![root_file, module];
        if root_file.ends_with("lib.rs") {
            alpha.push("k/src/tests/mod.rs");
        }
        let found = rust_edges(&files, &alpha, &[helpers]);
        assert_eq!(found, ["alpha>beta"], "{module}");
    }
}

#[test]
fn a_raw_c_string_hides_a_quote_before_a_mod_line() {
    // Read as an ordinary string, `cr#"a " b"#` ends at its inner quote and swallows `mod b;`.
    let found = rust_edges(
        &[
            ("k/src/lib.rs", "const S: &CStr = cr#\"a \" b\"#;\nmod b;\n"),
            ("k/src/b.rs", ""),
        ],
        &["k/src/lib.rs"],
        &["k/src/b.rs"],
    );
    assert_eq!(found, ["alpha>beta"]);
}

#[test]
fn a_use_between_two_parts_is_found_when_the_crate_root_is_ignored() {
    let mut map = split_map(&["rust-modules"], &["k/src/a.rs"], &["k/src/b.rs"]);
    map["ignored"] = json!(["system-map.json", "k/src/lib.rs"]);
    let (_dir, root) = tree(
        &map,
        &[
            ("k/src/lib.rs", "mod a;\nmod b;\n"),
            ("k/src/a.rs", "use crate::b::B;\n"),
            ("k/src/b.rs", "pub struct B;\n"),
        ],
    );
    let report = report(&root);
    assert_eq!(other_problems(&report), Vec::<String>::new());
    assert_eq!(edges(&report), ["alpha>beta"]);
}

#[test]
fn an_inline_crate_path_outside_a_use_line_reaches_the_module_file() {
    for text in [
        "fn f() -> crate::a::A { crate::a::A }\n",
        "fn f() { let _ = crate::a::inner::make(); }\n",
        "pub fn f(x: crate::a::A) {}\n",
        "pub struct S { x: crate::a::A }\n",
        "fn f<T: crate::a::inner::Tr>() {}\n",
        "fn f() { S { x: crate::a::inner::make() }; }\n",
    ] {
        let found = rust_edges(
            &[
                ("k/src/lib.rs", "mod a;\nmod b;\n"),
                ("k/src/a.rs", "pub struct A;\npub mod inner {}\n"),
                ("k/src/b.rs", text),
            ],
            &["k/src/lib.rs", "k/src/a.rs"],
            &["k/src/b.rs"],
        );
        assert_eq!(found, ["alpha>beta", "beta>alpha"], "{text}");
    }
}

#[test]
fn a_bare_path_to_a_child_module_reaches_that_module_file() {
    // The crate root declares `ci_laws` and names a module below it bare, from its own module and
    // from an inline one. `ci_laws.rs` is ignored, so only the bare path can pair `lib.rs` with
    // `ci_laws/cache.rs`.
    for text in [
        "pub mod ci_laws;\nfn f() -> ci_laws::cache::CiCache { todo!() }\n",
        "pub mod ci_laws;\nmod tests { use super::*; fn f() { ci_laws::cache::CiCache::new(); } }\n",
    ] {
        let mut map = split_map(&["rust-modules"], &["k/src/ci_laws/"], &["k/src/lib.rs"]);
        map["ignored"] = json!(["system-map.json", "k/src/ci_laws.rs"]);
        let (_dir, root) = tree(
            &map,
            &[
                ("k/src/lib.rs", text),
                ("k/src/ci_laws.rs", "pub mod cache;\n"),
                ("k/src/ci_laws/cache.rs", "pub struct CiCache;\n"),
            ],
        );
        let report = report(&root);
        assert_eq!(other_problems(&report), Vec::<String>::new(), "{text}");
        assert_eq!(edges(&report), ["beta>alpha"], "{text}");
    }
}

#[test]
fn a_bare_path_through_a_module_a_use_line_brings_in_reaches_the_module_below() {
    let mut map = split_map(&["rust-modules"], &["k/src/a/"], &["k/src/b.rs"]);
    map["ignored"] = json!(["system-map.json", "k/src/lib.rs", "k/src/a.rs"]);
    let (_dir, root) = tree(
        &map,
        &[
            ("k/src/lib.rs", "mod a;\nmod b;\n"),
            ("k/src/a.rs", "pub mod inner;\n"),
            ("k/src/a/inner.rs", "pub struct X;\n"),
            ("k/src/b.rs", "use crate::a;\nfn f() { a::inner::X; }\n"),
        ],
    );
    assert_eq!(edges(&report(&root)), ["beta>alpha"]);
}

#[test]
fn a_bare_path_whose_first_word_is_no_module_gives_no_edge() {
    let found = rust_edges(
        &[
            ("k/src/lib.rs", "mod a;\nmod b;\n"),
            ("k/src/a.rs", ""),
            (
                "k/src/b.rs",
                "fn f() { std::mem::drop(1); Vec::<u8>::new(); a_thing::X; }\n",
            ),
        ],
        &["k/src/lib.rs", "k/src/b.rs"],
        &["k/src/a.rs"],
    );
    // Only the crate root's `mod a;`: `b.rs` declares no child `a`, so nothing in it reaches `a.rs`.
    assert_eq!(found, ["alpha>beta"]);
}

/// A cargo workspace of the packages `(package, folder, library name or "", manifest tail)`, each
/// with the files given, and a map giving `alpha` and `beta` their paths and ignoring `ignored`.
fn rust_workspace(
    packages: &[(&str, &str, &str, &str)],
    files: &[(&str, &str)],
    alpha: &[&str],
    beta: &[&str],
    ignored: &[&str],
) -> system_map::Report {
    let members: Vec<String> = packages.iter().map(|p| format!("{:?}", p.1)).collect();
    let mut all: Vec<(String, String)> = vec![(
        "Cargo.toml".into(),
        format!(
            "[workspace]\nresolver = \"2\"\nmembers = [{}]\n",
            members.join(", ")
        ),
    )];
    for (package, folder, lib, tail) in packages {
        let lib = if lib.is_empty() {
            String::new()
        } else {
            format!("[lib]\nname = \"{lib}\"\n\n")
        };
        let text = format!(
            "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n{lib}{tail}\n"
        );
        all.push((format!("{folder}/Cargo.toml"), text));
    }
    all.extend(files.iter().map(|(f, t)| (f.to_string(), t.to_string())));
    let all: Vec<(&str, &str)> = all.iter().map(|(f, t)| (f.as_str(), t.as_str())).collect();
    let mut map = split_map(&["rust-modules"], alpha, beta);
    let mut ignore = vec!["system-map.json", "Cargo.toml", "**/Cargo.toml"];
    ignore.extend(ignored);
    map["ignored"] = json!(ignore);
    let (_dir, root) = tree(&map, &all);
    report(&root)
}

const ON_SERVER: &str = "[dependencies]\nstudio-server = { path = \"../server\" }\n";

#[test]
fn a_path_into_a_workspace_crate_reaches_the_module_file_it_names() {
    // `lib.rs` is ignored, so an edge to it would be dropped: only a pair with `runner.rs` shows.
    for text in [
        "use studio_server::runner::launch;\n",
        "fn main() { studio_server::runner::launch(); }\n",
        "use studio_server::{runner, Other};\nfn main() { runner::launch(); }\n",
        "use ::studio_server::runner::launch;\n",
        "fn main() { ::studio_server::runner::launch(); }\n",
        "fn main() { let _: ::studio_server::runner::Launch; }\n",
    ] {
        let report = rust_workspace(
            &[
                ("studio-server", "server", "", ""),
                ("studio-app", "app", "", ON_SERVER),
            ],
            &[
                ("server/src/lib.rs", "pub mod runner;\npub struct Other;\n"),
                ("server/src/runner.rs", "pub fn launch() {}\n"),
                ("app/src/main.rs", text),
            ],
            &["server/src/runner.rs"],
            &["app/"],
            &["server/src/lib.rs"],
        );
        assert_eq!(other_problems(&report), Vec::<String>::new(), "{text}");
        assert_eq!(edges(&report), ["beta>alpha"], "{text}");
    }
}

#[test]
fn a_glob_of_a_workspace_crate_s_root_reaches_its_lib_rs() {
    let report = rust_workspace(
        &[
            ("studio-server", "server", "", ""),
            ("studio-app", "app", "", ON_SERVER),
        ],
        &[
            ("server/src/lib.rs", "pub struct Other;\n"),
            ("app/src/main.rs", "use studio_server::*;\n"),
        ],
        &["server/"],
        &["app/"],
        &[],
    );
    assert_eq!(other_problems(&report), Vec::<String>::new());
    assert_eq!(edges(&report), ["beta>alpha"]);
}

#[test]
fn a_path_to_a_workspace_crate_s_root_reaches_its_lib_rs_not_a_module() {
    let report = rust_workspace(
        &[
            ("studio-server", "server", "", ""),
            ("studio-app", "app", "", ON_SERVER),
        ],
        &[
            ("server/src/lib.rs", "pub mod runner;\npub struct Other;\n"),
            ("server/src/runner.rs", ""),
            ("app/src/main.rs", "use studio_server::Other;\n"),
        ],
        &["server/src/runner.rs"],
        &["app/"],
        &["server/src/lib.rs"],
    );
    assert_eq!(edges(&report), Vec::<String>::new());
}

#[test]
fn a_path_into_the_crate_s_own_library_from_its_binary_or_test_is_read() {
    for (file, text) in [
        (
            "server/src/bin/run.rs",
            "fn main() { studio_server::runner::launch(); }\n",
        ),
        ("server/tests/t.rs", "use studio_server::runner;\n"),
    ] {
        let report = rust_workspace(
            &[("studio-server", "server", "", "")],
            &[
                ("server/src/lib.rs", "pub mod runner;\n"),
                ("server/src/runner.rs", "pub fn launch() {}\n"),
                (file, text),
            ],
            &["server/src/lib.rs", "server/src/runner.rs"],
            &[file],
            &[],
        );
        assert_eq!(other_problems(&report), Vec::<String>::new(), "{file}");
        assert_eq!(edges(&report), ["beta>alpha"], "{file}");
    }
}

#[test]
fn a_workspace_crate_is_read_by_its_library_name_not_its_package_name() {
    let dep = "[dependencies]\nstudio-system-map = { path = \"../map\" }\n";
    for (text, expected) in [
        ("use system_map::check::run;\n", &["beta>alpha"][..]),
        ("use studio_system_map::check::run;\n", &[][..]),
    ] {
        let report = rust_workspace(
            &[
                ("studio-system-map", "map", "system_map", ""),
                ("studio-app", "app", "", dep),
            ],
            &[
                ("map/src/lib.rs", "pub mod check;\n"),
                ("map/src/check.rs", "pub fn run() {}\n"),
                ("app/src/main.rs", text),
            ],
            &["map/"],
            &["app/"],
            &[],
        );
        assert_eq!(other_problems(&report), Vec::<String>::new(), "{text}");
        assert_eq!(edges(&report), expected, "{text}");
    }
}

#[test]
fn a_crate_named_path_without_a_normal_dependency_gives_no_cross_crate_edge() {
    // `app` has a local module `studio_server` and only a dev-dependency on the real crate.
    for (tail, app_files) in [
        (
            "",
            vec![
                (
                    "app/src/main.rs",
                    "mod studio_server;\nuse studio_server::runner;\n",
                ),
                ("app/src/studio_server.rs", "pub mod runner {}\n"),
            ],
        ),
        (
            "[dev-dependencies]\nstudio-server = { path = \"../server\" }\n",
            vec![("app/tests/t.rs", "use studio_server::runner::launch;\n")],
        ),
    ] {
        let mut files = vec![
            ("server/src/lib.rs", "pub mod runner;\n"),
            ("server/src/runner.rs", "pub fn launch() {}\n"),
            ("app/src/lib.rs", ""),
        ];
        files.extend(app_files);
        let report = rust_workspace(
            &[
                ("studio-server", "server", "", ""),
                ("studio-app", "app", "", tail),
            ],
            &files,
            &["server/"],
            &["app/"],
            &[],
        );
        assert_eq!(report.problems, Vec::<String>::new(), "{tail}");
        assert_eq!(edges(&report), Vec::<String>::new(), "{tail}");
    }
}

#[test]
fn when_cargo_cannot_run_workspace_crate_paths_are_not_read_and_one_problem_says_so() {
    let mut map = split_map(
        &["rust-modules"],
        &["k/src/lib.rs", "k/src/a.rs"],
        &["k/src/b.rs"],
    );
    map["ignored"] = json!(["system-map.json", "Cargo.toml"]);
    let (_dir, root) = tree(
        &map,
        &[
            ("Cargo.toml", "this is not toml ["),
            ("k/src/lib.rs", "mod a;\nmod b;\n"),
            ("k/src/a.rs", ""),
            ("k/src/b.rs", "use crate::a::A;\n"),
        ],
    );
    let report = report(&root);
    assert_eq!(other_problems(&report).len(), 1, "{:?}", report.problems);
    assert!(
        has(
            &report.problems,
            &["rust-modules", "workspace-crate paths were not read"]
        ),
        "{:?}",
        report.problems
    );
    // The paths inside a crate are still read.
    assert_eq!(edges(&report), ["alpha>beta", "beta>alpha"]);
}

#[test]
fn when_cargo_cannot_run_an_import_link_a_workspace_crate_path_may_prove_passes_as_unproven() {
    let mut map = split_map(&["rust-modules"], &["server/"], &["app/"]);
    map["links"] = json!([{ "id": "app-server", "from": "beta", "to": "alpha", "kind": "import" }]);
    map["ignored"] = json!(["system-map.json", "Cargo.toml", "**/Cargo.toml"]);
    let (_dir, root) = tree(
        &map,
        &[
            ("Cargo.toml", "this is not toml ["),
            ("server/Cargo.toml", "[package]\nname = \"studio-server\"\n"),
            ("server/src/lib.rs", "pub mod runner;\n"),
            ("server/src/runner.rs", "pub fn launch() {}\n"),
            (
                "app/src/main.rs",
                "fn main() { studio_server::runner::launch(); }\n",
            ),
        ],
    );
    let report = report(&root);
    assert_eq!(report.problems.len(), 1, "{:?}", report.problems);
    assert!(
        has(&report.problems, &["workspace-crate paths were not read"]),
        "{:?}",
        report.problems
    );
}

fn ts_edges(files: &[(&str, &str)]) -> Vec<String> {
    let map = split_map(&["typescript-imports"], &["alpha/"], &["beta/"]);
    let (_dir, root) = tree(&map, files);
    let report = report(&root);
    assert_eq!(other_problems(&report), Vec::<String>::new());
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
fn check_prints_an_undeclared_edge_as_a_problem_and_json_has_no_unlinked() {
    let (_dir, root) = workspace(&map(&["cargo"], json!([])), &[("alpha", "beta")], &[]);
    let out = run_check(&root, &[]);
    assert!(!out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(
        text.starts_with("part \"alpha\" depends on part \"beta\""),
        "{text}"
    );

    let out = run_check(&root, &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value.get("unlinked"), None);
}

#[test]
fn import_like_text_in_comments_strings_and_template_text_gives_no_edge() {
    for text in [
        "// import { x } from '../beta/b.ts';\n",
        "/* see require('../beta/b.ts') */\n",
        "/*\n * export * from \"../beta/b.ts\";\n */\n",
        "const s = \"import '../beta/b.ts'\";\n",
        "const s = 'from \"../beta/b.ts\"';\n",
        "const s = `import(\"../beta/b.ts\")`;\n",
        "const s = `a ${1} from '../beta/b.ts'`;\n",
        "const s = \"\\\" import '../beta/b.ts'\";\n",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, Vec::<String>::new(), "{text}");
    }
}

#[test]
fn an_import_call_inside_a_template_s_substitution_gives_an_edge() {
    for text in [
        "const s = `a ${await import('../beta/b.ts')} b`;\n",
        "const s = `a ${`x ${{ k: require('../beta/b.ts') }.k}`} b`;\n",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, ["alpha>beta"], "{text}");
    }
}

#[test]
fn each_import_and_re_export_form_gives_its_edge() {
    for text in [
        "import x from '../beta/b.ts';",
        "import x, { y as z } from '../beta/b.ts';",
        "import * as ns from '../beta/b.ts';",
        "import type { T } from '../beta/b.ts';",
        "import '../beta/b.ts'",
        "const m = import(`../beta/b.ts`);",
        "const m = require(\"../beta/b.ts\");",
        "import fs = require('../beta/b.ts');",
        "export { x, y as z } from '../beta/b.ts';",
        "export * as ns from '../beta/b.ts';",
        "export type { T } from '../beta/b.ts';",
        "export type * from '../beta/b.ts';",
        "const q = /'\"`/g;\nimport x from '../beta/b.ts';",
        "#!/usr/bin/env node\nconst a = 1\nexport { a } from '../beta/b.ts'",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, ["alpha>beta"], "{text}");
    }
}

#[test]
fn a_local_export_or_a_member_named_import_gives_no_edge() {
    for text in [
        "export const from = '../beta/b.ts';\n",
        "export default '../beta/b.ts';\n",
        "obj.require('../beta/b.ts');\n",
        "const t = x.import('../beta/b.ts');\n",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, Vec::<String>::new(), "{text}");
    }
}

/// The edges and other problems `typescript-imports` gives in a repository holding `files`, with `alpha/` and `beta/` as
/// the parts, `beta/b.ts` so that `beta` is read, and the root's workspace config ignored.
fn ws_report(files: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let files = &[files, &[("beta/b.ts", "")]].concat();
    let mut map = split_map(&["typescript-imports"], &["alpha/"], &["beta/"]);
    let mut ignored = vec!["system-map.json"];
    for config in ["package.json", "pnpm-workspace.yaml"] {
        if files.iter().any(|(file, _)| *file == config) {
            ignored.push(config);
        }
    }
    map["ignored"] = json!(ignored);
    let (_dir, root) = tree(&map, files);
    let report = report(&root);
    (edges(&report), other_problems(&report))
}

const CONTRACT: (&str, &str) = (
    "beta/contract/package.json",
    "{ \"name\": \"@chery/contract\", \"version\": \"0.0.0\" }",
);

#[test]
fn an_import_of_a_declared_scoped_package_or_its_subpath_is_an_edge_to_its_part() {
    for spec in ["@chery/contract", "@chery/contract/sub/x"] {
        let text = format!("import {{ x }} from '{spec}';\n");
        let found = ws_report(&[
            ("package.json", "{ \"workspaces\": [\"beta/*\"] }"),
            ("alpha/a.ts", &text),
            CONTRACT,
            ("beta/contract/src/index.ts", "export const x = 1;\n"),
        ]);
        assert_eq!(found, (vec!["alpha>beta".to_string()], vec![]), "{spec}");
    }
}

#[test]
fn pnpm_workspace_yaml_s_packages_declare_the_workspace() {
    let found = ws_report(&[
        (
            "pnpm-workspace.yaml",
            "# The packages.\npackages:\n  - 'other/*'\n  - \"beta/*\" # contract\ncatalog:\n  - x\n",
        ),
        ("alpha/a.ts", "import { x } from '@chery/contract';\n"),
        CONTRACT,
    ]);
    assert_eq!(found, (vec!["alpha>beta".to_string()], vec![]));
}

#[test]
fn a_package_json_outside_the_workspace_globs_gives_no_edge() {
    for (workspaces, fixture) in [
        ("[\"alpha/*\"]", "beta/test/contract/package.json"),
        (
            "[\"beta/*\", \"!beta/fixtures\"]",
            "beta/fixtures/package.json",
        ),
    ] {
        let config = format!("{{ \"workspaces\": {workspaces} }}");
        let found = ws_report(&[
            ("package.json", &config),
            ("alpha/a.ts", "import { x } from '@chery/contract';\n"),
            (fixture, CONTRACT.1),
        ]);
        assert_eq!(found, (vec![], vec![]), "{workspaces}");
    }
}

#[test]
fn a_name_two_declared_packages_share_gives_no_edge_and_is_a_problem() {
    let (edges, problems) = ws_report(&[
        (
            "package.json",
            "{ \"workspaces\": [\"beta/*\", \"alpha/pkg\"] }",
        ),
        ("alpha/a.ts", "import { x } from '@chery/contract';\n"),
        ("alpha/pkg/package.json", CONTRACT.1),
        CONTRACT,
    ]);
    assert_eq!(edges, Vec::<String>::new());
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        has(
            &problems,
            &["@chery/contract", "\"alpha/pkg\"", "\"beta/contract\""]
        ),
        "{problems:?}"
    );
}

#[test]
fn without_a_workspace_config_a_bare_specifier_gives_no_edge_and_no_problem() {
    let found = ws_report(&[
        ("package.json", "{ \"name\": \"root\" }"),
        (
            "alpha/a.ts",
            "import { x } from '@chery/contract';\nimport y from 'contract';\n",
        ),
        CONTRACT,
        ("beta/plain/package.json", "{ \"name\": \"contract\" }"),
    ]);
    assert_eq!(found, (vec![], vec![]));
}

/// The report of a repository holding `files` and a map naming `typescript-imports` with `links`, with each of `app/`,
/// `entry/`, `one/`, `two/` and `three/` that holds a file a part, and the root's `package.json` ignored.
fn barrel_report(files: &[(&str, &str)], links: Value) -> system_map::Report {
    let mut map = map(&["typescript-imports"], links);
    let template = map["parts"][0].clone();
    map["parts"] = ["app", "entry", "one", "two", "three"]
        .iter()
        .filter(|id| {
            files
                .iter()
                .any(|(file, _)| file.starts_with(&format!("{id}/")))
        })
        .map(|id| {
            let mut part = template.clone();
            part["id"] = json!(id);
            part["name"] = json!(id);
            part["paths"] = json!([format!("{id}/")]);
            part
        })
        .collect();
    map["laws"][0]["on"] = json!("app");
    map["ignored"] = json!(["system-map.json"]);
    if files.iter().any(|(file, _)| *file == "package.json") {
        map["ignored"] = json!(["system-map.json", "package.json"]);
    }
    let (_dir, root) = tree(&map, files);
    report(&root)
}

/// The edges `barrel_report` gives with no links, so every edge is undeclared.
fn barrel_edges(files: &[(&str, &str)]) -> Vec<String> {
    let report = barrel_report(files, json!([]));
    assert_eq!(other_problems(&report), Vec::<String>::new());
    edges(&report)
}

#[test]
fn a_named_import_through_a_barrel_is_also_an_edge_to_the_file_that_exports_the_name() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { A } from '../entry';\n"),
        (
            "entry/index.ts",
            "\"use client\";\nexport { A } from '../one/a';\nexport { B } from '../two/b';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
        ("two/b.ts", "export const B = 2;\n"),
    ]);
    assert_eq!(found, ["app>entry", "app>one", "entry>one", "entry>two"]);
}

#[test]
fn an_aliased_re_export_and_an_imported_name_exported_again_are_followed() {
    for barrel in [
        "export { default as A } from '../one/a';\nexport { B as C } from '../two/b';\n",
        "import A from '../one/a';\nimport { B } from '../two/b';\nexport { A, B as C };\n",
    ] {
        let found = barrel_edges(&[
            ("app/x.ts", "import { C } from '../entry';\n"),
            ("entry/index.ts", barrel),
            ("one/a.ts", "export default 1;\n"),
            ("two/b.ts", "export const B = 2;\n"),
        ]);
        assert_eq!(
            found,
            ["app>entry", "app>two", "entry>one", "entry>two"],
            "{barrel}"
        );
    }
}

#[test]
fn a_type_import_through_a_type_re_export_is_followed() {
    let found = barrel_edges(&[
        ("app/x.ts", "import type { T } from '../entry';\n"),
        (
            "entry/index.ts",
            "export type { T } from '../one/a';\nexport { type U, B } from '../two/b';\n\
             export { default } from '../three/c';\n",
        ),
        ("one/a.ts", "export type T = number;\n"),
        ("two/b.ts", "export type U = string;\nexport const B = 2;\n"),
        ("three/c.ts", "export default 3;\n"),
    ]);
    assert_eq!(
        found,
        [
            "app>entry",
            "app>one",
            "entry>one",
            "entry>three",
            "entry>two"
        ]
    );
}

#[test]
fn a_name_found_only_behind_export_star_goes_to_every_export_star_target() {
    let files = |import: &str| {
        barrel_edges(&[
            ("app/x.ts", import),
            (
                "entry/index.ts",
                "export { A } from '../one/a';\nexport * from '../two/b';\nexport * from '../three/c';\n",
            ),
            ("one/a.ts", "export const A = 1;\n"),
            ("two/b.ts", "export const B = 2;\n"),
            ("three/c.ts", "export const C = 3;\n"),
        ])
    };
    let entry = ["entry>one", "entry>three", "entry>two"];
    assert_eq!(
        files("import { B } from '../entry';\n"),
        [&["app>entry", "app>three", "app>two"][..], &entry].concat()
    );
    assert_eq!(
        files("import { A } from '../entry';\n"),
        [&["app>entry", "app>one"][..], &entry].concat()
    );
}

#[test]
fn a_namespace_import_of_a_barrel_goes_to_every_target() {
    let found = barrel_edges(&[
        ("app/x.ts", "import * as all from '../entry';\n"),
        (
            "entry/index.ts",
            "export { A } from '../one/a';\nexport * from '../two/b';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
        ("two/b.ts", "export const B = 2;\n"),
    ]);
    assert_eq!(
        found,
        ["app>entry", "app>one", "app>two", "entry>one", "entry>two"]
    );
}

#[test]
fn an_import_through_two_chained_barrels_reaches_the_file_behind_both() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { A } from '../entry';\n"),
        ("entry/index.ts", "export { A } from '../two';\n"),
        (
            "two/index.ts",
            "export { A } from '../one/a';\nexport { B } from '../three/b';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
        ("three/b.ts", "export const B = 2;\n"),
    ]);
    assert_eq!(
        found,
        [
            "app>entry",
            "app>one",
            "entry>one",
            "entry>two",
            "two>one",
            "two>three"
        ]
    );
}

#[test]
fn two_barrels_that_import_each_other_end_and_each_gets_its_edges() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { B } from '../entry';\n"),
        (
            "entry/index.ts",
            "export * from '../two';\nexport { A } from '../one/a';\n",
        ),
        (
            "two/index.ts",
            "export * from '../entry';\nexport { B } from '../three/b';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
        ("three/b.ts", "export const B = 2;\n"),
    ]);
    assert_eq!(
        found,
        [
            "app>entry",
            "app>three",
            "entry>one",
            "entry>three",
            "entry>two",
            "two>entry",
            "two>one",
            "two>three"
        ]
    );
}

#[test]
fn a_file_with_code_of_its_own_beside_its_re_exports_is_not_followed() {
    for code in [
        "export const version = 1;\n",
        "import '../three/side';\n",
        "export default function f() {}\n",
    ] {
        let text = format!("import {{ A }} from '../one/a';\nexport {{ A }};\n{code}");
        let found = barrel_edges(&[
            ("app/x.ts", "import { A } from '../entry';\n"),
            ("entry/index.ts", &text),
            ("one/a.ts", "export const A = 1;\n"),
            ("three/side.ts", ""),
        ]);
        let entry: &[&str] = if code.contains("three") {
            &["entry>one", "entry>three"]
        } else {
            &["entry>one"]
        };
        assert_eq!(found, [&["app>entry"][..], entry].concat(), "{code}");
    }
}

#[test]
fn a_part_whose_only_file_is_a_barrel_keeps_its_edges_and_is_read() {
    let links = json!([
        import("app-entry", "app", "entry"),
        import("entry-one", "entry", "one"),
        import("app-one", "app", "one")
    ]);
    let report = barrel_report(
        &[
            ("app/x.ts", "import { A } from '../entry';\n"),
            ("entry/index.ts", "export { A } from '../one/a';\n"),
            ("one/a.ts", "export const A = 1;\n"),
        ],
        links,
    );
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(
        (report.counts.links.proven, report.counts.links.unproven),
        (3, 0)
    );
}

#[test]
fn an_import_of_a_workspace_package_is_not_followed_past_its_package_json() {
    let found = barrel_edges(&[
        ("package.json", "{ \"workspaces\": [\"entry/*\"] }"),
        ("app/x.ts", "import { A } from '@chery/contract';\n"),
        (
            "entry/contract/package.json",
            "{ \"name\": \"@chery/contract\", \"main\": \"src/index.ts\" }",
        ),
        (
            "entry/contract/src/index.ts",
            "export { A } from '../../../one/a';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
    ]);
    assert_eq!(found, ["app>entry", "entry>one"]);
}

#[test]
fn a_braced_clause_ends_at_its_brace_without_semicolons() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { A } from '../entry'\n"),
        (
            "entry/index.ts",
            "import { A } from '../one/a'\nexport { A }\nexport * from '../two/x'\n",
        ),
        ("one/a.ts", "export const A = 1\n"),
        ("two/x.ts", "export const X = 2\n"),
    ]);
    assert_eq!(found, ["app>entry", "app>one", "entry>one", "entry>two"]);

    let found = barrel_edges(&[
        ("app/x.ts", "import { B } from '../entry'\n"),
        (
            "entry/index.ts",
            "import {A} from '../one/a'\nimport {B} from '../two/b'\nexport { A }\nexport { B }\n",
        ),
        ("one/a.ts", "export const A = 1\n"),
        ("two/b.ts", "export const B = 2\n"),
    ]);
    assert_eq!(found, ["app>entry", "app>two", "entry>one", "entry>two"]);
}

#[test]
fn an_imported_type_or_default_exported_again_is_a_barrel_that_is_followed() {
    for (import, barrel) in [
        (
            "import type { A } from '../entry';\n",
            "import type { A } from '../one/a';\nexport type { A };\n",
        ),
        (
            "import A from '../entry';\n",
            "import A from '../one/a';\nexport default A;\n",
        ),
    ] {
        let found = barrel_edges(&[
            ("app/x.ts", import),
            ("entry/index.ts", barrel),
            ("one/a.ts", "export default 1;\n"),
        ]);
        assert_eq!(found, ["app>entry", "app>one", "entry>one"], "{barrel}");
    }
}

#[test]
fn a_file_with_an_import_no_re_export_uses_is_not_followed() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { A } from '../entry';\n"),
        (
            "entry/index.ts",
            "import { B } from '../two/b';\nexport { A } from '../one/a';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
        ("two/b.ts", "export const B = 2;\n"),
    ]);
    assert_eq!(found, ["app>entry", "entry>one", "entry>two"]);
}

#[test]
fn a_default_import_is_not_followed_through_export_star() {
    let found = barrel_edges(&[
        ("app/x.ts", "import A from '../entry';\n"),
        ("entry/index.ts", "export * from '../one/a';\n"),
        ("one/a.ts", "export default 1;\n"),
    ]);
    assert_eq!(found, ["app>entry", "entry>one"]);
}

#[test]
fn a_slash_after_an_increment_or_decrement_is_a_division() {
    for text in [
        "let i = 0;\ni++ / 2; import a from '../beta/b.ts';\n",
        "let i = 0;\ni-- / 2; import a from '../beta/b.ts';\n",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, ["alpha>beta"], "{text}");
    }
}

#[test]
fn a_binding_named_by_a_string_keeps_its_edge() {
    for text in [
        "import { \"a-b\" as c } from '../beta/b.ts';\n",
        "export { x as \"y\" } from '../beta/b.ts';\n",
    ] {
        let found = ts_edges(&[("alpha/a.ts", text), ("beta/b.ts", "")]);
        assert_eq!(found, ["alpha>beta"], "{text}");
    }
}

#[test]
fn a_pnpm_flow_list_and_a_package_json_with_a_bom_declare_the_workspace() {
    for (config, text) in [
        ("pnpm-workspace.yaml", "packages: ['beta/*']\n"),
        (
            "pnpm-workspace.yaml",
            "packages: [\"other/*\", \"beta/*\"]\n",
        ),
        ("package.json", "\u{feff}{ \"workspaces\": [\"beta/*\"] }"),
    ] {
        let found = ws_report(&[
            (config, text),
            ("alpha/a.ts", "import { x } from '@chery/contract';\n"),
            (CONTRACT.0, &format!("\u{feff}{}", CONTRACT.1)),
        ]);
        assert_eq!(found, (vec!["alpha>beta".to_string()], vec![]), "{text}");
    }
}

#[test]
fn a_barrel_inside_another_workspace_package_is_not_followed() {
    let found = barrel_edges(&[
        ("package.json", "{ \"workspaces\": [\"entry/*\"] }"),
        (
            "app/x.ts",
            "import { A } from '../entry/contract/src/index';\n",
        ),
        (
            "entry/contract/package.json",
            "{ \"name\": \"@chery/contract\" }",
        ),
        (
            "entry/contract/src/index.ts",
            "export { A } from '../../../one/a';\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
    ]);
    assert_eq!(found, ["app>entry", "entry>one"]);
}

#[test]
fn an_export_default_without_a_semicolon_before_another_re_export_is_a_barrel() {
    for next in [
        "export * from '../two/b'\n",
        "export { B } from '../two/b'\n",
    ] {
        let text = format!("import A from '../one/a'\nexport default A\n{next}");
        let found = barrel_edges(&[
            ("app/x.ts", "import A from '../entry';\n"),
            ("entry/index.ts", &text),
            ("one/a.ts", "export default 1;\n"),
            ("two/b.ts", "export const B = 2;\n"),
        ]);
        assert_eq!(
            found,
            ["app>entry", "app>one", "entry>one", "entry>two"],
            "{next}"
        );
    }
}

#[test]
fn a_barrel_starting_with_a_byte_order_mark_is_followed() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { A } from '../entry';\n"),
        ("entry/index.ts", "\u{feff}export * from '../one/a';\n"),
        ("one/a.ts", "export const A = 1;\n"),
    ]);
    assert_eq!(found, ["app>entry", "app>one", "entry>one"]);
}

#[test]
fn a_file_with_an_import_that_binds_nothing_is_not_followed() {
    let found = barrel_edges(&[
        ("app/x.ts", "import { A } from '../entry';\n"),
        (
            "entry/index.ts",
            "import {} from '../two/b'\nexport * from '../one/a'\n",
        ),
        ("one/a.ts", "export const A = 1;\n"),
        ("two/b.ts", ""),
    ]);
    assert_eq!(found, ["app>entry", "entry>one", "entry>two"]);
}

#[test]
fn a_test_file_importing_another_part_gives_no_edge() {
    for (file, import) in [
        ("app/x.test.ts", "../one/a"),
        ("app/x.spec.tsx", "../one/a"),
        ("app/x.test.mjs", "../one/a.ts"),
        ("app/__tests__/x.ts", "../../one/a"),
        ("app/test/x.ts", "../../one/a"),
        ("app/tests/x.mjs", "../../one/a.ts"),
        ("app/src/__tests__/deep/x.ts", "../../../../one/a"),
    ] {
        let text = format!("import {{ A }} from '{import}';\n");
        let found = barrel_edges(&[
            ("app/x.ts", "export const X = 1;\n"),
            (file, &text),
            ("one/a.ts", "export const A = 1;\n"),
        ]);
        assert_eq!(found, Vec::<String>::new(), "{file}");
    }
}

#[test]
fn a_part_made_only_of_test_files_is_not_read() {
    // Were `app` read, its link to `one` would have no edge and fail.
    let report = barrel_report(
        &[
            ("app/x.test.ts", "import { A } from '../one/a';\n"),
            ("app/tests/helpers.ts", "export const H = 1;\n"),
            ("one/a.ts", "export const A = 1;\n"),
        ],
        json!([import("app-one", "app", "one")]),
    );
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(
        (report.counts.links.proven, report.counts.links.unproven),
        (0, 1)
    );
}

#[test]
fn a_file_whose_name_only_looks_like_a_test_keeps_its_edges() {
    for (file, import) in [
        ("app/x.ts", "../one/a"),
        ("app/test.ts", "../one/a"),
        ("app/latest.ts", "../one/a"),
        ("app/test-utils.ts", "../one/a"),
        ("app/x.testing.ts", "../one/a"),
        ("app/testing/x.ts", "../../one/a"),
        ("app/contest/x.ts", "../../one/a"),
    ] {
        let text = format!("import {{ A }} from '{import}';\n");
        let found = barrel_edges(&[(file, &text), ("one/a.ts", "export const A = 1;\n")]);
        assert_eq!(found, ["app>one"], "{file}");
    }
}
