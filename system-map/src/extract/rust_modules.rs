//! The `rust-modules` extractor: the files a Rust file's `mod` lines and paths reach inside its crate and in the
//! workspace's libraries, read from a token stream that leaves out comments, strings and lifetimes. The workspace's
//! crates come from `cargo metadata` (`docs/adr/0002-extractors-read-text-only.md`).

use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use super::{
    Extractor, Output, Pair, Repo,
    cargo::{metadata, relative_to},
};

pub(crate) struct RustModules;

impl Extractor for RustModules {
    fn name(&self) -> &'static str {
        "rust-modules"
    }

    fn reads(&self, file: &str) -> bool {
        let base = file.rsplit('/').next().unwrap_or(file);
        base.rsplit_once('.').map(|(_, ext)| ext) == Some("rs")
    }

    fn extract(&self, repo: &Repo) -> Output {
        let mut problems = Vec::new();
        let crates = workspace_crates(repo.root()).unwrap_or_else(|problem| {
            problems.push(format!(
                "extractor \"rust-modules\": workspace-crate paths were not read: {problem}"
            ));
            Vec::new()
        });
        Output {
            problems,
            pairs: rust_modules(repo, &crates),
        }
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Case, Fixture, MissingConfig};
        Fixture {
            support: vec![
                (
                    "Cargo.toml",
                    "[package]\nname = \"kit\"\nversion = \"0.1.0\"\nedition = \"2021\"\n".into(),
                ),
                ("src/lib.rs", "mod a;\nmod b;\nmod c;\n".into()),
            ],
            source: |name| format!("src/{name}.rs"),
            importing: |_, to| {
                let module = to.trim_start_matches("src/").trim_end_matches(".rs");
                format!("use crate::{module}::Thing;\n")
            },
            plain: |_| "pub struct Thing;\n".into(),
            missing_config: MissingConfig::Fallback { file: "Cargo.toml" },
            malformed_config: Some("[package\n"),
            test_file: None,
            skip: vec![(
                Case::TestFile,
                "Rust reads its tests: files under tests/ and #[cfg(test)] code make pairs (map.md)",
            )],
        }
    }
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
/// workspace library's name in `crates`, from that library's root. A crate root's `mod` lines
/// give declaration pairs: they declare the module tree.
#[allow(
    clippy::too_many_lines,
    reason = "left whole when the lint came in (#555); spec #548 names no split of it"
)]
fn rust_modules(repo: &Repo, crates: &[Crate]) -> Vec<Pair> {
    let tracked = repo.files();
    let parse: HashMap<&str, RustFile> = tracked
        .iter()
        .filter(|f| f.ends_with(".rs"))
        .filter_map(|f| Some((*f, scan_rust(&repo.read(f)?))))
        .collect();
    let mut pairs = Vec::new();
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
        let declaration = is_crate_root(file) && !declared.contains(file);
        for (path, in_own_file) in &scanned.mods {
            if let (true, Some(target)) = (*in_own_file, resolve(&declared, file, path)) {
                pairs.push(Pair {
                    declaration,
                    ..Pair::new(file, target)
                });
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
                    pairs.push(Pair::new(file, &target));
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
    use std::collections::BTreeSet;

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
        let repo = Repo::new(dir.path(), &listed, |_| false);
        let found = super::super::run(&[&RustModules], &names, &repo);
        assert_eq!(found.problems, Vec::<String>::new());
        found.pairs.into_iter().map(|p| (p.from, p.to)).collect()
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
