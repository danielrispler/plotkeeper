//! The `java-imports` extractor: the imports of Java files, read as text with no `javac`, Maven or Gradle run (ADR
//! 0002), resolved against an index of the types the repository's production files declare, and paired with the files
//! that declare them (spec #590).

mod lexer;

use std::collections::{BTreeMap, BTreeSet};

use lexer::{Tok, tokens};

use super::{
    Extractor, Output, Pair, Repo, Unsure,
    names::{Lookup, Names},
};

pub(crate) struct JavaImports;

impl Extractor for JavaImports {
    fn name(&self) -> &'static str {
        "java-imports"
    }

    fn reads(&self, file: &str) -> bool {
        file.ends_with(".java")
    }

    /// A file outside production code: judged on its source root, from its path and its package line (`outside_production`).
    fn skips(&self, repo: &Repo, file: &str) -> bool {
        let Some(text) = repo.read(file) else {
            return outside_production(file, None);
        };
        // A file the reader cannot lex is left out whole (spec #590, "A file it cannot lex").
        Unit::read(&text).is_none_or(|unit| outside_production(file, unit.package.as_deref()))
    }

    fn extract(&self, repo: &Repo) -> Output {
        java_imports(repo)
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Case, Fixture, MissingConfig};
        // The package and the one type of a source file: `src/main/java/a/A.java` is `a` and `A`.
        fn names(file: &str) -> (String, String) {
            let path = file
                .trim_start_matches("src/main/java/")
                .trim_start_matches("src/test/java/");
            let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
            (
                folder.replace('/', "."),
                name.trim_end_matches(".java").to_string(),
            )
        }
        Fixture {
            support: Vec::new(),
            source: |name| format!("src/main/java/{name}/{}.java", name.to_uppercase()),
            importing: |from, to| {
                let ((from_package, from_type), (to_package, to_type)) = (names(from), names(to));
                format!(
                    "package {from_package};\n\nimport {to_package}.{to_type};\n\npublic class {from_type} {{}}\n"
                )
            },
            plain: |name| {
                let (package, ty) = names(name);
                format!("package {package};\n\npublic class {ty} {{}}\n")
            },
            missing_config: MissingConfig::None,
            malformed_config: None,
            test_file: Some(|name| format!("src/test/java/{name}/{}.java", name.to_uppercase())),
            skip: vec![(Case::MalformedConfig, "java-imports has no config file")],
        }
    }
}

/// Whether `set`, the folder after a `src` folder, holds production code: `main`, `java`, or `java` and digits (a
/// multi-release root, `java24`).
fn production_set(set: &str) -> bool {
    set == "main"
        || set
            .strip_prefix("java")
            .is_some_and(|n| n.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether a file is outside production code (spec #590, "The source root and `skips`"). Its source root is its folder
/// less a trailing package path; when the folder does not end with the package path, or the file has no package line, the
/// whole folder stands in for it. The root is outside production when it holds a `src` folder followed by a set that is
/// no production set (`src/test`, `src/jmh`), or, with no such `src/<set>` shape, a folder named `test` or `tests`.
fn outside_production(file: &str, package: Option<&str>) -> bool {
    let folder: Vec<&str> = match file.rsplit_once('/') {
        Some((folder, _)) => folder.split('/').collect(),
        None => Vec::new(),
    };
    let package: Vec<&str> = package.map_or_else(Vec::new, |p| p.split('.').collect());
    let root = match folder.len().checked_sub(package.len()) {
        Some(at) if !package.is_empty() && folder[at..] == package[..] => &folder[..at],
        _ => &folder[..],
    };
    let mut shaped = false;
    for pair in root.windows(2) {
        if pair[0] == "src" {
            shaped = true;
            if !production_set(pair[1]) {
                return true;
            }
        }
    }
    !shaped && root.iter().any(|&f| f == "test" || f == "tests")
}

/// One `import` declaration.
#[derive(Debug)]
struct Import {
    is_static: bool,
    /// The dotted name's segments, a `.*` left out.
    name: Vec<String>,
    on_demand: bool,
    /// The raw line of the `import` keyword.
    line: u32,
}

/// What the reader takes from a Java file: its package line, its imports and the top-level types it declares.
#[derive(Debug, Default)]
struct Unit {
    package: Option<String>,
    imports: Vec<Import>,
    /// The modules its `import module M;` declarations name.
    module_imports: Vec<String>,
    /// The name of the module a `module-info.java` declares.
    module: Option<String>,
    types: Vec<String>,
}

type Tokens = [(Tok, u32)];

fn is_ident(tokens: &Tokens, at: usize, word: &str) -> bool {
    matches!(tokens.get(at), Some((Tok::Ident(w), _)) if w == word)
}

fn is_punct(tokens: &Tokens, at: usize, byte: u8) -> bool {
    matches!(tokens.get(at), Some((Tok::Punct(b), _)) if *b == byte)
}

impl Unit {
    /// The unit of `text`, or none when the reader cannot lex it.
    fn read(text: &str) -> Option<Unit> {
        let tokens = tokens(text)?;
        let mut unit = Unit::default();
        unit.read_header(&tokens);
        unit.types = declarations(&tokens);
        unit.module = module_declared(&tokens);
        Some(unit)
    }

    /// The header as JLS §7.3 has it: leading annotations, an optional `package` line, then `import` declarations and
    /// stray `;` until the first other token.
    fn read_header(&mut self, tokens: &Tokens) {
        let mut at = 0;
        while is_punct(tokens, at, b'@') && !is_ident(tokens, at + 1, "interface") {
            at = after_annotation(tokens, at);
        }
        if is_ident(tokens, at, "package") {
            let (name, next, _) = dotted(tokens, at + 1);
            if is_punct(tokens, next, b';') && !name.is_empty() {
                self.package = Some(name.join("."));
                at = next + 1;
            } else {
                return;
            }
        } else {
            at = 0;
        }
        loop {
            if is_punct(tokens, at, b';') {
                at += 1;
            } else if is_ident(tokens, at, "import") {
                match self.read_import(tokens, at) {
                    Some(next) => at = next,
                    None => return,
                }
            } else {
                return;
            }
        }
    }

    /// Reads the import declaration at `at`, `import` and its name, and gives the token after its `;`; none when the
    /// header ends in it. A declaration that is no name is passed over: `import a.;`, and `import module M;`, a module
    /// import, whose `module` and `M` are two names in a row.
    fn read_import(&mut self, tokens: &Tokens, at: usize) -> Option<usize> {
        let line = tokens[at].1;
        let mut next = at + 1;
        let is_static = is_ident(tokens, next, "static");
        if is_static {
            next += 1;
        }
        // `module` and then an identifier is a module import; `module` and then `.` is a package segment (JLS §3.9).
        let module_import = !is_static
            && is_ident(tokens, next, "module")
            && matches!(tokens.get(next + 1), Some((Tok::Ident(_), _)));
        if module_import {
            let (name, end, on_demand) = dotted(tokens, next + 1);
            if is_punct(tokens, end, b';') && !on_demand {
                self.module_imports.push(name.join("."));
                return Some(end + 1);
            }
        }
        let (name, end, on_demand) = dotted(tokens, next);
        if is_punct(tokens, end, b';') {
            if !name.is_empty() {
                self.imports.push(Import {
                    is_static,
                    name,
                    on_demand,
                    line,
                });
            }
            return Some(end + 1);
        }
        // Not a name: pass over it to its `;`.
        let semicolon = (next..tokens.len())
            .take_while(|&i| !is_punct(tokens, i, b'{'))
            .find(|&i| is_punct(tokens, i, b';'))?;
        Some(semicolon + 1)
    }
}

/// The token after the annotation at `at`, a `@`: its name, and its parenthesised arguments if it has any.
fn after_annotation(tokens: &Tokens, at: usize) -> usize {
    let (_, mut next, _) = dotted(tokens, at + 1);
    if !is_punct(tokens, next, b'(') {
        return next.max(at + 1);
    }
    let mut depth = 0;
    while let Some((tok, _)) = tokens.get(next) {
        match tok {
            Tok::Punct(b'(') => depth += 1,
            Tok::Punct(b')') => depth -= 1,
            _ => {}
        }
        next += 1;
        if depth == 0 {
            break;
        }
    }
    next
}

/// A dotted name read from `at`, `Ident { . Ident }` with an optional `.*` after it: its segments, the token after it,
/// and whether `.*` ended it.
fn dotted(tokens: &Tokens, mut at: usize) -> (Vec<String>, usize, bool) {
    let mut name = Vec::new();
    while let Some((Tok::Ident(segment), _)) = tokens.get(at) {
        name.push(segment.clone());
        at += 1;
        if !is_punct(tokens, at, b'.') {
            break;
        }
        at += 1;
        if is_punct(tokens, at, b'*') {
            return (name, at + 1, true);
        }
    }
    (name, at, false)
}

/// The names of the types declared at brace depth 0 (JLS §7.6): `class`, `interface`, `enum` and `record`, each when an
/// identifier follows it, so `Suite.class` in an annotation and a field named `record` declare nothing. `@interface`
/// is its two tokens, `@` and `interface`.
fn declarations(tokens: &Tokens) -> Vec<String> {
    let mut types = Vec::new();
    let mut depth = 0usize;
    for (at, (tok, _)) in tokens.iter().enumerate() {
        match tok {
            Tok::Punct(b'{') => depth += 1,
            Tok::Punct(b'}') => depth = depth.saturating_sub(1),
            Tok::Ident(word)
                if depth == 0
                    && matches!(word.as_str(), "class" | "interface" | "enum" | "record") =>
            {
                if let Some((Tok::Ident(name), _)) = tokens.get(at + 1) {
                    types.push(name.clone());
                }
            }
            _ => {}
        }
    }
    types
}

/// The name a module declaration gives, `[open] module M {` at brace depth 0.
fn module_declared(tokens: &Tokens) -> Option<String> {
    let mut depth = 0usize;
    for (at, (tok, _)) in tokens.iter().enumerate() {
        match tok {
            Tok::Punct(b'{') => depth += 1,
            Tok::Punct(b'}') => depth = depth.saturating_sub(1),
            Tok::Ident(word) if depth == 0 && word == "module" => {
                let (name, end, _) = dotted(tokens, at + 1);
                if !name.is_empty() && is_punct(tokens, end, b'{') {
                    return Some(name.join("."));
                }
            }
            _ => {}
        }
    }
    None
}

/// The types the repository's production files declare, by `package.Name`, and each package with its type names.
struct Index {
    names: Names<String>,
    packages: BTreeMap<String, BTreeSet<String>>,
    /// The files that declare a type.
    files: BTreeSet<String>,
}

/// What a name stands for: its `package.Type` name, and the files that declare it.
type Found<'i> = (String, Lookup<'i, String>);

impl Index {
    /// The index of `units`, each file with its unit: every top-level type of a file with a package line, but none of
    /// `package-info.java` or `module-info.java`.
    fn new(units: &BTreeMap<&str, Unit>) -> Index {
        let mut index = Index {
            names: Names::new(),
            packages: BTreeMap::new(),
            files: BTreeSet::new(),
        };
        for (&file, unit) in units {
            let name = file.rsplit('/').next().unwrap_or(file);
            let Some(package) = &unit.package else {
                continue;
            };
            if name == "package-info.java" || name == "module-info.java" {
                continue;
            }
            for ty in &unit.types {
                index.files.insert(file.to_string());
                index
                    .names
                    .register(&format!("{package}.{ty}"), file.to_string());
                index
                    .packages
                    .entry(package.clone())
                    .or_default()
                    .insert(ty.clone());
            }
        }
        index
    }

    /// The top-level type a dotted name starts with, as javac walks it (JLS §6.5.4.2): the first prefix that is a
    /// package and a type's name after it. The segments after the type are member names, so a nested type stands for
    /// its outermost type.
    fn type_of(&self, name: &[String]) -> Option<Found<'_>> {
        (2..=name.len()).find_map(|end| {
            let prefix = name[..end].join(".");
            match self.names.lookup(&prefix) {
                Lookup::Unknown => None,
                found => Some((prefix, found)),
            }
        })
    }
}

/// Each Java file paired with the files that declare the types it imports.
fn java_imports(repo: &Repo) -> Output {
    // Each file the map reads, with its unit: tracked, not ignored, not outside production code. Each is read once.
    let mut units: BTreeMap<&str, Unit> = BTreeMap::new();
    for &file in repo.files() {
        if !JavaImports.reads(file) || repo.is_ignored(file) {
            continue;
        }
        let Some(text) = repo.read(file) else {
            continue;
        };
        let Some(unit) = Unit::read(&text) else {
            continue;
        };
        if !outside_production(file, unit.package.as_deref()) {
            units.insert(file, unit);
        }
    }
    let index = Index::new(&units);
    // Each module a `module-info.java` declares, with its roots: the folders of its declarers.
    let mut modules: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (&file, unit) in &units {
        if let (Some(module), Some(root)) = (&unit.module, file.strip_suffix("module-info.java")) {
            modules
                .entry(module)
                .or_default()
                .push(root.trim_end_matches('/'));
        }
    }
    let mut found = Output::default();
    for (&file, unit) in &units {
        for module in &unit.module_imports {
            let candidates: BTreeSet<String> = index
                .files
                .iter()
                .filter(|f| {
                    modules.get(module.as_str()).is_some_and(|roots| {
                        roots.iter().any(|r| {
                            r.is_empty()
                                || f.strip_prefix(r).is_some_and(|rest| rest.starts_with('/'))
                        })
                    })
                })
                .cloned()
                .collect();
            if !candidates.is_empty() {
                found.unsure.push(Unsure {
                    file: file.to_string(),
                    name: module.clone(),
                    reason: "module import".into(),
                    candidates: candidates.into_iter().collect(),
                });
            }
        }
        for import in &unit.imports {
            let line = import.line;
            let mut hit = |(name, lookup): Found| match lookup {
                Lookup::One(to) => found.pairs.push(Pair::at(file, to, line)),
                Lookup::Ambiguous(targets) => found.unsure.push(Unsure {
                    file: file.to_string(),
                    name,
                    reason: "declared twice".into(),
                    candidates: targets.into_iter().cloned().collect(),
                }),
                Lookup::Unknown => {}
            };
            let name = &import.name;
            if import.on_demand {
                // `a.b.C.*` of a type, or `import static a.b.C.*`; else `a.b.*` of a package.
                if let Some(of_type) = index.type_of(name) {
                    hit(of_type);
                    continue;
                }
                let package = name.join(".");
                if import.is_static
                    || package == "java.lang"
                    || unit.package.as_deref() == Some(&package)
                {
                    continue;
                }
                for ty in index.packages.get(&package).into_iter().flatten() {
                    let name = format!("{package}.{ty}");
                    let lookup = index.names.lookup(&name);
                    hit((name, lookup));
                }
            } else {
                // `import static a.b.C.x;` names the type before its member.
                let walked = if import.is_static {
                    &name[..name.len() - 1]
                } else {
                    name
                };
                if let Some(of_type) = index.type_of(walked) {
                    hit(of_type);
                }
            }
        }
    }
    found
}
