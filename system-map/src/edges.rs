//! Computed edges: what the map's extractors read from the code, as part-to-part dependencies.

use serde::Serialize;

/// A dependency of the part `from` on the part `to`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
}

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    path::Path,
    process::Command,
};

use crate::{Link, PartFiles};

/// The extractors a map may name.
const KNOWN: [&str; 3] = ["cargo", "rust-modules", "typescript-imports"];

/// One question: which tracked file depends on which, as `(dependent, dependency)` pairs.
type Pairs = Vec<(String, String)>;

/// The edges between parts that the named extractors find and no link joins, and the problems of
/// extractors that are unknown or cannot run.
pub(crate) fn unlinked(
    root: &Path,
    names: &[String],
    parts: &[PartFiles],
    links: &[Link],
) -> (Vec<Edge>, Vec<String>) {
    let mut problems = Vec::new();
    let mut pairs = Pairs::new();
    for name in names {
        let found = match name.as_str() {
            "cargo" => cargo(root),
            // Built by #293; until then they find nothing and a map may already name them.
            "rust-modules" | "typescript-imports" => Ok(Pairs::new()),
            other => Err(format!(
                "unknown extractor {other:?} in extractors; use one of {}",
                KNOWN.join(", ")
            )),
        };
        match found {
            Ok(found) => pairs.extend(found),
            Err(problem) => problems.push(problem),
        }
    }
    (edges_between_parts(&pairs, parts, links), problems)
}

/// Turns pairs of files into edges between the parts that own them, dropping pairs inside one
/// part, pairs touching a file no part owns (ignored files too), and edges a link joins.
fn edges_between_parts(pairs: &Pairs, parts: &[PartFiles], links: &[Link]) -> Vec<Edge> {
    let owner: HashMap<&str, &str> = parts
        .iter()
        .flat_map(|part| part.files.iter().map(|f| (f.as_str(), part.id.as_str())))
        .collect();
    let joined: HashSet<(&str, &str)> = links
        .iter()
        .flat_map(|l| {
            [
                (l.from.as_str(), l.to.as_str()),
                (l.to.as_str(), l.from.as_str()),
            ]
        })
        .collect();
    let edges: BTreeSet<Edge> = pairs
        .iter()
        .filter_map(|(from, to)| Some((owner.get(from.as_str())?, owner.get(to.as_str())?)))
        .filter(|(from, to)| from != to && !joined.contains(&(**from, **to)))
        .map(|(from, to)| Edge {
            from: from.to_string(),
            to: to.to_string(),
        })
        .collect();
    edges.into_iter().collect()
}

/// Each workspace crate's manifest paired with the manifests of the workspace crates it depends
/// on, dev-dependencies left out.
fn cargo(root: &Path) -> Result<Pairs, String> {
    if !root.join("Cargo.toml").is_file() {
        return Err(
            "extractor \"cargo\" is named but the repository has no Cargo.toml; remove \"cargo\" from extractors"
                .into(),
        );
    }
    let output = Command::new("cargo")
        .args([
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--offline",
        ])
        .current_dir(root)
        .output()
        .map_err(|error| format!("extractor \"cargo\": cannot run cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "extractor \"cargo\": cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("extractor \"cargo\": unreadable cargo metadata: {error}"))?;
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let packages = metadata["packages"].as_array().map_or(&[][..], |p| p);
    let manifest = |package: &serde_json::Value| -> Option<String> {
        let path = Path::new(package["manifest_path"].as_str()?);
        let path = path.strip_prefix(&root).ok()?;
        Some(path.to_str()?.replace('\\', "/"))
    };
    let manifests: HashMap<&str, String> = packages
        .iter()
        .filter_map(|p| Some((p["name"].as_str()?, manifest(p)?)))
        .collect();
    let mut pairs = Pairs::new();
    for package in packages {
        let Some(from) = manifest(package) else {
            continue;
        };
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            if let Some(to) = dep["name"].as_str().and_then(|n| manifests.get(n)) {
                pairs.push((from.clone(), to.clone()));
            }
        }
    }
    Ok(pairs)
}
