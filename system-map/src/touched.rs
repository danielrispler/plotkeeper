//! Which parts a list of paths touches, by pattern alone.

use std::{collections::BTreeSet, path::Path};

use serde::Serialize;

use crate::{Owners, PartFiles, Placement};

/// What `touched` found.
#[derive(Debug, Default, Serialize)]
pub struct Touched {
    pub parts: Vec<PartFiles>,
    pub ignored: Vec<String>,
    pub unowned: Vec<String>,
}

/// Places `paths` by the map in `root`, by pattern alone: a path need not exist or be tracked.
/// Fails with the problems of a map file that cannot be read or fails the shape check, as `check`
/// would report them.
pub fn touched(root: &Path, paths: &[String]) -> Result<Touched, Vec<String>> {
    let map = crate::load_checked(root)?;
    let owners = Owners::new(&map);
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
    Ok(found)
}
