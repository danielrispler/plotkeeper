//! Checks (danielrispler/plotkeeper#112): which commands are checks, from a per-project list, and how a
//! turn's checks ended, from the step ledger's rows of the main agent and its workers.

use crate::ledger::{Ledger, Outcome, Row};
use serde::{Deserialize, Serialize};
use std::{io, path::Path};

/// The checks a project without a list of its own has: the fixed list of danielrispler/Falinks#328 plus this
/// repository's own, `npm run check` (danielrispler/plotkeeper#112).
pub(crate) const DEFAULT: [&str; 9] = [
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

/// How a statement is joined to the one next to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Joiner {
    /// `;` or a line break.
    Semi,
    And,
    Or,
    Pipe,
    /// `&`.
    Background,
}

/// One statement of a command: the words from its command on, and where it stands in the command.
#[derive(Clone, Debug)]
pub(crate) struct Statement<'a> {
    /// The list entry it runs, when it runs one.
    pub suite: Option<&'a str>,
    /// Its words from the command on (what a run's key is), redirections left out.
    pub text: String,
    /// Its command's group: the statements between `;`s and line breaks, counted from 0.
    pub group: usize,
    /// How it is joined to the statement before it, and to the one after it.
    pub before: Option<Joiner>,
    pub after: Option<Joiner>,
    /// It starts with `!`.
    pub negated: bool,
    /// It is `set` with `pipefail`.
    pub pipefail: bool,
}

/// A word of a statement: its text as written, and whether it holds no quote or escape.
struct Word {
    text: String,
    plain: bool,
}

/// A statement before it is read: its words, and its joiners.
struct Raw {
    words: Vec<Word>,
    before: Option<Joiner>,
    after: Option<Joiner>,
    group: usize,
}

/// Splits `command` into statements at `;`, line breaks, `&&`, `||`, `|` and `&` outside quoted
/// text. A heredoc's body is skipped, and a backslash-escaped quote does not end quoted text.
fn split(command: &str) -> Vec<Raw> {
    struct Reader {
        done: Vec<Raw>,
        words: Vec<Word>,
        word: String,
        plain: bool,
        in_word: bool,
        group: usize,
        before: Option<Joiner>,
    }
    impl Reader {
        fn end_word(&mut self) {
            if self.in_word {
                let text = std::mem::take(&mut self.word);
                self.words.push(Word {
                    text,
                    plain: self.plain,
                });
            }
            self.in_word = false;
            self.plain = true;
        }
        fn push(&mut self, c: char, plain: bool) {
            self.word.push(c);
            self.in_word = true;
            self.plain &= plain;
        }
        /// A joiner between statements; one with no words before it joins nothing.
        fn join(&mut self, joiner: Joiner) {
            self.end_word();
            if self.words.is_empty() {
                return;
            }
            self.done.push(Raw {
                words: std::mem::take(&mut self.words),
                before: self.before,
                after: Some(joiner),
                group: self.group,
            });
            self.before = Some(joiner);
            if joiner == Joiner::Semi {
                self.group += 1;
            }
        }
    }
    let chars: Vec<char> = command.chars().collect();
    let mut reader = Reader {
        done: Vec::new(),
        words: Vec::new(),
        word: String::new(),
        plain: true,
        in_word: false,
        group: 0,
        before: None,
    };
    // The heredocs whose bodies start after this line: their delimiter, and whether `<<-` strips tabs.
    let mut heredocs: Vec<(String, bool)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        i += 1;
        match c {
            '\\' => match next {
                Some('\n') => i += 1,
                Some(next) => {
                    reader.push(c, false);
                    reader.push(next, false);
                    i += 1;
                }
                None => reader.push(c, false),
            },
            '\'' | '"' => {
                reader.push(c, false);
                while i < chars.len() {
                    let inner = chars[i];
                    i += 1;
                    reader.push(inner, false);
                    if inner == c {
                        break;
                    }
                    if inner == '\\' && c == '"' && i < chars.len() {
                        reader.push(chars[i], false);
                        i += 1;
                    }
                }
            }
            '#' if !reader.in_word => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\n' => {
                reader.join(Joiner::Semi);
                for (delimiter, strip) in heredocs.drain(..) {
                    while i < chars.len() {
                        let end = chars[i..].iter().position(|&c| c == '\n');
                        let end = end.map_or(chars.len(), |n| i + n);
                        let line: String = chars[i..end].iter().collect();
                        i = (end + 1).min(chars.len());
                        let line = if strip {
                            line.trim_start_matches('\t')
                        } else {
                            &line
                        };
                        if line.trim_end_matches('\r') == delimiter {
                            break;
                        }
                    }
                }
            }
            ';' => reader.join(Joiner::Semi),
            '&' if next == Some('&') => {
                i += 1;
                reader.join(Joiner::And);
            }
            // A redirection, as in `2>&1` and `&>file`, is no joiner.
            '&' if reader.word.ends_with(['<', '>']) || next == Some('>') => reader.push(c, true),
            '&' => reader.join(Joiner::Background),
            '|' if next == Some('|') => {
                i += 1;
                reader.join(Joiner::Or);
            }
            '|' => reader.join(Joiner::Pipe),
            '<' if next == Some('<') && chars.get(i + 1) != Some(&'<') => {
                i += 1;
                let strip = chars.get(i) == Some(&'-');
                if strip {
                    i += 1;
                }
                while chars.get(i).is_some_and(|c| *c == ' ' || *c == '\t') {
                    i += 1;
                }
                let mut delimiter = String::new();
                while let Some(&d) = chars.get(i) {
                    if d.is_whitespace() || ";&|()<>".contains(d) {
                        break;
                    }
                    if !"'\"\\".contains(d) {
                        delimiter.push(d);
                    }
                    i += 1;
                }
                heredocs.push((delimiter, strip));
            }
            c if c == ' ' || c == '\t' || c == '\r' || c == '(' || c == ')' => reader.end_word(),
            c => reader.push(c, true),
        }
    }
    reader.end_word();
    if !reader.words.is_empty() {
        reader.done.push(Raw {
            words: reader.words,
            before: reader.before,
            after: None,
            group: reader.group,
        });
    }
    reader.done
}

/// `words` without its redirections (`2>&1`, `> out`, `< in`) and the file each names.
fn without_redirections(words: Vec<Word>) -> Vec<Word> {
    let mut kept = Vec::new();
    let mut words = words.into_iter();
    while let Some(word) = words.next() {
        let rest = word.text.trim_start_matches(|c: char| c.is_ascii_digit());
        let operator = rest.chars().take_while(|c| "<>&".contains(*c)).count();
        let redirects = word.plain && (rest.starts_with(['<', '>']) || rest.starts_with("&>"));
        if !redirects {
            kept.push(word);
        } else if rest.len() == operator {
            words.next();
        }
    }
    kept
}

/// Words that may stand before a statement's command, and how many words after each belong to it.
fn wrapper(word: &str) -> Option<usize> {
    match word {
        "time" | "env" | "nice" | "exec" | "command" | "do" | "then" | "else" | "if" | "while"
        | "until" | "!" | "{" => Some(0),
        "timeout" => Some(1),
        _ => None,
    }
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        name.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
    })
}

/// Where a statement's command starts among its words, past what may stand before it.
fn command_at(words: &[Word]) -> usize {
    let mut at = 0;
    while let Some(word) = words.get(at).filter(|w| w.plain) {
        let wraps = wrapper(&word.text);
        if wraps.is_none() && !is_assignment(&word.text) {
            break;
        }
        at += 1;
        if let Some(duration) = wraps {
            while words
                .get(at)
                .is_some_and(|w| w.plain && w.text.starts_with('-'))
            {
                at += 1;
                if word.text == "nice" && words[at - 1].text == "-n" {
                    at += 1;
                }
            }
            at += duration;
        }
    }
    at.min(words.len())
}

/// Whether the statement `words`, from its command on, runs `entry`: the entry's first word is the
/// command, and its other words follow in order among the words that are not flags. `cargo run … --bin
/// <entry>` runs a one-word entry.
fn runs_entry(entry: &str, words: &[Word]) -> bool {
    let mut entry = entry.split_whitespace();
    let (Some(first), Some(command)) = (entry.next(), words.first().filter(|w| w.plain)) else {
        return false;
    };
    let rest: Vec<&str> = entry.collect();
    let args: Vec<&Word> = words[1..].iter().collect();
    let command = command.text.rsplit('/').next();
    let positional = || {
        args.iter()
            .filter(|w| !(w.plain && w.text.starts_with(['-', '+'])))
    };
    if command == Some(first) {
        let matching = positional()
            .zip(&rest)
            .filter(|(w, r)| w.plain && w.text == **r)
            .count();
        return matching == rest.len();
    }
    command == Some("cargo")
        && rest.is_empty()
        && positional()
            .next()
            .is_some_and(|w| w.plain && w.text == "run")
        && args
            .iter()
            .take_while(|w| w.text != "--")
            .enumerate()
            .any(|(n, w)| {
                w.plain
                    && (w.text.strip_prefix("--bin=") == Some(first)
                        || (w.text == "--bin"
                            && args.get(n + 1).is_some_and(|b| b.plain && b.text == first)))
            })
}

/// The statements of `command`, each with the entry of `list` it runs, if any.
pub(crate) fn statements<'a, S: AsRef<str>>(list: &'a [S], command: &str) -> Vec<Statement<'a>> {
    split(command)
        .into_iter()
        .map(|raw| {
            let words = without_redirections(raw.words);
            let at = command_at(&words);
            let own = &words[at..];
            let suite = list
                .iter()
                .map(AsRef::as_ref)
                .find(|entry| runs_entry(entry, own));
            Statement {
                suite,
                text: own
                    .iter()
                    .map(|w| w.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" "),
                group: raw.group,
                before: raw.before,
                after: raw.after,
                negated: words[..at].iter().any(|w| w.plain && w.text == "!"),
                pipefail: own.first().is_some_and(|w| w.plain && w.text == "set")
                    && own.iter().any(|w| w.text.contains("pipefail")),
            }
        })
        .collect()
}

/// The entry of `list` that `command` runs, if any: the first of its statements that runs one. This
/// is the one test of whether a command is a check.
pub fn matched<'a, S: AsRef<str>>(list: &'a [S], command: &str) -> Option<&'a str> {
    statements(list, command).iter().find_map(|s| s.suite)
}

/// Whether a check's exit code says whether it passed (danielrispler/Falinks#328, module 4). A command exits with its
/// last statement group's code, so the check must be in the last group, and nothing in it may mask the
/// code: `|| true` exits 0, `!` flips it, `&` runs the check in the background, and a pipe exits
/// with its last command's code unless an earlier `set` turns on `pipefail`. A command with more
/// than one check does not count, until the chain rule (spec #254) says which one failed.
pub fn exit_code_counts<S: AsRef<str>>(list: &[S], command: &str) -> bool {
    let statements = statements(list, command);
    let mut runs = statements.iter().filter(|s| s.suite.is_some());
    let (Some(run), None) = (runs.next(), runs.next()) else {
        return false;
    };
    let pipefail = statements.iter().any(|s| s.pipefail);
    let last = statements.last().map_or(0, |s| s.group);
    let group = || statements.iter().filter(|s| s.group == last);
    run.group == last
        && !group().next().is_some_and(|s| s.negated)
        && !group().any(|s| {
            [s.before, s.after]
                .into_iter()
                .flatten()
                .any(|joiner| match joiner {
                    Joiner::Or | Joiner::Background => true,
                    Joiner::Pipe => !pipefail,
                    Joiner::Semi | Joiner::And => false,
                })
        })
}

/// How a check, or one run of it, ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    Failed,
    /// It ran, but its exit code does not say whether it passed (a pipe), so it is never counted as
    /// passed: **Not counted**.
    Ran,
}

/// One run of a check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// The whole command.
    pub command: String,
    /// Its own statement: the words from the check's command on, redirections left out.
    pub statement: String,
    pub status: Status,
    /// Its exit code, when the log says it.
    pub exit_code: Option<i32>,
    /// Its call's id and its worker, which find its row in the ledger (`Ledger::row`).
    pub call: String,
    pub agent: Option<String>,
}

/// A suite of a turn: the list entry its commands matched (`cargo test`, `npm run check`), whose
/// status is its last run's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub suite: String,
    pub status: Status,
    /// How many runs before the last one failed.
    pub retries: u32,
    /// Its runs, oldest first, with their own commands.
    pub runs: Vec<Run>,
}

/// The runs `row` holds, one for each statement of its command that runs an entry of `list`: none for
/// a call that is no check run, one that did not run (denied or interrupted) or one that has no result
/// yet.
fn runs<'a>(row: &Row, list: &'a [String]) -> Vec<(&'a str, Run)> {
    let Some(command) = row.command.as_deref() else {
        return Vec::new();
    };
    let counts = exit_code_counts(list, command);
    let (status, exit_code) = match row.outcome {
        Some(Outcome::Succeeded) if counts && !row.background => (Status::Passed, Some(0)),
        Some(Outcome::Succeeded) => (Status::Ran, None),
        Some(Outcome::Failed { exit_code }) if row.ran && counts => (Status::Failed, exit_code),
        Some(Outcome::Failed { exit_code }) if row.ran => (Status::Ran, exit_code),
        Some(Outcome::Failed { .. } | Outcome::Rejected) | None => return Vec::new(),
    };
    statements(list, command)
        .into_iter()
        .filter_map(|s| {
            let run = Run {
                command: command.into(),
                statement: s.text,
                status,
                exit_code,
                call: row.call.clone(),
                agent: row.agent.clone(),
            };
            Some((s.suite?, run))
        })
        .collect()
}

/// Turn `turn`'s checks, its workers' included: the ledger's rows of the turn that match `list`,
/// grouped by suite in the order each first ran.
pub fn of_turn(ledger: &Ledger, turn: usize, list: &[String]) -> Vec<Check> {
    let mut checks: Vec<Check> = Vec::new();
    for (suite, run) in ledger.turn(turn).flat_map(|row| runs(row, list)) {
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
