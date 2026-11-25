//! The `typescript-imports` extractor: the import specifiers of script files, read from a token stream that leaves out
//! comments, strings and the text of template literals, resolved to tracked files.

use std::{collections::BTreeSet, path::Path};

/// The extensions of the script files `typescript-imports` reads.
pub(crate) const SCRIPTS: [&str; 7] = ["ts", "tsx", "js", "mjs", "jsx", "mts", "cjs"];

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

/// Whether a `/` after `prev` starts a regular expression rather than a division.
fn regex_may_follow(prev: Option<&Tok>) -> bool {
    match prev {
        None => true,
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
/// are left out; the code in a template's `${}` is read.
fn tokens(text: &str) -> Vec<Tok> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
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
        } else if c == '/' && regex_may_follow(tokens.last()) {
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
    }
    tokens
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

/// The specifiers a file's tokens import: `import … from "x"`, `import "x"`, `import("x")`, `require("x")`,
/// `export … from "x"` (`export *`, `export * as ns`, `export { … }`) and `export type … from "x"`.
fn specifiers(tokens: &[Tok]) -> Vec<&str> {
    let mut specs = Vec::new();
    for (at, tok) in tokens.iter().enumerate() {
        let Tok::Ident(word) = tok else { continue };
        if at > 0 && tokens[at - 1] == Tok::Sym('.') {
            continue;
        }
        let call = || match (tokens.get(at + 1), tokens.get(at + 2)) {
            (Some(Tok::Sym('(')), Some(Tok::Str(spec))) => Some(spec.as_str()),
            _ => None,
        };
        let found = match word.as_str() {
            "require" => call(),
            "import" => match tokens.get(at + 1) {
                Some(Tok::Str(spec)) => Some(spec.as_str()),
                Some(Tok::Sym('(')) => call(),
                _ => from_clause(tokens, at + 1),
            },
            "export" => {
                let mut i = at + 1;
                if is_word(tokens.get(i), "type") {
                    i += 1;
                }
                match tokens.get(i) {
                    Some(Tok::Sym('*' | '{')) => from_clause(tokens, i),
                    _ => None,
                }
            }
            _ => None,
        };
        specs.extend(found);
    }
    specs
}

/// The specifier of the `from "x"` that ends an import or re-export clause starting at `i`: names, `type`, `as`, `*`,
/// commas and braces, nothing else.
fn from_clause(tokens: &[Tok], mut i: usize) -> Option<&str> {
    while let Some(tok) = tokens.get(i) {
        match tok {
            Tok::Ident(word) if word == "from" => {
                if let Some(Tok::Str(spec)) = tokens.get(i + 1) {
                    return Some(spec);
                }
            }
            Tok::Ident(_) | Tok::Sym('*' | '{' | '}' | ',') => {}
            _ => return None,
        }
        i += 1;
    }
    None
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

/// Each script file paired with the tracked files its relative imports name.
pub(crate) fn typescript_imports(root: &Path, tracked: &BTreeSet<&str>) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for file in tracked {
        let script = file
            .rsplit_once('.')
            .is_some_and(|(_, ext)| SCRIPTS.contains(&ext));
        let Some(text) = script.then(|| crate::read_tracked(root, file)).flatten() else {
            continue;
        };
        for spec in specifiers(&tokens(&text)) {
            if let Some(target) = resolve_import(file, spec, tracked).filter(|t| t != file) {
                pairs.push((file.to_string(), target));
            }
        }
    }
    pairs
}
