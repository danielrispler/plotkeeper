//! The PR brief trial, on the product's own path: for each merged PR named, `fetch_meta` and `gather` fill a
//! `PrSource`, and `write_pr_brief` builds the facts, calls the real sandboxed writer (`ClaudeWriter`: no tools,
//! the neutral folder), checks the reply and stores the brief, so judging a brief by hand judges what Studio
//! writes. Run it outside auto mode (`claude -p` nested), from the repository's root:
//!
//! `cargo run -q -p studio-understanding --example brief_trial -- --out <dir> [--model opus] 274 276 317`
//!
//! Under `--out <dir>`, per PR: `<n>-facts.txt`, `<n>-budget.md`, `<n>-brief.json` (the stored brief, flags on
//! its items) and `<n>-checks.md`; the store and its records are under `<dir>/pr-briefs/`. With `BRIEF_DRY` set
//! it writes the facts and budget only and makes no call. Calls `gh` and `git`; reads no credential file.

use std::path::{Path, PathBuf};
use std::process::Command;
use studio_understanding::ClaudeWriter;
use studio_understanding::pr_brief::{
    BriefStore, DEFAULT_MODEL, PrMeta, build_facts, fetch_meta, gather, write_pr_brief,
};

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git ran");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let after = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .map(|i| args[i + 1].clone())
    };
    let out = PathBuf::from(after("--out").expect("--out <dir>"));
    let model = after("--model").unwrap_or_else(|| DEFAULT_MODEL.into());
    let prs: Vec<u64> = args.iter().filter_map(|a| a.parse().ok()).collect();
    std::fs::create_dir_all(&out).unwrap();
    let repo = std::env::current_dir().unwrap();
    let store = BriefStore::open(&out).expect("the brief store");
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let writer = ClaudeWriter::beside(&studio_hook::event_log(&home)).expect("writer folder");
    println!("# PR brief trial, on the product's entry ({model})\n");
    for number in prs {
        let mut meta: PrMeta = fetch_meta(&repo, number).expect("gh pr view");
        // A merged PR is read at its merge commit, against its first parent.
        let merge = git(
            &repo,
            &[
                "log",
                "--format=%H",
                "-1",
                "--grep",
                &format!("(#{number})$"),
                "origin/main",
            ],
        );
        if !merge.is_empty() {
            meta.head = merge.clone();
        }
        let base = format!("{}^", meta.head);
        let source = gather(&repo, &base, &meta.head, meta.clone()).expect("gather");
        let facts = build_facts(&source).expect("facts");
        std::fs::write(out.join(format!("{number}-facts.txt")), &facts.text).unwrap();
        let mut budget = format!(
            "# #{number} budget\n\nTotal: {} of {} bytes.\n\n| Section | Bytes |\n|---|---|\n",
            facts.budget.total_bytes,
            studio_understanding::pr_brief::BUDGET
        );
        for (name, bytes) in &facts.budget.sections {
            budget += &format!("| {name} | {bytes} |\n");
        }
        budget += &format!(
            "\nDiff {} bytes in all. Files sent {}, partial {}, cut {}.\n",
            facts.budget.diff_bytes_full,
            facts.budget.files_sent,
            facts.budget.files_partial,
            facts.budget.files_cut
        );
        std::fs::write(out.join(format!("{number}-budget.md")), budget).unwrap();
        println!(
            "## #{number} {}\n\n- shape: {}; facts {} bytes",
            meta.title,
            facts.shape.label(),
            facts.text.len()
        );
        if std::env::var_os("BRIEF_DRY").is_some() {
            println!("- facts written, no call (BRIEF_DRY)\n");
            continue;
        }
        match write_pr_brief(&writer, &model, &source, &store, |line| eprintln!("{line}")) {
            Ok(stored) => {
                std::fs::write(
                    out.join(format!("{number}-brief.json")),
                    serde_json::to_string_pretty(&stored).unwrap(),
                )
                .unwrap();
                let flags = stored.brief.all_flags();
                std::fs::write(
                    out.join(format!("{number}-checks.md")),
                    flags.iter().map(|f| format!("- {f}\n")).collect::<String>(),
                )
                .unwrap();
                let records = store.records();
                let call = &records.last().unwrap().calls[0];
                println!(
                    "- steps {}; flagged items {}; {} s; ${:.3}",
                    stored.brief.steps().count(),
                    stored.brief.flagged(),
                    call.seconds,
                    call.cost_usd
                );
                println!(
                    "- code checks: {}",
                    if flags.is_empty() {
                        "all passed".into()
                    } else {
                        format!("{} flags (see {number}-checks.md)", flags.len())
                    }
                );
            }
            Err(e) => println!("- **no brief**: {e}"),
        }
        println!();
    }
}
