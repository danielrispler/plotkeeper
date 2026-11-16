//! The Brief's slots (danielrispler/plotkeeper#122): Open is computed from the core's facts and the
//! turns' summaries, Decisions from the decision history.

use serde_json::json;
use studio_core::{Check, Decision, Session, SummaryRequest, TaskFacts, Turn};
use studio_understanding::{
    CardStore, Line, LineKind, OpenKind, TurnSummary, answered, open_items,
};

fn line(kind: LineKind, text: &str) -> Line {
    Line {
        kind,
        text: text.into(),
    }
}

/// A turn that ended, named `id`, with the files and the checks `files` and `checks` say.
fn turn(id: &str, files: bool, checks: &[(&str, &str)]) -> Turn {
    let checks: Vec<Check> = checks
        .iter()
        .map(|(suite, status)| {
            let check = json!({"suite": suite, "status": status, "retries": 0, "runs": []});
            serde_json::from_value(check).unwrap()
        })
        .collect();
    let files = files.then(|| {
        let file = json!({"root": "/r", "path": "a.rs", "from": null, "added": 1, "removed": 0});
        serde_json::from_value(json!({"files": [file], "external": 0})).unwrap()
    });
    let mut turn = Turn::default();
    (turn.ended_at, turn.id) = (Some(2), Some(id.into()));
    (turn.files, turn.checks) = (files, checks);
    turn
}

/// A session with an open question, a suite that ended failed, and a second turn that changed files
/// and ran no check.
fn session() -> Session {
    let question = json!({"tool_use_id": "t1", "agent": null, "turn": 0, "answer": "open",
        "questions": [{"question": "Anchor the hunks under a ref?", "options": []}]});
    let mut session = Session::default();
    session.questions = vec![serde_json::from_value(question).unwrap()];
    session.turns = vec![
        turn("s@1", true, &[("cargo test", "failed")]),
        turn("s@2", true, &[]),
    ];
    session.identity.pr = Some(7);
    session
}

/// A store holding the canned summary of turn `s@2`, with an `open` and an `ask` line.
fn store(dir: &std::path::Path) -> CardStore {
    let mut store = CardStore::open(dir.join("cards.json")).unwrap();
    let request = SummaryRequest {
        id: "s@2".into(),
        ..SummaryRequest::default()
    };
    let summary = TurnSummary {
        summary: "Done.".into(),
        headline: Some("Done.".into()),
        lines: vec![
            line(LineKind::Why, "It was the cheaper way."),
            line(LineKind::Open, "The hunk cap is untested."),
            line(LineKind::Ask, "Should the ref be pruned?"),
        ],
        quiz: None,
    };
    store.add_summary(request, summary).unwrap();
    store
}

const UNSETTLED: TaskFacts = TaskFacts {
    safe: false,
    merged: false,
    issue_closed: false,
};

fn kinds(session: &Session, task: Option<TaskFacts>, store: &CardStore) -> Vec<OpenKind> {
    open_items(session, task, store)
        .iter()
        .map(|item| item.kind)
        .collect()
}

#[test]
fn open_lists_each_unsettled_thing_and_drops_each_once_it_is_settled() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let mut session = session();

    let items = open_items(&session, Some(UNSETTLED), &store);
    let texts: Vec<_> = items.iter().map(|i| (i.kind, i.text.as_str())).collect();
    assert_eq!(
        texts,
        [
            (OpenKind::Question, "Anchor the hunks under a ref?"),
            (OpenKind::Check, "cargo test ended failed"),
            (OpenKind::Unchecked, "Files changed, no check run since"),
            (OpenKind::Open, "The hunk cap is untested."),
            (OpenKind::Ask, "Should the ref be pruned?"),
            (OpenKind::Unpushed, "Uncommitted or unpushed work"),
            (OpenKind::Pr, "PR #7 is not merged"),
        ]
    );

    // The question is answered.
    session.questions[0].answer = studio_core::Answer::NotAnswered;
    assert!(!kinds(&session, Some(UNSETTLED), &store).contains(&OpenKind::Question));
    // The work is pushed and its PR merged.
    let landed = TaskFacts {
        safe: true,
        merged: true,
        ..UNSETTLED
    };
    let after = kinds(&session, Some(landed), &store);
    assert!(!after.contains(&OpenKind::Unpushed) && !after.contains(&OpenKind::Pr));
    // A later turn passes the suite, which also is a check run since the files changed. Its own
    // summary has no lines, so the earlier turn's open and ask lines go with it.
    session
        .turns
        .push(turn("s@3", false, &[("cargo test", "passed")]));
    assert_eq!(kinds(&session, Some(landed), &store), []);
}

#[test]
fn a_turn_still_running_drops_the_lines_of_the_one_before_and_no_task_facts_show_no_git_items() {
    let dir = tempfile::tempdir().unwrap();
    let store = store(dir.path());
    let mut session = session();
    session.turns.push(Turn::default());

    let shown = kinds(&session, None, &store);

    assert!(!shown.contains(&OpenKind::Open) && !shown.contains(&OpenKind::Ask));
    assert!(!shown.contains(&OpenKind::Unpushed) && !shown.contains(&OpenKind::Pr));
}

#[test]
fn decisions_list_each_answered_question_with_daniels_reply_in_order() {
    let decision = |call: &str, tool: &str, input, answer| -> Decision {
        serde_json::from_value(
            json!({"call": call, "agent": null, "tool": tool, "input": input,
            "answer": answer, "at": 1, "source": "card"}),
        )
        .unwrap()
    };
    let decisions = [
        decision(
            "t1",
            "AskUserQuestion",
            json!({"questions": [{"question": "Which store?"}, {"question": "Prune when?"}]}),
            json!({"Which store?": "Refs", "Prune when?": "At merge"}),
        ),
        decision("t2", "Bash", json!({}), json!("allow")),
        decision(
            "t3",
            "AskUserQuestion",
            json!({"questions": [{"question": "Cap?"}]}),
            json!({"Cap?": "40 lines"}),
        ),
    ];

    let shown: Vec<_> = answered(&decisions)
        .into_iter()
        .map(|a| (a.question, a.reply))
        .collect();

    let pairs = [
        ("Which store?", "Refs"),
        ("Prune when?", "At merge"),
        ("Cap?", "40 lines"),
    ];
    let pairs: Vec<_> = pairs.map(|(q, r)| (q.to_string(), r.to_string())).into();
    assert_eq!(shown, pairs);
}
