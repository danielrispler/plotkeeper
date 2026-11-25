//! `imports`: how well the map's parts follow the code's imports, counted from the deduplicated
//! file pairs the map's extractors find between owned files.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
};

use serde::Serialize;

use crate::{PartFiles, extract::Pair};

/// What `imports` counted. Weighted counts give each pair 1 / the fan-in of its target, rounded
/// to two places; shares are percentages of the total, rounded to one place.
#[derive(Debug, Default, Serialize)]
pub struct Imports {
    /// Every counted pair: both files owned, a crate root's `mod` lines left out.
    pub total: Count,
    pub inside: Count,
    pub across: Count,
    /// `across` as a share of `total`, raw and weighted.
    pub across_share: Share,
    /// The mean degree (imports in plus out) over the files with at least one counted pair.
    pub mean_degree: f64,
    /// Files whose degree is at least three times `mean_degree`, highest first.
    pub hubs: Vec<Hub>,
    /// Each part with the imports inside it, in the map's order.
    pub parts: Vec<Inside>,
    /// Each ordered pair of parts with the imports from the first to the second.
    pub between: Vec<Across>,
    /// Pairs of parts that import each other both ways, each pair once.
    pub both_ways: Vec<[String; 2]>,
    /// Extractors that are unknown or could not run; the counts leave out what they would find.
    pub problems: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Count {
    pub raw: usize,
    pub weighted: f64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize)]
pub struct Share {
    pub raw: f64,
    pub weighted: f64,
}

#[derive(Debug, Serialize)]
pub struct Hub {
    pub file: String,
    pub part: String,
    pub degree: usize,
    pub fan_in: usize,
    pub fan_out: usize,
}

#[derive(Debug, Serialize)]
pub struct Inside {
    pub part: String,
    #[serde(flatten)]
    pub count: Count,
}

#[derive(Debug, Serialize)]
pub struct Across {
    pub from: String,
    pub to: String,
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
    Ok(count(&parts, &counted(&found.pairs), found.problems))
}

/// The pairs the report counts: a crate root's `mod` lines declare the module tree, so their pairs are left out.
fn counted(pairs: &[Pair]) -> BTreeSet<(String, String)> {
    pairs
        .iter()
        .filter(|p| !p.declaration)
        .map(|p| (p.from.clone(), p.to.clone()))
        .collect()
}

fn count(
    parts: &[PartFiles],
    pairs: &BTreeSet<(String, String)>,
    problems: Vec<String>,
) -> Imports {
    let owner: HashMap<&str, &str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(|f| (f.as_str(), part.id.as_str())))
        .collect();
    let owned: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(from, to)| (from.as_str(), to.as_str()))
        .filter(|(from, to)| from != to && owner.contains_key(from) && owner.contains_key(to))
        .collect();
    let mut fan_in: HashMap<&str, usize> = HashMap::new();
    let mut fan_out: HashMap<&str, usize> = HashMap::new();
    for (from, to) in &owned {
        *fan_out.entry(from).or_default() += 1;
        *fan_in.entry(to).or_default() += 1;
    }
    let mut inside: HashMap<&str, (usize, f64)> = HashMap::new();
    let mut between: BTreeMap<(&str, &str), (usize, f64)> = BTreeMap::new();
    let (mut total, mut across) = ((0, 0.0), (0, 0.0));
    for (from, to) in &owned {
        let weight = 1.0 / fan_in[to] as f64;
        let (a, b) = (owner[from], owner[to]);
        let cell = if a == b {
            inside.entry(a).or_default()
        } else {
            add(&mut across, weight);
            between.entry((a, b)).or_default()
        };
        add(cell, weight);
        add(&mut total, weight);
    }
    let degree = |file: &str| fan_in.get(file).unwrap_or(&0) + fan_out.get(file).unwrap_or(&0);
    let nodes: BTreeSet<&str> = owned.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let mean = if nodes.is_empty() {
        0.0
    } else {
        2.0 * owned.len() as f64 / nodes.len() as f64
    };
    let mut hubs: Vec<Hub> = nodes
        .iter()
        .filter(|file| degree(file) as f64 >= 3.0 * mean)
        .map(|file| Hub {
            file: file.to_string(),
            part: owner[file].to_string(),
            degree: degree(file),
            fan_in: *fan_in.get(file).unwrap_or(&0),
            fan_out: *fan_out.get(file).unwrap_or(&0),
        })
        .collect();
    hubs.sort_by(|a, b| b.degree.cmp(&a.degree).then_with(|| a.file.cmp(&b.file)));
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
            [("a.rs".to_string(), "b.rs".to_string())]
        );
    }
}
