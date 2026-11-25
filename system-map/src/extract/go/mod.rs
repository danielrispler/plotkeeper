//! The `go-imports` extractor: the imports of Go files, read as text with no `go` run (ADR 0002), resolved by Go's rule
//! for which module provides a package (go.dev/ref/mod, "Resolving a package to a module") against every tracked
//! `go.mod`, and paired with the package files of the folder they name (spec #569).

mod work;

use std::collections::{BTreeMap, BTreeSet};

use super::{
    Extractor, Output, Pair, Repo, Unsure,
    names::{Lookup, Names},
};

pub(crate) struct GoImports;

impl Extractor for GoImports {
    fn name(&self) -> &'static str {
        "go-imports"
    }

    /// A `.go` file with no path element, its name included, that Go leaves out of packages: one named `testdata` or
    /// `vendor`, or starting with `_` or `.`.
    fn reads(&self, file: &str) -> bool {
        file.ends_with(".go") && !excluded(file)
    }

    /// A test file, `*_test.go`, or one whose head says the build never compiles it (`skipped`). Only the head is
    /// parsed.
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        file.ends_with("_test.go")
            || repo
                .read(file)
                .is_some_and(|text| skipped(&head_only(&text)))
    }

    /// A tracked `go.mod` the module index keeps: outside the excluded folders, with a readable module line, even when the
    /// map ignores it, as `cargo`'s override does. So a repository with Go files and no such `go.mod` is never told to add
    /// an extractor that would report its missing config (spec #569, "Detection").
    fn detect(&self, repo: &Repo) -> bool {
        !Modules::read(repo).0.paths.is_empty()
    }

    fn extract(&self, repo: &Repo) -> Output {
        go_imports(repo)
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Fixture, MissingConfig};
        // A source is the one file of its own package's folder, so an import of that folder names exactly it.
        fn package(file: &str) -> &str {
            file.split('/').next().unwrap_or(file)
        }
        Fixture {
            support: vec![("go.mod", "module example.com/kit\n\ngo 1.22\n".into())],
            source: |name| format!("{name}/{name}.go"),
            importing: |from, to| {
                format!(
                    "package {}\n\nimport \"example.com/kit/{}\"\n",
                    package(from),
                    package(to)
                )
            },
            plain: |name| format!("package {}\n", package(name)),
            missing_config: MissingConfig::Problem {
                file: "go.mod",
                says: "go-imports found no go.mod",
            },
            malformed_config: Some("go 1.22\n"),
            test_file: Some(|name| format!("{name}/{name}_test.go")),
            skip: Vec::new(),
        }
    }
}

/// Whether a path has an element Go leaves out of packages and patterns: one named `testdata` or `vendor`, or starting
/// with `_` or `.`. Judged from the repository root.
fn excluded(path: &str) -> bool {
    path.split('/')
        .any(|e| e == "testdata" || e == "vendor" || e.starts_with(['_', '.']))
}

/// Whether a file's head leaves it out of every build: a `package documentation` file, or one whose build constraint is
/// exactly `ignore`, a `//go:build ignore` line or, with no `//go:build` line, a `// +build ignore` line.
fn skipped(head: &Head) -> bool {
    let go_build = constraints(head, "//go:build");
    let ignored = if go_build.is_empty() {
        constraints(head, "// +build").contains(&"ignore")
    } else {
        go_build.contains(&"ignore")
    };
    ignored || head.package.as_deref() == Some("documentation")
}

/// The expressions of the head's comments that start with `prefix`, such as `//go:build`.
fn constraints<'h>(head: &'h Head, prefix: &str) -> Vec<&'h str> {
    head.comments
        .iter()
        .filter_map(|c| c.strip_prefix(prefix))
        .filter(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
        .map(str::trim)
        .collect()
}

/// The folder holding `file`, as a prefix: empty for the root, else ending in `/`.
fn folder_of(file: &str) -> &str {
    file.rfind('/').map_or("", |at| &file[..=at])
}

/// The folder above `folder`, or none for the root.
fn parent(folder: &str) -> Option<&str> {
    if folder.is_empty() {
        return None;
    }
    Some(folder_of(&folder[..folder.len() - 1]))
}

/// The repository's modules: every folder holding a tracked `go.mod`, each one a module boundary, and the indexed ones'
/// paths.
struct Modules {
    /// The folders holding a tracked `go.mod`.
    boundaries: BTreeSet<String>,
    /// Each indexed module's path, with its root folder.
    names: Names<String>,
    /// Each indexed module's root folder, with its path.
    paths: BTreeMap<String, String>,
    /// Each path two or more indexed modules declare that `go.work` settles, with the winner's root.
    winners: BTreeMap<String, String>,
}

impl Modules {
    fn read(repo: &Repo) -> (Modules, Vec<String>) {
        let mut modules = Modules {
            boundaries: BTreeSet::new(),
            names: Names::new(),
            paths: BTreeMap::new(),
            winners: BTreeMap::new(),
        };
        let mut problems = Vec::new();
        // The `go.mod` files outside the excluded folders, readable or not.
        let mut indexed = 0;
        for file in repo.files() {
            if file.rsplit('/').next() != Some("go.mod") {
                continue;
            }
            let root = folder_of(file).to_string();
            modules.boundaries.insert(root.clone());
            if excluded(&root) {
                continue;
            }
            indexed += 1;
            let Some(text) = repo.read(file) else {
                continue;
            };
            match module_path(&text) {
                Ok(path) => {
                    modules.names.register(&path, root.clone());
                    modules.paths.insert(root, path);
                }
                Err(problem) => problems.push(format!("{file}: {problem}")),
            }
        }
        if indexed == 0 {
            problems.push(MISSING.to_string());
        }
        let (winners, malformed) = work::winners(repo, &modules);
        modules.winners = winners;
        problems.extend(malformed);
        (modules, problems)
    }

    /// The folder of the nearest tracked `go.mod` at or above `folder`.
    fn nearest<'f>(&self, mut folder: &'f str) -> Option<&'f str> {
        loop {
            if self.boundaries.contains(folder) {
                return Some(folder);
            }
            folder = parent(folder)?;
        }
    }

    /// The folders of the modules that provide the package `path` to a file of the module at `own`, and whether any of
    /// them is doubtful, its path declared twice (spec #569, "Resolution"). Own module first: an entry other than `own`
    /// that declares `own`'s path is dropped. A module provides the package when its folder for `path` lies in it, with
    /// no tracked `go.mod` in between, and directly holds a tracked `.go` file, whatever its name (Go's `fsys.IsGoDir`).
    fn providers(
        &self,
        own: &str,
        path: &str,
        go_files: &BTreeMap<&str, Vec<&str>>,
    ) -> (Vec<String>, bool) {
        let own_path = self.paths[own].as_str();
        let mut entries = self.entries(path);
        entries.retain(|&(module, root)| root == own || module != own_path);
        // A tie `go.work` settles keeps only its winner; own's path is settled already.
        entries.retain(|&(module, root)| {
            module == own_path || self.winners.get(module).is_none_or(|winner| winner == root)
        });
        let declared_twice =
            |module: &str| entries.iter().filter(|(m, _)| *m == module).count() > 1;
        let mut folders = Vec::new();
        let mut doubtful = false;
        for &(module, root) in &entries {
            let folder = match &path[module.len()..] {
                "" => root.to_string(),
                rest => format!("{root}{}/", &rest[1..]),
            };
            if self.nearest(&folder) == Some(root) && go_files.contains_key(folder.as_str()) {
                doubtful |= declared_twice(module);
                folders.push(folder);
            }
        }
        (folders, doubtful)
    }

    /// The indexed modules whose path is `path` or a prefix of it at a `/`, one entry per `go.mod`: each module's path
    /// and root.
    fn entries<'s>(&'s self, path: &'s str) -> Vec<(&'s str, &'s str)> {
        let mut entries = Vec::new();
        let ends = path
            .match_indices('/')
            .map(|(at, _)| at)
            .chain([path.len()]);
        for end in ends {
            let prefix = &path[..end];
            let roots = match self.names.lookup(prefix) {
                Lookup::Unknown => Vec::new(),
                Lookup::One(root) => vec![root],
                Lookup::Ambiguous(roots) => roots,
            };
            entries.extend(roots.into_iter().map(|root| (prefix, root.as_str())));
        }
        entries
    }
}

/// The problem of a repository with no `go.mod` to index.
const MISSING: &str = "go-imports found no go.mod outside testdata/, vendor/ and folders starting with _ or .; add one \
                       with a module line, or remove go-imports from extractors";

/// The module path a `go.mod` declares, or the problem of one that declares none or more than one: "A `go.mod` file
/// must contain exactly one `module` directive" (go.dev/ref/mod).
fn module_path(text: &str) -> Result<String, String> {
    let mut paths = Vec::new();
    let mut block: Option<&str> = None;
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (verb, rest) = match block {
            Some(verb) if line == ")" => {
                block = None;
                (verb, "")
            }
            Some(verb) => (verb, line),
            None => {
                let (verb, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
                let rest = rest.trim();
                match rest.strip_prefix('(') {
                    Some(inner) => match inner.strip_suffix(')') {
                        // The block on one line: `module ( example.com/m )`.
                        Some(inner) => (verb, inner.trim()),
                        None => {
                            block = Some(verb);
                            (verb, inner.trim())
                        }
                    },
                    None => (verb, rest),
                }
            }
        };
        if verb == "module" && !rest.is_empty() {
            paths.extend(unquoted(rest));
        }
    }
    if block.is_some() {
        return Err("a block opened with `(` is never closed with `)`".into());
    }
    match <[String; 1]>::try_from(paths) {
        Ok([path]) => Ok(path),
        Err(paths) if paths.is_empty() => Err(
            "no readable module line; a go.mod declares its module with one line `module <path>`"
                .into(),
        ),
        Err(_) => Err("more than one module line; a go.mod declares exactly one module".into()),
    }
}

/// A `go.mod` token's text: unquoted from `"…"` or `` `…` ``, or bare; none for one that holds a space or is empty.
fn unquoted(token: &str) -> Option<String> {
    let inner = ['"', '`']
        .iter()
        .find_map(|&q| token.strip_prefix(q)?.strip_suffix(q))
        .unwrap_or(token);
    (!inner.is_empty() && !inner.contains(char::is_whitespace) && !inner.contains('\\'))
        .then(|| inner.to_string())
}

/// Whether an import path may name a local package: not `"C"`, not relative or absolute (module mode rejects them), with
/// no `vendor` element and no space or backslash.
fn resolvable(path: &str) -> bool {
    path != "C"
        && !path.is_empty()
        && !path.starts_with(['.', '/'])
        && !path.contains([' ', '\\'])
        && !path.split('/').any(|element| element == "vendor")
}

/// What the reader takes from a Go file's head: the package clause's name and each import's path with its line.
#[derive(Debug, Default)]
struct Head {
    /// The `//` comments before the package clause, where build constraints stand.
    comments: Vec<String>,
    package: Option<String>,
    imports: Vec<(String, u32)>,
}

#[derive(Debug, PartialEq)]
enum Tok {
    Ident(String),
    /// A string literal's text, or none when it holds an escape.
    Str(Option<String>),
    Punct(u8),
    /// A `//` comment's text.
    Comment(String),
}

/// Go's tokens, as far as the reader needs them: identifiers, string literals, punctuation and line comments, with
/// whitespace and `/* */` comments skipped.
struct Lexer<'a> {
    text: &'a [u8],
    at: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Lexer<'a> {
        let text = text.as_bytes();
        let at = if text.starts_with(b"\xEF\xBB\xBF") {
            3
        } else {
            0
        };
        Lexer { text, at, line: 1 }
    }

    /// The bytes from `start` to the first `end` byte, the lexer moved past it; none at the end of the text.
    fn until(&mut self, start: usize, end: u8) -> Option<&'a [u8]> {
        let len = self.text[start..].iter().position(|&b| b == end)?;
        let found = &self.text[start..start + len];
        self.line += found.iter().filter(|&&b| b == b'\n').count() as u32;
        self.at = start + len + 1;
        Some(found)
    }

    /// The next token with its line, or none at the end of the text or at one it cannot read.
    fn next(&mut self) -> Option<(Tok, u32)> {
        loop {
            let &byte = self.text.get(self.at)?;
            let rest = &self.text[self.at..];
            if byte == b'\n' {
                self.line += 1;
                self.at += 1;
            } else if byte.is_ascii_whitespace() {
                self.at += 1;
            } else if rest.starts_with(b"/*") {
                let len = rest[2..].windows(2).position(|w| w == b"*/")?;
                self.line += rest[..len + 2].iter().filter(|&&b| b == b'\n').count() as u32;
                self.at += len + 4;
            } else {
                break;
            }
        }
        let line = self.line;
        let start = self.at;
        let byte = self.text[start];
        let tok = if self.text[start..].starts_with(b"//") {
            let end = self.text[start..]
                .iter()
                .position(|&b| b == b'\n')
                .map_or(self.text.len(), |len| start + len);
            self.at = end;
            Tok::Comment(String::from_utf8_lossy(&self.text[start..end]).into_owned())
        } else if byte == b'"' {
            let mut i = start + 1;
            let mut escaped = false;
            loop {
                match self.text.get(i)? {
                    b'"' => break,
                    b'\n' => return None,
                    b'\\' => {
                        escaped = true;
                        i += 2;
                        continue;
                    }
                    _ => {}
                }
                i += 1;
            }
            self.at = i + 1;
            let text = String::from_utf8_lossy(&self.text[start + 1..i]).into_owned();
            Tok::Str((!escaped).then_some(text))
        } else if byte == b'`' {
            let text = self.until(start + 1, b'`')?;
            Tok::Str(Some(String::from_utf8_lossy(text).into_owned()))
        } else if byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80 {
            let len = self.text[start..]
                .iter()
                .position(|&b| !(b == b'_' || b.is_ascii_alphanumeric() || b >= 0x80))
                .unwrap_or(self.text.len() - start);
            self.at = start + len;
            Tok::Ident(String::from_utf8_lossy(&self.text[start..start + len]).into_owned())
        } else {
            self.at += 1;
            Tok::Punct(byte)
        };
        Some((tok, line))
    }

    /// The next token that is not a comment or a `;`.
    fn next_code(&mut self) -> Option<(Tok, u32)> {
        loop {
            match self.next()? {
                (Tok::Comment(_) | Tok::Punct(b';'), _) => {}
                found => return Some(found),
            }
        }
    }
}

fn is_ident(tok: &Tok, word: &str) -> bool {
    matches!(tok, Tok::Ident(w) if w == word)
}

/// Reads a Go file's head up to the package clause, its imports left out.
fn head_only(text: &str) -> Head {
    let mut lexer = Lexer::new(text);
    package_clause(&mut lexer)
}

/// Reads the comments and the package clause, the lexer left after them.
fn package_clause(lexer: &mut Lexer) -> Head {
    let mut head = Head::default();
    loop {
        match lexer.next() {
            Some((Tok::Comment(c), _)) => head.comments.push(c),
            Some((tok, _)) if is_ident(&tok, "package") => break,
            _ => return head,
        }
    }
    if let Some((Tok::Ident(name), _)) = lexer.next_code() {
        head.package = Some(name);
    }
    head
}

/// Reads a Go file's head as `SourceFile = PackageClause { ImportDecl } …` has it: the package clause, then import
/// declarations, single or grouped, until the first other token.
fn head(text: &str) -> Head {
    let mut lexer = Lexer::new(text);
    let mut head = package_clause(&mut lexer);
    if head.package.is_none() {
        return head;
    }
    let mut tok = lexer.next_code();
    while tok.as_ref().is_some_and(|(t, _)| is_ident(t, "import")) {
        match lexer.next_code() {
            Some((Tok::Punct(b'('), _)) => loop {
                match lexer.next_code() {
                    Some((Tok::Punct(b')'), _)) => break,
                    Some(first) => {
                        if !spec(&mut lexer, first, &mut head.imports) {
                            return head;
                        }
                    }
                    None => return head,
                }
            },
            Some(first) => {
                if !spec(&mut lexer, first, &mut head.imports) {
                    return head;
                }
            }
            None => return head,
        }
        tok = lexer.next_code();
    }
    head
}

/// Reads one import spec, `[ "." | "_" | name ] path`, starting at `first`, and adds its path when it holds no escape.
/// False when it is no spec.
fn spec(lexer: &mut Lexer, first: (Tok, u32), imports: &mut Vec<(String, u32)>) -> bool {
    let (tok, line) = first;
    let path = match tok {
        Tok::Ident(_) | Tok::Punct(b'.') => match lexer.next_code() {
            Some((Tok::Str(path), _)) => path,
            _ => return false,
        },
        Tok::Str(path) => path,
        _ => return false,
    };
    imports.extend(path.map(|path| (path, line)));
    true
}

/// Each Go file paired with the package files of each local package it imports.
fn go_imports(repo: &Repo) -> Output {
    let (modules, problems) = Modules::read(repo);
    // Each folder's tracked `.go` files, and the head of each one that is a package file: read, never skipped. Each file
    // is read once.
    let mut go_files: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut heads: BTreeMap<&str, Head> = BTreeMap::new();
    for &file in repo.files() {
        if !file.ends_with(".go") {
            continue;
        }
        go_files.entry(folder_of(file)).or_default().push(file);
        if GoImports.reads(file) && !file.ends_with("_test.go") {
            let Some(text) = repo.read(file) else {
                continue;
            };
            let head = head(&text);
            if !skipped(&head) {
                heads.insert(file, head);
            }
        }
    }
    let package_files = |folder: &str| -> Vec<&str> {
        go_files
            .get(folder)
            .into_iter()
            .flatten()
            .copied()
            .filter(|f| heads.contains_key(f))
            .collect()
    };
    let mut found = Output {
        problems,
        ..Output::default()
    };
    for (&file, head) in &heads {
        // Only a file inside an indexed module can import a local package.
        let Some(own) = modules.nearest(folder_of(file)) else {
            continue;
        };
        if !modules.paths.contains_key(own) {
            continue;
        }
        for (path, line) in head.imports.iter().filter(|(p, _)| resolvable(p)) {
            let (folders, doubtful) = modules.providers(own, path, &go_files);
            let files: Vec<&str> = folders.iter().flat_map(|f| package_files(f)).collect();
            if files.is_empty() {
                // The standard library, a third-party module, a missing folder, or one with no package file.
                continue;
            }
            match folders.as_slice() {
                [folder] if !doubtful => {
                    found.pairs.extend(files.iter().map(|to| Pair {
                        group: Some(group(folder)),
                        from_group: Some(group(folder_of(file))),
                        ..Pair::at(file, to, *line)
                    }));
                }
                _ => found.unsure.push(Unsure {
                    file: file.to_string(),
                    name: path.clone(),
                    reason: if doubtful {
                        "declared twice"
                    } else {
                        "two modules provide it"
                    }
                    .into(),
                    candidates: files.iter().map(ToString::to_string).collect(),
                }),
            }
        }
    }
    found
}

/// The name a folder's package goes by as a pair's group: the folder, or `./` for the root.
fn group(folder: &str) -> String {
    if folder.is_empty() { "./" } else { folder }.to_string()
}
