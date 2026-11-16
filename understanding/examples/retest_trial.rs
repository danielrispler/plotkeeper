//! The retest writer on real Quizzes (danielrispler/plotkeeper#129): the `--quizzes` most recent stored Quizzes (30 by
//! default) that have a hunk, each given one retest by `claude -p`, as the app writes it. It prints Markdown on standard
//! output: the counts of the trial and, for each Quiz, its stem and its retest's. Counts: replies the Quiz guards rejected
//! at the first ask and after the one retry, and retests that only reword the first Quiz (a stem that shares 60% of its
//! words of 4 letters or more with the first's, or the same right option; a heuristic, to be read by eye). It reads the
//! studio's card store and keeps nothing: no retest is written to it. It reads no credential file. Run it outside auto
//! mode, from the repository's root:
//!
//! `cargo run -q -p studio-understanding --example retest_trial -- --quizzes 30`

use std::collections::BTreeSet;
use studio_understanding::{CardStore, CardWriter, ClaudeWriter, Quiz, RetestAsk, retest_quiz};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let quizzes: usize = match args.iter().position(|a| a == "--quizzes") {
        Some(at) => args
            .get(at + 1)
            .and_then(|n| n.parse().ok())
            .filter(|&n| n > 0)
            .expect("--quizzes takes a positive number"),
        None => 30,
    };
    let home = std::env::home_dir().expect("a home directory");
    let log = studio_hook::event_log(&home);
    let dir = log.parent().expect("the log's folder").to_owned();
    let store = CardStore::open(dir.join("cards-v2.json")).expect("read the card store");
    // Every Quiz with a hunk, as if each had been offered a week ago; the newest `quizzes` of them.
    let mut asks: Vec<RetestAsk> = store.due_retests(u64::MAX);
    let at = asks.len().saturating_sub(quizzes);
    asks.drain(..at);
    let writer = ClaudeWriter::beside(&log).expect("create the card writer's folder");
    println!("# The retest of {} real Quizzes\n", asks.len());
    let (mut asked, mut first_rejected, mut rejected, mut written, mut rewords) = (0, 0, 0, 0, 0);
    let mut unanswered = 0;
    for ask in &asks {
        println!("## {}\n", ask.of);
        println!("**First:** {}\n", ask.first.text);
        asked += 1;
        let attempt = || writer.retest(ask).map(|reply| retest_quiz(&reply));
        let mut result = attempt();
        if matches!(result, Ok(Err(_))) {
            first_rejected += 1;
            result = attempt();
        }
        match result {
            Ok(Ok(quiz)) => {
                written += 1;
                let reword = rewords_of(&ask.first, &quiz);
                rewords += usize::from(reword);
                println!(
                    "**Retest{}:** {}\n",
                    if reword { " (rewords)" } else { "" },
                    quiz.text
                );
            }
            Ok(Err(line)) => {
                rejected += 1;
                println!("**Rejected twice:** {line}\n");
            }
            Err(line) => {
                unanswered += 1;
                println!("**No reply:** {line}\n");
            }
        }
    }
    let replied = asked - unanswered;
    let share =
        |n: usize, of: usize| format!("{n} of {of} ({:.0}%)", 100.0 * n as f64 / of.max(1) as f64);
    println!("## Counts\n");
    println!("- Quizzes asked about: {asked}, with no reply from the writer: {unanswered}");
    println!(
        "- Rejected by the guards at the first reply: {}",
        share(first_rejected, replied)
    );
    println!(
        "- Rejected by the guards after the retry (no retest): {}",
        share(rejected, replied)
    );
    println!("- Written: {written}");
    println!(
        "- Of those, only reword the first Quiz: {}",
        share(rewords, written)
    );
}

/// Whether `retest` only rewords `first`: its stem shares at least 60% of its words of 4 letters or more with the first's,
/// or its right option is the first's.
fn rewords_of(first: &Quiz, retest: &Quiz) -> bool {
    let words = |text: &str| -> BTreeSet<String> {
        let words = text.split(|c: char| !c.is_alphanumeric());
        words
            .filter(|w| w.chars().count() >= 4)
            .map(str::to_lowercase)
            .collect()
    };
    let (asked, given) = (words(&first.text), words(&retest.text));
    let stem = !given.is_empty() && given.intersection(&asked).count() * 10 >= given.len() * 6;
    let right = |quiz: &Quiz| quiz.options[quiz.key].text.trim().to_lowercase();
    stem || right(first) == right(retest)
}
