//! `go.work` files, read only to break a tie between modules that declare one path (spec #569, "The module index";
//! `docs/adr/0003-go-imports-indexes-every-module.md`).

use std::collections::{BTreeMap, BTreeSet};

use super::{Modules, Repo, excluded, folder_of, group, unquoted};

/// Each path two or more indexed modules declare that the tracked, non-excluded `go.work` files settle: exactly one
/// declarer used, and no other `go.work` picking a different one. With it, the problems of the malformed ones.
pub(super) fn winners(repo: &Repo, modules: &Modules) -> (BTreeMap<String, String>, Vec<String>) {
    let mut picks: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut problems = Vec::new();
    for file in repo.files() {
        if file.rsplit('/').next() != Some("go.work") || excluded(file) {
            continue;
        }
        let Some(text) = repo.read(file) else {
            continue;
        };
        let uses = match uses(&text) {
            Ok(uses) => uses,
            Err((line, problem)) => {
                problems.push(format!("{file}:{line}: {problem}"));
                continue;
            }
        };
        // Each used module's path, with its root. A `go.work` that uses both declarers of a path is malformed.
        let mut used: BTreeMap<&str, &str> = BTreeMap::new();
        let mut malformed = false;
        for (path, line) in &uses {
            let Some(folder) = folder(folder_of(file), path) else {
                continue;
            };
            let Some((root, module)) = modules.paths.get_key_value(&folder) else {
                continue;
            };
            match used.get(module.as_str()) {
                None => {
                    used.insert(module, root);
                }
                Some(first) if first != root => {
                    problems.push(format!(
                        "{file}:{line}: uses both {} and {}, which declare {module}; a workspace holds one module \
                         per path, so use one of them",
                        group(first),
                        group(root)
                    ));
                    malformed = true;
                }
                Some(_) => {}
            }
        }
        if malformed {
            continue;
        }
        for (module, root) in used {
            if declared_twice(modules, module) {
                picks
                    .entry(module.to_string())
                    .or_default()
                    .insert(root.to_string());
            }
        }
    }
    let winners = picks
        .into_iter()
        .filter_map(|(module, roots)| {
            let [root] = <[String; 1]>::try_from(Vec::from_iter(roots)).ok()?;
            Some((module, root))
        })
        .collect();
    (winners, problems)
}

/// Whether two or more indexed modules declare `module`.
fn declared_twice(modules: &Modules, module: &str) -> bool {
    modules.paths.values().filter(|m| *m == module).count() > 1
}

/// The folder a `use` path names, joined to the `go.work`'s folder `base` and cleaned, as a prefix: `.` and `..`
/// collapsed and a trailing `/` dropped. None for an absolute path or one that leaves the repository.
fn folder(base: &str, path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    crate::extract::folders::join(base, path).map(|joined| crate::extract::folders::prefix(&joined))
}

/// The paths a `go.work`'s `use` directives name, each with its line, or the line and problem of a malformed one.
fn uses(text: &str) -> Result<Vec<(String, u32)>, (u32, String)> {
    let mut uses = Vec::new();
    // The open block's kind and line.
    let mut block: Option<(String, u32)> = None;
    for (line, number) in text.lines().zip(1..) {
        let Some(tokens) = tokens(line) else {
            return Err((
                number,
                "the line does not tokenise; close each quoted string on its line".into(),
            ));
        };
        let tokens: Vec<&str> = tokens.iter().map(String::as_str).collect();
        let args = match (&block, tokens.as_slice()) {
            (_, []) => continue,
            (Some(_), [")"]) => {
                block = None;
                continue;
            }
            (Some((kind, _)), args) => (kind.clone(), args.to_vec()),
            (None, [verb, "(", inner @ .., ")"]) => {
                known_block(verb, number)?;
                (verb.to_string(), inner.to_vec())
            }
            (None, [verb, "("]) => {
                known_block(verb, number)?;
                block = Some((verb.to_string(), number));
                continue;
            }
            (None, [verb, args @ ..]) => {
                if !DIRECTIVES.contains(verb) {
                    return Err((
                        number,
                        format!(
                            "unknown directive `{verb}`; a go.work holds only go, toolchain, godebug, use and replace"
                        ),
                    ));
                }
                (verb.to_string(), args.to_vec())
            }
        };
        if let ("use", [path]) = (args.0.as_str(), args.1.as_slice()) {
            uses.extend(unquoted(path).map(|path| (path, number)));
        }
    }
    if let Some((_, number)) = block {
        return Err((
            number,
            "a block opened with `(` is never closed with `)`".into(),
        ));
    }
    Ok(uses)
}

/// The directives a `go.work` may hold (golang/mod `modfile/work.go`).
const DIRECTIVES: [&str; 5] = ["go", "toolchain", "godebug", "use", "replace"];

/// The problem of a block whose kind a `go.work` never holds: only `godebug`, `use` and `replace` take a block.
fn known_block(verb: &str, number: u32) -> Result<(), (u32, String)> {
    if ["godebug", "use", "replace"].contains(&verb) {
        return Ok(());
    }
    Err((
        number,
        format!("unknown block kind `{verb}`; only use, replace and godebug take a `(` block"),
    ))
}

/// A `go.work` line's tokens, as golang/mod's lexer reads them: bare words, `"…"` and `` `…` `` strings kept quoted,
/// `(` and `)`, cut at a `//` comment. None for a line that does not tokenise, such as one with an unterminated string.
fn tokens(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        if rest.starts_with("//") {
            break;
        }
        let len = match rest.as_bytes()[0] {
            b'(' | b')' => 1,
            b'"' => {
                let mut escaped = false;
                1 + rest[1..].find(|c| {
                    let end = c == '"' && !escaped;
                    escaped = c == '\\' && !escaped;
                    end
                })? + 1
            }
            b'`' => 1 + rest[1..].find('`')? + 1,
            _ => rest
                .find(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == '"' || c == '`')
                .map_or(rest.len(), |at| at.max(1)),
        };
        tokens.push(rest[..len].to_string());
        rest = rest[len..].trim_start();
    }
    Some(tokens)
}
