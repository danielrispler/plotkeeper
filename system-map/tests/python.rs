//! `python-imports`: Python repositories in temporary folders, read through `check` and `imports` (spec #589, "Testing
//! Decisions").

use std::{path::Path, process::Command};

use serde_json::{Value, json};

use crate::common::{has, project_text, write};

/// A map naming `python-imports` whose parts are `parts`, each an id with its paths, and whose import links are `links`.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)]) -> Value {
    json!({
        "version": 2,
        "extractors": ["python-imports"],
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

/// Every file pair of the computed edges, as `from>to`, sorted.
fn pairs(report: &system_map::Report) -> Vec<String> {
    let mut found: Vec<String> = report
        .edges
        .iter()
        .flat_map(|e| e.pairs.iter().map(|p| format!("{}>{}", p.from, p.to)))
        .collect();
    found.sort();
    found
}

/// A map in which each `.py` file is a part of its own, so that every pair crosses parts, and the other files are one
/// part, with no link: only the pairs matter.
fn each_file_a_part(files: &[(&str, &str)]) -> Value {
    let python: Vec<&str> = files
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| f.ends_with(".py"))
        .collect();
    let ids: Vec<String> = (0..python.len()).map(|i| format!("f{i}")).collect();
    let mut parts: Vec<(&str, Vec<&str>)> = ids
        .iter()
        .zip(&python)
        .map(|(id, file)| (id.as_str(), vec![*file]))
        .collect();
    let other: Vec<&str> = files
        .iter()
        .map(|(f, _)| *f)
        .filter(|f| !f.ends_with(".py"))
        .collect();
    if !other.is_empty() {
        parts.push(("other", other));
    }
    let parts: Vec<(&str, &[&str])> = parts.iter().map(|(id, f)| (*id, f.as_slice())).collect();
    map(&parts, &[])
}

/// The file pairs a repository of `files` gives.
fn py_pairs(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&each_file_a_part(files), files);
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

const PYPROJECT: (&str, &str) = ("pyproject.toml", "[project]\nname = \"m\"\n");

#[test]
fn an_import_pairs_with_the_module_file_under_the_repository_root_with_no_config() {
    let found = py_pairs(&[
        ("app.py", "import lib\n"),
        ("lib.py", "x = 1\n"),
        ("other.py", "x = 1\n"),
    ]);
    assert_eq!(found, ["app.py>lib.py"]);
}

#[test]
fn a_folder_holding_a_config_is_a_root() {
    let found = py_pairs(&[
        ("p/pyproject.toml", PYPROJECT.1),
        ("p/m.py", "import n\n"),
        ("p/n.py", "x = 1\n"),
    ]);
    assert_eq!(found, ["p/m.py>p/n.py"]);
    for config in ["setup.cfg", "setup.py"] {
        let found = py_pairs(&[
            (&format!("p/{config}"), ""),
            ("p/m.py", "import n\n"),
            ("p/n.py", "x = 1\n"),
        ]);
        assert_eq!(found, ["p/m.py>p/n.py"], "{config}");
    }
}

#[test]
fn a_script_folder_with_no_config_is_not_a_root() {
    let found = py_pairs(&[("tools/a.py", "import b\n"), ("tools/b.py", "x = 1\n")]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn every_project_folder_of_a_monorepo_is_a_root() {
    let found = py_pairs(&[
        ("libs/a/pyproject.toml", PYPROJECT.1),
        ("libs/b/pyproject.toml", PYPROJECT.1),
        ("libs/a/apkg/__init__.py", "import bpkg\n"),
        ("libs/b/bpkg/__init__.py", "import apkg\n"),
    ]);
    assert_eq!(
        found,
        [
            "libs/a/apkg/__init__.py>libs/b/bpkg/__init__.py",
            "libs/b/bpkg/__init__.py>libs/a/apkg/__init__.py"
        ]
    );
}

#[test]
fn a_roots_src_folder_is_a_root_with_or_without_a_config() {
    let found = py_pairs(&[
        ("app.py", "import pkg\n"),
        ("src/pkg/__init__.py", "x = 1\n"),
    ]);
    assert_eq!(found, ["app.py>src/pkg/__init__.py"]);
    let found = py_pairs(&[
        ("proj/pyproject.toml", PYPROJECT.1),
        ("proj/app.py", "import pkg\n"),
        ("proj/src/pkg/__init__.py", "x = 1\n"),
    ]);
    assert_eq!(found, ["proj/app.py>proj/src/pkg/__init__.py"]);
}

#[test]
fn a_config_under_an_excluded_folder_or_an_ignored_path_makes_no_root() {
    for folder in ["tests", "test", "testdata", "fixtures", "x/fixtures"] {
        let config = format!("{folder}/d/pyproject.toml");
        let helper = format!("{folder}/d/helper.py");
        let files = [
            (config.as_str(), PYPROJECT.1),
            (helper.as_str(), "x = 1\n"),
            ("app.py", "import helper\n"),
        ];
        assert_eq!(py_pairs(&files), Vec::<String>::new(), "{folder}");
    }
    // The same files under a folder that is not excluded do make a root.
    let found = py_pairs(&[
        ("data/d/pyproject.toml", PYPROJECT.1),
        ("data/d/helper.py", "x = 1\n"),
        ("app.py", "import helper\n"),
    ]);
    assert_eq!(found, ["app.py>data/d/helper.py"]);
}

#[test]
fn a_config_in_a_path_the_map_ignores_makes_no_root() {
    let mut ignoring = each_file_a_part(&[("app.py", ""), ("vendored/d/helper.py", "")]);
    ignoring["ignored"] = json!(["system-map.json", "vendored/d/pyproject.toml"]);
    let (_dir, root) = repo(
        &ignoring,
        &[
            ("vendored/d/pyproject.toml", PYPROJECT.1),
            ("vendored/d/helper.py", "x = 1\n"),
            ("app.py", "import helper\n"),
        ],
    );
    assert_eq!(pairs(&report(&root)), Vec::<String>::new());
}

#[test]
fn import_a_b_c_pairs_with_both_package_files_and_the_module() {
    let found = py_pairs(&[
        ("app.py", "import a.b.c\n"),
        ("a/__init__.py", ""),
        ("a/b/__init__.py", ""),
        ("a/b/c.py", ""),
        ("a/b/d.py", ""),
    ]);
    assert_eq!(
        found,
        [
            "app.py>a/__init__.py",
            "app.py>a/b/__init__.py",
            "app.py>a/b/c.py"
        ]
    );
}

#[test]
fn from_a_b_import_c_pairs_with_the_package_and_with_c_when_it_is_a_submodule() {
    let name = py_pairs(&[
        ("app.py", "from a.b import c, d\n"),
        ("a/__init__.py", ""),
        ("a/b/__init__.py", "c = 1\n"),
    ]);
    assert_eq!(name, ["app.py>a/__init__.py", "app.py>a/b/__init__.py"]);
    let submodule = py_pairs(&[
        ("app.py", "from a.b import c, d\n"),
        ("a/__init__.py", ""),
        ("a/b/__init__.py", ""),
        ("a/b/c.py", ""),
        ("a/b/d/__init__.py", ""),
    ]);
    assert_eq!(
        submodule,
        [
            "app.py>a/__init__.py",
            "app.py>a/b/__init__.py",
            "app.py>a/b/c.py",
            "app.py>a/b/d/__init__.py"
        ]
    );
}

#[test]
fn a_star_import_pairs_with_the_modules_init_file_only() {
    let found = py_pairs(&[
        ("app.py", "from a import *\n"),
        ("a/__init__.py", "__all__ = ['sub']\n"),
        ("a/sub.py", ""),
    ]);
    assert_eq!(found, ["app.py>a/__init__.py"]);
}

#[test]
fn an_import_skips_the_init_file_of_a_package_holding_the_importer() {
    // Inside `a/`, `from a.x import y` gives what `from .x import y` will: the package ran before the importer.
    let found = py_pairs(&[
        ("a/__init__.py", ""),
        ("a/m.py", "from a.x import y\n"),
        ("a/x.py", "y = 1\n"),
    ]);
    assert_eq!(found, ["a/m.py>a/x.py"]);
    let deeper = py_pairs(&[
        ("a/__init__.py", ""),
        ("a/b/__init__.py", ""),
        ("a/b/m.py", "import a.d.e\n"),
        ("a/d/__init__.py", ""),
        ("a/d/e.py", ""),
    ]);
    assert_eq!(deeper, ["a/b/m.py>a/d/__init__.py", "a/b/m.py>a/d/e.py"]);
}

#[test]
fn a_stub_is_a_target_only_when_no_py_module_or_package_exists() {
    let found = py_pairs(&[
        (
            "app.py",
            "import compiled\nimport both\nimport pkg\nimport stubs.s\n",
        ),
        ("compiled.pyi", ""),
        ("both.py", ""),
        ("both.pyi", ""),
        ("pkg/__init__.py", ""),
        ("pkg/__init__.pyi", ""),
        ("stubs/__init__.pyi", ""),
        ("stubs/s.pyi", ""),
    ]);
    assert_eq!(
        found,
        [
            "app.py>both.py",
            "app.py>compiled.pyi",
            "app.py>pkg/__init__.py",
            "app.py>stubs/__init__.pyi",
            "app.py>stubs/s.pyi"
        ]
    );
}

#[test]
fn a_package_beats_a_module_and_a_module_beats_a_bare_folder_in_one_root() {
    let found = py_pairs(&[
        ("app.py", "import p\nimport m\nimport p.q\n"),
        ("p/__init__.py", ""),
        ("p.py", ""),
        ("m.py", ""),
        ("m/n.py", ""),
    ]);
    assert_eq!(found, ["app.py>m.py", "app.py>p/__init__.py"]);
}

#[test]
fn a_bare_folder_gives_no_pair_of_its_own_but_its_files_do() {
    let found = py_pairs(&[("app.py", "import ns.mod\nimport ns\n"), ("ns/mod.py", "")]);
    assert_eq!(found, ["app.py>ns/mod.py"]);
}

#[test]
fn third_party_standard_library_and_missing_names_give_no_pair() {
    let found = py_pairs(&[
        (
            "app.py",
            "import numpy\nimport os.path\nimport a.missing\nfrom a import nothing\n",
        ),
        ("a/__init__.py", ""),
    ]);
    // `a.missing` walks to a name no file provides: the import fails, so it gives nothing.
    assert_eq!(found, ["app.py>a/__init__.py"]);
}

#[test]
fn a_first_segment_two_roots_provide_gives_no_pair_and_no_problem() {
    let found = py_pairs(&[
        ("p/pyproject.toml", PYPROJECT.1),
        ("q/pyproject.toml", PYPROJECT.1),
        ("p/m.py", ""),
        ("q/m.py", ""),
        ("p/only.py", ""),
        ("p/user.py", "import m\nimport only\n"),
    ]);
    assert_eq!(found, ["p/user.py>p/only.py"]);
}

#[test]
fn a_relative_import_gives_no_pair_for_now() {
    let found = py_pairs(&[
        ("a/__init__.py", ""),
        (
            "a/m.py",
            "from . import x\nfrom .x import y\nfrom .. import z\n",
        ),
        ("a/x.py", ""),
    ]);
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn an_import_pairs_to_a_file_named_like_a_standard_library_module_as_a_local_one() {
    let found = py_pairs(&[("app.py", "import logging\n"), ("logging.py", "")]);
    assert_eq!(found, ["app.py>logging.py"]);
}

const IMPORTS_LIB: &str = "import lib\n";

#[test]
fn a_skipped_file_gives_no_pair_and_does_not_make_its_part_read() {
    for file in [
        "s/test_x.py",
        "s/x_test.py",
        "s/conftest.py",
        "s/tests/x.py",
        "s/test/x.py",
        "tests/x.py",
        "s/tests/deep/x.py",
        "s/x.pyi",
        "s/x.pyw",
        "s/x.ipynb",
    ] {
        let (_dir, root) = repo(
            &map(&[("lib", &["lib.py"]), ("s", &[file])], &[("s", "lib")]),
            &[("lib.py", "x = 1\n"), (file, IMPORTS_LIB)],
        );
        let found = report(&root);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{file}");
        assert_eq!(found.problems, Vec::<String>::new(), "{file}");
        assert_eq!(found.counts.links.unproven, 1, "{file}");
    }
    // A file the extractor reads, named like none of those, makes its part read and gives its pair.
    for file in [
        "s/x.py",
        "s/testing.py",
        "s/contest.py",
        "s/tests_x.py",
        "s/attest.py",
    ] {
        let (_dir, root) = repo(
            &map(&[("lib", &["lib.py"]), ("s", &[file])], &[("s", "lib")]),
            &[("lib.py", "x = 1\n"), (file, IMPORTS_LIB)],
        );
        let found = report(&root);
        assert_eq!(pairs(&found), [format!("{file}>lib.py")], "{file}");
        assert_eq!(found.counts.links.proven, 1, "{file}");
    }
}

#[test]
fn an_unparseable_pyproject_toml_is_a_problem_naming_the_file() {
    let (_dir, root) = repo(
        &each_file_a_part(&[("p/m.py", "")]),
        &[("p/pyproject.toml", "[project\nname = \n"), ("p/m.py", "")],
    );
    let found = report(&root).problems;
    assert!(
        has(&found, &["p/pyproject.toml", "not valid TOML"]),
        "{found:?}"
    );
}

#[test]
fn a_broken_config_under_an_excluded_folder_or_an_ignored_path_is_no_problem() {
    for folder in ["testdata", "tests", "test", "fixtures", "a/fixtures"] {
        let broken = format!("{folder}/pyproject.toml");
        let (_dir, root) = repo(
            &map(&[("all", &["**"])], &[]),
            &[(broken.as_str(), "[project\nname = \n"), ("m.py", "")],
        );
        assert_eq!(problems(&root), Vec::<String>::new(), "{folder}");
    }
    let mut ignoring = map(&[("all", &["**"])], &[]);
    ignoring["ignored"] = json!(["system-map.json", "vendored/"]);
    let (_dir, root) = repo(
        &ignoring,
        &[
            ("vendored/pyproject.toml", "[project\nname = \n"),
            ("m.py", ""),
        ],
    );
    assert_eq!(problems(&root), Vec::<String>::new());
}

fn problems(root: &str) -> Vec<String> {
    report(root).problems
}

#[test]
fn a_repository_with_no_config_is_no_problem() {
    let (_dir, root) = repo(&each_file_a_part(&[("m.py", "")]), &[("m.py", "x = 1\n")]);
    assert_eq!(problems(&root), Vec::<String>::new());
}

#[test]
fn check_suggests_python_imports_only_for_a_repository_with_a_py_file_it_would_read() {
    let note = "note: python-imports could read this repository; add it to extractors\n";
    for (files, expected) in [
        (&[("a.py", "x = 1\n")][..], note),
        (&[("tests/a.py", "x = 1\n")], ""),
        (
            &[("test_a.py", "x = 1\n"), ("a.pyi", ""), ("a.ipynb", "")],
            "",
        ),
    ] {
        let mut map = map(&[("a", &["**"])], &[]);
        map["extractors"] = json!([]);
        let (_dir, root) = repo(&map, files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

/// Every import site of the computed edges, as `from>to file:line`.
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
fn a_pair_keeps_the_line_of_each_import_statement_that_makes_it() {
    let files = [
        (
            "app.py",
            "import os\nimport a.b\n\nfrom a import (\n    b,\n    c,\n)\ndef f():\n    import a.b as z\n",
        ),
        ("a/__init__.py", ""),
        ("a/b.py", ""),
        ("a/c.py", ""),
    ];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    assert_eq!(
        sites(&root),
        [
            "app.py>a/__init__.py app.py:2",
            "app.py>a/__init__.py app.py:4",
            "app.py>a/__init__.py app.py:9",
            "app.py>a/b.py app.py:2",
            "app.py>a/b.py app.py:4",
            "app.py>a/b.py app.py:9",
            "app.py>a/c.py app.py:4",
        ]
    );
}

/// Whether `app.py`, holding `text`, imports `lib.py`, at which lines.
fn imports_lib_at(text: &str) -> Vec<String> {
    let files = [("app.py", text), ("lib.py", "x = 1\n"), ("os.py", "")];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    sites(&root)
        .into_iter()
        .filter_map(|s| s.strip_prefix("app.py>lib.py app.py:").map(String::from))
        .collect()
}

#[test]
fn import_like_text_in_a_string_of_any_form_and_prefix_gives_no_pair() {
    let prefixes = [
        "", "r", "u", "b", "f", "t", "R", "U", "B", "F", "T", "br", "rb", "fr", "rf", "tr", "rt",
        "Br", "rB", "bR", "RB", "Fr", "rF", "TR", "Rt",
    ];
    for prefix in prefixes {
        for quote in ["'", "\"", "'''", "\"\"\""] {
            let one_line = format!("x = {prefix}{quote}import lib{quote}\n");
            assert_eq!(
                imports_lib_at(&one_line),
                Vec::<String>::new(),
                "{one_line:?}"
            );
            if quote.len() == 3 {
                let lines =
                    format!("x = {prefix}{quote}\nimport lib\nfrom lib import y\n{quote}\n");
                assert_eq!(imports_lib_at(&lines), Vec::<String>::new(), "{lines:?}");
            }
        }
    }
}

#[test]
fn the_text_of_a_nested_f_string_field_is_lexed_as_code_and_its_strings_as_strings() {
    for text in [
        // The inner string reuses the outer quote (PEP 701).
        "x = f\"{d[\"import lib\"]}\"\n",
        // A nested f-string, and a brace in a nested string that is no field end.
        "x = f\"{f'{y}' + '}'} import lib\"\n",
        "x = f\"\"\"{f\"{y!r:>{w}}\"}\nimport lib\n\"\"\"\n",
        "x = t'{x:{'import lib'}}'\n",
        // A `#` comment in a field of a triple-quoted f-string ends at the line, with a quote and a brace in it.
        "x = f\"\"\"{y # ' }\n}\nimport lib\n\"\"\"\n",
        // Doubled braces are text.
        "x = f'{{import lib}}'\n",
        "x = rf'\\{y}import lib'\n",
        "x = f'\\N{BULLET} import lib'\n",
    ] {
        assert_eq!(imports_lib_at(text), Vec::<String>::new(), "{text:?}");
    }
    // An import after such a string is read, at its own line.
    assert_eq!(
        imports_lib_at("x = f\"{d[\"a\"]} {f'{y}'}\"\nimport lib\n"),
        ["2"]
    );
}

#[test]
fn a_comment_or_a_word_that_only_contains_import_gives_no_pair() {
    for text in [
        "# import lib\n",
        "x = 1  # from lib import y\n",
        "important = 1\nimport_lib = 2\nreimport = lib\nobj.from_lib(lib)\nlib.import_x()\n",
        "from_lib = 1\nimport_ = 1\n",
        "def f():\n    '''Docs.\n    import lib\n    '''\n",
    ] {
        assert_eq!(imports_lib_at(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn from_in_raise_and_yield_is_never_read_as_an_import() {
    for text in [
        "raise E from lib\nimport os\n",
        "def g():\n    yield from lib\nimport os\n",
        "def g():\n    x = (yield from lib)\n    import os\n",
        "raise E from lib; import os\n",
        "try:\n    pass\nexcept E as e: raise F from lib\nimport os\n",
        "lazy = 1\nimport os\n",
        "lazy\nfrom lib\nimport os\n",
    ] {
        assert_eq!(imports_lib_at(text), Vec::<String>::new(), "{text:?}");
    }
}

#[test]
fn an_import_is_read_wherever_a_statement_can_start_and_at_the_line_it_starts() {
    for (text, line) in [
        ("import lib\n", "1"),
        ("import os, lib\n", "1"),
        ("import os as o, lib as l\n", "1"),
        ("from lib import y\n", "1"),
        ("from lib import (\n    y,\n    z,\n)\n", "1"),
        ("from lib import (y,\n    z)\n", "1"),
        ("from lib import y as z, w\n", "1"),
        ("from lib import *\n", "1"),
        ("x = 1\nimport \\\n    lib\n", "2"),
        ("from \\\n    lib \\\n    import y\n", "1"),
        ("x = 1; import lib\n", "1"),
        ("x = 1;import lib;y = 2\n", "1"),
        ("if TYPE_CHECKING:\n    import lib\n", "2"),
        (
            "try:\n    import lib\nexcept ImportError:\n    lib = None\n",
            "2",
        ),
        ("try: import lib\nexcept ImportError: pass\n", "1"),
        ("def f():\n    import lib\n", "2"),
        ("class A: import lib\n", "1"),
        ("if x: import os\nelse: from lib import y\n", "2"),
        ("lazy import lib\n", "1"),
        ("lazy from lib import y\n", "1"),
        ("lazy = 1\nimport lib\n", "2"),
        ("x = \"\"\"\n\n\"\"\"\nimport lib\n", "4"),
        ("x = 'a\\\nb'\nimport lib\n", "3"),
        ("x = [\n    1,  # import os\n]\nimport lib\n", "4"),
        ("\u{feff}import lib\n", "1"),
        ("import lib\r\nimport os\r\n", "1"),
        ("import lib", "1"),
        ("with a as b: import lib\n", "1"),
        ("import os  # lib\nfrom lib import y  # import os\n", "2"),
    ] {
        assert_eq!(imports_lib_at(text), [line], "{text:?}");
    }
}

#[test]
fn from_future_gives_nothing_and_import_future_is_an_ordinary_import() {
    let files = [
        ("app.py", "from __future__ import annotations\n"),
        ("__future__.py", ""),
    ];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    assert_eq!(pairs(&report(&root)), Vec::<String>::new());
    let files = [("app.py", "import __future__\n"), ("__future__.py", "")];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    assert_eq!(pairs(&report(&root)), ["app.py>__future__.py"]);
}

#[test]
fn text_that_is_not_valid_python_gives_no_problem_and_never_a_guessed_pair() {
    let template = "from lib import y\n\
                    x = {{ cookiecutter.value }}\n\
                    {% if cookiecutter.flag %}\n\
                    import {{ cookiecutter.dep }}\n\
                    {% endif %}\n\
                    s = \"unterminated\n\
                    t = '''\n\
                    import os\n";
    let files = [
        ("{{cookiecutter.slug}}/app.py", template),
        ("lib.py", "x = 1\n"),
        ("os.py", ""),
        (
            "py2.py",
            "print \"x\"\nexec 'import lib'\nimport os, lib,\n",
        ),
    ];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    let found = report(&root);
    // The files own no links, so the only problems that may show are the missing ones.
    let broken: Vec<_> = found
        .problems
        .iter()
        .filter(|p| !p.contains("no import link runs"))
        .collect();
    assert_eq!(broken, Vec::<&String>::new());
    let found = pairs(&found);
    assert!(
        found.contains(&"{{cookiecutter.slug}}/app.py>lib.py".to_string()),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .all(|p| !p.ends_with(">os.py") && !p.starts_with("py2.py")),
        "{found:?}"
    );
}

fn imports_json(root: &str) -> Value {
    let out = run(root, "imports", &["--json"]);
    serde_json::from_slice(&out.stdout).unwrap()
}

/// `app/main.py` importing `a.b` twice, over a package `a/` whose `__init__.py` and `a/b/` sit in parts of their own.
fn nested_package(app: &str) -> (tempfile::TempDir, String) {
    repo(
        &map(
            &[
                ("app", &["app/"]),
                ("top", &["a/__init__.py"]),
                ("sub", &["a/b/"]),
            ],
            &[("app", "top"), ("app", "sub")],
        ),
        &[
            ("app/main.py", app),
            ("a/__init__.py", ""),
            ("a/b/__init__.py", ""),
            ("a/b/c.py", ""),
            ("a/b/d.py", ""),
            ("a/b/e.py", ""),
        ],
    )
}

#[test]
fn the_files_of_one_package_folder_an_importer_names_count_as_one_unit() {
    let (_dir, root) = nested_package("from a.b import c, d\nfrom a.b import e\n");
    let value = imports_json(&root);
    // One unit into `a/b/` (the folder of `__init__.py`, `c.py`, `d.py` and `e.py`) and one into `a/`.
    assert_eq!(value["total"]["raw"], 2, "{value}");
    assert_eq!(value["across"]["raw"], 2, "{value}");
    let between: Vec<(String, String, u64)> = value["between"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            (
                b["from"].as_str().unwrap().into(),
                b["to"].as_str().unwrap().into(),
                b["raw"].as_u64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        between,
        [
            ("app".to_string(), "sub".to_string(), 1),
            ("app".to_string(), "top".to_string(), 1)
        ]
    );
}

#[test]
fn an_import_from_inside_the_package_adds_no_unit_into_the_package_itself() {
    let (_dir, root) = repo(
        &map(
            &[
                ("top", &["a/__init__.py"]),
                ("sub", &["a/b/"]),
                ("in", &["a/m.py"]),
            ],
            &[("in", "sub")],
        ),
        &[
            ("a/__init__.py", ""),
            ("a/m.py", "from a.b import c\n"),
            ("a/b/__init__.py", ""),
            ("a/b/c.py", ""),
        ],
    );
    let value = imports_json(&root);
    assert_eq!(value["total"]["raw"], 1, "{value}");
}

#[test]
fn a_files_node_is_its_own_folders_slice_whichever_file_of_it_is_imported() {
    // Ten importers, each in a folder of its own, each importing one of three files of `lib/`: `lib/` is one node of
    // fan-in ten, named by its folder.
    let importers: Vec<(String, String)> = (0..10)
        .map(|i| (format!("app/c{i}/m.py"), format!("import lib.f{}\n", i % 3)))
        .collect();
    let mut files: Vec<(&str, &str)> = importers
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    files.extend([("lib/f0.py", ""), ("lib/f1.py", ""), ("lib/f2.py", "")]);
    let (_dir, root) = repo(
        &map(&[("app", &["app/"]), ("lib", &["lib/"])], &[("app", "lib")]),
        &files,
    );
    let value = imports_json(&root);
    assert_eq!(value["total"]["raw"], 10, "{value}");
    assert_eq!(value["mean_degree"], 1.82, "{value}");
    assert_eq!(
        value["hubs"],
        json!([{"file": "lib/", "part": "lib", "degree": 10, "fan_in": 10, "fan_out": 0}])
    );
    let text = String::from_utf8(run(&root, "imports", &[]).stdout).unwrap();
    assert!(text.contains("  lib/: degree 10, 10 in, 0 out\n"), "{text}");
}

#[test]
fn a_mixed_python_typescript_and_go_repository_reads_each_languages_parts_side_by_side() {
    let mut mixed = map(
        &[
            ("pylib", &["pylib.py"]),
            ("pyapp", &["pyapp.py"]),
            ("golib", &["go.mod", "lib/"]),
            ("goapp", &["a/"]),
            ("tslib", &["web/lib/"]),
            ("tsapp", &["web/app/"]),
        ],
        &[("pyapp", "pylib"), ("goapp", "golib"), ("tsapp", "tslib")],
    );
    mixed["extractors"] = json!(["go-imports", "python-imports", "typescript-imports"]);
    let (_dir, root) = repo(
        &mixed,
        &[
            ("go.mod", "module example.com/m\n\ngo 1.22\n"),
            (
                "a/main.go",
                "package main\n\nimport \"example.com/m/lib\"\n",
            ),
            ("lib/x.go", "package lib\n"),
            ("pyapp.py", "import pylib\n"),
            ("pylib.py", "x = 1\n"),
            ("web/app/main.ts", "import { x } from \"../lib/x\";\n"),
            ("web/lib/x.ts", "export const x = 1;\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        pairs(&found),
        [
            "a/main.go>lib/x.go",
            "pyapp.py>pylib.py",
            "web/app/main.ts>web/lib/x.ts"
        ]
    );
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (3, 0)
    );
}

#[test]
fn commit_mode_gives_the_edges_of_the_working_tree() {
    let (dir, root) = nested_package("from a.b import c, d\nimport a.b.e\n");
    write(
        dir.path(),
        "proj/pyproject.toml",
        "[project]\nname = \"p\"\n",
    );
    write(dir.path(), "proj/src/pkg/__init__.py", "import other\n");
    write(dir.path(), "proj/other.py", "");
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "More"]);
    let tree = run(&root, "check", &["--json"]);
    let commit = run(&root, "check", &["--json", "--commit", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
    let value: Value = serde_json::from_slice(&commit.stdout).unwrap();
    assert!(
        value["edges"].as_array().is_some_and(|e| e.len() >= 2),
        "{value}"
    );
    assert!(
        value.to_string().contains("a/b/e.py") && value.to_string().contains("proj/other.py"),
        "{value}"
    );
    // An edit not committed changes nothing at the commit.
    write(dir.path(), "app/main.py", "x = 1\n");
    assert_eq!(
        String::from_utf8_lossy(&run(&root, "check", &["--json", "--commit", "HEAD"]).stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
}
