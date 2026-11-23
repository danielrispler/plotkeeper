//! Which changed files are logic and which are tests, as a PR brief's facts read them (spec danielrispler/plotkeeper#334). The
//! table that gave a turn a card, which this module's functions came from, is retired (danielrispler/plotkeeper#348).

/// Whether `path` is a logic file: not documentation, a lockfile, generated output or a fixture.
pub fn is_logic(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let segment = |s: &[&str]| path.split('/').any(|p| s.contains(&p));
    let ends = |e: &[&str]| e.iter().any(|e| name.ends_with(e));
    let docs = ends(&[".md", ".mdx", ".txt", ".rst"])
        || segment(&["docs"])
        || name.starts_with("README")
        || name.starts_with("LICENSE");
    let lock = ends(&[".lock", "package-lock.json", "pnpm-lock.yaml", "go.sum"]);
    let generated = segment(&[
        "dist",
        "target",
        "node_modules",
        "generated",
        "__snapshots__",
    ]) || ends(&[".min.js", ".snap", ".pb.go", ".generated.ts"]);
    let fixture = segment(&["fixtures", "fixture", "testdata"]);
    !(docs || lock || generated || fixture)
}

/// Whether `path` is a test file.
pub(crate) fn is_test(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    path.split('/')
        .any(|p| matches!(p, "tests" | "test" | "__tests__"))
        || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name
            .split('.')
            .next()
            .is_some_and(|stem| stem.ends_with("_test"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_lockfiles_fixtures_and_generated_output_are_not_logic() {
        for path in [
            "README.md",
            "docs/studio/a.rs",
            "Cargo.lock",
            "package-lock.json",
            "core/tests/fixtures/run.json",
            "ui/dist/main.js",
        ] {
            assert!(!is_logic(path), "{path}");
        }
        assert!(is_logic("core/src/lib.rs") && is_logic("core/tests/a.rs"));
    }
}
