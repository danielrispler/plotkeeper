//! Ask on real anchors (danielrispler/plotkeeper#126): one suggested question on each of `--anchors` stored anchors (a
//! file, a failed check, a subagent, a Headline) taken from the newest sessions of the studio's event log, each answered
//! by the card writer's real call (`claude -p`) on the anchor's recorded facts and, for a file, its hunk, as the app asks
//! it. It prints Markdown on standard output: each anchor's facts, its answer and seconds, then the counts: the median and
//! slowest answer, and how many answers name a code span (`like this`) that is in no fact, to be read by eye. It reads the
//! event log and the writer's replies and no credential file. Run it outside auto mode, from the repository's root:
//!
//! `cargo run -q -p studio-understanding --example ask_trial -- --anchors 30`

use std::{io::BufRead, time::Instant};
use studio_core::{Anchor, Studio};
use studio_understanding::{ClaudeWriter, ask_about, ask_item, turn_hunk};

/// The first suggested question of each anchor's kind in the UI (`ui/ask.ts`, `questions`).
fn question(anchor: &Anchor) -> &'static str {
    match anchor {
        Anchor::File { .. } => "Why did it touch this file?",
        Anchor::Check { .. } => "Why did this check fail?",
        Anchor::Subagent { .. } => "What did this worker do?",
        Anchor::Headline { .. } => "What did this turn change?",
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "a hand-run trial's whole script, read top to bottom"
)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wanted: usize = args
        .iter()
        .position(|a| a == "--anchors")
        .and_then(|at| args.get(at + 1)?.parse().ok())
        .unwrap_or(30);
    let home = std::env::home_dir().expect("a home directory");
    let log = studio_hook::event_log(&home);
    let lines = std::io::BufReader::new(std::fs::File::open(&log).expect("the event log"))
        .lines()
        .map_while(Result::ok);
    let studio = Studio::replayed(lines);
    let writer = ClaudeWriter::beside(&log).expect("create the card writer's folder");

    // Anchors of the newest sessions, a kind at a time, so the 30 hold all four kinds.
    let mut sessions = studio.sessions();
    sessions.sort_by_key(|s| std::cmp::Reverse(s.agent.last_event_at));
    let mut by_kind: [Vec<(String, Anchor)>; 4] = Default::default();
    for session in sessions {
        let id = session.agent.id.clone();
        for (turn, _) in session.turns.iter().enumerate() {
            by_kind[3].push((id.clone(), Anchor::Headline { turn }));
            for row in session.ledger.turn(turn) {
                if row.failed() && row.command.is_some() {
                    let anchor = Anchor::Check {
                        agent: row.agent.clone(),
                        call: row.call.clone(),
                    };
                    by_kind[1].push((id.clone(), anchor));
                }
                let edit = ["Edit", "Write", "MultiEdit"].contains(&row.tool.as_str());
                if let (true, Some(file)) = (edit, &row.file)
                    && let Some(path) = file.strip_prefix(&format!("{}/", session.cwd))
                {
                    let anchor = Anchor::File {
                        turn,
                        root: session.cwd.clone(),
                        path: path.into(),
                        from: None,
                    };
                    if !by_kind[0].iter().any(|(s, a)| s == &id && a == &anchor) {
                        by_kind[0].push((id.clone(), anchor));
                    }
                }
            }
        }
        for worker in &session.agents {
            by_kind[2].push((
                id.clone(),
                Anchor::Subagent {
                    agent: worker.agent.id.clone(),
                },
            ));
        }
    }
    let mut picked = Vec::new();
    let mut round = 0;
    while picked.len() < wanted && by_kind.iter().any(|k| round < k.len()) {
        for kind in &by_kind {
            if let Some(item) = kind.get(round).filter(|_| picked.len() < wanted) {
                picked.push(item.clone());
            }
        }
        round += 1;
    }

    println!("# Ask on {} stored anchors\n", picked.len());
    let mut seconds = Vec::new();
    let (mut answered, mut outside) = (0, 0);
    for (n, (session, anchor)) in picked.iter().enumerate() {
        let Some(facts) = studio.ask_facts(session, anchor, &log) else {
            continue;
        };
        let hunk = match &anchor {
            Anchor::File {
                turn, root, path, ..
            } => studio
                .turn_trees(session, *turn, root)
                .and_then(|(start, end)| {
                    turn_hunk(
                        std::path::Path::new(root),
                        &start,
                        end.as_deref(),
                        path,
                        None,
                        false,
                    )
                })
                .filter(|h| !h.binary)
                .map(|h| h.text),
            _ => None,
        };
        let item = ask_item("this anchor", &facts, hunk.as_deref());
        let transcript = studio
            .session(session)
            .map(|s| s.transcript.clone())
            .unwrap_or_default();
        let start = Instant::now();
        let answer = ask_about(&writer, &transcript, &item, question(anchor));
        let took = start.elapsed().as_secs_f64();
        println!("## {}. {anchor:?}\n", n + 1);
        println!(
            "**Question:** {}\n\n**Facts sent:**\n\n```\n{}\n```\n",
            question(anchor),
            item
        );
        match answer {
            Ok(answer) => {
                answered += 1;
                seconds.push(took);
                let spans = answer
                    .split('`')
                    .skip(1)
                    .step_by(2)
                    .filter(|span| !item.contains(span))
                    .count();
                outside += usize::from(spans > 0);
                println!("**Answer ({took:.1} s, {spans} code spans in no fact):** {answer}\n");
            }
            Err(e) => println!("**No answer ({took:.1} s):** {e}\n"),
        }
    }
    seconds.sort_by(f64::total_cmp);
    println!("## Counts\n");
    println!("- Anchors asked: {}; answered: {answered}", picked.len());
    if let (Some(median), Some(slowest)) = (seconds.get(seconds.len() / 2), seconds.last()) {
        println!("- Seconds per answer: median {median:.1}, slowest {slowest:.1}");
    }
    println!(
        "- Answers naming a code span that is in no fact: {outside} of {answered} (read the answers by eye for the rest)"
    );
}
