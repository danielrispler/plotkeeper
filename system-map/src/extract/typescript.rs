//! The `typescript-imports` extractor: the import specifiers of script files, read from a token stream that leaves out
//! comments, strings and the text of template literals, resolved to tracked files: a relative specifier to the file it
//! names, a bare one to the `package.json` of the workspace package it names.

use std::collections::{BTreeMap, BTreeSet};

use super::{Extractor, Output, Pair, Repo};
use crate::pattern::Pattern;

pub(crate) struct TypescriptImports;

impl Extractor for TypescriptImports {
    fn name(&self) -> &'static str {
        "typescript-imports"
    }

    fn reads(&self, file: &str) -> bool {
        let base = file.rsplit('/').next().unwrap_or(file);
        base.rsplit_once('.')
            .is_some_and(|(_, ext)| SCRIPTS.contains(&ext))
    }

    /// A file named `*.test.*` or `*.spec.*`, or under a folder named `__tests__`, `test` or `tests` at any depth: the
    /// defaults of Jest, Vitest, Mocha and Node's test runner, and the folders a workspace package keeps its tests in.
    fn skips(&self, _repo: &Repo, file: &str) -> bool {
        let mut segments: Vec<&str> = file.split('/').collect();
        let base = segments.pop().unwrap_or(file);
        let names: Vec<&str> = base.split('.').collect();
        segments
            .iter()
            .any(|dir| matches!(*dir, "__tests__" | "test" | "tests"))
            || (names.len() > 2
                && names[1..names.len() - 1]
                    .iter()
                    .any(|name| matches!(*name, "test" | "spec")))
    }

    fn extract(&self, repo: &Repo) -> Output {
        let (pairs, problems) = typescript_imports(repo);
        Output {
            problems,
            pairs,
            ..Output::default()
        }
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Case, Fixture, MissingConfig};
        Fixture {
            support: vec![("package.json", "{ \"name\": \"kit\" }\n".into())],
            source: |name| format!("{name}.ts"),
            importing: |_, to| {
                format!(
                    "import {{ thing }} from \"./{}\";\n",
                    to.trim_end_matches(".ts")
                )
            },
            plain: |_| "export const thing = 1;\n".into(),
            missing_config: MissingConfig::Fallback {
                file: "package.json",
            },
            malformed_config: None,
            test_file: Some(|name| format!("{name}.test.ts")),
            skip: vec![(
                Case::MalformedConfig,
                "a package.json that does not parse declares no workspace packages, and only bare imports miss them",
            )],
        }
    }
}

/// The extensions of the script files `typescript-imports` reads.
const SCRIPTS: [&str; 7] = ["ts", "tsx", "js", "mjs", "jsx", "mts", "cjs"];

const TS_SUFFIXES: [&str; 7] = [".ts", ".tsx", ".js", ".mjs", ".jsx", ".mts", ".cjs"];

/// One token of script source that matters to the import scan.
#[derive(Debug, PartialEq)]
enum Tok {
    /// A word: an identifier, a keyword or a number.
    Ident(String),
    /// A string literal, or a template literal with no `${}`, holding its text.
    Str(String),
    Sym(char),
}

/// Whether a `/` after `tokens` starts a regular expression rather than a division.
fn regex_may_follow(tokens: &[Tok]) -> bool {
    match tokens.last() {
        None => true,
        // `i++ / 2`, `i-- / 2`.
        Some(Tok::Sym(c @ ('+' | '-')))
            if tokens.len() > 1 && tokens[tokens.len() - 2] == Tok::Sym(*c) =>
        {
            false
        }
        Some(Tok::Sym(c)) => !matches!(c, ')' | ']' | '}'),
        Some(Tok::Str(_)) => false,
        Some(Tok::Ident(word)) => matches!(
            word.as_str(),
            "return"
                | "typeof"
                | "case"
                | "do"
                | "else"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "instanceof"
                | "yield"
                | "await"
        ),
    }
}

/// The words, strings and punctuation of script source. Comments, regular expressions and the text of template literals
/// are left out; the code in a template's `${}` is read. Also the 1-based line each token starts on.
fn tokens(text: &str) -> (Vec<Tok>, Vec<u32>) {
    // A UTF-8 byte order mark, which some editors write, is not code.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    // Where each token starts: each turn of the loop below reads at most one token, which starts where the turn does.
    let mut starts = Vec::new();
    // For each template literal whose `${}` is being read, the brace depth its `${` opened at.
    let mut templates: Vec<usize> = Vec::new();
    let mut depth = 0;
    let mut i = 0;
    if text.starts_with("#!") {
        while i < chars.len() && chars[i] != '\n' {
            i += 1;
        }
    }
    while i < chars.len() {
        let at = i;
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            i += 2;
            while i < chars.len() && !(chars[i] == '*' && chars.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i += 2;
        } else if c == '/' && regex_may_follow(&tokens) {
            i = skip_regex(&chars, i + 1);
        } else if c == '"' || c == '\'' {
            let (end, body) = string(&chars, i + 1, c);
            tokens.push(Tok::Str(body));
            i = end;
        } else if c == '`' || (c == '}' && templates.last() == Some(&depth)) {
            if c == '}' {
                templates.pop();
            }
            let (end, body, substitution) = template(&chars, i + 1);
            i = end;
            if substitution {
                templates.push(depth);
            } else if c == '`' {
                tokens.push(Tok::Str(body));
            }
        } else if c.is_alphanumeric() || c == '_' || c == '$' {
            let start = i;
            while i < chars.len()
                && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '$')
            {
                i += 1;
            }
            tokens.push(Tok::Ident(chars[start..i].iter().collect()));
        } else {
            match c {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            if !c.is_whitespace() {
                tokens.push(Tok::Sym(c));
            }
            i += 1;
        }
        if starts.len() < tokens.len() {
            starts.push(at);
        }
    }
    let mut lines = Vec::with_capacity(starts.len());
    let (mut line, mut counted) = (1u32, 0usize);
    for start in starts {
        line += chars[counted..start].iter().filter(|c| **c == '\n').count() as u32;
        counted = start;
        lines.push(line);
    }
    (tokens, lines)
}

/// The index after a string whose body starts at `i` and closes with `quote`, and its text. A string ends at a line
/// break too, as an unescaped one cannot hold one, so an apostrophe in JSX text spoils one line at most.
fn string(chars: &[char], mut i: usize, quote: char) -> (usize, String) {
    let mut body = String::new();
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                body.extend(chars.get(i + 1));
                i += 2;
            }
            '\n' => return (i, body),
            c if c == quote => return (i + 1, body),
            c => {
                body.push(c);
                i += 1;
            }
        }
    }
    (i, body)
}

/// Reads template text from `i`: the index after it, its text, and whether it stopped at a `${` rather than the
/// closing backtick.
fn template(chars: &[char], mut i: usize) -> (usize, String, bool) {
    let mut body = String::new();
    while i < chars.len() {
        match chars[i] {
            '\\' => {
                body.extend(chars.get(i + 1));
                i += 2;
            }
            '`' => return (i + 1, body, false),
            '$' if chars.get(i + 1) == Some(&'{') => return (i + 2, body, true),
            c => {
                body.push(c);
                i += 1;
            }
        }
    }
    (i, body, false)
}

/// The index after a regular expression whose body starts at `i`, its flags read later as a word.
fn skip_regex(chars: &[char], mut i: usize) -> usize {
    let mut class = false;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '\n' => return i,
            '[' => class = true,
            ']' => class = false,
            '/' if !class => return i + 1,
            _ => {}
        }
        i += 1;
    }
    i
}

fn is_word(tok: Option<&Tok>, word: &str) -> bool {
    matches!(tok, Some(Tok::Ident(w)) if w == word)
}

/// What a binding of an import or re-export clause takes from the module it names: one export by name (`default` for a
/// default import), or all of them (`* as ns`, `export *`, a call to `import()` or `require()`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    Name(String),
    All,
}

/// An import or re-export clause: the specifier its `from "x"` names, each binding's origin and the name it takes in this
/// file (`*` for a bare `export *`), and the index after the clause.
struct Clause<'a> {
    spec: &'a str,
    names: Vec<(Origin, String)>,
    end: usize,
}

/// The specifiers a file's tokens import, each with what it takes and the line of the word that starts it (`import`,
/// `export`, `require`): `import … from "x"`, `import "x"` (nothing), `import("x")` and `require("x")` (all), `export … from
/// "x"` (`export *`, `export * as ns`, `export { … }`) and `export type … from "x"`.
fn specifiers<'a>(tokens: &'a [Tok], lines: &[u32]) -> Vec<(&'a str, Vec<Origin>, u32)> {
    let mut specs = Vec::new();
    for (at, tok) in tokens.iter().enumerate() {
        let Tok::Ident(word) = tok else { continue };
        if at > 0 && tokens[at - 1] == Tok::Sym('.') {
            continue;
        }
        let call = || match (tokens.get(at + 1), tokens.get(at + 2)) {
            (Some(Tok::Sym('(')), Some(Tok::Str(spec))) => Some((spec.as_str(), vec![Origin::All])),
            _ => None,
        };
        let clause = |i| {
            from_clause(tokens, i).map(|c| (c.spec, c.names.into_iter().map(|(o, _)| o).collect()))
        };
        let found = match word.as_str() {
            "require" => call(),
            "import" => match tokens.get(at + 1) {
                Some(Tok::Str(spec)) => Some((spec.as_str(), Vec::new())),
                Some(Tok::Sym('(')) => call(),
                _ => clause(at + 1),
            },
            "export" => {
                let mut i = at + 1;
                if is_word(tokens.get(i), "type") {
                    i += 1;
                }
                match tokens.get(i) {
                    Some(Tok::Sym('*' | '{')) => clause(i),
                    _ => None,
                }
            }
            _ => None,
        };
        specs.extend(found.map(|(spec, origins)| (spec, origins, lines[at])));
    }
    specs
}

/// The bindings of an import or re-export clause starting at `i` (names, `type`, `as`, `*`, commas and braces), and the
/// index of the first token after them.
fn bindings(tokens: &[Tok], mut i: usize) -> (Vec<(Origin, String)>, usize) {
    let mut names = Vec::new();
    let mut braced = false;
    loop {
        let alias = |at: usize| match (tokens.get(at), tokens.get(at + 1)) {
            (Some(Tok::Ident(word)), Some(Tok::Ident(name) | Tok::Str(name))) if word == "as" => {
                Some(name.clone())
            }
            _ => None,
        };
        match tokens.get(i) {
            Some(Tok::Ident(word))
                if word == "from" && matches!(tokens.get(i + 1), Some(Tok::Str(_))) =>
            {
                break;
            }
            Some(Tok::Ident(word))
                if word == "type"
                    && match tokens.get(i + 1) {
                        // `type` is a modifier unless it is the name itself: `import type from "x"`, `{ type as t }`.
                        Some(Tok::Ident(next)) => next != "from" && next != "as",
                        Some(Tok::Sym(c)) => matches!(c, '{' | '*'),
                        _ => false,
                    } =>
            {
                i += 1;
            }
            // A name written as a string, `{ "a-b" as c }`, is braced.
            Some(Tok::Str(word)) if braced => match alias(i + 1) {
                Some(name) => {
                    names.push((Origin::Name(word.clone()), name));
                    i += 3;
                }
                None => {
                    names.push((Origin::Name(word.clone()), word.clone()));
                    i += 1;
                }
            },
            Some(Tok::Ident(word)) => {
                let origin = Origin::Name(if braced {
                    word.clone()
                } else {
                    "default".to_string()
                });
                match alias(i + 1) {
                    Some(name) => {
                        names.push((origin, name));
                        i += 3;
                    }
                    None => {
                        names.push((origin, word.clone()));
                        i += 1;
                    }
                }
            }
            Some(Tok::Sym('*')) => match alias(i + 1) {
                Some(name) => {
                    names.push((Origin::All, name));
                    i += 3;
                }
                None => {
                    names.push((Origin::All, "*".to_string()));
                    i += 1;
                }
            },
            // A clause's braces come last, so its `}` ends it: without semicolons, the next statement follows.
            Some(Tok::Sym('}')) => {
                i += 1;
                break;
            }
            Some(Tok::Sym(c @ ('{' | ','))) => {
                braced |= *c == '{';
                i += 1;
            }
            _ => break,
        }
    }
    (names, i)
}

/// The import or re-export clause starting at `i` that ends with `from "x"`.
fn from_clause(tokens: &[Tok], i: usize) -> Option<Clause<'_>> {
    let (names, i) = bindings(tokens, i);
    match (tokens.get(i), tokens.get(i + 1)) {
        (Some(Tok::Ident(word)), Some(Tok::Str(spec))) if word == "from" => Some(Clause {
            spec,
            names,
            end: i + 2,
        }),
        _ => None,
    }
}

/// What a pure barrel exposes: each name it exports with the specifiers and origins it comes from, and the specifiers of
/// its `export *`.
#[derive(Default)]
struct Barrel<S> {
    named: BTreeMap<String, Vec<(S, Origin)>>,
    stars: Vec<S>,
}

/// The barrel a file's tokens make, if it is a pure one: its statements are only re-exports, the imports they use and
/// directives such as `"use client"`, and it exports something.
fn barrel(tokens: &[Tok]) -> Option<Barrel<&str>> {
    let mut found = Barrel::default();
    // The names the file imports, and the names it exports again without a `from`, each with the name it exports as.
    let mut imported: BTreeMap<String, (&str, Origin)> = BTreeMap::new();
    let mut local: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while let Some(tok) = tokens.get(i) {
        match tok {
            Tok::Sym(';') | Tok::Str(_) => i += 1,
            Tok::Ident(word) if word == "import" => {
                let clause = from_clause(tokens, i + 1)?;
                // `import {} from "x"` binds nothing, so no re-export uses it: it is there for its side effects.
                if clause.names.is_empty() {
                    return None;
                }
                for (origin, name) in clause.names {
                    imported.insert(name, (clause.spec, origin));
                }
                i = clause.end;
            }
            Tok::Ident(word) if word == "export" => {
                // Only `export *`, `export {` and `export type` can be a re-export; `export default A` is read below.
                let reexport = matches!(tokens.get(i + 1), Some(Tok::Sym('*' | '{')))
                    || is_word(tokens.get(i + 1), "type");
                if let Some(clause) = reexport.then(|| from_clause(tokens, i + 1)).flatten() {
                    for (origin, name) in clause.names {
                        if name == "*" {
                            found.stars.push(clause.spec);
                            continue;
                        }
                        found
                            .named
                            .entry(name)
                            .or_default()
                            .push((clause.spec, origin));
                    }
                    i = clause.end;
                } else if is_word(tokens.get(i + 1), "default") {
                    // `export default A`, an imported binding exported as the default, and nothing after it.
                    let Some(Tok::Ident(inner)) = tokens.get(i + 2) else {
                        return None;
                    };
                    match tokens.get(i + 3) {
                        None | Some(Tok::Sym(';')) => {}
                        Some(Tok::Ident(word)) if word == "export" || word == "import" => {}
                        _ => return None,
                    }
                    local.push((inner.clone(), "default".to_string()));
                    i += 3;
                } else {
                    let brace = if is_word(tokens.get(i + 1), "type") {
                        i + 2
                    } else {
                        i + 1
                    };
                    if tokens.get(brace) != Some(&Tok::Sym('{')) {
                        return None;
                    }
                    let (names, end) = bindings(tokens, i + 1);
                    for (origin, name) in names {
                        let Origin::Name(inner) = origin else {
                            return None;
                        };
                        local.push((inner, name));
                    }
                    i = end;
                }
            }
            _ => return None,
        }
    }
    let mut used = BTreeSet::new();
    for (inner, name) in local {
        let source = imported.get(&inner)?.clone();
        found.named.entry(name).or_default().push(source);
        used.insert(inner);
    }
    // An import no re-export uses is there for code of its own, or for its side effects.
    if imported.keys().any(|name| !used.contains(name)) {
        return None;
    }
    (!found.named.is_empty() || !found.stars.is_empty()).then_some(found)
}

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

/// The `package.json` of each package the workspace declares, by its name: a folder that matches a pattern of the root
/// `package.json`'s `workspaces` or of `pnpm-workspace.yaml`'s `packages` and holds a tracked `package.json` with a
/// `name`. A name two packages share names neither, and is a problem.
fn workspace_packages(repo: &Repo) -> (BTreeMap<String, String>, Vec<String>) {
    let tracked = repo.files();
    // A UTF-8 byte order mark, which some editors write, is not JSON.
    let read = |file: &str| {
        repo.read(file)
            .map(|text| text.trim_start_matches('\u{feff}').to_string())
    };
    let mut globs: Vec<String> = Vec::new();
    if let Some(json) =
        read("package.json").and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    {
        // npm and yarn write a list; yarn also `{ "packages": [...] }`.
        let list = json["workspaces"]
            .get("packages")
            .unwrap_or(&json["workspaces"]);
        globs.extend(
            list.as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| g.as_str().map(str::to_string)),
        );
    }
    if let Some(yaml) = read("pnpm-workspace.yaml") {
        globs.extend(pnpm_packages(&yaml));
    }
    let (exclude, include): (Vec<&str>, Vec<&str>) = globs
        .iter()
        .map(|g| g.trim_start_matches("./").trim_end_matches('/'))
        .partition(|g| g.starts_with('!'));
    let include: Vec<Pattern> = include.into_iter().map(Pattern::new).collect();
    let exclude: Vec<Pattern> = exclude
        .into_iter()
        .map(|g| Pattern::new(g[1..].trim_start_matches("./")))
        .collect();
    let mut folders: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for file in tracked {
        let Some(folder) = file.strip_suffix("/package.json") else {
            continue;
        };
        if !include.iter().any(|p| p.matches(folder)) || exclude.iter().any(|p| p.matches(folder)) {
            continue;
        }
        let name = read(file)
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|json| json["name"].as_str().map(str::to_string));
        if let Some(name) = name {
            folders.entry(name).or_default().push(folder);
        }
    }
    let mut packages = BTreeMap::new();
    let mut problems = Vec::new();
    for (name, found) in folders {
        if let [folder] = found[..] {
            packages.insert(name, format!("{folder}/package.json"));
        } else {
            problems.push(format!(
                "extractor \"typescript-imports\": the workspace declares package {name:?} in {}; npm rejects a name \
                 declared twice, so imports of it give no edge until one is renamed",
                found.iter().map(|f| format!("{f:?}")).collect::<Vec<_>>().join(" and ")
            ));
        }
    }
    (packages, problems)
}

/// The patterns of `pnpm-workspace.yaml`'s `packages` list, written one per `- ` line or on one line as
/// `packages: ['a/*', 'b']`.
fn pnpm_packages(yaml: &str) -> Vec<String> {
    let unquote = |item: &str| item.trim().trim_matches(['"', '\'']).to_string();
    let mut globs = Vec::new();
    let mut inside = false;
    for line in yaml.lines() {
        let text = line.split(" #").next().unwrap_or("").trim_end();
        if text.trim().is_empty() || text.trim_start().starts_with('#') {
            continue;
        }
        if !text.starts_with([' ', '-']) {
            inside = text.trim() == "packages:";
            let flow = text
                .strip_prefix("packages:")
                .map(str::trim)
                .and_then(|rest| rest.strip_prefix('[')?.strip_suffix(']'));
            if let Some(list) = flow {
                globs.extend(list.split(',').map(unquote).filter(|g| !g.is_empty()));
            }
        } else if inside && let Some(item) = text.trim_start().strip_prefix("- ") {
            globs.push(unquote(item));
        }
    }
    globs
}

/// The workspace package a bare specifier names: `name` or `name/sub`, split after the scope of `@scope/name/sub`.
fn package_name(spec: &str) -> Option<&str> {
    if spec.starts_with('.') || spec.starts_with('/') {
        return None;
    }
    let segments = if spec.starts_with('@') { 2 } else { 1 };
    let end = spec
        .match_indices('/')
        .nth(segments - 1)
        .map_or(spec.len(), |(at, _)| at);
    Some(&spec[..end])
}

/// The folder of the declared workspace package a file lies in, if any: the deepest one that holds it.
fn package_of<'a>(folders: &'a [String], file: &str) -> Option<&'a str> {
    folders
        .iter()
        .filter(|folder| file.starts_with(&format!("{folder}/")))
        .max_by_key(|folder| folder.len())
        .map(String::as_str)
}

/// The files a binding with `origin` reaches through the file `at`, which `from` imports: `at` itself unless it is a pure
/// barrel in `from`'s own workspace package (or outside every package, as `from` is), else the files the re-exports that
/// bring in what it takes reach in turn. `seen` holds the barrels and origins already followed, so a cycle of barrels
/// ends.
fn follow<'a>(
    barrels: &'a BTreeMap<String, Barrel<String>>,
    folders: &[String],
    from: &str,
    at: &'a str,
    origin: &Origin,
    seen: &mut BTreeSet<(&'a str, Origin)>,
    out: &mut BTreeSet<String>,
) {
    let barrel = barrels
        .get(at)
        .filter(|_| package_of(folders, at) == package_of(folders, from));
    let Some(barrel) = barrel else {
        out.insert(at.to_string());
        return;
    };
    if !seen.insert((at, origin.clone())) {
        return;
    }
    let mut next: Vec<(&'a String, Origin)> = Vec::new();
    match origin {
        Origin::Name(name) => match barrel.named.get(name) {
            Some(sources) => next.extend(sources.iter().map(|(target, o)| (target, o.clone()))),
            // `export *` never carries `default`.
            None if name == "default" => {}
            // An over-count: every `export *` target, as the name's real source is not read.
            None => next.extend(barrel.stars.iter().map(|target| (target, origin.clone()))),
        },
        Origin::All => {
            next.extend(
                barrel
                    .named
                    .values()
                    .flatten()
                    .map(|(target, o)| (target, o.clone())),
            );
            next.extend(barrel.stars.iter().map(|target| (target, Origin::All)));
        }
    }
    for (target, origin) in next {
        follow(barrels, folders, at, target, &origin, seen, out);
    }
}

/// Each script file paired with the tracked files its relative imports name and the `package.json` of each workspace
/// package it imports, and with the file behind each pure barrel it imports that exports what it takes; and the problems
/// of the workspace's packages.
fn typescript_imports(repo: &Repo) -> (Vec<Pair>, Vec<String>) {
    let tracked = repo.files();
    let (packages, problems) = workspace_packages(repo);
    let resolve = |file: &str, spec: &str| {
        resolve_import(file, spec, tracked).or_else(|| packages.get(package_name(spec)?).cloned())
    };
    let mut imports = Vec::new();
    let mut barrels: BTreeMap<String, Barrel<String>> = BTreeMap::new();
    for file in tracked {
        let script = file
            .rsplit_once('.')
            .is_some_and(|(_, ext)| SCRIPTS.contains(&ext));
        let Some(text) = script.then(|| repo.read(file)).flatten() else {
            continue;
        };
        let (tokens, lines) = tokens(&text);
        let targets: Vec<(String, Vec<Origin>, u32)> = specifiers(&tokens, &lines)
            .into_iter()
            .filter_map(|(spec, origins, line)| Some((resolve(file, spec)?, origins, line)))
            .collect();
        imports.push((file.to_string(), targets));
        if let Some(found) = barrel(&tokens) {
            let mut resolved = Barrel::default();
            for (name, sources) in found.named {
                let sources = sources
                    .into_iter()
                    .filter_map(|(spec, origin)| Some((resolve(file, spec)?, origin)))
                    .collect();
                resolved.named.insert(name, sources);
            }
            resolved.stars = found
                .stars
                .into_iter()
                .filter_map(|spec| resolve(file, spec))
                .collect();
            barrels.insert(file.to_string(), resolved);
        }
    }
    let folders: Vec<String> = packages
        .values()
        .filter_map(|manifest| manifest.strip_suffix("/package.json"))
        .map(str::to_string)
        .collect();
    let mut pairs = Vec::new();
    for (file, targets) in imports {
        // Each file an import reaches, with the lines of the imports that reach it: a followed barrel's target too.
        let mut ends: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        for (target, origins, line) in targets {
            let mut reached = BTreeSet::from([target.clone()]);
            for origin in &origins {
                let seen = &mut BTreeSet::new();
                follow(
                    &barrels,
                    &folders,
                    &file,
                    &target,
                    origin,
                    seen,
                    &mut reached,
                );
            }
            for end in reached {
                ends.entry(end).or_default().insert(line);
            }
        }
        ends.remove(&file);
        pairs.extend(ends.into_iter().map(|(end, lines)| Pair {
            lines,
            ..Pair::new(&file, &end)
        }));
    }
    (pairs, problems)
}
