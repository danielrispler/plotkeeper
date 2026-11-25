//! The facts: what the writer reads and what the checks need, and the diff and map helpers that build them.

use super::{CUT_NOTE_ROOM, TaggedTest, TouchedFile};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
/// What the facts builder knows that the checks need.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CheckContext {
    /// The facts the writer was given: every quote must be in them.
    #[serde(default)]
    pub facts: String,
    /// The whole diff, which a pick check's test must be in. Not stored in samples; they use the facts.
    #[serde(default)]
    pub diff: String,
    pub chapters_mode: bool,
    /// The parts the chapters (or the single story) must light.
    pub required: Vec<String>,
    /// The PR's changed files, which Also touched sorts once the chapters say which parts they light.
    #[serde(default)]
    pub files: Vec<TouchedFile>,
    pub overview: bool,
    pub parts: BTreeSet<String>,
    pub actors: BTreeSet<String>,
    pub links: BTreeSet<String>,
    pub laws: BTreeSet<String>,
    pub flows: BTreeSet<String>,
    /// Each law's lowercased map text, which the banned-word check leaves out.
    pub law_texts: Vec<String>,
    /// The flows the map's diff changes, with the parts their hops run through in order.
    pub flows_changed: BTreeMap<String, Vec<String>>,
    pub tagged: Vec<TaggedTest>,
    /// The spec was created the same day as the PR: its "Not in the plan" step comes first in its chapter.
    #[serde(default)]
    pub spec_same_day: bool,
    /// The Core parts, by Daniel's effective tiers (#397): a chapter that lights one carries a gist, any other chapter none.
    #[serde(default)]
    pub core: Vec<String>,
}

/// How the budget was spent, for the record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BudgetRecord {
    pub total_bytes: usize,
    pub sections: Vec<(String, usize)>,
    pub diff_bytes_full: usize,
    pub files_sent: usize,
    /// Files with some hunks sent and some not.
    pub files_partial: usize,
    pub files_cut: usize,
}

/// What a brief was written without (#477, "Partial facts"), decided when it is written and stored with it, so the reader
/// lists each missing input without guessing. Every field has a default, so a brief stored before it, or by a newer build,
/// still reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartialFacts {
    /// The diff was cut: what the writer did not get whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut: Option<DiffCut>,
    /// The claim check failed or did not run: its error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_error: Option<String>,
    /// The extensions of the diff's source languages that no extractor of the map reads, sorted, each once (".go").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub no_extractor: Vec<String>,
}

impl PartialFacts {
    /// Nothing was missing: the reader shows no chip.
    pub fn is_empty(&self) -> bool {
        self == &PartialFacts::default()
    }
}

/// The import sites behind a link the PR adds (#499, spec #477 "When it is computed"), read from the PR's head in commit
/// mode when the brief is written, so Ratify shows them without extracting anything live.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LinkEvidence {
    /// The added link's id in the map.
    pub link: String,
    /// Each import site of the link's computed edge, once per file and line, in the edge's order.
    #[serde(default)]
    pub sites: Vec<ImportSite>,
}

/// Where an import stands: in the importing file, at its 1-based line when one is known.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSite {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// The files of a cut diff, and its size sent and not sent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DiffCut {
    /// Files none of whose hunks were sent.
    #[serde(default)]
    pub files_cut: Vec<String>,
    /// Files with some hunks sent and some not.
    #[serde(default)]
    pub files_partial: Vec<String>,
    /// Bytes of the diff sent to the writer.
    #[serde(default)]
    pub bytes_sent: usize,
    /// Bytes of the whole diff not sent.
    #[serde(default)]
    pub bytes_cut: usize,
}

/// The source languages without an extractor (#477), by extension. `.rs` and the TypeScript and JavaScript ones count only
/// when the map does not name their extractor; CSS, HTML, JSON, YAML, Markdown and shell are not in the table, so they never
/// count.
const UNREAD_LANGUAGES: &[&str] = &[
    ".go", ".py", ".dart", ".java", ".kt", ".swift", ".rb", ".php", ".cs", ".c", ".cc", ".cpp",
    ".h", ".hpp", ".ex", ".exs", ".scala", ".m",
];
const RUST: &[&str] = &[".rs"];
const TYPESCRIPT: &[&str] = &[".ts", ".tsx", ".js", ".jsx", ".mjs", ".mts", ".cjs"];

/// The extensions, sorted and each once, of `paths`' files in a source language no extractor of the map in `map_text`
/// reads. Ignored files never count.
pub fn no_extractor(map_text: &str, paths: &[String]) -> Result<Vec<String>, String> {
    let map = system_map::parse_lenient(map_text)?;
    let mj: serde_json::Value = serde_json::from_str(map_text).map_err(|e| e.to_string())?;
    let named = |name: &str| {
        mj["extractors"]
            .as_array()
            .is_some_and(|a| a.iter().any(|e| e == name))
    };
    let mut table: Vec<&str> = UNREAD_LANGUAGES.to_vec();
    if !named("rust-modules") {
        table.extend(RUST);
    }
    if !named("typescript-imports") {
        table.extend(TYPESCRIPT);
    }
    let ignored: BTreeSet<String> = system_map::touched_for_map(&map, paths)
        .ignored
        .into_iter()
        .collect();
    let found: BTreeSet<String> = paths
        .iter()
        .filter(|p| !ignored.contains(*p))
        .filter_map(|p| {
            let name = p.rsplit('/').next().unwrap_or(p);
            let ext = &name[name.rfind('.')?..];
            table.contains(&ext).then(|| ext.to_string())
        })
        .collect();
    Ok(found.into_iter().collect())
}

/// The shape code decided.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    pub chapters: bool,
    pub required: Vec<String>,
    pub also: Vec<String>,
    pub overview: bool,
    pub flows_changed: Vec<String>,
}

impl Shape {
    pub fn label(&self) -> String {
        if self.chapters {
            format!("chapters over {}", self.required.join(", "))
        } else {
            "single story".into()
        }
    }
}

/// Guard with a law (#401): a PR that closes an issue labelled `bug` and adds or changes no `@law:`-tagged test offers to leave a
/// law behind it, on one of the parts it touched.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Guard {
    /// The map's parts the PR touched, sorted: their laws on trust can be backed, and a new law goes on one of them.
    pub parts: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub text: String,
    pub shape: Shape,
    pub budget: BudgetRecord,
    pub context: CheckContext,
    /// The writer's instructions when they are not the PR brief's (a part catch-up's, `part_catch_up.rs`).
    pub instructions: Option<String>,
    /// Guard with a law's offer, when the PR fixed a bug and left no tagged test behind.
    pub guard: Option<Guard>,
    /// What the facts lack, as far as building them knows: the cut diff and the languages no extractor reads.
    pub partial_facts: PartialFacts,
}

pub(crate) struct FileDiff {
    pub(crate) path: String,
    pub(crate) header: String,
    pub(crate) hunks: Vec<String>,
    pub(crate) added: usize,
    pub(crate) removed: usize,
}

pub(crate) fn split_diff(diff: &str) -> Vec<FileDiff> {
    let mut files = Vec::new();
    for chunk in diff.split("\ndiff --git ").filter(|c| !c.is_empty()) {
        let chunk = chunk.strip_prefix("diff --git ").unwrap_or(chunk);
        let path = chunk
            .lines()
            .next()
            .and_then(|l| l.split(" b/").last())
            .unwrap_or("")
            .to_string();
        let mut parts = chunk.split("\n@@");
        let header = format!("diff --git {}\n", parts.next().unwrap_or(""));
        let hunks: Vec<String> = parts.map(|h| format!("@@{h}\n")).collect();
        let (mut added, mut removed) = (0, 0);
        for h in &hunks {
            for l in h.lines().skip(1) {
                if l.starts_with('+') {
                    added += 1;
                } else if l.starts_with('-') {
                    removed += 1;
                }
            }
        }
        files.push(FileDiff {
            path,
            header,
            hunks,
            added,
            removed,
        });
    }
    files
}

/// The `@law:` tags the PR added, each with its file and the next added line that names a test.
pub(super) fn added_tags(files: &[FileDiff]) -> Vec<String> {
    let mut tags = Vec::new();
    for f in files {
        for h in &f.hunks {
            let added: Vec<&str> = h.lines().filter(|l| l.starts_with('+')).collect();
            for (i, l) in added.iter().enumerate() {
                if let Some(at) = l.find("@law:") {
                    let id = l[at + 5..].split_whitespace().next().unwrap_or("");
                    let test = added[i + 1..]
                        .iter()
                        .find(|n| n.contains("fn ") || n.contains("test("))
                        .map(|n| n.trim_start_matches('+').trim())
                        .unwrap_or("(no test line found)");
                    tags.push(format!("- law `{id}` in {}: {test}", f.path));
                }
            }
        }
    }
    tags
}

/// Whether two ISO 8601 timestamps fall on the same calendar day (their date parts, as GitHub gives them in UTC); false when either
/// is missing.
pub(super) fn same_day(a: &str, b: &str) -> bool {
    let day = |t: &str| {
        t.get(..10)
            .filter(|d| d.as_bytes().get(4) == Some(&b'-'))
            .map(str::to_string)
    };
    matches!((day(a), day(b)), (Some(a), Some(b)) if a == b)
}

/// The spec's sections the writer gets: a spec's User Stories and Implementation Decisions, a ticket's What to
/// build and Acceptance criteria.
pub fn trim_spec(body: &str) -> String {
    const KEEP: &[&str] = &[
        "User Stories",
        "Implementation Decisions",
        "What to build",
        "Acceptance criteria",
    ];
    let mut out = String::new();
    let mut keep = false;
    for line in body.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            keep = KEEP.iter().any(|k| h.trim().eq_ignore_ascii_case(k));
        }
        if keep {
            out += line;
            out += "\n";
        }
    }
    out
}

pub(crate) fn section(name: &str, text: &str) -> String {
    format!("=== {name} ===\n{}\n\n", text.trim_end())
}

pub(crate) fn ids(map: &serde_json::Value, key: &str) -> BTreeSet<String> {
    map[key]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x["id"].as_str().map(String::from))
        .collect()
}

/// DIFF NOT SENT: the files not fully sent, as many as fit in its room, or `whole` when every file was.
pub(crate) fn cut_note(cut: &[String], whole: &str) -> String {
    if cut.is_empty() {
        whole.to_string()
    } else {
        let mut t = format!(
            "{} files not fully sent; do not guess what they hold:\n",
            cut.len()
        );
        for (i, line) in cut.iter().enumerate() {
            if t.len() + line.len() + 1 > CUT_NOTE_ROOM - 200 {
                // Writing to a `String` cannot fail.
                writeln!(t, "- ... and {} more", cut.len() - i).ok();
                break;
            }
            t += line;
            t += "\n";
        }
        t
    }
}

/// `text` cut to `max` bytes at a character boundary, with a note of what was cut.
pub(crate) fn cut_to(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let end = text.floor_char_boundary(max);
    format!("{}\n[{} more bytes cut]", &text[..end], text.len() - end)
}

/// The parts a flow's hops run through, each once, in the order the flow first reaches it; actors are left out. Each hop runs
/// the way `hop_ends` reads it. A part the flow revisits is not repeated: an order check taps each part once, so a key that
/// repeated one could not be answered.
pub(crate) fn flow_key(
    map: &serde_json::Value,
    flow: &serde_json::Value,
    parts: &BTreeSet<String>,
) -> Vec<String> {
    let mut key: Vec<String> = Vec::new();
    for (from, to) in hop_ends(map, flow).into_iter().flatten() {
        for id in [from, to] {
            if parts.contains(&id) && !key.contains(&id) {
                key.push(id);
            }
        }
    }
    key
}

/// Each hop of `flow` as `(the end it leaves, the end it reaches)`, none for a hop whose link is not on `map`. A hop that
/// leaves the end the flow is at through its link's `to` runs backwards; the first hop runs backwards when the second
/// leaves from its link's `from` end and not its `to`.
pub fn hop_ends(
    map: &serde_json::Value,
    flow: &serde_json::Value,
) -> Vec<Option<(String, String)>> {
    let ends = |hop: &serde_json::Value| {
        let link = map["links"]
            .as_array()?
            .iter()
            .find(|l| l["id"] == hop["link"])?;
        Some((
            link["from"].as_str().unwrap_or("").to_string(),
            link["to"].as_str().unwrap_or("").to_string(),
        ))
    };
    let hops: Vec<Option<(String, String)>> = flow["hops"]
        .as_array()
        .into_iter()
        .flatten()
        .map(ends)
        .collect();
    let mut at: Option<String> = None;
    let mut out = Vec::new();
    for (n, hop) in hops.iter().enumerate() {
        let Some((from, to)) = hop.clone() else {
            out.push(None);
            continue;
        };
        let backwards = match &at {
            Some(at) => *at == to,
            None => hops
                .get(n + 1)
                .cloned()
                .flatten()
                .is_some_and(|(next_from, next_to)| {
                    let joins = |end: &str| end == next_from || end == next_to;
                    joins(&from) && !joins(&to)
                }),
        };
        let (from, to) = if backwards { (to, from) } else { (from, to) };
        at = Some(to.clone());
        out.push(Some((from, to)));
    }
    out
}

#[cfg(test)]
mod flow_key_tests {
    use super::*;

    #[test]
    fn a_flow_that_revisits_a_part_keys_each_part_once_in_first_visit_order() {
        let map = serde_json::json!({"links": [
            {"id": "core-shell", "from": "core", "to": "shell"},
            {"id": "shell-understanding", "from": "shell", "to": "understanding"},
            {"id": "shell-window", "from": "shell", "to": "window"},
        ]});
        let flow = serde_json::json!({"hops": [
            {"link": "core-shell"},
            {"link": "shell-understanding"},
            {"link": "shell-understanding"},
            {"link": "shell-window"},
        ]});
        let parts: BTreeSet<String> = ["core", "shell", "understanding", "window"]
            .map(String::from)
            .into();
        assert_eq!(
            flow_key(&map, &flow, &parts),
            ["core", "shell", "understanding", "window"]
        );
    }

    #[test]
    fn a_hop_run_backwards_keys_the_part_it_arrives_at_after_the_one_it_left() {
        let map = serde_json::json!({"links": [
            {"id": "a-b", "from": "a", "to": "b"},
            {"id": "c-b", "from": "c", "to": "b"},
        ]});
        let flow = serde_json::json!({"hops": [{"link": "a-b"}, {"link": "c-b"}]});
        let parts: BTreeSet<String> = ["a", "b", "c"].map(String::from).into();
        assert_eq!(flow_key(&map, &flow, &parts), ["a", "b", "c"]);
    }

    #[test]
    fn a_first_hop_run_backwards_keys_the_end_the_second_hop_leaves_from_last() {
        // Turn writing: core to the server against `server-core`, then on from the server.
        let map = serde_json::json!({"links": [
            {"id": "server-core", "from": "server", "to": "core"},
            {"id": "server-understanding", "from": "server", "to": "understanding"},
        ]});
        let flow = serde_json::json!({"hops": [{"link": "server-core"}, {"link": "server-understanding"}]});
        let parts: BTreeSet<String> = ["core", "server", "understanding"].map(String::from).into();
        assert_eq!(
            flow_key(&map, &flow, &parts),
            ["core", "server", "understanding"]
        );
    }
}
