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

    /// A root `Cargo.toml` on disk, the condition `extract` needs, even when the map ignores it (spec #529, "Detection").
    fn detect(&self, repo: &Repo) -> bool {
        repo.root().join("Cargo.toml").is_file()
    }

    fn extract(&self, repo: &Repo) -> Output {
        match cargo(repo.root()) {
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
/// on, dev-dependencies left out.
fn cargo(root: &Path) -> Result<Vec<Pair>, String> {
    if !root.join("Cargo.toml").is_file() {
        return Err(
            "extractor \"cargo\" is named but the repository has no Cargo.toml; remove \"cargo\" from extractors"
                .into(),
        );
    }
    let (metadata, root) = metadata(root).map_err(|e| format!("extractor \"cargo\": {e}"))?;
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
                pairs.push(Pair::new(&from, to));
            }
        }
    }
    Ok(pairs)
}
