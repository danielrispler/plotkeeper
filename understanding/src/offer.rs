//! What a turn gets for a Quiz (danielrispler/plotkeeper#119): a card with its Quiz, a lone Quiz, or
//! nothing, by the size of its change; the rules are in `docs/studio/question-rules.md`.

use crate::{CardStore, CardWriter, Place, SummarySetup, TurnSummary, write_stop, write_summary};
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use studio_core::{CardRequest, CheckStatus, Facts, Session, SummaryRequest};

/// Changed lines, in logic files, from which a turn gets a card.
const CARD_LINES: u64 = 25;
/// Logic files from which a turn gets a card.
const CARD_FILES: usize = 2;
/// Non-trivial lines from which a turn gets a lone Quiz.
const QUIZ_LINES: u64 = 8;

/// What a turn gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Offer {
    Card,
    Quiz,
    Nothing,
}

/// A file a turn changed, as the table reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Changed {
    pub path: String,
    /// Lines added and removed.
    pub lines: u64,
    /// Of those, the non-trivial ones (`non_trivial`).
    pub non_trivial: u64,
}

/// A turn as the table reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub files: Vec<Changed>,
    /// A check ended failed.
    pub failed: bool,
    /// The turn ran `git merge` or merged a pull request.
    pub merge: bool,
}

impl Shape {
    /// The shape of a turn with `facts`, whose files carry their hunks; a file whose hunk could not
    /// be read counts all its lines as non-trivial. A turn with no files in `facts` reads `diff`,
    /// the change of its card request, which an agent that reports no trees leaves.
    pub fn of(facts: &Facts, diff: &str, merge: bool) -> Shape {
        Shape::read(facts, of_diff(diff), merge, &[])
    }

    /// As `of`, with the files of the card's change already read, and leaving out the files in the
    /// `workers` worktrees, which are judged at the workers' own cards.
    fn read(facts: &Facts, change: Vec<Changed>, merge: bool, workers: &[String]) -> Shape {
        let failed = facts.checks.iter().any(|c| c.status == CheckStatus::Failed);
        let files = match facts.files.is_empty() {
            true => change,
            false => facts
                .files
                .iter()
                .filter(|f| !workers.contains(&f.file.root))
                .map(|f| {
                    let lines = f.file.added.unwrap_or(0) + f.file.removed.unwrap_or(0);
                    let non_trivial = match f.hunk.is_empty() {
                        true => lines,
                        false => non_trivial(&f.hunk),
                    };
                    Changed {
                        path: f.file.path.clone(),
                        lines,
                        non_trivial,
                    }
                })
                .collect(),
        };
        Shape {
            files,
            failed,
            merge,
        }
    }

    fn logic(&self) -> impl Iterator<Item = &Changed> {
        self.files.iter().filter(|f| is_logic(&f.path))
    }

    /// What the turn gets, by the table.
    pub fn offer(&self) -> Offer {
        let (files, lines) = self.logic().fold((0, 0), |(n, l), f| (n + 1, l + f.lines));
        if lines >= CARD_LINES || files >= CARD_FILES {
            return Offer::Card;
        }
        let non_trivial: u64 = self.logic().map(|f| f.non_trivial).sum();
        let test_run = files > 0 && self.logic().all(|f| is_test(&f.path));
        match non_trivial >= QUIZ_LINES && !self.failed && !self.merge && !test_run {
            true => Offer::Quiz,
            false => Offer::Nothing,
        }
    }
}

/// What a decision is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OfferKind {
    Turn,
    Worker,
    LateChange,
}

/// What the hooks and git recorded of a turn, as the offer reads it: the one input both paths share.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnFacts {
    pub facts: Facts,
    /// The turn ran `git merge` or merged a pull request.
    pub merge: bool,
    /// The worktrees that were a worker's home during the turn: their files are judged at the worker's card.
    pub workers: Vec<String>,
}

/// The table's inputs, as the record keeps them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Inputs {
    pub logic_lines: u64,
    pub logic_files: usize,
    pub non_trivial: u64,
    pub failed: bool,
    pub merge: bool,
    pub test_run: bool,
    pub docs: bool,
}

/// What a change is offered, with the inputs and the reason for nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub kind: OfferKind,
    pub offer: Offer,
    pub inputs: Inputs,
    pub reason: Option<&'static str>,
}

/// The one function that decides an offer, by the table (danielrispler/plotkeeper#119) read from `turn`'s
/// files, or from `change`, the files of the card's own diff, where `turn` has none: an agent that
/// reports no trees, and a worker's card. A worker's card, and a subagent's late change, must pass
/// the table's first row, and get no lone Quiz, since neither has a summary.
pub fn decide(kind: OfferKind, turn: &TurnFacts, change: &[Changed]) -> Decision {
    let shape = Shape::read(&turn.facts, change.to_vec(), turn.merge, &turn.workers);
    let logic: Vec<&Changed> = shape.logic().collect();
    let non_trivial: u64 = logic.iter().map(|f| f.non_trivial).sum();
    let inputs = Inputs {
        logic_lines: logic.iter().map(|f| f.lines).sum(),
        logic_files: logic.len(),
        non_trivial,
        failed: shape.failed,
        merge: shape.merge,
        test_run: !logic.is_empty() && logic.iter().all(|f| is_test(&f.path)),
        docs: logic.is_empty() && !shape.files.is_empty(),
    };
    let mut offer = shape.offer();
    let reason = match offer {
        Offer::Card => None,
        _ if kind != OfferKind::Turn => {
            offer = Offer::Nothing;
            Some("below the card row")
        }
        Offer::Quiz => None,
        Offer::Nothing => Some(match () {
            _ if shape.files.is_empty() => "no change",
            _ if logic.is_empty() => "no logic file",
            _ if non_trivial < QUIZ_LINES => "under 8 non-trivial lines",
            _ if inputs.failed => "a check ended failed",
            _ if inputs.merge => "a merge turn",
            _ => "a test run",
        }),
    };
    Decision {
        kind,
        offer,
        inputs,
        reason,
    }
}

/// The files of the diff of a card request.
pub fn changed_in(diff: &str) -> Vec<Changed> {
    of_diff(diff)
}

/// How a decision ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Outcome {
    #[serde(rename = "written")]
    Written,
    #[serde(rename = "held by the hourly cap")]
    HeldByCap,
    #[serde(rename = "no reply from the writer")]
    NoReply,
    #[serde(rename = "no quiz in the reply")]
    NoQuiz,
}

/// One line of the record.
#[derive(Serialize)]
struct Line<'a> {
    at: u64,
    session: &'a str,
    id: &'a str,
    kind: OfferKind,
    inputs: &'a Inputs,
    offer: Offer,
    #[serde(skip_serializing_if = "Option::is_none")]
    outcome: Option<Outcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'static str>,
}

/// The record of offers (`offers.jsonl`), beside the event log: one JSON line for each offer decided,
/// with the table's inputs and how it ended, and no diff and no text of a file. Nothing reads it yet.
pub struct Offers(PathBuf);

impl Offers {
    /// The record beside the event log `log`.
    pub fn beside(log: &Path) -> Offers {
        Offers(log.with_file_name("offers.jsonl"))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Whether the record already holds the final decision on `id` of `session`.
    pub fn settled(&self, session: &str, id: &str) -> bool {
        let text = std::fs::read_to_string(&self.0).unwrap_or_default();
        let no_reply = serde_json::to_value(Outcome::NoReply).unwrap_or_default();
        text.lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .any(|l| {
                l["session"] == session && l["id"] == id && l.get("outcome") != Some(&no_reply)
            })
    }

    /// Appends the line of `decision` on `id` of `session`, ending in `outcome` (or, with none,
    /// in the decision's reason). A failed write is no reason to lose the offer: it is logged.
    /// A decision already on record, other than a writer's silence, is not recorded again: a turn
    /// the app asks about again after a restart adds no line.
    pub fn record(&self, session: &str, id: &str, decision: &Decision, outcome: Option<Outcome>) {
        if self.settled(session, id) {
            return;
        }
        let line = Line {
            at: crate::millis(),
            session,
            id,
            kind: decision.kind,
            inputs: &decision.inputs,
            offer: decision.offer,
            outcome,
            reason: decision.reason,
        };
        let mut text = serde_json::to_string(&line).unwrap_or_default();
        text.push('\n');
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0);
        if let Err(e) = file.and_then(|mut f| f.write_all(text.as_bytes())) {
            eprintln!("save the offers record: {e}");
        }
    }
}

/// What settling a turn's summary found.
pub struct Settled {
    /// Whether the summary is written: it is wanted for itself, or for a lone Quiz.
    pub write: bool,
    pub decision: Decision,
}

/// Fills in `request`'s facts from `turn` and whether its summary may keep a Quiz, from the same
/// decision the turn's card got: a lone Quiz only when the table gives that, and the card's own
/// `change` is read only where `turn` has no files. A `decided` decision, the one the card path made
/// for this turn, is taken as it is: the summary never decides again on facts that moved since.
pub fn settle(
    request: &mut SummaryRequest,
    turn: &TurnFacts,
    change: &[Changed],
    decided: Option<&Decision>,
) -> Settled {
    request.facts = turn.facts.clone();
    let decision = match decided {
        Some(decided) => decided.clone(),
        None => decide(OfferKind::Turn, turn, change),
    };
    let lone = decision.offer == Offer::Quiz;
    request.no_quiz = !lone;
    Settled {
        write: request.wanted || lone,
        decision,
    }
}

/// Writes the stop card of `request` when the table gives it one, and puts the decision on record. A
/// check's card is not judged. See `write_stop` for the rest.
#[allow(clippy::too_many_arguments)]
pub fn offer_stop(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    request: CardRequest,
    session: &Session,
    turn: &TurnFacts,
    offers: &Offers,
    stored: impl FnOnce(),
    mut log: impl FnMut(String),
) -> (bool, Option<Decision>) {
    if request.check.is_some() {
        return (
            write_stop(store, writer, request, session, stored, log),
            None,
        );
    }
    let kind = match () {
        _ if request.agent.is_some() => OfferKind::Worker,
        _ if turn.facts.files.is_empty() && !request.changed_by.is_empty() => OfferKind::LateChange,
        _ => OfferKind::Turn,
    };
    let decision = decide(kind, turn, &changed_in(request.diff()));
    let (id, session_id) = (request.id.clone(), request.session_id.clone());
    match decision.offer {
        Offer::Card => {
            let written = write_stop(store, writer, request, session, stored, &mut log);
            let outcome = if written {
                Outcome::Written
            } else {
                Outcome::NoReply
            };
            offers.record(&session_id, &id, &decision, Some(outcome));
            (written, Some(decision))
        }
        // Its summary carries the lone Quiz, and puts the line there.
        Offer::Quiz => {
            log(format!("no card on {id}: its change gets a lone Quiz"));
            (false, Some(decision))
        }
        Offer::Nothing => {
            let why = decision.reason.unwrap_or_default();
            log(format!("no card on {id}: {why}"));
            offers.record(&session_id, &id, &decision, None);
            (false, Some(decision))
        }
    }
}

/// Writes the summary of `request`, and, when `lone` is the decision that gave the turn a lone Quiz,
/// puts it on record with its outcome.
pub fn offer_summary(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    setup: &SummarySetup,
    request: SummaryRequest,
    lone: Option<&Decision>,
    offers: &Offers,
    log: impl FnMut(String),
) -> Option<TurnSummary> {
    let (session, id) = (request.session_id.clone(), request.id.clone());
    let summary = write_summary(store, writer, setup, request, log);
    if let Some(decision) = lone {
        let outcome = match &summary {
            None => Outcome::NoReply,
            Some(s) if s.quiz.is_some() => Outcome::Written,
            Some(_) if store.lock().unwrap().place_quiz(false, crate::millis()) != Place::Offer => {
                Outcome::HeldByCap
            }
            Some(_) => Outcome::NoQuiz,
        };
        offers.record(&session, &id, decision, Some(outcome));
    }
    summary
}

/// The files of a unified diff: one `diff --git` header per file, as `git diff` has, or only the
/// `---`/`+++` pair of each, as a fingerprint's does.
fn of_diff(diff: &str) -> Vec<Changed> {
    let lines: Vec<&str> = diff.lines().collect();
    // Each file's path and text, and whether its `---`/`+++` pair was seen.
    let mut files: Vec<(String, String, bool)> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let pair = line.starts_with("--- ")
            && lines.get(i + 1).is_some_and(|l| l.starts_with("+++ "))
            && lines.get(i + 2).is_some_and(|l| l.starts_with("@@"));
        if let Some(rest) = line.strip_prefix("diff --git ") {
            let path = rest.rsplit_once(" b/").map_or(rest, |(_, b)| b);
            files.push((path.into(), String::new(), false));
        } else if pair && files.last().is_none_or(|(_, _, seen)| *seen) {
            let new = lines[i + 1].strip_prefix("+++ ").unwrap_or_default();
            files.push((
                new.strip_prefix("b/").unwrap_or(new).into(),
                String::new(),
                true,
            ));
        } else if let Some((_, text, seen)) = files.last_mut() {
            *seen |= pair;
            text.push_str(line);
            text.push('\n');
        }
    }
    files
        .into_iter()
        .map(|(path, text, _)| Changed {
            path,
            lines: changes(&text).count() as u64,
            non_trivial: non_trivial(&text),
        })
        .collect()
}

/// The added and removed lines of a diff, with their sign.
fn changes(diff: &str) -> impl Iterator<Item = &str> {
    let header = |l: &&str| !(l.starts_with("+++") || l.starts_with("---"));
    diff.lines()
        .filter(|l| l.starts_with('+') || l.starts_with('-'))
        .filter(header)
}

/// How many changed lines of `diff` are non-trivial: not blank, a comment, an import, a lone
/// bracket, or formatting only (the line is also removed or added with different spacing).
fn non_trivial(diff: &str) -> u64 {
    let squeeze = |l: &str| -> String { l.split_whitespace().collect() };
    let (added, removed): (Vec<&str>, Vec<&str>) = changes(diff).partition(|l| l.starts_with('+'));
    let squeezed =
        |lines: &[&str]| -> Vec<String> { lines.iter().map(|l| squeeze(&l[1..])).collect() };
    let (added_sq, removed_sq) = (squeezed(&added), squeezed(&removed));
    // Each removed line cancels at most one added line of the same squeezed text, and the reverse.
    let (in_added, in_removed) = (tally(&added_sq), tally(&removed_sq));
    let pairs: HashMap<&str, usize> = in_added
        .iter()
        .filter_map(|(text, n)| Some((*text, (*n).min(*in_removed.get(text)?))))
        .collect();
    let count = |lines: &[&str], squeezed: &[String]| {
        let mut left = pairs.clone();
        let kept = lines.iter().zip(squeezed);
        kept.filter(|(l, sq)| {
            let moved = left.get_mut(sq.as_str()).is_some_and(|n| {
                let cancels = *n > 0;
                *n = n.saturating_sub(1);
                cancels
            });
            !trivial(&l[1..]) && !moved
        })
        .count() as u64
    };
    count(&added, &added_sq) + count(&removed, &removed_sq)
}

/// How many times each text is in `texts`.
fn tally(texts: &[String]) -> HashMap<&str, usize> {
    let mut tally = HashMap::new();
    texts
        .iter()
        .for_each(|t| *tally.entry(t.as_str()).or_default() += 1);
    tally
}

/// A line that is blank, a comment, an import or only brackets and punctuation.
fn trivial(line: &str) -> bool {
    let line = line.trim();
    const COMMENTS: [&str; 6] = ["//", "#", "/*", "*", "--", "<!--"];
    const IMPORTS: [&str; 6] = [
        "use ", "import ", "from ", "#include", "pub use ", "export {",
    ];
    line.chars().all(|c| !c.is_alphanumeric())
        || COMMENTS.iter().any(|c| line.starts_with(c))
        || IMPORTS.iter().any(|i| line.starts_with(i))
}

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
fn is_test(path: &str) -> bool {
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

    fn file(path: &str, lines: u64, non_trivial: u64) -> Changed {
        Changed {
            path: path.into(),
            lines,
            non_trivial,
        }
    }

    fn shape(files: Vec<Changed>) -> Shape {
        Shape {
            files,
            ..Shape::default()
        }
    }

    #[test]
    fn a_card_at_25_lines_or_two_logic_files() {
        let one = |lines| shape(vec![file("a.rs", lines, lines)]).offer();
        assert_eq!(one(24), Offer::Quiz);
        assert_eq!(one(25), Offer::Card);
        let two = shape(vec![file("a.rs", 3, 3), file("b.rs", 3, 3)]).offer();
        assert_eq!(two, Offer::Card);
        // A doc beside a logic file is one logic file.
        let doc = shape(vec![file("a.rs", 9, 9), file("docs/x.md", 90, 90)]).offer();
        assert_eq!(doc, Offer::Quiz);
    }

    #[test]
    fn a_lone_quiz_at_8_non_trivial_lines() {
        let one = |n| shape(vec![file("a.rs", 12, n)]).offer();
        assert_eq!(one(7), Offer::Nothing);
        assert_eq!(one(8), Offer::Quiz);
    }

    #[test]
    fn a_failed_check_a_merge_or_a_test_run_gets_no_lone_quiz() {
        let base = shape(vec![file("a.rs", 12, 12)]);
        let failed = Shape {
            failed: true,
            ..base.clone()
        };
        let merge = Shape {
            merge: true,
            ..base.clone()
        };
        let tests = shape(vec![file("tests/a.rs", 12, 12)]);
        let spec = shape(vec![file("ui/a.test.mjs", 12, 12)]);
        for s in [failed, merge, tests, spec] {
            assert_eq!(s.offer(), Offer::Nothing);
        }
        assert_eq!(base.offer(), Offer::Quiz);
    }

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
            assert_eq!(shape(vec![file(path, 90, 90)]).offer(), Offer::Nothing);
        }
        assert!(is_logic("core/src/lib.rs") && is_logic("core/tests/a.rs"));
    }

    #[test]
    fn blank_comment_import_and_formatting_lines_are_trivial() {
        let diff = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,9 @@\n\
                    +use std::fs;\n+\n+// a note\n+}\n+let a = 1;\n-let   b = 2;\n+let b = 2;\n+let c = 3;\n";
        let files = of_diff(diff);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].lines, 8);
        // `let a`, `let c`: the reindented `let b` is formatting only.
        assert_eq!(files[0].non_trivial, 2);
    }

    #[test]
    fn a_removed_line_cancels_only_one_added_line_of_the_same_text() {
        let diff = format!(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1,8 @@\n-  return None;\n{}",
            "+return None;\n".repeat(8)
        );
        assert_eq!(of_diff(&diff)[0].non_trivial, 7);
    }
}
