//! `php-imports`: PHP repositories in temporary folders, read through `check` and `imports` (spec #647, "Testing
//! Decisions"): requires, the reader's traps and test folders (#652), then class `use` items resolved through Composer's
//! autoload rules (#653).

use serde_json::{Value, json};

use crate::common::{extractor_map, pairs, repo, report, run, write};

/// A map naming `php-imports` whose parts are `parts`, each an id with its paths, whose import links are `links`, and
/// which ignores `ignored` besides its own file.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)], ignored: &[&str]) -> Value {
    extractor_map("php-imports", parts, links, ignored)
}

/// The file pairs a repository of `files` gives, its part `a` owning everything under `a/` and `b` the rest, with no
/// link, so that only the pairs matter.
fn php_pairs(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]), files);
    pairs(&report(&root))
}

/// The importing file of most tests: part `a`, one folder below the root.
const MAIN: &str = "a/main.php";

/// The files `MAIN` requires: part `b`.
const X: &str = "lib/x.php";
const Y: &str = "lib/y.php";
const Z: &str = "lib/sub/z.php";
const TOP: &str = "top.php";

fn targets() -> [(&'static str, &'static str); 4] {
    [
        (X, "<?php\n"),
        (Y, "<?php\n"),
        (Z, "<?php\n"),
        (TOP, "<?php\n"),
    ]
}

/// The pairs of `MAIN` holding `text`, among the targets.
fn pairs_of_text(text: &str) -> Vec<String> {
    let mut files = vec![(MAIN, text)];
    files.extend(targets());
    php_pairs(&files)
}

/// The pairs of `MAIN` holding the PHP code `code` after an opening tag.
fn pairs_of(code: &str) -> Vec<String> {
    pairs_of_text(&format!("<?php\n{code}\n"))
}

fn from_main(to: &[&str]) -> Vec<String> {
    to.iter().map(|t| format!("{MAIN}>{t}")).collect()
}

/// Each pair's import sites in `check --json`, as `from>to file:line`, sorted.
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

// Class `use`.

/// A root `composer.json` mapping `App\` to `src/`.
const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#;

/// The importing file of the class cases is `MAIN`, part `a`; every other file is part `b`.
fn class_pairs(composer: &str, files: &[(&str, &str)]) -> Vec<String> {
    let mut all = vec![("composer.json", composer)];
    all.extend_from_slice(files);
    php_pairs(&all)
}

/// The pairs `MAIN` holding `code` gives, with `files` beside the root `composer.json`.
fn use_pairs(composer: &str, code: &str, files: &[&str]) -> Vec<String> {
    let text = format!("<?php\n{code}\n");
    let mut all = vec![(MAIN, text.as_str())];
    all.extend(files.iter().map(|f| (*f, "<?php\n")));
    class_pairs(composer, &all)
}

#[test]
fn a_class_use_pairs_with_the_file_the_psr4_table_maps_it_to() {
    assert_eq!(
        use_pairs(COMPOSER, "use App\\Lib\\X;", &["src/Lib/X.php"]),
        from_main(&["src/Lib/X.php"])
    );
}

/// A `composer.json` text with the `psr-4` table `table` (a JSON object) under `autoload`.
fn psr4(table: &str) -> String {
    format!(r#"{{"autoload":{{"psr-4":{table}}}}}"#)
}

#[test]
fn a_prefix_whose_folders_lack_the_file_falls_through_to_a_shorter_prefix() {
    let composer = psr4(r#"{"App\\":"src/","App\\Lib\\":"lib/"}"#);
    // `lib/` lacks `Deep/X.php`, so the walk goes on to `App\` and `src/Lib/Deep/X.php`.
    assert_eq!(
        use_pairs(&composer, "use App\\Lib\\Deep\\X;", &["src/Lib/Deep/X.php"]),
        from_main(&["src/Lib/Deep/X.php"])
    );
    // The longer prefix wins when its folder holds the file.
    assert_eq!(
        use_pairs(
            &composer,
            "use App\\Lib\\Deep\\X;",
            &["src/Lib/Deep/X.php", "lib/Deep/X.php"]
        ),
        from_main(&["lib/Deep/X.php"])
    );
}

#[test]
fn a_prefix_mapped_to_an_array_tries_its_folders_in_order() {
    let composer = psr4(r#"{"App\\":["src/", "more/"]}"#);
    assert_eq!(
        use_pairs(&composer, "use App\\X;", &["more/X.php"]),
        from_main(&["more/X.php"])
    );
    // The first folder that holds the file is the one, and a later one gets no pair.
    assert_eq!(
        use_pairs(&composer, "use App\\X;", &["src/X.php", "more/X.php"]),
        from_main(&["src/X.php"])
    );
}

#[test]
fn the_empty_prefix_is_the_fallback_after_every_prefix() {
    let composer = psr4(r#"{"":"fallback/","App\\":"src/"}"#);
    assert_eq!(
        use_pairs(&composer, "use App\\X;", &["fallback/App/X.php"]),
        from_main(&["fallback/App/X.php"])
    );
    assert_eq!(
        use_pairs(
            &composer,
            "use App\\X;",
            &["fallback/App/X.php", "src/X.php"]
        ),
        from_main(&["src/X.php"])
    );
    assert_eq!(
        use_pairs(&composer, "use Other\\X;", &["fallback/Other/X.php"]),
        from_main(&["fallback/Other/X.php"])
    );
}

#[test]
fn autoload_dev_is_read_like_autoload() {
    let composer = r#"{"autoload-dev":{"psr-4":{"Dev\\":"dev/"}}}"#;
    assert_eq!(
        use_pairs(composer, "use Dev\\X;", &["dev/X.php"]),
        from_main(&["dev/X.php"])
    );
}

#[test]
fn a_leading_backslash_of_a_prefix_is_stripped() {
    let composer = psr4(r#"{"\\App\\":"src/"}"#);
    assert_eq!(
        use_pairs(&composer, "use App\\X;", &["src/X.php"]),
        from_main(&["src/X.php"])
    );
}

#[test]
fn class_names_match_paths_with_exact_case() {
    for code in ["use App\\x;", "use app\\X;", "use App\\Lib\\X;"] {
        assert_eq!(
            use_pairs(COMPOSER, code, &["src/X.php"]),
            Vec::<String>::new(),
            "{code}"
        );
    }
    // A namespace import never pairs with a file of the same letters in another case.
    assert_eq!(
        use_pairs(
            &psr4(r#"{"Up\\":"src/"}"#),
            "use Up\\Rector;",
            &["src/rector.php"]
        ),
        Vec::<String>::new()
    );
}

#[test]
fn a_namespace_a_built_in_an_external_class_and_an_untracked_target_give_nothing() {
    for code in [
        "use App\\Lib;",
        "use ArrayObject;",
        "use Vendor\\Pkg\\Thing;",
        "use App\\Missing;",
        "use App;",
    ] {
        assert_eq!(
            use_pairs(COMPOSER, code, &["src/Lib/X.php"]),
            Vec::<String>::new(),
            "{code}"
        );
    }
}

#[test]
fn every_form_of_a_class_use_gives_its_pairs() {
    let files = [
        "src/A.php",
        "src/B.php",
        "src/C/D.php",
        "src/E.php",
        "src/F.php",
    ];
    for code in [
        "use App\\A, App\\B;",
        "use \\App\\A, \\App\\B;",
        "use App\\A as Alias, App\\B as Other;",
        "use App\\{A, B};",
        "use \\App\\{A, B,};",
        "use App\\{A as X, B as Y};",
        "use App\\A;\nuse App\\B;",
        "use App\\{A, function f, const C, B};",
        "USE App\\A, App\\B;",
        "namespace N;\nuse App\\A;\nuse App\\B;",
    ] {
        assert_eq!(
            use_pairs(COMPOSER, code, &files),
            from_main(&["src/A.php", "src/B.php"]),
            "{code}"
        );
    }
    assert_eq!(
        use_pairs(COMPOSER, "use App\\{A, C\\D as E};", &files),
        from_main(&["src/A.php", "src/C/D.php"])
    );
}

#[test]
fn use_function_and_use_const_and_trait_and_closure_use_pair_nothing() {
    for code in [
        "use function App\\A;",
        "use const App\\A;",
        "use function App\\{A, B};",
        "class K { use App\\A; }",
        "$f = function () use ($x) {};",
    ] {
        assert_eq!(
            use_pairs(COMPOSER, code, &["src/A.php", "src/B.php"]),
            Vec::<String>::new(),
            "{code}"
        );
    }
}

#[test]
fn two_namespaces_in_one_file_each_read_their_use_lines() {
    for code in [
        "namespace N1;\nuse App\\A;\nnamespace N2;\nuse App\\B;",
        "namespace N1 { use App\\A; }\nnamespace N2 { use App\\B; }",
        "namespace N1 { use App\\A; }\nnamespace { use App\\B; }",
    ] {
        assert_eq!(
            use_pairs(COMPOSER, code, &["src/A.php", "src/B.php"]),
            from_main(&["src/A.php", "src/B.php"]),
            "{code}"
        );
    }
}

#[test]
fn a_composer_json_in_a_subfolder_maps_relative_to_its_folder() {
    let files = [
        (MAIN, "<?php\nuse Pkg\\X;\nuse Pkg\\Y;\n"),
        (
            "pkgs/one/composer.json",
            r#"{"autoload":{"psr-4":{"Pkg\\":"src/"}}}"#,
        ),
        ("pkgs/one/src/X.php", "<?php\n"),
        ("src/Y.php", "<?php\n"),
        ("pkgs/one/Y.php", "<?php\n"),
    ];
    assert_eq!(php_pairs(&files), from_main(&["pkgs/one/src/X.php"]));
    // `..` leaves the folder, and the path is cleaned.
    let files = [
        (MAIN, "<?php\nuse Pkg\\X;\n"),
        (
            "pkgs/one/composer.json",
            r#"{"autoload":{"psr-4":{"Pkg\\":"../shared/./src/"}}}"#,
        ),
        ("pkgs/shared/src/X.php", "<?php\n"),
    ];
    assert_eq!(php_pairs(&files), from_main(&["pkgs/shared/src/X.php"]));
}

#[test]
fn a_class_uses_site_is_the_line_of_its_name() {
    let text = "<?php\n\nuse App\\A,\n    App\\B;\nuse App\\{\n    A as X,\n    C,\n};\n";
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            ("composer.json", COMPOSER),
            (MAIN, text),
            ("src/A.php", "<?php\n"),
            ("src/B.php", "<?php\n"),
            ("src/C.php", "<?php\n"),
        ],
    );
    assert_eq!(
        sites(&root),
        [
            format!("{MAIN}>src/A.php {MAIN}:3"),
            format!("{MAIN}>src/A.php {MAIN}:6"),
            format!("{MAIN}>src/B.php {MAIN}:4"),
            format!("{MAIN}>src/C.php {MAIN}:7"),
        ]
    );
}

#[test]
fn a_link_between_parts_is_proved_by_a_class_use_and_failed_without_one() {
    let files = [
        (MAIN, "<?php\nuse App\\A;\n"),
        ("composer.json", COMPOSER),
        ("src/A.php", "<?php\n"),
    ];
    let parts: &[(&str, &[&str])] = &[("b", &["**"]), ("a", &["a/"])];
    let (_dir, root) = repo(&map(parts, &[("a", "b")], &[]), &files);
    let out = run(&root, "check", &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("1 import (1 proven, 0 unproven)"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let (_dir, root) = repo(&map(parts, &[("b", "a")], &[]), &files);
    let out = run(&root, "check", &[]);
    assert!(!out.status.success());
}

// Ignored files.

/// The pairs `MAIN` holding `code` gives with `files` beside `composer.json`, under a map that ignores `ignored`.
fn ignoring_pairs(composer: &str, code: &str, files: &[&str], ignored: &[&str]) -> Vec<String> {
    let text = format!("<?php\n{code}\n");
    let mut all = vec![(MAIN, text.as_str()), ("composer.json", composer)];
    all.extend(files.iter().map(|f| (*f, "<?php\n")));
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], ignored), &all);
    pairs(&report(&root))
}

#[test]
fn an_ignored_composer_json_is_still_read() {
    assert_eq!(
        ignoring_pairs(COMPOSER, "use App\\X;", &["src/X.php"], &["composer.json"]),
        from_main(&["src/X.php"])
    );
}

#[test]
fn an_ignored_first_hit_gives_its_composer_json_no_answer_and_no_pair_past_it() {
    let composer = psr4(r#"{"App\\":["src/", "more/"], "App\\Lib\\":"lib/"}"#);
    // The first folder's file is ignored: no pair to the second folder's.
    assert_eq!(
        ignoring_pairs(
            &composer,
            "use App\\X;",
            &["src/X.php", "more/X.php"],
            &["src/"]
        ),
        Vec::<String>::new()
    );
    // The longer prefix's file is ignored: no fall-through to the shorter prefix's.
    assert_eq!(
        ignoring_pairs(
            &composer,
            "use App\\Lib\\X;",
            &["lib/X.php", "src/Lib/X.php"],
            &["lib/"]
        ),
        Vec::<String>::new()
    );
}

// Unsure.

/// A repository where `App\X` is `src/X.php` through the root `composer.json` and `copy/src/X.php` through the copy's,
/// whose parts are `a` (the importer), `x` (`src/`), `y` (`copy/`), `other` (`other/`) and `rest`.
fn doubled(links: &[(&str, &str)], ignored: &[&str]) -> (tempfile::TempDir, String) {
    let parts: &[(&str, &[&str])] = &[
        ("rest", &["**"]),
        ("a", &["a/"]),
        ("x", &["src/"]),
        ("y", &["copy/"]),
        ("other", &["other/"]),
    ];
    repo(
        &map(parts, links, ignored),
        &[
            ("composer.json", COMPOSER),
            ("copy/composer.json", COMPOSER),
            (MAIN, "<?php\nuse App\\X;\n"),
            ("src/X.php", "<?php\n"),
            ("copy/src/X.php", "<?php\n"),
            ("other/o.php", "<?php\n"),
        ],
    )
}

#[test]
fn two_composer_files_mapping_a_class_to_two_files_make_the_importer_unsure() {
    let (_dir, root) = doubled(&[("a", "x"), ("a", "other")], &[]);
    let out = run(&root, "check", &[]);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "note: a/main.php is unsure of App\\X; its links to x, y pass as unproven\n"
    );
    // Only the link to a candidate's part is relaxed; the link to `other` still fails.
    let problems = String::from_utf8_lossy(&out.stdout);
    assert_eq!(problems.lines().count(), 1, "{problems}");
    assert!(
        problems.starts_with(
            "import link \"a-other\" runs from \"a\" to \"other\", but the code shows no dependency"
        ),
        "{problems}"
    );
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["unsure"],
        json!([{
            "file": MAIN,
            "name": "App\\X",
            "reason": "two composer files map it",
            "candidates": ["copy/src/X.php", "src/X.php"]
        }])
    );
    assert_eq!(value["edges"], json!([]));
}

#[test]
fn two_composer_files_mapping_a_class_to_one_file_are_not_unsure() {
    let files = [
        ("composer.json", COMPOSER),
        (
            "copy/composer.json",
            r#"{"autoload":{"psr-4":{"App\\":"../src/"}}}"#,
        ),
        (MAIN, "<?php\nuse App\\X;\n"),
        ("src/X.php", "<?php\n"),
    ];
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]), &files);
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value.get("unsure"), None);
    assert_eq!(pairs(&report(&root)), from_main(&["src/X.php"]));
}

#[test]
fn ignoring_one_copy_settles_a_doubled_class() {
    let (_dir, root) = doubled(&[("a", "x")], &["copy/src/"]);
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value.get("unsure"), None);
    assert_eq!(pairs(&report(&root)), from_main(&["src/X.php"]));
}

// Problems.

/// The `composer.json` problems and the pairs of a repository holding `MAIN` with `use App\X;`, `src/X.php` and the `composer.json` files `configs`.
fn config_problems(configs: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
    let mut files = vec![(MAIN, "<?php\nuse App\\X;\n"), ("src/X.php", "<?php\n")];
    files.extend_from_slice(configs);
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]), &files);
    let found = report(&root);
    // The map holds no link, so the problems about the imports it lacks are left out.
    let problems = found
        .problems
        .iter()
        .filter(|p| p.contains("composer.json"))
        .cloned()
        .collect();
    (problems, pairs(&found))
}

#[test]
fn a_composer_json_that_is_not_json_is_a_problem_naming_it_and_adds_nothing() {
    let (problems, pairs) = config_problems(&[
        ("composer.json", COMPOSER),
        ("pkgs/composer.json", r#"{"autoload": {"psr-4": "#),
    ]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("pkgs/composer.json: "),
        "{problems:?}"
    );
    // The other file still resolves.
    assert_eq!(pairs, from_main(&["src/X.php"]));
    // The broken one is the only file mapping `App\`: nothing resolves.
    let (problems, pairs) = config_problems(&[("composer.json", "{ nope")]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("composer.json: "), "{problems:?}");
    assert_eq!(pairs, Vec::<String>::new());
}

#[test]
fn a_prefix_without_its_trailing_backslash_is_a_problem_naming_the_file_and_the_prefix() {
    let (problems, pairs) = config_problems(&[(
        "composer.json",
        r#"{"autoload":{"psr-4":{"App\\":"src/","Bad":"src/"}}}"#,
    )]);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].starts_with("composer.json: ") && problems[0].contains("\"Bad\""),
        "{problems:?}"
    );
    // The file adds nothing, its good prefix too.
    assert_eq!(pairs, Vec::<String>::new());
    // A stripped leading backslash and the empty prefix are fine.
    let (problems, _) = config_problems(&[(
        "composer.json",
        r#"{"autoload-dev":{"psr-4":{"\\App\\":"src/","":"src/"}}}"#,
    )]);
    assert_eq!(problems, Vec::<String>::new());
}

#[test]
fn a_psr4_value_that_is_not_a_folder_or_a_list_of_folders_is_a_problem() {
    for table in [
        r#"{"App\\": 7}"#,
        r#"{"App\\": null}"#,
        r#"{"App\\": {"a": "src/"}}"#,
        r#"{"App\\": ["src/", 7]}"#,
    ] {
        let (problems, pairs) = config_problems(&[("composer.json", &psr4(table))]);
        assert_eq!(problems.len(), 1, "{table}: {problems:?}");
        assert!(
            problems[0].starts_with("composer.json: ") && problems[0].contains("App\\"),
            "{table}: {problems:?}"
        );
        assert_eq!(pairs, Vec::<String>::new(), "{table}");
    }
}

#[test]
fn keys_of_other_types_elsewhere_give_nothing_and_no_problem() {
    for text in [
        "[]",
        "7",
        r#"{"autoload": 7}"#,
        r#"{"autoload": {"psr-4": []}}"#,
        r#"{"autoload": {"psr-4": "src/"}, "autoload-dev": null}"#,
        r#"{"autoload": {"classmap": 7}}"#,
    ] {
        let (problems, pairs) = config_problems(&[("composer.json", text)]);
        assert_eq!(problems, Vec::<String>::new(), "{text}");
        assert_eq!(pairs, Vec::<String>::new(), "{text}");
    }
}

#[test]
fn a_composer_json_under_a_test_or_vendor_folder_is_never_read() {
    for config in [
        "tests/composer.json",
        "pkgs/Tests/fixture/composer.json",
        "test/composer.json",
        "vendor/pkg/composer.json",
        "pkgs/vendor/composer.json",
    ] {
        // Broken: no problem.
        let (problems, _) = config_problems(&[("composer.json", COMPOSER), (config, "{ nope")]);
        assert_eq!(problems, Vec::<String>::new(), "{config}");
        // Valid and the only one: it resolves nothing.
        let (problems, pairs) =
            config_problems(&[(config, r#"{"autoload":{"psr-4":{"App\\":"../src/"}}}"#)]);
        assert_eq!(problems, Vec::<String>::new(), "{config}");
        assert_eq!(pairs, Vec::<String>::new(), "{config}");
    }
}

// Skipped files.

/// `check` on a repository whose part `a` owns `a/main.php` (importing `App\X`, which `src/X.php` holds), with a link from
/// `a` to `c`, a part no import reaches, and the `composer.json` text `composer`.
fn link_check(composer: &str, importer: &str) -> (bool, String) {
    let parts: &[(&str, &[&str])] = &[("b", &["**"]), ("a", &["a/"]), ("c", &["c/"])];
    let (_dir, root) = repo(
        &map(parts, &[("a", "c")], &[]),
        &[
            ("composer.json", composer),
            (importer, "<?php\nuse App\\X;\n"),
            ("src/X.php", "<?php\n"),
            ("c/c.php", "<?php\n"),
        ],
    );
    let out = run(&root, "check", &[]);
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn a_file_under_a_classmap_path_and_no_psr4_folder_is_skipped_and_its_part_passes_unproven() {
    let plain = r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#;
    // Read, the part's link to `c` has no import behind it: a dead link.
    let (ok, out) = link_check(plain, MAIN);
    assert!(!ok && out.contains("no dependency"), "{out}");
    for composer in [
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["a/"]}}"#,
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["a"]}}"#,
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["./a/main.php"]}}"#,
        r#"{"autoload":{"psr-4":{"App\\":"src/"}},"autoload-dev":{"classmap":["a/"]}}"#,
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["x/", "a/"]}}"#,
    ] {
        let (ok, out) = link_check(composer, MAIN);
        assert!(ok, "{composer}: {out}");
        assert!(out.contains("1 unproven"), "{composer}: {out}");
    }
}

#[test]
fn a_classmap_path_names_a_folder_or_a_file_not_a_name_prefix() {
    for composer in [
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["a/ma"]}}"#,
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["a/other.php"]}}"#,
        r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["b/"]}}"#,
    ] {
        let (ok, out) = link_check(composer, MAIN);
        assert!(!ok && out.contains("no dependency"), "{composer}: {out}");
    }
}

#[test]
fn a_file_under_a_classmap_path_and_a_psr4_folder_is_read() {
    // `a/` is both a classmap path and a PSR-4 folder, of this or of another `composer.json`.
    let psr = r#"{"autoload":{"psr-4":{"App\\":"src/", "Other\\":"a/"},"classmap":["a/"]}}"#;
    let (ok, out) = link_check(psr, MAIN);
    assert!(!ok && out.contains("no dependency"), "{out}");
    let parts: &[(&str, &[&str])] = &[("b", &["**"]), ("a", &["a/"]), ("c", &["c/"])];
    let (_dir, root) = repo(
        &map(parts, &[("a", "c")], &[]),
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"src/"},"classmap":["a/"]}}"#,
            ),
            (
                "c/composer.json",
                r#"{"autoload":{"psr-4":{"Other\\":"../a/"}}}"#,
            ),
            (MAIN, "<?php\nuse App\\X;\n"),
            ("src/X.php", "<?php\n"),
            ("c/c.php", "<?php\n"),
        ],
    );
    let out = run(&root, "check", &[]);
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("no dependency"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn the_empty_prefix_folder_covers_the_files_under_it_too() {
    let composer = r#"{"autoload":{"psr-4":{"App\\":"src/","":"a/"},"classmap":["a/"]}}"#;
    let (ok, out) = link_check(composer, MAIN);
    assert!(!ok && out.contains("no dependency"), "{out}");
}

// Commit mode.

#[test]
fn a_commit_resolves_a_class_use_through_its_own_composer_json() {
    let (dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[("a", "b")], &[]),
        &[
            ("composer.json", COMPOSER),
            (MAIN, "<?php\nuse App\\X;\n"),
            ("src/X.php", "<?php\n"),
        ],
    );
    let edges = |extra: &[&str]| -> Value {
        let args = [&["--json"], extra].concat();
        let out = run(&root, "check", &args);
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["edges"].clone()
    };
    let at_commit = edges(&["--commit", "HEAD"]);
    assert_eq!(at_commit[0]["pairs"][0]["to"], "src/X.php", "{at_commit}");
    assert_eq!(edges(&[]), at_commit);
    // An edited working-tree copy changes nothing at the commit, and everything in the tree.
    write(
        dir.path(),
        "composer.json",
        r#"{"autoload":{"psr-4":{"App\\":"elsewhere/"}}}"#,
    );
    assert_eq!(edges(&["--commit", "HEAD"]), at_commit);
    assert_eq!(edges(&[]), json!([]));
}

// Requires.

#[test]
fn each_static_form_of_a_require_pairs_with_the_file_it_names() {
    for code in [
        "require __DIR__ . '/../lib/x.php';",
        "require_once __DIR__ . '/../lib/x.php';",
        "include __DIR__ . '/../lib/x.php';",
        "include_once __DIR__ . '/../lib/x.php';",
        "require __DIR__ . \"/../lib/x.php\";",
        "require dirname(__FILE__) . '/../lib/x.php';",
        "require dirname(__DIR__) . '/lib/x.php';",
        "require dirname(__DIR__, 1) . '/lib/x.php';",
        "require dirname(__FILE__, 2) . '/lib/x.php';",
        "require dirname(dirname(__FILE__)) . '/lib/x.php';",
        "require dirname(dirname(__FILE__, 1)) . '/lib/x.php';",
        "require \\dirname(__DIR__) . '/lib/x.php';",
        "require \\dirname(\\dirname(__FILE__)) . '/lib/x.php';",
        "require realpath(__DIR__ . '/../lib/x.php');",
        "require realpath(__DIR__) . '/../lib/x.php';",
        "require \\realpath(dirname(__DIR__)) . '/lib/x.php';",
        "require(__DIR__ . '/../lib/x.php');",
        "require (dirname(__DIR__) . '/lib/x.php');",
        "require ((__DIR__) . '/../lib/x.php');",
        "require __DIR__ . '/../lib' . '/x.php';",
        "require __DIR__ . '/../lib/./sub/../x.php';",
        "require DIRNAME(__dir__) . '/lib/x.php';",
        "@require __DIR__ . '/../lib/x.php';",
    ] {
        assert_eq!(pairs_of(code), from_main(&[X]), "{code}");
    }
}

#[test]
fn a_file_at_the_root_and_dirname_up_to_the_root_pair() {
    let found = php_pairs(&[
        (
            "a/main.php",
            "<?php\nrequire dirname(__FILE__, 2) . '/top.php';\n",
        ),
        (TOP, "<?php\n"),
    ]);
    assert_eq!(found, [format!("{MAIN}>{TOP}")]);
    let found = php_pairs(&[
        ("top.php", "<?php\nrequire __DIR__ . '/a/main.php';\n"),
        (MAIN, "<?php\n"),
    ]);
    assert_eq!(found, [format!("{TOP}>{MAIN}")]);
}

#[test]
fn a_path_the_reader_cannot_fix_statically_gives_nothing() {
    for code in [
        // Relative to the working directory or `include_path`.
        "require './../lib/x.php';",
        "require '../lib/x.php';",
        "require 'lib/x.php';",
        "require 'x.php';",
        "require '/lib/x.php';",
        // Built from a constant or a variable.
        "require ABSPATH . 'lib/x.php';",
        "require ABSPATH . '/lib/x.php';",
        "require $dir . '/lib/x.php';",
        "require __DIR__ . $name;",
        "require __DIR__ . '/../lib/' . $name . '.php';",
        "require __DIR__ . \"/../lib/$name.php\";",
        "require __DIR__ . \"/../lib/{$name}.php\";",
        "require __DIR__ . \"/../lib/${name}.php\";",
        "require __DIR__ . \"/../lib/\\x78.php\";",
        "require __FILE__ . '/../lib/x.php';",
        "require __DIR__ . '/../lib/' . PHP_VERSION . '.php';",
        "require sprintf('%s/../lib/x.php', __DIR__);",
        "require getcwd() . '/lib/x.php';",
        // A path out of the repository.
        "require __DIR__ . '/../../../x.php';",
        "require dirname(__DIR__, 3) . '/lib/x.php';",
        "require dirname(__FILE__, 3) . '/lib/x.php';",
        // `dirname` with a level PHP rejects, or one it cannot read.
        "require dirname(__DIR__, 0) . '/lib/x.php';",
        "require dirname(__DIR__, $n) . '/lib/x.php';",
        // The literal does not start with a slash: it joins the last name of the folder.
        "require dirname(__DIR__) . 'lib/x.php';",
        // More than the path follows it.
        "require __DIR__ . '/../lib/x.php' . $suffix;",
        "require (__DIR__ . '/../lib/x.php')[0];",
    ] {
        assert_eq!(pairs_of(code), Vec::<String>::new(), "{code}");
    }
}

#[test]
fn a_method_or_a_static_call_named_like_a_keyword_is_no_include() {
    for code in [
        "$this->require(__DIR__ . '/../lib/x.php');",
        "$this?->include(__DIR__ . '/../lib/x.php');",
        "Loader::require_once(__DIR__ . '/../lib/x.php');",
        "$this->include_once(__DIR__ . '/../lib/x.php');",
    ] {
        assert_eq!(pairs_of(code), Vec::<String>::new(), "{code}");
    }
    // The same call without the member access is an include.
    assert_eq!(
        pairs_of("$this->go(); require __DIR__ . '/../lib/x.php';"),
        from_main(&[X])
    );
}

#[test]
fn a_require_anywhere_in_the_code_pairs() {
    let code = "function boot() {\n    if ($x) {\n        require_once __DIR__ . '/../lib/x.php';\n    }\n}\n\
                class K { function f() { include dirname(__DIR__) . '/lib/y.php'; } }\n\
                $r = (require __DIR__ . '/../lib/sub/z.php');\n\
                if (include __DIR__ . '/../top.php') {}\n";
    assert_eq!(pairs_of(code), from_main(&[Z, X, Y, TOP]));
}

#[test]
fn a_require_pairs_once_however_many_times_it_is_written() {
    let code =
        "require __DIR__ . '/../lib/x.php';\nrequire_once dirname(__DIR__) . '/lib/x.php';\n";
    assert_eq!(pairs_of(code), from_main(&[X]));
}

#[test]
fn a_requires_site_is_the_line_of_its_keyword() {
    let text = "<?php\n\nrequire __DIR__\n    . '/../lib/x.php';\n\n$a = 1;\nif ($a) { include_once\n    dirname(__DIR__) . '/lib/x.php'; }\ninclude __DIR__ . '/../lib/y.php';\n";
    let mut files = vec![(MAIN, text)];
    files.extend(targets());
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]), &files);
    assert_eq!(
        sites(&root),
        [
            format!("{MAIN}>{X} {MAIN}:3"),
            format!("{MAIN}>{X} {MAIN}:7"),
            format!("{MAIN}>{Y} {MAIN}:9"),
        ]
    );
}

// Lexing traps.
//
// Each case holds a `require` of `lib/x.php` the reader must still reach, and one of `lib/y.php` it must not read.

/// The require the reader must reach, and the one it must leave unread, as PHP code.
const WANTED: &str = "require __DIR__ . '/../lib/x.php';";
const HIDDEN: &str = "require __DIR__ . '/../lib/y.php';";

/// The pairs of `MAIN` holding the file `text`, with `{WANTED}` and `{HIDDEN}` filled in.
fn trap(text: &str) -> Vec<String> {
    pairs_of_text(&text.replace("{WANTED}", WANTED).replace("{HIDDEN}", HIDDEN))
}

#[test]
fn text_outside_the_php_tags_is_html() {
    for text in [
        "<html>\n<?php {WANTED} ?>\n</html>\n",
        "<p>{HIDDEN}</p>\n<?php {WANTED}\n",
        "<?PHP\n{WANTED}\n",
        "<?Php {WANTED}\n",
        "<?php\t{WANTED}\n",
        "<?php\r\n{WANTED}\n",
        "<?= include __DIR__ . '/../lib/x.php' ?>\n",
        "<?=include __DIR__ . '/../lib/x.php'?>\n",
        "<?php {WANTED} ?>\n{HIDDEN}\n<?php {HIDDEN_AS_HTML_ENDS} ?>",
        "<?phpx {HIDDEN} ?>\n<?php {WANTED}\n",
        "<?php {WANTED} ?>\n<?php?>\n<p>{HIDDEN}</p>\n",
    ] {
        let text = text.replace("{HIDDEN_AS_HTML_ENDS}", "");
        assert_eq!(trap(&text), from_main(&[X]), "{text}");
    }
}

#[test]
fn a_bare_short_open_tag_stays_html() {
    assert_eq!(
        trap("<? {HIDDEN} ?>\n<p>{HIDDEN}</p>\n"),
        Vec::<String>::new()
    );
    assert_eq!(trap("<? {HIDDEN} ?>\n<?php {WANTED}\n"), from_main(&[X]));
}

#[test]
fn a_close_tag_ends_a_line_comment_and_a_block_comment_does_not() {
    for text in [
        "<?php // note ?>\n{HIDDEN}\n<?php {WANTED}\n",
        "<?php # note ?>\n{HIDDEN}\n<?php {WANTED}\n",
        "<?php // note ?>\n<ul>{HIDDEN}</ul>\n<?php {WANTED} ?>\n",
        "<?php\n// a ?> b\n{HIDDEN}\n<?php {WANTED}\n",
        // A `?>` in a block comment, a string or a nested comment inside a string's code does not leave PHP mode.
        "<?php /* ?> {HIDDEN} */ {WANTED}\n",
        "<?php $a = '?>'; $b = \"?>\"; {WANTED}\n",
        "<?php\n{WANTED} // ?>\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
}

#[test]
fn comments_hide_the_code_they_hold() {
    for text in [
        "<?php\n# {HIDDEN}\n{WANTED}\n",
        "<?php\n// {HIDDEN}\n{WANTED}\n",
        "<?php\n/* {HIDDEN} */ {WANTED}\n",
        "<?php\n/* \n{HIDDEN}\n */\n{WANTED}\n",
        "<?php\n/** @see {HIDDEN} */\n{WANTED}\n",
        // A comment closes at its own `*/`, and does not nest.
        "<?php\n/* /* */ {WANTED}\n",
        // A hash that opens an attribute is code, not a comment.
        "<?php\n#[Attr('{HIDDEN}')]\n{WANTED}\n",
        "<?php\n#[Route('/x', methods: ['GET'])]\nfunction f() {}\n{WANTED}\n",
        "<?php\n#[A, B(1)] #[C]\nclass K {}\n{WANTED}\n",
        // Code on the line of an attribute is read.
        "<?php\n#[Pure] function f() {} {WANTED}\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
}

#[test]
fn strings_hide_the_code_they_hold() {
    for text in [
        "<?php\n$a = '{HIDDEN}';\n{WANTED}\n",
        "<?php\n$a = \"{HIDDEN}\";\n{WANTED}\n",
        "<?php\n$a = `{HIDDEN}`;\n{WANTED}\n",
        // Single quotes take only `\\` and `\'` as escapes.
        "<?php\n$a = 'it\\'s {HIDDEN}';\n{WANTED}\n",
        "<?php\n$a = 'a\\\\'; {WANTED}\n",
        "<?php\n$a = \"say \\\"{HIDDEN}\\\"\";\n{WANTED}\n",
        "<?php\n$a = \"a\\\\\"; {WANTED}\n",
        // A quote in the other kind of string.
        "<?php\n$a = \"it's\"; $b = 'say \"'; {WANTED}\n",
        // A string spread over lines.
        "<?php\n$a = 'one\n{HIDDEN}\nthree';\n{WANTED}\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
}

#[test]
fn code_nested_in_a_string_is_read_with_its_own_strings_and_braces() {
    for text in [
        "<?php\n$a = \"{$a[\"}\"]}\";\n{WANTED}\n",
        "<?php\n$a = \"{$a['}']} and {$b->c()}\";\n{WANTED}\n",
        "<?php\n$a = \"{$a[\"x; {HIDDEN} \"]}\";\n{WANTED}\n",
        "<?php\n$a = \"{$a['\"']}\"; {WANTED}\n",
        "<?php\n$a = \"${a[\"x; {HIDDEN} \"]}\";\n{WANTED}\n",
        "<?php\n$a = <<<EOT\n${a[\"x; {HIDDEN} \"]} {$b[\"y; {HIDDEN} \"]}\nEOT;\n{WANTED}\n",
        "<?php\n$a = \"{$a[\"k\"]} {HIDDEN}\";\n{WANTED}\n",
        "<?php\n$a = \"${a} {$b} $c {HIDDEN}\";\n{WANTED}\n",
        "<?php\n$a = \"{${'x'}}\";\n{WANTED}\n",
        "<?php\n$a = \"{$f(function () { return \"}\"; })}\";\n{WANTED}\n",
        "<?php\n$a = \"{$a[\"{$b[\"}\"]}\"]}\";\n{WANTED}\n",
        "<?php\n$a = `ls {$dir['}']}`;\n{WANTED}\n",
        "<?php\n$a = <<<EOT\n{$a[\"}\"]}\n{HIDDEN}\nEOT;\n{WANTED}\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
}

#[test]
fn heredoc_and_nowdoc_close_at_their_label_even_indented_and_hide_what_they_hold() {
    for text in [
        "<?php\n$a = <<<EOT\n{HIDDEN}\nEOT;\n{WANTED}\n",
        "<?php\n$a = <<<'EOT'\n{HIDDEN}\nEOT;\n{WANTED}\n",
        "<?php\n$a = <<<\"EOT\"\n{HIDDEN}\nEOT;\n{WANTED}\n",
        "<?php\n$a = <<< EOT\n{HIDDEN}\nEOT;\n{WANTED}\n",
        // PHP 7.3: an indented closing label, with more code after it on its line.
        "<?php\n$a = <<<EOT\n    {HIDDEN}\n    EOT;\n{WANTED}\n",
        "<?php\n$a = <<<'EOT'\n\t{HIDDEN}\n\tEOT;\n{WANTED}\n",
        "<?php\nfoo(<<<EOT\n  text\n  EOT, 1); {WANTED}\n",
        "<?php\nfoo(<<<EOT\n  text\n  EOT\n); {WANTED}\n",
        "<?php\r\n$a = <<<EOT\r\n{HIDDEN}\r\nEOT;\r\n{WANTED}\r\n",
        // A line that only starts with the label is no closing label; neither is one that holds it later.
        "<?php\n$a = <<<EOT\nEOTX;\n{HIDDEN}\nEOT_2\nx EOT\nEOT;\n{WANTED}\n",
        "<?php\n$a = <<<'EOT'\nEOTX;\n{HIDDEN}\nEOT_2\nx EOT\nEOT;\n{WANTED}\n",
        // An empty body.
        "<?php\n$a = <<<EOT\nEOT;\n{WANTED}\n",
        // A `<<` that is a shift.
        "<?php\n$a = 1 << 2; $b = $a <<< 3; {WANTED}\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
}

#[test]
fn reading_stops_at_halt_compiler() {
    for text in [
        "<?php\n{WANTED}\n__halt_compiler();\n{HIDDEN}\n",
        "<?php\n{WANTED}\n__HALT_COMPILER(); ?>\n<?php {HIDDEN}\n",
        "<?php\n{WANTED}\n__halt_compiler();\n'unclosed string\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
    // A member of that name is no `__halt_compiler`.
    assert_eq!(
        trap("<?php\n$o->__halt_compiler(); {WANTED}\n"),
        from_main(&[X])
    );
}

#[test]
fn keywords_match_in_any_case_and_a_name_is_one_token() {
    for text in [
        "<?php\nREQUIRE_ONCE __DIR__ . '/../lib/x.php';\n",
        "<?php\nRequire_Once __dir__ . '/../lib/x.php';\n",
        "<?php\nINCLUDE_ONCE Dirname(__DIR__) . '/lib/x.php';\n",
        "<?php\nuse A\\B;\nuse function A\\f;\n{WANTED}\n",
        "<?php\nUSE A\\B;\n{WANTED}\n",
        // A qualified or relative name is one token, so it is no keyword.
        "<?php\nA\\require(__DIR__ . '/../lib/y.php');\nnamespace\\include(1);\n{WANTED}\n",
    ] {
        assert_eq!(trap(text), from_main(&[X]), "{text}");
    }
}

#[test]
fn a_php_block_split_by_html_keeps_the_code_around_it() {
    let text = "<?php\nclass K {\n    function f() {\n        ob_start(); ?>\n<p>{HIDDEN}</p>\n<?php\n        {WANTED}\n    }\n}\n";
    assert_eq!(trap(text), from_main(&[X]));
}

// Files the reader cannot lex.

/// Files whose last string, heredoc or comment is never closed.
const UNLEXABLE: [(&str, &str); 7] = [
    ("an unclosed single-quoted string", "$a = 'open;\n"),
    ("an unclosed double-quoted string", "$a = \"open;\n"),
    ("an unclosed backtick string", "$a = `open;\n"),
    ("an unclosed block comment", "/* open\n"),
    ("an unclosed heredoc", "$a = <<<EOT\nopen\n"),
    (
        "an unclosed nowdoc",
        "$a = <<<'EOT'\nopen\nEOT_NOT_THE_LABEL;\n",
    ),
    ("an unclosed nested string", "$a = \"{$b[\"open}\";\n"),
];

#[test]
fn a_file_the_reader_cannot_lex_gives_no_pair_no_problem_and_no_unsure_and_leaves_its_part_unread()
{
    for (what, tail) in UNLEXABLE {
        let text = format!("<?php\n{WANTED}\n{tail}");
        let (_dir, root) = repo(
            &map(&[("lib", &["lib/"]), ("a", &["a/"])], &[("a", "lib")], &[]),
            &[(MAIN, &text), (X, "<?php\n")],
        );
        let found = report(&root);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{what}");
        assert_eq!(found.problems, Vec::<String>::new(), "{what}");
        assert!(found.unsure.is_empty(), "{what}");
        assert_eq!(found.counts.links.unproven, 1, "{what}");
        assert_eq!(found.counts.links.proven, 0, "{what}");
    }
}

#[test]
fn a_link_resting_only_on_a_file_the_reader_cannot_lex_fails_when_its_part_has_another_read_file() {
    let broken = format!("<?php\n{WANTED}\n$a = 'open;\n");
    let (_dir, root) = repo(
        &map(&[("lib", &["lib/"]), ("a", &["a/"])], &[("a", "lib")], &[]),
        &[(MAIN, &broken), ("a/other.php", "<?php\n"), (X, "<?php\n")],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), Vec::<String>::new());
    assert!(
        found
            .problems
            .iter()
            .any(|p| p.contains("\"a\"") && p.contains("\"lib\"")),
        "{:?}",
        found.problems
    );
}

#[test]
fn a_file_the_reader_cannot_lex_gives_no_detection_note() {
    for (what, tail) in UNLEXABLE {
        let text = format!("<?php\n{tail}");
        let mut map = map(&[("a", &["**"])], &[], &[]);
        map["extractors"] = json!([]);
        let (_dir, root) = repo(&map, &[(MAIN, &text)]);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{what}");
    }
}

// Files PHP leaves out.

/// A repository where part `s` holds only `file`, which requires `lib/x.php`, and part `lib` holds that file, with an
/// import link `s` to `lib`.
fn skipped_or_read(file: &str) -> system_map::Report {
    let text = format!(
        "<?php\nrequire __DIR__ . '/{}{X}';\n",
        "../".repeat(file.matches('/').count() + 1)
    );
    let (_dir, root) = repo(
        &map(&[("lib", &["lib/"]), ("s", &["s/"])], &[("s", "lib")], &[]),
        &[(&format!("s/{file}"), &text), (X, "<?php\n")],
    );
    report(&root)
}

#[test]
fn a_file_in_a_test_folder_gives_no_pair_and_does_not_make_its_part_read() {
    for file in [
        "tests/a.php",
        "test/a.php",
        "Tests/a.php",
        "TESTS/a.php",
        "src/Foo/Tests/a.php",
        "src/tests/Unit/deep/a.php",
        "x/test/y/a.php",
    ] {
        let found = skipped_or_read(file);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{file}");
        assert_eq!(found.problems, Vec::<String>::new(), "{file}");
        assert_eq!(found.counts.links.unproven, 1, "{file}");
    }
}

#[test]
fn a_file_outside_a_test_folder_gives_its_pairs_and_makes_its_part_read_whatever_its_name() {
    for file in [
        "src/a.php",
        "src/CreatesMatchingTest.php",
        "src/Framework/Attributes/Test.php",
        "src/UnitTest.php",
        "src/testing/a.php",
        "src/tester/a.php",
        "src/contests/a.php",
        "src/latest/a.php",
        // A folder name with a suffix is no test folder; neither is a file named `tests.php`.
        "src/tests.php",
        "src/test.php",
    ] {
        let found = skipped_or_read(file);
        assert_eq!(pairs(&found), [format!("s/{file}>{X}")], "{file}");
        assert_eq!(found.counts.links.unproven, 0, "{file}");
    }
}

#[test]
fn phpt_and_blade_files_are_not_read() {
    for file in [
        "src/a.phpt",
        "views/a.blade.php",
        "src/a.php.txt",
        "src/a.inc",
    ] {
        let found = skipped_or_read(file);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{file}");
        assert_eq!(found.counts.links.unproven, 1, "{file}");
    }
    // A name that only ends like Blade's is a PHP file.
    let found = skipped_or_read("src/blade.php");
    assert_eq!(pairs(&found), [format!("s/src/blade.php>{X}")]);
}

// Detection.

#[test]
fn check_suggests_php_imports_only_for_a_repository_with_a_production_php_file_the_map_does_not_ignore()
 {
    let note = "note: php-imports could read this repository; add it to extractors\n";
    let code = "<?php\n";
    for (files, ignored, expected) in [
        (&[("a/src/A.php", code)][..], &[][..], note),
        (&[("a/tests/A.php", code)], &[], ""),
        (&[("a/src/Tests/A.php", code)], &[], ""),
        (&[("a/test/A.php", code)], &[], ""),
        (&[("gen/A.php", code)], &["gen/"], ""),
        (&[("a/views/a.blade.php", code)], &[], ""),
        (&[("a/src/a.phpt", code)], &[], ""),
        (&[("a/readme.md", "x\n")], &[], ""),
        // A production file beside the skipped ones is enough.
        (&[("a/tests/A.php", code), ("a/src/B.php", code)], &[], note),
        (&[("a/src/CreatesMatchingTest.php", code)], &[], note),
    ] {
        let mut map = map(&[("a", &["**"])], &[], ignored);
        map["extractors"] = json!([]);
        let (_dir, root) = repo(&map, files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

// No `composer.json`, and commit mode.

#[test]
fn without_a_composer_json_static_requires_pair_and_nothing_is_a_problem() {
    let (_dir, root) = repo(
        &map(&[("lib", &["lib/"]), ("a", &["a/"])], &[("a", "lib")], &[]),
        &[
            (MAIN, "<?php\nrequire dirname(__DIR__) . '/lib/x.php';\n"),
            (X, "<?php\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), [format!("{MAIN}>{X}")]);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(found.counts.links.proven, 1);
}

#[test]
fn commit_mode_gives_the_edges_of_the_working_tree() {
    let (dir, root) = repo(
        &map(
            &[("lib", &["lib/"]), ("a", &["a/"])],
            &[("a", "lib"), ("lib", "a")],
            &[],
        ),
        &[
            (MAIN, "<?php\nrequire dirname(__DIR__) . '/lib/x.php';\n"),
            (X, "<?php\ninclude __DIR__ . '/../a/main.php';\n"),
        ],
    );
    let tree = run(&root, "check", &["--json"]);
    let commit = run(&root, "check", &["--json", "--commit", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
    let value: Value = serde_json::from_slice(&commit.stdout).unwrap();
    assert_eq!(
        value["edges"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| format!("{}>{}", e["from"], e["to"]))
            .collect::<Vec<_>>(),
        ["\"a\">\"lib\"", "\"lib\">\"a\""],
        "{value}"
    );
    // An edit not committed changes nothing at the commit.
    write(dir.path(), MAIN, "<?php\n");
    assert_eq!(
        String::from_utf8_lossy(&run(&root, "check", &["--json", "--commit", "HEAD"]).stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
}
