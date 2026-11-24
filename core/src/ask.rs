//! The facts Ask shows first (danielrispler/plotkeeper#125): what the step ledger and the event log recorded about
//! one thing Daniel asks about, with no model in it. Details come through the ledger's offsets, one seek and
//! one read each; nothing scans the log.

use crate::{Studio, checks::output, ledger::Row};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most steps listed before a first edit, and the most listed for a subagent.
const BEFORE: usize = 8;
const STEPS: usize = 30;

/// A thing Daniel asks about.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    /// A file a turn changed, in worktree `root`.
    File {
        turn: usize,
        root: String,
        path: String,
        /// Its path before the turn, when the turn renamed it.
        #[serde(default)]
        from: Option<String>,
    },
    /// A check's run, by its call and the worker that made it.
    Check { agent: Option<String>, call: String },
    /// A subagent's line.
    Subagent { agent: String },
    /// A turn's Headline.
    Headline { turn: usize },
}

/// One recorded fact: when it happened, in Unix milliseconds, when that is known.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskFact {
    pub at: Option<u64>,
    pub text: String,
}

/// Facts under a label.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AskSection {
    pub label: String,
    pub facts: Vec<AskFact>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AskFacts {
    pub sections: Vec<AskSection>,
}

impl AskFacts {
    fn add(&mut self, label: &str, facts: Vec<AskFact>) {
        if !facts.is_empty() {
            self.sections.push(AskSection {
                label: label.into(),
                facts,
            });
        }
    }

    fn line(&mut self, label: &str, text: impl Into<String>) {
        self.add(
            label,
            vec![AskFact {
                at: None,
                text: text.into(),
            }],
        );
    }
}

impl AskFacts {
    /// The facts as lines of text under their labels, which the writer reads.
    pub fn text(&self) -> String {
        self.sections
            .iter()
            .map(|s| {
                let lines = s.facts.iter().map(|f| f.text.as_str());
                format!("{}:\n{}", s.label, lines.collect::<Vec<_>>().join("\n"))
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

fn edits(row: &Row) -> bool {
    crate::turn_files::edits(&row.tool)
}

/// Whether `row` works on `path` of worktree `root`.
fn on_file(row: &Row, root: &str, path: &str) -> bool {
    row.file
        .as_deref()
        .is_some_and(|f| f == path || f == format!("{root}/{path}"))
}

/// What a step did, in a few words: its tool and what it worked on.
fn describe(row: &Row) -> String {
    let target = row
        .file
        .as_deref()
        .or(row.pattern.as_deref())
        .or(row.command.as_deref())
        .unwrap_or("");
    format!("{} {target}", row.tool).trim().to_string()
}

impl Studio {
    /// What was recorded in turn `turn` about `name`, a file, a symbol or a phrase (danielrispler/plotkeeper#126): the
    /// steps that changed, read or searched it, the commands that name it, and where Daniel's prompt or the agent's
    /// replies mention it. No sections when the turn never touched it. `None` when the session or turn is not known.
    pub fn ask_named(&self, session: &str, turn: usize, name: &str) -> Option<AskFacts> {
        let session = self.session(session)?;
        let turn_ = session.turns.get(turn)?;
        let name = name.trim().to_lowercase();
        let has = |text: &str| !name.is_empty() && text.to_lowercase().contains(&name);
        let mut changed = vec![];
        let mut read = vec![];
        let mut searched = vec![];
        let mut named = vec![];
        for row in session.ledger.turn(turn) {
            let fact = AskFact {
                at: row.started_at,
                text: describe(row),
            };
            let on = |text: &Option<String>| text.as_deref().is_some_and(has);
            if edits(row) && on(&row.file) {
                changed.push(fact);
            } else if row.tool == "Read" && on(&row.file) {
                read.push(fact);
            } else if matches!(row.tool.as_str(), "Grep" | "Glob")
                && (on(&row.pattern) || on(&row.file))
            {
                searched.push(fact);
            } else if on(&row.command) || on(&row.file) || on(&row.pattern) {
                named.push(fact);
            }
        }
        let mut facts = AskFacts::default();
        facts.add("Changed it", changed);
        facts.add("Read it", read);
        facts.add("Searched for it", searched);
        facts.add("Named in a command", named);
        if has(&turn_.prompt) {
            facts.line("Named in Daniel's prompt", turn_.prompt.clone());
        }
        // The transcript holds the replies of the newest turn only.
        if turn + 1 == session.turns.len() {
            let replies: Vec<AskFact> = session
                .transcript
                .responses
                .iter()
                .filter(|r| has(r))
                .map(|r| AskFact {
                    at: None,
                    text: r.lines().next().unwrap_or_default().to_string(),
                })
                .collect();
            facts.add("Named in the agent's replies", replies);
        }
        Some(facts)
    }

    /// The recorded facts of `anchor` in session `session`: from its ledger, and from the event log at `log` through the
    /// ledger's offsets. `None` when the session or the thing is not known.
    pub fn ask_facts(&self, session: &str, anchor: &Anchor, log: &Path) -> Option<AskFacts> {
        let session = self.session(session)?;
        let ledger = &session.ledger;
        let who = |agent: &Option<String>| match agent {
            None => "the session's agent".to_string(),
            Some(id) => session
                .agents
                .iter()
                .find(|a| &a.agent.id == id)
                .map_or(id.clone(), |a| a.name.clone()),
        };
        let mut facts = AskFacts::default();
        match anchor {
            Anchor::File {
                turn, root, path, ..
            } => {
                let turn_ = session.turns.get(*turn)?;
                facts.line("Daniel asked", turn_.prompt.clone());
                let rows: Vec<&Row> = ledger.turn(*turn).collect();
                let edited: Vec<&Row> = rows
                    .iter()
                    .copied()
                    .filter(|r| edits(r) && on_file(r, root, path))
                    .collect();
                facts.add(
                    "Steps that edited it",
                    edited
                        .iter()
                        .map(|r| AskFact {
                            at: r.started_at,
                            text: format!("{} by {}", r.tool, who(&r.agent)),
                        })
                        .collect(),
                );
                if let Some(first) = edited.first() {
                    let at = rows.iter().position(|r| std::ptr::eq(*r, *first))?;
                    let before: Vec<AskFact> = rows[..at]
                        .iter()
                        .filter(|r| r.agent == first.agent && !edits(r))
                        .map(|r| AskFact {
                            at: r.started_at,
                            text: describe(r),
                        })
                        .collect();
                    let skip = before.len().saturating_sub(BEFORE);
                    facts.add(
                        "Read or searched before the first edit",
                        before.into_iter().skip(skip).collect(),
                    );
                }
            }
            Anchor::Check { agent, call } => {
                let row = ledger.row(agent.as_deref(), call)?;
                facts.line("Command", row.command.clone().unwrap_or_default());
                match output(row, log) {
                    Ok(out) => {
                        let code = out
                            .exit_code
                            .map_or("The log does not say".to_string(), |c| c.to_string());
                        facts.line("Exit code", code);
                        facts.line("End of the output", out.tail);
                    }
                    Err(e) => facts.line("Output", format!("It could not be read: {e}")),
                }
            }
            Anchor::Subagent { agent } => {
                let worker = session.agents.iter().find(|a| &a.agent.id == agent)?;
                facts.line("Its prompt", worker.prompt.clone());
                let rows: Vec<&Row> = ledger
                    .rows()
                    .iter()
                    .filter(|r| r.agent.as_deref() == Some(agent))
                    .collect();
                facts.add(
                    "Its steps",
                    rows.iter()
                        .take(STEPS)
                        .map(|r| AskFact {
                            at: r.started_at,
                            text: describe(r),
                        })
                        .collect(),
                );
                let mut files: Vec<&str> = rows
                    .iter()
                    .filter(|r| edits(r))
                    .filter_map(|r| r.file.as_deref())
                    .collect();
                files.sort();
                files.dedup();
                facts.add(
                    "Files it changed",
                    files
                        .into_iter()
                        .map(|f| AskFact {
                            at: None,
                            text: f.into(),
                        })
                        .collect(),
                );
            }
            Anchor::Headline { turn } => {
                let turn_ = session.turns.get(*turn)?;
                facts.line("Daniel asked", turn_.prompt.clone());
                if let Some(files) = &turn_.files {
                    let count = |n: Option<u64>| n.map_or("?".to_string(), |n| n.to_string());
                    facts.add(
                        "Files",
                        files
                            .files
                            .iter()
                            .map(|f| AskFact {
                                at: None,
                                text: format!(
                                    "{} +{} −{}",
                                    f.path,
                                    count(f.added),
                                    count(f.removed)
                                ),
                            })
                            .collect(),
                    );
                }
                facts.add(
                    "Checks",
                    turn_
                        .checks
                        .iter()
                        .map(|c| AskFact {
                            at: None,
                            text: format!("{} {:?}", c.suite, c.status).to_lowercase(),
                        })
                        .collect(),
                );
            }
        }
        Some(facts)
    }
}
