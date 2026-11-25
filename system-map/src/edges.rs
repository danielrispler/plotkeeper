//! Computed edges: what the map's extractors read from the code, as part-to-part dependencies, and the link check that
//! holds the map's import links to them.

/// A dependency of the part `from` on the part `to`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Edge {
    pub from: String,
    pub to: String,
}

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
    process::Command,
};

use crate::{Kind, Link, PartFiles, typescript::SCRIPTS};

/// The extractors a map may name.
const KNOWN: [&str; 3] = ["cargo", "rust-modules", "typescript-imports"];

/// One question: which tracked file depends on which, as `(dependent, dependency)` pairs.
type Pairs = Vec<(String, String)>;

/// What the link check found: its problems, and how many import links a computed edge proves and how many have an end no
/// extractor reads.
pub(crate) struct LinkCheck {
    pub(crate) problems: Vec<String>,
    pub(crate) proven: usize,
    pub(crate) unproven: usize,
}

/// Holds the map's import links to the edges the named extractors compute (spec #473, "The link check"). Between two read
/// parts, an import link with no edge in its direction fails, and so does an edge with no import link in its direction,
/// whatever run-time links join the pair. An import link with an unread end passes as unproven, and so does one no edge
/// proves while an extractor is unknown or cannot run. Also the problems of those extractors.
pub(crate) fn check_links(
    root: &Path,
    names: &[String],
    files: &[String],
    parts: &[PartFiles],
    links: &[Link],
) -> LinkCheck {
    let (pairs, mut problems) = file_pairs(root, names, files, true);
    // An extractor that could not run leaves pairs unread, so a missing edge proves nothing.
    let incomplete = !problems.is_empty();
    let edges = edges_between_parts(&pairs, parts);
    let read: HashSet<&str> = parts
        .iter()
        .filter(|part| part.files.iter().any(|file| is_read(names, file)))
        .map(|part| part.id.as_str())
        .collect();
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
        problems,
        proven,
        unproven,
    }
}

/// Whether one of the named extractors reads `file`: Rust files for `rust-modules`, `Cargo.toml` files for `cargo` and
/// script files for `typescript-imports`.
fn is_read(names: &[String], file: &str) -> bool {
    let base = file.rsplit('/').next().unwrap_or(file);
    let ext = base.rsplit_once('.').map(|(_, ext)| ext);
    names.iter().any(|name| match name.as_str() {
        "cargo" => base == "Cargo.toml",
        "rust-modules" => ext == Some("rs"),
        "typescript-imports" => ext.is_some_and(|ext| SCRIPTS.contains(&ext)),
        _ => false,
    })
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

/// The `(dependent, dependency)` file pairs the named extractors find, each pair once, and the
/// problems of extractors that are unknown or cannot run. Without `root_mod_lines`, a crate root's
/// `mod` lines give no pair, as the imports report counts them: they declare the module tree.
pub(crate) fn file_pairs(
    root: &Path,
    names: &[String],
    files: &[String],
    root_mod_lines: bool,
) -> (BTreeSet<(String, String)>, Vec<String>) {
    let mut problems = Vec::new();
    let mut pairs = BTreeSet::new();
    // Every tracked file, ignored ones too: a module tree may run through a file no part owns, and
    // `edges_between_parts` drops the pairs that touch one.
    let tracked: BTreeSet<&str> = files.iter().map(String::as_str).collect();
    for name in names {
        let found = match name.as_str() {
            "cargo" => cargo(root),
            "rust-modules" => {
                let crates = workspace_crates(root).unwrap_or_else(|problem| {
                    problems.push(format!(
                        "extractor \"rust-modules\": workspace-crate paths were not read: {problem}"
                    ));
                    Vec::new()
                });
                Ok(rust_modules(root, &tracked, &crates, root_mod_lines))
            }
            "typescript-imports" => {
                let (found, found_problems) = crate::typescript::typescript_imports(root, &tracked);
                problems.extend(found_problems);
                Ok(found)
            }
            other => Err(format!(
                "unknown extractor {other:?} in extractors; use one of {}",
                KNOWN.join(", ")
            )),
        };
        match found {
            Ok(found) => pairs.extend(found),
            Err(problem) => problems.push(problem),
        }
    }
    (pairs, problems)
}

/// Turns pairs of files into edges between the parts that own them, dropping pairs inside one
/// part and pairs touching a file no part owns (ignored files too).
fn edges_between_parts(pairs: &BTreeSet<(String, String)>, parts: &[PartFiles]) -> Vec<Edge> {
    let owner: HashMap<&str, &str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(|f| (f.as_str(), part.id.as_str())))
        .collect();
    let edges: BTreeSet<Edge> = pairs
        .iter()
        .filter_map(|(from, to)| Some((owner.get(from.as_str())?, owner.get(to.as_str())?)))
        .filter(|(from, to)| from != to)
        .map(|(from, to)| Edge {
            from: from.to_string(),
            to: to.to_string(),
        })
        .collect();
    edges.into_iter().collect()
}

/// `cargo metadata` of the workspace at `root`, as JSON, and the root it reports paths against.
fn metadata(root: &Path) -> Result<(serde_json::Value, std::path::PathBuf), String> {
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("unreadable cargo metadata: {error}"))?;
    Ok((
        metadata,
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
    ))
}

/// `path` relative to `root`, with forward slashes.
fn relative_to(root: &Path, path: &str) -> Option<String> {
    let path = Path::new(path).strip_prefix(root).ok()?;
    Some(path.to_str()?.replace('\\', "/"))
}

/// Each workspace crate's manifest paired with the manifests of the workspace crates it depends
/// on, dev-dependencies left out.
fn cargo(root: &Path) -> Result<Pairs, String> {
    if !root.join("Cargo.toml").is_file() {
        return Err(
            "extractor \"cargo\" is named but the repository has no Cargo.toml; remove \"cargo\" from extractors"
                .into(),
        );
    }
    let (metadata, root) = metadata(root).map_err(|e| format!("extractor \"cargo\": {e}"))?;
    let packages = metadata["packages"].as_array().map_or(&[][..], |p| p);
    let manifest =
        |package: &serde_json::Value| relative_to(&root, package["manifest_path"].as_str()?);
    // A dependency on a workspace crate carries that crate's folder as its `path`; one on the
    // registry's crate of the same name does not.
    let manifests: HashMap<String, String> = packages
        .iter()
        .filter_map(|p| {
            let manifest = manifest(p)?;
            let dir = manifest.strip_suffix("Cargo.toml")?.trim_end_matches('/');
            Some((dir.to_string(), manifest))
        })
        .collect();
    let mut pairs = Pairs::new();
    for package in packages {
        let Some(from) = manifest(package) else {
            continue;
        };
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            if let Some(to) = dep["path"]
                .as_str()
                .and_then(|p| relative_to(&root, p))
                .and_then(|dir| manifests.get(&dir))
            {
                pairs.push((from.clone(), to.clone()));
            }
        }
    }
    Ok(pairs)
}

/// A workspace package, as the Rust reader needs it.
struct Crate {
    /// The package's folder, relative to the repository's root (`""` for the root).
    dir: String,
    /// Its library: the name code uses for it and its root file.
    lib: Option<(String, String)>,
    /// The folders of the workspace packages it depends on as normal dependencies, each with the
    /// name it renames that dependency to, if it does.
    deps: Vec<(String, Option<String>)>,
}

/// The workspace's packages from `cargo metadata`; none when the repository has no Cargo.toml.
fn workspace_crates(root: &Path) -> Result<Vec<Crate>, String> {
    if !root.join("Cargo.toml").is_file() {
        return Ok(Vec::new());
    }
    let (metadata, root) = metadata(root)?;
    let packages = metadata["packages"].as_array().map_or(&[][..], |p| p);
    let crates = packages
        .iter()
        .filter_map(|package| {
            let manifest = relative_to(&root, package["manifest_path"].as_str()?)?;
            let dir = manifest.strip_suffix("Cargo.toml")?.trim_end_matches('/');
            let lib = package["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .find_map(|t| {
                    let kinds = t["kind"].as_array()?;
                    let is_lib = kinds.iter().any(|k| {
                        matches!(
                            k.as_str(),
                            Some("lib" | "rlib" | "dylib" | "cdylib" | "staticlib" | "proc-macro")
                        )
                    });
                    is_lib.then_some(())?;
                    let name = t["name"].as_str()?.replace('-', "_");
                    Some((name, relative_to(&root, t["src_path"].as_str()?)?))
                });
            let deps = package["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|dep| dep["kind"].is_null())
                .filter_map(|dep| {
                    let dir = relative_to(&root, dep["path"].as_str()?)?;
                    let rename = dep["rename"].as_str().map(|r| r.replace('-', "_"));
                    Some((dir, rename))
                })
                .collect();
            Some(Crate {
                dir: dir.to_string(),
                lib,
                deps,
            })
        })
        .collect();
    Ok(crates)
}

/// The workspace libraries a crate whose root is `root_file` may name, by the name it uses, each
/// with its library's root file: its normal dependencies, and its own package's library when the
/// root is a binary, test or other target of that package.
fn extern_crates(root_file: &str, crates: &[Crate]) -> HashMap<String, String> {
    let inside = |dir: &str| dir.is_empty() || root_file.starts_with(&format!("{dir}/"));
    let Some(own) = crates
        .iter()
        .filter(|c| inside(&c.dir))
        .max_by_key(|c| c.dir.len())
    else {
        return HashMap::new();
    };
    let mut names = HashMap::new();
    if let Some((name, lib_root)) = own.lib.as_ref().filter(|(_, r)| r != root_file) {
        names.insert(name.clone(), lib_root.clone());
    }
    for (dir, rename) in &own.deps {
        let Some((name, lib_root)) = crates
            .iter()
            .find(|c| &c.dir == dir)
            .and_then(|c| c.lib.as_ref())
        else {
            continue;
        };
        names.insert(
            rename.clone().unwrap_or_else(|| name.clone()),
            lib_root.clone(),
        );
    }
    names
}

/// One token of Rust source that matters to the module scan.
#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    Sym(char),
}

/// The identifiers and the punctuation of Rust source, comments, strings and lifetimes left out.
fn rust_tokens(text: &str) -> Vec<Tok> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
        } else if c == '"' {
            i = skip_string(&chars, i + 1);
        } else if c == '\'' {
            // A character literal closes within a few characters; a lifetime does not.
            let close = if next == Some('\\') { i + 3 } else { i + 2 };
            if chars.get(close) == Some(&'\'') && next != Some('\'') {
                i = close + 1;
            } else if next == Some('\\') {
                i = skip_char_escape(&chars, i + 1);
            } else {
                i += 1;
            }
        } else if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let raw_string = matches!(word.as_str(), "r" | "br" | "cr")
                && matches!(chars.get(i), Some('"' | '#'));
            if raw_string {
                let mut hashes = 0;
                while chars.get(i) == Some(&'#') {
                    hashes += 1;
                    i += 1;
                }
                if chars.get(i) == Some(&'"') {
                    i = skip_raw_string(&chars, i + 1, hashes);
                }
            } else {
                tokens.push(Tok::Ident(word));
            }
        } else {
            if !c.is_whitespace() {
                tokens.push(Tok::Sym(c));
            }
            i += 1;
        }
    }
    tokens
}

/// The index after the closing quote of a string whose body starts at `i`.
fn skip_string(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 2,
            '"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn skip_raw_string(chars: &[char], mut i: usize, hashes: usize) -> usize {
    while i < chars.len() {
        if chars[i] == '"' && (0..hashes).all(|k| chars.get(i + 1 + k) == Some(&'#')) {
            return i + 1 + hashes;
        }
        i += 1;
    }
    i
}

/// The index after a character literal with an escape, whose backslash is at `i`.
fn skip_char_escape(chars: &[char], mut i: usize) -> usize {
    while i < chars.len() && chars[i] != '\'' {
        i += 1;
    }
    i + 1
}

/// What one Rust file declares: the files its `mod name;` lines name, the module paths its inline
/// modules and `mod` lines open, the paths it writes and the names its `use` lines bring in.
struct RustFile {
    /// `(module path from this file's module, whether the module is declared in a file of its own)`
    mods: Vec<(Vec<String>, bool)>,
    /// Each path in a `use` line, and each inline path of two words or more: `(the inline modules
    /// it sits in, its words)`.
    paths: Vec<(Vec<String>, Vec<String>)>,
    /// Each name a `use` line brings in: `(the inline modules the line sits in, the name, the path
    /// it names)`.
    aliases: Vec<(Vec<String>, String, Vec<String>)>,
}

fn scan_rust(text: &str) -> RustFile {
    let tokens = rust_tokens(text);
    let ident = |i: usize| match tokens.get(i) {
        Some(Tok::Ident(word)) => Some(word.as_str()),
        _ => None,
    };
    let sym = |i: usize, c: char| tokens.get(i) == Some(&Tok::Sym(c));
    let mut file = RustFile {
        mods: Vec::new(),
        paths: Vec::new(),
        aliases: Vec::new(),
    };
    // Names of the inline modules the scan is inside, with the brace depth that closes each.
    let mut inline: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < tokens.len() {
        let scope = || {
            inline
                .iter()
                .map(|(n, _)| n.clone())
                .collect::<Vec<String>>()
        };
        let after_colon = i > 0 && sym(i - 1, ':');
        // Inside a path whose start the scan did not read (after `>` in `Vec::<u8>::new`, or after a
        // word): a leading `::`, as in `::studio_server::x`, follows neither.
        let mid_path = i > 1
            && sym(i - 1, ':')
            && sym(i - 2, ':')
            && (i < 3 || ident(i - 3).is_some() || sym(i - 3, '>'));
        if sym(i, '{') {
            depth += 1;
        } else if sym(i, '}') {
            depth = depth.saturating_sub(1);
            while inline.last().is_some_and(|(_, d)| *d > depth) {
                inline.pop();
            }
        } else if ident(i) == Some("mod") && ident(i + 1).is_some() && !after_colon {
            let mut path = scope();
            let name = ident(i + 1).unwrap_or_default().to_string();
            if sym(i + 2, ';') {
                path.push(name);
                file.mods.push((path, true));
            } else if sym(i + 2, '{') {
                path.push(name.clone());
                file.mods.push((path, false));
                depth += 1;
                inline.push((name, depth));
                i += 2;
            }
        } else if ident(i) == Some("use")
            && (ident(i + 1).is_some() || (sym(i + 1, ':') && sym(i + 2, ':')))
        {
            let mut end = i + 1;
            let mut found = Vec::new();
            use_tree(&tokens, &mut end, Vec::new(), &mut found);
            for (path, alias) in found {
                if let Some(alias) = alias {
                    file.aliases.push((scope(), alias, path.clone()));
                }
                if !path.is_empty() {
                    file.paths.push((scope(), path));
                }
            }
            i = end;
            continue;
        } else if let Some(first) =
            ident(i).filter(|_| !mid_path && sym(i + 1, ':') && sym(i + 2, ':'))
        {
            // An inline path: words joined by `::`, read up to the first thing that is no word.
            let mut words = vec![first.to_string()];
            let mut end = i + 1;
            while sym(end, ':') && sym(end + 1, ':') {
                let Some(word) = ident(end + 2) else { break };
                words.push(word.to_string());
                end += 3;
            }
            if words.len() > 1 {
                file.paths.push((scope(), words));
            }
            i = end;
            continue;
        }
        i += 1;
    }
    file
}

/// Reads the use tree that starts at `*i` below `prefix`, pushing each full path with the name it
/// brings in (none for a glob or `as _`). `self` names the path before it and `as` renames, so
/// neither adds a step.
fn use_tree(
    tokens: &[Tok],
    i: &mut usize,
    prefix: Vec<String>,
    out: &mut Vec<(Vec<String>, Option<String>)>,
) {
    let start = prefix.len();
    let mut path = prefix;
    let mut names_self = false;
    let mut rename: Option<Option<String>> = None;
    let mut glob = false;
    while let Some(token) = tokens.get(*i) {
        match token {
            Tok::Ident(word) if word == "as" => {
                rename = Some(match tokens.get(*i + 1) {
                    Some(Tok::Ident(name)) if name != "_" => Some(name.clone()),
                    _ => None,
                });
                *i += 2;
            }
            Tok::Ident(word) => {
                if word == "self" {
                    names_self = true;
                } else {
                    path.push(word.clone());
                }
                *i += 1;
            }
            Tok::Sym(':') if tokens.get(*i + 1) == Some(&Tok::Sym(':')) => *i += 2,
            Tok::Sym('{') => {
                *i += 1;
                loop {
                    use_tree(tokens, i, path.clone(), out);
                    match tokens.get(*i) {
                        Some(Tok::Sym(',')) => *i += 1,
                        Some(Tok::Sym('}')) => {
                            *i += 1;
                            return;
                        }
                        _ => return,
                    }
                }
            }
            Tok::Sym('*') => {
                glob = true;
                *i += 1;
                break;
            }
            _ => break,
        }
    }
    if path.len() > start || names_self {
        let alias = match rename {
            Some(rename) => rename,
            None if glob => None,
            None => path.last().cloned(),
        };
        out.push((path, alias));
    }
}

/// Inside each crate, the file a `mod name;` line names (as `name.rs` or `name/mod.rs`) and the
/// file each path reaches, paired with the file that holds the line. A crate's module tree starts
/// at a root (`lib.rs`, `main.rs`, `build.rs`, or a file of `bin`, `tests`, `examples` or
/// `benches`) that no `mod` line names. A path reaches the file that defines the longest stretch
/// of it that names a module: from the crate's root after `crate::`, from a module the file (or an
/// inline module it sits in) declares, through a name a `use` line brought in, or, by a
/// workspace library's name in `crates`, from that library's root. Without `root_mod_lines`, a
/// crate root's `mod` lines pair nothing.
#[allow(
    clippy::too_many_lines,
    reason = "left whole when the lint came in (#555); spec #548 names no split of it"
)]
fn rust_modules(
    root: &Path,
    tracked: &BTreeSet<&str>,
    crates: &[Crate],
    root_mod_lines: bool,
) -> Pairs {
    let parse: HashMap<&str, RustFile> = tracked
        .iter()
        .filter(|f| f.ends_with(".rs"))
        .filter_map(|f| Some((*f, scan_rust(&crate::read_tracked(root, f)?))))
        .collect();
    let mut pairs = Pairs::new();
    // `declared` holds every file a `mod name;` line names, to tell crate roots from module files.
    let resolve = |declared: &HashSet<&str>, from: &str, path: &[String]| -> Option<&str> {
        // The folder that holds the files of `from`'s submodules.
        let (dir, file) = from.rsplit_once('/').unwrap_or(("", from));
        let stem = file.trim_end_matches(".rs");
        let mut folder = if is_module_root(from, declared) {
            dir.to_string()
        } else if dir.is_empty() {
            stem.to_string()
        } else {
            format!("{dir}/{stem}")
        };
        let (last, inline) = path.split_last()?;
        for name in inline {
            folder = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
        }
        let join = |tail: String| {
            if folder.is_empty() {
                tail
            } else {
                format!("{folder}/{tail}")
            }
        };
        [join(format!("{last}.rs")), join(format!("{last}/mod.rs"))]
            .iter()
            .find_map(|c| tracked.get(c.as_str()).copied())
    };
    // Which files are declared depends on where each declaring file looks, and that on which files
    // are declared: start with none and repeat until the set holds still.
    let mut declared: HashSet<&str> = HashSet::new();
    for _ in 0..=parse.len() {
        let mut next: HashSet<&str> = HashSet::new();
        for (file, scanned) in &parse {
            for (path, in_own_file) in &scanned.mods {
                if let (true, Some(target)) = (*in_own_file, resolve(&declared, file, path)) {
                    next.insert(target);
                }
            }
        }
        if next == declared {
            break;
        }
        declared = next;
    }
    for (file, scanned) in &parse {
        if !root_mod_lines && is_crate_root(file) && !declared.contains(file) {
            continue;
        }
        for (path, in_own_file) in &scanned.mods {
            if let (true, Some(target)) = (*in_own_file, resolve(&declared, file, path)) {
                pairs.push((file.to_string(), target.to_string()));
            }
        }
    }
    // Each crate's module tree: the file that defines each module path, and each file's own path.
    struct Tree<'a> {
        modules: HashMap<Vec<String>, &'a str>,
        files: Vec<(&'a str, Vec<String>)>,
    }
    let mut trees: HashMap<&str, Tree> = HashMap::new();
    for root_file in parse
        .keys()
        .filter(|f| is_crate_root(f) && !declared.contains(*f))
    {
        let mut tree = Tree {
            modules: HashMap::new(),
            files: Vec::new(),
        };
        let mut seen: HashSet<&str> = HashSet::new();
        let mut queue = vec![(*root_file, Vec::<String>::new())];
        while let Some((file, prefix)) = queue.pop() {
            if !seen.insert(file) {
                continue;
            }
            tree.modules.entry(prefix.clone()).or_insert(file);
            tree.files.push((file, prefix.clone()));
            for (path, in_own_file) in parse.get(file).map_or(&[][..], |f| &f.mods[..]) {
                let mut full = prefix.clone();
                full.extend(path.iter().cloned());
                if !*in_own_file {
                    tree.modules.entry(full).or_insert(file);
                } else if let Some(target) = resolve(&declared, file, path) {
                    queue.push((target, full));
                }
            }
        }
        trees.insert(*root_file, tree);
    }
    // The file that defines the longest stretch of `words`, at least `min` words, in `tree`.
    let longest = |tree: &Tree, words: &[String], min: usize| -> Option<String> {
        (min..=words.len())
            .rev()
            .find_map(|n| tree.modules.get(&words[..n]))
            .map(ToString::to_string)
    };
    for (root_file, tree) in &trees {
        let externs = extern_crates(root_file, crates);
        for (file, prefix) in &tree.files {
            let Some(scanned) = parse.get(file) else {
                continue;
            };
            // Where a path reaches from the crate's root, a declared module or a workspace library.
            let direct = |scope: &[String], words: &[String]| -> Option<String> {
                let (first, rest) = words.split_first()?;
                if first == "crate" {
                    return longest(tree, rest, 0);
                }
                // A module the file's module, or an inline module the path sits in, declares.
                for k in (0..=scope.len()).rev() {
                    let mut full = prefix.clone();
                    full.extend(scope[..k].iter().cloned());
                    let min = full.len() + 1;
                    full.extend(words.iter().cloned());
                    if tree.modules.contains_key(&full[..min]) {
                        return longest(tree, &full, min);
                    }
                }
                let lib = trees.get(externs.get(first)?.as_str())?;
                longest(lib, rest, 0)
            };
            // Else through the innermost `use` line that brought its first word in.
            let reach = |scope: &[String], words: &[String]| -> Option<String> {
                direct(scope, words).or_else(|| {
                    let (first, rest) = words.split_first()?;
                    let (_, _, path) = scanned
                        .aliases
                        .iter()
                        .filter(|(s, name, _)| name == first && scope.starts_with(s))
                        .max_by_key(|(s, _, _)| s.len())?;
                    let mut full = path.clone();
                    full.extend(rest.iter().cloned());
                    direct(scope, &full)
                })
            };
            for (scope, words) in &scanned.paths {
                if let Some(target) = reach(scope, words).filter(|t| t != file) {
                    pairs.push((file.to_string(), target));
                }
            }
        }
    }
    pairs
}

/// Whether the submodules of `file` sit in its own folder: a crate root, or a `mod.rs`.
fn is_module_root(file: &str, declared: &HashSet<&str>) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    (is_crate_root(file) && !declared.contains(file)) || name == "mod.rs"
}

/// A file a crate's module tree starts from.
fn is_crate_root(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    let parent = file.rsplit('/').nth(1).unwrap_or("");
    matches!(name, "lib.rs" | "main.rs" | "build.rs")
        || matches!(parent, "bin" | "tests" | "examples" | "benches")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file pairs `rust-modules` finds in a folder holding `files`, with no workspace.
    fn rust_pairs(files: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        let dir = tempfile::tempdir().unwrap();
        for (file, text) in files {
            let path = dir.path().join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let names = vec!["rust-modules".to_string()];
        let listed: Vec<String> = files.iter().map(|(f, _)| f.to_string()).collect();
        let (pairs, problems) = file_pairs(dir.path(), &names, &listed, true);
        assert_eq!(problems, Vec::<String>::new());
        pairs
    }

    #[test]
    fn a_use_group_naming_two_items_of_one_module_gives_one_pair() {
        let pairs = rust_pairs(&[
            (
                "k/src/lib.rs",
                "pub mod ci_laws;\nmod b;\nfn f() { ci_laws::CiCache::new(); }\n",
            ),
            ("k/src/ci_laws.rs", "pub struct CiCache;\n"),
            (
                "k/src/b.rs",
                "use crate::ci_laws::{A, B};\nfn g() { crate::ci_laws::A; }\n",
            ),
        ]);
        let from_b: Vec<_> = pairs.iter().filter(|(f, _)| f == "k/src/b.rs").collect();
        assert_eq!(from_b.len(), 1, "{pairs:?}");
        // The bare child-module path in the crate root reaches `ci_laws.rs`, once with its `mod` line.
        let from_lib: Vec<_> = pairs
            .iter()
            .filter(|(f, t)| f == "k/src/lib.rs" && t == "k/src/ci_laws.rs")
            .collect();
        assert_eq!(from_lib.len(), 1, "{pairs:?}");
    }
}
