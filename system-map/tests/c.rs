//! `c-includes`: C and C++ repositories in temporary folders, read through `check` (spec #806, "Testing Decisions"):
//! the reader's lexing, the thirteen extensions, the includes next to the including file, skipped files and detection.

use serde_json::{Value, json};

use crate::common::{extractor_map, pairs, repo, report, run, sites, write};

/// A map naming `c-includes` that gives every tracked file but `ignored` ones its own part, so that a file pair is a
/// pair of parts and shows in the pairs, and with no link.
fn per_file(files: &[(&str, &str)], ignored: &[&str]) -> Value {
    let owned: Vec<String> = files
        .iter()
        .map(|(f, _)| f.to_string())
        .filter(|f| !ignored.iter().any(|i| f.starts_with(i)))
        .collect();
    let ids: Vec<String> = (0..owned.len()).map(|i| format!("p{i}")).collect();
    let parts: Vec<(&str, Vec<&str>)> = ids
        .iter()
        .zip(&owned)
        .map(|(id, f)| (id.as_str(), vec![f.as_str()]))
        .collect();
    let parts: Vec<(&str, &[&str])> = parts.iter().map(|(id, p)| (*id, p.as_slice())).collect();
    extractor_map("c-includes", &parts, &[], ignored)
}

/// The file pairs `files` give, with the folders `ignored` ignored.
fn c_pairs(files: &[(&str, &str)], ignored: &[&str]) -> Vec<String> {
    let (_dir, root) = repo(&per_file(files, ignored), files);
    pairs(&report(&root))
}

/// The pairs of `src/a.c` holding `text`, with `src/x.h` and `top.h` tracked beside it.
fn pairs_of(text: &str) -> Vec<String> {
    c_pairs(&[("src/a.c", text), ("src/x.h", ""), ("top.h", "")], &[])
}

const A_TO_X: &str = "src/a.c>src/x.h";

#[test]
fn the_thirteen_extensions_are_read_in_any_case_and_no_others() {
    let read = [
        "c", "h", "cc", "cpp", "cxx", "c++", "hpp", "hh", "hxx", "h++", "inl", "ipp", "tcc", "C",
        "HPP", "Cc",
    ];
    for ext in read {
        let from = format!("src/a.{ext}");
        let found = c_pairs(&[(&from, "#include \"x.h\"\n"), ("src/x.h", "")], &[]);
        assert_eq!(found, [format!("{from}>src/x.h")], ".{ext}");
    }
    for ext in ["m", "mm", "cu", "inc", "def", "rc", "txt"] {
        let from = format!("src/a.{ext}");
        let found = c_pairs(&[(&from, "#include \"x.h\"\n"), ("src/x.h", "")], &[]);
        assert_eq!(found, Vec::<String>::new(), ".{ext}");
    }
}

#[test]
fn a_quoted_include_pairs_with_a_file_next_to_the_including_file() {
    let from_main = |to: &[&str]| {
        to.iter()
            .map(|t| format!("src/a.c>{t}"))
            .collect::<Vec<_>>()
    };
    for (text, expected) in [
        ("#include \"x.h\"\n", from_main(&["src/x.h"])),
        ("#include \"./x.h\"\n", from_main(&["src/x.h"])),
        ("#include \"sub/../x.h\"\n", from_main(&["src/x.h"])),
        ("#include \"../top.h\"\n", from_main(&["top.h"])),
        (
            "#include \"x.h\"\n#include \"../top.h\"\n",
            from_main(&["src/x.h", "top.h"]),
        ),
        // Not next to the file, and no later step yet: angle brackets, a root-relative path, a missing file.
        ("#include <x.h>\n", vec![]),
        ("#include \"src/x.h\"\n", vec![]),
        ("#include \"gone.h\"\n", vec![]),
        // A climb above the root.
        ("#include \"../../top.h\"\n", vec![]),
    ] {
        assert_eq!(pairs_of(text), expected, "{text}");
    }
}

#[test]
fn the_pair_carries_the_line_of_the_directive() {
    let (_dir, root) = repo(
        &per_file(&[("src/a.c", ""), ("src/x.h", "")], &[]),
        &[("src/a.c", "int a;\n\n#include \"x.h\"\n"), ("src/x.h", "")],
    );
    assert_eq!(sites(&root, &[]), ["src/a.c>src/x.h src/a.c:3"]);
}

#[test]
fn an_adjacent_file_the_map_ignores_or_the_reader_skips_gives_no_pair_and_stops_the_search() {
    let text = "#include \"x.h\"\n";
    for (files, ignored) in [
        // An ignored sibling.
        (
            vec![("src/a.c", text), ("src/gen/x.h", ""), ("x.h", "")],
            vec!["src/gen/"],
        ),
        // A skipped sibling: `test_x.h` is named like a test file.
        (
            vec![("src/a.c", "#include \"test_x.h\"\n"), ("src/test_x.h", "")],
            vec![],
        ),
    ] {
        let ignored: Vec<&str> = ignored;
        let found = c_pairs(&files, &ignored);
        assert_eq!(found, Vec::<String>::new(), "{files:?}");
    }
    // The ignored sibling itself.
    let found = c_pairs(
        &[
            ("src/a.c", text),
            ("src/x.h", ""),
            ("gen/b.c", "#include \"x.h\"\n"),
            ("gen/x.h", ""),
        ],
        &["gen/"],
    );
    assert_eq!(found, [A_TO_X]);
}

#[test]
fn the_lexer_reads_what_the_preprocessor_reads() {
    // The lines of `src/a.c` that pair with `src/x.h`, for the text of `src/a.c`.
    let cases: [(&str, &str, &[u32]); 24] = [
        ("a line comment", "// #include \"x.h\"\n", &[]),
        ("a block comment", "/* \n#include \"x.h\"\n*/\n", &[]),
        (
            "a block comment, then code",
            "/* c */\n#include \"x.h\"\n",
            &[2],
        ),
        ("an unclosed block comment", "/* \n#include \"x.h\"\n", &[]),
        (
            "a string",
            "const char *s = \"\n#include \\\"x.h\\\"\n\";\n",
            &[],
        ),
        (
            "a string with an include inside",
            "char *s = \"#include \\\"x.h\\\"\";\n",
            &[],
        ),
        (
            "a raw string",
            "const char *s = R\"d(\n#include \"x.h\"\n)d\";\n",
            &[],
        ),
        (
            "code after a raw string",
            "const char *s = R\"d(x)d\";\n#include \"x.h\"\n",
            &[2],
        ),
        ("a char literal", "char c = '\"';\n#include \"x.h\"\n", &[2]),
        ("spaces after the hash", "#  include \"x.h\"\n", &[1]),
        ("a space before the hash", "  # include \"x.h\"\n", &[1]),
        ("tabs", "#\tinclude\t\"x.h\"\n", &[1]),
        ("a backslash continuation", "#include \\\n  \"x.h\"\n", &[1]),
        (
            "a continuation moves the later lines",
            "#define A \\\n  1\n#include \"x.h\"\n",
            &[3],
        ),
        ("a macro", "#include MACRO\n", &[]),
        ("include_next", "#include_next \"x.h\"\n", &[]),
        ("import", "#import \"x.h\"\n", &[]),
        ("has_include", "#if __has_include(\"x.h\")\n#endif\n", &[]),
        ("the word is not a directive", "#includes \"x.h\"\n", &[]),
        (
            "both branches",
            "#if A\n#include \"x.h\"\n#else\n#include \"x.h\"\n#endif\n",
            &[2, 4],
        ),
        ("if zero", "#if 0\n#include \"x.h\"\n#endif\n", &[2]),
        ("not at a line start", "int a; #include \"x.h\"\n", &[]),
        (
            "a comment before the hash",
            "/* c */ #include \"x.h\"\n",
            &[1],
        ),
        ("a closing quote missing", "#include \"x.h\n", &[]),
    ];
    for (name, text, lines) in cases {
        let (_dir, root) = repo(
            &per_file(&[("src/a.c", ""), ("src/x.h", "")], &[]),
            &[("src/a.c", text), ("src/x.h", "")],
        );
        let expected: Vec<String> = lines
            .iter()
            .map(|l| format!("src/a.c>src/x.h src/a.c:{l}"))
            .collect();
        assert_eq!(sites(&root, &[]), expected, "{name}");
    }
}

#[test]
fn an_empty_absolute_or_backslashed_path_gives_nothing() {
    for text in [
        "#include \"\"\n",
        "#include \"/src/x.h\"\n",
        "#include \"sub\\\\..\\\\x.h\"\n",
        "#include \"\\\\x.h\"\n",
        "#include \".\"\n",
        "#include <>\n",
    ] {
        assert_eq!(pairs_of(text), Vec::<String>::new(), "{text}");
    }
}

#[test]
fn includes_inside_an_ignored_file_give_nothing() {
    let found = c_pairs(
        &[
            ("gen/a.c", "#include \"x.h\"\n"),
            ("gen/x.h", ""),
            ("b.c", ""),
        ],
        &["gen/"],
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_test_file_makes_no_pair_and_does_not_make_its_part_read() {
    let skipped = [
        "tests/a.c",
        "src/Test/a.c",
        "fuzz/a.c",
        "src/testing/a.c",
        "src/unittest/a.c",
        "src/a_test.cc",
        "src/a_tests.cc",
        "src/test_a.c",
    ];
    let read = ["src/testutil.cc", "bench/a.cc", "src/attest.c", "src/a.c"];
    for (file, is_read) in skipped
        .iter()
        .map(|f| (f, false))
        .chain(read.iter().map(|f| (f, true)))
    {
        let to = "lib/x.h";
        let from_dir = file.rsplit_once('/').unwrap().0;
        // A skipped file includes a sibling and `lib/x.h` through its own folder's copy.
        let text = format!("#include \"{}\"\n", "sib.h");
        let files = [
            (*file, text.as_str()),
            (&format!("{from_dir}/sib.h")[..], ""),
            (to, ""),
        ];
        let mut map = extractor_map(
            "c-includes",
            &[
                ("t", &[*file]),
                ("lib", &["lib/"]),
                ("sib", &[&format!("{from_dir}/sib.h")]),
            ],
            &[("t", "lib")],
            &[],
        );
        map["ignored"] = json!(["system-map.json"]);
        let (_dir, root) = repo(&map, &files);
        let found = report(&root);
        let edges = pairs(&found);
        let sib_pairs = edges.iter().any(|p| p.starts_with(file));
        let link_fails = found
            .problems
            .iter()
            .any(|p| p.contains("runs from \"t\" to \"lib\""));
        assert_eq!(sib_pairs, is_read, "{file}: {edges:?}");
        // A read part's link without a dependency fails; an unread part's link passes as unproven.
        assert_eq!(link_fails, is_read, "{file}: {:?}", found.problems);
    }
}

#[test]
fn check_suggests_c_includes_only_for_a_repository_with_a_production_c_file_the_map_does_not_ignore()
 {
    let note = "note: c-includes could read this repository; add it to extractors\n";
    let mut ignoring = extractor_map("c-includes", &[("a", &["**"])], &[], &["gen/"]);
    ignoring["extractors"] = json!([]);
    for (files, expected) in [
        (&[("src/a.c", "")][..], note),
        (&[("src/a.HPP", "")], note),
        (
            &[("tests/a.c", ""), ("fuzz/b.c", ""), ("src/a_test.cc", "")],
            "",
        ),
        (&[("gen/a.c", "")], ""),
        (&[("src/a.m", "")], ""),
        (&[("tests/a.c", ""), ("src/b.c", "")], note),
    ] {
        let (_dir, root) = repo(&ignoring, files);
        let out = run(&root, "check", &[]);
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected, "{files:?}");
    }
}

#[test]
fn commit_mode_gives_the_pairs_of_the_working_tree() {
    let files = [
        ("a/a.c", "#include \"../b/b.h\"\n"),
        ("b/b.h", "#include \"../a/a.h\"\n"),
        ("a/a.h", ""),
    ];
    let (dir, root) = repo(
        &extractor_map(
            "c-includes",
            &[("a", &["a/"]), ("b", &["b/"])],
            &[("a", "b"), ("b", "a")],
            &[],
        ),
        &files,
    );
    let tree = run(&root, "check", &["--json"]);
    let commit = run(&root, "check", &["--json", "--commit", "HEAD"]);
    assert_eq!(
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&tree.stdout)
    );
    assert_eq!(
        sites(&root, &["--commit", "HEAD"]),
        ["a/a.c>b/b.h a/a.c:1", "b/b.h>a/a.h b/b.h:1"]
    );
    // An edit not committed changes nothing at the commit.
    write(dir.path(), "a/a.c", "int a;\n");
    assert_eq!(
        sites(&root, &["--commit", "HEAD"]),
        ["a/a.c>b/b.h a/a.c:1", "b/b.h>a/a.h b/b.h:1"]
    );
}
