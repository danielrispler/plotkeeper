//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for phases and the estimate (danielrispler/Falinks#461, decision 14): replay real
//! sessions, scrubbed (`ordinary-tickets.jsonl`: seven `/implement` ticket sessions and one
//! `/wayfinder` session of danielrispler/Falinks#338's, every text replaced by "…", commands kept only as the phase
//! signals they carry), and check the phases each passes through and the estimate it shows.

use studio_core::{Phase, Studio, Work};

const ORDINARY: &str = include_str!("fixtures/ordinary-tickets.jsonl");

/// The session whose id starts with `prefix`, in `studio`.
fn session<'a>(studio: &'a Studio, prefix: &str) -> &'a studio_core::Session {
    let sessions = studio.sessions();
    sessions
        .into_iter()
        .find(|s| s.agent.id.starts_with(prefix))
        .unwrap()
}

#[test]
fn an_ordinary_ticket_session_passes_through_its_phases_in_order() {
    let studio = Studio::replayed(ORDINARY.lines());
    // Issue 365's: a baseline check, tests, a review, the PR, its CI and the merge.
    let work = &session(&studio, "1a53da9c").work;
    let phases: Vec<Phase> = work.phases.iter().map(|p| p.phase).collect();
    assert_eq!(
        phases,
        [
            Phase::Orienting,
            Phase::Implementing,
            Phase::Reviewing,
            Phase::OpeningPr,
            Phase::WaitingOnCi,
            Phase::Merged,
        ]
    );
    assert_eq!(work.kind.as_deref(), Some("implement"));
}

#[test]
fn kinds_of_work_with_fewer_than_20_past_sessions_show_no_estimate() {
    // The log holds seven `/implement` sessions and one `/wayfinder` one, so no kind has 20.
    let studio = Studio::replayed(ORDINARY.lines());
    let sessions = studio.sessions();
    assert_eq!(sessions.len(), 8);
    for session in sessions {
        assert!(session.work.phases.len() > 1, "{}", session.agent.id);
        assert_eq!(session.work.estimate.range, None, "{}", session.agent.id);
    }
}

/// Each ordinary ticket session's real remaining work from each phase after its first, against the
/// range the other six give: leave-one-out, as the log holds too few for 20 past sessions each.
#[test]
fn on_replayed_ticket_sessions_the_range_holds_the_real_remaining_time_in_2_of_3() {
    let studio = Studio::replayed(ORDINARY.lines());
    let tickets: Vec<&Work> = studio
        .sessions()
        .into_iter()
        .map(|s| &s.work)
        .filter(|w| w.kind.as_deref() == Some("implement"))
        .collect();
    assert_eq!(tickets.len(), 7);
    let (mut held, mut tried) = (0, 0);
    for (i, work) in tickets.iter().enumerate() {
        let others: Vec<&Work> = tickets
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, w)| *w)
            .collect();
        for start in work
            .phases
            .iter()
            .skip(1)
            .filter(|p| p.phase != Phase::Merged)
        {
            let real = work.agent_ms - start.agent_ms;
            let (low, high) = studio_core::range(&others, start.phase, 3).unwrap();
            tried += 1;
            if (low..=high).contains(&real) {
                held += 1;
            }
            eprintln!(
                "{} {:?} real {}m range {}m..{}m",
                i,
                start.phase,
                real / MIN,
                low / MIN,
                high / MIN
            );
        }
    }
    eprintln!("held {held} of {tried}");
    assert!(tried >= 15, "{tried}");
    assert!(3 * held >= 2 * tried, "held {held} of {tried}");
}

const MIN: u64 = 60_000;
const T0: u64 = 1_793_600_000_000;
const MADE_UP: &str = "5e1f0c2a-0000-4a00-8000-00000000c475";

/// A made-up hook event of session `id` at `T0 + at`, with `fields` (JSON object members).
fn event(id: &str, name: &str, at: u64, fields: &str) -> String {
    let extra = if fields.is_empty() {
        String::new()
    } else {
        format!(",{fields}")
    };
    format!(
        r#"{{"session_id":"{id}","hook_event_name":"{name}","cwd":"/w","studio_received_ms":{}{extra}}}"#,
        T0 + at
    )
}

fn bash(id: &str, call: &str, at: u64, command: &str) -> [String; 2] {
    let input = format!(
        r#""tool_name":"Bash","tool_use_id":"{call}","tool_input":{{"command":"{command}"}}"#
    );
    [
        event(id, "PreToolUse", at, &input),
        event(id, "PostToolUse", at + 1_000, &input),
    ]
}

/// A made-up `/implement` session `n`, starting at `start` minutes: it orients for 1 minute, tests for
/// `implementing` minutes, runs the full checks for 2, then opens its PR, stops 1 minute later and
/// ends.
fn ticket(n: usize, start: u64, implementing: u64) -> Vec<String> {
    let id = format!("5e1f0c2a-0000-4a00-8000-{n:012}");
    let at = |m: u64| (start + m) * MIN;
    let mut lines = vec![
        event(&id, "SessionStart", at(0), r#""source":"startup""#),
        event(
            &id,
            "UserPromptSubmit",
            at(0),
            r#""prompt":"/implement issue 1""#,
        ),
    ];
    lines.extend(bash(&id, "b1", at(1), "cargo test"));
    lines.extend(bash(
        &id,
        "b2",
        at(1 + implementing),
        "cargo run --bin ci-checks",
    ));
    lines.extend(bash(&id, "b3", at(3 + implementing), "gh pr create --fill"));
    lines.push(event(&id, "Stop", at(4 + implementing), ""));
    lines.push(event(
        &id,
        "SessionEnd",
        at(5 + implementing),
        r#""reason":"other""#,
    ));
    lines
}

#[test]
fn with_20_past_sessions_of_its_kind_a_session_shows_a_range_that_updates_as_phases_end() {
    // 20 past sessions implemented in 10 to 29 minutes, one after another.
    let past: Vec<String> = (0..20)
        .flat_map(|n| ticket(n, 100 * n as u64, 10 + n as u64))
        .collect();
    let now = ticket(99, 5_000, 15);
    let shown = |past: &[String], lines: usize| {
        let studio = Studio::replayed(past.iter().chain(&now[..lines]));
        let id = "5e1f0c2a-0000-4a00-8000-000000000099";
        studio.session(id).unwrap().work.estimate.range
    };
    // At its prompt: 1 + 10..29 + 3 minutes, from a sixth in to a sixth from the end.
    assert_eq!(shown(&past, 2), Some((17 * MIN, 30 * MIN)));
    // Its tests start implementing: 1 minute less.
    assert_eq!(shown(&past, 3), Some((16 * MIN, 29 * MIN)));
    // Its full checks: 3 minutes are left in every past session.
    assert_eq!(shown(&past, 5), Some((3 * MIN, 3 * MIN)));
    // With one past session less, no estimate.
    assert_eq!(shown(&past[ticket(0, 0, 10).len()..], 3), None);
}

#[test]
fn past_sessions_that_have_not_ended_do_not_count_toward_the_estimate() {
    // The same 20 past sessions, but each only paused between turns: their times may be cut short.
    let past: Vec<String> = (0..20)
        .flat_map(|n| ticket(n, 100 * n as u64, 10 + n as u64))
        .filter(|line| !line.contains("SessionEnd"))
        .collect();
    let now = ticket(99, 5_000, 15);
    let studio = Studio::replayed(past.iter().chain(&now[..3]));
    let id = "5e1f0c2a-0000-4a00-8000-000000000099";
    assert_eq!(studio.session(id).unwrap().work.estimate.range, None);
}

#[test]
fn the_agents_time_leaves_out_time_between_turns_and_time_that_needs_daniel() {
    let ask = r#""tool_name":"AskUserQuestion","tool_use_id":"q1","tool_input":{"questions":[]}"#;
    let mut lines = vec![
        event(MADE_UP, "SessionStart", 0, r#""source":"startup""#),
        event(
            MADE_UP,
            "UserPromptSubmit",
            MIN,
            r#""prompt":"/implement issue 1""#,
        ),
    ];
    lines.extend(bash(MADE_UP, "b1", 3 * MIN, "cargo test"));
    lines.push(event(MADE_UP, "Stop", 11 * MIN, ""));
    // Two hours later Daniel prompts again; the agent asks him at once and he answers in 5 minutes.
    lines.push(event(
        MADE_UP,
        "UserPromptSubmit",
        131 * MIN,
        r#""prompt":"go on""#,
    ));
    lines.push(event(MADE_UP, "PreToolUse", 132 * MIN, ask));
    let studio = Studio::replayed(&lines);
    let work = &studio.session(MADE_UP).unwrap().work;
    assert_eq!(work.agent_ms, 11 * MIN);
    assert!(work.estimate.depends_on_you);

    lines.push(event(MADE_UP, "PostToolUse", 137 * MIN, ask));
    lines.push(event(MADE_UP, "Stop", 139 * MIN, ""));
    let studio = Studio::replayed(&lines);
    let work = &studio.session(MADE_UP).unwrap().work;
    assert_eq!(work.agent_ms, 13 * MIN);
    assert!(!work.estimate.depends_on_you);
    // Its tests started implementing two minutes into the agent's work.
    assert_eq!(work.phases[1].phase, Phase::Implementing);
    assert_eq!(work.phases[1].agent_ms, 2 * MIN);
}
