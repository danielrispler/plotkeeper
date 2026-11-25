//! Law tags: `@law: <id>` on a comment line of its own ties a law to the test that holds it.

use std::{collections::BTreeMap, path::Path};

use serde::Serialize;

use crate::{Backing, Map};

/// One law with the tags that name it.
#[derive(Debug, Serialize)]
pub struct LawReport {
    /// The law's id.
    pub id: String,
    /// Whether a test stands behind it.
    pub backing: Backing,
    /// The tags that name it.
    pub tags: Vec<Tag>,
}

/// Where a tag stands: a tracked file and a 1-based line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    /// The file's path from the repository's root.
    pub file: String,
    /// The tag's line, from 1.
    pub line: usize,
}

/// Each law with its tags, and the problems of laws held to the wrong backing and of tags naming
/// no law. Every tracked file is read, ignored paths included.
pub(crate) fn check(root: &Path, map: &Map, files: &[String]) -> (Vec<LawReport>, Vec<String>) {
    let mut found: BTreeMap<String, Vec<Tag>> = BTreeMap::new();
    for file in files {
        // A symlink or a file gone from disk carries no tag; other bytes read lossily.
        let Some(text) = crate::read_tracked(root, file) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if let Some(id) = tag_id(line) {
                found.entry(id.to_string()).or_default().push(Tag {
                    file: file.clone(),
                    line: index + 1,
                });
            }
        }
    }
    let mut problems = Vec::new();
    let mut laws = Vec::new();
    for law in &map.laws {
        let tags = found.remove(&law.id).unwrap_or_default();
        match law.backing {
            Backing::Check if tags.is_empty() => problems.push(format!(
                "law {:?} says check, but no test is tagged for it; tag a test with a comment line \"@law: {}\" or mark the law trust",
                law.id, law.id
            )),
            Backing::Trust if !tags.is_empty() => {
                for tag in &tags {
                    problems.push(format!(
                        "law {:?} says trust, but {}:{} tags it; mark the law check or remove the tag",
                        law.id, tag.file, tag.line
                    ));
                }
            }
            _ => {}
        }
        laws.push(LawReport {
            id: law.id.clone(),
            backing: law.backing,
            tags,
        });
    }
    for (id, tags) in found {
        for tag in tags {
            problems.push(format!(
                "{}:{}: tag names law {id:?}, which the map does not have; fix the id or add the law",
                tag.file, tag.line
            ));
        }
    }
    (laws, problems)
}

/// The id a line tags, when the line is a comment of its own holding only `@law: <id>`.
/// A comment line starts, after its indentation, with a comment leader (`//`, `///`, `//!`, `#`,
/// `--`, `/*`, `*`, `<!--`); a closing `*/` or `-->` may follow the id.
pub fn tag_id(line: &str) -> Option<&str> {
    let mut rest = line.trim();
    let leader = ["<!--", "/*", "//", "#", "--", "*"]
        .into_iter()
        .find(|leader| rest.starts_with(leader))?;
    rest = rest[leader.len()..].trim_start_matches(['/', '!', '*', '#', '-', ' ', '\t']);
    let rest = rest.strip_prefix("@law:")?.trim();
    let rest = rest
        .strip_suffix("*/")
        .or_else(|| rest.strip_suffix("-->"))
        .unwrap_or(rest)
        .trim();
    (!rest.is_empty() && !rest.contains(char::is_whitespace)).then_some(rest)
}

/// A tag with the test it sits above.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LawTest {
    /// The law the tag names.
    pub law: String,
    /// Where the tag stands.
    pub tag: Tag,
    /// The test's name; none when no test follows the tag.
    pub test: Option<String>,
}

/// The test each tag of `laws` sits above, read from the files in `root`. A file that cannot be
/// read leaves its tags without a test.
pub fn law_tests(root: &Path, laws: &[LawReport]) -> Vec<LawTest> {
    let mut texts: BTreeMap<&str, Option<String>> = BTreeMap::new();
    let mut found = Vec::new();
    for law in laws {
        for tag in &law.tags {
            let text = texts
                .entry(&tag.file)
                .or_insert_with(|| crate::read_tracked(root, &tag.file));
            found.push(LawTest {
                law: law.id.clone(),
                tag: tag.clone(),
                test: text.as_deref().and_then(|t| test_name_after(t, tag.line)),
            });
        }
    }
    found
}

/// The name of the test that starts after the 1-based `line` of `text`: a Rust `fn` or a Node
/// `test("...")`, reached over blank lines, comments (other tags among them) and attributes.
pub fn test_name_after(text: &str, line: usize) -> Option<String> {
    let mut lines = text.lines().skip(line);
    let mut depth = 0usize; // Open brackets of an attribute that runs over several lines.
    for raw in lines.by_ref() {
        let trimmed = raw.trim();
        if let Some(open) = attribute_brackets(depth, trimmed) {
            depth = open;
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }
        return rust_fn_name(trimmed).or_else(|| node_test_title(trimmed));
    }
    None
}

/// Whether the trimmed `line` is an attribute, or a line of one that runs over several, with `depth`
/// of its brackets open before it: the brackets left open after it when it is, `None` when it is not.
pub fn attribute_brackets(depth: usize, line: &str) -> Option<usize> {
    (depth > 0 || line.starts_with("#[") || line.starts_with("#!["))
        .then(|| (depth + line.matches('[').count()).saturating_sub(line.matches(']').count()))
}

fn rust_fn_name(line: &str) -> Option<String> {
    let mut rest = line;
    loop {
        let word = rest.split_whitespace().next()?;
        if let Some(after) = rest.strip_prefix("fn ") {
            let name: String = after
                .trim_start()
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            return (!name.is_empty()).then_some(name);
        }
        let qualifier = word.starts_with("pub") || ["async", "unsafe", "const"].contains(&word);
        if !qualifier {
            return None;
        }
        // `pub(crate)` is one word; `pub (crate)` is not used.
        rest = rest[rest.find(word)? + word.len()..].trim_start();
    }
}

fn node_test_title(line: &str) -> Option<String> {
    let rest = line.strip_prefix("test(")?.trim_start();
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &rest[1..];
    let end = body.find(quote)?;
    Some(body[..end].to_string())
}
