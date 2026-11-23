//! Broken laws (spec danielrispler/plotkeeper#306, ticket #312): the latest finished run of the project's CI on its default
//! branch, read through `gh`, and which of the laws' tests failed in it.
//!
//! A law is held by the test function that follows its `@law: <id>` tag. When the run failed, the failed log is read for the
//! failing tests of `cargo nextest` and of Node's test runner, and only a failure of a tagged test marks its law's part. A
//! missing mark means "no known failure" and nothing more: a run that failed before the law tests ran (fmt, clippy, build), in
//! untagged tests only, or with a log that cannot be read, marks nothing and says so in `status`.
//!
//! `read` asks GitHub; the shell calls it only when the Map view opens, when the window gains focus while it is open and when a
//! merge is seen (`refresh_later`), never on a timer, and `CiCache` keeps the last result for the payload and for Home's mark.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};
use system_map::LawTest;

/// The CI workflow whose run holds the law tests: the Studio's `npm run check` job.
pub const WORKFLOW: &str = "studio.yml";
/// The most lines of a test "See the proof" shows.
const SNIPPET: usize = 40;
/// How many lines of the failed log the drill-down keeps, from its end.
const TAIL: usize = 60;

/// One failing test named in a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// `cargo nextest`: the test's path in its binary, such as `tests::unit_fails`; Node: its title.
    pub name: String,
    /// `cargo nextest` only: the binary's id, such as `studio-understanding::wire` or a bare crate name.
    pub binary: Option<String>,
    /// Node only: the file the test is in, when the log says.
    pub file: Option<String>,
}

/// The lines of a `gh run view --log-failed` log as the runner printed them: no job and step columns, no timestamp, no colour.
pub fn clean_lines(log: &str) -> Vec<String> {
    log.lines().map(clean_line).collect()
}

fn clean_line(line: &str) -> String {
    // `job<TAB>step<TAB>timestamp text`.
    let mut columns = line.splitn(3, '\t');
    let text = match (columns.next(), columns.next(), columns.next()) {
        (Some(_), Some(_), Some(rest)) => rest.strip_prefix('\u{feff}').unwrap_or(rest),
        _ => line,
    };
    let text = match text.split_once(' ') {
        Some((stamp, rest)) if is_timestamp(stamp) => rest,
        _ => text,
    };
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
        } else if chars.next_if_eq(&'[').is_some() {
            // A colour or cursor code ends at its letter.
            while chars.next().is_some_and(|c| !c.is_ascii_alphabetic()) {}
        } else {
            chars.next();
        }
    }
    out
}

fn is_timestamp(word: &str) -> bool {
    word.len() >= 20
        && word.ends_with('Z')
        && word.as_bytes().get(4) == Some(&b'-')
        && word.as_bytes().get(10) == Some(&b'T')
}

/// The failing tests a log names, from `cargo nextest` (`FAIL [..] binary name`) and Node's test runner (TAP `not ok N - title`
/// with its `location:`, or the spec reporter's `✖ title`).
pub fn parse_failures(log: &str) -> Vec<Failure> {
    let mut found: Vec<Failure> = Vec::new();
    // A TAP failure waits for its `location:`; a spec failure takes the `test at` line before it.
    let mut open_tap: Option<usize> = None;
    let mut spec_file: Option<String> = None;
    for line in clean_lines(log) {
        let text = line.trim();
        if let Some(rest) = text.strip_prefix("not ok ") {
            open_tap = None;
            let title = rest
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .trim_start()
                .strip_prefix("- ");
            if let Some(title) = title {
                open_tap = Some(push(
                    &mut found,
                    Failure {
                        name: title.trim().to_string(),
                        binary: None,
                        file: None,
                    },
                ));
            }
        } else if let Some(rest) = text.strip_prefix("location: ") {
            if let Some(at) = open_tap.take() {
                found[at].file = Some(location_file(rest));
            }
        } else if let Some(rest) = text.strip_prefix("test at ") {
            spec_file = Some(location_file(rest));
        } else if let Some(rest) = text.strip_prefix('\u{2716}') {
            let name = strip_duration(rest.trim());
            // The summary's own heading.
            if name == "failing tests:" {
                continue;
            }
            let file = spec_file.take();
            // The summary lists a failure again with its file: that one is the same test.
            match found
                .iter_mut()
                .find(|f| f.binary.is_none() && f.name == name)
            {
                Some(again) => again.file = again.file.take().or(file),
                None => {
                    push(
                        &mut found,
                        Failure {
                            name: name.to_string(),
                            binary: None,
                            file,
                        },
                    );
                }
            }
        } else if let Some(failure) = nextest_failure(text) {
            push(&mut found, failure);
        }
    }
    found
}

/// Adds `failure` unless it is there already; the index it has in `found` either way.
fn push(found: &mut Vec<Failure>, failure: Failure) -> usize {
    if let Some(at) = found.iter().position(|f| *f == failure) {
        return at;
    }
    found.push(failure);
    found.len() - 1
}

/// `'/a/b.mjs:5:1'` or `a/b.mjs:5:1`: the file.
fn location_file(text: &str) -> String {
    let text = text.trim().trim_matches('\'');
    let text = text.strip_prefix("file://").unwrap_or(text);
    let mut parts = text.rsplitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(a), Some(b), Some(file))
            if a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()) =>
        {
            file.to_string()
        }
        _ => text.to_string(),
    }
}

/// `title (0.27ms)` -> `title`.
fn strip_duration(text: &str) -> &str {
    match text.rfind(" (") {
        Some(at) if text.ends_with("ms)") || text.ends_with("s)") => &text[..at],
        _ => text,
    }
}

/// `FAIL [   0.013s] (2/4) studio-nx::wire unrelated_in_same_file`; a retry (`TRY 1 FAIL`) and a leak are not read.
fn nextest_failure(text: &str) -> Option<Failure> {
    let (status, rest) = text.split_once(' ')?;
    if !(status == "FAIL" || status == "TIMEOUT" || status.starts_with("SIG")) {
        return None;
    }
    let rest = rest.trim_start().strip_prefix('[')?;
    let mut rest = rest.split_once(']')?.1.trim_start();
    if rest.starts_with('(') {
        rest = rest.split_once(')')?.1.trim_start();
    }
    let (binary, name) = rest.split_once(' ')?;
    let name = name.trim();
    (!name.is_empty()).then(|| Failure {
        name: name.to_string(),
        binary: Some(binary.to_string()),
        file: None,
    })
}

/// The law ids whose tagged test is among `failures`. A nextest failure matches a Rust test by its function name and, when the
/// binary is an integration test, by the file it is in; a Node failure matches a test by its title and, when the log names it, its
/// file. Another test in the same file never matches.
pub fn broken_laws(failures: &[Failure], tests: &[LawTest]) -> Vec<String> {
    let mut laws: Vec<String> = Vec::new();
    for test in tests
        .iter()
        .filter(|test| failures.iter().any(|failure| holds(failure, test)))
    {
        if !laws.contains(&test.law) {
            laws.push(test.law.clone());
        }
    }
    laws
}

fn holds(failure: &Failure, test: &LawTest) -> bool {
    let Some(name) = &test.test else { return false };
    let file = &test.tag.file;
    match &failure.binary {
        Some(binary) => file.ends_with(".rs") && rust_holds(binary, &failure.name, file, name),
        None => {
            !file.ends_with(".rs")
                && failure.name == *name
                && failure
                    .file
                    .as_ref()
                    .is_none_or(|seen| seen == file || seen.ends_with(&format!("/{file}")))
        }
    }
}

/// Whether a nextest failure `name` in `binary` is the Rust test `test` of the tagged `file`. The file's crate is its first folder
/// (`core` is `studio-core`, `src-tauri` is `studio`). An integration test file `<crate>/tests/<module>.rs` is a module of the
/// crate's one `it` binary (`studio-core::it`, test `<module>::<fn>`); a source file holds unit tests in the bare crate binary
/// (`<module>::tests::<fn>`), and so does a binary target's `bin/<name>` id (the studio crate's own).
fn rust_holds(binary: &str, name: &str, file: &str, test: &str) -> bool {
    if name.rsplit("::").next() != Some(test) {
        return false;
    }
    let (krate, test_binary) = match binary.split_once("::") {
        Some((krate, test_binary)) => (krate, Some(test_binary)),
        None => (binary, None),
    };
    // A binary target's unit tests are `<crate>::bin/<name>`: unit tests of the crate's sources, like the bare library id.
    let test_binary = test_binary.filter(|id| !id.starts_with("bin/"));
    let Some((dir, rest)) = file.split_once('/') else {
        return false;
    };
    let dir_crate = if dir == "src-tauri" {
        "studio".to_string()
    } else {
        format!("studio-{dir}")
    };
    if krate != dir && krate != dir_crate {
        return false;
    }
    let module = |path: &str| -> Vec<String> {
        let path = path.trim_end_matches(".rs");
        let mut parts: Vec<&str> = path.split('/').collect();
        if matches!(parts.last(), Some(&"mod" | &"lib" | &"main")) {
            parts.pop();
        }
        parts.into_iter().map(str::to_string).collect()
    };
    let path_in_name: Vec<&str> = name.split("::").collect();
    let path_in_name = &path_in_name[..path_in_name.len() - 1];
    match (rest.strip_prefix("tests/"), test_binary) {
        // `tests/<module path>.rs` of the `it` binary.
        (Some(tests_path), Some("it")) => module(tests_path) == path_in_name,
        // A binary named for its own test file, or for a folder of it.
        (Some(tests_path), Some(test_binary)) => tests_path
            .trim_end_matches(".rs")
            .split('/')
            .any(|part| part == test_binary),
        (Some(_), None) => false,
        (None, None) => {
            let module = module(rest.strip_prefix("src/").unwrap_or(rest));
            // `src/a/b.rs` holds `a::b::tests::<fn>`; the crate root holds `tests::<fn>`.
            path_in_name.starts_with(&module.iter().map(String::as_str).collect::<Vec<_>>())
        }
        (None, Some(_)) => false,
    }
}

/// What a finished run says of the laws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// There is no finished run to read, or `gh` could not be asked.
    Unknown,
    /// The run passed.
    Passed,
    /// A test that backs a law failed.
    Broken,
    /// The run failed, but not in a test that backs a law: the law tests may not have run.
    NotRun,
    /// The run failed and its log could not be fetched or holds nothing to read.
    Unreadable,
}

/// A failed run's log judged against the laws' tests.
#[derive(Debug, PartialEq)]
pub struct Assessment {
    pub status: Status,
    pub laws: Vec<String>,
    pub tail: String,
}

/// Judges the failed log of a run (`None` when it could not be fetched) against `tests`.
pub fn assess(log: Option<&str>, tests: &[LawTest]) -> Assessment {
    let lines = log.map(clean_lines).unwrap_or_default();
    if lines.iter().all(|line| line.trim().is_empty()) {
        return Assessment {
            status: Status::Unreadable,
            laws: Vec::new(),
            tail: String::new(),
        };
    }
    let tail = lines[lines.len().saturating_sub(TAIL)..].join("\n");
    let laws = broken_laws(&parse_failures(log.unwrap_or_default()), tests);
    let status = if laws.is_empty() {
        Status::NotRun
    } else {
        Status::Broken
    };
    Assessment { status, laws, tail }
}

/// One run of the workflow, as `gh run list --json` gives it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub database_id: u64,
    /// `completed`, `in_progress`, `queued` and the like.
    pub status: String,
    /// Set once the run finished: `success`, `failure`, `cancelled`, `skipped`...
    pub conclusion: Option<String>,
    pub head_sha: String,
    pub url: String,
}

/// The runs `gh run list --json databaseId,status,conclusion,headSha,url` printed, newest first.
pub fn parse_runs(json: &str) -> Result<Vec<Run>, String> {
    serde_json::from_str(json).map_err(|e| format!("gh's run list does not read: {e}"))
}

/// The latest run that finished with a result (passed or failed; a cancelled or skipped one says nothing), and whether a newer
/// run is still going, so its result is for an earlier commit.
pub fn pick(runs: &[Run]) -> Option<(&Run, bool)> {
    let at = runs.iter().position(|run| {
        run.status == "completed"
            && matches!(run.conclusion.as_deref(), Some("success" | "failure"))
    })?;
    let newer = runs[..at].iter().any(|run| run.status != "completed");
    Some((&runs[at], newer))
}

/// What the window shows of CI on the default branch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ci {
    pub status: Status,
    /// The finished run's page on GitHub.
    #[serde(default)]
    pub run_url: Option<String>,
    /// The commit that run was for.
    #[serde(default)]
    pub commit: Option<String>,
    /// A newer run is still going: this result is for an earlier commit.
    #[serde(default)]
    pub earlier: bool,
    /// The parts that own a broken law's test: only when `status` is `broken`.
    #[serde(default)]
    pub parts: Vec<String>,
    /// The broken laws' ids, for the drill-down's own use.
    #[serde(default)]
    pub laws: Vec<String>,
    /// The end of the failed log.
    #[serde(default)]
    pub tail: String,
    /// Why `status` is `unknown`.
    #[serde(default)]
    pub note: Option<String>,
}

impl Ci {
    fn unknown(note: impl Into<String>) -> Ci {
        Ci {
            status: Status::Unknown,
            run_url: None,
            commit: None,
            earlier: false,
            parts: Vec::new(),
            laws: Vec::new(),
            tail: String::new(),
            note: Some(note.into()),
        }
    }
}

/// The tests the tags of `at` (a ref or commit) sit above: every `@law: <id>` comment line of its own, with the test function after it.
pub fn law_tests_at(repo: &Path, at: &str) -> Vec<LawTest> {
    // No match is git's exit 1, as is a ref it does not know: neither tags anything.
    let Ok(found) = crate::map::git(
        repo,
        &["grep", "-n", "-I", "-F", "--no-color", "@law:", at, "--"],
    ) else {
        return Vec::new();
    };
    let prefix = format!("{at}:");
    let mut tags: Vec<(String, usize, String)> = Vec::new();
    for line in found.lines() {
        let Some(line) = line.strip_prefix(&prefix) else {
            continue;
        };
        let mut parts = line.splitn(3, ':');
        let (Some(file), Some(number), Some(text)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if let (Some(id), Ok(number)) = (system_map::tag_id(text), number.parse::<usize>()) {
            tags.push((file.to_string(), number, id.to_string()));
        }
    }
    let mut blobs: HashMap<String, Option<String>> = HashMap::new();
    tags.into_iter()
        .map(|(file, line, law)| {
            let blob = blobs
                .entry(file.clone())
                .or_insert_with(|| crate::map::git(repo, &["show", &format!("{at}:{file}")]).ok());
            let test = blob
                .as_deref()
                .and_then(|text| system_map::test_name_after(text, line));
            LawTest {
                law,
                tag: system_map::Tag { file, line },
                test,
            }
        })
        .collect()
}

/// A tagged test, for "See the proof".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proof {
    pub file: String,
    /// The tag's line, 1-based.
    pub line: usize,
    pub test: Option<String>,
    /// The line `snippet` starts at.
    pub first: usize,
    /// The tag and the test that follows it.
    pub snippet: String,
}

/// The tag line and the test after it, up to the test's closing line (the first line back at its indentation that closes it), and
/// at most `SNIPPET` lines. The line the snippet starts at (1-based), and the text.
pub fn snippet(text: &str, tag_line: usize) -> (usize, String) {
    let lines: Vec<&str> = text.lines().collect();
    let from = tag_line.saturating_sub(1).min(lines.len());
    let mut limit = (from + SNIPPET).min(lines.len());
    let mut base: Option<usize> = None; // The indentation of the test's first line, past attributes and comments.
    let mut depth = 0i32;
    for (at, raw) in lines.iter().enumerate().skip(from + 1).take(SNIPPET) {
        let (trimmed, indent) = (raw.trim(), raw.len() - raw.trim_start().len());
        match base {
            None if depth > 0 || trimmed.starts_with("#[") || trimmed.starts_with("#![") => {
                depth += trimmed.matches('[').count() as i32 - trimmed.matches(']').count() as i32;
            }
            None if trimmed.is_empty() || trimmed.starts_with("//") => {}
            None => {
                base = Some(indent);
                // A test written on one line closes there.
                if trimmed.ends_with('}') || trimmed.ends_with(");") {
                    limit = at + 1;
                    break;
                }
            }
            Some(base)
                if indent <= base && (trimmed.starts_with('}') || trimmed.starts_with(')')) =>
            {
                limit = at + 1;
                break;
            }
            Some(_) => {}
        }
    }
    (from + 1, lines[from..limit].join("\n"))
}

/// The tests that back `law` on the default branch, with their code.
pub fn proofs(repo: &Path, law: &str) -> Vec<Proof> {
    let Some((at, _)) = crate::map::default_ref(repo) else {
        return Vec::new();
    };
    law_tests_at(repo, &at)
        .into_iter()
        .filter(|test| test.law == law)
        .filter_map(|test| {
            let text = crate::map::git(repo, &["show", &format!("{at}:{}", test.tag.file)]).ok()?;
            let (first, snippet) = snippet(&text, test.tag.line);
            Some(Proof {
                file: test.tag.file,
                line: test.tag.line,
                test: test.test,
                first,
                snippet,
            })
        })
        .collect()
}

/// Asks GitHub for the default branch's latest finished run, and for a failed one its log.
pub fn read(repo: &Path) -> Ci {
    let Some((reference, _)) = crate::map::default_ref(repo) else {
        return Ci::unknown("this repository has no default branch");
    };
    let branch = reference.strip_prefix("origin/").unwrap_or(&reference);
    let listed = gh(
        repo,
        &[
            "run",
            "list",
            "--branch",
            branch,
            "--workflow",
            WORKFLOW,
            "--limit",
            "20",
            "--json",
            "databaseId,status,conclusion,headSha,url",
        ],
    );
    let runs = match listed.and_then(|json| parse_runs(&json)) {
        Ok(runs) => runs,
        Err(why) => return Ci::unknown(why),
    };
    let Some((run, earlier)) = pick(&runs) else {
        return Ci::unknown(format!("no finished run of {WORKFLOW} on {branch}"));
    };
    let mut ci = Ci {
        status: Status::Passed,
        run_url: Some(run.url.clone()),
        commit: Some(run.head_sha.clone()),
        earlier,
        parts: Vec::new(),
        laws: Vec::new(),
        tail: String::new(),
        note: None,
    };
    if run.conclusion.as_deref() == Some("success") {
        return ci;
    }
    let log = gh(
        repo,
        &["run", "view", &run.database_id.to_string(), "--log-failed"],
    )
    .ok();
    // The tags as they stood in that run, when this clone has its commit.
    let at = if crate::map::git(
        repo,
        &["cat-file", "-e", &format!("{}^{{commit}}", run.head_sha)],
    )
    .is_ok()
    {
        run.head_sha.clone()
    } else {
        reference
    };
    let found = assess(log.as_deref(), &law_tests_at(repo, &at));
    ci.status = found.status;
    ci.parts = parts_of(repo, &found.laws);
    ci.laws = found.laws;
    ci.tail = found.tail;
    ci
}

/// The ids of the parts that hold `laws` on the default branch's map, in the map's order; a law on a link marks no part.
fn parts_of(repo: &Path, laws: &[String]) -> Vec<String> {
    let Some(source) = crate::map::read_map(repo).source else {
        return Vec::new();
    };
    let Ok(map) = serde_json::from_str::<serde_json::Value>(&source) else {
        return Vec::new();
    };
    let on: HashSet<&str> = map["laws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|law| {
            law["id"]
                .as_str()
                .is_some_and(|id| laws.iter().any(|l| l == id))
        })
        .filter_map(|law| law["on"].as_str())
        .collect();
    map["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| part["id"].as_str())
        .filter(|id| on.contains(id))
        .map(String::from)
        .collect()
}

fn gh(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(studio_workbench::gh_path())
        .args(args)
        .current_dir(repo)
        .env("GH_PROMPT_DISABLED", "1")
        .output()
        .map_err(|e| format!("gh cannot run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "gh: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The last CI read of each project: the payload and Home's mark read it, and only a trigger replaces it. The last finished
/// result is also kept in `file` (when there is one), so a restart does not lose Home's mark before the map is opened.
#[derive(Default)]
pub struct CiCache {
    last: Mutex<BTreeMap<String, Ci>>,
    running: Mutex<HashSet<String>>,
    file: Option<PathBuf>,
}

/// Marks a read of its project as no longer running when dropped, also when the read panics.
struct Running<'a>(&'a Mutex<HashSet<String>>, String);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.1);
    }
}

impl CiCache {
    /// A cache seeded from `file`: a missing or unreadable file, or an entry that does not read, seeds nothing.
    pub fn load(file: PathBuf) -> CiCache {
        let stored: BTreeMap<String, serde_json::Value> = std::fs::read_to_string(&file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let last = stored
            .into_iter()
            .filter_map(|(project, ci)| Some((project, serde_json::from_value::<Ci>(ci).ok()?)))
            .collect();
        CiCache {
            last: Mutex::new(last),
            running: Mutex::default(),
            file: Some(file),
        }
    }

    pub fn get(&self, project: &str) -> Option<Ci> {
        self.last.lock().unwrap().get(project).cloned()
    }

    /// Whether a read is on record for `project`.
    pub fn has(&self, project: &str) -> bool {
        self.last.lock().unwrap().contains_key(project)
    }

    /// Keeps `ci`, except that an answer of none (`gh` failed) never replaces a real result. Whether the record changed.
    pub fn put(&self, project: &str, ci: Ci) -> bool {
        let mut last = self.last.lock().unwrap();
        if ci.status == Status::Unknown
            && last
                .get(project)
                .is_some_and(|old| old.status != Status::Unknown)
        {
            return false;
        }
        let changed = last.insert(project.to_string(), ci.clone()) != Some(ci);
        if changed && let Some(file) = &self.file {
            self.save(file, &last);
        }
        changed
    }

    /// Writes the finished results; a failure is logged and the memory stays the truth.
    fn save(&self, file: &Path, last: &BTreeMap<String, Ci>) {
        let finished: BTreeMap<&String, &Ci> = last
            .iter()
            .filter(|(_, ci)| ci.status != Status::Unknown)
            .collect();
        let write = || -> Result<(), String> {
            let text = serde_json::to_string(&finished).map_err(|e| e.to_string())?;
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let tmp = file.with_extension("json.tmp");
            std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, file).map_err(|e| e.to_string())
        };
        if let Err(e) = write() {
            log::warn!("ci: cannot keep the last result: {e}");
        }
    }

    /// Whether `project` is marked: a part owns a broken law's test.
    pub fn broken(&self, project: &str) -> bool {
        self.get(project).is_some_and(|ci| !ci.parts.is_empty())
    }
}

/// The cache seeded from Studio's data folder, or an empty one when there is no home folder.
pub fn load_cache() -> CiCache {
    std::env::home_dir().map_or_else(CiCache::default, |home| {
        CiCache::load(studio_hook::data_dir(&home).join("ci-last.json"))
    })
}

/// Reads CI for `project` in the background and tells the window (`map-landed`) when the result changed. One read per project at a
/// time; a trigger that finds one running adds nothing.
pub fn refresh_later(cache: &Arc<CiCache>, events: &Arc<crate::Events>, project: &str) {
    if !cache.running.lock().unwrap().insert(project.to_string()) {
        return;
    }
    let (cache, events, project) = (cache.clone(), events.clone(), project.to_string());
    std::thread::spawn(move || {
        let _running = Running(&cache.running, project.clone());
        let ci = read(Path::new(&project));
        if cache.put(&project, ci) {
            events.emit("map-landed", project.into());
        }
    });
}

/// How a failed local check reads on the rail's evidence line (spec #334, ticket #345): the first failing test of its output
/// tail, with the part that owns the test's file, then "(+N more)"; with no test parsed, the step that failed.
///
/// `tail` is the check's output tail, `suite` its name (`cargo fmt --check`) and `map_source` the project's map, when it has one.
/// Not the sentence's mark: the line adds "✗ " itself.
pub fn name_failed_check(tail: &str, suite: &str, map_source: Option<&str>) -> String {
    let failures = parse_failures(tail);
    let Some(first) = failures.first() else {
        return format!("{} failed", failed_step(tail, suite));
    };
    let test = plain_test(first);
    let more = match failures.len() - 1 {
        0 => String::new(),
        n => format!(" (+{n} more)"),
    };
    match part_name(first, map_source) {
        Some(part) => format!("{part} tests failed: {test}{more}"),
        None => format!("Test failed: {test}{more}"),
    }
}

/// A test's name in words: Node's title as written; a Rust test's last path segment with its underscores as spaces.
fn plain_test(failure: &Failure) -> String {
    match failure.binary {
        Some(_) => failure
            .name
            .rsplit("::")
            .next()
            .unwrap_or(&failure.name)
            .replace('_', " "),
        None => failure.name.clone(),
    }
}

/// The name of the part of the map that owns the failing test's file.
fn part_name(failure: &Failure, map_source: Option<&str>) -> Option<String> {
    let source = map_source?;
    let map = system_map::parse_lenient(source).ok()?;
    let id = test_paths(failure).into_iter().find_map(|path| {
        // A log from another machine names the file by its own folders: drop leading folders until a part owns the rest.
        let mut rest = path.as_str();
        loop {
            let found = system_map::touched_for_map(&map, &[rest.to_string()]);
            if let Some(part) = found.parts.into_iter().next() {
                return Some(part.id);
            }
            rest = rest.split_once('/')?.1;
        }
    })?;
    let names: serde_json::Value = serde_json::from_str(source).ok()?;
    names["parts"]
        .as_array()?
        .iter()
        .find(|part| part["id"].as_str() == Some(id.as_str()))
        .and_then(|part| part["name"].as_str())
        .map(String::from)
}

/// Where a failing test's file is: Node's own file; a nextest binary's crate folder, with its integration test file when the
/// binary names one (the `it` binary's by the test's module). The crate's folder is as `rust_holds` reads it.
fn test_paths(failure: &Failure) -> Vec<String> {
    let Some(binary) = &failure.binary else {
        return failure.file.iter().cloned().collect();
    };
    let (krate, test_binary) = match binary.split_once("::") {
        Some((krate, id)) => (krate, Some(id)),
        None => (binary.as_str(), None),
    };
    let dir = match krate {
        "studio" => "src-tauri",
        other => other.strip_prefix("studio-").unwrap_or(other),
    };
    match test_binary.filter(|id| !id.starts_with("bin/")) {
        Some("it") => {
            let module = failure.name.split("::").next().unwrap_or_default();
            vec![format!("{dir}/tests/{module}.rs")]
        }
        Some(id) => vec![format!("{dir}/tests/{id}.rs")],
        None => vec![format!("{dir}/src/lib.rs")],
    }
}

/// The step a check that ran no failing test broke on, in words.
fn failed_step(tail: &str, suite: &str) -> String {
    let suite_has = |word: &str| suite.contains(word);
    let tail_has = |text: &str| tail.contains(text);
    let step = if suite_has("fmt") || suite_has("format") || tail_has("Diff in ") {
        "formatting check"
    } else if suite_has("clippy") || suite_has("lint") || tail_has("clippy") {
        "lint check"
    } else if suite_has("build")
        || tail_has("could not compile")
        || tail_has("error[E")
        || tail_has("error TS")
    {
        "build"
    } else {
        return suite.to_string();
    };
    step.to_string()
}

/// The words for a failed check's output tail, read on the project's own map (the default branch's). Off the studio's lock.
pub fn failed_check_name(tail: String, suite: String, worktree: String) -> String {
    let map = crate::map::read_map(Path::new(&worktree)).source;
    name_failed_check(&tail, &suite, map.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use system_map::Tag;

    const NEXTEST: &str = include_str!("../tests/ci-logs/nextest-failed.log");
    const NEXTEST_FAIL_FAST: &str = include_str!("../tests/ci-logs/nextest-fail-fast.log");
    const NODE_TAP: &str = include_str!("../tests/ci-logs/node-tap-failed.log");
    const NODE_SPEC: &str = include_str!("../tests/ci-logs/node-spec-failed.log");
    const NEXTEST_REAL: &str = include_str!("../tests/ci-logs/nextest-real-binaries.log");
    const FMT: &str = include_str!("../tests/ci-logs/fmt-failed.log");

    fn tagged(law: &str, file: &str, test: &str) -> LawTest {
        LawTest {
            law: law.into(),
            tag: Tag {
                file: file.into(),
                line: 1,
            },
            test: Some(test.into()),
        }
    }
    fn names(failures: &[Failure]) -> Vec<&str> {
        failures.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn the_runners_prefix_and_colour_are_stripped_from_each_line() {
        let log = "macos\tRun npm run check\t2026-11-22T09:12:01.1234567Z \u{1b}[31mFAIL\u{1b}[0m [   0.1s] a b\nplain line";
        assert_eq!(clean_lines(log), vec!["FAIL [   0.1s] a b", "plain line"]);
    }

    #[test]
    fn a_nextest_log_names_every_failing_test_once_with_its_binary() {
        let found = parse_failures(NEXTEST);
        assert_eq!(
            names(&found),
            [
                "unrelated_in_same_file",
                "tests::unit_fails",
                "wire_types_match"
            ]
        );
        assert_eq!(found[0].binary.as_deref(), Some("studio-nx::wire"));
        assert_eq!(found[1].binary.as_deref(), Some("studio-nx"));
    }

    #[test]
    fn a_node_tap_log_names_each_failing_test_with_its_file() {
        let found = parse_failures(NODE_TAP);
        assert_eq!(
            names(&found),
            ["a delete keeps the live terminal", "an unrelated one fails"]
        );
        assert_eq!(
            found[0].file.as_deref(),
            Some("/Users/runner/work/plotkeeper/plotkeeper/ui/tests/t.test.mjs")
        );
        assert_eq!(found[0].binary, None);
    }

    #[test]
    fn a_tap_failure_titled_like_an_earlier_one_does_not_panic_on_its_location() {
        let log = "not ok 1 - dup\nnot ok 2 - dup\n  location: 'ui/tests/a.test.mjs:5:1'\n";
        let found = parse_failures(log);
        assert_eq!(names(&found), ["dup"]);
    }

    #[test]
    fn a_node_spec_log_names_each_failing_test_with_its_file() {
        let found = parse_failures(NODE_SPEC);
        assert_eq!(
            names(&found),
            ["a delete keeps the live terminal", "an unrelated one fails"]
        );
        assert_eq!(found[1].file.as_deref(), Some("ui/tests/t.test.mjs"));
    }

    #[test]
    fn a_formatting_failure_names_no_test() {
        assert!(parse_failures(FMT).is_empty());
    }

    #[test]
    fn only_the_tagged_test_of_a_nextest_log_breaks_its_law() {
        let tests = [tagged(
            "wire-types-agree",
            "studio-nx/tests/wire.rs",
            "wire_types_match",
        )];
        assert_eq!(
            broken_laws(&parse_failures(NEXTEST), &tests),
            ["wire-types-agree"]
        );
        assert_eq!(
            broken_laws(&parse_failures(NEXTEST_FAIL_FAST), &tests),
            ["wire-types-agree"]
        );
    }

    #[test]
    fn a_failure_in_this_workspaces_it_binary_breaks_the_law_tagged_in_its_test_file() {
        let failures = parse_failures(NEXTEST_REAL);
        let tests = [tagged(
            "wire-types-agree",
            "core/tests/wire.rs",
            "the_wire_samples_of_the_core_are_written_for_tsc",
        )];
        assert_eq!(broken_laws(&failures, &tests), ["wire-types-agree"]);
        // The same function in another test file, another crate, or a passing one breaks nothing.
        for (file, test) in [
            (
                "core/tests/other.rs",
                "the_wire_samples_of_the_core_are_written_for_tsc",
            ),
            (
                "understanding/tests/wire.rs",
                "the_wire_samples_of_the_core_are_written_for_tsc",
            ),
            ("core/tests/wire.rs", "passes"),
        ] {
            assert!(
                broken_laws(&failures, &[tagged("l", file, test)]).is_empty(),
                "{file} {test}"
            );
        }
        // The studio crate has only `src/main.rs`, so nextest names its unit tests `studio::bin/studio`; they belong to `src/` files.
        assert_eq!(
            broken_laws(
                &failures,
                &[tagged("b", "src-tauri/src/answer.rs", "bin_unit_fails")]
            ),
            ["b"]
        );
        for (file, test) in [
            ("src-tauri/src/other.rs", "bin_unit_fails"),
            ("src-tauri/tests/answer.rs", "bin_unit_fails"),
        ] {
            assert!(
                broken_laws(&failures, &[tagged("b", file, test)]).is_empty(),
                "{file}"
            );
        }
        // A unit test of the crate's own source: bare crate binary, module path.
        let unit = [tagged("u", "core/src/ids.rs", "unit_fails")];
        assert_eq!(broken_laws(&failures, &unit), ["u"]);
        let other = [tagged("u", "core/src/other.rs", "unit_fails")];
        assert!(broken_laws(&failures, &other).is_empty());
    }

    #[test]
    fn an_unrelated_failing_test_in_the_same_file_breaks_no_law() {
        let tests = [tagged(
            "wire-types-agree",
            "studio-nx/tests/wire.rs",
            "passes",
        )];
        assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
        let tests = [tagged(
            "delete-keeps-live-terminal",
            "ui/tests/t.test.mjs",
            "a delete keeps the live terminal and more",
        )];
        assert!(broken_laws(&parse_failures(NODE_TAP), &tests).is_empty());
    }

    #[test]
    fn a_test_of_the_same_name_in_another_binary_breaks_no_law() {
        let tests = [tagged("l", "studio-nx/tests/other.rs", "wire_types_match")];
        assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
        // A Node test never matches a Rust failure, nor the other way round.
        let tests = [
            tagged("l", "ui/tests/t.test.mjs", "wire_types_match"),
            tagged("m", "src/lib.rs", "a delete keeps the live terminal"),
        ];
        assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
        assert!(broken_laws(&parse_failures(NODE_TAP), &tests).is_empty());
    }

    #[test]
    fn a_failing_node_test_breaks_the_law_tagged_above_it() {
        let tests = [tagged(
            "delete-keeps-live-terminal",
            "ui/tests/t.test.mjs",
            "a delete keeps the live terminal",
        )];
        assert_eq!(
            broken_laws(&parse_failures(NODE_TAP), &tests),
            ["delete-keeps-live-terminal"]
        );
        assert_eq!(
            broken_laws(&parse_failures(NODE_SPEC), &tests),
            ["delete-keeps-live-terminal"]
        );
        // The same title in another file is another test.
        let elsewhere = [tagged(
            "delete-keeps-live-terminal",
            "ui/tests/home.test.mjs",
            "a delete keeps the live terminal",
        )];
        assert!(broken_laws(&parse_failures(NODE_TAP), &elsewhere).is_empty());
    }

    #[test]
    fn a_tag_with_no_test_after_it_breaks_nothing() {
        let tests = [LawTest {
            law: "l".into(),
            tag: Tag {
                file: "a.rs".into(),
                line: 1,
            },
            test: None,
        }];
        assert!(broken_laws(&parse_failures(NEXTEST), &tests).is_empty());
    }

    #[test]
    fn a_failed_law_test_is_broken_and_keeps_the_log_tail() {
        let tests = [tagged(
            "wire-types-agree",
            "studio-nx/tests/wire.rs",
            "wire_types_match",
        )];
        let found = assess(Some(NEXTEST), &tests);
        assert_eq!(found.status, Status::Broken);
        assert_eq!(found.laws, ["wire-types-agree"]);
        assert!(found.tail.contains("Summary") && found.tail.lines().count() <= TAIL);
        assert!(!found.tail.contains('\t'), "the columns are stripped");
    }

    #[test]
    fn a_run_that_failed_before_the_tests_ran_says_the_law_tests_did_not_run() {
        let tests = [tagged(
            "wire-types-agree",
            "studio-nx/tests/wire.rs",
            "wire_types_match",
        )];
        let found = assess(Some(FMT), &tests);
        assert_eq!(found.status, Status::NotRun);
        assert!(found.laws.is_empty());
        assert!(found.tail.contains("Diff in"));
    }

    #[test]
    fn a_run_that_failed_only_in_untagged_tests_marks_nothing() {
        let tests = [tagged(
            "wire-types-agree",
            "studio-nx/tests/wire.rs",
            "passes",
        )];
        let found = assess(Some(NEXTEST), &tests);
        assert_eq!(found.status, Status::NotRun);
        assert!(found.laws.is_empty());
    }

    #[test]
    fn a_log_that_cannot_be_fetched_or_is_empty_is_unreadable_and_marks_nothing() {
        let tests = [tagged(
            "wire-types-agree",
            "studio-nx/tests/wire.rs",
            "wire_types_match",
        )];
        for log in [None, Some(""), Some("  \n\n")] {
            let found = assess(log, &tests);
            assert_eq!(found.status, Status::Unreadable);
            assert!(found.laws.is_empty());
        }
    }

    fn run(id: u64, status: &str, conclusion: Option<&str>) -> Run {
        Run {
            database_id: id,
            status: status.into(),
            conclusion: conclusion.map(String::from),
            head_sha: format!("sha{id}"),
            url: format!("https://github.com/o/r/actions/runs/{id}"),
        }
    }

    #[test]
    fn gh_run_list_json_reads_into_runs() {
        let json = r#"[{"conclusion":"failure","databaseId":42,"headSha":"abc","status":"completed","url":"https://github.com/o/r/actions/runs/42"},
                       {"conclusion":"","databaseId":41,"headSha":"abd","status":"in_progress","url":"u"}]"#;
        let runs = parse_runs(json).unwrap();
        assert_eq!(runs[0], run(42, "completed", Some("failure")).with("abc"));
        assert_eq!(runs[1].conclusion.as_deref(), Some(""));
        assert!(parse_runs("not json").is_err());
    }
    impl Run {
        fn with(mut self, sha: &str) -> Run {
            self.head_sha = sha.into();
            self.url = format!("https://github.com/o/r/actions/runs/{}", self.database_id);
            self
        }
    }

    #[test]
    fn the_latest_finished_run_is_read_and_a_newer_one_in_progress_says_it_is_for_an_earlier_commit()
     {
        let runs = [
            run(5, "in_progress", None),
            run(4, "completed", Some("cancelled")),
            run(3, "completed", Some("failure")),
            run(2, "completed", Some("success")),
        ];
        let (picked, earlier) = pick(&runs).unwrap();
        assert_eq!((picked.database_id, earlier), (3, true));
        let (picked, earlier) = pick(&runs[2..]).unwrap();
        assert_eq!((picked.database_id, earlier), (3, false));
        assert!(pick(&runs[..2]).is_none());
        assert!(pick(&[]).is_none());
    }

    const RUST_TESTS: &str = "use x;\n\n// @law: wire-types-agree\n#[test]\nfn wire_types_match() {\n    assert_eq!(1, 1);\n}\n\n#[test]\nfn another() {}\n";

    #[test]
    fn the_proof_is_the_tag_and_the_test_after_it_up_to_its_closing_line() {
        assert_eq!(snippet(RUST_TESTS, 3), (3, "// @law: wire-types-agree\n#[test]\nfn wire_types_match() {\n    assert_eq!(1, 1);\n}".to_string()));
        let node =
            "// @law: a\ntest(\"one\", () => {\n  assert.ok(1);\n});\ntest(\"two\", () => {});\n";
        assert_eq!(
            snippet(node, 1),
            (
                1,
                "// @law: a\ntest(\"one\", () => {\n  assert.ok(1);\n});".to_string()
            )
        );
        assert_eq!(
            snippet("// @law: a\n#[test]\nfn one() {}\nfn two() {}", 1).1,
            "// @law: a\n#[test]\nfn one() {}"
        );
    }

    #[test]
    fn a_proof_that_never_closes_stops_at_the_limit() {
        let text = format!("// @law: a\nfn long() {{\n{}", "    x;\n".repeat(200));
        assert_eq!(snippet(&text, 1).1.lines().count(), SNIPPET);
    }

    fn commit_file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
        studio_test_support::git(dir, &["add", name]);
        studio_test_support::git(dir, &["commit", "-q", "-m", name]);
    }

    #[test]
    fn the_tagged_tests_are_read_from_the_default_branch_not_the_working_tree() {
        let o = studio_test_support::origin();
        std::fs::create_dir_all(o.seed.join("tests")).unwrap();
        commit_file(&o.seed, "tests/wire.rs", RUST_TESTS);
        studio_test_support::git(&o.seed, &["push", "-q", "origin", "HEAD"]);
        studio_test_support::git(&o.main, &["fetch", "-q", "origin"]);
        std::fs::create_dir_all(o.main.join("tests")).unwrap();
        commit_file(&o.main, "tests/wire.rs", "// @law: other\nfn nope() {}\n");
        let found = law_tests_at(&o.main, "origin/trunk");
        assert_eq!(
            found,
            [LawTest {
                law: "wire-types-agree".into(),
                tag: system_map::Tag {
                    file: "tests/wire.rs".into(),
                    line: 3
                },
                test: Some("wire_types_match".into())
            }]
        );
        assert!(law_tests_at(&o.main, "origin/nowhere").is_empty());
        let map = r#"{"version":1,"parts":[{"id":"shell","name":"S","caption":"","description":"","tier":"core","paths":["tests/"]}],"actors":[],"links":[],"ignored":[],"laws":[{"id":"wire-types-agree","on":"shell","backing":"check","text":"t"}]}"#;
        commit_file(&o.seed, "system-map.json", map);
        studio_test_support::git(&o.seed, &["push", "-q", "origin", "HEAD"]);
        studio_test_support::git(&o.main, &["fetch", "-q", "origin"]);
        let proof = proofs(&o.main, "wire-types-agree");
        assert_eq!(
            (
                proof.len(),
                proof[0].file.as_str(),
                proof[0].line,
                proof[0].first
            ),
            (1, "tests/wire.rs", 3, 3)
        );
        assert!(proof[0].snippet.contains("fn wire_types_match"));
        assert!(proofs(&o.main, "no-such-law").is_empty());
        assert_eq!(
            parts_of(&o.main, &["wire-types-agree".to_string()]),
            ["shell"]
        );
        assert!(parts_of(&o.main, &["other".to_string()]).is_empty());
    }

    fn ci(status: Status, parts: &[&str]) -> Ci {
        Ci {
            status,
            run_url: None,
            commit: None,
            earlier: false,
            parts: parts.iter().map(|p| p.to_string()).collect(),
            laws: Vec::new(),
            tail: String::new(),
            note: None,
        }
    }

    #[test]
    fn the_last_finished_result_survives_a_restart_and_older_files_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ci-last.json");
        let cache = CiCache::load(file.clone());
        assert!(cache.put("/p", ci(Status::Broken, &["shell"])));
        assert!(cache.put("/q", ci(Status::Unknown, &[])));
        let again = CiCache::load(file.clone());
        assert!(again.broken("/p"));
        assert!(!again.has("/q"), "an unknown result is not kept");
        // An entry missing newer fields still reads; one that does not read is skipped.
        std::fs::write(
            &file,
            r#"{"/a":{"status":"broken","parts":["x"]},"/b":{"nonsense":1}}"#,
        )
        .unwrap();
        let older = CiCache::load(file.clone());
        assert!(older.broken("/a") && !older.has("/b"));
        std::fs::write(&file, "not json").unwrap();
        assert!(!CiCache::load(file).has("/a"));
    }

    #[test]
    fn the_last_result_is_kept_in_a_data_folder_that_does_not_exist_yet() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("fresh").join("ci-last.json");
        let cache = CiCache::load(file.clone());
        assert!(cache.put("/p", ci(Status::Broken, &["shell"])));
        assert!(CiCache::load(file).broken("/p"));
    }

    #[test]
    fn a_panicking_read_still_frees_its_project() {
        let cache = CiCache::default();
        cache.running.lock().unwrap().insert("/p".into());
        let freed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _running = Running(&cache.running, "/p".into());
            panic!("read panicked");
        }));
        assert!(freed.is_err());
        assert!(!cache.running.lock().unwrap().contains("/p"));
    }

    #[test]
    fn a_failed_read_never_replaces_a_real_result_and_a_mark_needs_a_part() {
        let cache = CiCache::default();
        assert!(!cache.broken("/p"));
        assert!(cache.put("/p", ci(Status::Broken, &["shell"])));
        assert!(
            !cache.put("/p", ci(Status::Broken, &["shell"])),
            "the same result changes nothing"
        );
        assert!(cache.broken("/p"));
        assert!(
            !cache.put("/p", ci(Status::Unknown, &[])),
            "gh failing keeps what was known"
        );
        assert!(cache.broken("/p"));
        assert!(cache.put("/p", ci(Status::NotRun, &[])));
        assert!(!cache.broken("/p"));
        assert!(!cache.has("/q") && cache.put("/q", ci(Status::Unknown, &[])) && cache.has("/q"));
    }

    const MAP: &str = r#"{"version":1,"parts":[
      {"id":"core","name":"Core","caption":"c","description":"d","tier":"core","paths":["core/"]},
      {"id":"understanding","name":"Understanding","caption":"c","description":"d","tier":"core","paths":["understanding/"]},
      {"id":"window","name":"Window","caption":"c","description":"d","tier":"core","paths":["ui/"]}],
      "actors":[],"links":[],"ignored":[],"laws":[]}"#;

    #[test]
    fn a_failed_check_names_its_one_test_and_the_part_that_owns_it() {
        assert_eq!(
            name_failed_check(NODE_TAP, "npm run check", Some(MAP)),
            "Window tests failed: a delete keeps the live terminal (+1 more)"
        );
        let one =
            "not ok 1 - a delete keeps the live terminal\n  location: 'ui/tests/t.test.mjs:5:1'\n";
        assert_eq!(
            name_failed_check(one, "npm run check", Some(MAP)),
            "Window tests failed: a delete keeps the live terminal"
        );
    }

    #[test]
    fn several_failures_across_parts_name_the_first_and_count_the_rest() {
        assert_eq!(
            name_failed_check(NEXTEST_REAL, "cargo nextest run", Some(MAP)),
            "Core tests failed: the wire samples of the core are written for tsc (+2 more)"
        );
    }

    #[test]
    fn a_failure_with_no_test_parsed_names_the_step() {
        assert_eq!(
            name_failed_check(FMT, "npm run check", Some(MAP)),
            "formatting check failed"
        );
        assert_eq!(
            name_failed_check("error: could not compile `x`", "cargo build", None),
            "build failed"
        );
        assert_eq!(name_failed_check("", "go vet", None), "go vet failed");
    }

    #[test]
    fn a_project_without_a_map_names_the_test_alone() {
        assert_eq!(
            name_failed_check(NODE_SPEC, "npm run check", None),
            "Test failed: a delete keeps the live terminal (+1 more)"
        );
        // A test no part owns is named the same way.
        assert_eq!(
            name_failed_check(NEXTEST, "cargo nextest run", Some(MAP)),
            "Test failed: unrelated in same file (+2 more)"
        );
    }
}
