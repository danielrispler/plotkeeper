use super::*;

#[test]
fn a_declaration_pair_is_left_out_of_the_report() {
    let declaration = Pair {
        declaration: true,
        ..Pair::new("lib.rs", "a.rs")
    };
    let counted = counted(&[declaration, Pair::new("a.rs", "b.rs")]);
    assert_eq!(
        counted.into_iter().collect::<Vec<_>>(),
        [("a.rs".to_string(), "b.rs".to_string(), None, None)]
    );
}

fn part(id: &str, files: &[&str]) -> PartFiles {
    PartFiles {
        id: id.into(),
        files: files.iter().map(ToString::to_string).collect(),
    }
}

/// The pairs from `from` to each of `files`, of the group `group`, with no group of `from`'s own.
fn import(from: &str, group: &str, files: &[&str]) -> Vec<Counted> {
    files
        .iter()
        .map(|to| {
            (
                from.to_string(),
                to.to_string(),
                Some(group.to_string()),
                None,
            )
        })
        .collect()
}

/// [`import`], from a file of the group `own`.
fn import_from(from: &str, own: &str, group: &str, files: &[&str]) -> Vec<Counted> {
    import(from, group, files)
        .into_iter()
        .map(|(from, to, group, _)| (from, to, group, Some(own.to_string())))
        .collect()
}

fn report(parts: &[PartFiles], pairs: &[Vec<Counted>]) -> Imports {
    count(
        parts,
        &pairs.concat().into_iter().collect(),
        &BTreeSet::new(),
        Vec::new(),
    )
}

fn raw(count: Count) -> usize {
    count.raw
}

#[test]
fn an_import_of_a_group_of_several_files_is_one_unit() {
    let parts = [
        part("p", &["a.go"]),
        part("q", &["s/x.go", "s/y.go", "s/z.go"]),
    ];
    let found = report(
        &parts,
        &[import("a.go", "s/", &["s/x.go", "s/y.go", "s/z.go"])],
    );
    assert_eq!((raw(found.total), raw(found.across)), (1, 1));
    assert_eq!(found.across.weighted, 1.0);
}

#[test]
fn an_import_of_a_split_group_is_one_unit_per_part() {
    let parts = [
        part("p", &["a.go", "s/x.go"]),
        part("q", &["s/y.go", "s/z.go"]),
    ];
    let found = report(
        &parts,
        &[import("a.go", "s/", &["s/x.go", "s/y.go", "s/z.go"])],
    );
    assert_eq!(
        (raw(found.total), raw(found.inside), raw(found.across)),
        (2, 1, 1)
    );
    assert_eq!(raw(found.parts[0].count), 1);
    let between: Vec<_> = found
        .between
        .iter()
        .map(|a| (a.from.as_str(), a.to.as_str(), a.count.raw))
        .collect();
    assert_eq!(between, [("p", "q", 1)]);
}

#[test]
fn a_slices_fan_in_counts_the_distinct_files_that_import_it() {
    let parts = [
        part("p", &["a.go", "b.go"]),
        part("q", &["s/x.go", "s/y.go"]),
    ];
    let found = report(
        &parts,
        &[
            import("a.go", "s/", &["s/x.go", "s/y.go"]),
            import("b.go", "s/", &["s/x.go", "s/y.go"]),
        ],
    );
    // Two units, each weighing 1 / 2, since two files import the slice.
    assert_eq!(
        found.across,
        Count {
            raw: 2,
            weighted: 1.0
        }
    );
}

fn hubs(found: &Imports) -> Vec<(&str, &str, usize, usize, usize)> {
    found
        .hubs
        .iter()
        .map(|h| {
            (
                h.file.as_str(),
                h.part.as_str(),
                h.degree,
                h.fan_in,
                h.fan_out,
            )
        })
        .collect()
}

#[test]
fn slices_are_the_nodes_of_degree_and_mean_degree_both_ways() {
    let importers = ["a1.go", "a2.go", "a3.go", "a4.go", "a5.go", "a6.go"];
    let parts = [
        part("p", &importers),
        part("q", &["s/x.go", "s/y.go", "t/w.go"]),
    ];
    let mut pairs: Vec<_> = importers
        .iter()
        .map(|a| import(a, "s/", &["s/x.go", "s/y.go"]))
        .collect();
    // A file of `s/` that imports counts as `s/`'s slice, not as itself.
    pairs.push(import_from("s/x.go", "s/", "t/", &["t/w.go"]));
    let found = report(&parts, &pairs);
    // Seven units over eight nodes: the six importers, `s/` and `t/`.
    assert_eq!(found.mean_degree, 1.75);
    assert_eq!(hubs(&found), [("s/", "q", 7, 6, 1)]);
}

#[test]
fn a_split_groups_hubs_name_their_part_in_the_text_only() {
    let importers: Vec<String> = (0..10).map(|i| format!("a{i}.go")).collect();
    let importers: Vec<&str> = importers.iter().map(String::as_str).collect();
    let parts = [
        part("p", &[importers.as_slice(), &["s/x.go"]].concat()),
        part("q", &["s/y.go", "u/v.go"]),
    ];
    let mut pairs: Vec<_> = importers
        .iter()
        .map(|a| import(a, "s/", &["s/x.go", "s/y.go"]))
        .collect();
    pairs.push(import("a0.go", "u/", &["u/v.go"]));
    let found = report(&parts, &pairs);
    assert_eq!(
        hubs(&found),
        [("s/", "p", 10, 10, 0), ("s/", "q", 10, 10, 0)]
    );
    let labels: Vec<_> = found.hubs.iter().map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["s/ (p)", "s/ (q)"]);
    assert_eq!(
        serde_json::to_value(&found.hubs[0]).unwrap(),
        serde_json::json!({"file": "s/", "part": "p", "degree": 10, "fan_in": 10, "fan_out": 0})
    );
}

#[test]
fn a_group_in_one_part_is_a_hub_named_by_the_group_alone() {
    let importers = ["a1.go", "a2.go", "a3.go", "a4.go", "a5.go", "a6.go"];
    let parts = [part("p", &importers), part("q", &["s/x.go", "s/y.go"])];
    let pairs: Vec<_> = importers
        .iter()
        .map(|a| import(a, "s/", &["s/x.go", "s/y.go"]))
        .collect();
    let found = report(&parts, &pairs);
    let labels: Vec<_> = found.hubs.iter().map(|h| h.label.as_str()).collect();
    assert_eq!(labels, ["s/"]);
}
