//! Checks a repository's `system-map.json` against the files git tracks.
//!
//! `check` is the entry point: a repository's root in, the parts with the files each owns, the
//! links and the problems out.

// Spec #548, Group 3: these two crates document their public API.
#![warn(missing_docs)]

mod edges;
mod imports;
mod laws;
mod parts;
mod pattern;
mod touched;
mod typescript;

use std::{
    collections::{BTreeSet, HashSet},
    path::Path,
    time::Duration,
};
use studio_workbench::run::{RunError, git, run_until};

use serde::{Deserialize, Serialize};

pub use imports::{Across, Count, Hub, Imports, Inside, Share, imports};
pub use laws::{LawReport, LawTest, Tag, law_tests, tag_id, test_name_after};
pub use parts::{convert, convert_value, part_text};
use pattern::Pattern;
pub use touched::{Touched, touched, touched_for_map};

/// The map file's name, at the repository's root.
pub const MAP_FILE: &str = "system-map.json";

/// What `check` found.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    /// The parts, in the file's order, each with the tracked files it owns.
    pub parts: Vec<PartFiles>,
    /// The links, in the file's order.
    pub links: Vec<Link>,
    /// Each law with its backing and the tags that name it.
    pub laws: Vec<LawReport>,
    /// One line per problem; the check fails when there is any.
    pub problems: Vec<String>,
    /// What the check covered, for the line a pass prints.
    pub counts: Counts,
}

/// What a check covered: parts, the tracked files they own and the ones ignored, links, and laws
/// by backing.
#[derive(Debug, Default, Serialize)]
pub struct Counts {
    /// The parts.
    pub parts: usize,
    /// The tracked files the parts own.
    pub files: usize,
    /// The tracked files no part owns on purpose.
    pub ignored: usize,
    /// The links, by kind.
    pub links: LinkCounts,
    /// The laws, by backing.
    pub laws: LawCounts,
}

/// The links by kind, and the import links proven by a computed edge or unproven, with an end no extractor reads.
#[derive(Debug, Default, Serialize)]
pub struct LinkCounts {
    /// Every link.
    pub total: usize,
    /// The `import` links.
    pub import: usize,
    /// The `http` links.
    pub http: usize,
    /// The `spawn` links.
    pub spawn: usize,
    /// The `file` links.
    pub file: usize,
    /// The import links a computed edge runs the way of.
    pub proven: usize,
    /// The import links with an end that no extractor reads.
    pub unproven: usize,
}

/// The laws by backing.
#[derive(Debug, Default, Serialize)]
pub struct LawCounts {
    /// Those with a test behind them.
    pub check: usize,
    /// Those with none.
    pub trust: usize,
}

/// A part with the files it owns or touches.
#[derive(Debug, Serialize)]
pub struct PartFiles {
    /// The part's id.
    pub id: String,
    /// The files' paths from the repository's root.
    pub files: Vec<String>,
}

/// A link of the map, from one end to another.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// The link's id.
    pub id: String,
    /// The id of the link's first end.
    pub from: String,
    /// The id of the link's second end.
    pub to: String,
    /// What the link carries; the check requires it, and a converted version 1 map's links have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Kind>,
    /// One sentence on what crosses a run-time link; required for every kind but `import`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A link's kind (spec #473): `import`, a code dependency the readers can prove, or one of three run-time kinds they cannot
/// see.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A code dependency, which the readers can prove.
    Import,
    /// An HTTP call at run time.
    Http,
    /// A process started at run time.
    Spawn,
    /// A file the two ends share at run time.
    File,
}

impl Kind {
    /// The kind as the map writes it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Import => "import",
            Kind::Http => "http",
            Kind::Spawn => "spawn",
            Kind::File => "file",
        }
    }
}

/// A parsed `system-map.json`. Its fields are private: build one with `parse` or `parse_lenient`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields the map must hold, which `deny_unknown_fields` checks; later checks and screens read them"
)]
pub struct Map {
    version: u32,
    extractors: Option<Vec<String>>,
    parts: Vec<Part>,
    actors: Vec<Actor>,
    links: Vec<Link>,
    ignored: Vec<String>,
    laws: Vec<Law>,
    flows: Option<Vec<Flow>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields the map must hold, which `deny_unknown_fields` checks"
)]
struct Part {
    id: String,
    name: String,
    caption: String,
    purpose: String,
    holds: Vec<parts::Held>,
    tier: Tier,
    paths: Vec<String>,
    zone: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Tier {
    Core,
    Interface,
    BlackBox,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields the map must hold, which `deny_unknown_fields` checks"
)]
struct Actor {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields the map must hold, which `deny_unknown_fields` checks"
)]
struct Law {
    id: String,
    on: String,
    backing: Backing,
    text: String,
}

/// What stands behind a law.
#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum Backing {
    /// A test tagged `@law: <id>` holds it.
    Check,
    /// No test does: it is taken on trust.
    Trust,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields the map must hold, which `deny_unknown_fields` checks"
)]
struct Flow {
    id: String,
    name: String,
    hops: Vec<Hop>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    dead_code,
    reason = "fields the map must hold, which `deny_unknown_fields` checks"
)]
struct Hop {
    link: String,
    text: String,
}

/// Reads the map in `root` and checks it against the files git tracks there.
pub fn check(root: &Path) -> Report {
    check_with(root, tracked_files(root))
}

/// `check` against `files`, the tracked files as a caller listed them: plotkeeper lists a clone's inside its sandbox, since
/// git outside it would read the clone's own config (back a law, #398).
pub fn check_files(root: &Path, files: Vec<String>) -> Report {
    let files: BTreeSet<String> = files.into_iter().collect();
    check_with(root, Ok(files.into_iter().collect()))
}

/// The laws of the map in `root` with their backing and tags, and the laws' problems, read leniently, so a version 1 map
/// answers too; the load's problem when the map does not read. `files` are the tracked files, as for `check_files`.
pub fn law_reports(
    root: &Path,
    files: Vec<String>,
) -> Result<(Vec<LawReport>, Vec<String>), String> {
    let path = root.join(MAP_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("{MAP_FILE}: cannot read it ({error})"))?;
    let map = parse_lenient(&text)?;
    let files: BTreeSet<String> = files.into_iter().collect();
    let files: Vec<String> = files.into_iter().collect();
    Ok(laws::check(root, &map, &files))
}

fn check_with(root: &Path, files: Result<Vec<String>, String>) -> Report {
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
    // A part's purpose and held items, and their caps, are `check`'s alone: `touched` and `imports` still answer a map over a cap.
    report.problems.extend(parts::check(&map));
    let actors: HashSet<&str> = map.actors.iter().map(|a| a.id.as_str()).collect();
    report
        .problems
        .extend(edges::check_link_fields(&map.links, &actors));
    match files {
        Ok(files) => {
            let (parts, ignored, problems) = ownership(&map, &files);
            report.problems.extend(problems);
            report.problems.extend(parts::check_paths(&map, &parts));
            let (laws, problems) = laws::check(root, &map, &files);
            report.laws = laws;
            report.problems.extend(problems);
            let names = map.extractors.as_deref().unwrap_or_default();
            let found = edges::check_links(root, names, &files, &parts, &map.links);
            report.problems.extend(found.problems);
            let kinds = |kind: Kind| map.links.iter().filter(|l| l.kind == Some(kind)).count();
            report.counts = Counts {
                parts: parts.len(),
                files: parts.iter().map(|p| p.files.len()).sum(),
                ignored,
                links: LinkCounts {
                    total: map.links.len(),
                    import: kinds(Kind::Import),
                    http: kinds(Kind::Http),
                    spawn: kinds(Kind::Spawn),
                    file: kinds(Kind::File),
                    proven: found.proven,
                    unproven: found.unproven,
                },
                laws: LawCounts {
                    check: map
                        .laws
                        .iter()
                        .filter(|l| l.backing == Backing::Check)
                        .count(),
                    trust: map
                        .laws
                        .iter()
                        .filter(|l| l.backing == Backing::Trust)
                        .count(),
                },
            };
            report.parts = parts;
        }
        Err(problem) => report.problems.push(problem),
    }
    report
}

/// The map, when it has the shape and no problem of ids or references; else every problem.
fn load_checked(root: &Path) -> Result<Map, Vec<String>> {
    let map = load(root).map_err(|problem| vec![problem])?;
    let problems = check_ids_and_references(&map);
    if problems.is_empty() {
        Ok(map)
    } else {
        Err(problems)
    }
}

fn load(root: &Path) -> Result<Map, String> {
    let path = root.join(MAP_FILE);
    let text = std::fs::read_to_string(&path).map_err(|error| match error.kind() {
        std::io::ErrorKind::NotFound => {
            format!("{MAP_FILE}: missing; add a {MAP_FILE} at the repository's root")
        }
        _ => format!("{MAP_FILE}: cannot read it ({error})"),
    })?;
    parse(&text)
}

/// Parses map text strictly, as `check` does: an unknown field is an error.
pub fn parse(text: &str) -> Result<Map, String> {
    if serde_json::from_str::<serde_json::Value>(text).is_ok_and(|map| map["version"] == 1) {
        return Err(format!(
            "{MAP_FILE}: version 1 is not one this checker reads; it reads version 2, so set \"version\" to 2 and give each \
             part a \"purpose\" and \"holds\" in place of its \"description\""
        ));
    }
    serde_json::from_str(text).map_err(|error| {
        let kind = if error.is_syntax() || error.is_eof() {
            "is not valid JSON"
        } else {
            "does not match the map's shape"
        };
        format!("{MAP_FILE}: {kind}: {error}; fix the file")
    })
}

/// The fields each kind of object in the map holds, for `parse_lenient` to tell the unknown ones.
const MAP_FIELDS: &[&str] = &[
    "version",
    "extractors",
    "parts",
    "actors",
    "links",
    "ignored",
    "laws",
    "flows",
];
const PART_FIELDS: &[&str] = &[
    "id", "name", "caption", "purpose", "holds", "tier", "paths", "zone",
];
const ACTOR_FIELDS: &[&str] = &["id", "name"];
const LINK_FIELDS: &[&str] = &["id", "from", "to", "kind", "reason"];
const LAW_FIELDS: &[&str] = &["id", "on", "backing", "text"];
const FLOW_FIELDS: &[&str] = &["id", "name", "hops"];
const HOP_FIELDS: &[&str] = &["link", "text"];

/// Parses the text of a historical `system-map.json` read from git: a version 1 map is converted
/// first, and fields this reader does not know are ignored, so a later schema addition never
/// breaks a walk through old commits. Fails only when the text is not JSON or lacks a field the
/// map must hold.
pub fn parse_lenient(text: &str) -> Result<Map, String> {
    let mut value = convert_value(text)?;
    prune(&mut value, MAP_FIELDS);
    for (key, fields, nested) in [
        ("parts", PART_FIELDS, Some(("holds", parts::HELD_FIELDS))),
        ("actors", ACTOR_FIELDS, None),
        ("links", LINK_FIELDS, None),
        ("laws", LAW_FIELDS, None),
        ("flows", FLOW_FIELDS, Some(("hops", HOP_FIELDS))),
    ] {
        let Some(items) = value.get_mut(key).and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for item in items {
            prune(item, fields);
            if let Some((inner, inner_fields)) = nested
                && let Some(inner) = item.get_mut(inner).and_then(|v| v.as_array_mut())
            {
                inner.iter_mut().for_each(|each| prune(each, inner_fields));
            }
        }
    }
    parse(&value.to_string())
}

/// Drops the keys of an object that are not in `known`; anything else is left as it is.
fn prune(value: &mut serde_json::Value, known: &[&str]) {
    if let Some(object) = value.as_object_mut() {
        object.retain(|key, _| known.contains(&key.as_str()));
    }
}

/// Whether `id` is an id the map takes: lowercase words of letters and digits joined by hyphens.
pub fn is_id(id: &str) -> bool {
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
    if map.version != 2 {
        problems.push(format!(
            "{MAP_FILE}: version {} is not one this checker reads; it reads version 2, so set \"version\" to 2",
            map.version
        ));
    }
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

/// The longest listing a repository's files may take.
const GIT_LIMIT: Duration = Duration::from_secs(30);

/// The files git tracks in `root`, sorted.
fn tracked_files(root: &Path) -> Result<Vec<String>, String> {
    let listed = run_until(git(root).args(["ls-files", "-z"]), GIT_LIMIT);
    let output = listed.map_err(|error| match error {
        RunError::Start(error) => format!("cannot run git in {}: {error}", root.display()),
        error => format!("git ls-files failed in {}: {error}", root.display()),
    })?;
    let files: BTreeSet<String> = output
        .stdout
        .split(|&b| b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect();
    Ok(files.into_iter().collect())
}

/// The patterns that place a path: the ignored ones and each part's.
pub(crate) struct Owners {
    ignored: Vec<Pattern>,
    parts: Vec<Vec<Pattern>>,
}

/// Where a path stands in the map.
pub(crate) enum Placement {
    Ignored,
    Part(usize),
    Unowned,
}

impl Owners {
    pub(crate) fn new(map: &Map) -> Owners {
        Owners {
            ignored: map.ignored.iter().map(|p| Pattern::new(p)).collect(),
            parts: map
                .parts
                .iter()
                .map(|part| part.paths.iter().map(|p| Pattern::new(p)).collect())
                .collect(),
        }
    }

    /// The ownership rule, held here for `check` and `touched` alike: the map file needs no owner,
    /// then a match in `ignored` wins, then the last part with a matching pattern.
    pub(crate) fn place(&self, path: &str) -> Placement {
        if path == MAP_FILE || self.ignored.iter().any(|p| p.matches(path)) {
            return Placement::Ignored;
        }
        match self
            .parts
            .iter()
            .rposition(|patterns| patterns.iter().any(|p| p.matches(path)))
        {
            Some(index) => Placement::Part(index),
            None => Placement::Unowned,
        }
    }
}

/// The text of a tracked file, or none for one the scans do not read: a symlink (its target may lie
/// outside the repository), a file that is gone or a folder. Bytes that are not UTF-8 read lossily.
pub(crate) fn read_tracked(root: &Path, file: &str) -> Option<String> {
    let path = root.join(file);
    if !std::fs::symlink_metadata(&path).ok()?.is_file() {
        return None;
    }
    Some(String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned())
}

/// Which part owns each file, how many files are ignored, and the problems of files nobody owns
/// and patterns that match none.
fn ownership(map: &Map, files: &[String]) -> (Vec<PartFiles>, usize, Vec<String>) {
    let mut problems = Vec::new();
    let mut ignored = 0;
    let owners = Owners::new(map);
    let mut parts: Vec<PartFiles> = map
        .parts
        .iter()
        .map(|part| PartFiles {
            id: part.id.clone(),
            files: Vec::new(),
        })
        .collect();
    for file in files {
        match owners.place(file) {
            Placement::Ignored => ignored += 1,
            Placement::Part(index) => parts[index].files.push(file.clone()),
            Placement::Unowned => problems.push(format!(
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
    for (part, patterns) in map.parts.iter().zip(&owners.parts) {
        let kind = format!("part {:?}", part.id);
        for pattern in patterns {
            let only_the_map = pattern.matches(MAP_FILE)
                && !files
                    .iter()
                    .any(|file| file != MAP_FILE && pattern.matches(file));
            if only_the_map {
                problems.push(format!(
                    "{kind} pattern {:?} names only {MAP_FILE}, which needs no owner; remove it",
                    pattern.text()
                ));
            } else {
                problems.extend(unmatched(&kind, pattern));
            }
        }
    }
    problems.extend(
        owners
            .ignored
            .iter()
            .filter(|p| p.text() != MAP_FILE)
            .filter_map(|p| unmatched("ignored", p)),
    );
    (parts, ignored, problems)
}
