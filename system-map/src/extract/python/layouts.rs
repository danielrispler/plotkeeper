//! The layouts a Python project declares in its build config (spec #589, "Declared layouts"): the source folders its
//! packages live in, and the top-level names a folder is renamed to. A form this reads is a root; a form it cannot read
//! leaves the project its default roots and says so in a note.

use std::collections::BTreeSet;

/// What one project declares.
#[derive(Debug, Default)]
pub(super) struct Layout {
    /// The folders it declares its packages in, each a prefix from the repository root, empty for the root itself; none
    /// when it declares no folder, which leaves the project its default `src/`.
    pub(super) roots: Option<BTreeSet<String>>,
    /// Each top-level name that stands for a folder of its own, with that folder's prefix: `package-dir = {mypkg = "lib"}`
    /// makes `lib/` the package `mypkg`.
    pub(super) packages: Vec<(String, String)>,
}

impl Layout {
    /// Adds a declared source folder.
    fn root(&mut self, folder: String) {
        self.roots.get_or_insert_with(BTreeSet::new).insert(folder);
    }

    /// Adds what `other` declares.
    pub(super) fn merge(&mut self, other: Layout) {
        for folder in other.roots.into_iter().flatten() {
            self.root(folder);
        }
        self.packages.extend(other.packages);
    }
}

/// What reading one config gives: its layout, and its notes.
pub(super) struct Reading<'a> {
    /// The project's folder, a prefix.
    project: &'a str,
    /// The config's path, for the notes.
    file: &'a str,
    pub(super) layout: Layout,
    pub(super) notes: Vec<String>,
}

impl<'a> Reading<'a> {
    pub(super) fn new(project: &'a str, file: &'a str) -> Reading<'a> {
        Reading {
            project,
            file,
            layout: Layout::default(),
            notes: Vec::new(),
        }
    }

    /// Notes that `key` of the config was not read.
    fn unread(&mut self, key: &str) {
        let folder = self.project.trim_end_matches('/');
        self.notes.push(format!(
            "python-imports did not read {key} in {}; failing links into {} may be false",
            self.file,
            if folder.is_empty() { "." } else { folder }
        ));
    }

    /// The prefix of `dir`, a folder written relative to the project, or none when it leaves the repository.
    fn folder(&self, dir: &str) -> Option<String> {
        if dir.starts_with('/') {
            return None;
        }
        let mut parts: Vec<&str> = self.project.split('/').filter(|e| !e.is_empty()).collect();
        for part in dir.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    parts.pop()?;
                }
                _ => parts.push(part),
            }
        }
        Some(parts.iter().map(|e| format!("{e}/")).collect())
    }

    /// Declares `dir` a source folder, or notes `key` when `dir` is not a folder of the repository.
    fn declare(&mut self, key: &str, dir: &str) {
        match self.folder(dir) {
            Some(folder) => self.layout.root(folder),
            None => self.unread(key),
        }
    }

    /// Declares each string of the array `value` a source folder; a value that is not an array of strings is noted.
    fn declare_all(&mut self, key: &str, value: &toml::Value) {
        let Some(dirs) = value.as_array().and_then(|a| {
            a.iter()
                .map(toml::Value::as_str)
                .collect::<Option<Vec<_>>>()
        }) else {
            return self.unread(key);
        };
        for dir in dirs {
            self.declare(key, dir);
        }
    }

    /// `package-dir = {"" = "<dir>"}` and `{name = "<dir>"}` pairs, from a table or from `setup.cfg`'s lines.
    fn package_dir(&mut self, key: &str, name: &str, dir: &str) {
        if name.is_empty() {
            return self.declare(key, dir);
        }
        match self.folder(dir) {
            Some(folder) if !name.contains('.') => {
                self.layout.packages.push((name.to_string(), folder))
            }
            _ => self.unread(key),
        }
    }

    /// The layout keys of a parsed `pyproject.toml`: setuptools, Poetry and Hatch.
    pub(super) fn pyproject(&mut self, table: &toml::Table) {
        let Some(tool) = table.get("tool").and_then(toml::Value::as_table) else {
            return;
        };
        if let Some(setuptools) = tool.get("setuptools").and_then(toml::Value::as_table) {
            if let Some(value) = setuptools.get("package-dir") {
                let key = "tool.setuptools.package-dir";
                match value.as_table() {
                    Some(dirs) => {
                        for (name, dir) in dirs {
                            match dir.as_str() {
                                Some(dir) => self.package_dir(key, name, dir),
                                None => self.unread(key),
                            }
                        }
                    }
                    None => self.unread(key),
                }
            }
            if let Some(where_) = setuptools
                .get("packages")
                .and_then(toml::Value::as_table)
                .and_then(|p| p.get("find"))
                .and_then(toml::Value::as_table)
                .and_then(|f| f.get("where"))
            {
                self.declare_all("tool.setuptools.packages.find.where", where_);
            }
        }
        if let Some(packages) = tool
            .get("poetry")
            .and_then(toml::Value::as_table)
            .and_then(|p| p.get("packages"))
        {
            self.poetry(packages);
        }
        if let Some(build) = tool
            .get("hatch")
            .and_then(toml::Value::as_table)
            .and_then(|h| h.get("build"))
            .and_then(toml::Value::as_table)
        {
            self.hatch("tool.hatch.build", build);
            if let Some(targets) = build.get("targets").and_then(toml::Value::as_table) {
                for (name, target) in targets {
                    if let Some(target) = target.as_table() {
                        self.hatch(&format!("tool.hatch.build.targets.{name}"), target);
                    }
                }
            }
        }
    }

    /// Poetry's `packages = [{ include, from }]`: each entry with a `from` gives that folder; one without is read as the
    /// project's own folder, which is a root already.
    fn poetry(&mut self, packages: &toml::Value) {
        let key = "tool.poetry.packages";
        let Some(entries) = packages.as_array() else {
            return self.unread(key);
        };
        for entry in entries {
            match entry.as_table().map(|t| t.get("from")) {
                Some(None) => {}
                Some(Some(from)) => match from.as_str() {
                    Some(from) => self.declare(key, from),
                    None => self.unread(key),
                },
                None => self.unread(key),
            }
        }
    }

    /// Hatch's array `sources = ["<dir>"]` and `packages = ["<dir>/<name>"]`, whose folder is the package's parent.
    fn hatch(&mut self, at: &str, build: &toml::Table) {
        if let Some(sources) = build.get("sources") {
            self.declare_all(&format!("{at}.sources"), sources);
        }
        if let Some(packages) = build.get("packages") {
            let key = format!("{at}.packages");
            let Some(paths) = packages.as_array().and_then(|a| {
                a.iter()
                    .map(toml::Value::as_str)
                    .collect::<Option<Vec<_>>>()
            }) else {
                return self.unread(&key);
            };
            for path in paths {
                self.declare(
                    &key,
                    path.trim_end_matches('/')
                        .rsplit_once('/')
                        .map_or("", |(dir, _)| dir),
                );
            }
        }
    }
}

/// One section of a `setup.cfg`.
struct Section {
    name: String,
    /// Each option, its key lower-cased as `configparser` keeps it, with its value's lines.
    options: Vec<(String, Vec<String>)>,
    /// Whether `configparser` would raise on it: the section header, or an option of it, comes twice.
    duplicate: bool,
}

/// A `setup.cfg` split into sections as Python's `configparser` does, with its defaults: `=` or `:` between key and
/// value, `#` and `;` comment lines, a line indented deeper than its key continuing the value. Err holds the 1-based line
/// that comes before the first section header, which raises `MissingSectionHeaderError`.
fn parse_ini(text: &str) -> Result<Vec<Section>, usize> {
    let mut sections: Vec<Section> = Vec::new();
    // The indent of the last line that was not a continuation, and whether the current option can be continued.
    let mut indent_level = usize::MAX;
    let mut continuable = false;
    for (n, line) in text.lines().enumerate() {
        let value = line.trim();
        if value.is_empty() || value.starts_with('#') || value.starts_with(';') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if continuable && indent > indent_level {
            if let Some(options) = sections.last_mut().and_then(|s| s.options.last_mut()) {
                options.1.push(value.to_string());
            }
            continue;
        }
        indent_level = indent;
        continuable = false;
        if let Some(rest) = value.strip_prefix('[')
            && let Some(end) = rest.rfind(']')
            && end > 0
        {
            let name = rest[..end].to_string();
            let duplicate = sections.iter().any(|s| s.name == name);
            sections.push(Section {
                name,
                options: Vec::new(),
                duplicate,
            });
            continue;
        }
        let Some(section) = sections.last_mut() else {
            return Err(n + 1);
        };
        let Some(at) = value.find(['=', ':']) else {
            continue;
        };
        let key = value[..at].trim().to_lowercase();
        if section.options.iter().any(|(k, _)| *k == key) {
            section.duplicate = true;
        }
        let inline = value[at + 1..].trim();
        let lines = if inline.is_empty() {
            vec![]
        } else {
            vec![inline.to_string()]
        };
        section.options.push((key, lines));
        continuable = true;
    }
    Ok(sections)
}

impl Reading<'_> {
    /// The layout keys of a `setup.cfg`: `[options] package_dir` and `[options.packages.find] where`. Err is the line that
    /// comes before its first section header.
    pub(super) fn setup_cfg(&mut self, text: &str) -> Result<(), usize> {
        let sections = parse_ini(text)?;
        let mut seen = BTreeSet::new();
        for section in sections
            .iter()
            .filter(|s| s.name == "options" || s.name == "options.packages.find")
        {
            if section.duplicate {
                if seen.insert(&section.name) {
                    self.unread(&format!("[{}]", section.name));
                }
                continue;
            }
            for (key, lines) in &section.options {
                match (section.name.as_str(), key.as_str()) {
                    ("options", "package_dir") => {
                        for line in lines {
                            match line.split_once('=') {
                                Some((name, dir)) => {
                                    self.package_dir(
                                        "options.package_dir",
                                        name.trim(),
                                        dir.trim(),
                                    );
                                }
                                None => self.unread("options.package_dir"),
                            }
                        }
                    }
                    ("options.packages.find", "where") => {
                        for dir in lines
                            .iter()
                            .flat_map(|l| l.split(','))
                            .map(str::trim)
                            .filter(|d| !d.is_empty())
                        {
                            self.declare("options.packages.find.where", dir);
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The literal `package_dir={"": "<dir>"}` of a `setup.py`, a guess since the file is code. Any other `package_dir` is
    /// noted.
    pub(super) fn setup_py(&mut self, text: &str) {
        let is_ident = |c: char| c.is_alphanumeric() || c == '_';
        for (at, _) in text.match_indices("package_dir") {
            let (before, after) = (&text[..at], &text[at + "package_dir".len()..]);
            let line_start = before.rfind('\n').map_or(0, |i| i + 1);
            if before.ends_with(is_ident)
                || after.starts_with(is_ident)
                || before[line_start..].contains('#')
            {
                continue;
            }
            match literal_package_dir(after) {
                Some(dir) => self.declare("package_dir", dir),
                None => self.unread("package_dir"),
            }
        }
    }
}

/// The folder of `= {"": "<dir>"}`, written with single or double quotes, whitespace and a trailing comma allowed.
fn literal_package_dir(text: &str) -> Option<&str> {
    fn string(text: &str) -> Option<(&str, &str)> {
        let quote = text.chars().next().filter(|c| matches!(c, '"' | '\''))?;
        let rest = &text[1..];
        let end = rest.find(quote)?;
        let body = &rest[..end];
        (!body.contains(['\\', '\n'])).then(|| (body, &rest[end + 1..]))
    }
    let text = text.trim_start().strip_prefix('=')?.trim_start();
    let text = text.strip_prefix('{')?.trim_start();
    let (empty, text) = string(text)?;
    let text = text.trim_start().strip_prefix(':')?.trim_start();
    let (dir, text) = string(text)?;
    let text = text.trim_start();
    let text = text.strip_prefix(',').unwrap_or(text).trim_start();
    (empty.is_empty() && text.starts_with('}')).then_some(dir)
}
