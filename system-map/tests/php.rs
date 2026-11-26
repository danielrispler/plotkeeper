//! `php-imports`: PHP repositories in temporary folders, read through `check` and `imports` (spec #647, "Testing
//! Decisions"). This ticket reads `require` and `include`; a case that needs a class `use` checks the `use` line through
//! a `require` after it instead, since the class resolution comes in the next ticket.

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
