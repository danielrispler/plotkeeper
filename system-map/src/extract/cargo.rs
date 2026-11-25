//! The `cargo` extractor: each workspace crate's manifest paired with the manifests of the workspace crates it depends on,
//! from `cargo metadata`, the one toolchain an extractor runs (`docs/adr/0002-extractors-read-text-only.md`).

use std::{collections::HashMap, path::Path, process::Command};

use super::{Extractor, Output, Pair, Repo};

pub(crate) struct Cargo;

impl Extractor for Cargo {
    fn name(&self) -> &'static str {
        "cargo"
    }

    fn reads(&self, file: &str) -> bool {
        file.rsplit('/').next() == Some("Cargo.toml")
    }

    /// A root `Cargo.toml` on disk (in commit mode, in the commit), the condition `extract` needs, even when the map
    /// ignores it (spec #529, "Detection").
    fn detect(&self, repo: &Repo) -> bool {
        repo.has("Cargo.toml")
    }

    fn extract(&self, repo: &Repo) -> Output {
        match cargo(repo) {
            Ok(pairs) => Output {
                pairs,
                ..Output::default()
            },
            Err(problem) => Output {
                problems: vec![problem],
                ..Output::default()
            },
        }
    }

    #[cfg(test)]
    fn fixture(&self) -> super::contract::Fixture {
        use super::contract::{Case, Fixture, MissingConfig};
        // Each crate's library is one shared file, so a source is its manifest alone.
        fn manifest(from: &str, deps: &str) -> String {
            let name = from
                .trim_end_matches("/Cargo.toml")
                .rsplit('/')
                .next()
                .unwrap_or(from);
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[lib]\npath = \"../../lib.rs\"\n\n\
                 [dependencies]\n{deps}"
            )
        }
        Fixture {
            support: vec![
                (
                    "Cargo.toml",
                    "[workspace]\nmembers = [\"crates/*\"]\nresolver = \"2\"\n".into(),
                ),
                ("lib.rs", String::new()),
            ],
            source: |name| format!("crates/{name}/Cargo.toml"),
            importing: |from, to| {
                let dir = to
                    .trim_end_matches("/Cargo.toml")
                    .rsplit('/')
                    .next()
                    .unwrap_or(to);
                manifest(from, &format!("{dir} = {{ path = \"../{dir}\" }}\n"))
            },
            plain: |name| manifest(name, ""),
            missing_config: MissingConfig::Problem {
                file: "Cargo.toml",
                says: "the repository has no Cargo.toml",
            },
            malformed_config: Some("[workspace\n"),
            test_file: None,
            skip: vec![(
                Case::TestFile,
                "a manifest reader has no test files; it leaves dev-dependencies out instead",
            )],
        }
    }
}

/// `cargo metadata` of the workspace at `root`, as JSON, and the root it reports paths against.
pub(super) fn metadata(root: &Path) -> Result<(serde_json::Value, std::path::PathBuf), String> {
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
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("unreadable cargo metadata: {error}"))?;
    Ok((
        metadata,
        root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
    ))
}

/// `path` relative to `root`, with forward slashes.
pub(super) fn relative_to(root: &Path, path: &str) -> Option<String> {
    let path = Path::new(path).strip_prefix(root).ok()?;
    Some(path.to_str()?.replace('\\', "/"))
}

/// Each workspace crate's manifest paired with the manifests of the workspace crates it depends
/// on, dev-dependencies left out, each pair at the line of the dependency's key when a text search finds it.
fn cargo(repo: &Repo) -> Result<Vec<Pair>, String> {
    if !repo.has("Cargo.toml") {
        return Err(
            "extractor \"cargo\" is named but the repository has no Cargo.toml; remove \"cargo\" from extractors"
                .into(),
        );
    }
    let (metadata, root) = repo
        .cargo_dir()
        .and_then(metadata)
        .map_err(|e| format!("extractor \"cargo\": {e}"))?;
    let packages = metadata["packages"].as_array().map_or(&[][..], |p| p);
    let manifest =
        |package: &serde_json::Value| relative_to(&root, package["manifest_path"].as_str()?);
    // A dependency on a workspace crate carries that crate's folder as its `path`; one on the
    // registry's crate of the same name does not.
    let manifests: HashMap<String, String> = packages
        .iter()
        .filter_map(|p| {
            let manifest = manifest(p)?;
            let dir = manifest.strip_suffix("Cargo.toml")?.trim_end_matches('/');
            Some((dir.to_string(), manifest))
        })
        .collect();
    let mut pairs = Vec::new();
    for package in packages {
        let Some(from) = manifest(package) else {
            continue;
        };
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            if let Some(to) = dep["path"]
                .as_str()
                .and_then(|p| relative_to(&root, p))
                .and_then(|dir| manifests.get(&dir))
            {
                // `cargo metadata` gives no line, so the site's line is the dependency's key, found by a text search.
                let key = dep["rename"].as_str().or(dep["name"].as_str());
                let line = key.and_then(|key| key_line(&repo.read(&from)?, key));
                pairs.push(match line {
                    Some(line) => Pair::at(&from, to, line),
                    None => Pair::new(&from, to),
                });
            }
        }
    }
    Ok(pairs)
}

/// The 1-based line of a manifest that names the dependency `key`: a line `key = …` or `key.… = …` (the key bare or
/// quoted) in a table of normal or build dependencies, or the header `[dependencies.key]`; none when no line does, as
/// for a top-level inline table.
fn key_line(manifest: &str, key: &str) -> Option<u32> {
    // `dependencies`, `build-dependencies` and a target's, never `dev-dependencies` or the workspace's
    // `[workspace.dependencies]`, which a root manifest that is also a package holds beside its own.
    let is_deps = |table: &str| {
        let table = table.trim();
        table != "workspace.dependencies"
            && ["dependencies", "build-dependencies"]
                .iter()
                .any(|name| table == *name || table.ends_with(&format!(".{name}")))
    };
    let is_key = |word: &str| word.trim().trim_matches(['"', '\'']) == key;
    let mut in_deps = false;
    for (index, line) in manifest.lines().enumerate() {
        let text = line.trim();
        let found = if let Some(header) = text.strip_prefix('[') {
            let header = header.split(']').next().unwrap_or_default();
            in_deps = is_deps(header);
            header
                .rsplit_once('.')
                .is_some_and(|(table, name)| is_deps(table) && is_key(name))
        } else {
            in_deps
                && text
                    .split_once(['=', '.'])
                    .is_some_and(|(word, _)| is_key(word))
        };
        if found {
            return Some(index as u32 + 1);
        }
    }
    None
}
