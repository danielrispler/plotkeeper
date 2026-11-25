//! Checks (danielrispler/plotkeeper#112): which commands are checks, from a per-project list, and how a
//! turn's checks ended, from the step ledger's rows of the main agent and its workers.

use crate::ledger::{Ledger, Outcome, Row};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashMap},
    io,
    path::Path,
};

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
    /// It is `set` with `pipefail`: `Some(true)` turns it on (`-o pipefail`), `Some(false)` off
    /// (`+o pipefail`).
    pub pipefail: Option<bool>,
    /// The folder of the last `cd` with an absolute path in a statement before it.
    pub cd: Option<String>,
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

/// Reads a command one character at a time into statements (`split`): the statements done, the
/// one being read, and the heredocs whose bodies start after the current line.
struct Reader {
    chars: Vec<char>,
    /// The next character to read.
    i: usize,
    done: Vec<Raw>,
    words: Vec<Word>,
    word: String,
    plain: bool,
    in_word: bool,
    group: usize,
    before: Option<Joiner>,
    /// Each heredoc's delimiter, and whether `<<-` strips tabs.
    heredocs: Vec<(String, bool)>,
}

impl Reader {
    fn new(command: &str) -> Self {
        Reader {
            chars: command.chars().collect(),
            i: 0,
            done: Vec::new(),
            words: Vec::new(),
            word: String::new(),
            plain: true,
            in_word: false,
            group: 0,
            before: None,
            heredocs: Vec::new(),
        }
    }

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
        // `;` and `&` each end a group: what follows runs in the foreground, after it.
        if matches!(joiner, Joiner::Semi | Joiner::Background) {
            self.group += 1;
        }
    }

    /// The rest of quoted text opened by `quote`, up to its closing quote; a backslash in double
    /// quotes escapes the character after it.
    fn quoted(&mut self, quote: char) {
        while self.i < self.chars.len() {
            let inner = self.chars[self.i];
            self.i += 1;
            self.push(inner, false);
            if inner == quote {
                break;
            }
            if inner == '\\' && quote == '"' && self.i < self.chars.len() {
                self.push(self.chars[self.i], false);
                self.i += 1;
            }
        }
    }

    /// The rest of a `$(…)` or `$((…))`, which belongs to the word it stands in, whatever quotes or
    /// parentheses it holds.
    fn substitution(&mut self) {
        let mut depth = 0;
        while self.i < self.chars.len() {
            let inner = self.chars[self.i];
            self.i += 1;
            self.push(inner, false);
            match inner {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                '\\' if self.i < self.chars.len() => {
                    self.push(self.chars[self.i], false);
                    self.i += 1;
                }
                '\'' | '"' => self.quoted(inner),
                _ => {}
            }
        }
    }

    /// A comment, up to the end of its line.
    fn comment(&mut self) {
        while self.i < self.chars.len() && self.chars[self.i] != '\n' {
            self.i += 1;
        }
    }

    /// The bodies of the heredocs the line just ended opened, each up to its delimiter.
    fn heredoc_bodies(&mut self) {
        let chars = &self.chars;
        for (delimiter, strip) in self.heredocs.drain(..) {
            while self.i < chars.len() {
                let end = chars[self.i..].iter().position(|&c| c == '\n');
                let end = end.map_or(chars.len(), |n| self.i + n);
                let line: String = chars[self.i..end].iter().collect();
                self.i = (end + 1).min(chars.len());
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

    /// A heredoc's `<<` or `<<-` and its delimiter, after the first `<`; its body starts after the
    /// line.
    fn heredoc(&mut self) {
        self.i += 1;
        let strip = self.chars.get(self.i) == Some(&'-');
        if strip {
            self.i += 1;
        }
        while self
            .chars
            .get(self.i)
            .is_some_and(|c| *c == ' ' || *c == '\t')
        {
            self.i += 1;
        }
        let mut delimiter = String::new();
        while let Some(&d) = self.chars.get(self.i) {
            if d.is_whitespace() || ";&|()<>".contains(d) {
                break;
            }
            if !"'\"\\".contains(d) {
                delimiter.push(d);
            }
            self.i += 1;
        }
        self.heredocs.push((delimiter, strip));
    }

    /// The statements read, the last one included.
    fn finish(mut self) -> Vec<Raw> {
        self.end_word();
        if !self.words.is_empty() {
            self.done.push(Raw {
                words: self.words,
                before: self.before,
                after: None,
                group: self.group,
            });
        }
        self.done
    }
}

/// Splits `command` into statements at `;`, line breaks, `&&`, `||`, `|` and `&` outside quoted
/// text. A heredoc's body is skipped, and a backslash-escaped quote does not end quoted text.
fn split(command: &str) -> Vec<Raw> {
    let mut reader = Reader::new(command);
    while reader.i < reader.chars.len() {
        let c = reader.chars[reader.i];
        let next = reader.chars.get(reader.i + 1).copied();
        reader.i += 1;
        match c {
            '\\' => match next {
                Some('\n') => reader.i += 1,
                Some(next) => {
                    reader.push(c, false);
                    reader.push(next, false);
                    reader.i += 1;
                }
                None => reader.push(c, false),
            },
            // What a `$(…)` runs is no statement of the command.
            '$' if next == Some('(') => {
                reader.push(c, false);
                reader.substitution();
            }
            '\'' | '"' => {
                reader.push(c, false);
                reader.quoted(c);
            }
            '#' if !reader.in_word => reader.comment(),
            '\n' => {
                reader.join(Joiner::Semi);
                reader.heredoc_bodies();
            }
            ';' => reader.join(Joiner::Semi),
            '&' if next == Some('&') => {
                reader.i += 1;
                reader.join(Joiner::And);
            }
            // A redirection, as in `2>&1` and `&>file`, is no joiner.
            '&' if reader.word.ends_with(['<', '>']) || next == Some('>') => reader.push(c, true),
            '&' => reader.join(Joiner::Background),
            '|' if next == Some('|') => {
                reader.i += 1;
                reader.join(Joiner::Or);
            }
            '|' => reader.join(Joiner::Pipe),
            // `<<<` is a here-string, no heredoc.
            '<' if next == Some('<')
                && reader.chars.get(reader.i + 1) != Some(&'<')
                && !(reader.i >= 2 && reader.chars[reader.i - 2] == '<') =>
            {
                reader.heredoc();
            }
            c if c == ' ' || c == '\t' || c == '\r' || c == '(' || c == ')' => reader.end_word(),
            c => reader.push(c, true),
        }
    }
    reader.finish()
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
    // A variable's value may be quoted: only the name before the `=` must be plain.
    while let Some(word) = words.get(at) {
        let wraps = word.plain.then(|| wrapper(&word.text)).flatten();
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

/// What the statement `own` does to `pipefail`, if it is a `set` that names it.
fn pipefail_of(own: &[Word]) -> Option<bool> {
    if !own.first().is_some_and(|w| w.plain && w.text == "set") {
        return None;
    }
    let at = own.iter().position(|w| w.plain && w.text == "pipefail")?;
    let option = own.get(at.checked_sub(1)?)?;
    option
        .text
        .starts_with(['-', '+'])
        .then(|| option.text.starts_with('-'))
}

/// The statements of `command`, each with the entry of `list` it runs, if any.
pub(crate) fn statements<'a, S: AsRef<str>>(list: &'a [S], command: &str) -> Vec<Statement<'a>> {
    let mut cd: Option<String> = None;
    split(command)
        .into_iter()
        .map(|raw| {
            let words = without_redirections(raw.words);
            let at = command_at(&words);
            let own = &words[at..];
            let before_cd = cd.clone();
            if own.first().is_some_and(|w| w.plain && w.text == "cd") {
                let target = own[1..].iter().find(|w| !w.text.starts_with('-'));
                let target = target.map(|w| w.text.trim_matches(['"', '\'']));
                if let Some(folder) = target.filter(|f| f.starts_with('/')) {
                    cd = Some(folder.trim_end_matches('/').to_string());
                }
            }
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
                pipefail: pipefail_of(own),
                cd: before_cd,
            }
        })
        .collect()
}

/// The entry of `list` that `command` runs, if any: the first of its statements that runs one. This
/// is the one test of whether a command is a check.
pub(crate) fn matched<'a, S: AsRef<str>>(list: &'a [S], command: &str) -> Option<&'a str> {
    statements(list, command).iter().find_map(|s| s.suite)
}

/// The entries of `list` that `command` runs, one for each of its statements that runs one.
pub(crate) fn suites<'a, S: AsRef<str>>(list: &'a [S], command: &str) -> Vec<&'a str> {
    statements(list, command)
        .iter()
        .filter_map(|s| s.suite)
        .collect()
}

/// Whether a check run's exit code counts, for each way its command can end.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    /// Exit 0 says it passed.
    pub zero: bool,
    /// A non-zero exit says it failed.
    pub failure: bool,
}

/// Whether the statement `text` only sets the scene for the next one: `cd`, `export`, `set` or a
/// variable assignment.
fn is_prelude(text: &str) -> bool {
    let first = text.split_whitespace().next().unwrap_or_default();
    matches!(first, "" | "cd" | "export" | "set") || is_assignment(first)
}

/// Whether the exit code of statement `at` of `statements` counts for it (danielrispler/Falinks#328, module 4, and
/// spec #254). A command exits with its last statement group's code, so the run must be in the last
/// group, and nothing in it may mask the code: `|| true` exits 0, `!` flips it, `&` runs it in the
/// background, and a pipe exits with its last command's code unless an earlier `set` turns on
/// `pipefail`. Exit 0 then says every run of the group passed; a non-zero exit names the run only
/// when it is the group's one statement besides `cd`, `export`, `set` and assignments.
fn counts_of(statements: &[Statement], at: usize) -> Counts {
    // On when the last `set` before the run that names it turned it on.
    let pipefail = statements[..at]
        .iter()
        .rev()
        .find_map(|s| s.pipefail)
        .unwrap_or(false);
    let last = statements.last().map_or(0, |s| s.group);
    let group = || statements.iter().filter(|s| s.group == last);
    let clear = statements[at].group == last
        && !group().any(|s| {
            // A `&` ends a group, so it masks only the statement before it.
            s.negated
                || [(s.before, false), (s.after, true)]
                    .into_iter()
                    .any(|(joiner, after)| match joiner {
                        Some(Joiner::Or) => true,
                        Some(Joiner::Background) => after,
                        Some(Joiner::Pipe) => !pipefail,
                        Some(Joiner::Semi | Joiner::And) | None => false,
                    })
        });
    // Under `pipefail` a pipeline exits non-zero for its failed command, so it counts as one statement.
    let alone = group()
        .filter(|s| !is_prelude(&s.text) && !(pipefail && s.before == Some(Joiner::Pipe)))
        .count()
        == 1;
    Counts {
        zero: clear,
        failure: clear && alone,
    }
}

/// What a command's typical duration is kept under: the keys of its check statements, joined in
/// order. `None` for a command that runs no check.
pub(crate) fn key<S: AsRef<str>>(list: &[S], command: &str) -> Option<String> {
    let keys: Vec<String> = statements(list, command)
        .into_iter()
        .filter(|s| s.suite.is_some())
        .map(|s| s.text)
        .collect();
    (!keys.is_empty()).then(|| keys.join("\n"))
}

/// How long each command of checks ran when it did not fail, by `key`, oldest first
/// (danielrispler/Falinks#461, decision 13), kept in the studio's saved state as the plain map.
#[derive(Default, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Durations(HashMap<String, Vec<u64>>);

impl Durations {
    /// Keeps that `command`, under `list`, took `ms`; nothing for a command that runs no check.
    pub(crate) fn record<S: AsRef<str>>(&mut self, list: &[S], command: &str, ms: u64) {
        if let Some(key) = key(list, command) {
            self.0.entry(key).or_default().push(ms);
        }
    }

    /// The lower median of how long `command`'s earlier runs took, `None` when it never ran.
    pub(crate) fn typical<S: AsRef<str>>(&self, list: &[S], command: &str) -> Option<u64> {
        let mut runs = self.0.get(&key(list, command)?)?.clone();
        runs.sort_unstable();
        runs.get(runs.len().checked_sub(1)? / 2).copied()
    }

    /// The durations in key order, to compare two studios.
    pub(crate) fn sorted(&self) -> BTreeMap<&String, &Vec<u64>> {
        self.0.iter().collect()
    }
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
    /// It started and has no result yet: not ended, so neither passed nor failed.
    Running,
}

/// One run of a check.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Run {
    /// The whole command.
    pub command: String,
    /// Its own statement: the words from the check's command on, redirections left out. Empty in a
    /// summary stored before it was kept.
    #[serde(default)]
    pub statement: String,
    pub status: Status,
    /// Its exit code, when the log says it.
    pub exit_code: Option<i32>,
    /// Its call's id and its worker, which find its row in the ledger (`Ledger::row`).
    pub call: String,
    pub agent: Option<String>,
    /// When it started, in Unix milliseconds, when the log says.
    pub started_at: Option<u64>,
    /// When it ended, when it has: its result's time, or for a background run its notification's.
    pub ended_at: Option<u64>,
}

/// A check the feed lists under its turn: the latest run of a check of the session's own agent, while
/// it runs or after it failed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Listed {
    /// The run's own statement.
    pub statement: String,
    pub status: Status,
    pub started_at: u64,
    /// `None` while it runs.
    pub ended_at: Option<u64>,
    /// Its command's typical duration.
    pub typical_ms: Option<u64>,
}

/// A suite of a turn: the list entry its commands matched (`cargo test`, `npm run check`), whose
/// status is its last run's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub suite: String,
    /// The worktree it ran in, as its root.
    #[serde(default)]
    pub worktree: String,
    /// It ran outside the session's own worktree, so a row names its worktree.
    #[serde(default)]
    pub foreign: bool,
    pub status: Status,
    /// How many runs before the last one failed.
    pub retries: u32,
    /// Its runs, oldest first, with their own commands.
    pub runs: Vec<Run>,
}

/// The worktree a run is in: the root that holds the folder of the last absolute `cd` before it, when
/// one of the call's roots does; else the call's first root; else `home`, the worktree of the session's
/// folder or of the worker that made the call.
fn worktree(row: &Row, cd: Option<&str>, home: &str) -> String {
    let holds = |root: &&String, folder: &str| {
        folder == root.as_str()
            || folder
                .strip_prefix(root.as_str())
                .is_some_and(|r| r.starts_with('/'))
    };
    let held = cd.and_then(|folder| {
        row.roots
            .iter()
            .filter(|root| holds(root, folder))
            .max_by_key(|root| root.len())
    });
    held.or(row.roots.first())
        .map_or(home, String::as_str)
        .into()
}

/// The runs `row` holds, one for each statement of its command that runs an entry of `list`: none for
/// a call that is no check run, or one that did not run (denied or interrupted). A call with no result
/// yet is running.
fn runs<'a>(row: &Row, list: &'a [String], home: &str) -> Vec<(&'a str, String, Run)> {
    let Some(command) = row.command.as_deref() else {
        return Vec::new();
    };
    let statements = statements(list, command);
    let outcome = |counts: Counts| match (&row.outcome, row.ran) {
        // Started, no result yet.
        (None, _) => Some((Status::Running, None)),
        // A background task keeps what its notification reports.
        (Some(Outcome::Succeeded), _) if row.background => Some(match row.reported.as_deref() {
            None => (Status::Running, None),
            Some("completed") if counts.zero => (Status::Passed, None),
            Some("failed") if counts.failure => (Status::Failed, None),
            Some(_) => (Status::Ran, None),
        }),
        (Some(Outcome::Succeeded), _) if counts.zero => Some((Status::Passed, Some(0))),
        (Some(Outcome::Succeeded), _) => Some((Status::Ran, None)),
        (Some(Outcome::Failed { exit_code }), true) if counts.failure => {
            Some((Status::Failed, *exit_code))
        }
        (Some(Outcome::Failed { exit_code }), true) => Some((Status::Ran, *exit_code)),
        (Some(Outcome::Failed { .. } | Outcome::Rejected), false)
        | (Some(Outcome::Rejected), _) => None,
    };
    (0..statements.len())
        .filter_map(|at| {
            let (status, exit_code) = outcome(counts_of(&statements, at))?;
            let s = &statements[at];
            let run = Run {
                command: command.into(),
                statement: s.text.clone(),
                status,
                exit_code,
                call: row.call.clone(),
                agent: row.agent.clone(),
                started_at: row.began(),
                ended_at: row.finished(),
            };
            Some((s.suite?, worktree(row, s.cd.as_deref(), home), run))
        })
        .collect()
}

/// Turn `turn`'s checks, its workers' included: the ledger's rows of the turn that match `list`,
/// grouped by suite and worktree in the order each first ran. `home` gives the worktree of a call that
/// recorded none: the session's folder's for `None`, else the worker's own.
pub(crate) fn of_turn(
    ledger: &Ledger,
    turn: usize,
    list: &[String],
    home: &dyn Fn(Option<&str>) -> String,
) -> Vec<Check> {
    let own = home(None);
    let mut checks: Vec<Check> = Vec::new();
    for row in ledger.turn(turn) {
        for (suite, worktree, run) in runs(row, list, &home(row.agent.as_deref())) {
            let at = checks
                .iter()
                .position(|c| c.suite == suite && c.worktree == worktree);
            let check = match at {
                Some(at) => &mut checks[at],
                None => {
                    checks.push(Check {
                        suite: suite.into(),
                        foreign: worktree != own,
                        worktree,
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

/// `checks` as a summary's facts hold them: only the runs that ended, so that a check still running
/// is left out, as is a running run of a check that has earlier ones. A check's status and retries
/// are those of the runs that remain.
pub(crate) fn ended(checks: &[Check]) -> Vec<Check> {
    checks
        .iter()
        .filter_map(|check| {
            let runs: Vec<Run> = check
                .runs
                .iter()
                .filter(|r| r.status != Status::Running)
                .cloned()
                .collect();
            let status = runs.last()?.status;
            let retries = runs[..runs.len() - 1]
                .iter()
                .filter(|r| r.status == Status::Failed)
                .count() as u32;
            Some(Check {
                status,
                retries,
                runs,
                ..check.clone()
            })
        })
        .collect()
}

/// The checks the feed lists, newest first: of the session's own agent, each check's latest run
/// when it is running or failed. `typical` gives a command's typical duration.
pub(crate) fn listed(checks: &[Check], typical: &dyn Fn(&str) -> Option<u64>) -> Vec<Listed> {
    let mut listed: Vec<Listed> = checks
        .iter()
        // A check that passed, as a worker's later run may have it, is not listed.
        .filter(|check| check.status != Status::Passed)
        .filter_map(|check| check.runs.iter().rfind(|run| run.agent.is_none()))
        .filter(|run| matches!(run.status, Status::Running | Status::Failed))
        .map(|run| Listed {
            statement: run.statement.clone(),
            status: run.status,
            started_at: run.started_at.or(run.ended_at).unwrap_or_default(),
            ended_at: run.ended_at,
            typical_ms: typical(&run.command),
        })
        .collect();
    listed.sort_by_key(|l| std::cmp::Reverse(l.started_at));
    listed
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
