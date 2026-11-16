//! A card's code by path and range on real cards (danielrispler/plotkeeper#121): the `--cards` most recent stored cards
//! with a diff (30 by default), each written again by `claude -p` from its stored input, as the app writes a card. The
//! turn's end tree is not stored, so a named range counts as existing when its path is one the diff changed and its
//! lines lie inside the new side of that file's hunks. It prints Markdown on standard output: the ranges each card
//! named, and the share of named ranges that exist and of cards left with no code. It reads the card store and no
//! credential file. Run it outside auto mode, from the repository's root:
//!
//! `cargo run -q -p studio-understanding --example code_trial -- --cards 30`

use studio_understanding::{Card, CardStore, CardWriter, ClaudeWriter};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let wanted: usize = match args.iter().position(|a| a == "--cards") {
        Some(at) => args
            .get(at + 1)
            .and_then(|n| n.parse().ok())
            .filter(|&n| n > 0)
            .expect("--cards takes a positive number"),
        None => 30,
    };
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    let store = CardStore::open_beside(&log).expect("read the card store");
    let writer = ClaudeWriter::beside(&log).expect("create the card writer's folder");
    println!("# {wanted} real cards, written again with ranges\n");
    let (mut asked, mut written, mut named, mut exist, mut bare) = (0, 0, 0, 0, 0);
    let stored = store.cards().iter().rev();
    for kept in stored.filter(|k| k.request.check.is_none() && k.request.diff().contains("+++ b/"))
    {
        if asked == wanted {
            break;
        }
        asked += 1;
        let request = &kept.request;
        let card = writer
            .reply(request, &[])
            .and_then(|r| Card::from_reply(&r));
        let Ok((card, _)) = card else {
            println!("- {}: no card\n", request.id);
            continue;
        };
        written += 1;
        let real: Vec<bool> = card
            .ranges
            .iter()
            .map(|r| exists(request.diff(), r))
            .collect();
        named += real.len();
        exist += real.iter().filter(|&&e| e).count();
        bare += usize::from(!real.contains(&true));
        let ranges: Vec<String> = (card.ranges.iter().zip(&real))
            .map(|(r, e)| {
                format!(
                    "`{}:{}-{}` {}",
                    r.path,
                    r.from,
                    r.to,
                    if *e { "exists" } else { "missing" }
                )
            })
            .collect();
        println!("- {}: {}", request.id, ranges.join(", "));
    }
    let share =
        |n: usize, of: usize| format!("{n} of {of} ({:.0}%)", 100.0 * n as f64 / of.max(1) as f64);
    println!("\n## Counts\n");
    println!("- Cards written: {written} of {asked} asked");
    println!("- Named ranges that exist: {}", share(exist, named));
    println!("- Cards left with no code: {}", share(bare, written));
}

/// Whether `range`'s path is in `diff` and its lines lie inside the new side of that file's hunks.
fn exists(diff: &str, range: &studio_understanding::CodeRange) -> bool {
    let (mut in_file, mut lo, mut hi) = (false, usize::MAX, 0);
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            in_file = path == range.path;
        } else if in_file && let Some(head) = line.strip_prefix("@@ ") {
            let new = head.split(' ').find_map(|part| part.strip_prefix('+'));
            let mut parts = new.unwrap_or_default().split(',');
            let start: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            let len: usize = parts.next().and_then(|n| n.parse().ok()).unwrap_or(1);
            if start > 0 {
                (lo, hi) = (lo.min(start), hi.max(start + len.saturating_sub(1)));
            }
        }
    }
    range.from >= 1 && range.to >= range.from && range.from >= lo && range.to <= hi
}
