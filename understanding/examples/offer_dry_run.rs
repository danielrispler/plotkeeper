//! The table of danielrispler/plotkeeper#119 run over the stored turns, with no model call: replays the real
//! event log, and for each turn whose two trees `git` can still read, takes its facts and counts what
//! it would get, a card or nothing, beside today's count (every turn whose trees differ
//! gets a card). It reads the log and git trees, writes no ref, and prints counts only. Run it with
//! `cargo run --release -p studio-understanding --example offer_dry_run`.

use std::io::{BufRead, BufReader};
use studio_core::Studio;
use studio_understanding::{
    Offer, OfferKind, TurnFacts, add_hunks, decide, turn_diffs, worktree_root,
};

fn main() {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    let file = std::fs::File::open(&log).expect("open the event log");
    let mut studio = Studio::with_roots(worktree_root);
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        studio.replay(&line);
    }
    let mut ids: Vec<(u64, String)> = Vec::new();
    for session in studio.sessions() {
        for turn in &session.turns {
            if let (Some(id), Some(ended)) = (&turn.id, turn.ended_at) {
                ids.push((ended, id.clone()));
            }
        }
    }
    ids.sort();
    let (mut turns, mut readable) = (ids.len(), 0);
    let mut with_diff = 0;
    let mut counts = [0usize; 2];
    for id in ids.iter().map(|(_, id)| id) {
        let Some(mut ask) = studio.summary_ask(id) else {
            continue;
        };
        // Never anchor a ref from a dry run.
        ask.ended = false;
        let Some(diffs) = turn_diffs(&ask, &mut drop) else {
            continue;
        };
        let mut facts = studio.summary_facts(&ask, Some(diffs));
        add_hunks(&ask, &mut facts);
        readable += 1;
        with_diff += usize::from(!facts.files.is_empty());
        let turn = TurnFacts {
            facts,
            merge: studio.turn_merged(id),
            workers: studio.turn_workers(id),
            traced: true,
        };
        let offer = decide(OfferKind::Turn, &turn, &[]).offer;
        counts[offer as usize] += 1;
    }
    println!("turns with an id and an end: {turns}");
    turns = readable;
    println!("turns whose trees are readable: {turns}");
    println!("turns whose trees differ: {with_diff}");
    println!("today: {with_diff} cards (every turn whose trees differ)");
    println!(
        "the table: {} cards, {} nothing",
        counts[Offer::Card as usize],
        counts[Offer::Nothing as usize]
    );
}
