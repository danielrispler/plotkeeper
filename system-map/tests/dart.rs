//! `dart-imports`: Dart repositories in temporary folders, read through `check` and `imports` (spec #621, "Testing
//! Decisions"). `package:` URIs are the next ticket's (#628), so only relative imports and parts are tried here.

use std::{path::Path, process::Command};

use serde_json::{Value, json};

use crate::common::{has, project_text, write};

/// A map naming `dart-imports` whose parts are `parts`, each an id with its paths, and whose import links are `links`.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)]) -> Value {
    json!({
        "version": 2,
        "extractors": ["dart-imports"],
        "parts": parts.iter().map(|(id, paths)| json!({
            "id": id, "name": id, "caption": id, "purpose": "A part.", "holds": [], "tier": "core", "paths": paths
        })).collect::<Vec<_>>(),
        "actors": [],
        "links": links.iter().map(|(from, to)| json!({
            "id": format!("{from}-{to}"), "from": from, "to": to, "kind": "import"
        })).collect::<Vec<_>>(),
        "ignored": ["system-map.json", "pubspec.yaml"],
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

/// The report of a repository of `files`, each Dart file a part of its own and the rest one more, with no link, so that
/// only the pairs matter.
fn dart_report(files: &[(&str, &str)]) -> system_map::Report {
    let mut parts: Vec<(String, Vec<&str>)> = vec![("rest".into(), vec!["**"])];
    for (i, (file, _)) in files.iter().enumerate() {
        if file.ends_with(".dart") {
            parts.push((format!("f{i}"), vec![file]));
        }
    }
    let parts: Vec<(&str, &[&str])> = parts
        .iter()
        .map(|(id, paths)| (id.as_str(), paths.as_slice()))
        .collect();
    let (_dir, root) = repo(&map(&parts, &[]), files);
    report(&root)
}

/// The file pairs of [`dart_report`], sorted.
fn dart_pairs(files: &[(&str, &str)]) -> Vec<String> {
    let mut found = pairs(&dart_report(files));
    found.sort();
    found
}

fn run(root: &str, command: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg(command)
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}

fn imports_json(root: &str) -> Value {
    let out = run(root, "imports", &["--json"]);
    serde_json::from_slice(&out.stdout).unwrap()
}

const PUBSPEC: (&str, &str) = ("pubspec.yaml", "name: app\n");

#[test]
fn a_relative_import_pairs_with_the_file_it_names_from_the_importers_folder() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/src/x.dart",
            "import 'y.dart';\nimport './z.dart';\nimport '../w.dart';\nimport 'sub/../v.dart';\nimport 'sub/u.dart';\n",
        ),
        ("a/lib/src/y.dart", ""),
        ("a/lib/src/z.dart", ""),
        ("a/lib/w.dart", ""),
        ("a/lib/src/v.dart", ""),
        ("a/lib/src/sub/u.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/lib/src/x.dart>a/lib/src/sub/u.dart",
            "a/lib/src/x.dart>a/lib/src/v.dart",
            "a/lib/src/x.dart>a/lib/src/y.dart",
            "a/lib/src/x.dart>a/lib/src/z.dart",
            "a/lib/src/x.dart>a/lib/w.dart",
        ]
    );
}

#[test]
fn export_pairs_like_import_and_every_uri_of_a_conditional_one_pairs() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/x.dart",
            "export 'e.dart' show E;\n\
             import 'h.dart' hide H;\n\
             import 'd.dart' deferred as d;\n\
             import 'stub.dart'\n  if (dart.library.io) 'io.dart'\n  if (dart.library.html) 'web.dart';\n\
             export 'm.dart' if (dart.library.js_interop) 'n.dart';\n",
        ),
        ("a/lib/e.dart", ""),
        ("a/lib/h.dart", ""),
        ("a/lib/d.dart", ""),
        ("a/lib/stub.dart", ""),
        ("a/lib/io.dart", ""),
        ("a/lib/web.dart", ""),
        ("a/lib/m.dart", ""),
        ("a/lib/n.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/lib/x.dart>a/lib/d.dart",
            "a/lib/x.dart>a/lib/e.dart",
            "a/lib/x.dart>a/lib/h.dart",
            "a/lib/x.dart>a/lib/io.dart",
            "a/lib/x.dart>a/lib/m.dart",
            "a/lib/x.dart>a/lib/n.dart",
            "a/lib/x.dart>a/lib/stub.dart",
            "a/lib/x.dart>a/lib/web.dart",
        ]
    );
}

#[test]
fn schemes_absolute_paths_empty_untracked_and_climbing_uris_give_no_pair() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/x.dart",
            "import 'dart:async';\nimport 'dart-ext:native';\nimport 'file:///a/lib/y.dart';\n\
             import 'http://example.com/y.dart';\nimport '/a/lib/y.dart';\nimport '';\n\
             import 'gone.dart';\nimport '../../../../y.dart';\nimport 'y%2Edart';\n\
             import 'package:app/y.dart';\nimport 'package:flutter/material.dart';\nimport 'ok.dart';\n",
        ),
        ("a/lib/y.dart", ""),
        ("a/lib/ok.dart", ""),
        ("y.dart", ""),
    ]);
    assert_eq!(found, ["a/lib/x.dart>a/lib/ok.dart"]);
}

#[test]
fn a_relative_import_proves_its_link_and_a_missing_one_fails() {
    let files = [
        PUBSPEC,
        ("a/lib/x.dart", "import '../../b/lib/y.dart';\n"),
        ("b/lib/y.dart", ""),
    ];
    let parts: &[(&str, &[&str])] = &[("a", &["a/"]), ("b", &["b/"])];
    let (_dir, root) = repo(&map(parts, &[("a", "b")]), &files);
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(found.counts.links.proven, 1);
    // The same import with no link in its direction fails, and so does a link with no import behind it.
    let (_dir, root) = repo(&map(parts, &[]), &files);
    assert!(
        has(
            &report(&root).problems,
            &["part \"a\" depends on part \"b\""]
        ),
        "{:?}",
        report(&root).problems
    );
    let (_dir, root) = repo(&map(parts, &[("b", "a")]), &files);
    assert!(
        has(&report(&root).problems, &["from \"b\" to \"a\""]),
        "{:?}",
        report(&root).problems
    );
}

#[test]
fn a_part_directive_makes_the_check_need_its_link_but_the_imports_report_leaves_it_out() {
    let files = [
        PUBSPEC,
        (
            "a/lib/x.dart",
            "library x;\n\npart '../../b/lib/x_part.dart';\n",
        ),
        ("b/lib/x_part.dart", "part of '../../a/lib/x.dart';\n"),
    ];
    let parts: &[(&str, &[&str])] = &[("a", &["a/"]), ("b", &["b/"])];
    let (_dir, root) = repo(&map(parts, &[]), &files);
    let found = report(&root);
    assert_eq!(pairs(&found), ["a/lib/x.dart>b/lib/x_part.dart"]);
    assert!(
        has(&found.problems, &["part \"a\" depends on part \"b\""]),
        "{:?}",
        found.problems
    );
    let (_dir, root) = repo(&map(parts, &[("a", "b")]), &files);
    assert_eq!(report(&root).problems, Vec::<String>::new());
    let value = imports_json(&root);
    assert_eq!(value["total"], json!({"raw": 0, "weighted": 0.0}));
}

#[test]
fn part_of_gives_no_pair_in_its_uri_or_its_library_name_form() {
    let found = dart_pairs(&[
        PUBSPEC,
        ("a/lib/p1.dart", "part of 'x.dart';\n"),
        ("a/lib/p2.dart", "part of x.y;\nimport 'x.dart';\n"),
        ("a/lib/x.dart", ""),
    ]);
    // Only the import after the second `part of` pairs: reading goes on after it.
    assert_eq!(found, ["a/lib/p2.dart>a/lib/x.dart"]);
}

#[test]
fn a_repository_with_no_pubspec_reads_its_relative_imports_with_no_problem() {
    let mut no_pubspec = map(&[("a", &["a/"])], &[]);
    no_pubspec["ignored"] = json!(["system-map.json"]);
    let (_dir, root) = repo(
        &no_pubspec,
        &[("a/lib/x.dart", "import 'y.dart';\n"), ("a/lib/y.dart", "")],
    );
    assert_eq!(report(&root).problems, Vec::<String>::new());
    assert_eq!(imports_json(&root)["total"]["raw"], 1);
}

#[test]
fn a_file_under_a_test_folder_gives_no_pair_and_does_not_make_its_part_read() {
    for file in [
        "s/test/x_test.dart",
        "test/x_test.dart",
        "packages/a/test/helpers/x.dart",
        "s/integration_test/x.dart",
        "s/test_driver/x.dart",
    ] {
        // An import of `lib/y.dart` from the file's own folder.
        let import = format!(
            "import '{}lib/y.dart';\n",
            "../".repeat(file.matches('/').count())
        );
        let (_dir, root) = repo(
            &map(&[("lib", &["lib/"]), ("s", &[file])], &[("s", "lib")]),
            &[PUBSPEC, ("lib/y.dart", ""), (file, &import)],
        );
        let found = report(&root);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{file}");
        assert_eq!(found.problems, Vec::<String>::new(), "{file}");
        assert_eq!(found.counts.links.unproven, 1, "{file}");
    }
}

#[test]
fn a_file_named_for_a_test_outside_the_test_folders_and_example_code_is_read() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/packages/bloc_test/lib/bloc_test.dart",
            "import 'src/helper.dart';\n",
        ),
        ("a/packages/bloc_test/lib/src/helper.dart", ""),
        ("a/example/lib/main.dart", "import 'util.dart';\n"),
        ("a/example/lib/util.dart", ""),
        ("a/lib/hit_test.dart", "import 'x.dart';\n"),
        ("a/lib/x.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/example/lib/main.dart>a/example/lib/util.dart",
            "a/lib/hit_test.dart>a/lib/x.dart",
            "a/packages/bloc_test/lib/bloc_test.dart>a/packages/bloc_test/lib/src/helper.dart",
        ]
    );
}

#[test]
fn test_folders_are_judged_below_the_packages_own_root() {
    let found = dart_pairs(&[
        ("a/pkgs/test/pubspec.yaml", "name: test\n"),
        ("a/pkgs/test/lib/test.dart", "import 'src/x.dart';\n"),
        ("a/pkgs/test/lib/src/x.dart", ""),
        (
            "a/pkgs/test/test/x_test.dart",
            "import '../lib/src/x.dart';\n",
        ),
        // A test folder below a nested package's root still counts, and the outer package's root does not apply.
        ("a/pkgs/other/pubspec.yaml", "name: other\n"),
        ("a/pkgs/other/lib/o.dart", "import 'p.dart';\n"),
        ("a/pkgs/other/lib/p.dart", ""),
        ("a/pkgs/other/test/o_test.dart", "import '../lib/p.dart';\n"),
    ]);
    assert_eq!(
        found,
        [
            "a/pkgs/other/lib/o.dart>a/pkgs/other/lib/p.dart",
            "a/pkgs/test/lib/test.dart>a/pkgs/test/lib/src/x.dart",
        ]
    );
}

#[test]
fn a_byte_order_mark_and_a_script_tag_are_skipped() {
    let found = dart_pairs(&[
        PUBSPEC,
        ("a/lib/bom.dart", "\u{feff}import 'y.dart';\n"),
        (
            "a/lib/script.dart",
            "#!/usr/bin/env dart\nimport 'y.dart';\n",
        ),
        (
            "a/lib/both.dart",
            "\u{feff}#!/usr/bin/env dart\n// Tool.\nimport 'y.dart';\n",
        ),
        ("a/lib/y.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/lib/bom.dart>a/lib/y.dart",
            "a/lib/both.dart>a/lib/y.dart",
            "a/lib/script.dart>a/lib/y.dart",
        ]
    );
}

#[test]
fn comments_that_nest_and_doc_comments_hide_the_directives_they_hold() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/x.dart",
            "// import 'n1.dart';\n/// import 'n2.dart';\n/// @docImport 'n3.dart';\n\
             /* import 'n4.dart'; */\n/** @docImport 'n5.dart'; */\n\
             /* outer /* inner */ import 'n6.dart'; */\n\
             /// A library.\nimport 'y.dart'; // import 'n7.dart';\n/* a * / b */ import 'z.dart';\n",
        ),
        ("a/lib/y.dart", ""),
        ("a/lib/z.dart", ""),
        ("a/lib/n1.dart", ""),
        ("a/lib/n2.dart", ""),
        ("a/lib/n3.dart", ""),
        ("a/lib/n4.dart", ""),
        ("a/lib/n5.dart", ""),
        ("a/lib/n6.dart", ""),
        ("a/lib/n7.dart", ""),
    ]);
    assert_eq!(
        found,
        ["a/lib/x.dart>a/lib/y.dart", "a/lib/x.dart>a/lib/z.dart"]
    );
}

#[test]
fn metadata_is_skipped_with_the_strings_inside_it_lexed_in_full() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/x.dart",
            "@deprecated\nimport 'a1.dart';\n\
             @a.b\n@C(\"import 'n1.dart';\")\nimport 'a2.dart';\n\
             @C<List<int>>(r'$ ) \\', k: '''\n) import 'n3.dart';\n''')\nimport 'a3.dart';\n\
             @C('${'}'} ${\"{\" /* } */}' ')')\nimport 'a4.dart';\n\
             @C(const [1, {2: (3)}])\n@D()\nexport 'a5.dart';\n",
        ),
        ("a/lib/a1.dart", ""),
        ("a/lib/a2.dart", ""),
        ("a/lib/a3.dart", ""),
        ("a/lib/a4.dart", ""),
        ("a/lib/a5.dart", ""),
        ("a/lib/n1.dart", ""),
        ("a/lib/n3.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/lib/x.dart>a/lib/a1.dart",
            "a/lib/x.dart>a/lib/a2.dart",
            "a/lib/x.dart>a/lib/a3.dart",
            "a/lib/x.dart>a/lib/a4.dart",
            "a/lib/x.dart>a/lib/a5.dart",
        ]
    );
}

#[test]
fn adjacent_and_raw_uris_are_joined_and_an_interpolated_one_gives_no_pair() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/x.dart",
            "import 'sub/' \"j1.dart\";\nimport r'r1.dart';\nimport '''t1.dart''';\n\
             import 'sub/' r'$j2.dart';\nexport 'sub/'\n  'e1.dart' if (dart.library.io == 'true') 'sub/' 'e2.dart';\n\
             import 'n/$v.dart';\nimport 'n/${v}.dart';\nimport 'sub/' 'n/$v.dart';\n",
        ),
        ("a/lib/sub/j1.dart", ""),
        ("a/lib/r1.dart", ""),
        ("a/lib/t1.dart", ""),
        ("a/lib/sub/$j2.dart", ""),
        ("a/lib/sub/e1.dart", ""),
        ("a/lib/sub/e2.dart", ""),
        ("a/lib/n/$v.dart", ""),
        ("a/lib/n/{v}.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/lib/x.dart>a/lib/r1.dart",
            "a/lib/x.dart>a/lib/sub/$j2.dart",
            "a/lib/x.dart>a/lib/sub/e1.dart",
            "a/lib/x.dart>a/lib/sub/e2.dart",
            "a/lib/x.dart>a/lib/sub/j1.dart",
            "a/lib/x.dart>a/lib/t1.dart",
        ]
    );
}

#[test]
fn reading_stops_at_the_first_declaration_and_goes_on_past_unquoted_imports() {
    let found = dart_pairs(&[
        PUBSPEC,
        (
            "a/lib/x.dart",
            "library x;\nimport foo;\nimport 'a1.dart';\nexport bar if (dart.library.io) baz;\n\
             import 'a2.dart';\n\nclass A {}\n\nimport 'n1.dart';\nconst s = \"import 'n2.dart';\";\n",
        ),
        (
            "a/lib/y.dart",
            "import 'a1.dart';\nconst s = '''\nimport 'n1.dart';\n''';\nimport 'n2.dart';\n",
        ),
        ("a/lib/a1.dart", ""),
        ("a/lib/a2.dart", ""),
        ("a/lib/n1.dart", ""),
        ("a/lib/n2.dart", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/lib/x.dart>a/lib/a1.dart",
            "a/lib/x.dart>a/lib/a2.dart",
            "a/lib/y.dart>a/lib/a1.dart",
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

#[test]
fn each_pair_keeps_the_line_of_its_directives_keyword_as_an_import_site() {
    let (_dir, root) = repo(
        &map(
            &[
                ("rest", &["**"]),
                ("x", &["x.dart"]),
                ("y", &["y.dart"]),
                ("z", &["z.dart"]),
            ],
            &[],
        ),
        &[
            (
                "x.dart",
                "// One.\n/* two\n three */ @deprecated\nimport\n  'y.dart'\n  if (dart.library.io) 'z.dart';\nexport 'y.dart';\npart 'z.dart';\n",
            ),
            ("y.dart", ""),
            ("z.dart", ""),
        ],
    );
    assert_eq!(
        sites(&root),
        [
            "x.dart>y.dart x.dart:4",
            "x.dart>y.dart x.dart:7",
            "x.dart>z.dart x.dart:4",
            "x.dart>z.dart x.dart:8",
        ]
    );
}

/// A repository whose part `s` holds `s/bad.dart`, with `header` as its text, and `extra`; part `lib` holds `lib/y.dart`,
/// and an import link runs from `s` to `lib`.
fn with_unlexable(header: &str, extra: &[(&str, &str)]) -> system_map::Report {
    let mut files = vec![PUBSPEC, ("lib/y.dart", ""), ("s/bad.dart", header)];
    files.extend_from_slice(extra);
    let (_dir, root) = repo(
        &map(&[("lib", &["lib/"]), ("s", &["s/"])], &[("s", "lib")]),
        &files,
    );
    report(&root)
}

#[test]
fn a_header_that_cannot_be_lexed_gives_no_pair_no_unsure_and_no_problem() {
    for header in [
        "import '../lib/y.dart';\n/* never closed\n",
        "/* outer /* inner */ import '../lib/y.dart';\n",
        "import '../lib/y.dart';\nimport '../lib/x.dart;\n",
        "import '../lib/y.dart';\nimport '''../lib/y.dart';\n",
        "@C('(' ] import '../lib/y.dart';\n",
        "@C(import '../lib/y.dart';\n",
        "import '../lib/y.dart';\nimport 'a.dart' if (dart.library.io 'b.dart';\n",
        "@C<int import '../lib/y.dart';\n",
        "import '../lib/y.dart';\nexport 'a${'}.dart';\n",
    ] {
        let found = with_unlexable(header, &[]);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{header}");
        assert_eq!(found.problems, Vec::<String>::new(), "{header}");
        assert_eq!(found.unsure, Vec::new(), "{header}");
        // The file's part is not read, so the link rests on nothing the extractor read.
        assert_eq!(found.counts.links.unproven, 1, "{header}");
    }
}

#[test]
fn a_link_resting_only_on_an_unlexable_file_fails_when_its_part_has_another_read_file() {
    let found = with_unlexable(
        "import '../lib/y.dart';\n/* never closed\n",
        &[("s/good.dart", "// Imports nothing.\n")],
    );
    assert!(
        has(&found.problems, &["link \"s-lib\"", "no dependency"]),
        "{:?}",
        found.problems
    );
}

#[test]
fn check_suggests_dart_imports_only_for_a_repository_with_a_production_dart_file() {
    let note = "note: dart-imports could read this repository; add it to extractors\n";
    let mut ignoring = map(&[("a", &["**"])], &[]);
    ignoring["extractors"] = json!([]);
    ignoring["ignored"] = json!(["system-map.json", "gen/"]);
    for (files, expected) in [
        (&[("a/a.go", "package a\n")][..], ""),
        (
            &[("test/a_test.dart", ""), ("integration_test/b.dart", "")],
            "",
        ),
        (&[("gen/a.dart", "")], ""),
        (&[("a/broken.dart", "/* never closed\n")], ""),
        (&[("test/a_test.dart", ""), ("lib/a.dart", "")], note),
        (&[("lib/hit_test.dart", "")], note),
    ] {
        let (_dir, root) = repo(&ignoring, files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

#[test]
fn commit_mode_gives_the_edges_of_the_working_tree() {
    let mut linked = map(&[("lib", &["lib/"]), ("app", &["app/"])], &[("app", "lib")]);
    linked["ignored"] = json!(["system-map.json"]);
    let (dir, root) = repo(
        &linked,
        &[
            ("app/main.dart", "import '../lib/y.dart';\npart 'p.dart';\n"),
            ("app/p.dart", "part of 'main.dart';\n"),
            ("lib/y.dart", "export 'z.dart';\n"),
            ("lib/z.dart", ""),
        ],
    );
    let tree = run(&root, "check", &["--json"]);
    let commit = run(&root, "check", &["--json", "--commit", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
    let value: Value = serde_json::from_slice(&commit.stdout).unwrap();
    assert_eq!(value["problems"], json!([]), "{value}");
    assert_eq!(
        value["edges"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| format!("{}>{}", e["from"], e["to"]))
            .collect::<Vec<_>>(),
        ["\"app\">\"lib\""],
        "{value}"
    );
    // An edit not committed changes nothing at the commit.
    write(dir.path(), "app/main.dart", "part 'p.dart';\n");
    assert_eq!(
        String::from_utf8_lossy(&run(&root, "check", &["--json", "--commit", "HEAD"]).stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
}
