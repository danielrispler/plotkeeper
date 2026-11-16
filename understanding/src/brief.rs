//! The Brief's slots that no model writes (danielrispler/plotkeeper#122): Decisions, from the decision
//! history, and Open, computed from what the core recorded. The Goal is the only slot a model writes
//! (`write_goal`).

use crate::{CardStore, LineKind};
use serde::Serialize;
use std::collections::BTreeMap;
use studio_core::{Answer, CheckStatus, Decision, Session, TaskFacts};

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
    for check in session.turns.iter().flat_map(|turn| &turn.checks) {
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
