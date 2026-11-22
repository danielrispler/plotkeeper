//! Law tags: `@law: <id>` on a comment line of its own ties a law to the test that holds it.

use std::{collections::BTreeMap, path::Path};

use serde::Serialize;

use crate::{Backing, Map};

/// One law with the tags that name it.
#[derive(Debug, Serialize)]
pub struct LawReport {
    pub id: String,
    pub backing: Backing,
    pub tags: Vec<Tag>,
}

/// Where a tag stands: a tracked file and a 1-based line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tag {
    pub file: String,
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
fn tag_id(line: &str) -> Option<&str> {
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
