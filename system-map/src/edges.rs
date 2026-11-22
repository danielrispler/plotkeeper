//! Computed edges: what the map's extractors read from the code, as part-to-part dependencies.

use serde::Serialize;

/// A dependency of the part `from` on the part `to`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
    process::Command,
};

use crate::{Link, PartFiles};

/// The extractors a map may name.
const KNOWN: [&str; 3] = ["cargo", "rust-modules", "typescript-imports"];

/// One question: which tracked file depends on which, as `(dependent, dependency)` pairs.
type Pairs = Vec<(String, String)>;

/// The edges between parts that the named extractors find and no link joins, and the problems of
/// extractors that are unknown or cannot run.
pub(crate) fn unlinked(
    root: &Path,
    names: &[String],
    parts: &[PartFiles],
    links: &[Link],
) -> (Vec<Edge>, Vec<String>) {
    let mut problems = Vec::new();
    let mut pairs = Pairs::new();
    let tracked: BTreeSet<&str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(String::as_str))
        .collect();
    for name in names {
        let found = match name.as_str() {
            "cargo" => cargo(root),
            "rust-modules" => Ok(rust_modules(root, &tracked)),
            "typescript-imports" => Ok(typescript_imports(root, &tracked)),
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
    (edges_between_parts(&pairs, parts, links), problems)
}

/// Turns pairs of files into edges between the parts that own them, dropping pairs inside one
/// part, pairs touching a file no part owns (ignored files too), and edges a link joins.
fn edges_between_parts(pairs: &Pairs, parts: &[PartFiles], links: &[Link]) -> Vec<Edge> {
    let owner: HashMap<&str, &str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(|f| (f.as_str(), part.id.as_str())))
        .collect();
    let joined: HashSet<(&str, &str)> = links
        .iter()
        .flat_map(|l| {
            [
                (l.from.as_str(), l.to.as_str()),
                (l.to.as_str(), l.from.as_str()),
            ]
        })
        .collect();
    let edges: BTreeSet<Edge> = pairs
        .iter()
        .filter_map(|(from, to)| Some((owner.get(from.as_str())?, owner.get(to.as_str())?)))
        .filter(|(from, to)| from != to && !joined.contains(&(**from, **to)))
        .map(|(from, to)| Edge {
            from: from.to_string(),
            to: to.to_string(),
        })
        .collect();
    edges.into_iter().collect()
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
        .map_err(|error| format!("extractor \"cargo\": cannot run cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "extractor \"cargo\": cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("extractor \"cargo\": unreadable cargo metadata: {error}"))?;
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let packages = metadata["packages"].as_array().map_or(&[][..], |p| p);
    let manifest = |package: &serde_json::Value| -> Option<String> {
        let path = Path::new(package["manifest_path"].as_str()?);
        let path = path.strip_prefix(&root).ok()?;
        Some(path.to_str()?.replace('\\', "/"))
    };
    let manifests: HashMap<&str, String> = packages
        .iter()
        .filter_map(|p| Some((p["name"].as_str()?, manifest(p)?)))
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
            if let Some(to) = dep["name"].as_str().and_then(|n| manifests.get(n)) {
                pairs.push((from.clone(), to.clone()));
            }
        }
    }
    Ok(pairs)
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
            let raw_string =
                matches!(word.as_str(), "r" | "br") && matches!(chars.get(i), Some('"' | '#'));
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
/// modules and `mod` lines open, and the `use crate::` paths it holds.
struct RustFile {
    /// `(module path from this file's module, whether the module is declared in a file of its own)`
    mods: Vec<(Vec<String>, bool)>,
    uses: Vec<Vec<String>>,
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
        uses: Vec::new(),
    };
    // Names of the inline modules the scan is inside, with the brace depth that closes each.
    let mut inline: Vec<(String, usize)> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    while i < tokens.len() {
        if sym(i, '{') {
            depth += 1;
        } else if sym(i, '}') {
            depth = depth.saturating_sub(1);
            while inline.last().is_some_and(|(_, d)| *d > depth) {
                inline.pop();
            }
        } else if ident(i) == Some("mod") && ident(i + 1).is_some() && !sym(i.wrapping_sub(1), ':')
        {
            let mut path: Vec<String> = inline.iter().map(|(n, _)| n.clone()).collect();
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
        } else if ident(i) == Some("use") && ident(i + 1) == Some("crate") && sym(i + 2, ':') {
            let mut end = i + 4;
            let mut paths = Vec::new();
            use_tree(&tokens, &mut end, Vec::new(), &mut paths);
            file.uses
                .extend(paths.into_iter().filter(|p| !p.is_empty()));
            i = end;
            continue;
        }
        i += 1;
    }
    file
}

/// Reads the use tree that starts at `*i` below `prefix`, pushing each full path. `self` names the
/// path before it and `as` renames, so neither adds a step.
fn use_tree(tokens: &[Tok], i: &mut usize, prefix: Vec<String>, out: &mut Vec<Vec<String>>) {
    let start = prefix.len();
    let mut path = prefix;
    while let Some(token) = tokens.get(*i) {
        match token {
            Tok::Ident(word) if word == "as" => *i += 2,
            Tok::Ident(word) => {
                if word != "self" {
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
                *i += 1;
                break;
            }
            _ => break,
        }
    }
    if path.len() > start {
        out.push(path);
    }
}

/// Inside each crate, the file a `mod name;` line names (as `name.rs` or `name/mod.rs`) and the
/// file a `use crate::` path reaches, paired with the file that holds the line. A crate's module
/// tree starts at a root (`lib.rs`, `main.rs`, `build.rs`, or a file of `bin`, `tests`, `examples`
/// or `benches`) that no `mod` line names.
fn rust_modules(root: &Path, tracked: &BTreeSet<&str>) -> Pairs {
    let parse: HashMap<&str, RustFile> = tracked
        .iter()
        .filter(|f| f.ends_with(".rs"))
        .filter_map(|f| Some((*f, scan_rust(&crate::read_tracked(root, f)?))))
        .collect();
    let mut pairs = Pairs::new();
    // Every file a `mod name;` line names, to tell crate roots from module files.
    let mut declared: HashSet<&str> = HashSet::new();
    let resolve = |from: &str, path: &[String]| -> Option<&str> {
        // The folder that holds the files of `from`'s submodules.
        let (dir, file) = from.rsplit_once('/').unwrap_or(("", from));
        let stem = file.trim_end_matches(".rs");
        let mut folder = if is_module_root(from) || stem == "mod" {
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
    for (file, scanned) in &parse {
        for (path, in_own_file) in &scanned.mods {
            if let (true, Some(target)) = (*in_own_file, resolve(file, path)) {
                declared.insert(target);
                pairs.push((file.to_string(), target.to_string()));
            }
        }
    }
    for root_file in parse
        .keys()
        .filter(|f| is_crate_root(f) && !declared.contains(*f))
    {
        let mut found: Vec<(Vec<String>, &str)> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        let mut queue = vec![(*root_file, Vec::<String>::new())];
        while let Some((file, prefix)) = queue.pop() {
            if !seen.insert(file) {
                continue;
            }
            found.push((prefix.clone(), file));
            for (path, in_own_file) in &parse[file].mods {
                let mut full = prefix.clone();
                full.extend(path.iter().cloned());
                if !*in_own_file {
                    found.push((full, file));
                } else if let Some(target) = resolve(file, path) {
                    queue.push((target, full));
                }
            }
        }
        for (_, file) in &found {
            for path in &parse[file].uses {
                // The longest prefix of the path that names a module.
                let target = (0..=path.len())
                    .rev()
                    .find_map(|n| found.iter().find(|(m, _)| m[..] == path[..n]))
                    .map(|(_, target)| *target);
                if let Some(target) = target.filter(|t| t != file) {
                    pairs.push((file.to_string(), target.to_string()));
                }
            }
        }
    }
    pairs
}

fn is_module_root(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    is_crate_root(file) || name == "mod.rs"
}

/// A file a crate's module tree starts from.
fn is_crate_root(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    let parent = file.rsplit('/').nth(1).unwrap_or("");
    matches!(name, "lib.rs" | "main.rs" | "build.rs")
        || matches!(parent, "bin" | "tests" | "examples" | "benches")
}

const TS_SUFFIXES: [&str; 7] = [".ts", ".tsx", ".js", ".mjs", ".jsx", ".mts", ".cjs"];

/// A tracked file a relative import specifier of `from` names, if one exists.
fn resolve_import(from: &str, spec: &str, tracked: &BTreeSet<&str>) -> Option<String> {
    if !spec.starts_with("./") && !spec.starts_with("../") {
        return None;
    }
    let mut segments: Vec<&str> = from.split('/').collect();
    segments.pop();
    for part in spec.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    let target = segments.join("/");
    let mut candidates = vec![target.clone()];
    for (written, real) in [
        (".js", ".ts"),
        (".js", ".tsx"),
        (".mjs", ".mts"),
        (".jsx", ".tsx"),
        (".cjs", ".cts"),
    ] {
        if let Some(stem) = target.strip_suffix(written) {
            candidates.push(format!("{stem}{real}"));
        }
    }
    candidates.extend(TS_SUFFIXES.iter().map(|s| format!("{target}{s}")));
    candidates.extend(TS_SUFFIXES.iter().map(|s| format!("{target}/index{s}")));
    candidates
        .into_iter()
        .find(|c| tracked.contains(c.as_str()))
}

/// The quoted specifiers that follow `from`, `import` and `import(` in the text of a file.
fn import_specifiers(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut specs = Vec::new();
    for keyword in ["from", "import", "require"] {
        for (at, _) in text.match_indices(keyword) {
            let before = text[..at].chars().next_back();
            let word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$' || c == '.';
            if before.is_some_and(word_char) {
                continue;
            }
            let mut i = at + keyword.len();
            if bytes
                .get(i)
                .is_some_and(|b| (*b as char).is_alphanumeric() || *b == b'_')
            {
                continue;
            }
            while bytes
                .get(i)
                .is_some_and(|b| b.is_ascii_whitespace() || *b == b'(')
            {
                i += 1;
            }
            let Some(&quote) = bytes.get(i).filter(|b| matches!(**b, b'\'' | b'"' | b'`')) else {
                continue;
            };
            let body = &text[i + 1..];
            if let Some(len) = body.find(quote as char) {
                specs.push(&body[..len]);
            }
        }
    }
    specs
}

/// Each script file paired with the tracked files its relative imports name.
fn typescript_imports(root: &Path, tracked: &BTreeSet<&str>) -> Pairs {
    const SCRIPTS: [&str; 7] = ["ts", "tsx", "js", "mjs", "jsx", "mts", "cjs"];
    let mut pairs = Pairs::new();
    for file in tracked {
        let script = file
            .rsplit_once('.')
            .is_some_and(|(_, ext)| SCRIPTS.contains(&ext));
        let Some(text) = script.then(|| crate::read_tracked(root, file)).flatten() else {
            continue;
        };
        for spec in import_specifiers(&text) {
            if let Some(target) = resolve_import(file, spec, tracked).filter(|t| t != file) {
                pairs.push((file.to_string(), target));
            }
        }
    }
    pairs
}
