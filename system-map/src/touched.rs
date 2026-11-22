//! Which parts a list of paths touches, by pattern alone.

use std::{collections::BTreeSet, path::Path};

use serde::Serialize;

use crate::{PartFiles, pattern::Pattern};

/// What `touched` found.
#[derive(Debug, Default, Serialize)]
pub struct Touched {
    pub parts: Vec<PartFiles>,
    pub ignored: Vec<String>,
    pub unowned: Vec<String>,
}

/// Places `paths` by the map in `root`, by pattern alone: a path need not exist or be tracked.
/// Fails with the map file's problem when it cannot be read or has the wrong shape.
pub fn touched(root: &Path, paths: &[String]) -> Result<Touched, String> {
    let map = crate::load(root)?;
    let ignored: Vec<Pattern> = map.ignored.iter().map(|p| Pattern::new(p)).collect();
    let part_patterns: Vec<Vec<Pattern>> = map
        .parts
        .iter()
        .map(|part| part.paths.iter().map(|p| Pattern::new(p)).collect())
        .collect();
    let mut found = Touched::default();
    let mut parts: Vec<Vec<String>> = vec![Vec::new(); map.parts.len()];
    for path in paths.iter().collect::<BTreeSet<_>>() {
        if ignored.iter().any(|p| p.matches(path)) {
            found.ignored.push(path.clone());
            continue;
        }
        match part_patterns
            .iter()
            .rposition(|patterns| patterns.iter().any(|p| p.matches(path)))
        {
            Some(index) => parts[index].push(path.clone()),
            None => found.unowned.push(path.clone()),
        }
    }
    found.parts = map
        .parts
        .iter()
        .zip(parts)
        .filter(|(_, files)| !files.is_empty())
        .map(|(part, files)| PartFiles {
            id: part.id.clone(),
            files,
        })
        .collect();
    Ok(found)
}
