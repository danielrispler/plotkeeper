//! `c-includes`: C and C++ repositories in temporary folders, read through `check` (spec #806, "Testing Decisions"):
//! the reader's lexing, the thirteen extensions, the includes next to the including file, skipped files and detection.

use serde_json::{Value, json};

use crate::common::{
    assert_only_candidate_links_relaxed, extractor_map, pairs, repo, report, run, sites, write,
};

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
        // Not next to the file: the path-ending match settles an angle bracket and a root-relative path.
        ("#include <x.h>\n", from_main(&["src/x.h"])),
        ("#include \"src/x.h\"\n", from_main(&["src/x.h"])),
        ("#include <src/x.h>\n", from_main(&["src/x.h"])),
        // A path naming no tracked file, and a climb above the root.
        ("#include \"gone.h\"\n", vec![]),
        ("#include \"../../top.h\"\n", vec![]),
        // A `..` path that does not resolve next to the file never reaches the path-ending match.
        ("#include \"../x.h\"\n", vec![]),
        ("#include <../x.h>\n", vec![]),
        ("#include \"sub/../../x.h\"\n", vec![]),
        // An empty, absolute or backslashed path.
        ("#include \"\"\n", vec![]),
        ("#include \"/src/x.h\"\n", vec![]),
        ("#include \"sub\\\\..\\\\x.h\"\n", vec![]),
        ("#include \"\\\\x.h\"\n", vec![]),
        ("#include \".\"\n", vec![]),
        ("#include <>\n", vec![]),
    ] {
        let files = [("src/a.c", text), ("src/x.h", ""), ("top.h", "")];
        let (_dir, root) = repo(&per_file(&files, &[]), &files);
        let found = report(&root);
        assert_eq!(pairs(&found), expected, "{text}");
        // An include that pairs with nothing is no problem either.
        if expected.is_empty() {
            assert_eq!(found.problems, Vec::<String>::new(), "{text}");
        }
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
fn an_ignored_or_skipped_file_gives_no_pair_and_an_adjacent_one_stops_the_search() {
    let text = "#include \"x.h\"\n";
    // The files, the ignored folders, and whether `src/a.c` is Unsure.
    for (files, ignored, unsure) in [
        // An ignored copy elsewhere, which keeps the include Unsure.
        (
            vec![("src/a.c", text), ("src/gen/x.h", ""), ("x.h", "")],
            vec!["src/gen/"],
            true,
        ),
        // An ignored sibling stops the search before a live copy elsewhere.
        (
            vec![("src/a.c", text), ("src/x.h", ""), ("lib/x.h", "")],
            vec!["src/x.h"],
            false,
        ),
        // A skipped sibling: `test_x.h` is named like a test file.
        (
            vec![("src/a.c", "#include \"test_x.h\"\n"), ("src/test_x.h", "")],
            vec![],
            false,
        ),
        // An including file the map ignores.
        (
            vec![("gen/a.c", text), ("gen/x.h", ""), ("b.c", "")],
            vec!["gen/"],
            false,
        ),
    ] {
        let ignored: Vec<&str> = ignored;
        let (_dir, root) = repo(&per_file(&files, &ignored), &files);
        let found = report(&root);
        assert_eq!(pairs(&found), Vec::<String>::new(), "{files:?}");
        assert_eq!(!found.unsure.is_empty(), unsure, "{files:?}");
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
    let cases: [(&str, &str, &[u32]); 33] = [
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
        (
            "a comment spanning lines after code",
            "int a; /* c\n c */ #include \"x.h\"\n",
            &[],
        ),
        // A `'` inside a number is a digit separator, not a character literal.
        (
            "a digit separator",
            "int a = 1'000; /* c\n#include \"x.h\"\n*/\n",
            &[],
        ),
        (
            "a digit separator before a raw string",
            "int a = 1'0 + R\"(\n#include \"x.h\"\n)\";\n",
            &[],
        ),
        (
            "a hex digit separator",
            "int a = 0xFF'FF; /* c\n#include \"x.h\"\n*/\n",
            &[],
        ),
        (
            "a binary digit separator",
            "int a = 0b1'0; /* c\n#include \"x.h\"\n*/\n",
            &[],
        ),
        // A prefixed character literal still opens one, so the `/*` inside it opens no comment.
        (
            "a u8 char literal",
            "char c = u8'/*';\n#include \"x.h\"\n",
            &[2],
        ),
        (
            "an L char literal",
            "char c = L'/*';\n#include \"x.h\"\n",
            &[2],
        ),
        (
            "a u char literal",
            "char c = u'/*';\n#include \"x.h\"\n",
            &[2],
        ),
        (
            "a U char literal",
            "char c = U'/*';\n#include \"x.h\"\n",
            &[2],
        ),
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
    // Each file includes its sibling `sib.h`, and its part `t` links to `lib`, which it does not include.
    let text = "#include \"sib.h\"\n";
    for (file, is_read) in skipped
        .iter()
        .map(|f| (f, false))
        .chain(read.iter().map(|f| (f, true)))
    {
        let to = "lib/x.h";
        let from_dir = file.rsplit_once('/').unwrap().0;
        let files = [
            (*file, text),
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

/// The files that make `n` unique includes of `<{tag}{i}.h>`, each header in `folder` and each included from a source
/// of its own.
fn evidence(folder: &str, tag: &str, n: usize) -> Vec<(String, String)> {
    (0..n)
        .flat_map(|i| {
            [
                (format!("m/{tag}e{i}.c"), format!("#include <{tag}{i}.h>\n")),
                (format!("{folder}{tag}{i}.h"), String::new()),
            ]
        })
        .collect()
}

fn owned(files: &[(String, String)]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect()
}

#[test]
fn a_system_name_gives_nothing_for_an_angle_bracket_include_only() {
    for (include, tracked) in [
        ("<string.h>", "util/string.h"),
        ("<sys/time.h>", "compat/sys/time.h"),
        ("<windows.h>", "win/windows.h"),
        ("<cpuid.h>", "compat/cpuid.h"),
        ("<vector>", "util/vector"),
        ("<linux/x.h>", "compat/linux/x.h"),
        ("<arpa/inet.h>", "compat/arpa/inet.h"),
        ("<memory.h>", "util/memory.h"),
        ("<direct.h>", "util/direct.h"),
    ] {
        let text = format!("#include {include}\n");
        let found = c_pairs(&[("src/a.c", &text), (tracked, "")], &[]);
        assert_eq!(found, Vec::<String>::new(), "{include}");
    }
    // A name not on the list matches, and a quoted include of a listed name does too, elsewhere or next to the file.
    for (include, tracked) in [
        ("<mystring.h>", "util/mystring.h"),
        ("\"string.h\"", "util/string.h"),
        ("\"string.h\"", "src/string.h"),
    ] {
        let text = format!("#include {include}\n");
        let found = c_pairs(&[("src/a.c", &text), (tracked, "")], &[]);
        assert_eq!(found, [format!("src/a.c>{tracked}")], "{include}");
    }
}

#[test]
fn candidates_are_files_whose_path_ends_with_the_include_after_a_slash() {
    for (text, files, expected) in [
        // The path ends the candidate's path only at a folder boundary.
        ("#include <x.h>\n", vec![("lib/ax.h", "")], vec![]),
        ("#include <b/x.h>\n", vec![("lib/ab/x.h", "")], vec![]),
        (
            "#include <b/x.h>\n",
            vec![("lib/a/b/x.h", "")],
            vec!["lib/a/b/x.h"],
        ),
        ("#include <x.h>\n", vec![("x.h", "")], vec!["x.h"]),
        // A skipped file is not a candidate, so the other file is the only one.
        (
            "#include <x.h>\n",
            vec![("lib/x.h", ""), ("tests/x.h", "")],
            vec!["lib/x.h"],
        ),
        ("#include <x.h>\n", vec![("tests/x.h", "")], vec![]),
        // A tie between a production header and test ones is the production one.
        (
            "#include <x.h>\n",
            vec![("src/x.h", ""), ("tests/x.h", ""), ("fuzz/x.h", "")],
            vec!["src/x.h"],
        ),
        // A sibling wins over a root-relative match elsewhere.
        (
            "#include \"db/x.h\"\n",
            vec![("src/db/x.h", ""), ("db/x.h", "")],
            vec!["src/db/x.h"],
        ),
        // An angle bracket skips the including file's folder: with its sibling and another copy it is Unsure.
        (
            "#include <x.h>\n",
            vec![("src/x.h", ""), ("lib/x.h", "")],
            vec![],
        ),
    ] {
        let mut all = vec![("src/a.c", text)];
        all.extend(files);
        let found = c_pairs(&all, &[]);
        let expected: Vec<String> = expected.iter().map(|t| format!("src/a.c>{t}")).collect();
        assert_eq!(found, expected, "{text} {all:?}");
    }
}

#[test]
fn a_tie_is_settled_by_the_folder_five_unique_matches_use() {
    for (name, ev, extra, expected) in [
        ("five settle it", 5, 0, Some("src/chain.h")),
        ("four do not", 4, 0, None),
        ("two folders with five each stay unsure", 5, 5, None),
    ] {
        let mut files = evidence("src/", "a", ev);
        files.extend(evidence("src/kernel/", "b", extra));
        files.push(("src/chain.h".into(), String::new()));
        files.push(("src/kernel/chain.h".into(), String::new()));
        files.push(("src/kernel/c.cc".into(), "#include <chain.h>\n".into()));
        let found: Vec<String> = c_pairs(&owned(&files), &[])
            .into_iter()
            .filter(|p| p.starts_with("src/kernel/c.cc"))
            .collect();
        let expected: Vec<String> = expected
            .iter()
            .map(|t| format!("src/kernel/c.cc>{t}"))
            .collect();
        assert_eq!(found, expected, "{name}");
    }
}

#[test]
fn evidence_does_not_depend_on_file_order() {
    // The includes that are the evidence sort after the tie's file, and before it.
    for prefix in ["a", "z"] {
        let mut files: Vec<(String, String)> = (0..5)
            .flat_map(|i| {
                [
                    (format!("{prefix}/e{i}.c"), format!("#include <u{i}.h>\n")),
                    (format!("lib/u{i}.h"), String::new()),
                ]
            })
            .collect();
        files.push(("lib/chain.h".into(), String::new()));
        files.push(("lib/kernel/chain.h".into(), String::new()));
        files.push(("m/c.cc".into(), "#include <chain.h>\n".into()));
        let found: Vec<String> = c_pairs(&owned(&files), &[])
            .into_iter()
            .filter(|p| p.starts_with("m/c.cc"))
            .collect();
        assert_eq!(found, ["m/c.cc>lib/chain.h"], "{prefix}");
    }
}

#[test]
fn an_ignored_copy_keeps_the_include_unsure_even_with_evidence_for_the_live_one() {
    let mut files = evidence("src/", "a", 5);
    files.push(("src/sds.h".into(), String::new()));
    files.push(("deps/hiredis/sds.h".into(), String::new()));
    files.push(("src/cli.c".into(), "#include <sds.h>\n".into()));
    let files = owned(&files);
    let found = c_pairs(&files, &["deps/"]);
    assert!(
        !found.iter().any(|p| p.starts_with("src/cli.c")),
        "{found:?}"
    );
    let (_dir, root) = repo(&per_file(&files, &["deps/"]), &files);
    let value: Value = serde_json::from_slice(&run(&root, "check", &["--json"]).stdout).unwrap();
    assert_eq!(
        value["unsure"],
        json!([{
            "file": "src/cli.c",
            "name": "sds.h",
            "reason": "an ignored copy matches it",
            "candidates": ["src/sds.h"]
        }])
    );
}

#[test]
fn an_include_matching_only_an_ignored_file_gives_nothing_and_no_unsure() {
    let files = [("src/a.c", "#include <sds.h>\n"), ("deps/sds.h", "")];
    assert_eq!(c_pairs(&files, &["deps/"]), Vec::<String>::new());
    let (_dir, root) = repo(&per_file(&files, &["deps/"]), &files);
    let value: Value = serde_json::from_slice(&run(&root, "check", &["--json"]).stdout).unwrap();
    assert!(value["unsure"].is_null(), "{value}");
}

/// A repository where `app` includes `<x.h>`, which two other parts hold, and a third part `other` holds a file `app`
/// also links to.
fn two_x() -> (tempfile::TempDir, String) {
    let files = [
        ("app/a.c", "#include <x.h>\n"),
        ("xa/x.h", ""),
        ("xb/x.h", ""),
        ("other/o.h", ""),
    ];
    repo(
        &extractor_map(
            "c-includes",
            &[
                ("app", &["app/"]),
                ("xa", &["xa/"]),
                ("xb", &["xb/"]),
                ("other", &["other/"]),
            ],
            &[("app", "xa"), ("app", "xb"), ("app", "other")],
            &[],
        ),
        &files,
    )
}

#[test]
fn several_matches_make_the_importer_unsure_and_relax_only_the_candidates_links() {
    let (_dir, root) = two_x();
    assert_only_candidate_links_relaxed(
        &root,
        "note: app/a.c is unsure of x.h; its links to xa, xb pass as unproven\n",
        "app",
        "other",
    );
    let value: Value = serde_json::from_slice(&run(&root, "check", &["--json"]).stdout).unwrap();
    assert_eq!(
        value["unsure"],
        json!([{
            "file": "app/a.c",
            "name": "x.h",
            "reason": "several files match it",
            "candidates": ["xa/x.h", "xb/x.h"]
        }])
    );
    assert_eq!(value["edges"], json!([]));
}
