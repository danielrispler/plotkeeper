//! Seam 1 (danielrispler/Falinks#434, Testing Decisions) for the rail's feed of turns (danielrispler/Falinks#574): each turn of a session,
//! with the checks, subagents and questions that started in it. The fixture is a replayed day of
//! ordinary tickets; session 477b6b89 has 9 prompts, wake-ups among them, checks, subagents and
//! questions.

use studio_core::{Session, Studio};

const DAY: &str = include_str!("fixtures/ordinary-tickets.jsonl");
const SESSION: &str = "477b6b89-5461-4ed2-9396-05daadff815a";

fn session(studio: &Studio) -> &Session {
    studio.session(SESSION).expect("the session")
}

/// The index of the turn that runs at `at`: the latest that started at or before it.
fn turn_at(session: &Session, at: u64) -> Option<usize> {
    session.turns.iter().rposition(|t| t.started_at <= at)
}

/// One replay of the day, three views of the feed: how a prompt opens a turn; how a stopped turn
/// ends (its end, card id and the first line of its reply); and how the checks, subagents and
/// questions group under the turns by when they started.
#[test]
fn the_feed_of_turns_opens_by_prompts_ends_by_stops_and_groups_what_started_in_each() {
    let studio = Studio::replayed(DAY.lines());
    let session = session(&studio);

    // Each prompt opens a turn named by its first line or by the worker that returned.
    let starts: Vec<u64> = session.turns.iter().map(|t| t.started_at).collect();
    let prompts: Vec<u64> = session.prompts.iter().map(|p| p.at).collect();
    assert_eq!(starts, prompts);
    let first = &session.turns[0];
    assert!(!first.prompt.is_empty() && !first.prompt.contains('\n'));
    assert!(
        session.turns[1].prompt.starts_with("worker "),
        "{}",
        session.turns[1].prompt
    );
    assert!(session.turns[1].prompt.ends_with(" returned"));

    // A stopped turn has its end, its card id and its reply's first line.
    let last = session.turns.last().expect("a turn");
    let stopped = session.stopped.as_ref().expect("its latest Stop");
    assert_eq!(last.id.as_ref(), Some(&stopped.card));
    assert_eq!(last.message, stopped.message);
    assert_eq!(last.id, last.ended_at.map(|at| format!("{SESSION}@{at}")));
    for pair in session.turns.windows(2) {
        let ended = pair[0].ended_at.expect("an earlier turn ended");
        assert!(ended <= pair[1].started_at);
    }
    let reply = studio.turn_reply(SESSION, session.turns.len() - 1).unwrap();
    assert!(reply.starts_with(last.message.as_deref().unwrap()));

    // Turns group their checks, subagents and questions by when they started.
    assert_eq!(session.checks.len(), 9);
    for check in &session.checks {
        assert_eq!(
            check.turn,
            turn_at(session, check.started_at),
            "{}",
            check.command
        );
        assert!(check.turn.is_some());
    }
    for agent in &session.agents {
        assert_eq!(
            agent.turn,
            turn_at(session, agent.agent.started_at),
            "{}",
            agent.name
        );
        assert!(agent.turn.is_some());
    }
    assert_eq!(session.questions.len(), 4);
    assert!(session.questions.iter().all(|q| q.turn.is_some()));
    // The checks fall in three turns, so the feed shows them under those three and no other.
    let mut turns: Vec<usize> = session.checks.iter().filter_map(|c| c.turn).collect();
    turns.dedup();
    assert_eq!(turns.len(), 3);
}
