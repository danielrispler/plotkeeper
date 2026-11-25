//! The source: what git and gh give, as plain data.

use super::{Decision, LinkEvidence};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
/// A spec or ticket the PR closes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Spec {
    pub number: u64,
    pub title: String,
    pub created_at: String,
    /// The issue's whole body; the facts keep the sections the writer needs.
    pub body: String,
}

/// What `gh` says of a PR.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrMeta {
    pub number: u64,
    pub title: String,
    pub body: String,
    /// The head commit's id.
    pub head: String,
    /// When it merged, empty while open.
    pub merged_at: String,
    /// When it was created (ISO 8601), empty when unknown.
    #[serde(default)]
    pub created_at: String,
    /// One "name conclusion" line per check.
    pub checks: Vec<String>,
    pub spec: Option<Spec>,
    /// Every issue the PR closes, lowest first; `spec` is the lowest of them (ticket #394).
    #[serde(default)]
    pub closing: Vec<u64>,
    /// One of the issues it closes is labelled `bug` (#401: Guard with a law).
    #[serde(default)]
    pub bug: bool,
}

/// A test that carries a `@law:` tag.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaggedTest {
    pub law: String,
    pub path: String,
    pub test: String,
    /// Whether this PR's diff touches it; set by `build_facts`.
    #[serde(default)]
    pub changed: bool,
}

/// Everything the facts builder reads, gathered.
#[derive(Clone, Debug, Default)]
pub struct PrSource {
    pub meta: PrMeta,
    /// `system-map.json` at the PR's head (or, for a PR older than the map, as of the map's birth).
    pub map_text: String,
    /// The map's diff in this PR, empty when it did not change.
    pub map_diff: String,
    /// A line saying why `map_diff` is empty when the map is older than the PR.
    pub map_note: Option<String>,
    /// `git diff -M base head`.
    pub diff: String,
    /// Every `@law:` tag at the head with its test, `changed` not yet set.
    pub tagged: Vec<TaggedTest>,
    /// Daniel's tier per part where it differs from the map's (`fog.rs`'s effective tiers).
    pub tier_overrides: BTreeMap<String, String>,
    /// The decisions of an earlier brief of this PR, which a rewrite keeps the ids of.
    pub earlier_decisions: Vec<Decision>,
    /// A "Since you read" story (#497, spec #477 part B): the head Daniel fully read, which the diff runs from. Its brief is
    /// stored apart from the full briefs (`BriefStore::since`).
    pub since: Option<String>,
    /// The import sites behind each link the PR adds (#499), read from its head by the server; stored on the brief as they are.
    pub link_evidence: Vec<LinkEvidence>,
}
