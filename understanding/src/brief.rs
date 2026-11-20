//! The Brief's slots that no model writes (danielrispler/plotkeeper#122): Decisions, from the decision
//! history, and Open, computed from what the core recorded. The Goal is the only slot a model writes
//! (`write_goal`).

use crate::{CardStore, CardWriter, LineKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use studio_core::{Answer, Check, CheckStatus, Decision, Session, TaskFacts};

/// A question the agent asked, with Daniel's reply.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Answered {
    pub question: String,
    pub reply: String,
}

/// What an Open item is, which the pane marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenKind {
    Question,
    Check,
    Unchecked,
    Open,
    Ask,
    Unpushed,
    Pr,
}

/// One line of the Open slot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OpenItem {
    pub kind: OpenKind,
    pub text: String,
}

/// The Decisions slot: each question of `decisions` with Daniel's reply, in the order asked.
pub fn answered(decisions: &[Decision]) -> Vec<Answered> {
    let asked = decisions
        .iter()
        .filter(|d| d.tool == "AskUserQuestion")
        .filter_map(|d| Some((d.input["questions"].as_array()?, d.answer.as_object()?)));
    asked
        .flat_map(|(questions, replies)| {
            questions.iter().filter_map(|asked| {
                let question = asked["question"].as_str()?;
                let reply = replies.get(question)?.as_str()?;
                Some(Answered {
                    question: question.into(),
                    reply: reply.into(),
                })
            })
        })
        .collect()
}

/// The Open slot of `session`, from facts: the questions it waits on, each check suite whose last
/// run failed, files changed with no check run since, the latest ended turn's `open` and `ask`
/// lines, and, with `task`'s facts, unpushed work and an unmerged PR. Each drops out once settled:
/// the question answered, the suite passing in a later turn, a check run, the next turn started, the
/// work pushed and merged.
pub fn open_items(session: &Session, task: Option<TaskFacts>, store: &CardStore) -> Vec<OpenItem> {
    let item = |kind, text: String| OpenItem { kind, text };
    let mut open = Vec::new();
    for question in &session.questions {
        if question.answer == Answer::Open {
            let first = question.questions[0]["question"].as_str();
            open.push(item(
                OpenKind::Question,
                first.unwrap_or("A question").into(),
            ));
        }
    }
    let mut suites = BTreeMap::new();
    // A check that has not ended changes nothing.
    let ended = |check: &&Check| check.status != CheckStatus::Running;
    for check in session
        .turns
        .iter()
        .flat_map(|turn| &turn.checks)
        .filter(ended)
    {
        suites.insert(&check.suite, check.status);
    }
    for (suite, status) in suites {
        if status == CheckStatus::Failed {
            open.push(item(OpenKind::Check, format!("{suite} ended failed")));
        }
    }
    let changed = session
        .turns
        .iter()
        .rposition(|turn| turn.files.as_ref().is_some_and(|f| !f.files.is_empty()));
    if let Some(at) = changed
        && session.turns[at..]
            .iter()
            .all(|turn| turn.checks.is_empty())
    {
        open.push(item(
            OpenKind::Unchecked,
            "Files changed, no check run since".into(),
        ));
    }
    let latest = session.turns.last().filter(|turn| turn.ended_at.is_some());
    let lines = latest.and_then(|turn| store.turn_summary(turn.id.as_deref()?));
    for line in lines.iter().flat_map(|kept| &kept.summary.lines) {
        match line.kind {
            LineKind::Open => open.push(item(OpenKind::Open, line.text.clone())),
            LineKind::Ask => open.push(item(OpenKind::Ask, line.text.clone())),
            _ => {}
        }
    }
    if let Some(task) = task {
        if !task.safe {
            open.push(item(
                OpenKind::Unpushed,
                "Uncommitted or unpushed work".into(),
            ));
        }
        if let Some(pr) = session.identity.pr
            && !task.merged
        {
            open.push(item(OpenKind::Pr, format!("PR #{pr} is not merged")));
        }
    }
    open
}

/// The most lines "Design so far" holds.
const DESIGN_LINES: usize = 5;

/// One line of "Design so far" (danielrispler/plotkeeper#123): a design decision with its reason, and the
/// id of the turn or card it came from, which the rail opens.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignLine {
    pub text: String,
    pub cite: String,
}

/// What the Design writer reads: the `why` lines of the turns of `turns` that have a summary, and the
/// summaries of `owner`'s cards, each with its id, oldest first.
pub fn design_sources(store: &CardStore, owner: &str, turns: &[String]) -> Vec<(String, String)> {
    let mut sources = Vec::new();
    for id in turns {
        let Some(kept) = store.turn_summary(id) else {
            continue;
        };
        for line in kept
            .summary
            .lines
            .iter()
            .filter(|l| l.kind == LineKind::Why)
        {
            sources.push((id.clone(), line.text.clone()));
        }
    }
    for kept in store.cards().iter().filter(|k| k.request.owner() == owner) {
        if !kept.card.summary.is_empty() {
            sources.push((kept.request.id.clone(), kept.card.summary.clone()));
        }
    }
    sources
}

/// The lines of a writer's `reply` that cite an id of `known`, the turns and cards of the session,
/// at most `DESIGN_LINES`. A line with no citation, or one that cites something not in the session,
/// is dropped, and so is a line past the fifth. Returns the kept lines and how many were dropped.
pub fn design_lines(reply: &str, known: &BTreeSet<&str>) -> Option<(Vec<DesignLine>, usize)> {
    let value: serde_json::Value = serde_json::from_str(crate::object(reply)).ok()?;
    let asked = value["design"].as_array()?;
    let mut kept = Vec::new();
    for line in asked {
        let text = line["text"].as_str().map(str::trim).unwrap_or_default();
        let cite = line["cite"].as_str().map(str::trim).unwrap_or_default();
        if !text.is_empty() && known.contains(cite) && kept.len() < DESIGN_LINES {
            kept.push(DesignLine {
                text: text.into(),
                cite: cite.into(),
            });
        }
    }
    let dropped = asked.len() - kept.len();
    Some((kept, dropped))
}

/// Writes "Design so far" of `owner`, a session, from `sources` (`design_sources`) once they changed
/// since it was written (danielrispler/plotkeeper#123). `turns` are the session's turn ids; a citation must
/// be one of them or a card of `owner`. A session with no sources has none. Each problem is one line
/// for `log`. Returns whether the lines changed.
pub fn write_design(
    store: &Mutex<CardStore>,
    writer: &(impl CardWriter + ?Sized),
    owner: &str,
    turns: &[String],
    mut log: impl FnMut(String),
) -> bool {
    let (sources, mut known): (_, BTreeSet<String>) = {
        let store = store.lock().unwrap();
        let cards = store.cards().iter().filter(|k| k.request.owner() == owner);
        let ids = cards.map(|kept| kept.request.id.clone());
        (design_sources(&store, owner, turns), ids.collect())
    };
    known.extend(turns.iter().cloned());
    let written = store.lock().unwrap().brief(owner).map(|b| b.design_from);
    if sources.is_empty() || written == Some(sources.len() as u32) {
        return false;
    }
    let reply = writer.design(&sources);
    let known: BTreeSet<&str> = known.iter().map(String::as_str).collect();
    match reply.and_then(|r| design_lines(&r, &known).ok_or("the reply holds none".to_string())) {
        Ok((lines, dropped)) => {
            if dropped > 0 {
                log(format!("{dropped} design lines of {owner} dropped"));
            }
            let saved = store
                .lock()
                .unwrap()
                .set_design(owner, lines, sources.len() as u32);
            match saved {
                Ok(()) => true,
                Err(e) => {
                    log(format!("save the card store: {e}"));
                    false
                }
            }
        }
        Err(line) => {
            log(format!("no design on {owner}: {line}"));
            false
        }
    }
}
