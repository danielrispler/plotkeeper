//! "Design so far" on stored sessions (danielrispler/plotkeeper#123): the `--sessions` sessions (30 by default)
//! with the most sources in the studio's own card store, whose turn summaries hold `why` lines and
//! whose cards hold summaries, each written once by `claude -p` as the app writes it. It writes
//! nothing to the store. It prints Markdown on standard output: each session's lines with their
//! citations, and the counts: lines the writer gave, lines dropped for a missing or wrong
//! citation, and lines that state a decision with a reason (by a reason word in the line, to be
//! read by eye). It reads the card store and no credential file. Run it outside auto mode, from
//! the repository's root:
//!
//! `cargo run -q -p studio-understanding --example design_trial -- --sessions 30`

use std::collections::{BTreeMap, BTreeSet};
use studio_understanding::{CardStore, CardWriter, ClaudeWriter, design_lines, design_sources};

/// Words that give a reason.
const REASONS: [&str; 7] = [
    "because",
    "so that",
    "since",
    "to avoid",
    "otherwise",
    "instead of",
    "rather than",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sessions: usize = match args.iter().position(|a| a == "--sessions") {
        Some(at) => args
            .get(at + 1)
            .and_then(|n| n.parse().ok())
            .filter(|&n| n > 0)
            .expect("--sessions takes a positive number"),
        None => 30,
    };
    let home = std::env::home_dir().expect("a home directory");
    let log = studio_hook::event_log(&home);
    let store = CardStore::open_beside(&log).expect("read the card store");
    let writer = ClaudeWriter::beside(&log).expect("create the card writer's folder");
    // Each session's turn ids, from its stored summaries.
    let mut turns: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for kept in store.summaries() {
        turns
            .entry(kept.request.session_id.clone())
            .or_default()
            .push(kept.request.id.clone());
    }
    let mut ranked: Vec<_> = turns
        .into_iter()
        .map(|(id, turns)| {
            let sources = design_sources(&store, &id, &turns);
            (sources, id, turns)
        })
        .filter(|(sources, ..)| !sources.is_empty())
        .collect();
    ranked.sort_by_key(|(sources, ..)| std::cmp::Reverse(sources.len()));
    println!("# \"Design so far\" of {sessions} stored sessions\n");
    let (mut asked, mut given, mut dropped, mut reasoned) = (0, 0, 0, 0);
    for (sources, id, turns) in ranked.into_iter().take(sessions) {
        asked += 1;
        println!("## {id} ({} sources)\n", sources.len());
        let mut known: BTreeSet<&str> = turns.iter().map(String::as_str).collect();
        known.extend(
            store
                .cards()
                .iter()
                .filter(|k| k.request.owner() == id)
                .map(|k| k.request.id.as_str()),
        );
        let reply = writer.design(&sources);
        let lines = reply.and_then(|r| design_lines(&r, &known).ok_or(format!("none in {r:?}")));
        match lines {
            Ok((kept, lost)) => {
                // The lines the writer gave: those kept and those dropped.
                given += kept.len() + lost;
                dropped += lost;
                for line in &kept {
                    let said = line.text.to_lowercase();
                    reasoned += usize::from(REASONS.iter().any(|w| said.contains(w)));
                    println!("- {} [{}]", line.text, line.cite);
                }
                println!("\n{lost} dropped\n");
            }
            Err(line) => println!("**No design:** {line}\n"),
        }
    }
    println!("## Counts\n");
    println!("- Sessions asked: {asked}");
    println!("- Lines the writer gave: {given}");
    println!("- Dropped for a missing or wrong citation, or past the fifth: {dropped}");
    println!("- Kept lines that state a reason: {reasoned}");
}
