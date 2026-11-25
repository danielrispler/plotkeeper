//! The contract cases: what every registered extractor must do, run on each through the files of its [`Fixture`]. The
//! cases build repositories from the source files `a`, `b` and `c`, which a kit's support files may name.

use super::{Extractor, Output, Repo, registry::EXTRACTORS, run};

/// What an extractor gives the contract cases to build their repositories from.
pub(crate) struct Fixture {
    /// Files every case writes besides the sources: a config, a crate root, a workspace manifest. The config is one.
    pub(crate) support: Vec<(&'static str, String)>,
    /// The path of the source file `name`.
    pub(crate) source: fn(&str) -> String,
    /// Valid source of the file `from` that imports the file `to`.
    pub(crate) importing: fn(&str, &str) -> String,
    /// Valid source of the file `name` that imports nothing.
    pub(crate) plain: fn(&str) -> String,
    /// What a repository without the extractor's config gives.
    pub(crate) missing_config: MissingConfig,
    /// The text of a config that does not parse.
    pub(crate) malformed_config: Option<&'static str>,
    /// The cases the extractor opts out of, each with the reason the test prints.
    pub(crate) skip: Vec<(Case, &'static str)>,
}

/// What a repository without an extractor's config gives.
pub(crate) enum MissingConfig {
    /// Without the support file `file`, the extractor falls back cleanly: no problem.
    Fallback { file: &'static str },
    /// Without the support file `file`, the extractor gives a problem that says `says`.
    Problem {
        file: &'static str,
        says: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Case {
    MalformedConfig,
}

/// Writes `files` into a new folder, runs `extractor` there through the core with every file tracked, and gives what
/// it found.
fn run_on(extractor: &dyn Extractor, files: &[(String, Vec<u8>)]) -> Output {
    let dir = tempfile::tempdir().unwrap();
    for (file, bytes) in files {
        let path = dir.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    let tracked: Vec<String> = files.iter().map(|(f, _)| f.clone()).collect();
    let repo = Repo::new(dir.path(), &tracked, |_| false);
    run(&[extractor], &[extractor.name().to_string()], &repo)
}

/// The support files and `a`, importing `b`, with `b` importing nothing.
fn anchor_files(kit: &Fixture) -> Vec<(String, Vec<u8>)> {
    let (a, b) = ((kit.source)("a"), (kit.source)("b"));
    let mut files: Vec<(String, Vec<u8>)> = kit
        .support
        .iter()
        .map(|(f, text)| (f.to_string(), text.clone().into_bytes()))
        .collect();
    files.push((a.clone(), (kit.importing)(&a, &b).into_bytes()));
    files.push((b.clone(), (kit.plain)(&b).into_bytes()));
    files
}

/// Each registered extractor with its kit, the cases it skips printed with their reasons.
fn kits() -> Vec<(&'static dyn Extractor, Fixture)> {
    EXTRACTORS
        .iter()
        .map(|extractor| {
            let kit = extractor.fixture();
            for (case, reason) in &kit.skip {
                println!("{}: skips {case:?}: {reason}", extractor.name());
            }
            (*extractor, kit)
        })
        .collect()
}

fn skips(kit: &Fixture, case: Case) -> bool {
    kit.skip.iter().any(|(c, _)| *c == case)
}

#[test]
fn the_pairs_from_a_file_importing_another_are_exactly_that_one() {
    for (extractor, kit) in kits() {
        let found = run_on(extractor, &anchor_files(&kit));
        let (a, b) = ((kit.source)("a"), (kit.source)("b"));
        let from_a: Vec<(&str, &str)> = found
            .pairs
            .iter()
            .filter(|p| p.from == a)
            .map(|p| (p.from.as_str(), p.to.as_str()))
            .collect();
        assert_eq!(
            from_a,
            [(a.as_str(), b.as_str())],
            "{}: {found:?}",
            extractor.name()
        );
        assert_eq!(found.problems, Vec::<String>::new(), "{}", extractor.name());
    }
}

#[test]
fn a_file_that_is_not_utf8_does_not_panic() {
    for (extractor, kit) in kits() {
        let mut files = anchor_files(&kit);
        let c = (kit.source)("c");
        let mut bytes = vec![0xff, 0xfe, b'\n'];
        bytes.extend((kit.importing)(&c, &(kit.source)("b")).into_bytes());
        bytes.extend([0xc3, b'\n']);
        files.push((c, bytes));
        run_on(extractor, &files);
    }
}

#[test]
fn a_missing_config_gives_what_the_kit_declares() {
    for (extractor, kit) in kits() {
        let without = |file: &str| -> Output {
            let files: Vec<_> = anchor_files(&kit)
                .into_iter()
                .filter(|(f, _)| f != file)
                .collect();
            run_on(extractor, &files)
        };
        match &kit.missing_config {
            MissingConfig::Fallback { file } => {
                let found = without(file);
                assert_eq!(found.problems, Vec::<String>::new(), "{}", extractor.name());
            }
            MissingConfig::Problem { file, says } => {
                let found = without(file);
                assert!(
                    found.problems.iter().any(|p| p.contains(says)),
                    "{}: no problem says {says:?}: {:?}",
                    extractor.name(),
                    found.problems
                );
            }
        }
    }
}

#[test]
fn a_malformed_config_gives_at_least_one_problem() {
    for (extractor, kit) in kits() {
        let (MissingConfig::Fallback { file: config }
        | MissingConfig::Problem { file: config, .. }) = kit.missing_config;
        let Some(text) = kit.malformed_config else {
            assert!(
                skips(&kit, Case::MalformedConfig),
                "{}: has no malformed config, so skip the case with a reason",
                extractor.name()
            );
            continue;
        };
        let files: Vec<_> = anchor_files(&kit)
            .into_iter()
            .map(|(f, bytes)| {
                if f == config {
                    (f, text.as_bytes().to_vec())
                } else {
                    (f, bytes)
                }
            })
            .collect();
        let found = run_on(extractor, &files);
        assert!(
            !found.problems.is_empty(),
            "{}: {found:?}",
            extractor.name()
        );
    }
}
