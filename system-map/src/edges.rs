//! The link check: the pairs the map's extractors find (`crate::extract`) as part-to-part edges, and the map's import
//! links held to them; and each link's own fields.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::{
    Kind, Link, PartFiles,
    extract::{self, Extractor, Pair, Repo},
};

/// A computed dependency of the part `from` on the part `to`, with the deduplicated file pairs that make it (spec #473,
/// "One pair per file pair").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// Sorted by their files.
    pub pairs: Vec<FilePair>,
}

/// A dependency of the tracked file `from` on the tracked file `to`, with the import sites behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilePair {
    pub from: String,
    pub to: String,
    /// One per line, in line order; one site with no line when the extractor finds none.
    pub sites: Vec<Site>,
}

/// Where an import stands: always in the importing file, at its 1-based line when one is known (spec #477, "What an
/// import site is").
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Site {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// What the link check found: the computed edges, its problems, and how many import links a computed edge proves and how
/// many have an end no extractor reads.
pub(crate) struct LinkCheck {
    pub(crate) edges: Vec<Edge>,
    pub(crate) problems: Vec<String>,
    pub(crate) proven: usize,
    pub(crate) unproven: usize,
}

/// Holds the map's import links to the edges the named extractors compute (spec #473, "The link check"). Between two read
/// parts, an import link with no edge in its direction fails, and so does an edge with no import link in its direction,
/// whatever run-time links join the pair. An import link with an unread end passes as unproven, and so does one no edge
/// proves while an extractor is unknown or cannot run. Also the problems of those extractors.
pub(crate) fn check_links(
    registry: &[&dyn Extractor],
    names: &[String],
    repo: &Repo,
    parts: &[PartFiles],
    links: &[Link],
) -> LinkCheck {
    let found = extract::run(registry, names, repo);
    let mut problems = found.problems;
    // An extractor that could not run leaves pairs unread, so a missing edge proves nothing.
    let incomplete = !problems.is_empty();
    let edges = edges_between_parts(&found.pairs, parts);
    let read = extract::read_parts(registry, names, repo, parts);
    let part_ids: HashSet<&str> = parts.iter().map(|p| p.id.as_str()).collect();
    let imports: HashSet<(&str, &str)> = links
        .iter()
        .filter(|l| l.kind == Some(Kind::Import))
        .map(|l| (l.from.as_str(), l.to.as_str()))
        .collect();
    let computed: HashSet<(&str, &str)> = edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    let (mut proven, mut unproven) = (0, 0);
    for link in links.iter().filter(|l| l.kind == Some(Kind::Import)) {
        let (from, to) = (link.from.as_str(), link.to.as_str());
        // An actor at an end is its own problem; an end that names nothing is a reference problem.
        if !part_ids.contains(from) || !part_ids.contains(to) {
            continue;
        }
        if computed.contains(&(from, to)) && read.contains(from) && read.contains(to) {
            proven += 1;
        } else if !read.contains(from) || !read.contains(to) || incomplete {
            unproven += 1;
        } else if computed.contains(&(to, from)) {
            problems.push(format!(
                "import link {:?} runs from {from:?} to {to:?}, but the code shows no dependency of {from:?} on {to:?}, only of \
                 {to:?} on {from:?}; turn it around, so it runs from the dependent part to the one it depends on",
                link.id
            ));
        } else {
            problems.push(format!(
                "import link {:?} runs from {from:?} to {to:?}, but the code shows no dependency of {from:?} on {to:?}; remove \
                 it, or give it the run-time kind (http, spawn or file) and the reason that joins them",
                link.id
            ));
        }
    }
    for edge in &edges {
        let (from, to) = (edge.from.as_str(), edge.to.as_str());
        if read.contains(from) && read.contains(to) && !imports.contains(&(from, to)) {
            problems.push(format!(
                "part {from:?} depends on part {to:?} in the code, and no import link runs from {from:?} to {to:?}; add \
                 {{ \"from\": {from:?}, \"to\": {to:?}, \"kind\": \"import\" }}, even beside a run-time link"
            ));
        }
    }
    LinkCheck {
        edges,
        problems,
        proven,
        unproven,
    }
}

/// The problems of each link's own fields: a missing kind, a run-time link without a reason or with one past its cap, and
/// an import link with an actor at an end, since an actor owns no code.
pub(crate) fn check_link_fields(links: &[Link], actors: &HashSet<&str>) -> Vec<String> {
    let mut problems = Vec::new();
    for link in links {
        let id = &link.id;
        let Some(kind) = link.kind else {
            problems.push(format!(
                "link {id:?} has no kind; set \"kind\" to import, http, spawn or file"
            ));
            continue;
        };
        if kind == Kind::Import {
            for end in [&link.from, &link.to] {
                if actors.contains(end.as_str()) {
                    problems.push(format!(
                        "import link {id:?} ends at the actor {end:?}, which owns no code; give the link a run-time kind \
                         (http, spawn or file) and a reason"
                    ));
                }
            }
        }
        let reason = link.reason.as_deref().unwrap_or_default();
        let length = reason.chars().count();
        if kind != Kind::Import && reason.trim().is_empty() {
            problems.push(format!(
                "link {id:?} is a {} link with no reason; say in one sentence what crosses it",
                kind.name()
            ));
        } else if length > REASON_CAP {
            problems.push(format!(
                "link {id:?}: reason is {} characters, the cap is {REASON_CAP}; keep one sentence here and move the detail \
                 into the area's doc under docs/studio/behaviour/",
                crate::parts::grouped(length)
            ));
        }
    }
    problems
}

/// The most characters a link's reason holds: one sentence.
const REASON_CAP: usize = 140;

/// Turns pairs of files into edges between the parts that own them, dropping pairs inside one
/// part and pairs touching a file no part owns (ignored files too).
fn edges_between_parts(pairs: &[Pair], parts: &[PartFiles]) -> Vec<Edge> {
    let owner: HashMap<&str, &str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(|f| (f.as_str(), part.id.as_str())))
        .collect();
    let mut edges: BTreeMap<(&str, &str), Vec<FilePair>> = BTreeMap::new();
    for pair in pairs {
        let (Some(from), Some(to)) = (owner.get(pair.from.as_str()), owner.get(pair.to.as_str()))
        else {
            continue;
        };
        if from == to {
            continue;
        }
        let site = |line| Site {
            file: pair.from.clone(),
            line,
        };
        let sites = if pair.lines.is_empty() {
            vec![site(None)]
        } else {
            pair.lines.iter().map(|line| site(Some(*line))).collect()
        };
        edges.entry((from, to)).or_default().push(FilePair {
            from: pair.from.clone(),
            to: pair.to.clone(),
            sites,
        });
    }
    edges
        .into_iter()
        .map(|((from, to), pairs)| Edge {
            from: from.to_string(),
            to: to.to_string(),
            pairs,
        })
        .collect()
}
