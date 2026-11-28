//! The free scan: a repository's first map proposed from its folders and build manifests, with no model (spec #753,
//! "The free scan"). It lists the tracked files and reads only manifests and Go files, never an import.
//!
//! The rule is the lean rule of 2026-11-24 with four additions: step into the folder that holds most of the code (80% of
//! its parent's code files); take the members of a tracked workspace manifest, and each folder holding a Go `package
//! main`, as parts; report the loose files (code or not) and tests no part owns; list the folders that look like reference
//! copies.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::Serialize;
use serde_json::{Value, json};

use crate::extract::{Repo, registry::EXTRACTORS};

/// The most parts a scan proposes.
const MAX_PARTS: usize = 15;

/// What the scan proposes.
#[derive(Debug, Serialize)]
pub struct Scan {
    /// A version 2 map: ids, `paths`, `tier: "interface"`, `holds: []`, placeholder text, the detected extractors and the
    /// ticked ignore candidates in `ignored`.
    pub map: Value,
    /// Folders that look like copies or vendored code, ticked when the scan is sure.
    pub ignore_candidates: Vec<IgnoreCandidate>,
    /// Tracked files no part owns and no ignored folder holds, code or not, tests apart.
    pub loose_files: Vec<String>,
    /// Test files no part owns and no ignored folder holds.
    pub loose_tests: Vec<String>,
    /// Languages of the code (tests apart) that an extractor reads, so the scan's links cover them.
    pub read_languages: Vec<String>,
    /// Languages of the code that no extractor reads.
    pub unread_languages: Vec<String>,
}

/// A folder that may be a reference copy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct IgnoreCandidate {
    /// The folder, with its trailing slash.
    pub path: String,
    /// Whether the scan is sure enough to ignore it already.
    pub ticked: bool,
}

/// Scans the repository at `root`.
///
/// # Errors
/// When git cannot list the tracked files.
pub fn scan(root: &Path) -> Result<Scan, String> {
    let files = crate::tracked_files(root)?;
    Ok(scan_files(root, &files))
}

/// The languages the scan knows by file extension.
const LANGUAGES: &[(&str, &str)] = &[
    ("ts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("js", "JavaScript"),
    ("jsx", "JavaScript"),
    ("mjs", "JavaScript"),
    ("cjs", "JavaScript"),
    ("go", "Go"),
    ("py", "Python"),
    ("rs", "Rust"),
    ("dart", "Dart"),
    ("java", "Java"),
    ("kt", "Kotlin"),
    ("php", "PHP"),
    ("cs", "C#"),
    ("rb", "Ruby"),
    ("swift", "Swift"),
    ("c", "C"),
    ("h", "C"),
    ("cpp", "C++"),
    ("cc", "C++"),
    ("hpp", "C++"),
    ("cxx", "C++"),
    ("c++", "C++"),
    ("hh", "C++"),
    ("hxx", "C++"),
    ("h++", "C++"),
    ("inl", "C++"),
    ("ipp", "C++"),
    ("tcc", "C++"),
    ("sql", "SQL"),
    ("sh", "Shell"),
    ("proto", "Protobuf"),
    ("html", "HTML"),
    ("css", "CSS"),
    ("vue", "Vue"),
    ("svelte", "Svelte"),
];

fn language(file: &str) -> Option<&'static str> {
    let ext = file.rsplit_once('.')?.1;
    LANGUAGES.iter().find(|(e, _)| *e == ext).map(|(_, l)| *l)
}

fn is_hidden(file: &str) -> bool {
    file.split('/').any(|part| part.starts_with('.'))
}

/// Whether `file` is code the scan proposes parts from: in a language it knows, in no hidden folder.
pub fn is_code(file: &str) -> bool {
    language(file).is_some() && !is_hidden(file)
}

/// Whether `file` is a test: in a folder of tests, or named as one.
pub fn is_test(file: &str) -> bool {
    let (dirs, name) = file.rsplit_once('/').map_or(("", file), |(d, n)| (d, n));
    dirs.split('/').any(|d| {
        matches!(
            d,
            "__tests__" | "tests" | "test" | "spec" | "specs" | "__mocks__"
        )
    }) || name.contains(".test.")
        || name.contains(".spec.")
        || name.contains("_test.")
        || name.starts_with("test_")
}

/// Whether `file` is in `folder` (which ends in a slash).
fn under(file: &str, folder: &str) -> bool {
    file.starts_with(folder)
}

/// The folder a reference copy would be, when `name` says so: `Some(true)` when the scan is sure.
fn reference_copy(name: &str) -> Option<bool> {
    let lower = name.to_lowercase();
    if matches!(lower.as_str(), "vendor" | "third_party" | "third-party") {
        return Some(true);
    }
    let named = matches!(lower.as_str(), "external" | "deps")
        || ["copy", "reference", "snapshot"]
            .iter()
            .any(|word| lower.contains(word));
    named.then_some(false)
}

/// A file read from the working tree, never through a symlink.
fn read_regular(root: &Path, file: &str) -> Option<String> {
    let path = root.join(file);
    fs::symlink_metadata(&path).ok()?.is_file().then_some(())?;
    fs::read_to_string(path).ok()
}

/// Whether the one-segment glob `pattern` (`*` matches any run of characters) matches `segment`.
fn segment_matches(pattern: &str, segment: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == segment,
        // The rest after `head` is tried from each character on, never inside one.
        Some((head, tail)) => segment.strip_prefix(head).is_some_and(|rest| {
            rest.char_indices()
                .map(|(at, _)| at)
                .chain([rest.len()])
                .any(|at| segment_matches(tail, &rest[at..]))
        }),
    }
}

/// The workspace member patterns the root manifests tracked in `files` list, and the patterns they exclude.
fn workspace_patterns(root: &Path, files: &[String]) -> (Vec<String>, Vec<String>) {
    let (mut include, mut exclude) = (Vec::new(), Vec::new());
    let mut add = |pattern: &str| {
        let pattern = pattern
            .trim()
            .trim_start_matches("./")
            .trim_end_matches('/');
        match pattern.strip_prefix('!') {
            Some(rest) => exclude.push(rest.trim_start_matches("./").trim_end_matches('/').into()),
            None if !pattern.is_empty() => include.push(pattern.to_string()),
            None => {}
        }
    };
    let has = |name: &str| files.iter().any(|f| f == name);
    if has("package.json")
        && let Some(text) = read_regular(root, "package.json")
        && let Ok(json) = serde_json::from_str::<Value>(&text)
    {
        let list = match &json["workspaces"] {
            Value::Array(list) => Some(list),
            Value::Object(object) => object.get("packages").and_then(Value::as_array),
            _ => None,
        };
        for pattern in list.into_iter().flatten().filter_map(Value::as_str) {
            add(pattern);
        }
    }
    if has("pnpm-workspace.yaml")
        && let Some(text) = read_regular(root, "pnpm-workspace.yaml")
    {
        let mut in_packages = false;
        for line in text.lines() {
            if line.trim_end() == "packages:" {
                in_packages = true;
            } else if in_packages {
                let item = line.trim_start().strip_prefix('-');
                match item {
                    Some(item) => add(item.trim().trim_matches(['\'', '"'])),
                    None if line.starts_with(char::is_whitespace) || line.trim().is_empty() => {}
                    None => in_packages = false,
                }
            }
        }
    }
    if has("go.work")
        && let Some(text) = read_regular(root, "go.work")
    {
        let mut in_block = false;
        for line in text.lines().map(str::trim) {
            if in_block {
                if line == ")" {
                    in_block = false;
                } else {
                    add(line);
                }
            } else if let Some(rest) = line.strip_prefix("use") {
                let rest = rest.trim();
                if rest == "(" {
                    in_block = true;
                } else {
                    add(rest);
                }
            }
        }
    }
    if has("Cargo.toml")
        && let Some(text) = read_regular(root, "Cargo.toml")
        && let Ok(toml) = toml::from_str::<toml::Value>(&text)
    {
        let members = toml.get("workspace").and_then(|w| w.get("members"));
        for pattern in members.and_then(|m| m.as_array()).into_iter().flatten() {
            if let Some(pattern) = pattern.as_str() {
                add(pattern);
            }
        }
    }
    (include, exclude)
}

/// The folders of `dirs` that the member `pattern` names.
fn matching(dirs: &BTreeSet<String>, pattern: &str) -> Vec<String> {
    let segments: Vec<&str> = pattern.split('/').collect();
    dirs.iter()
        .filter(|dir| {
            let parts: Vec<&str> = dir.trim_end_matches('/').split('/').collect();
            parts.len() == segments.len()
                && parts
                    .iter()
                    .zip(&segments)
                    .all(|(part, pattern)| segment_matches(pattern, part))
        })
        .cloned()
        .collect()
}

/// Every folder, with its trailing slash, that holds one of `files` at any depth.
fn folders_of<'a>(files: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    let mut dirs = BTreeSet::new();
    for file in files {
        for (at, _) in file.match_indices('/') {
            dirs.insert(file[..=at].to_string());
        }
    }
    dirs
}

/// The child folders of `prefix` among `files`, each with its file count.
fn children<'a>(prefix: &str, files: impl Iterator<Item = &'a str>) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    for file in files {
        if let Some(rest) = file.strip_prefix(prefix)
            && let Some((first, _)) = rest.split_once('/')
        {
            *found.entry(format!("{prefix}{first}/")).or_insert(0) += 1;
        }
    }
    found
}

/// The seeds of the parts: the members of a tracked workspace manifest and the folders of Go `main` packages, none inside
/// another.
fn seeds_of(root: &Path, files: &[String], pool: &[&str]) -> Vec<String> {
    let (include, exclude) = workspace_patterns(root, files);
    let dirs = folders_of(pool.iter().copied());
    let excluded: BTreeSet<String> = exclude.iter().flat_map(|p| matching(&dirs, p)).collect();
    let mut seeds: BTreeSet<String> = include
        .iter()
        .flat_map(|p| matching(&dirs, p))
        .filter(|d| !excluded.contains(d))
        .collect();
    for file in pool {
        if file.ends_with(".go")
            && let Some((dir, _)) = file.rsplit_once('/')
            && read_regular(root, file).is_some_and(|text| {
                text.lines()
                    .any(|l| l.trim() == "package main" || l.starts_with("package main "))
            })
        {
            seeds.insert(format!("{dir}/"));
        }
    }
    seeds
        .iter()
        .filter(|s| !seeds.iter().any(|o| o != *s && under(s, o)))
        .cloned()
        .collect()
}

/// The build manifests that mark a folder as built on its own.
const MANIFESTS: &[&str] = &[
    "package.json",
    "go.mod",
    "Cargo.toml",
    "pubspec.yaml",
    "pyproject.toml",
    "setup.py",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
    "composer.json",
    "Gemfile",
];

/// The top-level folders that look like reference copies though no name says so: a folder holding at least a fifth of the
/// code, with no build manifest at any depth, that no code file outside it names. Only in a repository that tracks a
/// manifest somewhere, since with none every folder would qualify. "Names" is a text search of the code files outside the
/// folder for its folder name, not the extractors, which need a map to run.
fn unbuilt_unimported(
    root: &Path,
    files: &[String],
    code: &[&str],
    known: &BTreeMap<String, bool>,
) -> Vec<String> {
    let is_manifest = |f: &str| {
        let name = f.rsplit('/').next().unwrap_or(f);
        MANIFESTS.contains(&name) || name.ends_with(".csproj")
    };
    if !files.iter().any(|f| is_manifest(f) && !is_hidden(f)) {
        return vec![];
    }
    let mut found = Vec::new();
    for (folder, count) in children("", code.iter().copied()) {
        if known.contains_key(&folder)
            || count * 5 < code.len()
            || files.iter().any(|f| under(f, &folder) && is_manifest(f))
        {
            continue;
        }
        let name = folder.trim_end_matches('/');
        let named = code
            .iter()
            .filter(|f| !under(f, &folder))
            .any(|f| read_regular(root, f).is_some_and(|text| text.contains(name)));
        if !named {
            found.push(folder);
        }
    }
    found
}

/// The languages of a scan's code (tests apart): those an extractor reads, and those none does.
struct Languages {
    read: Vec<String>,
    unread: Vec<String>,
}

/// The extractors that detect a file they read, and the languages of the code (tests apart), read and not.
fn readers(
    root: &Path,
    files: &[String],
    code: &[&str],
    ticked: &[String],
) -> (Vec<&'static str>, Languages) {
    let ignored = |f: &str| ticked.iter().any(|t| under(f, t));
    let repo = Repo::new(root, files, ignored);
    let extractors = EXTRACTORS
        .iter()
        .filter(|e| e.detect(&repo))
        .map(|e| e.name())
        .collect();
    let (mut read, mut unread) = (BTreeSet::new(), BTreeSet::new());
    for file in code.iter().filter(|f| !is_test(f) && !ignored(f)) {
        if let Some(language) = language(file) {
            if EXTRACTORS.iter().any(|e| e.reads(file)) {
                read.insert(language);
            } else {
                unread.insert(language);
            }
        }
    }
    let list = |set: BTreeSet<&str>| set.into_iter().map(String::from).collect();
    (
        extractors,
        Languages {
            read: list(read),
            unread: list(unread),
        },
    )
}

fn scan_files(root: &Path, files: &[String]) -> Scan {
    let code: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|f| is_code(f))
        .collect();
    let all: Vec<&str> = files
        .iter()
        .map(String::as_str)
        .filter(|f| !is_hidden(f))
        .collect();

    // Reference copies: among the top-level folders, and at each step of the descent.
    let mut candidates: BTreeMap<String, bool> = BTreeMap::new();
    let mut ticked: Vec<String> = Vec::new();
    let note = |prefix: &str, candidates: &mut BTreeMap<String, bool>, ticked: &mut Vec<String>| {
        for folder in children(prefix, all.iter().copied()).into_keys() {
            let name = folder
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("");
            if let Some(sure) = reference_copy(name)
                && candidates.insert(folder.clone(), sure).is_none()
                && sure
            {
                ticked.push(folder);
            }
        }
    };
    note("", &mut candidates, &mut ticked);

    let in_play = |file: &str, ticked: &[String]| !ticked.iter().any(|t| under(file, t));
    let mut pool: Vec<&str> = code
        .iter()
        .copied()
        .filter(|f| !is_test(f) && in_play(f, &ticked))
        .collect();

    let seeds = seeds_of(root, files, &pool);

    let mut parts: Vec<String> = if seeds.is_empty() {
        let mut prefix = String::new();
        loop {
            let counts = children(&prefix, pool.iter().copied());
            let top = counts
                .iter()
                .max_by_key(|(path, n)| (**n, std::cmp::Reverse((*path).clone())));
            match top {
                Some((path, n)) if n * 5 >= pool.len() * 4 => {
                    prefix.clone_from(path);
                    pool.retain(|f| under(f, &prefix));
                    note(&prefix, &mut candidates, &mut ticked);
                    pool.retain(|f| in_play(f, &ticked));
                    // A folder with no folder of code in it is a part itself, not a frontier with only loose files.
                    if children(&prefix, pool.iter().copied()).is_empty() {
                        break vec![prefix];
                    }
                }
                _ => break counts.into_keys().collect(),
            }
        }
    } else {
        let rest = pool
            .iter()
            .copied()
            .filter(|f| !seeds.iter().any(|s| under(f, s)));
        // A folder that holds a seed is left to its loose files, so no two parts overlap.
        let mut parts: Vec<String> = children("", rest)
            .into_keys()
            .filter(|folder| !seeds.iter().any(|s| under(s, folder)))
            .collect();
        parts.extend(seeds);
        parts.sort();
        parts
    };
    parts.sort();
    fold(&mut parts, &code);

    for folder in unbuilt_unimported(root, files, &code, &candidates) {
        candidates.insert(folder, false);
    }

    // Every tracked file no part owns and no ignored folder holds, code or not, hidden or not: the check asks a home for each,
    // the map file apart, which it ignores with no entry for it.
    let owned = |file: &str| file == crate::MAP_FILE || parts.iter().any(|p| under(file, p));
    let loose = |tests: bool| -> Vec<String> {
        files
            .iter()
            .filter(|f| is_test(f) == tests && !owned(f) && in_play(f, &ticked))
            .cloned()
            .collect()
    };
    let (loose_files, loose_tests) = (loose(false), loose(true));

    ticked.sort();
    let (extractors, languages) = readers(root, files, &code, &ticked);

    let map = draft_map(&extractors, &parts, &ticked);
    Scan {
        map,
        ignore_candidates: candidates
            .into_iter()
            .map(|(path, ticked)| IgnoreCandidate { path, ticked })
            .collect(),
        loose_files,
        loose_tests,
        read_languages: languages.read,
        unread_languages: languages.unread,
    }
}

/// The version 2 map for `parts`, each a folder, ignoring the `ignored` folders.
fn draft_map(extractors: &[&str], parts: &[String], ignored: &[String]) -> Value {
    let ids = ids_of(parts);
    json!({
        "version": 2,
        "extractors": extractors,
        "parts": parts
            .iter()
            .zip(&ids)
            .map(|(path, id)| placeholder_part(id, &folder_title(path), vec![path.clone()]))
            .collect::<Vec<_>>(),
        "actors": [],
        "links": [],
        "ignored": ignored,
        "laws": [],
    })
}

fn depth(path: &str) -> usize {
    path.trim_end_matches('/').split('/').count()
}

/// While there are more than [`MAX_PARTS`] parts, folds the deepest level's sibling folders into their parent folder's
/// part, the parent with the fewest code files first. Parts at the top level cannot fold further.
fn fold(parts: &mut Vec<String>, code: &[&str]) {
    while parts.len() > MAX_PARTS {
        let deepest = parts.iter().map(|p| depth(p)).max().unwrap_or(0);
        if deepest <= 1 {
            return;
        }
        let parents: BTreeSet<String> = parts
            .iter()
            .filter(|p| depth(p) == deepest)
            .filter_map(|p| p.trim_end_matches('/').rsplit_once('/'))
            .map(|(parent, _)| format!("{parent}/"))
            .collect();
        let Some(parent) = parents.into_iter().min_by_key(|parent| {
            (
                code.iter().filter(|f| under(f, parent)).count(),
                parent.clone(),
            )
        }) else {
            return;
        };
        parts.retain(|p| !under(p, &parent));
        parts.push(parent);
        parts.sort();
    }
}

/// A part as the scan drafts one, `id` and `name` owning `paths`: tier Interface, no held items, and the placeholder caption and
/// purpose the writers replace.
#[must_use]
pub fn placeholder_part(id: &str, name: &str, paths: Vec<String>) -> Value {
    json!({
        "id": id,
        "name": name,
        "caption": "Not yet described",
        "purpose": "Not yet described.",
        "holds": [],
        "tier": "interface",
        "paths": paths,
    })
}

/// The words a part id for the folder `path` is made of, one for each of its folders: the folder's name lowercased, each run of
/// characters other than letters and digits one `-`, or `part` when nothing is left. A scan's ids and a split's new ones join the
/// last of them.
#[must_use]
pub fn id_words(path: &str) -> Vec<String> {
    path.trim_end_matches('/')
        .split('/')
        .map(|segment| {
            let slug: String = segment
                .to_lowercase()
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            let slug = slug
                .split('-')
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join("-");
            if slug.is_empty() { "part".into() } else { slug }
        })
        .collect()
}

/// An id for each part: its last folder's name, with its parents' names added while two parts would share one.
fn ids_of(parts: &[String]) -> Vec<String> {
    let segments: Vec<Vec<String>> = parts.iter().map(|p| id_words(p)).collect();
    let id = |i: usize, take: usize| -> String {
        let s = &segments[i];
        s[s.len().saturating_sub(take)..].join("-")
    };
    let mut takes = vec![1usize; parts.len()];
    loop {
        let ids: Vec<String> = (0..parts.len()).map(|i| id(i, takes[i])).collect();
        let mut changed = false;
        for i in 0..parts.len() {
            let clash = ids
                .iter()
                .enumerate()
                .any(|(j, other)| j != i && *other == ids[i]);
            if clash && takes[i] < segments[i].len() {
                takes[i] += 1;
                changed = true;
            }
        }
        if !changed {
            let mut seen = BTreeSet::new();
            return ids
                .into_iter()
                .map(|id| {
                    let mut unique = id.clone();
                    let mut n = 2;
                    while !seen.insert(unique.clone()) {
                        unique = format!("{id}-{n}");
                        n += 1;
                    }
                    unique
                })
                .collect();
        }
    }
}

/// A part's name: its last folder's words, capitalised.
pub fn folder_title(path: &str) -> String {
    let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let words: Vec<String> = last
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        })
        .collect();
    if words.is_empty() {
        "Part".into()
    } else {
        words.join(" ")
    }
}
