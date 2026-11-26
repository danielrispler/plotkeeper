//! Which files an absolute import means, and when that is in doubt (spec #589, "Resolution of an absolute import"): the
//! providers of the first segment, the merging of bare folders, and the `__init__.py` that only extends its `__path__`.

use std::collections::BTreeSet;

use super::{
    Index, Step,
    reader::{Import, Imported},
    stdlib,
};
use crate::extract::names::Lookup;

/// What an absolute import resolves to.
pub(super) enum Resolution {
    /// No local file, or none the import can be said to mean: a third-party or missing module, the standard library.
    Nothing,
    /// The files the import runs.
    Pairs(Vec<String>),
    /// More than one provider could be meant, so the importer is Unsure of `name` for `reason`.
    Unsure {
        name: String,
        reason: &'static str,
        candidates: Vec<String>,
    },
}

/// One place a dotted name lands: the files the import runs on the way, whether the last segment is a module or a
/// regular package, and the folder a name may be a submodule in.
struct Landing {
    files: Vec<String>,
    /// The last segment is a module or a regular package, so the import has a final file of its own (it may be the
    /// init file a package holding the importer leaves out), and not only a bare folder.
    found: bool,
    /// The folder of the last segment when it is a package or a bare folder; none for a module.
    folder: Option<String>,
}

/// A walk in progress: the folder the next segment is looked up in, and the files met so far.
struct Walk {
    dir: String,
    files: Vec<String>,
}

impl Index<'_, '_> {
    /// Walks the dotted name `module` down from each of `dirs` at once. At every segment, a module or regular package
    /// in any folder beats a bare folder in another, as in PEP 420; the bare folders merge, and the walk goes on in all
    /// of them. A folder that holds the name as a module, or lacks it, ends its walk.
    fn walk_all(&self, importer: &str, dirs: &[&str], module: &[String]) -> Vec<Landing> {
        let mut live: Vec<Walk> = dirs
            .iter()
            .map(|dir| Walk {
                dir: dir.to_string(),
                files: Vec::new(),
            })
            .collect();
        let mut landed = Vec::new();
        for (i, name) in module.iter().enumerate() {
            let steps: Vec<(Walk, Step)> = live
                .into_iter()
                .filter_map(|w| self.step(&w.dir, name).map(|step| (w, step)))
                .collect();
            let strong = steps.iter().any(|(_, s)| !matches!(s, Step::Bare { .. }));
            live = Vec::new();
            for (mut walk, step) in steps {
                match step {
                    Step::Bare { folder } if !strong => {
                        walk.dir = folder;
                        live.push(walk);
                    }
                    Step::Bare { .. } => {}
                    Step::Package { folder, init } => {
                        walk.files.extend(Self::init(importer, &folder, init));
                        if i + 1 == module.len() {
                            landed.push(Landing {
                                files: walk.files,
                                found: true,
                                folder: Some(folder),
                            });
                        } else {
                            walk.dir = folder;
                            live.push(walk);
                        }
                    }
                    Step::Module { file } if i + 1 == module.len() => {
                        walk.files.push(file);
                        landed.push(Landing {
                            files: walk.files,
                            found: true,
                            folder: None,
                        });
                    }
                    Step::Module { .. } => {}
                }
            }
        }
        // Only the last segment leaves a walk alive: a bare folder.
        landed.extend(live.into_iter().map(|walk| Landing {
            files: walk.files,
            found: false,
            folder: Some(walk.dir),
        }));
        landed
    }

    /// The files of `name` as a submodule of any of `folders`: one list for each folder that holds it as a module or a
    /// regular package, none when it is only a bare folder or missing.
    fn submodules(&self, importer: &str, folders: &[&str], name: &str) -> Vec<Vec<String>> {
        folders
            .iter()
            .filter_map(|folder| match self.step(folder, name) {
                Some(Step::Package { folder, init }) => {
                    Some(Self::init(importer, &folder, init).into_iter().collect())
                }
                Some(Step::Module { file }) => Some(vec![file]),
                Some(Step::Bare { .. }) | None => None,
            })
            .collect()
    }

    /// The files an absolute import in `importer` runs: the init file of each package on the way and the module's file;
    /// for `from M import n`, also `n`'s file when it is a submodule of `M`. Nothing for a relative import, or when no
    /// root provides the first segment.
    ///
    /// The first segment is looked up under every root and in the standard-library list, each a provider. Two or more
    /// that have it as a module or regular package make the importer Unsure, even when only one holds the final file; a
    /// bare folder is no such provider, and never competes with the standard library. Below that, the importer is Unsure
    /// only when the final file is found more than once.
    pub(super) fn absolute(&self, importer: &str, import: &Import) -> Resolution {
        let Some(first) = import.module.first().filter(|_| import.dots == 0) else {
            return Resolution::Nothing;
        };
        let roots: Vec<&String> = match self.first.lookup(first) {
            Lookup::Unknown => Vec::new(),
            Lookup::One(root) => vec![root],
            Lookup::Ambiguous(roots) => roots,
        };
        let strong = roots
            .iter()
            .filter(|root| {
                matches!(
                    self.step(root, first),
                    Some(Step::Package { .. } | Step::Module { .. })
                )
            })
            .count();
        let stdlib = stdlib::contains(first);
        if stdlib && strong == 0 {
            return Resolution::Nothing;
        }
        let dirs: Vec<&str> = roots.iter().map(|r| r.as_str()).collect();
        let landings = self.walk_all(importer, &dirs, &import.module);
        let mut files: Vec<String> = landings.iter().flat_map(|l| l.files.clone()).collect();
        let folders: Vec<&str> = landings
            .iter()
            .filter_map(|l| l.folder.as_deref())
            .collect();
        let mut unsure = None;
        if strong + usize::from(stdlib) >= 2 {
            unsure = Some((import.module.join("."), reason(stdlib)));
        } else if landings.iter().filter(|l| l.found).count() >= 2 {
            unsure = Some((import.module.join("."), reason(false)));
        }
        if let Imported::Names(names) = &import.imported {
            for name in names {
                let hits = self.submodules(importer, &folders, name);
                if hits.len() >= 2 && unsure.is_none() {
                    unsure = Some((format!("{}.{name}", import.module.join(".")), reason(false)));
                }
                files.extend(hits.into_iter().flatten());
            }
        }
        match unsure {
            None if files.is_empty() => Resolution::Nothing,
            None => Resolution::Pairs(files),
            // An empty candidate set is no finding (spec #569, "An empty candidate set is no finding").
            Some(_) if files.is_empty() => Resolution::Nothing,
            Some((name, reason)) => Resolution::Unsure {
                name,
                reason,
                candidates: files
                    .into_iter()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            },
        }
    }
}

/// The reason two providers give: the standard library is one of them, or two roots are.
fn reason(stdlib: bool) -> &'static str {
    if stdlib {
        "shadows the standard library"
    } else {
        "two roots provide it"
    }
}

/// Whether an `__init__.py` holds nothing but the extension of its own `__path__`, which makes its package a namespace
/// one that merges across roots, as a bare folder does: `__path__ = __import__("pkgutil").extend_path(__path__,
/// __name__)`, `pkgutil.extend_path` after an import, or `pkg_resources.declare_namespace(__name__)`, alone or in the
/// `try` and `except` of the two. Any other statement makes it a regular package.
pub(super) fn extends_path_only(text: &str) -> bool {
    let mut extends = false;
    let statements = text
        .lines()
        .map(|line| line.split_once('#').map_or(line, |(code, _)| code))
        .flat_map(|line| line.split(';'))
        .map(|statement| statement.replace([' ', '\t', '\r'], "").replace('\'', "\""))
        .filter(|statement| !statement.is_empty());
    for statement in statements {
        match statement.as_str() {
            "try:"
            | "else:"
            | "finally:"
            | "importpkgutil"
            | "importpkg_resources"
            | "frompkgutilimportextend_path"
            | "frompkg_resourcesimportdeclare_namespace" => {}
            s if s.starts_with("except") && s.ends_with(':') => {}
            "__path__=__import__(\"pkgutil\").extend_path(__path__,__name__)"
            | "__path__=pkgutil.extend_path(__path__,__name__)"
            | "__path__=extend_path(__path__,__name__)"
            | "__import__(\"pkg_resources\").declare_namespace(__name__)"
            | "pkg_resources.declare_namespace(__name__)"
            | "declare_namespace(__name__)" => extends = true,
            _ => return false,
        }
    }
    extends
}
