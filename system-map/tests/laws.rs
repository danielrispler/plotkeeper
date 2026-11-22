//! Law tags: a law's backing is held to the tests that name it.

use std::path::Path;

use serde_json::{Value, json};

use crate::support::{FILES, base, has, problems, project, write};
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
