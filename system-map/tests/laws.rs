//! Law tags: a law's backing is held to the tests that name it.

use std::path::Path;

use serde_json::{Value, json};

use crate::common::{FILES, base, has, problems, project, write};
use studio_test_support::git;

/// A tag line for `id`. The tag's marker is spliced in, so that no line of this file is itself a
/// tag line: the checker runs on this repository and would count it.
fn tag(id: &str) -> String {
    format!("// @{}: {id}\n", "law")
}

fn with_backing(backing: &str) -> Value {
    let mut map = base();
    map["laws"][0]["backing"] = json!(backing);
    map
}

fn commit(root: &str, file: &str, text: &str) {
    write(Path::new(root), file, text);
    git(Path::new(root), &["add", "-A"]);
    git(Path::new(root), &["commit", "-q", "-m", "More"]);
}

#[test]
fn a_law_that_says_check_with_no_tag_fails_naming_the_law() {
    let (_dir, root) = project(&with_backing("check"), &FILES);
    assert!(has(&problems(&root), &["stay-true", "check"]));
}

#[test]
fn a_law_that_says_check_with_a_tag_passes_and_lists_the_tag() {
    let (_dir, root) = project(&with_backing("check"), &FILES);
    commit(
        &root,
        "alpha/a.rs",
        &format!("fn a() {{}}\n{}fn b() {{}}\n", tag("stay-true")),
    );
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    let law = report.laws.iter().find(|l| l.id == "stay-true").unwrap();
    assert_eq!(law.tags.len(), 1);
    assert_eq!(law.tags[0].file, "alpha/a.rs");
    assert_eq!(law.tags[0].line, 2);
}

#[test]
fn a_law_on_trust_with_no_tag_passes() {
    let (_dir, root) = project(&with_backing("trust"), &FILES);
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    assert!(report.laws[0].tags.is_empty());
}

#[test]
fn a_law_on_trust_with_a_tag_fails_naming_the_law_and_the_tag() {
    let (_dir, root) = project(&with_backing("trust"), &FILES);
    commit(&root, "beta/b.rs", &format!("\n\n{}", tag("stay-true")));
    assert!(has(
        &problems(&root),
        &["stay-true", "trust", "beta/b.rs:3"]
    ));
}

#[test]
fn a_tag_naming_no_law_fails_naming_the_tag() {
    let (_dir, root) = project(&with_backing("trust"), &FILES);
    commit(&root, "beta/b.rs", &tag("no-such-law"));
    assert!(has(&problems(&root), &["no-such-law", "beta/b.rs:1"]));
}

#[test]
fn tags_are_found_in_ignored_paths_and_one_file_may_carry_several() {
    let (_dir, root) = project(&with_backing("check"), &FILES);
    let two = format!("{}{}", tag("stay-true"), tag("stay-true"));
    commit(&root, "docs/d.md", &two);
    commit(&root, "alpha/a.rs", &tag("stay-true"));
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    let lines: Vec<_> = report.laws[0]
        .tags
        .iter()
        .map(|t| (t.file.as_str(), t.line))
        .collect();
    assert_eq!(
        lines,
        [("alpha/a.rs", 1), ("docs/d.md", 1), ("docs/d.md", 2)]
    );
}

#[test]
fn a_tag_in_an_untracked_file_is_not_counted() {
    let (dir, root) = project(&with_backing("check"), &FILES);
    write(dir.path(), "alpha/untracked.rs", &tag("stay-true"));
    assert!(has(&problems(&root), &["stay-true", "check"]));
}

#[test]
fn only_a_comment_line_of_its_own_is_a_tag() {
    let (_dir, root) = project(&with_backing("trust"), &FILES);
    let marker = format!("@{}:", "law");
    let text = format!(
        "let s = \"// {marker} stay-true\";\nprintln!(\"{marker} stay-true\");\nsee {marker} stay-true\n"
    );
    commit(&root, "alpha/a.rs", &text);
    assert_eq!(problems(&root), Vec::<String>::new());
}

#[test]
fn comment_leaders_of_other_languages_and_indentation_are_accepted() {
    let (_dir, root) = project(&with_backing("trust"), &FILES);
    let m = format!("@{}:", "law");
    for (i, line) in [
        format!("    /// {m} stay-true"),
        format!("# {m} stay-true"),
        format!(" * {m} stay-true"),
        format!("/* {m} stay-true */"),
    ]
    .iter()
    .enumerate()
    {
        let file = format!("alpha/f{i}.txt");
        commit(&root, &file, &format!("{line}\n"));
        assert!(has(&problems(&root), &[&file, "stay-true"]), "{line}");
    }
}

#[test]
fn a_tag_behind_a_tracked_symlink_to_a_file_outside_the_repository_is_not_read() {
    let (_dir, root) = project(&with_backing("trust"), &FILES);
    let outside = tempfile::tempdir().unwrap();
    write(outside.path(), "secret.rs", &tag("stay-true"));
    std::os::unix::fs::symlink(
        outside.path().join("secret.rs"),
        Path::new(&root).join("alpha/link.rs"),
    )
    .unwrap();
    git(Path::new(&root), &["add", "-A"]);
    git(Path::new(&root), &["commit", "-q", "-m", "Link"]);
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    assert!(report.laws[0].tags.is_empty());
    let alpha = report.parts.iter().find(|p| p.id == "alpha").unwrap();
    assert!(alpha.files.contains(&"alpha/link.rs".to_string()));
}

#[test]
fn a_tag_in_a_file_that_is_not_valid_utf8_is_still_found() {
    let (_dir, root) = project(&with_backing("check"), &FILES);
    let mut bytes = b"\xff\xfe junk\n".to_vec();
    bytes.extend(tag("stay-true").as_bytes());
    std::fs::write(Path::new(&root).join("alpha/a.rs"), bytes).unwrap();
    git(Path::new(&root), &["add", "-A"]);
    git(Path::new(&root), &["commit", "-q", "-m", "Bytes"]);
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(report.laws[0].tags.len(), 1);
    assert_eq!(report.laws[0].tags[0].line, 2);
}

mod resolver {
    use system_map::{LawTest, Tag, law_tests, test_name_after};

    use super::tag;
    use crate::common::{base, project, write};

    fn after(source: &str) -> Option<String> {
        test_name_after(source, 1)
    }

    #[test]
    fn a_rust_test_after_its_attribute_gives_the_fn_name() {
        let source = format!("{}#[test]\nfn it_holds() {{}}\n", tag("x"));
        assert_eq!(after(&source), Some("it_holds".into()));
    }

    #[test]
    fn blank_lines_comments_and_attributes_are_skipped() {
        let source = format!(
            "{}\n// a note\n/// a doc\n#[test]\n#[ignore = \"slow\"]\n\nasync fn later() {{}}\n",
            tag("x")
        );
        assert_eq!(after(&source), Some("later".into()));
        let multi = format!(
            "{}#[cfg_attr(\n    all(),\n    test\n)]\npub(crate) fn wide() {{}}\n",
            tag("x")
        );
        assert_eq!(after(&multi), Some("wide".into()));
    }

    #[test]
    fn several_tags_above_one_test_all_resolve_to_it() {
        let source = format!("{}{}#[test]\nfn both() {{}}\n", tag("a"), tag("b"));
        assert_eq!(test_name_after(&source, 1), Some("both".into()));
        assert_eq!(test_name_after(&source, 2), Some("both".into()));
    }

    #[test]
    fn a_node_test_call_gives_its_title_in_either_quote() {
        assert_eq!(
            after(&format!(
                "{}test(\"a title, with commas\", async () => {{}});\n",
                tag("x")
            )),
            Some("a title, with commas".into())
        );
        assert_eq!(
            after(&format!("{}test('single', () => {{}});\n", tag("x"))),
            Some("single".into())
        );
    }

    #[test]
    fn a_tag_with_no_test_after_it_resolves_to_nothing() {
        assert_eq!(after(&tag("x")), None);
        assert_eq!(after(&format!("{}const x = 1;\n", tag("x"))), None);
        assert_eq!(
            after(&format!("{}struct A;\nfn not_next() {{}}\n", tag("x"))),
            None
        );
    }

    #[test]
    fn only_the_test_right_after_the_tag_is_named() {
        let source = format!(
            "fn before() {{}}\n{}#[test]\nfn mine() {{}}\nfn after() {{}}\n",
            tag("x")
        );
        assert_eq!(test_name_after(&source, 2), Some("mine".into()));
    }

    #[test]
    fn plotkeepers_real_tag_patterns_resolve() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let laws = system_map::check(&root).laws;
        let tests = law_tests(&root, &laws);
        let name = |law: &str| {
            tests
                .iter()
                .find(|t| t.law == law)
                .and_then(|t| t.test.clone())
        };
        assert_eq!(
            name("writer-has-no-tools").as_deref(),
            Some("the_claude_command_has_no_tools_a_schema_and_the_internal_mark")
        );
        assert_eq!(
            name("session-pieces-in-order").as_deref(),
            Some("jobs_run_in_the_order_they_were_asked_for_on_the_session_queue")
        );
    }

    #[test]
    fn law_tests_reads_each_tag_of_a_repository() {
        let mut map = base();
        map["laws"][0]["backing"] = "check".into();
        let (dir, root) = project(&map, &[]);
        write(
            dir.path(),
            "alpha/a.rs",
            &format!("{}#[test]\nfn holds() {{}}\n", tag("stay-true")),
        );
        studio_test_support::git(dir.path(), &["add", "-A"]);
        let root = std::path::Path::new(&root);
        let report = system_map::check(root);
        assert_eq!(
            law_tests(root, &report.laws),
            vec![LawTest {
                law: "stay-true".into(),
                tag: Tag {
                    file: "alpha/a.rs".into(),
                    line: 1
                },
                test: Some("holds".into()),
            }]
        );
    }
}
