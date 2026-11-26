//! Which parts a list of paths touches, and which of a part's held items it reaches, by pattern alone.

use std::{collections::BTreeSet, path::Path};

use serde::Serialize;

use crate::{Map, Owners, PartFiles, Placement, pattern::Pattern};

/// What `touched` found.
#[derive(Debug, Default, Serialize)]
pub struct Touched {
    /// The parts the paths touch, in the map's order, each with its paths.
    pub parts: Vec<PartFiles>,
    /// The paths an ignore pattern places.
    pub ignored: Vec<String>,
    /// The paths no part owns and no pattern ignores.
    pub unowned: Vec<String>,
}

/// Places `paths` by the map in `root`, by pattern alone: a path need not exist or be tracked.
/// Fails with the problems of a map file that cannot be read or fails the shape check, as `check`
/// would report them.
pub fn touched(root: &Path, paths: &[String]) -> Result<Touched, Vec<String>> {
    Ok(touched_for_map(&crate::load_checked(root)?, paths))
}

/// Places `paths` by an already parsed `map`, with no file read and no check of the map.
pub fn touched_for_map(map: &Map, paths: &[String]) -> Touched {
    let owners = Owners::new(map);
    let mut found = Touched::default();
    let mut parts: Vec<Vec<String>> = vec![Vec::new(); map.parts.len()];
    for path in paths.iter().collect::<BTreeSet<_>>() {
        match owners.place(path) {
            Placement::Ignored => found.ignored.push(path.clone()),
            Placement::Part(index) => parts[index].push(path.clone()),
            Placement::Unowned => found.unowned.push(path.clone()),
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
    found
}

/// The places, counting from 0, of part `part`'s held items in `map` whose `paths` match one of `files`, by the map's own path
/// patterns; a held item with no paths is reached by none. Empty for a part the map does not hold.
pub fn held_reached(map: &Map, part: &str, files: &[String]) -> Vec<usize> {
    let Some(part) = map.parts.iter().find(|p| p.id == part) else {
        return Vec::new();
    };
    part.holds
        .iter()
        .enumerate()
        .filter(|(_, held)| {
            let patterns: Vec<Pattern> = held
                .paths
                .iter()
                .flatten()
                .map(|p| Pattern::new(p))
                .collect();
            files
                .iter()
                .any(|file| patterns.iter().any(|p| p.matches(file)))
        })
        .map(|(n, _)| n)
        .collect()
}
