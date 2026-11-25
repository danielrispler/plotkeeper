//! Version 2 of a part (spec #473): a short `purpose` and the named things it `holds`, their caps, the conversion of a
//! version 1 map and the one text that prompts give a part.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

use crate::{MAP_FILE, Map, PartFiles, pattern::Pattern};

/// One named thing a part holds: a sentence on it and, optionally, its key files in the parts' pattern syntax.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Held {
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) paths: Option<Vec<String>>,
}

/// The fields of a held item, for `parse_lenient` to tell the unknown ones.
pub(crate) const HELD_FIELDS: &[&str] = &["name", "text", "paths"];

/// The most characters a purpose holds: one or two sentences.
const PURPOSE_CAP: usize = 240;
/// The most characters a held item's text holds: one sentence.
const TEXT_CAP: usize = 140;
/// The most items a part holds.
const ITEMS_CAP: usize = 8;

/// A version 1 map's text as version 2: each part's `description` becomes its `purpose`, untruncated, and its `holds` is
/// empty. A map of any other version comes back as it was. Fails only when the text is not JSON.
pub fn convert(text: &str) -> Result<String, String> {
    let mut map: Value = serde_json::from_str(text)
        .map_err(|error| format!("{MAP_FILE}: is not valid JSON: {error}"))?;
    if map["version"] != 1 {
        return Ok(text.to_string());
    }
    map["version"] = 2.into();
    if let Some(parts) = map.get_mut("parts").and_then(Value::as_array_mut) {
        for part in parts.iter_mut().filter_map(Value::as_object_mut) {
            let purpose = part.remove("description").unwrap_or_else(|| "".into());
            part.insert("purpose".into(), purpose);
            part.entry("holds").or_insert_with(|| Value::Array(vec![]));
        }
    }
    Ok(map.to_string())
}

/// A part of a version 2 map as a prompt quotes it: its purpose, then each held item on a line of its own with its key
/// files.
pub fn part_text(part: &Value) -> String {
    let words = |item: &Value, key: &str| item[key].as_str().unwrap_or_default().to_string();
    let mut text = words(part, "purpose");
    for item in part["holds"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "\n  - {}: {}",
            words(item, "name"),
            words(item, "text")
        ));
        let paths: Vec<&str> = item["paths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect();
        if !paths.is_empty() {
            text.push_str(&format!(" (key files: {})", paths.join(", ")));
        }
    }
    text
}

/// `n` with a comma between each three digits, as the cap messages print a length.
pub(crate) fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The problems of each part's purpose and held items: blank texts, the caps and a name held twice.
pub(crate) fn check(map: &Map) -> Vec<String> {
    let mut problems = Vec::new();
    for part in &map.parts {
        let id = &part.id;
        let length = part.purpose.chars().count();
        if part.purpose.trim().is_empty() {
            problems.push(format!(
                "part {id:?}: purpose is empty; say the part's job in one or two sentences"
            ));
        } else if length > PURPOSE_CAP {
            problems.push(format!(
                "part {id:?}: purpose is {} characters, the cap is {PURPOSE_CAP}; keep the job here, move the detail into a held \
                 item or the area's doc, or split the part",
                grouped(length)
            ));
        }
        if part.holds.len() > ITEMS_CAP {
            problems.push(format!(
                "part {id:?}: holds {} items, the cap is {ITEMS_CAP}; merge the smaller ones into the area's doc, or split the part",
                part.holds.len()
            ));
        }
        let mut names = HashSet::new();
        for item in &part.holds {
            let name = &item.name;
            if name.trim().is_empty() {
                problems.push(format!(
                    "part {id:?}: a held item's name is empty; name the thing it holds"
                ));
            } else if !names.insert(name.as_str()) {
                problems.push(format!(
                    "part {id:?}: the held item {name:?} is named more than once; give each item its own name"
                ));
            }
            let length = item.text.chars().count();
            if item.text.trim().is_empty() {
                problems.push(format!(
                    "part {id:?}: the held item {name:?} text is empty; say what it is in one sentence"
                ));
            } else if length > TEXT_CAP {
                problems.push(format!(
                    "part {id:?}: the held item {name:?} text is {} characters, the cap is {TEXT_CAP}; keep one sentence here and \
                     move the detail into the area's doc",
                    grouped(length)
                ));
            }
        }
    }
    problems
}

/// The problems of held items whose paths match no file their part owns.
pub(crate) fn check_paths(map: &Map, owned: &[PartFiles]) -> Vec<String> {
    let mut problems = Vec::new();
    for (part, files) in map.parts.iter().zip(owned) {
        for item in &part.holds {
            for path in item.paths.iter().flatten() {
                let pattern = Pattern::new(path);
                if !files.files.iter().any(|file| pattern.matches(file)) {
                    problems.push(format!(
                        "part {:?}: the held item {:?} names {path:?}, which matches no file the part owns; name a file of its own",
                        part.id, item.name
                    ));
                }
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::grouped;

    #[test]
    fn a_length_is_grouped_by_thousands() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(240), "240");
        assert_eq!(grouped(3986), "3,986");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
