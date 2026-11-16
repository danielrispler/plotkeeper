//! Times the replay of the real event log, as a launch runs it (danielrispler/plotkeeper#132): reading
//! the lines, the core's replay with no root lookup, and with the shell's `git` lookup. It reads the log
//! and prints times and counts, nothing of the log's content. Run it with
//! `cargo run --release -p studio-understanding --example replay_time`.

use std::{
    io::{BufRead, BufReader},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
use studio_core::Studio;
use studio_understanding::worktree_root;

static LOOKUPS: AtomicUsize = AtomicUsize::new(0);

fn main() {
    let log = studio_hook::event_log(&std::env::home_dir().expect("a home directory"));
    let start = Instant::now();
    let file = std::fs::File::open(&log).expect("open the event log");
    let lines: Vec<String> = BufReader::new(file).lines().map_while(Result::ok).collect();
    let read = start.elapsed();

    let start = Instant::now();
    let mut studio = Studio::default();
    lines.iter().for_each(|line| studio.replay(line));
    let plain = start.elapsed();

    let start = Instant::now();
    let mut studio = Studio::with_roots(|folder| {
        LOOKUPS.fetch_add(1, Ordering::Relaxed);
        worktree_root(folder)
    });
    lines.iter().for_each(|line| studio.replay(line));
    let looked_up = start.elapsed();

    let lookups = LOOKUPS.swap(0, Ordering::Relaxed);

    let start = Instant::now();
    let roots = studio.known_roots();
    let mut studio = Studio::with_roots(|folder| {
        LOOKUPS.fetch_add(1, Ordering::Relaxed);
        worktree_root(folder)
    });
    studio.know_roots(roots);
    lines.iter().for_each(|line| studio.replay(line));
    let saved = start.elapsed();

    println!("{} lines, read in {read:?}", lines.len());
    println!("replay with no root lookup: {plain:?}");
    println!("replay with the git lookup: {looked_up:?} ({lookups} lookups)");
    println!(
        "replay with the roots a launch before saved: {saved:?} ({} lookups)",
        LOOKUPS.load(Ordering::Relaxed)
    );
}
