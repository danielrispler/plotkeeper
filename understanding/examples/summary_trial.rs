//! The turn summary on real turns (danielrispler/Falinks#526; the grounded writer's trial, danielrispler/plotkeeper#115): the last `--turns` turns that get a summary (danielrispler/Falinks#517,
//! decision 12) from Daniel's most recent Falinks transcripts in `~/.claude/projects/`, each run
//! through `write_summary` at the settings' defaults (Haiku at its default effort), as the app runs
//! it: with its retry, and a throwaway store in a temporary folder. It prints Markdown on standard
//! output: each turn's prompt, summary, Quiz, any logged problem lines, calls, seconds
//! and `total_cost_usd`. It reads transcripts and no credential file. Run it outside auto mode,
//! from the repository's root:
//!
//! `cargo run -q -p studio-understanding --example summary_trial -- --turns 3`
//!
//! It ends with the counts of the grounded writer's trial: the share of turns whose first reply was
//! rejected (asked once more), of Headlines that narrate instead of stating an outcome (by the
//! words they open with, to be read by eye), and of lines the guards stripped or prefixed. A
//! transcript holds no hook facts, so every turn is written with none: a claim of a fix always gets
//! "Agent says:" here.
//!
//! It also counts the Quiz trial of danielrispler/plotkeeper#118: the share of replies rejected, of
//! the Quizzes whose key is the longest option before the guard (the first reply of each turn),
//! and of stems that the Headline answers, which one more Haiku call per Quiz judges (a judge, so
//! read a few by eye). Only the summary's prompt runs: a transcript holds no diff for a card's.

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
    /// The raw replies of the turn being written, oldest first.
    replies: Mutex<Vec<String>>,
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
        let result: String = reply["result"].as_str().unwrap_or_default().into();
        self.replies.lock().unwrap().push(result.clone());
        Ok(result)
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
        replies: Mutex::default(),
    };
    let mut counts = Counts::default();
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
        writer.replies.lock().unwrap().clear();
        let mut lines = Vec::new();
        let start = Instant::now();
        let summary = write_summary(&store, &writer, &setup, request, |l| lines.push(l));
        let seconds = start.elapsed().as_secs_f64();
        let (calls, cost) = *writer.calls.lock().unwrap();
        println!("**Calls:** {calls} · **Time:** {seconds:.1} s · **total_cost_usd:** {cost}\n");
        for line in &lines {
            println!("**Logged:** {line}\n");
        }
        counts.turns += 1;
        counts.rejected += usize::from(calls > 1);
        counts.replies += calls as usize;
        let replies = writer.replies.lock().unwrap();
        counts.longest += replies.first().is_some_and(|r| key_is_longest(r)) as usize;
        counts.first_quizzes += replies.first().is_some_and(|r| has_quiz(r)) as usize;
        // A turn whose last reply was kept without a Quiz after a problem rejected every reply.
        let unusable = summary.as_ref().is_none_or(|s| s.quiz.is_none())
            && lines.iter().any(|l| l.contains("no quiz on"));
        counts.rejected_replies += (calls as usize).saturating_sub(usize::from(!unusable));
        counts.no_quiz_after_retry += usize::from(unusable);
        if let Some(summary) = summary {
            counts.add(&summary, replies.last().map_or("", String::as_str));
            print(&summary);
            if let Some(quiz) = &summary.quiz {
                drop(replies);
                let answered = answers(
                    &writer,
                    summary.headline.as_deref().unwrap_or_default(),
                    quiz,
                );
                println!("**The Headline answers the stem:** {answered}\n");
                counts.judged += 1;
                counts.answered += usize::from(answered);
            }
        }
    }
    counts.print();
}

/// The trial's counts.
#[derive(Default)]
struct Counts {
    /// Calls made, and how many of them were replies the guards rejected.
    replies: usize,
    rejected_replies: usize,
    /// Turns whose every reply was rejected, so the turn has no Quiz.
    no_quiz_after_retry: usize,
    /// First replies that held a Quiz, and of them the ones whose key is the longest option.
    first_quizzes: usize,
    longest: usize,
    /// Kept Quizzes judged, and the ones whose stem the Headline answers.
    judged: usize,
    answered: usize,
    turns: usize,
    rejected: usize,
    narrating: usize,
    prefixed: usize,
    lines: usize,
    stripped: usize,
}

/// Words a narrating Headline opens with: it says what the agent did on the way, not an outcome.
const NARRATING: [&str; 12] = [
    "looked",
    "looking",
    "read",
    "reading",
    "started",
    "reviewed",
    "explored",
    "investigated",
    "examined",
    "checked",
    "went",
    "began",
];

impl Counts {
    fn add(&mut self, summary: &TurnSummary, last_reply: &str) {
        let headline = summary.headline.as_deref().unwrap_or_default();
        let first = headline
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        self.narrating += usize::from(NARRATING.contains(&first.as_str()));
        self.prefixed += usize::from(headline.starts_with("Agent says:"));
        let value: serde_json::Value = serde_json::from_str(
            last_reply
                .get(last_reply.find('{').unwrap_or(0)..)
                .unwrap_or_default(),
        )
        .unwrap_or_default();
        let given = value["lines"].as_array().map_or(0, Vec::len);
        self.lines += given;
        self.stripped += given.saturating_sub(summary.lines.len());
    }

    fn print(&self) {
        let share = |n: usize, of: usize| {
            format!("{n} of {of} ({:.0}%)", 100.0 * n as f64 / of.max(1) as f64)
        };
        println!("## Counts\n");
        println!(
            "- Turns with a rejected first reply: {}",
            share(self.rejected, self.turns)
        );
        println!(
            "- Headlines that narrate: {}",
            share(self.narrating, self.turns)
        );
        println!(
            "- Headlines prefixed \"Agent says:\": {}",
            share(self.prefixed, self.turns)
        );
        println!(
            "- Replies rejected (by any guard): {}",
            share(self.rejected_replies, self.replies)
        );
        println!(
            "- Turns left with no Quiz after the retry: {}",
            share(self.no_quiz_after_retry, self.turns)
        );
        println!(
            "- First replies whose key is the longest option, before the guard: {}",
            share(self.longest, self.first_quizzes)
        );
        println!(
            "- Kept Quizzes whose stem the Headline answers (judged): {}",
            share(self.answered, self.judged)
        );
        println!(
            "- Lines dropped by the guards or for an unknown type: {}",
            share(self.stripped, self.lines)
        );
    }
}

fn print(summary: &TurnSummary) {
    println!("### Summary\n\n{}\n", summary.summary);
    if summary.headline.as_deref() == Some("") {
        println!("**Headline stripped by the guards.**\n");
    }
    let Some(quiz) = &summary.quiz else {
        println!("**No Quiz.**\n");
        return;
    };
    println!("### Quiz\n\n**{}**\n", quiz.text);
    for (i, option) in quiz.options.iter().enumerate() {
        let key = if i == quiz.key { " (key)" } else { "" };
        println!("- {}{key}: {}", option.text, option.why);
    }
    println!();
}

/// The Quiz in a raw reply, as its JSON.
fn quiz_in(reply: &str) -> serde_json::Value {
    let json = reply
        .get(reply.find('{').unwrap_or(0)..)
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    value["quiz"].clone()
}

fn has_quiz(reply: &str) -> bool {
    quiz_in(reply)["options"].is_array()
}

/// Whether a raw reply's key is its longest option, which the guard rejects.
fn key_is_longest(reply: &str) -> bool {
    let quiz = quiz_in(reply);
    let lengths: Vec<usize> = (quiz["options"].as_array().into_iter().flatten())
        .map(|o| o["text"].as_str().unwrap_or_default().chars().count())
        .collect();
    let Some(key) = quiz["key"].as_u64().and_then(|k| lengths.get(k as usize)) else {
        return false;
    };
    let wrong = (lengths.iter().enumerate())
        .filter(|(i, _)| *i as u64 != quiz["key"].as_u64().unwrap_or(0));
    wrong.map(|(_, l)| *l).max().is_some_and(|w| *key > w)
}

/// One Haiku call: whether `quiz`'s stem can be answered from `headline` alone.
fn answers(writer: &Costed, headline: &str, quiz: &studio_understanding::Quiz) -> bool {
    let options: String = quiz
        .options
        .iter()
        .map(|o| format!("- {}\n", o.text))
        .collect();
    let (mut command, input) = writer.writer.run_with(
        &["--model", "haiku", "--system-prompt", "You judge a quiz. Say whether a developer who has read only the headline could pick the right option of the question, without knowing the code. Reply with one word: yes or no."],
        &format!("Headline: {headline}\n\nQuestion: {}\nOptions:\n{options}\nThe right option: {}", quiz.text, quiz.options[quiz.key].text),
    );
    command.args(["--output-format", "json"]);
    let output = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(input.as_bytes())?;
            child.wait_with_output()
        });
    let reply: serde_json::Value = output
        .ok()
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or_default();
    reply["result"]
        .as_str()
        .is_some_and(|r| r.trim().to_lowercase().starts_with("yes"))
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
        .filter(|f| {
            let name = f.file_name().to_string_lossy().to_lowercase();
            // The card writer's own folder holds the writer's calls, not turns.
            (name.contains("falinks") || name.contains("plotkeeper"))
                && !name.contains("card-writer")
        })
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
