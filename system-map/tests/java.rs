//! `java-imports`: Java repositories in temporary folders, read through `check` and `imports` (spec #590, "Testing
//! Decisions").

use serde_json::{Value, json};

use crate::common::{
    assert_only_candidate_links_relaxed, assert_unsure_file_listed_and_link_unproven,
    extractor_map, has, imports_json, pairs, repo, report, run, sites, write,
};

/// A map naming `java-imports` whose parts are `parts`, each an id with its paths, whose import links are `links`, and
/// which ignores `ignored` besides its own file.
fn map(parts: &[(&str, &[&str])], links: &[(&str, &str)], ignored: &[&str]) -> Value {
    extractor_map("java-imports", parts, links, ignored)
}

/// The file pairs a repository of `files` gives, its part `a` owning everything under `a/` and `b` the rest, with no
/// link, so that only the pairs matter.
fn java_pairs(files: &[(&str, &str)]) -> Vec<String> {
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]), files);
    pairs(&report(&root))
}

/// The importing file of most tests: part `a`, package `app`, with `imports` from line 3, one per line.
const MAIN: &str = "a/src/main/java/app/Main.java";

fn main_importing(imports: &[&str]) -> String {
    format!(
        "package app;\n\n{}\npublic class Main {{}}\n",
        imports.join("\n")
    )
}

/// A production file of part `b` in package `lib`, named `name`, holding `body`.
fn lib(name: &str, body: &str) -> (String, String) {
    (
        format!("lib/src/main/java/lib/{name}.java"),
        format!("package lib;\n\n{body}\n"),
    )
}

/// The pairs of `MAIN` importing `imports`, among the library files `libs`.
fn pairs_of_imports(imports: &[&str], libs: &[(String, String)]) -> Vec<String> {
    let main = main_importing(imports);
    let mut files: Vec<(&str, &str)> = vec![(MAIN, &main)];
    files.extend(libs.iter().map(|(f, t)| (f.as_str(), t.as_str())));
    java_pairs(&files)
}

#[test]
fn an_import_of_a_type_a_file_declares_pairs_with_that_file() {
    let found = pairs_of_imports(
        &["import lib.Util;"],
        &[
            lib("Util", "public class Util {}"),
            lib("Other", "public class Other {}"),
        ],
    );
    assert_eq!(found, [format!("{MAIN}>lib/src/main/java/lib/Util.java")]);
}

#[test]
fn a_type_is_found_by_what_files_declare_never_by_path() {
    // A second type of `Foo.java`, and a file whose folder does not match its package.
    let stray = (
        "lib/src/main/java/elsewhere/Z.java".to_string(),
        "package lib;\n\npublic class Z {}\n".to_string(),
    );
    let found = pairs_of_imports(
        &["import lib.Helper;", "import lib.Z;"],
        &[lib("Foo", "public class Foo {}\nclass Helper {}"), stray],
    );
    assert_eq!(
        found,
        [
            format!("{MAIN}>lib/src/main/java/elsewhere/Z.java"),
            format!("{MAIN}>lib/src/main/java/lib/Foo.java")
        ]
    );
}

#[test]
fn every_declaration_kind_is_indexed_and_nothing_else_is() {
    let kinds = [
        ("Iface", "public interface Iface {}"),
        ("Color", "public enum Color { RED }"),
        ("Point", "public record Point(int x, int y) {}"),
        ("Marker", "public @interface Marker {}"),
        ("Spaced", "public @ interface Spaced {}"),
        ("Shape", "public sealed interface Shape permits Sq {}"),
        ("Sq", "public non-sealed class Sq implements Shape {}"),
        ("Plain", "public abstract class Plain {}"),
    ];
    let libs: Vec<_> = kinds.iter().map(|(n, b)| lib(n, b)).collect();
    let imports: Vec<String> = kinds
        .iter()
        .map(|(n, _)| format!("import lib.{n};"))
        .collect();
    let imports: Vec<&str> = imports.iter().map(String::as_str).collect();
    let found = pairs_of_imports(&imports, &libs);
    let expected: Vec<String> = {
        let mut all: Vec<_> = kinds
            .iter()
            .map(|(n, _)| format!("{MAIN}>lib/src/main/java/lib/{n}.java"))
            .collect();
        all.sort();
        all
    };
    assert_eq!(found, expected);
}

#[test]
fn a_class_literal_in_an_annotation_and_a_field_named_record_declare_nothing() {
    let real = lib(
        "Real",
        "@RunWith(Suite.class)\n@SuiteClasses({A.class, B.class})\npublic class Real {}",
    );
    let found = pairs_of_imports(
        &["import lib.Suite;", "import lib.A;", "import lib.B;"],
        &[real],
    );
    assert_eq!(found, Vec::<String>::new());
    let holder = lib(
        "Holder",
        "public class Holder {\n    private final String record = \"r\";\n}",
    );
    let found = pairs_of_imports(&["import lib.record;", "import lib.Holder;"], &[holder]);
    assert_eq!(found, [format!("{MAIN}>lib/src/main/java/lib/Holder.java")]);
}

#[test]
fn a_type_declared_inside_another_is_no_top_level_name() {
    let holder = lib(
        "Holder",
        "public class Holder {\n    record Inner(int x) {}\n    interface Face {}\n    enum Color { RED }\n}",
    );
    let found = pairs_of_imports(
        &["import lib.Inner;", "import lib.Face;", "import lib.Color;"],
        &[holder],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_file_with_no_package_line_is_read_for_its_imports_but_never_a_target() {
    // A compact source file: a top-level method and no package line.
    let compact = (
        "lib/src/main/java/Compact.java".to_string(),
        "import lib.Util;\n\nvoid main() {}\nclass Loose {}\n".to_string(),
    );
    let found = pairs_of_imports(
        &["import Loose;", "import lib.Loose;"],
        &[lib("Util", "public class Util {}"), compact],
    );
    assert_eq!(found, Vec::<String>::new());
    let found = java_pairs(&[
        (
            "a/src/main/java/Compact.java",
            "import lib.Util;\n\nvoid main() {}\n",
        ),
        (
            "lib/src/main/java/lib/Util.java",
            "package lib;\n\npublic class Util {}\n",
        ),
    ]);
    assert_eq!(
        found,
        ["a/src/main/java/Compact.java>lib/src/main/java/lib/Util.java"]
    );
}

#[test]
fn jdk_and_third_party_imports_give_no_pair() {
    let found = pairs_of_imports(
        &[
            "import java.util.List;",
            "import javax.inject.Inject;",
            "import org.junit.Test;",
            "import static org.junit.Assert.assertEquals;",
            "import com.google.common.collect.*;",
        ],
        &[lib("Util", "public class Util {}")],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_nested_import_pairs_with_the_outer_types_file() {
    let found = pairs_of_imports(
        &["import lib.Outer.Inner;", "import lib.Outer.Inner.Deep;"],
        &[lib(
            "Outer",
            "public class Outer { public static class Inner { public static class Deep {} } }",
        )],
    );
    assert_eq!(found, [format!("{MAIN}>lib/src/main/java/lib/Outer.java")]);
}

#[test]
fn the_split_between_package_and_type_comes_from_the_index_never_from_letter_case() {
    let lower = (
        "lib/src/main/java/Upper/Sub/x.java".to_string(),
        "package Upper.Sub;\n\npublic class widget {}\n".to_string(),
    );
    let found = pairs_of_imports(&["import Upper.Sub.widget;"], &[lower]);
    assert_eq!(
        found,
        [format!("{MAIN}>lib/src/main/java/Upper/Sub/x.java")]
    );
}

#[test]
fn static_imports_and_a_type_wildcard_pair_exactly_with_their_types_file() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (
                MAIN,
                &main_importing(&[
                    "import static lib.Util.helper;",
                    "import static lib.Util.*;",
                    "import lib.Other.*;",
                    // A static import names a member, so the type is the name less its last segment.
                    "import static lib.Third;",
                ]),
            ),
            (
                "lib/src/main/java/lib/Util.java",
                "package lib;\n\npublic class Util { public static void helper() {} }\n",
            ),
            (
                "lib/src/main/java/lib/Other.java",
                "package lib;\n\npublic class Other {}\n",
            ),
            (
                "lib/src/main/java/lib/Third.java",
                "package lib;\n\npublic class Third {}\n",
            ),
        ],
    );
    assert_eq!(
        sites(&root, &[]),
        [
            format!("{MAIN}>lib/src/main/java/lib/Other.java {MAIN}:5"),
            format!("{MAIN}>lib/src/main/java/lib/Util.java {MAIN}:3"),
            format!("{MAIN}>lib/src/main/java/lib/Util.java {MAIN}:4"),
        ]
    );
}

#[test]
fn java_lang_and_the_files_own_package_give_nothing_on_demand() {
    let java_lang = (
        "lib/src/main/java/java/lang/Fake.java".to_string(),
        "package java.lang;\n\npublic class Fake {}\n".to_string(),
    );
    // `app` is a split package: `lib/` holds another file of it.
    let same = (
        "lib/src/main/java/app/Other.java".to_string(),
        "package app;\n\npublic class Other {}\n".to_string(),
    );
    let found = pairs_of_imports(
        &["import java.lang.*;", "import app.*;"],
        &[java_lang, same],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn imports_of_package_info_and_module_info_count_but_neither_is_a_target() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (
                "a/src/main/java/app/package-info.java",
                "@Deprecated\npackage app;\n\nimport lib.Util;\n",
            ),
            (
                "a/src/main/java/module-info.java",
                "import lib.Other;\n\nmodule app.mod {}\n",
            ),
            (
                MAIN,
                &main_importing(&["import lib.Stray;", "import app.mod;"]),
            ),
            (
                "lib/src/main/java/lib/Util.java",
                "package lib;\n\npublic class Util {}\n",
            ),
            (
                "lib/src/main/java/lib/Other.java",
                "package lib;\n\npublic class Other {}\n",
            ),
            // Neither file declares a type that can be imported.
            (
                "lib/src/main/java/lib/package-info.java",
                "package lib;\n\nclass Stray {}\n",
            ),
        ],
    );
    assert_eq!(
        pairs(&report(&root)),
        [
            "a/src/main/java/app/package-info.java>lib/src/main/java/lib/Util.java",
            "a/src/main/java/module-info.java>lib/src/main/java/lib/Other.java"
        ]
    );
}

// Lexing traps.

#[test]
fn import_like_and_class_like_text_in_comments_and_literals_gives_no_pair_and_no_declaration() {
    // `Mixed.java` imports `ex.Extra` once; the other `import` lines sit in comments. It declares `Real` and `Second`;
    // the other `class` words sit in a comment, a block comment and a text block, and the braces in a char literal and
    // a string at depth 0 are not braces.
    let mixed = "package lib;\n// import util.Util;\n/* import util.Util;\nimport util.Util; */\nimport ex.Extra;\n\n\
                 // class Fakeone {\n/* class Faketwo { */\n@Marker('{')\n@Name(\"\\\"{\\\\\")\npublic class Real {\n    \
                 String text = \"\"\"\n        class Fakethree {\n        import util.Util;\n        \\\"\"\" class Fakefour {\n        \"\"\";\n    \
                 char brace = '{';\n    String quote = \"\\\"\";\n}\nclass Second {}\n";
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (
                MAIN,
                &main_importing(&[
                    "import lib.Real;",
                    "import lib.Second;",
                    "import lib.Fakeone;",
                    "import lib.Faketwo;",
                    "import lib.Fakethree;",
                    "import lib.Fakefour;",
                ]),
            ),
            ("lib/src/main/java/lib/Mixed.java", mixed),
            (
                "a/src/main/java/util/Util.java",
                "package util;\n\npublic class Util {}\n",
            ),
            (
                "a/src/main/java/ex/Extra.java",
                "package ex;\n\npublic class Extra {}\n",
            ),
        ],
    );
    assert_eq!(
        sites(&root, &[]),
        [
            format!("{MAIN}>lib/src/main/java/lib/Mixed.java {MAIN}:3"),
            format!("{MAIN}>lib/src/main/java/lib/Mixed.java {MAIN}:4"),
            "lib/src/main/java/lib/Mixed.java>a/src/main/java/ex/Extra.java lib/src/main/java/lib/Mixed.java:5"
                .to_string(),
        ]
    );
}

#[test]
fn a_block_comment_does_not_nest() {
    let found = java_pairs(&[
        (
            MAIN,
            "/* outer /* inner */\npackage app;\n\nimport lib.Util;\n\npublic class Main {}\n",
        ),
        (
            "lib/src/main/java/lib/Util.java",
            "package lib;\n\n/* a /* b */ public class Util {}\n",
        ),
    ]);
    assert_eq!(found, [format!("{MAIN}>lib/src/main/java/lib/Util.java")]);
}

#[test]
fn the_header_reads_annotations_a_package_line_imports_and_stray_semicolons_up_to_the_first_other_token()
 {
    let header = "@Generated(value = {\"a)\", \"b\"}, comments = \"(\")\npackage app;\n\n;\nimport lib.A;;\nimport\n  lib\n  .\n  \
                  B ;\nimport static lib.C.*;\nimport module.x.Y;\npublic class Header {}\nimport lib.D;\n";
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            ("a/src/main/java/app/Header.java", header),
            ("lib/src/main/java/lib/A.java", "package lib;\nclass A {}\n"),
            ("lib/src/main/java/lib/B.java", "package lib;\nclass B {}\n"),
            ("lib/src/main/java/lib/C.java", "package lib;\nclass C {}\n"),
            ("lib/src/main/java/lib/D.java", "package lib;\nclass D {}\n"),
            (
                "lib/src/main/java/module/x/Y.java",
                "package module.x;\nclass Y {}\n",
            ),
        ],
    );
    let from = "a/src/main/java/app/Header.java";
    assert_eq!(
        sites(&root, &[]),
        [
            format!("{from}>lib/src/main/java/lib/A.java {from}:5"),
            format!("{from}>lib/src/main/java/lib/B.java {from}:6"),
            format!("{from}>lib/src/main/java/lib/C.java {from}:10"),
            format!("{from}>lib/src/main/java/module/x/Y.java {from}:11"),
        ]
    );
}

#[test]
fn an_import_or_package_name_ending_in_a_dot_is_no_name() {
    // `import lib.Util.;` is passed over and the header reads on; `package lib.;` is no package line, so `Gone` is
    // never a target.
    let found = pairs_of_imports(
        &["import lib.Util.;", "import lib.Gone;", "import lib.Other;"],
        &[
            lib("Util", "public class Util {}"),
            lib("Other", "public class Other {}"),
            (
                "lib/src/main/java/lib/Gone.java".to_string(),
                "package lib.;\n\npublic class Gone {}\n".to_string(),
            ),
        ],
    );
    assert_eq!(found, [format!("{MAIN}>lib/src/main/java/lib/Other.java")]);
}

#[test]
fn import_sites_count_lines_on_the_raw_text_whichever_line_terminator_it_uses() {
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (
                "a/src/main/java/app/Crlf.java",
                "package app;\r\n/* one\r\n two */\r\nimport lib.Util;\r\n",
            ),
            (
                "a/src/main/java/app/Cr.java",
                "package app;\r\rimport lib.Util;\r",
            ),
            (
                "lib/src/main/java/lib/Util.java",
                "package lib;\n\npublic class Util {}\n",
            ),
        ],
    );
    let util = "lib/src/main/java/lib/Util.java";
    assert_eq!(
        sites(&root, &[]),
        [
            format!("a/src/main/java/app/Cr.java>{util} a/src/main/java/app/Cr.java:3"),
            format!("a/src/main/java/app/Crlf.java>{util} a/src/main/java/app/Crlf.java:4"),
        ]
    );
}

// Unicode escapes (JLS §3.3) and files the reader cannot lex.

const LIB_UTIL: &str = "lib/src/main/java/lib/Util.java";
const LIB_OTHER: &str = "lib/src/main/java/lib/Other.java";

/// `Util` and `Other`, the two types the escape tests import.
fn util_and_other() -> Vec<(String, String)> {
    vec![
        lib("Util", "public class Util {}"),
        lib("Other", "public class Other {}"),
    ]
}

#[test]
fn an_escaped_import_keyword_and_a_name_spelled_with_several_us_are_read_as_javac_reads_them() {
    // `\u0069` is `i`; `\uuu0055` is `U`, and any number of `u`s may follow the backslash.
    for import in ["\\u0069mport lib.Util;", "import lib.\\uuu0055til;"] {
        let found = pairs_of_imports(&[import], &util_and_other());
        assert_eq!(found, [format!("{MAIN}>{LIB_UTIL}")], "{import}");
    }
}

#[test]
fn a_translated_line_break_ends_a_comment_and_the_import_site_stays_on_the_raw_line() {
    // Line 3 ends its `//` comment at `\u000a`, so `import lib.Util;` is code on raw line 3; the next import is on raw
    // line 4, not 5.
    let main = "package app;\n\n// note \\u000aimport lib.Util;\nimport lib.Other;\n\npublic class Main {}\n";
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (MAIN, main),
            (&util_and_other()[0].0, &util_and_other()[0].1),
            (&util_and_other()[1].0, &util_and_other()[1].1),
        ],
    );
    assert_eq!(
        sites(&root, &[]),
        [
            format!("{MAIN}>{LIB_OTHER} {MAIN}:4"),
            format!("{MAIN}>{LIB_UTIL} {MAIN}:3"),
        ]
    );
}

#[test]
fn a_backslash_after_an_odd_number_of_backslashes_is_no_escape_and_a_translated_one_starts_none() {
    // Line 3: `\\u000a` has an even run before its `u`'s backslash? No: the second backslash has one before it, so
    // nothing is translated and the comment runs on. Line 4: `\\\u000a` has two before the third, so it is translated
    // and the comment ends, leaving `import lib.Other;` as code. Line 5: `\u005c` translates to a backslash that starts
    // no further escape, so `\u0055til` stays text and names no type.
    let main = "package app;\n\n// \\\\u000aimport lib.Util;\n// \\\\\\u000aimport lib.Other;\n\
                import lib.\\u005cu0055til;\n\npublic class Main {}\n";
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (MAIN, main),
            (&util_and_other()[0].0, &util_and_other()[0].1),
            (&util_and_other()[1].0, &util_and_other()[1].1),
        ],
    );
    assert_eq!(sites(&root, &[]), [format!("{MAIN}>{LIB_OTHER} {MAIN}:4")]);
}

#[test]
fn a_leading_byte_order_mark_is_dropped_so_the_package_line_and_imports_are_read() {
    let (_dir, root) = repo(
        &map(&[("a", &["a/"]), ("b", &["b/"])], &[("a", "b")], &[]),
        &[
            (
                "a/src/main/java/y/D.java",
                "\u{feff}package y;\nimport x.B;\npublic class D {}\n",
            ),
            (
                "b/src/main/java/x/B.java",
                "package x;\n\npublic class B {}\n",
            ),
        ],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        pairs(&found),
        ["a/src/main/java/y/D.java>b/src/main/java/x/B.java"]
    );
}

/// Files the reader cannot lex, each with its text: a malformed escape, a comment, a string, a char literal and a text
/// block never closed, and a text block whose opening `"""` no line terminator follows.
const UNLEXABLE: [(&str, &str); 7] = [
    ("malformed escape", "class T { String s = \"\\u00zz\"; }\n"),
    ("short escape", "class T {}\n// \\u12\n"),
    (
        "unclosed text block",
        "class T { String s = \"\"\"\n  text\n}\n",
    ),
    (
        "text block opened with no line terminator",
        "class T { String s = \"\"\" \tx\"\"\"; }\n",
    ),
    ("unclosed block comment", "class T {}\n/* never closed\n"),
    ("unclosed string", "class T { String s = \"abc;\n}\n"),
    ("unclosed char literal", "class T { char c = 'a;\n}\n"),
];

#[test]
fn a_file_the_reader_cannot_lex_gives_no_pair_no_problem_and_no_unsure_and_leaves_its_part_unread()
{
    for (what, tail) in UNLEXABLE {
        let file = "s/src/main/java/app/T.java";
        let text = format!("package app;\n\nimport lib.Util;\n\n{tail}");
        let (_dir, root) = repo(
            &map(&[("lib", &["lib/"]), ("s", &["s/"])], &[("s", "lib")], &[]),
            &[
                (LIB_UTIL, "package lib;\n\npublic class Util {}\n"),
                (file, &text),
            ],
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
fn a_file_the_reader_cannot_lex_gives_no_detection_note() {
    for (what, tail) in UNLEXABLE {
        let text = format!("package a;\n\n{tail}");
        let mut map = map(&[("a", &["**"])], &[], &[]);
        map["extractors"] = json!([]);
        let (_dir, root) = repo(&map, &[("a/src/main/java/a/T.java", &text)]);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), "", "{what}");
    }
}

#[test]
fn the_types_of_a_file_the_reader_cannot_lex_leave_the_index() {
    // `Bad` declares `lib.Gone` and `lib.Twice` but never closes a text block; `Good` declares `lib.Twice` too.
    let bad = "package lib;\n\npublic class Gone {}\nclass Twice { String s = \"\"\"\n  open\n}\n";
    let good = lib("Twice", "public class Twice {}");
    let main = main_importing(&["import lib.Gone;", "import lib.Twice;"]);
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &[]),
        &[
            (MAIN, &main),
            ("lib/src/main/java/lib/Bad.java", bad),
            (&good.0, &good.1),
        ],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), [format!("{MAIN}>{}", good.0)]);
    assert!(found.unsure.is_empty());
}

// Files Java leaves out.

const IMPORTS_UTIL: &str = "import lib.Util;\n";

/// A repository where part `s` holds only `file`, of package `package` and importing `lib.Util`, and part `lib` holds
/// `Util`, with an import link `s` to `lib`.
fn skipped_or_read(file: &str, package: &str) -> system_map::Report {
    let text = format!("package {package};\n\n{IMPORTS_UTIL}");
    let (_dir, root) = repo(
        &map(&[("lib", &["lib/"]), ("s", &[file])], &[("s", "lib")], &[]),
        &[
            (
                "lib/src/main/java/lib/Util.java",
                "package lib;\n\npublic class Util {}\n",
            ),
            (file, &text),
        ],
    );
    report(&root)
}

#[test]
fn a_file_outside_production_gives_no_pair_and_does_not_make_its_part_read() {
    for (file, package) in [
        ("s/src/test/java/app/T.java", "app"),
        ("s/src/testFixtures/java/app/T.java", "app"),
        ("s/src/integrationTest/java/app/T.java", "app"),
        ("s/src/jmh/java/app/T.java", "app"),
        ("s/src/tools/java/app/T.java", "app"),
        // An Android build type is no production set: a stated under-count.
        ("s/src/debug/java/app/T.java", "app"),
        // Guava keeps its tests in `test/`.
        ("s/test/app/T.java", "app"),
        ("s/tests/app/T.java", "app"),
        // The package does not match the folders: the whole folder is judged, and `src/test` skips it.
        ("s/src/test/java/x/testdata/F.java", "other.pkg"),
        // No package line at all: the whole folder is judged.
        ("s/src/test/java/F.java", ""),
    ] {
        let package = if package.is_empty() { "app" } else { package };
        let found = if file.ends_with("/F.java") && file.contains("java/F.java") {
            // A file with no package line.
            let (_dir, root) = repo(
                &map(&[("lib", &["lib/"]), ("s", &[file])], &[("s", "lib")], &[]),
                &[
                    (
                        "lib/src/main/java/lib/Util.java",
                        "package lib;\n\npublic class Util {}\n",
                    ),
                    (file, IMPORTS_UTIL),
                ],
            );
            report(&root)
        } else {
            skipped_or_read(file, package)
        };
        assert_eq!(pairs(&found), Vec::<String>::new(), "{file}");
        assert_eq!(found.problems, Vec::<String>::new(), "{file}");
        assert_eq!(found.counts.links.unproven, 1, "{file}");
    }
}

#[test]
fn a_production_file_gives_its_pairs_and_makes_its_part_read() {
    for (file, package) in [
        ("s/src/main/java/app/T.java", "app"),
        ("s/src/main/java24/app/T.java", "app"),
        // Lucene's layout.
        ("s/src/java/app/T.java", "app"),
        ("s/src/java25/app/T.java", "app"),
        // Guava's: `src/` holds the packages directly.
        ("s/src/app/T.java", "app"),
        // A production package named `test` is no test folder: the root is cut at the package.
        ("s/src/main/java/test/T.java", "test"),
        ("s/guava/test/T.java", "guava.test"),
        // The package does not match the folders, and the whole folder holds no test word.
        ("s/odd/T.java", "app"),
    ] {
        let found = skipped_or_read(file, package);
        assert_eq!(
            pairs(&found),
            [format!("{file}>lib/src/main/java/lib/Util.java")],
            "{file}"
        );
        assert_eq!(found.counts.links.unproven, 0, "{file}");
    }
}

#[test]
fn a_type_only_a_skipped_file_declares_is_never_a_target() {
    let found = pairs_of_imports(
        &["import lib.Helper;"],
        &[(
            "lib/src/test/java/lib/Helper.java".to_string(),
            "package lib;\n\npublic class Helper {}\n".to_string(),
        )],
    );
    assert_eq!(found, Vec::<String>::new());
}

// The map's `ignored`.

#[test]
fn an_ignored_copy_of_a_type_neither_resolves_nor_causes_doubt() {
    let files = [
        (MAIN, main_importing(&["import lib.Util;"])),
        (
            "lib/src/main/java/lib/Util.java",
            "package lib;\n\npublic class Util {}\n".to_string(),
        ),
        (
            "variant/src/main/java/lib/Util.java",
            "package lib;\n\npublic class Util {}\n".to_string(),
        ),
    ];
    let files: Vec<(&str, &str)> = files.iter().map(|(f, t)| (*f, t.as_str())).collect();
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &["variant/"]),
        &files,
    );
    let found = report(&root);
    assert_eq!(
        pairs(&found),
        [format!("{MAIN}>lib/src/main/java/lib/Util.java")]
    );
    assert_eq!(found.unsure, []);
    // With only the ignored copy, the name resolves to nothing.
    let (_dir, root) = repo(
        &map(&[("b", &["**"]), ("a", &["a/"])], &[], &["variant/"]),
        &[files[0], files[2]],
    );
    let found = report(&root);
    assert_eq!(pairs(&found), Vec::<String>::new());
    assert_eq!(found.unsure, []);
}

// Wildcards.

fn wildcard_repo(links: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    repo(
        &map(
            &[("app", &["a/"]), ("one", &["lib/"]), ("two", &["other/"])],
            links,
            &[],
        ),
        &[
            (MAIN, &main_importing(&["import lib.*;"])),
            (
                "lib/src/main/java/lib/A.java",
                "package lib;\n\npublic class A {}\n",
            ),
            // Lucene's layout, and a second source root in another part.
            (
                "lib/src/java/lib/B.java",
                "package lib;\n\npublic class B {}\nclass B2 {}\n",
            ),
            (
                "other/src/main/java/lib/C.java",
                "package lib;\n\npublic class C {}\n",
            ),
            (
                "lib/src/test/java/lib/D.java",
                "package lib;\n\npublic class D {}\n",
            ),
        ],
    )
}

#[test]
fn a_package_wildcard_pairs_with_every_production_file_of_the_package_across_source_roots() {
    let (_dir, root) = wildcard_repo(&[("app", "one"), ("app", "two")]);
    let found = report(&root);
    assert_eq!(
        pairs(&found),
        [
            format!("{MAIN}>lib/src/java/lib/B.java"),
            format!("{MAIN}>lib/src/main/java/lib/A.java"),
            format!("{MAIN}>other/src/main/java/lib/C.java"),
        ]
    );
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn over_a_package_split_across_two_parts_a_missing_link_to_either_part_fails() {
    let (_dir, root) = wildcard_repo(&[("app", "one")]);
    let found = report(&root);
    assert!(
        has(&found.problems, &["\"app\"", "\"two\"", "no import link"]),
        "{:?}",
        found.problems
    );
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
}

#[test]
fn in_the_report_each_wildcard_pair_counts_as_its_own_unit() {
    let (_dir, root) = wildcard_repo(&[("app", "one"), ("app", "two")]);
    let value = imports_json(&root);
    // Three files of the package, each one pair: three units, never one import of `lib`.
    assert_eq!(value["total"]["raw"], 3);
    assert_eq!(value["across"]["raw"], 3);
}

// Unsure for a doubled name.

/// `a/` and `b/` both declare `x.Util`; only `a/` declares `x.Solo`. `app/` imports `import_lines`, and part `d` has a
/// file nothing imports, so a link to it is dead.
fn doubled(import_lines: &[&str], links: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let main = format!(
        "package app;\n\n{}\npublic class Main {{}}\n",
        import_lines.join("\n")
    );
    repo(
        &map(
            &[
                ("app", &["app/"]),
                ("xa", &["a/"]),
                ("xb", &["b/"]),
                ("c", &["c/"]),
                ("d", &["d/"]),
            ],
            links,
            &[],
        ),
        &[
            ("app/src/main/java/app/Main.java", &main),
            (
                "a/src/main/java/x/Util.java",
                "package x;\n\npublic class Util {}\n",
            ),
            (
                "a/src/main/java/x/Solo.java",
                "package x;\n\npublic class Solo {}\n",
            ),
            (
                "b/src/main/java/x/Util.java",
                "package x;\n\npublic class Util {}\n",
            ),
            (
                "c/src/main/java/c/Thing.java",
                "package c;\n\npublic class Thing {}\n",
            ),
            (
                "d/src/main/java/d/Dead.java",
                "package d;\n\npublic class Dead {}\n",
            ),
        ],
    )
}

const MAIN_APP: &str = "app/src/main/java/app/Main.java";

#[test]
fn a_doubled_name_makes_the_importer_unsure_and_relaxes_only_the_links_to_its_candidates_parts() {
    let (_dir, root) = doubled(
        &["import x.Util.Inner;", "import c.Thing;"],
        &[("app", "xa"), ("app", "xb"), ("app", "c"), ("app", "d")],
    );
    // The link to `d` still fails; `xa` and `xb` pass as unproven, and `c` is proven by its pair.
    assert_only_candidate_links_relaxed(
        &root,
        &format!("note: {MAIN_APP} is unsure of x.Util; its links to xa, xb pass as unproven\n"),
        "app",
        "d",
    );
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["unsure"],
        json!([{
            "file": MAIN_APP,
            "name": "x.Util",
            "reason": "declared twice",
            "candidates": ["a/src/main/java/x/Util.java", "b/src/main/java/x/Util.java"]
        }])
    );
    assert_eq!(
        pairs(&report(&root)),
        [format!("{MAIN_APP}>c/src/main/java/c/Thing.java")]
    );
}

#[test]
fn a_wildcard_over_a_package_with_one_doubled_name_pairs_with_the_rest_and_is_unsure_for_the_doubled_one()
 {
    let (_dir, root) = doubled(&["import x.*;"], &[("app", "xa"), ("app", "xb")]);
    let found = report(&root);
    assert_eq!(
        pairs(&found),
        [format!("{MAIN_APP}>a/src/main/java/x/Solo.java")]
    );
    assert_eq!(found.unsure.len(), 1, "{:?}", found.unsure);
    assert_eq!(found.unsure[0].name, "x.Util");
    assert_eq!(found.unsure[0].reason, "declared twice");
    assert_eq!(
        found.unsure[0].candidates,
        ["a/src/main/java/x/Util.java", "b/src/main/java/x/Util.java"]
    );
    // `xb` has no pair, but is relaxed; `xa` has one.
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn imports_json_lists_the_unsure_files_and_the_pass_line_counts_the_relaxed_link_unproven() {
    let (_dir, root) = doubled(&["import x.Util;"], &[("app", "xa")]);
    assert_unsure_file_listed_and_link_unproven(&root, MAIN_APP);
}

#[test]
fn unsure_notes_group_by_part_and_relaxed_parts_and_name_at_most_three_names() {
    // `a/` and `b/` both declare `x.A` to `x.D`; `a/` and `c/` both declare `y.E`.
    let class = |folder: &str, package: &str, name: &str| {
        (
            format!("{folder}/src/main/java/{package}/{name}.java"),
            format!("package {package};\n\npublic class {name} {{}}\n"),
        )
    };
    let mut files: Vec<(String, String)> = ["A", "B", "C", "D"]
        .iter()
        .flat_map(|n| [class("a", "x", n), class("b", "x", n)])
        .collect();
    files.extend([class("a", "y", "E"), class("c", "y", "E")]);
    files.push((
        "app/src/main/java/app/One.java".into(),
        "package app;\n\nimport x.A;\nimport x.B;\nimport y.E;\npublic class One {}\n".into(),
    ));
    files.push((
        "app/src/main/java/app/Two.java".into(),
        "package app;\n\nimport x.C;\nimport x.D;\npublic class Two {}\n".into(),
    ));
    let files: Vec<(&str, &str)> = files
        .iter()
        .map(|(f, t)| (f.as_str(), t.as_str()))
        .collect();
    let (_dir, root) = repo(
        &map(
            &[
                ("app", &["app/"]),
                ("xa", &["a/"]),
                ("xb", &["b/"]),
                ("c", &["c/"]),
            ],
            &[("app", "xa"), ("app", "xb"), ("app", "c")],
            &[],
        ),
        &files,
    );
    let out = run(&root, "check", &[]);
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "note: app/src/main/java/app/One.java is unsure of y.E; its links to c, xa pass as unproven\n\
         note: 2 files of app are unsure of 4 names (x.A, x.B, x.C, …); its links to xa, xb pass as unproven; \
         --json lists them\n"
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("3 import (0 proven, 3 unproven)"),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out = run(&root, "check", &["--json"]);
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["unsure"].as_array().unwrap().len(), 5);
}

// Detection.

#[test]
fn check_suggests_java_imports_only_for_a_repository_with_a_production_java_file_the_map_does_not_ignore()
 {
    let note = "note: java-imports could read this repository; add it to extractors\n";
    let class = "package a;\n\nclass A {}\n";
    for (files, ignored, expected) in [
        (&[("a/src/main/java/a/A.java", class)][..], &[][..], note),
        (&[("a/src/test/java/a/A.java", class)], &[], ""),
        (&[("a/test/a/A.java", class)], &[], ""),
        (&[("gen/src/main/java/a/A.java", class)], &["gen/"], ""),
        (&[("a/readme.md", "x\n")], &[], ""),
    ] {
        let mut map = map(&[("a", &["**"])], &[], ignored);
        map["extractors"] = json!([]);
        let (_dir, root) = repo(&map, files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

// Commit mode and mixed repositories.

#[test]
fn commit_mode_gives_the_edges_and_doubts_of_the_working_tree() {
    let (dir, root) = doubled(
        &["import x.Util;", "import c.Thing;"],
        &[("app", "xa"), ("app", "c"), ("c", "app")],
    );
    // A pair across parts the other way, so the edges hold two.
    write(
        dir.path(),
        "c/src/main/java/c/Thing.java",
        "package c;\n\nimport app.Main;\n\npublic class Thing {}\n",
    );
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Thing"]);
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
    assert_eq!(value["unsure"].as_array().map(Vec::len), Some(1), "{value}");
    assert_eq!(
        value["edges"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| format!("{}>{}", e["from"], e["to"]))
            .collect::<Vec<_>>(),
        ["\"app\">\"c\"", "\"c\">\"app\""],
        "{value}"
    );
    // An edit not committed changes nothing at the commit.
    write(dir.path(), MAIN_APP, "package app;\n");
    assert_eq!(
        String::from_utf8_lossy(&run(&root, "check", &["--json", "--commit", "HEAD"]).stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
}

#[test]
fn a_mixed_java_and_typescript_repository_reads_each_languages_parts_side_by_side() {
    let mut mixed = map(
        &[
            ("jlib", &["lib/"]),
            ("japp", &["a/"]),
            ("tslib", &["web/lib/"]),
            ("tsapp", &["web/app/"]),
        ],
        &[("japp", "jlib"), ("tsapp", "tslib")],
        &[],
    );
    mixed["extractors"] = json!(["java-imports", "typescript-imports"]);
    let (_dir, root) = repo(
        &mixed,
        &[
            (MAIN, &main_importing(&["import lib.Util;"])),
            (
                "lib/src/main/java/lib/Util.java",
                "package lib;\n\npublic class Util {}\n",
            ),
            ("web/app/main.ts", "import { x } from \"../lib/x\";\n"),
            ("web/lib/x.ts", "export const x = 1;\n"),
        ],
    );
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(
        pairs(&found),
        [
            format!("{MAIN}>lib/src/main/java/lib/Util.java"),
            "web/app/main.ts>web/lib/x.ts".to_string()
        ]
    );
    assert_eq!(
        (found.counts.links.proven, found.counts.links.unproven),
        (2, 0)
    );
}

// `import module`.

/// A class of package `lib` in `folder`'s source root.
fn module_class(folder: &str, name: &str) -> (String, String) {
    (
        format!("{folder}/lib/{name}.java"),
        format!("package lib;\n\npublic class {name} {{}}\n"),
    )
}

/// The Unsure findings, as `name reason [candidates]`, of `MAIN` importing `import_line`, among `files`, with the
/// paths in `ignored` ignored.
fn module_unsure(import_line: &str, files: &[(String, String)], ignored: &[&str]) -> Vec<String> {
    let main = main_importing(&[import_line]);
    let mut all: Vec<(&str, &str)> = vec![(MAIN, &main)];
    all.extend(files.iter().map(|(f, t)| (f.as_str(), t.as_str())));
    let (_dir, root) = repo(&map(&[("b", &["**"]), ("a", &["a/"])], &[], ignored), &all);
    let found = report(&root);
    assert_eq!(found.problems, Vec::<String>::new());
    assert_eq!(pairs(&found), Vec::<String>::new());
    found
        .unsure
        .iter()
        .map(|u| {
            assert_eq!(u.file, MAIN);
            format!("{} {} {:?}", u.name, u.reason, u.candidates)
        })
        .collect()
}

fn module_info(folder: &str, text: &str) -> (String, String) {
    (format!("{folder}/module-info.java"), text.to_string())
}

#[test]
fn a_module_import_of_a_declared_module_makes_the_importer_unsure_of_every_file_under_its_root() {
    let files = [
        module_info("m/src/main/java", "module m.core {\n  exports lib;\n}\n"),
        module_class("m/src/main/java", "A"),
        module_class("m/src/main/java", "B"),
        module_class("other/src/main/java", "C"),
    ];
    assert_eq!(
        module_unsure("import module m.core;", &files, &[]),
        [r#"m.core module import ["m/src/main/java/lib/A.java", "m/src/main/java/lib/B.java"]"#]
    );
}

#[test]
fn a_module_the_repository_does_not_declare_or_whose_root_holds_no_indexed_file_gives_no_finding() {
    let files = [
        module_info("m/src/main/java", "module m.core {}\n"),
        module_info("empty/src/main/java", "module m.empty {}\n"),
        module_class("m/src/main/java", "A"),
    ];
    assert_eq!(
        module_unsure("import module java.base;", &files, &[]),
        Vec::<String>::new()
    );
    assert_eq!(
        module_unsure("import module m.empty;", &files, &[]),
        Vec::<String>::new()
    );
}

#[test]
fn a_skipped_or_ignored_module_info_declares_no_module() {
    let files = [
        module_info("m/src/test/java", "module m.core {}\n"),
        module_info("n/src/main/java", "module m.hidden {}\n"),
        module_class("m/src/test/java", "T"),
        module_class("n/src/main/java", "N"),
    ];
    let ignored = ["n/src/main/java/module-info.java"];
    assert_eq!(
        module_unsure("import module m.core;", &files, &ignored),
        Vec::<String>::new()
    );
    assert_eq!(
        module_unsure("import module m.hidden;", &files, &ignored),
        Vec::<String>::new()
    );
}

#[test]
fn two_declarers_of_one_module_name_give_candidates_from_both_roots() {
    let files = [
        module_info("m/src/main/java", "open module m.core {}\n"),
        module_info(
            "copy/src/main/java",
            "import java.util.List;\nmodule m.core {}\n",
        ),
        module_class("m/src/main/java", "A"),
        module_class("copy/src/main/java", "A"),
    ];
    assert_eq!(
        module_unsure("import module m.core;", &files, &[]),
        [r#"m.core module import ["copy/src/main/java/lib/A.java", "m/src/main/java/lib/A.java"]"#]
    );
}
