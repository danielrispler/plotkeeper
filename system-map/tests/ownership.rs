//! Which part owns which tracked file, and what is left over.

use std::path::Path;

use serde_json::{Value, json};

use crate::common::{base, has, problems, project, write};

fn owned(map: &Value, files: &[&str]) -> Vec<(String, Vec<String>)> {
    let (_dir, root) = project(map, files);
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    report.parts.into_iter().map(|p| (p.id, p.files)).collect()
}

fn paths(map: &mut Value, alpha: Value, beta: Value) {
    map["parts"][0]["paths"] = alpha;
    map["parts"][1]["paths"] = beta;
}

fn files(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_folder_pattern_owns_everything_under_it() {
    let got = owned(
        &base(),
        &["alpha/a.rs", "alpha/deep/x.rs", "beta/b.rs", "docs/d"],
    );
    assert_eq!(
        got[0],
        ("alpha".into(), files(&["alpha/a.rs", "alpha/deep/x.rs"]))
    );
    assert_eq!(got[1], ("beta".into(), files(&["beta/b.rs"])));
}

#[test]
fn a_star_stays_within_one_segment() {
    let mut map = base();
    paths(&mut map, json!(["alpha/*.rs"]), json!(["beta/"]));
    map["ignored"] = json!(["alpha/sub/", "docs/", "system-map.json"]);
    let got = owned(
        &map,
        &["alpha/a.rs", "alpha/sub/x.rs", "beta/b.rs", "docs/d"],
    );
    assert_eq!(got[0].1, files(&["alpha/a.rs"]));
}

#[test]
fn a_double_star_crosses_segments() {
    let mut map = base();
    paths(&mut map, json!(["alpha/**/*.rs"]), json!(["beta/"]));
    map["ignored"] = json!(["alpha/notes.md", "docs/", "system-map.json"]);
    let got = owned(
        &map,
        &[
            "alpha/a.rs",
            "alpha/sub/deep/x.rs",
            "alpha/notes.md",
            "beta/b",
            "docs/d",
        ],
    );
    assert_eq!(got[0].1, files(&["alpha/a.rs", "alpha/sub/deep/x.rs"]));
}

#[test]
fn an_exact_name_owns_one_file() {
    let mut map = base();
    paths(&mut map, json!(["alpha/a.rs"]), json!(["beta/"]));
    map["ignored"] = json!(["alpha/other.rs", "docs/", "system-map.json"]);
    let got = owned(&map, &["alpha/a.rs", "alpha/other.rs", "beta/b", "docs/d"]);
    assert_eq!(got[0].1, files(&["alpha/a.rs"]));
}

#[test]
fn the_last_matching_part_wins() {
    let mut map = base();
    paths(
        &mut map,
        json!(["shared/"]),
        json!(["shared/special.rs", "beta/"]),
    );
    let got = owned(
        &map,
        &["shared/a.rs", "shared/special.rs", "beta/b", "docs/d"],
    );
    assert_eq!(got[0].1, files(&["shared/a.rs"]));
    assert_eq!(got[1].1, files(&["beta/b", "shared/special.rs"]));
}

#[test]
fn an_ignored_match_wins_over_every_part() {
    let mut map = base();
    map["ignored"] = json!(["docs/", "alpha/gen/", "system-map.json"]);
    let got = owned(&map, &["alpha/a.rs", "alpha/gen/g.rs", "beta/b", "docs/d"]);
    assert_eq!(got[0].1, files(&["alpha/a.rs"]));
}

#[test]
fn an_untracked_file_is_never_counted() {
    let (dir, root) = project(&base(), &["alpha/a.rs", "beta/b", "docs/d"]);
    write(dir.path(), "stray/new.rs", "x");
    write(dir.path(), "alpha/untracked.rs", "x");
    let report = system_map::check(Path::new(&root));
    assert_eq!(report.problems, Vec::<String>::new());
    assert_eq!(report.parts[0].files, files(&["alpha/a.rs"]));
}

#[test]
fn a_tracked_file_nobody_owns_fails_and_names_it() {
    let (_dir, root) = project(&base(), &["alpha/a.rs", "beta/b", "docs/d", "stray/new.rs"]);
    assert!(has(&problems(&root), &["stray/new.rs", "ignored"]));
}

#[test]
fn a_part_pattern_matching_nothing_fails_and_names_it() {
    let mut map = base();
    map["parts"][1]["paths"] = json!(["beta/", "gone/"]);
    let (_dir, root) = project(&map, &["alpha/a.rs", "beta/b", "docs/d"]);
    assert!(has(&problems(&root), &["gone/", "no tracked file"]));
}

#[test]
fn an_ignored_pattern_matching_nothing_fails_and_names_it() {
    let mut map = base();
    map["ignored"] = json!(["docs/", "old/*.tmp"]);
    let (_dir, root) = project(&map, &["alpha/a.rs", "beta/b", "docs/d"]);
    assert!(has(&problems(&root), &["old/*.tmp", "no tracked file"]));
}

#[test]
fn a_pattern_beaten_by_a_later_one_still_counts_as_matching() {
    let mut map = base();
    paths(&mut map, json!(["alpha/", "beta/b"]), json!(["beta/"]));
    let got = owned(&map, &["alpha/a.rs", "beta/b", "docs/d"]);
    assert_eq!(got[1].1, files(&["beta/b"]));
}
