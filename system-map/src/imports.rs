//! `imports`: how well the map's parts follow the code's imports, counted from the deduplicated
//! file pairs the map's extractors find between owned files, as units: one per importing file,
//! group and target part, where a pair with no group is its own unit.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

use serde::Serialize;

use crate::{PartFiles, extract::Pair};

/// What `imports` counted. Weighted counts give each unit 1 / the fan-in of its target, rounded
/// to two places; shares are percentages of the total, rounded to one place.
#[derive(Debug, Default, Serialize)]
pub struct Imports {
    /// Every counted unit: both files owned, a crate root's `mod` lines left out.
    pub total: Count,
    /// The units whose importing file and target are in one part.
    pub inside: Count,
    /// The units whose importing file and target are in two different parts.
    pub across: Count,
    /// `across` as a share of `total`, raw and weighted.
    pub across_share: Share,
    /// The mean degree (units in plus out) over the nodes with at least one counted unit.
    pub mean_degree: f64,
    /// Nodes whose degree is at least three times `mean_degree`, highest first.
    pub hubs: Vec<Hub>,
    /// Each part with the imports inside it, in the map's order.
    pub parts: Vec<Inside>,
    /// Each ordered pair of parts with the imports from the first to the second.
    pub between: Vec<Across>,
    /// Pairs of parts that import each other both ways, each pair once.
    pub both_ways: Vec<[String; 2]>,
    /// Extractors that are unknown or could not run; the counts leave out what they would find.
    pub problems: Vec<String>,
    /// The Unsure files the parts own, each with its owned candidates. Left out of the JSON when empty.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unsure: Vec<crate::Unsure>,
}

/// A number of import units, raw and weighted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Count {
    /// Each unit counts 1.
    pub raw: usize,
    /// Each unit counts 1 / the fan-in of its target.
    pub weighted: f64,
}

/// A percentage of `total`, raw and weighted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Share {
    /// Of the raw counts.
    pub raw: f64,
    /// Of the weighted counts.
    pub weighted: f64,
}

/// A node, a file or a slice (a group's files in one part), with many more imports in and out
/// than the mean.
#[derive(Debug, Serialize)]
pub struct Hub {
    /// The file's path, or the slice's group.
    pub file: String,
    /// The part that owns it.
    pub part: String,
    /// Its units in plus out.
    pub degree: usize,
    /// The distinct counted files that import it.
    pub fan_in: usize,
    /// The units it imports.
    pub fan_out: usize,
    /// How the text output names it: a file with its part, a slice by its group, with its part only
    /// when the group's files sit in more than one part.
    #[serde(skip)]
    pub label: String,
}

/// One part with the imports inside it.
#[derive(Debug, Serialize)]
pub struct Inside {
    /// The part's id.
    pub part: String,
    /// Its units inside it.
    #[serde(flatten)]
    pub count: Count,
}

/// The imports from one part to another.
#[derive(Debug, Serialize)]
pub struct Across {
    /// The importing part's id.
    pub from: String,
    /// The imported part's id.
    pub to: String,
    /// The units from `from`'s files to `to`'s.
    #[serde(flatten)]
    pub count: Count,
}

/// Counts the imports of the map in `root`. Fails only with the problems of a map that does not
/// load, as `check` would report them, or of git that cannot list the files.
pub fn imports(root: &Path) -> Result<Imports, Vec<String>> {
    let map = crate::load_checked(root)?;
    let files = crate::tracked_files(root).map_err(|problem| vec![problem])?;
    // Unowned files are `check`'s to report; here they only fall out of the counts.
    let (parts, _, _) = crate::ownership(&map, &files);
    let names = map.extractors.as_deref().unwrap_or_default();
    let repo = crate::repo(root, &map, &files);
    let found = crate::extract::run(crate::extract::registry::EXTRACTORS, names, &repo);
    Ok(Imports {
        unsure: crate::edges::owned_unsure(found.unsure, &parts),
        ..count(
            &parts,
            &counted(&found.pairs),
            &found.members,
            found.problems,
        )
    })
}

/// The pairs the report counts, each with its groups: a crate root's `mod` lines declare the module tree, so their
/// pairs are left out.
fn counted(pairs: &[Pair]) -> BTreeSet<Counted> {
    pairs
        .iter()
        .filter(|p| !p.declaration)
        .map(|p| {
            (
                p.from.clone(),
                p.to.clone(),
                p.group.clone(),
                p.from_group.clone(),
            )
        })
        .collect()
}

/// A counted pair: its two files, its group and the group of its importer.
type Counted = (String, String, Option<String>, Option<String>);

/// Counts `pairs` over `parts`; `members` are the files of each group, whose parts decide how a slice is named.
fn count(
    parts: &[PartFiles],
    pairs: &BTreeSet<Counted>,
    members: &BTreeSet<(String, String)>,
    problems: Vec<String>,
) -> Imports {
    let owner: HashMap<&str, &str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(|f| (f.as_str(), part.id.as_str())))
        .collect();
    let (units, group_parts) = units(pairs, members, &owner);
    let mut fan_in: HashMap<Node, usize> = HashMap::new();
    let mut fan_out: HashMap<Node, usize> = HashMap::new();
    for unit in &units {
        *fan_out.entry(unit.source).or_default() += 1;
        *fan_in.entry(unit.target).or_default() += 1;
    }
    let mut inside: HashMap<&str, (usize, f64)> = HashMap::new();
    let mut between: BTreeMap<(&str, &str), (usize, f64)> = BTreeMap::new();
    let (mut total, mut across) = ((0, 0.0), (0, 0.0));
    for unit in &units {
        let weight = 1.0 / fan_in[&unit.target] as f64;
        let (a, b) = (owner[unit.from], unit.target.part(&owner));
        let cell = if a == b {
            inside.entry(a).or_default()
        } else {
            add(&mut across, weight);
            between.entry((a, b)).or_default()
        };
        add(cell, weight);
        add(&mut total, weight);
    }
    let degree = |node: &Node| fan_in.get(node).unwrap_or(&0) + fan_out.get(node).unwrap_or(&0);
    let nodes: BTreeSet<Node> = units
        .iter()
        .flat_map(|unit| [unit.source, unit.target])
        .collect();
    let mean = if nodes.is_empty() {
        0.0
    } else {
        2.0 * units.len() as f64 / nodes.len() as f64
    };
    let mut hubs: Vec<Hub> = nodes
        .iter()
        .filter(|node| degree(node) as f64 >= 3.0 * mean)
        .map(|node| Hub {
            file: node.name().to_string(),
            part: node.part(&owner).to_string(),
            degree: degree(node),
            fan_in: *fan_in.get(node).unwrap_or(&0),
            fan_out: *fan_out.get(node).unwrap_or(&0),
            label: node.label(&owner, &group_parts),
        })
        .collect();
    hubs.sort_by(|a, b| {
        b.degree
            .cmp(&a.degree)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.part.cmp(&b.part))
    });
    let both_ways = between
        .keys()
        .filter(|(a, b)| a < b && between.contains_key(&(*b, *a)))
        .map(|(a, b)| [a.to_string(), b.to_string()])
        .collect();
    let mut between: Vec<Across> = between
        .into_iter()
        .map(|((from, to), cell)| Across {
            from: from.to_string(),
            to: to.to_string(),
            count: rounded(cell),
        })
        .collect();
    between.sort_by_key(|a| std::cmp::Reverse(a.count.raw));
    let share = |part: f64, whole: f64| {
        if whole == 0.0 {
            0.0
        } else {
            (part / whole * 1000.0).round() / 10.0
        }
    };
    Imports {
        total: rounded(total),
        inside: rounded((total.0 - across.0, total.1 - across.1)),
        across: rounded(across),
        across_share: Share {
            raw: share(across.0 as f64, total.0 as f64),
            weighted: share(across.1, total.1),
        },
        mean_degree: round2(mean),
        hubs,
        parts: parts
            .iter()
            .map(|part| Inside {
                part: part.id.clone(),
                count: rounded(inside.get(part.id.as_str()).copied().unwrap_or_default()),
            })
            .collect(),
        between,
        both_ways,
        problems,
        unsure: Vec::new(),
    }
}

/// One unit per (importing file, target node): a grouped pair's target is its group's slice in the part that owns
/// `to`, so the pairs of one import of a group count once per part; an ungrouped pair is its own unit. A grouped pair's
/// importer counts as the slice of its own group when the pair names one, else as the file. Also gives the parts each
/// group's files sit in, its `members` and the files of its pairs alike.
fn units<'a>(
    pairs: &'a BTreeSet<Counted>,
    members: &'a BTreeSet<(String, String)>,
    owner: &HashMap<&'a str, &'a str>,
) -> (Vec<Unit<'a>>, GroupParts<'a>) {
    let mut group_parts = GroupParts::new();
    for (file, group) in members {
        if let Some(part) = owner.get(file.as_str()) {
            group_parts.entry(group.as_str()).or_default().insert(*part);
        }
    }
    let mut units: BTreeSet<(&str, Node, Node)> = BTreeSet::new();
    for (from, to, group, from_group) in pairs {
        let (from, to) = (from.as_str(), to.as_str());
        if from == to || !owner.contains_key(from) || !owner.contains_key(to) {
            continue;
        }
        let (target, source) = match group.as_deref() {
            Some(group) => {
                group_parts.entry(group).or_default().insert(owner[to]);
                let source = match from_group.as_deref() {
                    Some(own) => {
                        group_parts.entry(own).or_default().insert(owner[from]);
                        Node::Slice(own, owner[from])
                    }
                    None => Node::File(from),
                };
                (Node::Slice(group, owner[to]), source)
            }
            None => (Node::File(to), Node::File(from)),
        };
        units.insert((from, source, target));
    }
    let units = units
        .into_iter()
        .map(|(from, source, target)| Unit {
            source,
            from,
            target,
        })
        .collect();
    (units, group_parts)
}

/// The parts each group's files sit in.
type GroupParts<'a> = HashMap<&'a str, BTreeSet<&'a str>>;

/// One counted import: from a file, counted as the node `source`, to the node `target`.
struct Unit<'a> {
    source: Node<'a>,
    from: &'a str,
    target: Node<'a>,
}

/// What the report counts degree over: a file, or a slice, a group's files in one part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Node<'a> {
    File(&'a str),
    /// A group and a part.
    Slice(&'a str, &'a str),
}

impl<'a> Node<'a> {
    /// The part that owns it.
    fn part(self, owner: &HashMap<&str, &'a str>) -> &'a str {
        match self {
            Node::File(file) => owner[file],
            Node::Slice(_, part) => part,
        }
    }

    /// The file's path, or the slice's group.
    fn name(self) -> &'a str {
        match self {
            Node::File(name) | Node::Slice(name, _) => name,
        }
    }

    /// How the text output names it: a file with its part, a slice by its group, with its part only when the group's
    /// files sit in more than one part.
    fn label(self, owner: &HashMap<&str, &'a str>, group_parts: &GroupParts) -> String {
        match self {
            Node::Slice(group, _) if group_parts[group].len() == 1 => group.to_string(),
            _ => format!("{} ({})", self.name(), self.part(owner)),
        }
    }
}

fn add(cell: &mut (usize, f64), weight: f64) {
    cell.0 += 1;
    cell.1 += weight;
}

fn rounded((raw, weighted): (usize, f64)) -> Count {
    Count {
        raw,
        weighted: round2(weighted),
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
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
}
