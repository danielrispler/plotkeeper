//! The Brief's Goal on real sessions (danielrispler/plotkeeper#122): the Goal of each of the `--sessions`
//! most recent Falinks and plotkeeper transcripts in `~/.claude/projects/` (30 by default), written
//! once from Daniel's prompts in it by `claude -p`, as the app writes it. It prints Markdown on
//! standard output: each session's first prompt and its Goal, and the counts of the trial: Goals
//! that restate the first prompt (by shared words, to be read by eye) and Goals that name an
//! outcome (by not opening with a verb of working on, to be read by eye). It reads transcripts and no
//! credential file. Run it outside auto mode, from the repository's root:
//!
//! `cargo run -q -p studio-understanding --example goal_trial -- --sessions 30`

use std::{collections::BTreeSet, path::PathBuf, time::SystemTime};
use studio_understanding::{CardWriter, ClaudeWriter};

/// Words a Goal opens with when it names the work and no outcome.
const WORKING: [&str; 9] = [
    "work",
    "working",
    "continue",
    "continuing",
    "look",
    "looking",
    "explore",
    "investigate",
    "review",
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
    let writer = ClaudeWriter::beside(&studio_hook::event_log(&home))
        .expect("create the card writer's folder");
    println!("# The Goal of {sessions} real sessions\n");
    let (mut asked, mut written, mut restating, mut outcomes) = (0, 0, 0, 0);
    for file in latest(&home.join(".claude/projects"), sessions) {
        let prompts = prompts(&std::fs::read_to_string(&file).unwrap_or_default());
        let Some(first) = prompts.first() else {
            continue;
        };
        if asked == sessions {
            break;
        }
        asked += 1;
        let refs: Vec<&str> = prompts.iter().map(String::as_str).collect();
        println!("## {}\n", file.display());
        println!("**First prompt:** {}\n", short(first));
        let goal = writer.goal(&refs).and_then(|reply| {
            let start = reply.find('{').zip(reply.rfind('}'));
            let object = start.map_or("", |(s, e)| &reply[s..=e]);
            let value: serde_json::Value = serde_json::from_str(object).unwrap_or_default();
            value["goal"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("no goal in {reply:?}"))
        });
        match goal {
            Ok(goal) => {
                println!("**Goal ({} prompts):** {goal}\n", prompts.len());
                written += 1;
                restating += usize::from(restates(first, &goal));
                outcomes += usize::from(names_an_outcome(&goal));
            }
            Err(line) => println!("**No goal:** {line}\n"),
        }
    }
    let share = |n: usize| {
        format!(
            "{n} of {written} ({:.0}%)",
            100.0 * n as f64 / written.max(1) as f64
        )
    };
    println!("## Counts\n");
    println!("- Goals written: {written}");
    println!(
        "- Goals that restate the first prompt: {}",
        share(restating)
    );
    println!("- Goals that name an outcome: {}", share(outcomes));
}

/// Whether `goal` restates `first`: at least 60% of the goal's words of 4 letters or more are the
/// prompt's. A heuristic, to be read by eye.
fn restates(first: &str, goal: &str) -> bool {
    let words = |text: &str| -> BTreeSet<String> {
        let words = text.split(|c: char| !c.is_alphanumeric());
        words
            .filter(|w| w.chars().count() >= 4)
            .map(str::to_lowercase)
            .collect()
    };
    let (asked, given) = (words(first), words(goal));
    !given.is_empty() && given.intersection(&asked).count() * 10 >= given.len() * 6
}

/// Whether `goal` names an outcome: it does not open with a verb of working on something.
fn names_an_outcome(goal: &str) -> bool {
    let first = goal
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_lowercase();
    !WORKING.contains(&first.as_str())
}

fn short(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    line.chars().take(200).collect()
}

/// The `n` most recent transcripts of the Falinks and plotkeeper project folders under `projects`.
fn latest(projects: &std::path::Path, n: usize) -> Vec<PathBuf> {
    let folders = std::fs::read_dir(projects).expect("read ~/.claude/projects");
    let mut files: Vec<(SystemTime, PathBuf)> = folders
        .flatten()
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_lowercase();
            // The card writer's own folder holds the writer's calls, not sessions.
            (name.contains("falinks") || name.contains("plotkeeper"))
                && !name.contains("card-writer")
        })
        .flat_map(|f| std::fs::read_dir(f.path()).into_iter().flatten().flatten())
        .map(|f| f.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    files.sort();
    // More than `n`, as a transcript with no prompt of Daniel's is skipped.
    files
        .into_iter()
        .rev()
        .take(n * 2)
        .map(|(_, p)| p)
        .collect()
}

/// Daniel's prompts in a transcript, oldest first: its user lines of plain text that are not
/// markup (a wake-up, a command's output) and not a subagent's.
fn prompts(text: &str) -> Vec<String> {
    let lines = text
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok());
    let mut prompts = Vec::new();
    for line in lines {
        if line["isSidechain"] == true || line["isMeta"] == true || line["type"] != "user" {
            continue;
        }
        if let Some(prompt) = line["message"]["content"].as_str()
            && !prompt.trim_start().starts_with('<')
        {
            prompts.push(prompt.to_owned());
        }
    }
    prompts
}
