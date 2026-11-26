//! Relative imports (spec #589, "Relative imports"): the importing file's own package is the anchor, and each dot past
//! the first climbs a folder. Whether a climb is legal depends on the root the file is imported under, as in CPython's
//! `_resolve_name`; where roots disagree the importer is Unsure.

use std::collections::BTreeSet;

use super::{
    Index, Step,
    doubt::Resolution,
    reader::{Import, Imported},
};
use crate::extract::folder_of;

/// The reason a climb that some roots allow and others do not gives.
const CLIMBS: &str = "climbs past the top package under some roots";

/// The folder `up` levels above `folder` (a prefix ending in `/`, or empty), or none when that leaves the repository.
fn climb(folder: &str, up: u32) -> Option<String> {
    let mut at = folder;
    for _ in 0..up {
        let parent = at.strip_suffix('/')?;
        at = parent.rfind('/').map_or("", |i| &parent[..=i]);
    }
    Some(at.to_string())
}

impl Index<'_, '_> {
    /// The files a relative import in `importer` runs, from the folder its dots climb to: the module's package inits
    /// and file, and each imported name that is a submodule. `from . import y` also pairs with the package's own
    /// `__init__.py`, where `y` may be defined; `from .. import y` does not. A climb is legal under a root holding the
    /// importer when the importer is at least as many folders below it as the import has dots: legal under all of them
    /// pairs, under some only makes the importer Unsure, and under none, or past the repository, or naming no file,
    /// gives nothing.
    pub(super) fn relative(
        &self,
        importer: &str,
        import: &Import,
        roots: &BTreeSet<String>,
    ) -> Resolution {
        let folder = folder_of(importer);
        let Some(anchor) = climb(folder, import.dots - 1) else {
            return Resolution::Nothing;
        };
        let landings = self.walk_all(importer, &[anchor.as_str()], &import.module);
        let mut files: Vec<String> = landings.iter().flat_map(|l| l.files.clone()).collect();
        // Only the importer's own package: two or more dots climb to a package that holds the importer, whose init file
        // ran before it, as an absolute import leaves it out (`Index::init`). A namespace `__init__.py`, which only
        // extends its `__path__`, is no package of its own and gets no pair.
        if import.module.is_empty()
            && import.dots == 1
            && let Some(Step::Package { init, .. }) = self.folder_step(&anchor)
            && init != importer
        {
            files.push(init);
        }
        if let Imported::Names(names) = &import.imported {
            let folders: Vec<&str> = landings
                .iter()
                .filter_map(|l| l.folder.as_deref())
                .collect();
            for name in names {
                files.extend(
                    self.submodules(importer, &folders, name)
                        .into_iter()
                        .flatten(),
                );
            }
        }
        let files: Vec<String> = files
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if files.is_empty() {
            return Resolution::Nothing;
        }
        // A renamed package is a root of its own whose top-level folder is the package's name, so a file under its folder
        // sits one level deeper than the folders between it and the folder.
        let renamed = self
            .renamed
            .values()
            .filter_map(|(_, folder)| importer.strip_prefix(folder.as_str()))
            .map(|rest| rest.matches('/').count() + 1);
        let depths: Vec<bool> = roots
            .iter()
            .filter_map(|root| importer.strip_prefix(root.as_str()))
            .map(|rest| rest.matches('/').count())
            .chain(renamed)
            .map(|depth| depth >= import.dots as usize)
            .collect();
        if depths.iter().all(|&legal| legal) {
            Resolution::Pairs(files)
        } else if depths.iter().any(|&legal| legal) {
            Resolution::Unsure {
                name: format!(
                    "{}{}",
                    ".".repeat(import.dots as usize),
                    import.module.join(".")
                ),
                reason: CLIMBS,
                candidates: files,
            }
        } else {
            Resolution::Nothing
        }
    }
}
