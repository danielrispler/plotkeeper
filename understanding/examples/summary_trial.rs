//! The turn summary on real turns (#526): the last `--turns` turns that get a summary (#517,
//! decision 12) from Daniel's most recent Falinks transcripts in `~/.claude/projects/`, each run
//! through the real summary call at the settings' defaults (Haiku at its default effort), as the
//! app runs it. It prints Markdown on standard output: each turn's prompt, summary, learning
//! section, seconds and `total_cost_usd`. It reads transcripts and no credential file. Run it
//! outside auto mode, from `apps/studio`:
//!
//! `cargo run -q -p studio-understanding --example summary_trial -- --turns 3`

use std::{
    io::Write,
    path::PathBuf,
    process::Stdio,
    time::{Instant, SystemTime},
};
use studio_core::{SummaryRequest, wants_summary};
use studio_understanding::{ClaudeWriter, SummarySetup, TurnSummary};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let turns: usize = match args.iter().position(|a| a == "--turns") {
        Some(at) => args
            .get(at + 1)
            .and_then(|n| n.parse().ok())
            .filter(|&n| n > 0)
            .expect("--turns takes a positive number"),
        None => 3,
    };
    let home = std::env::home_dir().expect("a home directory");
    let writer = ClaudeWriter::beside(&studio_hook::event_log(&home))
        .expect("create the card writer's folder");
    let setup = SummarySetup::default();
    println!("# Turn summaries on {turns} real turns\n");
    println!("Model `{}` at its default effort.\n", setup.model);
    for (file, request) in latest(&home.join(".claude/projects"), turns) {
        println!("## {}\n", file.display());
        println!("**Prompt:** {}\n", first_line(&request.prompt));
        let (mut command, input) = writer.summary_command(&setup, &request, &[]);
        command.args(["--output-format", "json"]);
        let start = Instant::now();
        let output = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(input.as_bytes())?;
                child.wait_with_output()
            });
        let seconds = start.elapsed().as_secs_f64();
        let reply: serde_json::Value = output
            .ok()
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or_default();
        let cost = &reply["total_cost_usd"];
        println!("**Time:** {seconds:.1} s · **total_cost_usd:** {cost}\n");
        match TurnSummary::from_reply(reply["result"].as_str().unwrap_or_default(), false) {
            Ok((summary, problem)) => print(&summary, problem),
            Err(line) => println!("**No summary:** {line}\n"),
        }
    }
}

fn print(summary: &TurnSummary, problem: Option<String>) {
    println!("### Summary\n\n{}\n", summary.summary);
    let Some(learning) = &summary.learning else {
        println!("**No learning section:** {}\n", problem.unwrap_or_default());
        return;
    };
    println!(
        "### Learning\n\n{}\n\n**{}**\n",
        learning.idea, learning.question.text
    );
    for (i, option) in learning.question.options.iter().enumerate() {
        let key = if i == learning.question.key {
            " (key)"
        } else {
            ""
        };
        println!("- {}{key}: {}", option.text, option.why);
    }
    println!();
}

fn first_line(text: &str) -> &str {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
}

/// The last `n` turns that get a summary, newest transcript first, from the Falinks project
/// folders under `projects`, oldest first.
fn latest(projects: &std::path::Path, n: usize) -> Vec<(PathBuf, SummaryRequest)> {
    let folders = std::fs::read_dir(projects).expect("read ~/.claude/projects");
    let mut files: Vec<(SystemTime, PathBuf)> = folders
        .flatten()
        .filter(|f| f.file_name().to_string_lossy().contains("Falinks"))
        .flat_map(|f| std::fs::read_dir(f.path()).into_iter().flatten().flatten())
        .map(|f| f.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    files.sort();
    let mut found = Vec::new();
    for (_, file) in files.into_iter().rev() {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let mut long: Vec<_> = turns(&text)
            .into_iter()
            .filter(|t| t.responses.last().is_some_and(|r| wants_summary(r, false)))
            .collect();
        while let Some(turn) = long.pop() {
            found.push((file.clone(), turn));
            if found.len() == n {
                found.reverse();
                return found;
            }
        }
    }
    found.reverse();
    found
}

/// A transcript's turns: each prompt of Daniel's, with the agent's texts after it.
fn turns(text: &str) -> Vec<SummaryRequest> {
    let mut turns: Vec<SummaryRequest> = Vec::new();
    for line in text.lines() {
        let Ok(line) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if line["isSidechain"] == true || line["isMeta"] == true {
            continue;
        }
        let content = &line["message"]["content"];
        match (line["type"].as_str(), content) {
            (Some("user"), serde_json::Value::String(prompt)) => turns.push(SummaryRequest {
                prompt: prompt.clone(),
                ..SummaryRequest::default()
            }),
            (Some("assistant"), serde_json::Value::Array(blocks)) => {
                let texts = blocks.iter().filter(|b| b["type"] == "text");
                let texts = texts.filter_map(|b| b["text"].as_str()).map(str::trim);
                if let Some(turn) = turns.last_mut() {
                    turn.responses
                        .extend(texts.filter(|t| !t.is_empty()).map(Into::into));
                }
            }
            _ => {}
        }
    }
    for turn in &mut turns {
        let last = turn.responses.len().saturating_sub(1);
        let lines = turn.responses.iter().enumerate().filter_map(|(i, r)| {
            let mut lines = r.lines().map(str::trim).filter(|l| !l.is_empty());
            if i == last {
                lines.next()
            } else {
                lines.next_back()
            }
        });
        turn.intentions = lines.map(Into::into).collect();
    }
    turns
}
