//! `python-imports`: Python repositories in temporary folders, read through `check` and `imports` (spec #589, "Testing
//! Decisions").

use serde_json::{Value, json};

use crate::common::{
    assert_only_candidate_links_relaxed, extractor_map, has, imports_json, pairs as edge_pairs,
    repo, report, run, sites, write,
};

/// Every file pair of the computed edges, as `from>to`, sorted.
fn pairs(report: &system_map::Report) -> Vec<String> {
    let mut found = edge_pairs(report);
    found.sort();
    found
}

/// A map naming `python-imports` whose parts are `parts`, each an id with its paths, and whose import links are `links`.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)]) -> Value {
    extractor_map("python-imports", parts, links, &[])
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
fn a_config_under_an_excluded_folder_makes_no_root() {
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

/// A repository of `files` whose map ignores `ignored` beside itself, each `.py` file a part of its own.
fn ignoring(ignored: &[&str], files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let python: Vec<(&str, &str)> = files
        .iter()
        .copied()
        .filter(|(f, _)| f.ends_with(".py"))
        .collect();
    let mut map = each_file_a_part(&python);
    map["ignored"] = json!(
        ["system-map.json"]
            .iter()
            .chain(ignored)
            .collect::<Vec<_>>()
    );
    repo(&map, files)
}

#[test]
fn a_config_in_a_path_the_map_ignores_still_makes_a_root() {
    let (_dir, root) = ignoring(
        &["vendored/d/pyproject.toml"],
        &[
            ("vendored/d/pyproject.toml", PYPROJECT.1),
            ("vendored/d/helper.py", "x = 1\n"),
            ("app.py", "import helper\n"),
        ],
    );
    assert_eq!(pairs(&report(&root)), ["app.py>vendored/d/helper.py"]);
}

#[test]
fn an_ignored_config_declaring_lib_replaces_src_with_lib() {
    for (config, text) in [
        (
            "pyproject.toml",
            "[tool.setuptools.packages.find]\nwhere = [\"lib\"]\n",
        ),
        ("setup.cfg", "[options]\npackage_dir =\n    = lib\n"),
    ] {
        let (_dir, root) = ignoring(
            &[config],
            &[
                (config, text),
                ("lib/pkg/__init__.py", "x = 1\n"),
                ("src/other/__init__.py", "x = 1\n"),
                ("app.py", "import pkg\nimport other\n"),
            ],
        );
        assert_eq!(
            pairs(&report(&root)),
            ["app.py>lib/pkg/__init__.py"],
            "{config}"
        );
    }
}

#[test]
fn an_ignored_config_that_does_not_parse_gives_the_note_and_no_problem() {
    for (config, text, folder) in [
        ("pyproject.toml", "[project\nname = \n", "."),
        ("setup.cfg", "name = m\n[options]\n", "."),
        ("vendored/pyproject.toml", "[project\nname = \n", "vendored"),
    ] {
        let mut linked = map(
            &[("app", &["app.py"]), ("pkg", &["src/**"])],
            &[("app", "pkg")],
        );
        linked["ignored"] = json!(["system-map.json", config]);
        let (_dir, root) = repo(
            &linked,
            &[
                (config, text),
                ("src/pkg/__init__.py", "x = 1\n"),
                ("app.py", "import pkg\n"),
            ],
        );
        let found = report(&root);
        assert_eq!(found.problems, Vec::<String>::new(), "{config}");
        assert_eq!(
            found.notes,
            [format!(
                "python-imports did not read {config}, which does not parse; failing links into {folder} may be false"
            )],
            "{config}"
        );
        // The project keeps its default roots.
        assert_eq!(pairs(&found), ["app.py>src/pkg/__init__.py"], "{config}");
        assert_eq!(run(&root, "check", &[]).status.code(), Some(0), "{config}");
    }
}

#[test]
fn an_ignored_config_under_testdata_is_never_read() {
    let (_dir, root) = ignoring(
        &["testdata/"],
        &[
            ("testdata/d/pyproject.toml", PYPROJECT.1),
            ("testdata/d/helper.py", "x = 1\n"),
            ("testdata/e/pyproject.toml", "[project\nname = \n"),
            ("app.py", "import helper\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), Vec::<String>::new());
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(found.notes, Vec::<String>::new());
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
fn a_first_segment_two_roots_provide_gives_no_pair_and_makes_the_importer_unsure() {
    let files = [
        ("p/pyproject.toml", PYPROJECT.1),
        ("q/pyproject.toml", PYPROJECT.1),
        ("p/m.py", ""),
        ("q/m.py", ""),
        ("p/only.py", ""),
        ("p/user.py", "import m\nimport only\n"),
    ];
    assert_eq!(py_pairs(&files), ["p/user.py>p/only.py"]);
    assert_eq!(
        doubts(&files),
        ["p/user.py | m | two roots provide it | p/m.py, q/m.py"]
    );
}

#[test]
fn from_dot_import_pairs_with_the_packages_own_init_file_and_with_a_submodule() {
    let found = py_pairs(&[
        ("a/__init__.py", ""),
        ("a/m.py", "from . import x, y\n"),
        ("a/x.py", ""),
        // An `__init__.py` is its own package's anchor, and never pairs with itself.
        ("b/__init__.py", "from . import z\n"),
        ("b/z.py", ""),
    ]);
    assert_eq!(
        found,
        [
            "a/m.py>a/__init__.py",
            "a/m.py>a/x.py",
            "b/__init__.py>b/z.py"
        ]
    );
}

#[test]
fn from_dot_x_import_y_pairs_with_the_module_and_with_y_when_it_is_a_submodule() {
    let found = py_pairs(&[
        ("a/__init__.py", ""),
        ("a/m.py", "from .x import y, v\nfrom .w import u\n"),
        ("a/x/__init__.py", ""),
        ("a/x/y.py", ""),
        ("a/w.py", ""),
    ]);
    assert_eq!(
        found,
        ["a/m.py>a/w.py", "a/m.py>a/x/__init__.py", "a/m.py>a/x/y.py"]
    );
}

#[test]
fn the_dots_of_a_relative_import_add_up_and_each_one_past_the_first_climbs_a_folder() {
    let inits: Vec<(String, &str)> = ["a", "a/b", "a/b/c", "a/b/c/d"]
        .iter()
        .map(|f| (format!("{f}/__init__.py"), ""))
        .collect();
    let mut files: Vec<(&str, &str)> = inits.iter().map(|(f, t)| (f.as_str(), *t)).collect();
    files.extend([
        ("a/b/c/d/three.py", "from ...x import y\n"),
        ("a/b/c/d/two.py", "from . . import y\n"),
        ("a/b/c/d/four.py", "from .... import y\n"),
        ("a/b/c/d/mixed.py", "from .. . import y\n"),
        ("a/b/x.py", ""),
        ("a/b/c/y.py", ""),
        ("a/y.py", ""),
        ("a/b/y.py", ""),
    ]);
    assert_eq!(
        py_pairs(&files),
        // Past one dot, `from .. import y` never pairs with the `__init__.py` it climbs to: that package holds the
        // importer, so its init file ran before it.
        [
            "a/b/c/d/four.py>a/y.py",
            "a/b/c/d/mixed.py>a/b/y.py",
            "a/b/c/d/three.py>a/b/x.py",
            "a/b/c/d/two.py>a/b/c/y.py",
        ]
    );
}

#[test]
fn a_relative_and_an_absolute_import_inside_one_package_give_the_same_pairs() {
    let found = py_pairs(&[
        ("mypkg/__init__.py", ""),
        ("mypkg/m.py", "from .x import y\n"),
        ("mypkg/n.py", "from mypkg.x import y\n"),
        ("mypkg/x/__init__.py", ""),
        ("mypkg/x/y.py", ""),
    ]);
    assert_eq!(
        found,
        [
            "mypkg/m.py>mypkg/x/__init__.py",
            "mypkg/m.py>mypkg/x/y.py",
            "mypkg/n.py>mypkg/x/__init__.py",
            "mypkg/n.py>mypkg/x/y.py",
        ]
    );
}

const CLIMBS: &str = "climbs past the top package under some roots";

#[test]
fn a_climb_that_works_under_every_root_pairs() {
    // `p/pkg/m.py` is two folders below the repository root and one below `p/`: one dot works under both.
    let files = [
        ("p/pyproject.toml", "[project]\nname = \"p\"\n"),
        ("p/pkg/m.py", "from .x import y\n"),
        ("p/pkg/x.py", ""),
    ];
    assert_eq!(py_pairs(&files), ["p/pkg/m.py>p/pkg/x.py"]);
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_climb_that_works_under_some_roots_only_makes_the_importer_unsure() {
    // Two dots work under the repository root (depth 2) but not under `p/` (depth 1).
    let files = [
        ("p/pyproject.toml", "[project]\nname = \"p\"\n"),
        ("p/__init__.py", ""),
        ("p/pkg/m.py", "from ..x import y\nfrom .. import w\n"),
        ("p/x.py", ""),
        ("p/w.py", ""),
    ];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        [
            format!("p/pkg/m.py | .. | {CLIMBS} | p/w.py"),
            format!("p/pkg/m.py | ..x | {CLIMBS} | p/x.py"),
        ]
    );
}

#[test]
fn a_relative_unsure_import_relaxes_only_the_links_to_its_candidates_parts() {
    let files = [
        ("p/pyproject.toml", "[project]\nname = \"p\"\n"),
        ("p/pkg/m.py", "from ..x import y\n"),
        ("p/x.py", ""),
        ("z/z.py", ""),
    ];
    let map = map(
        &[
            ("pk", &["p/pkg/"]),
            ("px", &["p/x.py"]),
            ("zz", &["z/"]),
            ("other", &["p/pyproject.toml"]),
        ],
        &[("pk", "px"), ("pk", "zz")],
    );
    let (_dir, root) = repo(&map, &files);
    assert_only_candidate_links_relaxed(
        &root,
        "note: p/pkg/m.py is unsure of ..x; its links to px pass as unproven\n",
        "pk",
        "zz",
    );
}

#[test]
fn a_climb_that_works_under_no_root_leaves_the_repository_or_names_nothing_gives_nothing() {
    let files = [
        ("p/pyproject.toml", "[project]\nname = \"p\"\n"),
        ("top.py", "from . import y\n"),
        ("p/pkg/m.py", "from ...x import y\nfrom .... import y\n"),
        ("x.py", ""),
        ("y.py", ""),
        // Two dots work under the repository root only, but nothing is named `nothing`.
        (
            "p/pkg/n.py",
            "from ..nothing import y\nfrom .nothing import y\n",
        ),
    ];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_renamed_package_is_a_root_whose_top_package_is_its_name() {
    // `a/lib/` is the package `mypkg`: `a/lib/m.py` is `mypkg.m`, one folder deep, so two dots climb past its top
    // package, though they work under the repository root (`a.lib.m`).
    let config = pyproject("[tool.setuptools]\npackage-dir = {mypkg = \"a/lib\"}\n");
    let files = [
        ("pyproject.toml", config.as_str()),
        ("a/lib/m.py", "from ..x import y\nfrom . import n\n"),
        ("a/lib/n.py", ""),
        ("a/x.py", ""),
    ];
    assert_eq!(py_pairs(&files), ["a/lib/m.py>a/lib/n.py"]);
    assert_eq!(
        doubts(&files),
        [format!("a/lib/m.py | ..x | {CLIMBS} | a/x.py")]
    );
}

#[test]
fn a_relative_import_whose_target_does_not_exist_gives_nothing() {
    let files = [
        ("a/__init__.py", ""),
        (
            "a/m.py",
            "from .nothing import y\nfrom .nothing.deeper import z\n",
        ),
    ];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(doubts(&files), Vec::<String>::new());
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
fn a_broken_config_under_an_excluded_folder_is_no_problem() {
    for folder in ["testdata", "tests", "test", "fixtures", "a/fixtures"] {
        let broken = format!("{folder}/pyproject.toml");
        let (_dir, root) = repo(
            &map(&[("all", &["**"])], &[]),
            &[(broken.as_str(), "[project\nname = \n"), ("m.py", "")],
        );
        assert_eq!(problems(&root), Vec::<String>::new(), "{folder}");
    }
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
        sites(&root, &[]),
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

/// Whether `app.py`, holding `text`, imports `lib.py`, at which lines. The reader's text cases are unit tests beside
/// it (`src/extract/python/reader.rs`); these keep one case per shape through the whole check (#664).
fn imports_lib_at(text: &str) -> Vec<String> {
    let files = [("app.py", text), ("lib.py", "x = 1\n"), ("os.py", "")];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    sites(&root, &[])
        .into_iter()
        .filter_map(|s| s.strip_prefix("app.py>lib.py app.py:").map(String::from))
        .collect()
}

#[test]
fn import_like_text_in_a_string_gives_no_pair() {
    let text = "x = rb\"\"\"\nimport lib\nfrom lib import y\n\"\"\"\n";
    assert_eq!(imports_lib_at(text), Vec::<String>::new(), "{text:?}");
}

#[test]
fn an_import_after_a_nested_f_string_is_read_and_the_string_gives_no_pair() {
    assert_eq!(
        imports_lib_at("x = f\"{d[\"import lib\"]} {f'{y}'}\"\nimport lib\n"),
        ["2"]
    );
}

#[test]
fn an_import_gives_its_pair_at_the_line_it_starts() {
    assert_eq!(imports_lib_at("x = 1\nimport \\\n    lib\n"), ["2"]);
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
    // An ordinary import of the standard-library module: a tracked file of its name is a shadow, so the importer is
    // Unsure.
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        ["app.py | __future__ | shadows the standard library | __future__.py"]
    );
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

/// The Unsure findings of a repository of `files`, as `file | name | reason | candidates`, read from `check --json`;
/// each also prints its note on stderr.
fn doubts(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&each_file_a_part(files), files);
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    value["unsure"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|u| {
            let (file, name) = (u["file"].as_str().unwrap(), u["name"].as_str().unwrap());
            assert!(
                stderr.contains(&format!("note: {file} is unsure of {name};")),
                "{stderr}"
            );
            let candidates: Vec<&str> = u["candidates"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap())
                .collect();
            format!(
                "{file} | {name} | {} | {}",
                u["reason"].as_str().unwrap(),
                candidates.join(", ")
            )
        })
        .collect()
}

const P_AND_Q: [(&str, &str); 2] = [
    ("p/pyproject.toml", "[project]\nname = \"p\"\n"),
    ("q/pyproject.toml", "[project]\nname = \"q\"\n"),
];

fn with_roots<'a>(files: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    P_AND_Q.iter().chain(files).copied().collect()
}

#[test]
fn two_roots_providing_a_first_segment_make_the_importer_unsure_even_when_only_one_holds_the_final_file()
 {
    let files = with_roots(&[
        ("app.py", "import m.sub\n"),
        ("p/m/__init__.py", ""),
        ("p/m/sub.py", ""),
        ("q/m/__init__.py", ""),
    ]);
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        ["app.py | m.sub | two roots provide it | p/m/__init__.py, p/m/sub.py"]
    );
}

#[test]
fn an_unsure_import_relaxes_only_the_links_to_its_candidates_parts() {
    let files = with_roots(&[
        ("app/main.py", "import m.sub\n"),
        ("p/m/__init__.py", ""),
        ("p/m/sub.py", ""),
        ("q/m/__init__.py", ""),
        ("z/z.py", ""),
    ]);
    let map = map(
        &[
            ("app", &["app/"]),
            ("pp", &["p/"]),
            ("qq", &["q/"]),
            ("zz", &["z/"]),
            ("other", &["p/pyproject.toml", "q/pyproject.toml"]),
        ],
        &[("app", "pp"), ("app", "zz")],
    );
    let (_dir, root) = repo(&map, &files);
    assert_only_candidate_links_relaxed(
        &root,
        "note: app/main.py is unsure of m.sub; its links to pp pass as unproven\n",
        "app",
        "zz",
    );
}

#[test]
fn a_root_whose_walk_fails_leaves_no_candidates_and_so_no_finding() {
    let files = with_roots(&[
        ("app.py", "import m.nothing\n"),
        ("p/m/__init__.py", ""),
        ("q/m/__init__.py", ""),
    ]);
    assert_eq!(py_pairs(&files).len(), 0);
    // Each root still gets to run `m/__init__.py`, which makes `m.nothing` a failing import under both.
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn bare_folders_under_two_roots_merge_and_give_no_doubt_by_themselves() {
    let files = with_roots(&[
        (
            "app.py",
            "import ns.a\nimport ns.b\nfrom ns import a, b\nimport ns\n",
        ),
        ("p/ns/a.py", ""),
        ("q/ns/b.py", ""),
    ]);
    assert_eq!(py_pairs(&files), ["app.py>p/ns/a.py", "app.py>q/ns/b.py"]);
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_final_file_found_below_merged_folders_more_than_once_makes_the_importer_unsure() {
    let files = with_roots(&[
        ("app.py", "import ns.c\n"),
        ("p/ns/c.py", ""),
        ("q/ns/c.py", ""),
    ]);
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        ["app.py | ns.c | two roots provide it | p/ns/c.py, q/ns/c.py"]
    );
}

#[test]
fn a_name_imported_from_merged_folders_that_both_hold_it_makes_the_importer_unsure() {
    let files = with_roots(&[
        ("app.py", "from ns import c\n"),
        ("p/ns/c.py", ""),
        ("q/ns/c.py", ""),
    ]);
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        ["app.py | ns.c | two roots provide it | p/ns/c.py, q/ns/c.py"]
    );
}

#[test]
fn the_rule_applies_again_at_each_segment_below_merged_folders() {
    // `ns/sub` is a regular package under p and a bare folder under q: the package beats it, and the walk goes on only
    // in p. Under `ns/two`, both roots hold a package, so the walk goes on in both and finds the file once.
    let files = with_roots(&[
        (
            "app.py",
            "import ns.sub.a\nimport ns.sub.b\nimport ns.two.x\n",
        ),
        ("p/ns/sub/__init__.py", ""),
        ("p/ns/sub/a.py", ""),
        ("q/ns/sub/b.py", ""),
        ("p/ns/two/__init__.py", ""),
        ("q/ns/two/__init__.py", ""),
        ("q/ns/two/x.py", ""),
    ]);
    assert_eq!(
        py_pairs(&files),
        [
            "app.py>p/ns/sub/__init__.py",
            "app.py>p/ns/sub/a.py",
            "app.py>q/ns/two/__init__.py",
            "app.py>q/ns/two/x.py"
        ]
    );
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_regular_package_under_one_root_beats_a_bare_folder_under_another() {
    let files = with_roots(&[
        ("app.py", "import m\nimport m.y\nimport m.z\n"),
        ("p/m/__init__.py", ""),
        ("p/m/y.py", ""),
        ("q/m/z.py", ""),
    ]);
    // `m.z` exists only in the bare folder, which the package hides: the import fails.
    assert_eq!(
        py_pairs(&files),
        ["app.py>p/m/__init__.py", "app.py>p/m/y.py"]
    );
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_module_under_one_root_beats_a_bare_folder_of_its_name_under_another() {
    let files = with_roots(&[("app.py", "import m\n"), ("p/m.py", ""), ("q/m/x.py", "")]);
    assert_eq!(py_pairs(&files), ["app.py>p/m.py"]);
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_bare_folder_named_like_a_standard_library_module_gives_no_doubt() {
    let files = [
        ("app.py", "import html\nimport html.x\n"),
        ("html/x.py", ""),
    ];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_pkgutil_or_pkg_resources_init_file_merges_like_a_bare_folder_and_gets_no_pair() {
    let forms = [
        "__path__ = __import__('pkgutil').extend_path(__path__, __name__)\n",
        "__path__ = __import__(\"pkgutil\").extend_path(__path__, __name__)  # type: ignore\n",
        "import pkgutil\n\n__path__ = pkgutil.extend_path(__path__, __name__)\n",
        "from pkgutil import extend_path\n__path__ = extend_path(__path__, __name__)\n",
        "__import__('pkg_resources').declare_namespace(__name__)\n",
        "import pkg_resources\npkg_resources.declare_namespace(__name__)\n",
        "try:\n    import pkg_resources\n    pkg_resources.declare_namespace(__name__)\nexcept ImportError:\n    import pkgutil\n    __path__ = pkgutil.extend_path(__path__, __name__)\n",
    ];
    for init in forms {
        let files = with_roots(&[
            (
                "app.py",
                "import azure\nimport azure.a\nimport azure.b\nfrom azure import a\n",
            ),
            ("p/azure/__init__.py", init),
            ("p/azure/a.py", ""),
            ("q/azure/__init__.py", init),
            ("q/azure/b.py", ""),
        ]);
        assert_eq!(
            py_pairs(&files),
            ["app.py>p/azure/a.py", "app.py>q/azure/b.py"],
            "{init}"
        );
        assert_eq!(doubts(&files), Vec::<String>::new(), "{init}");
    }
}

#[test]
fn an_init_file_with_any_other_statement_is_a_regular_package_even_beside_extend_path() {
    let files = with_roots(&[
        ("app.py", "import azure.a\n"),
        (
            "p/azure/__init__.py",
            "__path__ = __import__('pkgutil').extend_path(__path__, __name__)\nVERSION = 1\n",
        ),
        ("p/azure/a.py", ""),
        ("q/azure/__init__.py", ""),
        ("q/azure/a.py", ""),
    ]);
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        [
            "app.py | azure.a | two roots provide it | p/azure/__init__.py, p/azure/a.py, q/azure/__init__.py, q/azure/a.py"
        ]
    );
}

#[test]
fn a_tracked_file_named_like_a_standard_library_module_makes_the_importer_unsure() {
    let files = [("app.py", "import logging\n"), ("logging.py", "")];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        ["app.py | logging | shadows the standard library | logging.py"]
    );
    // A package shadowing a standard-library package: the candidates are the files the import would pair to.
    let files = [
        ("app.py", "import logging.handlers\n"),
        ("logging/__init__.py", ""),
        ("logging/handlers.py", ""),
    ];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        [
            "app.py | logging.handlers | shadows the standard library | logging/__init__.py, logging/handlers.py"
        ]
    );
}

#[test]
fn the_standard_library_list_spans_versions_and_includes_built_in_names() {
    // `distutils` left the standard library in 3.12, `sys` is built in: both are in the union of 3.10 to 3.15.
    for name in ["distutils", "imp", "sys", "os", "tomllib", "__future__"] {
        let files = [
            ("app.py", &*format!("import {name}\n")),
            (&*format!("{name}.py"), ""),
        ]
        .map(|(f, t)| (f.to_string(), t.to_string()));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(f, t)| (f.as_str(), t.as_str()))
            .collect();
        assert_eq!(
            doubts(&borrowed),
            [format!(
                "app.py | {name} | shadows the standard library | {name}.py"
            )],
            "{name}"
        );
    }
    let files = [("app.py", "import pandas\n"), ("pandas.py", "")];
    assert_eq!(py_pairs(&files), ["app.py>pandas.py"]);
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_standard_library_shadow_that_names_no_file_the_import_reaches_is_no_finding() {
    let files = [("app.py", "import os.path\n"), ("os.py", "")];
    assert_eq!(doubts(&files), Vec::<String>::new());
}

// Declared layouts (spec #589, "Declared layouts").

/// A `pyproject.toml` text holding `body` under a `[project]` table.
fn pyproject(body: &str) -> String {
    format!("[project]\nname = \"m\"\n{body}")
}

/// The notes `check` gives a repository of `files`.
fn notes(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&each_file_a_part(files), files);
    report(&root).notes
}

#[test]
fn a_declared_where_replaces_the_default_src_and_where_lib_and_src_gives_both() {
    let layout = |where_: &str| {
        let config = pyproject(&format!(
            "[tool.setuptools.packages.find]\nwhere = {where_}\n"
        ));
        py_pairs(&[
            ("pyproject.toml", &config),
            ("lib/a/__init__.py", "x = 1\n"),
            ("src/b/__init__.py", "x = 1\n"),
            ("app.py", "import a\nimport b\n"),
        ])
    };
    assert_eq!(layout("[\"lib\"]"), ["app.py>lib/a/__init__.py"]);
    assert_eq!(
        layout("[\"lib\", \"src\"]"),
        ["app.py>lib/a/__init__.py", "app.py>src/b/__init__.py"]
    );
    // The project folder, written as a dot, is no second root, and `src/` is not one.
    assert_eq!(layout("[\".\"]"), Vec::<String>::new());
}

#[test]
fn where_dot_is_the_project_folder_deduplicated() {
    let config = pyproject("[tool.setuptools.packages.find]\nwhere = [\".\", \"./\"]\n");
    let found = py_pairs(&[
        ("p/pyproject.toml", &config),
        ("p/app.py", "import a\n"),
        ("p/a.py", "x = 1\n"),
    ]);
    assert_eq!(found, ["p/app.py>p/a.py"]);
}

#[test]
fn a_declared_folder_is_read_from_its_project_folder() {
    let config = pyproject("[tool.setuptools.packages.find]\nwhere = [\"lib\"]\n");
    let found = py_pairs(&[
        ("p/pyproject.toml", &config),
        ("p/lib/a.py", "x = 1\n"),
        ("p/app.py", "import a\n"),
    ]);
    assert_eq!(found, ["p/app.py>p/lib/a.py"]);
}

#[test]
fn package_dir_with_an_empty_name_gives_a_root_and_a_named_one_registers_the_name_at_its_folder() {
    let config = pyproject("[tool.setuptools]\npackage-dir = {\"\" = \"lib\"}\n");
    let found = py_pairs(&[
        ("pyproject.toml", &config),
        ("lib/a/__init__.py", "x = 1\n"),
        ("src/b/__init__.py", "x = 1\n"),
        ("app.py", "import a\nimport b\n"),
    ]);
    assert_eq!(found, ["app.py>lib/a/__init__.py"]);

    let config = pyproject("[tool.setuptools]\npackage-dir = {mypkg = \"lib\"}\n");
    let found = py_pairs(&[
        ("pyproject.toml", &config),
        ("lib/__init__.py", "x = 1\n"),
        ("lib/sub.py", "x = 1\n"),
        ("app.py", "import mypkg.sub\nfrom mypkg import sub as s\n"),
    ]);
    assert_eq!(found, ["app.py>lib/__init__.py", "app.py>lib/sub.py"]);
}

#[test]
fn a_renamed_package_without_an_init_file_is_a_bare_folder() {
    let config = pyproject("[tool.setuptools]\npackage-dir = {mypkg = \"lib\"}\n");
    let found = py_pairs(&[
        ("pyproject.toml", &config),
        ("lib/sub.py", "x = 1\n"),
        ("app.py", "import mypkg.sub\n"),
    ]);
    assert_eq!(found, ["app.py>lib/sub.py"]);
}

#[test]
fn a_renamed_package_in_one_project_and_a_folder_of_that_name_in_another_are_a_first_segment_two_roots_provide()
 {
    let config = pyproject("[tool.setuptools]\npackage-dir = {mypkg = \"lib\"}\n");
    let files = [
        ("pyproject.toml", config.as_str()),
        ("lib/__init__.py", "x = 1\n"),
        ("mypkg/__init__.py", "x = 1\n"),
        ("app.py", "import mypkg\n"),
    ];
    assert_eq!(py_pairs(&files), Vec::<String>::new());
    assert_eq!(
        doubts(&files),
        ["app.py | mypkg | two roots provide it | lib/__init__.py, mypkg/__init__.py"]
    );
}

#[test]
fn a_renamed_package_at_the_folder_a_root_already_gives_its_name_is_one_provider() {
    let config = pyproject("[tool.setuptools.package-dir]\nmypkg = \"src/mypkg\"\n");
    let files = [
        ("pyproject.toml", config.as_str()),
        ("src/mypkg/__init__.py", ""),
        ("src/mypkg/core.py", ""),
        ("app.py", "from mypkg import core\n"),
    ];
    assert_eq!(
        py_pairs(&files),
        ["app.py>src/mypkg/__init__.py", "app.py>src/mypkg/core.py"]
    );
    assert_eq!(doubts(&files), Vec::<String>::new());
    let files = [
        ("setup.cfg", "[options]\npackage_dir =\n    mypkg = mypkg\n"),
        ("mypkg/__init__.py", ""),
        ("mypkg/core.py", ""),
        ("app.py", "import mypkg.core\n"),
    ];
    assert_eq!(
        py_pairs(&files),
        ["app.py>mypkg/__init__.py", "app.py>mypkg/core.py"]
    );
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn a_poetry_entry_with_no_from_declares_the_project_folder_and_replaces_src() {
    let config = pyproject("[tool.poetry]\npackages = [{ include = \"a\" }]\n");
    let found = py_pairs(&[
        ("pyproject.toml", &config),
        ("a/__init__.py", "x = 1\n"),
        ("src/b/__init__.py", "x = 1\n"),
        ("app.py", "import a\nimport b\n"),
    ]);
    assert_eq!(found, ["app.py>a/__init__.py"]);
}

#[test]
fn two_names_package_dir_renames_to_one_folder_both_resolve_to_it() {
    let config = pyproject("[tool.setuptools]\npackage-dir = {alpha = \"lib\", beta = \"lib\"}\n");
    let files = [
        ("pyproject.toml", config.as_str()),
        ("lib/__init__.py", "x = 1\n"),
        ("lib/m.py", "from . import n\n"),
        ("lib/n.py", "x = 1\n"),
        ("one.py", "import alpha\n"),
        ("two.py", "import beta\n"),
    ];
    assert_eq!(
        py_pairs(&files),
        [
            "lib/m.py>lib/__init__.py",
            "lib/m.py>lib/n.py",
            "one.py>lib/__init__.py",
            "two.py>lib/__init__.py"
        ]
    );
    assert_eq!(doubts(&files), Vec::<String>::new());
}

#[test]
fn poetry_from_and_hatch_sources_and_packages_give_a_root() {
    let poetry = pyproject(
        "[tool.poetry]\npackages = [{ include = \"a\", from = \"lib\" }, { include = \"c\" }]\n",
    );
    let hatch_sources = pyproject("[tool.hatch.build]\nsources = [\"lib\"]\n");
    let hatch_target = pyproject("[tool.hatch.build.targets.wheel]\nsources = [\"lib\"]\n");
    let hatch_packages = pyproject("[tool.hatch.build.targets.wheel]\npackages = [\"lib/a\"]\n");
    for (name, config) in [
        ("poetry", poetry),
        ("hatch sources", hatch_sources),
        ("hatch target sources", hatch_target),
        ("hatch packages", hatch_packages),
    ] {
        let found = py_pairs(&[
            ("pyproject.toml", &config),
            ("lib/a/__init__.py", "x = 1\n"),
            ("src/b/__init__.py", "x = 1\n"),
            ("app.py", "import a\nimport b\n"),
        ]);
        assert_eq!(found, ["app.py>lib/a/__init__.py"], "{name}");
    }
}

/// The pairs of a repository with `config` as its `setup.cfg`, a package in `lib/` and one in `src/`, and `app.py`
/// importing both.
fn cfg_pairs(config: &str) -> Vec<String> {
    py_pairs(&[
        ("setup.cfg", config),
        ("lib/a/__init__.py", "x = 1\n"),
        ("src/b/__init__.py", "x = 1\n"),
        ("app.py", "import a\nimport b\n"),
    ])
}

#[test]
fn a_setup_cfg_package_dir_and_find_where_give_roots() {
    let lib = ["app.py>lib/a/__init__.py"];
    assert_eq!(
        cfg_pairs("[options]\npackage_dir =\n    =lib\n"),
        lib,
        "continuation line"
    );
    assert_eq!(
        cfg_pairs("[options]\npackage_dir = =lib\n"),
        lib,
        "same line"
    );
    assert_eq!(
        cfg_pairs("[options.packages.find]\nwhere = lib\n"),
        lib,
        "where"
    );
    assert_eq!(
        cfg_pairs("[options]\nPackage_Dir:\n\t= lib\n"),
        lib,
        "mixed case, a colon and a tab"
    );
    assert_eq!(
        cfg_pairs("[options.packages.find]\nWHERE : lib\n"),
        lib,
        "upper case and a colon"
    );
}

#[test]
fn a_setup_cfgs_layout_is_read_as_configparser_does_and_only_from_its_options_section() {
    for (what, config, expected) in [
        (
            "comments and blank lines",
            "# a comment\n[metadata]\nname = m\n\n[options]\n; another\npackage_dir =\n    # indented comment\n\n    = lib\n    ; more\n",
            "app.py>lib/a/__init__.py",
        ),
        (
            "a key in another section",
            "[tool:other]\npackage_dir = lib\nwhere = lib\n[options]\nname = m\n",
            "app.py>src/b/__init__.py",
        ),
    ] {
        assert_eq!(cfg_pairs(config), [expected], "{what}");
    }
}

#[test]
fn a_setup_cfg_package_dir_line_with_a_name_registers_it_at_its_folder() {
    let config = "[options]\npackage_dir =\n    mypkg = lib\n";
    let found = py_pairs(&[
        ("setup.cfg", config),
        ("lib/__init__.py", "x = 1\n"),
        ("app.py", "import mypkg\n"),
    ]);
    assert_eq!(found, ["app.py>lib/__init__.py"]);
    let config = "[options]\npackage_dir =\n    =src\n    mypkg = lib\n";
    let found = py_pairs(&[
        ("setup.cfg", config),
        ("lib/__init__.py", "x = 1\n"),
        ("src/b/__init__.py", "x = 1\n"),
        ("app.py", "import mypkg\nimport b\n"),
    ]);
    assert_eq!(
        found,
        ["app.py>lib/__init__.py", "app.py>src/b/__init__.py"]
    );
}

#[test]
fn a_literal_setup_py_package_dir_gives_a_root() {
    let lib = ["app.py>lib/a/__init__.py"];
    for text in [
        "from setuptools import setup\nsetup(name='m', package_dir={'': 'lib'})\n",
        "setup(\n    package_dir = {\n        \"\": \"lib\",\n    },\n)\n",
    ] {
        let found = py_pairs(&[
            ("setup.py", text),
            ("lib/a/__init__.py", "x = 1\n"),
            ("src/b/__init__.py", "x = 1\n"),
            ("app.py", "import a\nimport b\n"),
        ]);
        assert_eq!(found, lib, "{text}");
    }
}

#[test]
fn a_layout_form_that_is_not_read_leaves_the_default_roots_and_gives_the_note() {
    let cases: [(&str, &str, &str); 5] = [
        (
            "p/pyproject.toml",
            "[tool.hatch.build.targets.wheel]\nsources = { \"lib\" = \"\" }\n",
            "tool.hatch.build.targets.wheel.sources",
        ),
        (
            "p/pyproject.toml",
            "[tool.setuptools.packages.find]\nwhere = \"lib\"\n",
            "tool.setuptools.packages.find.where",
        ),
        (
            "p/pyproject.toml",
            "[tool.setuptools]\npackage-dir = \"lib\"\n",
            "tool.setuptools.package-dir",
        ),
        (
            "p/setup.py",
            "setup(package_dir=PACKAGE_DIR)\n",
            "package_dir",
        ),
        (
            "p/setup.py",
            "setup(package_dir={'': 'lib', 'x': 'y'})\n",
            "package_dir",
        ),
    ];
    for (file, body, key) in cases {
        let text = if file.ends_with(".toml") {
            pyproject(body)
        } else {
            body.to_string()
        };
        let files = [
            (file, text.as_str()),
            ("p/lib/a/__init__.py", "x = 1\n"),
            ("p/src/b/__init__.py", "x = 1\n"),
            ("p/app.py", "import a\nimport b\n"),
        ];
        // The default roots stay: `src/` is read, `lib/` is not.
        assert_eq!(py_pairs(&files), ["p/app.py>p/src/b/__init__.py"], "{key}");
        assert_eq!(
            notes(&files),
            [format!(
                "python-imports did not read {key} in {file}; failing links into p may be false"
            )]
        );
    }
}

#[test]
fn a_note_goes_to_standard_error_and_leaves_the_exit_code_and_standard_output_as_they_were() {
    let config = pyproject("[tool.setuptools.packages.find]\nwhere = \"lib\"\n");
    let files = [("pyproject.toml", config.as_str()), ("app.py", "x = 1\n")];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    let out = run(&root, "check", &[]);
    assert!(out.status.success());
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(stdout.starts_with("ok: "), "{stdout}");
    assert!(!stdout.contains("note:"), "{stdout}");
    assert_eq!(
        stderr,
        "note: python-imports did not read tool.setuptools.packages.find.where in pyproject.toml; failing links into . may be false\n"
    );
    let json = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(
        value["notes"],
        json!([
            "python-imports did not read tool.setuptools.packages.find.where in pyproject.toml; failing links into . may be false"
        ])
    );
}

#[test]
fn a_repository_whose_layouts_are_all_read_gives_no_note() {
    let config = pyproject("[tool.setuptools.packages.find]\nwhere = [\"lib\"]\n");
    let files = [
        ("pyproject.toml", config.as_str()),
        ("setup.cfg", "[options]\npackage_dir =\n    =lib\n"),
        (
            "setup.py",
            "# package_dir is read from setup.cfg\nsetup(my_package_dir=1)\n",
        ),
        ("lib/a.py", "x = 1\n"),
    ];
    assert_eq!(notes(&files), Vec::<String>::new());
}

#[test]
fn a_setup_cfg_with_a_line_before_its_first_section_is_a_problem_naming_the_file() {
    for text in [
        "name = m\n[options]\n",
        "# a comment\n\n  \nstray\n[metadata]\n",
    ] {
        let files = [("p/setup.cfg", text), ("p/a.py", "x = 1\n")];
        let (_dir, root) = repo(&each_file_a_part(&files), &files);
        let problems = problems(&root);
        assert!(
            has(&problems, &["p/setup.cfg", "section header"]),
            "{problems:?}"
        );
    }
    // A comment or blank lines before the first section are fine.
    let files = [("setup.cfg", "# a comment\n\n; another\n[options]\n")];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    assert_eq!(problems(&root), Vec::<String>::new());
}

#[test]
fn a_duplicate_key_in_a_section_the_extractor_does_not_read_is_ignored_and_one_in_a_read_section_gives_the_note()
 {
    let files = [
        (
            "setup.cfg",
            "[flake8]\nignore = E1\nignore = E2\n[options]\nname = m\n",
        ),
        ("a.py", "x = 1\n"),
    ];
    let (_dir, root) = repo(&each_file_a_part(&files), &files);
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(found.notes, Vec::<String>::new());

    for section in ["options", "options.packages.find"] {
        let config = format!("[{section}]\nwhere = lib\nWhere = src\n");
        let files = [("setup.cfg", config.as_str()), ("a.py", "x = 1\n")];
        let (_dir, root) = repo(&each_file_a_part(&files), &files);
        let found = report(&root);
        assert_eq!(found.problems, Vec::<String>::new(), "{section}");
        assert_eq!(
            found.notes,
            [format!(
                "python-imports did not read [{section}] in setup.cfg; failing links into . may be false"
            )]
        );
    }
}

#[test]
fn a_repeated_section_is_read_in_none_of_its_copies() {
    let config = "[options]\npackage_dir =\n    = lib\n\n[options]\npackage_dir =\n    = other\n";
    let files = [
        ("setup.cfg", config),
        ("lib/a/__init__.py", "x = 1\n"),
        ("src/b/__init__.py", "x = 1\n"),
        ("app.py", "import a\nimport b\n"),
    ];
    assert_eq!(py_pairs(&files), ["app.py>src/b/__init__.py"]);
    assert_eq!(
        notes(&files),
        ["python-imports did not read [options] in setup.cfg; failing links into . may be false"]
    );
}

#[test]
fn a_config_under_an_excluded_folder_gives_no_note() {
    let config = pyproject("[tool.setuptools.packages.find]\nwhere = \"lib\"\n");
    let files = [
        ("tests/d/pyproject.toml", config.as_str()),
        ("a.py", "x = 1\n"),
    ];
    assert_eq!(notes(&files), Vec::<String>::new());
}
