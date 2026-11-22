//! Checks a repository's `system-map.json` against the files git tracks.
//!
//! `check` is the entry point: a repository's root in, the parts with the files each owns, the
//! links and the problems out.

mod pattern;

use std::{
    collections::{BTreeSet, HashSet},
    path::Path,
    process::Command,
};

use serde::{Deserialize, Serialize};

use pattern::Pattern;

/// The map file's name, at the repository's root.
pub const MAP_FILE: &str = "system-map.json";

/// What `check` found.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// The parts, in the file's order, each with the tracked files it owns.
    pub parts: Vec<PartFiles>,
    pub links: Vec<Link>,
    /// One line per problem; the check fails when there is any.
    pub problems: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PartFiles {
    pub id: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub id: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)] // Fields the map must hold; later checks and screens read them.
struct Map {
    version: u32,
    extractors: Option<Vec<String>>,
    parts: Vec<Part>,
    actors: Vec<Actor>,
    links: Vec<Link>,
    ignored: Vec<String>,
    laws: Vec<Law>,
    flows: Option<Vec<Flow>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Part {
    id: String,
    name: String,
    caption: String,
    description: String,
    tier: Tier,
    paths: Vec<String>,
    zone: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Tier {
    Core,
    Interface,
    BlackBox,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Actor {
    id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Law {
    id: String,
    on: String,
    backing: Backing,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Backing {
    Check,
    Trust,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Flow {
    id: String,
    name: String,
    hops: Vec<Hop>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct Hop {
    link: String,
    text: String,
}

/// Reads the map in `root` and checks it against the files git tracks there.
pub fn check(root: &Path) -> Report {
    let mut report = Report::default();
    let map = match load(root) {
        Ok(map) => map,
        Err(problem) => {
            report.problems.push(problem);
            return report;
        }
    };
    report.links = map.links.clone();
    report.problems.extend(check_ids_and_references(&map));
    match tracked_files(root) {
        Ok(files) => {
            let (parts, problems) = ownership(&map, &files);
            report.parts = parts;
            report.problems.extend(problems);
        }
        Err(problem) => report.problems.push(problem),
    }
    report
}

fn load(root: &Path) -> Result<Map, String> {
    let path = root.join(MAP_FILE);
    let text = std::fs::read_to_string(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            format!("{MAP_FILE}: missing; add a {MAP_FILE} at the repository's root")
        }
        _ => format!("{MAP_FILE}: cannot read it ({error})"),
    })?;
    serde_json::from_str(&text).map_err(|error| {
        let kind = if error.is_syntax() || error.is_eof() {
            "is not valid JSON"
        } else {
            "does not match the map's shape"
        };
        format!("{MAP_FILE}: {kind}: {error}; fix the file")
    })
}

fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn check_ids_and_references(map: &Map) -> Vec<String> {
    let mut problems = Vec::new();
    let flows = map.flows.as_deref().unwrap_or_default();
    let ids = map
        .parts
        .iter()
        .map(|p| ("part", &p.id))
        .chain(map.actors.iter().map(|a| ("actor", &a.id)))
        .chain(map.links.iter().map(|l| ("link", &l.id)))
        .chain(map.laws.iter().map(|l| ("law", &l.id)))
        .chain(flows.iter().map(|f| ("flow", &f.id)));
    let mut seen = HashSet::new();
    for (kind, id) in ids {
        if !is_id(id) {
            problems.push(format!(
                "{kind} id {id:?} is not lowercase words joined by hyphens; rename it, for example \"my-id\""
            ));
        }
        if !seen.insert(id.as_str()) {
            problems.push(format!(
                "id {id:?} is used more than once ({kind} repeats it); give each part, actor, link, law and flow its own id"
            ));
        }
    }
    let ends: HashSet<&str> = map
        .parts
        .iter()
        .map(|p| p.id.as_str())
        .chain(map.actors.iter().map(|a| a.id.as_str()))
        .collect();
    for link in &map.links {
        for end in [&link.from, &link.to] {
            if !ends.contains(end.as_str()) {
                problems.push(format!(
                    "link {:?} ends at {end:?}, which is neither a part nor an actor; name a part or add the actor",
                    link.id
                ));
            }
        }
    }
    let link_ids: HashSet<&str> = map.links.iter().map(|l| l.id.as_str()).collect();
    let targets: HashSet<&str> = map
        .parts
        .iter()
        .map(|p| p.id.as_str())
        .chain(link_ids.iter().copied())
        .chain(flows.iter().map(|f| f.id.as_str()))
        .collect();
    for law in &map.laws {
        if !targets.contains(law.on.as_str()) {
            problems.push(format!(
                "law {:?} is on {:?}, which is no part, link or flow; point it at one that exists",
                law.id, law.on
            ));
        }
    }
    for flow in flows {
        for hop in &flow.hops {
            if !link_ids.contains(hop.link.as_str()) {
                problems.push(format!(
                    "flow {:?} has a hop on link {:?}, which does not exist; name a declared link",
                    flow.id, hop.link
                ));
            }
        }
    }
    problems
}

/// The files git tracks in `root`, sorted.
fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git in {}: {error}", root.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git ls-files failed in {}: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let files: BTreeSet<String> = output
        .stdout
        .split(|&b| b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    Ok(files.into_iter().collect())
}

/// Which part owns each file, and the problems of files nobody owns and patterns that match none.
fn ownership(map: &Map, files: &[String]) -> (Vec<PartFiles>, Vec<String>) {
    let mut problems = Vec::new();
    let ignored: Vec<Pattern> = map.ignored.iter().map(|p| Pattern::new(p)).collect();
    let part_patterns: Vec<Vec<Pattern>> = map
        .parts
        .iter()
        .map(|part| part.paths.iter().map(|p| Pattern::new(p)).collect())
        .collect();
    let mut parts: Vec<PartFiles> = map
        .parts
        .iter()
        .map(|part| PartFiles {
            id: part.id.clone(),
            files: Vec::new(),
        })
        .collect();
    for file in files {
        if ignored.iter().any(|p| p.matches(file)) {
            continue;
        }
        let owner = part_patterns
            .iter()
            .rposition(|patterns| patterns.iter().any(|p| p.matches(file)));
        match owner {
            Some(index) => parts[index].files.push(file.clone()),
            None => problems.push(format!(
                "{file}: tracked, but no part owns it and no ignored pattern matches it; add it to a part's paths or to ignored"
            )),
        }
    }
    let unmatched = |kind: &str, pattern: &Pattern| {
        (!files.iter().any(|file| pattern.matches(file))).then(|| {
            format!(
                "{kind} pattern {:?} matches no tracked file; fix or remove it",
                pattern.text()
            )
        })
    };
    for (part, patterns) in map.parts.iter().zip(&part_patterns) {
        let kind = format!("part {:?}", part.id);
        problems.extend(patterns.iter().filter_map(|p| unmatched(&kind, p)));
    }
    problems.extend(ignored.iter().filter_map(|p| unmatched("ignored", p)));
    (parts, problems)
}
