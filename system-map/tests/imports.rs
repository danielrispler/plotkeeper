//! `system-map imports`: hubs listed apart, imports inside and across parts, raw and weighted.

use std::{
    path::Path,
    process::{Command, Output},
};

use serde_json::{Value, json};

use crate::common::{base, project_text, write};

/// The map `base` makes, naming `extractors`, with alpha and beta owning `alpha` and `beta` paths.
fn map(extractors: &[&str], alpha: &[&str], beta: &[&str]) -> Value {
    let mut map = base();
    map["extractors"] = json!(extractors);
    map["parts"][0]["paths"] = json!(alpha);
    map["parts"][1]["paths"] = json!(beta);
    map
}

/// A repository holding `map` and `files` with their texts.
fn repo(map: &Value, files: &[(&str, &str)]) -> (tempfile::TempDir, String) {
    let (dir, root) = project_text(&map.to_string(), &[]);
    for (file, text) in files {
        write(dir.path(), file, text);
    }
    studio_test_support::git(dir.path(), &["add", "-A"]);
    studio_test_support::git(dir.path(), &["commit", "-q", "-m", "Code"]);
    (dir, root)
}

/// `alpha/u.ts` is a hub: three alpha files and five beta files import it. `alpha/a1.ts` also
/// imports `beta/b1.ts`, so the two parts import each other. An ignored file's import is not
/// counted.
fn hub_repo() -> (tempfile::TempDir, String) {
    let u = "import { x } from '../alpha/u';\n";
    repo(
        &map(&["typescript-imports"], &["alpha/"], &["beta/"]),
        &[
            ("alpha/u.ts", "export const x = 1;\n"),
            (
                "alpha/a1.ts",
                "import { x } from './u';\nimport { y } from '../beta/b1';\n",
            ),
            ("alpha/a2.ts", "import { x } from './u';\n"),
            ("alpha/a3.ts", "import { x } from './u';\n"),
            (
                "beta/b1.ts",
                "import { x } from '../alpha/u';\nexport const y = 2;\n",
            ),
            ("beta/b2.ts", u),
            ("beta/b3.ts", u),
            ("beta/b4.ts", u),
            ("beta/b5.ts", u),
            ("docs/d.ts", u),
        ],
    )
}

fn counted(root: &str) -> system_map::Imports {
    system_map::imports(Path::new(root)).unwrap()
}

fn count(raw: usize, weighted: f64) -> system_map::Count {
    system_map::Count { raw, weighted }
}

#[test]
fn a_hub_is_listed_apart_and_edges_into_it_weigh_less_than_they_count() {
    let (_dir, root) = hub_repo();
    let found = counted(&root);
    let hubs: Vec<_> = found
        .hubs
        .iter()
        .map(|h| (h.file.as_str(), h.degree))
        .collect();
    assert_eq!(hubs, [("alpha/u.ts", 8)]);
    assert_eq!(found.mean_degree, 2.0);
    let beta_alpha = found
        .between
        .iter()
        .find(|a| a.from == "beta" && a.to == "alpha")
        .unwrap();
    // Five imports into a file eight import: 5 / 8.
    assert_eq!(beta_alpha.count, count(5, 0.63));
}

#[test]
fn inside_and_across_add_up_to_the_pairs_between_owned_files() {
    let (_dir, root) = hub_repo();
    let found = counted(&root);
    assert_eq!(found.total, count(9, 2.0));
    assert_eq!(found.inside, count(3, 0.38));
    assert_eq!(found.across, count(6, 1.63));
    let parts: Vec<_> = found
        .parts
        .iter()
        .map(|p| (p.part.as_str(), p.count))
        .collect();
    assert_eq!(parts, [("alpha", count(3, 0.38)), ("beta", count(0, 0.0))]);
    assert_eq!(
        found.across_share,
        system_map::Share {
            raw: 66.7,
            weighted: 81.3
        }
    );
}

#[test]
fn parts_that_import_each_other_are_named() {
    let (_dir, root) = hub_repo();
    assert_eq!(counted(&root).both_ways, [["alpha", "beta"]]);
}

#[test]
fn a_crate_roots_mod_lines_are_left_out_but_its_paths_count() {
    let (_dir, root) = repo(
        &map(
            &["rust-modules"],
            &["k/src/lib.rs", "k/src/a.rs"],
            &["k/src/b.rs"],
        ),
        &[
            ("k/src/lib.rs", "mod a;\nmod b;\nfn f() { b::X; }\n"),
            ("k/src/a.rs", ""),
            ("k/src/b.rs", "pub struct X;\n"),
        ],
    );
    let found = counted(&root);
    assert_eq!(found.total, count(1, 1.0));
    assert_eq!(found.inside, count(0, 0.0));
    let across: Vec<_> = found
        .between
        .iter()
        .map(|a| (a.from.as_str(), a.to.as_str()))
        .collect();
    assert_eq!(across, [("alpha", "beta")]);
}

fn run(root: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_system-map"))
        .arg("imports")
        .args(args)
        .args(["--root", root])
        .output()
        .unwrap()
}

#[test]
fn the_command_prints_the_counts_and_exits_zero() {
    let (_dir, root) = hub_repo();
    let out = run(&root, &[]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "9 imports between owned files: 3 inside parts, 6 across (66.7% raw, 81.3% weighted)\n\
         hubs (degree at least 3 x the mean of 2.00):\n\
         \x20 alpha/u.ts (alpha): degree 8, 8 in, 0 out\n\
         inside:\n\
         \x20 alpha: 3 (weighted 0.38)\n\
         \x20 beta: 0 (weighted 0.00)\n\
         across:\n\
         \x20 beta -> alpha: 5 (weighted 0.63)\n\
         \x20 alpha -> beta: 1 (weighted 1.00)\n\
         both ways:\n\
         \x20 alpha <-> beta\n"
    );
}

#[test]
fn json_prints_the_same_numbers() {
    let (_dir, root) = hub_repo();
    let out = run(&root, &["--json"]);
    assert!(out.status.success(), "{out:?}");
    let found: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(found["total"], json!({ "raw": 9, "weighted": 2.0 }));
    assert_eq!(found["across"], json!({ "raw": 6, "weighted": 1.63 }));
    assert_eq!(
        found["across_share"],
        json!({ "raw": 66.7, "weighted": 81.3 })
    );
    assert_eq!(found["hubs"][0]["file"], "alpha/u.ts");
    assert_eq!(found["hubs"][0]["degree"], 8);
    assert_eq!(found["mean_degree"], 2.0);
    assert_eq!(
        found["between"][0],
        json!({ "from": "beta", "to": "alpha", "raw": 5, "weighted": 0.63 })
    );
    assert_eq!(found["both_ways"], json!([["alpha", "beta"]]));
}

#[test]
fn json_leaves_out_unsure_when_no_file_is_unsure() {
    let (_dir, root) = hub_repo();
    let found: Value = serde_json::from_slice(&run(&root, &["--json"]).stdout).unwrap();
    assert!(found.get("problems").is_some(), "{found}");
    assert_eq!(found.get("unsure"), None, "{found}");
}

#[test]
fn no_imports_still_exit_zero() {
    let (_dir, root) = repo(&map(&[], &["alpha/"], &["beta/"]), &[("alpha/a.ts", "")]);
    let out = run(&root, &[]);
    assert!(out.status.success(), "{out:?}");
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .starts_with("0 imports between owned files"),
    );
}

#[test]
fn a_map_that_does_not_load_exits_two_with_its_problem() {
    let (_dir, root) = project_text("{ not json", &[]);
    let out = run(&root, &[]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("is not valid JSON")
    );
}

#[test]
fn a_map_over_a_cap_still_counts() {
    let mut over = map(&[], &["alpha/"], &["beta/"]);
    over["parts"][0]["purpose"] = json!("a".repeat(241));
    let (_dir, root) = repo(&over, &[("alpha/a.ts", "")]);
    let out = run(&root, &[]);
    assert!(out.status.success(), "{out:?}");
}
