//! The turn summary on real turns (#526): the last `--turns` turns that get a summary (#517,
//! decision 12) from Daniel's most recent Falinks transcripts in `~/.claude/projects/`, each run
//! through `write_summary` at the settings' defaults (Haiku at its default effort), as the app runs
//! it: with its retry, and a throwaway store in a temporary folder. It prints Markdown on standard
//! output: each turn's prompt, summary, learning section, any logged problem lines, calls, seconds
//! and `total_cost_usd`. It reads transcripts and no credential file. Run it outside auto mode,
//! from `apps/studio`:
//!
//! `cargo run -q -p studio-understanding --example summary_trial -- --turns 3`

use std::{
    io::Write,
    path::PathBuf,
    process::Stdio,
    sync::Mutex,
    time::{Instant, SystemTime},
};
use studio_core::{CardRequest, SummaryRequest, wants_summary};
use studio_understanding::{
    Card, CardStore, CardWriter, ClaudeWriter, SummarySetup, TurnSummary, write_summary,
};

/// The real summary call, which also counts its calls and adds up their `total_cost_usd`.
struct Costed {
    writer: ClaudeWriter,
    calls: Mutex<(u32, f64)>,
}

impl CardWriter for Costed {
    fn reply(&self, _: &CardRequest, _: &[String], _: &str) -> Result<String, String> {
        Err("the trial writes summaries only".into())
    }

    fn more(&self, _: &CardRequest, _: &Card) -> Result<String, String> {
        Err("the trial writes summaries only".into())
    }

    fn rewrite(&self, _: &[&str], _: &[String], _: &str) -> Result<String, String> {
        Err("the trial writes summaries only".into())
    }

    fn update(&self, _: &str, _: &str) -> Result<String, String> {
        Err("the trial writes summaries only".into())
    }

    fn summarize(
        &self,
        setup: &SummarySetup,
        request: &SummaryRequest,
        earlier: &[TurnSummary],
    ) -> Result<String, String> {
        let (mut command, input) = self.writer.summary_command(setup, request, earlier);
        command.args(["--output-format", "json"]);
        let output = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                child.stdin.take().unwrap().write_all(input.as_bytes())?;
                child.wait_with_output()
            })
            .map_err(|e| format!("`claude -p` did not run: {e}"))?;
        let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_default();
        let mut calls = self.calls.lock().unwrap();
        calls.0 += 1;
        calls.1 += reply["total_cost_usd"].as_f64().unwrap_or_default();
        Ok(reply["result"].as_str().unwrap_or_default().into())
    }
}

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
    let writer = Costed {
        writer: ClaudeWriter::beside(&studio_hook::event_log(&home))
            .expect("create the card writer's folder"),
        calls: Mutex::new((0, 0.0)),
    };
    let folder = tempfile::tempdir().expect("create a temporary folder");
    let store = Mutex::new(CardStore::open(folder.path().join("cards.json")).expect("a store"));
    let setup = SummarySetup::default();
    println!("# Turn summaries on {turns} real turns\n");
    println!("Model `{}` at its default effort.\n", setup.model);
    for (n, (file, mut request)) in latest(&home.join(".claude/projects"), turns)
        .into_iter()
        .enumerate()
    {
        println!("## {}\n", file.display());
        println!("**Prompt:** {}\n", first_line(&request.prompt));
        // As in the app, a turn reads its session's earlier summaries.
        request.session_id = file.display().to_string();
        request.id = format!("turn {}", n + 1);
        *writer.calls.lock().unwrap() = (0, 0.0);
        let mut lines = Vec::new();
        let start = Instant::now();
        let summary = write_summary(&store, &writer, &setup, request, |l| lines.push(l));
        let seconds = start.elapsed().as_secs_f64();
        let (calls, cost) = *writer.calls.lock().unwrap();
        println!("**Calls:** {calls} · **Time:** {seconds:.1} s · **total_cost_usd:** {cost}\n");
        for line in lines {
            println!("**Logged:** {line}\n");
        }
        if let Some(summary) = summary {
            print(&summary);
        }
    }
}

fn print(summary: &TurnSummary) {
    println!("### Summary\n\n{}\n", summary.summary);
    let Some(learning) = &summary.learning else {
        println!("**No learning section.**\n");
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
