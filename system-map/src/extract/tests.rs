//! The core's rules, tested once with fake extractors passed in as the registry.

use super::{Extractor, Output, Pair, Repo, contract::Fixture, run};
use crate::{PartFiles, edges::check_links};

/// An extractor that reads every file and finds what it is given.
struct Fake {
    name: &'static str,
    pairs: Vec<Pair>,
    problems: Vec<&'static str>,
    /// The files it names as test files.
    tests: Vec<&'static str>,
}

impl Extractor for Fake {
    fn name(&self) -> &'static str {
        self.name
    }

    fn reads(&self, _file: &str) -> bool {
        true
    }

    fn is_test(&self, _repo: &Repo, file: &str) -> bool {
        self.tests.contains(&file)
    }

    fn extract(&self, _repo: &Repo) -> Output {
        Output {
            problems: self.problems.iter().map(|p| p.to_string()).collect(),
            pairs: self.pairs.clone(),
        }
    }

    fn fixture(&self) -> Fixture {
        unreachable!("a fake is never registered")
    }
}

fn fake(name: &'static str, pairs: &[(&str, &str)]) -> Fake {
    Fake {
        name,
        pairs: pairs.iter().map(|(from, to)| Pair::new(from, to)).collect(),
        problems: Vec::new(),
        tests: Vec::new(),
    }
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}

fn ends(out: &Output) -> Vec<(&str, &str)> {
    out.pairs
        .iter()
        .map(|p| (p.from.as_str(), p.to.as_str()))
        .collect()
}

fn part(id: &str, files: &[&str]) -> PartFiles {
    PartFiles {
        id: id.into(),
        files: files.iter().map(|f| f.to_string()).collect(),
    }
}

#[test]
fn a_pair_to_an_untracked_file_is_dropped() {
    // On disk is not enough: `ghost.x` is written but not tracked.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("ghost.x"), "").unwrap();
    let files = names(&["a.x", "b.x"]);
    let repo = Repo::new(dir.path(), &files, |_| false);
    let one = fake(
        "one",
        &[("a.x", "b.x"), ("a.x", "ghost.x"), ("ghost.x", "b.x")],
    );
    let out = run(&[&one], &names(&["one"]), &repo);
    assert_eq!(ends(&out), [("a.x", "b.x")]);
}

#[test]
fn a_pair_from_a_file_to_itself_is_dropped() {
    let files = names(&["a.x", "b.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = fake("one", &[("a.x", "a.x"), ("a.x", "b.x")]);
    let out = run(&[&one], &names(&["one"]), &repo);
    assert_eq!(ends(&out), [("a.x", "b.x")]);
}

#[test]
fn a_pair_two_extractors_find_is_kept_once_with_the_lines_of_both() {
    let files = names(&["a.x", "b.x", "c.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let at = |from: &str, to: &str, lines: &[u32], declaration: bool| Pair {
        lines: lines.iter().copied().collect(),
        declaration,
        ..Pair::new(from, to)
    };
    let one = Fake {
        pairs: vec![at("b.x", "c.x", &[], false), at("a.x", "b.x", &[7], true)],
        ..fake("one", &[])
    };
    let two = Fake {
        pairs: vec![
            at("a.x", "b.x", &[3, 7], false),
            at("a.x", "b.x", &[], true),
        ],
        ..fake("two", &[])
    };
    let out = run(&[&one, &two], &names(&["one", "two"]), &repo);
    assert_eq!(
        out.pairs,
        [
            at("a.x", "b.x", &[3, 7], false),
            at("b.x", "c.x", &[], false)
        ]
    );
}

#[test]
fn problems_keep_the_maps_extractor_order_and_an_unknown_name_lists_the_registry() {
    let repo_files = names(&[]);
    let repo = Repo::new(std::path::Path::new("."), &repo_files, |_| false);
    let first = Fake {
        problems: vec!["first cannot run"],
        ..fake("first", &[])
    };
    let second = Fake {
        problems: vec!["second cannot run"],
        ..fake("second", &[])
    };
    let out = run(
        &[&first, &second],
        &names(&["second", "nope", "first"]),
        &repo,
    );
    assert_eq!(
        out.problems,
        [
            "second cannot run",
            "unknown extractor \"nope\" in extractors; use one of first, second",
            "first cannot run",
        ]
    );
}

#[test]
fn a_pair_through_an_ignored_file_gives_no_part_edge() {
    let files = names(&["a.x", "b.x", "ignored.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |f| f == "ignored.x");
    let one = fake("one", &[("a.x", "ignored.x"), ("ignored.x", "b.x")]);
    let parts = [part("p", &["a.x"]), part("q", &["b.x"])];
    let found = check_links(&[&one], &names(&["one"]), &repo, &parts, &[]);
    assert_eq!(found.problems, Vec::<String>::new());
}

#[test]
fn a_declaration_pair_counts_in_the_check() {
    let files = names(&["a.x", "b.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        pairs: vec![Pair {
            declaration: true,
            ..Pair::new("a.x", "b.x")
        }],
        ..fake("one", &[])
    };
    let parts = [part("p", &["a.x"]), part("q", &["b.x"])];
    let found = check_links(&[&one], &names(&["one"]), &repo, &parts, &[]);
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert!(found.problems[0].starts_with("part \"p\" depends on part \"q\""));
}

#[test]
fn a_pair_that_starts_in_a_test_file_is_dropped() {
    let files = names(&["a.x", "b.x", "a.test.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        tests: vec!["a.test.x"],
        ..fake(
            "one",
            &[("a.test.x", "b.x"), ("a.x", "a.test.x"), ("a.x", "b.x")],
        )
    };
    let out = run(&[&one], &names(&["one"]), &repo);
    assert_eq!(ends(&out), [("a.x", "a.test.x"), ("a.x", "b.x")]);
}

#[test]
fn a_part_made_only_of_test_files_is_not_read() {
    let files = names(&["a.x", "a.test.x", "b.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        tests: vec!["a.test.x"],
        ..fake("one", &[])
    };
    let parts = [part("tests", &["a.test.x"]), part("q", &["a.x", "b.x"])];
    let read = super::read_parts(&[&one], &names(&["one"]), &repo, &parts);
    assert_eq!(read, ["q"].into_iter().collect());
}
