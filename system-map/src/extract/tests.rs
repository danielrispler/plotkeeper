//! The core's rules, tested once with fake extractors passed in as the registry.

use std::cell::RefCell;

use super::{Extractor, Output, Pair, Repo, Unsure, contract::Fixture, run};
use crate::{PartFiles, edges::check_links};

/// An extractor that reads every file but those it is told not to, and finds what it is given.
struct Fake {
    name: &'static str,
    pairs: Vec<Pair>,
    problems: Vec<&'static str>,
    unsure: Vec<Unsure>,
    /// The files it skips.
    tests: Vec<&'static str>,
    /// The files it does not read.
    unread: Vec<&'static str>,
    /// The files `skips` was asked about, in order.
    asked: RefCell<Vec<String>>,
}

impl Extractor for Fake {
    fn name(&self) -> &'static str {
        self.name
    }

    fn reads(&self, file: &str) -> bool {
        !self.unread.contains(&file)
    }

    fn skips(&self, _repo: &Repo, file: &str) -> bool {
        self.asked.borrow_mut().push(file.to_string());
        self.tests.contains(&file)
    }

    fn extract(&self, _repo: &Repo) -> Output {
        Output {
            problems: self.problems.iter().map(ToString::to_string).collect(),
            pairs: self.pairs.clone(),
            unsure: self.unsure.clone(),
            ..Output::default()
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
        unsure: Vec::new(),
        tests: Vec::new(),
        unread: Vec::new(),
        asked: RefCell::new(Vec::new()),
    }
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(ToString::to_string).collect()
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
        files: files.iter().map(ToString::to_string).collect(),
    }
}

/// `file` unsure of the name `n` for `reason`, with `candidates`.
fn unsure(file: &str, reason: &str, candidates: &[&str]) -> Unsure {
    Unsure {
        file: file.into(),
        name: "n".into(),
        reason: reason.into(),
        candidates: candidates.iter().map(ToString::to_string).collect(),
    }
}

fn import(from: &str, to: &str) -> crate::Link {
    serde_json::from_value(serde_json::json!({
        "id": format!("{from}-{to}"), "from": from, "to": to, "kind": "import"
    }))
    .unwrap()
}

#[test]
fn an_unsure_file_relaxes_only_its_parts_links_to_its_candidates_parts() {
    let files = names(&["a.x", "b.x", "c.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        unsure: vec![unsure("a.x", "declared twice", &["b.x"])],
        ..fake("one", &[])
    };
    let parts = [
        part("p", &["a.x"]),
        part("q", &["b.x"]),
        part("r", &["c.x"]),
    ];
    let links = [import("p", "q"), import("p", "r")];
    let found = check_links(&[&one], &names(&["one"]), &repo, &parts, &links);
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert!(
        found.problems[0].starts_with("import link \"p-r\" runs from \"p\" to \"r\""),
        "{:?}",
        found.problems
    );
    assert_eq!((found.proven, found.unproven), (0, 1));
}

#[test]
fn an_unsure_finding_with_no_candidates_in_raw_output_breaks_the_contract() {
    let files = names(&["a.x", "b.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        unsure: vec![unsure("a.x", "declared twice", &[])],
        ..fake("one", &[])
    };
    let out = run(&[&one], &names(&["one"]), &repo);
    assert_eq!(
        out.problems,
        [
            "extractor \"one\" found a.x unsure of \"n\" with no candidates; an Unsure finding names at least one \
             candidate file"
        ]
    );
    assert_eq!(out.unsure, []);
}

#[test]
fn an_unsure_finding_whose_candidates_the_core_drops_relaxes_nothing_and_breaks_no_contract() {
    let files = names(&["a.x", "b.x", "ignored.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |f| f == "ignored.x");
    let one = Fake {
        unsure: vec![unsure("a.x", "declared twice", &["ghost.x", "ignored.x"])],
        ..fake("one", &[])
    };
    let parts = [part("p", &["a.x"]), part("q", &["b.x"])];
    let found = check_links(
        &[&one],
        &names(&["one"]),
        &repo,
        &parts,
        &[import("p", "q")],
    );
    assert_eq!(found.problems.len(), 1, "{:?}", found.problems);
    assert!(found.problems[0].starts_with("import link \"p-q\""));
    assert_eq!(found.unsure, []);
}

#[test]
fn an_unsure_finding_in_an_ignored_or_skipped_file_is_dropped() {
    let files = names(&["a.x", "a.test.x", "ignored.x", "b.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |f| f == "ignored.x");
    let one = Fake {
        tests: vec!["a.test.x"],
        unsure: vec![
            unsure("a.test.x", "declared twice", &["b.x"]),
            unsure("ignored.x", "declared twice", &["b.x"]),
            unsure("a.x", "declared twice", &["b.x"]),
        ],
        ..fake("one", &[])
    };
    let out = run(&[&one], &names(&["one"]), &repo);
    assert_eq!(out.unsure, [unsure("a.x", "declared twice", &["b.x"])]);
}

#[test]
fn two_unsure_findings_of_one_file_name_and_reason_merge_with_the_union_of_their_candidates() {
    let files = names(&["a.x", "b.x", "c.x", "d.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        unsure: vec![
            unsure("a.x", "two modules provide it", &["d.x"]),
            unsure("a.x", "declared twice", &["c.x", "b.x"]),
        ],
        ..fake("one", &[])
    };
    let two = Fake {
        unsure: vec![unsure("a.x", "declared twice", &["b.x", "d.x"])],
        ..fake("two", &[])
    };
    let out = run(&[&one, &two], &names(&["one", "two"]), &repo);
    assert_eq!(
        out.unsure,
        [
            unsure("a.x", "declared twice", &["b.x", "c.x", "d.x"]),
            unsure("a.x", "two modules provide it", &["d.x"]),
        ]
    );
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
fn run_asks_skips_only_of_files_it_reads_once_per_from_file() {
    let files = names(&["a.x", "b.x", "c.x", "d.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let one = Fake {
        unread: vec!["d.x"],
        ..fake(
            "one",
            &[
                ("a.x", "b.x"),
                ("a.x", "c.x"),
                ("b.x", "c.x"),
                ("a.x", "d.x"),
                ("d.x", "a.x"),
                ("d.x", "b.x"),
            ],
        )
    };
    let out = run(&[&one], &names(&["one"]), &repo);
    assert_eq!(ends(&out).len(), 6);
    let mut asked = one.asked.take();
    asked.sort();
    assert_eq!(asked, ["a.x", "b.x"]);
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
        pairs: vec![
            at("b.x", "c.x", &[], false),
            at("a.x", "b.x", &[5, 7], true),
        ],
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
            at("a.x", "b.x", &[3, 5, 7], false),
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

#[test]
fn a_merged_pair_keeps_the_lowest_group_with_none_lowest() {
    let files = names(&["a.x", "b.x", "c.x"]);
    let repo = Repo::new(std::path::Path::new("."), &files, |_| false);
    let grouped = |from: &str, to: &str, group: Option<&str>| Pair {
        group: group.map(ToString::to_string),
        ..Pair::new(from, to)
    };
    let one = Fake {
        pairs: vec![
            grouped("a.x", "b.x", Some("q/")),
            grouped("a.x", "c.x", Some("q/")),
        ],
        ..fake("one", &[])
    };
    let two = Fake {
        pairs: vec![
            grouped("a.x", "b.x", Some("p/")),
            grouped("a.x", "c.x", None),
        ],
        ..fake("two", &[])
    };
    let out = run(&[&one, &two], &names(&["one", "two"]), &repo);
    assert_eq!(
        out.pairs,
        [
            grouped("a.x", "b.x", Some("p/")),
            grouped("a.x", "c.x", None)
        ]
    );
}
