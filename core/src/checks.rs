//! Checks (danielrispler/plotkeeper#112): which commands are checks, from a per-project list, and how a
//! turn's checks ended, from the step ledger's rows of the main agent and its workers.

use crate::ledger::{Ledger, Outcome, Row};
use serde::Serialize;
use std::{io, path::Path};

/// The checks a project without a list of its own has: the fixed list of danielrispler/Falinks#328 plus this
/// repository's own, `npm run check` (danielrispler/plotkeeper#112).
const DEFAULT: [&str; 9] = [
    "cargo test",
    "cargo nextest",
    "cargo clippy",
    "cargo check",
    "cargo build",
    "ci-checks",
    "go test",
    "go vet",
    "npm run check",
];

/// How many lines of a check's output `output` keeps.
const TAIL_LINES: usize = 40;

/// The check lists of the settings' `checks`: a list of commands per project folder, which a session
/// working in that folder or under it uses.
pub struct CheckLists {
    default: Vec<String>,
    /// Each project's folder and list; the longest folder that holds a session's `cwd` wins.
    projects: Vec<(String, Vec<String>)>,
}

impl Default for CheckLists {
    fn default() -> Self {
        CheckLists {
            default: DEFAULT.map(String::from).to_vec(),
            projects: Vec::new(),
        }
    }
}

impl CheckLists {
    pub fn set(&mut self, projects: impl IntoIterator<Item = (String, Vec<String>)>) {
        self.projects = projects.into_iter().collect();
    }

    /// The list of a session working in `cwd`.
    pub fn of(&self, cwd: &str) -> &[String] {
        let holds = |folder: &str| {
            let folder = folder.trim_end_matches('/');
            cwd.strip_prefix(folder)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        };
        let project = self
            .projects
            .iter()
            .filter(|(folder, _)| holds(folder))
            .max_by_key(|(folder, _)| folder.len());
        project.map_or(&self.default, |(_, list)| list)
    }
}

/// The entry of `list` that `command` runs, if any: its words, in order, among the command's, with
/// any flags before the subcommand skipped (`cargo +nightly test`). Quoted text is not run, so a PR
/// body that names `cargo test` is no check (danielrispler/Falinks#343).
pub fn matched<'a, S: AsRef<str>>(list: &'a [S], command: &str) -> Option<&'a str> {
    let mut quote = None;
    let unquoted: String = command
        .chars()
        .map(|c| {
            match quote {
                None if c == '\'' || c == '"' => quote = Some(c),
                None => return c,
                Some(open) if c == open => quote = None,
                Some(_) => {}
            }
            ' '
        })
        .collect();
    let words: Vec<&str> = unquoted
        .split(|c: char| c.is_whitespace() || ";&|()".contains(c))
        .filter(|word| !word.is_empty())
        .collect();
    list.iter().map(AsRef::as_ref).find(|entry| {
        let mut entry = entry.split_whitespace();
        let Some(first) = entry.next() else {
            return false;
        };
        let rest: Vec<&str> = entry.collect();
        words.iter().enumerate().any(|(i, word)| {
            if word.rsplit('/').next() != Some(first) {
                return false;
            }
            let following = words[i + 1..].iter().filter(|w| !w.starts_with(['-', '+']));
            following.zip(&rest).filter(|(w, r)| w == r).count() == rest.len()
        })
    })
}

/// Whether `command` runs a check of the default list.
pub fn is_check(command: &str) -> bool {
    matched(&DEFAULT, command).is_some()
}

/// Whether a check's exit code says whether it passed (danielrispler/Falinks#328, module 4). A command exits with its
/// last statement's code, so the check must be the last statement, and nothing in it may mask the
/// code: `|| true` exits 0, `!` flips it, `&` runs the check in the background, and a pipe exits
/// with its last command's code unless an earlier `set` turns on `pipefail`.
pub fn exit_code_counts<S: AsRef<str>>(list: &[S], command: &str) -> bool {
    let statements = command.split([';', '\n']).map(str::trim);
    let Some(last) = statements.clone().rfind(|s| !s.is_empty()) else {
        return false;
    };
    let pipefail = statements
        .flat_map(|s| s.split("&&"))
        .any(|s| s.trim().starts_with("set ") && s.contains("pipefail"));
    let background = ["&&", ">&", "&>"]
        .iter()
        .fold(last.to_string(), |rest, and| rest.replace(and, ""))
        .contains('&');
    matched(list, last).is_some()
        && !last.contains("||")
        && !last.starts_with('!')
        && !background
        && (pipefail || !last.contains('|'))
}

/// How a check, or one run of it, ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Failed,
    /// It ran, but its exit code does not say whether it passed (a pipe), so it is never counted as
    /// passed: **Not counted**.
    Ran,
}

/// One run of a check.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Run {
    /// The whole command.
    pub command: String,
    pub status: Status,
    /// Its exit code, when the log says it.
    pub exit_code: Option<i32>,
    /// Its call's id and its worker, which find its row in the ledger (`Ledger::row`).
    pub call: String,
    pub agent: Option<String>,
}

/// A suite of a turn: the list entry its commands matched (`cargo test`, `npm run check`), whose
/// status is its last run's.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Check {
    pub suite: String,
    pub status: Status,
    /// How many runs before the last one failed.
    pub retries: u32,
    /// Its runs, oldest first, with their own commands.
    pub runs: Vec<Run>,
}

/// The run `row` is, `None` for a call that is no check run: one that matches no entry of `list`,
/// that did not run (denied or interrupted) or that has no result yet.
fn run<'a>(row: &Row, list: &'a [String]) -> Option<(&'a str, Run)> {
    let command = row.command.as_deref().filter(|_| row.tool == "Bash")?;
    let suite = matched(list, command)?;
    let counts = exit_code_counts(list, command);
    let (status, exit_code) = match row.outcome? {
        Outcome::Succeeded if counts && !row.background => (Status::Passed, Some(0)),
        Outcome::Succeeded => (Status::Ran, None),
        Outcome::Failed { exit_code } if row.ran && counts => (Status::Failed, exit_code),
        Outcome::Failed { exit_code } if row.ran => (Status::Ran, exit_code),
        Outcome::Failed { .. } | Outcome::Rejected => return None,
    };
    let run = Run {
        command: command.into(),
        status,
        exit_code,
        call: row.call.clone(),
        agent: row.agent.clone(),
    };
    Some((suite, run))
}

/// Turn `turn`'s checks, its workers' included: the ledger's rows of the turn that match `list`,
/// grouped by suite in the order each first ran.
pub fn of_turn(ledger: &Ledger, turn: usize, list: &[String]) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();
    for (suite, run) in ledger.turn(turn).filter_map(|row| run(row, list)) {
        let at = checks.iter().position(|c| c.suite == suite);
        let check = match at {
            Some(at) => &mut checks[at],
            None => {
                checks.push(Check {
                    suite: suite.into(),
                    status: run.status,
                    retries: 0,
                    runs: Vec::new(),
                });
                checks.last_mut().expect("just pushed")
            }
        };
        check.status = run.status;
        check.runs.push(run);
    }
    for check in &mut checks {
        let earlier = &check.runs[..check.runs.len() - 1];
        check.retries = earlier
            .iter()
            .filter(|r| r.status == Status::Failed)
            .count() as u32;
    }
    checks
}

/// A check run's exit code and the tail of its output.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Output {
    pub exit_code: Option<i32>,
    pub tail: String,
}

/// Reads `row`'s output from the event log at `log`, through the ledger's offset: one seek and one
/// read, with no scan. A failed command's output is in its `error`, after its exit code's line.
pub fn output(row: &Row, log: &Path) -> io::Result<Output> {
    let end = row.details(log)?.end;
    let end = end.as_ref();
    let text = match end.and_then(|e| e["error"].as_str()) {
        Some(error) => error.to_string(),
        None => ["stdout", "stderr"]
            .iter()
            .filter_map(|k| end?["tool_response"][k].as_str())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
    };
    let lines: Vec<&str> = text.lines().collect();
    let exit_code = match row.outcome {
        Some(Outcome::Failed { exit_code }) => exit_code,
        Some(Outcome::Succeeded) if !row.background => Some(0),
        _ => None,
    };
    Ok(Output {
        exit_code,
        tail: lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n"),
    })
}
