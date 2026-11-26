//! The `python-imports` extractor: the imports of Python files, read as text with no `python` run (ADR 0002), resolved
//! against the repository's roots and paired with the files the import runs (spec #589).

mod reader;

use std::collections::BTreeSet;

use reader::{Import, Imported};

use super::{
    Extractor, Output, Pair, Repo,
    go::{folder_of, group},
    names::{Lookup, Names},
};

pub(crate) struct PythonImports;

impl Extractor for PythonImports {
    fn name(&self) -> &'static str {
        "python-imports"
    }

    /// A `.py` file: `.pyi`, `.pyw` and `.ipynb` never make a pair.
    fn reads(&self, file: &str) -> bool {
        file.ends_with(".py")
    }

    /// A test file by pytest's defaults, `test_*.py`, `*_test.py` and `conftest.py`, or any file under a folder named
    /// `tests` or `test`.
    fn skips(&self, _repo: &Repo, file: &str) -> bool {
        let (folders, name) = file.rsplit_once('/').unwrap_or(("", file));
        name == "conftest.py"
            || (name.starts_with("test_") && name.ends_with(".py"))
            || name.ends_with("_test.py")
            || folders.split('/').any(|e| e == "tests" || e == "test")
    }

    fn extract(&self, repo: &Repo) -> Output {
        python_imports(repo)
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Fixture, MissingConfig};
        // A source is a module of the repository root, so an import of its name names exactly it.
        fn module(file: &str) -> &str {
            file.strip_suffix(".py").unwrap_or(file)
        }
        Fixture {
            support: vec![("pyproject.toml", "[project]\nname = \"kit\"\n".into())],
            source: |name| format!("{name}.py"),
            importing: |_from, to| format!("import {}\n", module(to)),
            plain: |_name| "x = 1\n".into(),
            missing_config: MissingConfig::Fallback {
                file: "pyproject.toml",
            },
            malformed_config: Some("[project\nname = \n"),
            test_file: Some(|name| format!("test_{name}.py")),
            skip: Vec::new(),
        }
    }
}

/// The files whose folder makes a project: a root is the folder holding one.
const CONFIGS: [&str; 3] = ["pyproject.toml", "setup.cfg", "setup.py"];

/// Whether a folder, from the repository root, is one whose configs are never read: a path element named `tests`,
/// `test`, `testdata` or `fixtures`, where packaging tools keep their own test projects.
fn excluded(folder: &str) -> bool {
    folder
        .split('/')
        .any(|e| matches!(e, "tests" | "test" | "testdata" | "fixtures"))
}

/// The roots of the repository: the folders a top-level name resolves from, each a prefix, empty for the repository
/// root, else ending in `/` (spec #589, "Roots").
struct Roots {
    /// The folders holding a tracked config outside the excluded folders and ignored paths, with the repository root.
    projects: BTreeSet<String>,
}

impl Roots {
    /// The roots of `repo`, and the problems of its configs: a `pyproject.toml` that does not parse.
    fn read(repo: &Repo) -> (Roots, Vec<String>) {
        let mut projects = BTreeSet::from([String::new()]);
        let mut problems = Vec::new();
        for file in repo.files() {
            let (folder, name) = (folder_of(file), file.rsplit('/').next().unwrap_or(file));
            if !CONFIGS.contains(&name) || excluded(folder) || repo.is_ignored(file) {
                continue;
            }
            projects.insert(folder.to_string());
            if name == "pyproject.toml"
                && let Some(text) = repo.read(file)
                && let Err(e) = text.parse::<toml::Table>()
            {
                problems.push(format!(
                    "{file}: not valid TOML; fix it or remove it: {}",
                    e.message()
                ));
            }
        }
        (Roots { projects }, problems)
    }

    /// Every root: each project folder and its `src/` when the repository tracks a file in it.
    fn folders(&self, repo: &Repo) -> BTreeSet<String> {
        let mut roots = BTreeSet::new();
        for project in &self.projects {
            let src = format!("{project}src/");
            if has_folder(repo, &src) {
                roots.insert(src);
            }
            roots.insert(project.clone());
        }
        roots
    }
}

/// Whether the repository tracks a file under the folder `prefix`.
fn has_folder(repo: &Repo, prefix: &str) -> bool {
    repo.files()
        .range::<&str, _>(prefix..)
        .next()
        .is_some_and(|f| f.starts_with(prefix))
}

/// What a name stands for in the folder `dir`, as `FileFinder` orders them: a regular package, a module, or a bare
/// folder. A stub is taken only when no `.py` module or package exists.
#[derive(Debug)]
enum Step {
    /// `<name>/__init__.py` (or `.pyi`): its folder and its init file.
    Package { folder: String, init: String },
    /// `<name>.py` (or `.pyi`).
    Module { file: String },
    /// A folder with no `__init__`: a namespace portion, no file of its own.
    Bare { folder: String },
}

/// The tracked files, and the first-segment lookup over every root.
struct Index<'r, 'a> {
    repo: &'r Repo<'a>,
    /// Each top-level name, with every root that holds something of that name.
    first: Names<String>,
}

impl<'r, 'a> Index<'r, 'a> {
    fn new(repo: &'r Repo<'a>, roots: &BTreeSet<String>) -> Index<'r, 'a> {
        let mut first = Names::new();
        for root in roots {
            for file in repo
                .files()
                .range::<&str, _>(root.as_str()..)
                .take_while(|f| f.starts_with(root.as_str()))
            {
                let rest = &file[root.len()..];
                let name = match rest.split_once('/') {
                    Some((folder, _)) => folder,
                    None => rest
                        .strip_suffix(".py")
                        .or_else(|| rest.strip_suffix(".pyi"))
                        .unwrap_or(rest),
                };
                first.register(name, root.clone());
            }
        }
        Index { repo, first }
    }

    fn step(&self, dir: &str, name: &str) -> Option<Step> {
        let folder = format!("{dir}{name}/");
        let has = |file: &str| self.repo.files().contains(file);
        for ext in ["py", "pyi"] {
            let init = format!("{folder}__init__.{ext}");
            if has(&init) {
                return Some(Step::Package { folder, init });
            }
            let file = format!("{dir}{name}.{ext}");
            if has(&file) {
                return Some(Step::Module { file });
            }
        }
        has_folder(self.repo, &folder).then_some(Step::Bare { folder })
    }
}

/// Where a module's name lands: the files the import runs on the way, and the folder a name may be a submodule in.
struct Landing {
    files: Vec<String>,
    /// The folder of the last segment when it is a package or a bare folder; none for a module.
    folder: Option<String>,
}

impl Index<'_, '_> {
    /// The init file of the package at `folder`, unless the package holds `importer`: it ran before the importer, so
    /// the import never depends on it (spec #589, "The walk").
    fn init(importer: &str, folder: &str, init: String) -> Option<String> {
        (!importer.starts_with(folder)).then_some(init)
    }

    /// Walks the dotted name `module` down from `root`: the first segment is found in `root`, and every later one in the
    /// folder before it. None when a segment names nothing, or follows a module.
    fn walk(&self, importer: &str, root: &str, module: &[String]) -> Option<Landing> {
        let mut dir = root.to_string();
        let mut files = Vec::new();
        for (i, name) in module.iter().enumerate() {
            match self.step(&dir, name)? {
                Step::Package { folder, init } => {
                    files.extend(Self::init(importer, &folder, init));
                    dir = folder;
                }
                Step::Module { file } if i + 1 == module.len() => {
                    files.push(file);
                    return Some(Landing {
                        files,
                        folder: None,
                    });
                }
                Step::Module { .. } => return None,
                Step::Bare { folder } => dir = folder,
            }
        }
        Some(Landing {
            files,
            folder: Some(dir),
        })
    }

    /// The files an absolute import in `importer` runs: the init file of each package on the way and the module's file;
    /// for `from M import n`, also `n`'s file when it is a submodule of `M`. Nothing for a relative import, or when the
    /// first segment is provided by no root, or by two or more.
    fn resolve(&self, importer: &str, import: &Import) -> Vec<String> {
        let Some(first) = import.module.first().filter(|_| import.dots == 0) else {
            return Vec::new();
        };
        let Lookup::One(root) = self.first.lookup(first) else {
            return Vec::new();
        };
        let Some(landing) = self.walk(importer, root, &import.module) else {
            return Vec::new();
        };
        let mut files = landing.files;
        if let (Imported::Names(names), Some(folder)) = (&import.imported, &landing.folder) {
            for name in names {
                match self.step(folder, name) {
                    Some(Step::Package { folder, init }) => {
                        files.extend(Self::init(importer, &folder, init));
                    }
                    Some(Step::Module { file }) => files.push(file),
                    Some(Step::Bare { .. }) | None => {}
                }
            }
        }
        files
    }
}

/// Each Python file paired with the files each local module it imports runs.
fn python_imports(repo: &Repo) -> Output {
    let (roots, problems) = Roots::read(repo);
    let index = Index::new(repo, &roots.folders(repo));
    let mut found = Output {
        problems,
        ..Output::default()
    };
    for &file in repo.files() {
        if !PythonImports.reads(file) || PythonImports.skips(repo, file) {
            continue;
        }
        let from_group = group(folder_of(file));
        found.members.insert((file.to_string(), from_group.clone()));
        let Some(text) = repo.read(file) else {
            continue;
        };
        for import in reader::imports(&text) {
            for to in index.resolve(file, &import) {
                found.pairs.push(Pair {
                    group: Some(group(folder_of(&to))),
                    from_group: Some(from_group.clone()),
                    ..Pair::at(file, &to, import.line)
                });
            }
        }
    }
    found
}
