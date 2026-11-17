//! The table of danielrispler/plotkeeper#119 run over the stored turns, with no model call: replays the real
//! event log, and for each turn whose two trees `git` can still read, takes its facts and counts what
//! it would get, a card, a lone Quiz or nothing, beside today's count (every turn whose trees differ
//! gets a card). It then plays the cap on Quizzes in turn order. It reads the log and git trees, writes
//! no ref, and prints counts only. Run it with
//! `cargo run --release -p studio-understanding --example offer_dry_run`.

use std::io::{BufRead, BufReader};
use studio_core::Studio;
use studio_understanding::{Offer, Shape, add_hunks, turn_diffs, worktree_root};

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
    let (mut external, mut with_diff) = (0, 0);
    let mut counts = [0usize; 3];
    let mut plan: Vec<(u64, Offer)> = Vec::new();
    for (ended, id) in &ids {
        let Some(mut ask) = studio.summary_ask(id) else {
            continue;
        };
        // Never anchor a ref from a dry run.
        ask.ended = false;
        let Some(diffs) = turn_diffs(&ask) else {
            continue;
        };
        let mut facts = studio.summary_facts(&ask, Some(diffs));
        add_hunks(&ask, &mut facts);
        readable += 1;
        external += facts.external;
        with_diff += usize::from(facts.files.len() + facts.external as usize > 0);
        let offer = Shape::of(&facts, "", studio.turn_merged(id)).offer();
        counts[offer as usize] += 1;
        plan.push((*ended, offer));
    }
    // The cap: a card's Quiz first, a lone Quiz while fewer than 3 were offered in the last hour.
    let hour = 60 * 60 * 1000;
    let mut offered: Vec<u64> = Vec::new();
    let (mut cards_kept, mut lone_kept) = (0, 0);
    for (at, offer) in &plan {
        offered.retain(|t| *t + hour > *at);
        match offer {
            Offer::Card if offered.len() < 3 => {
                offered.push(*at);
                cards_kept += 1;
            }
            Offer::Card => {}
            Offer::Quiz if offered.len() < 3 => {
                offered.push(*at);
                lone_kept += 1;
            }
            _ => {}
        }
    }
    println!("turns with an id and an end: {turns}");
    turns = readable;
    println!("turns whose trees are readable: {turns}");
    println!("turns whose trees differ: {with_diff}, other files in them: {external}");
    println!("today: {with_diff} cards (every turn whose trees differ)");
    println!(
        "the table: {} cards, {} lone Quizzes, {} nothing",
        counts[Offer::Card as usize],
        counts[Offer::Quiz as usize],
        counts[Offer::Nothing as usize]
    );
    println!("after the cap: {cards_kept} card Quizzes, {lone_kept} lone Quizzes offered");
}
