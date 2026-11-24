//! The writer bench (#450): one real part catch-up or PR brief write, with `claude -p` as the app runs it, on a recorded input,
//! into a folder of its own (never the data folder). It appends one JSON line to `<out>/results.jsonl` and prints it: each call's
//! time, cost and tokens (those read from the cache and written to it), the claim check's flags before the retry, whether the
//! retry ran, the figures, and the arguments the calls ran with (the writer's and the retry's, then the claim check's, which
//! runs at claude's own effort). `dry=1` builds the facts only, calls nothing and costs nothing.
//!
//!   cargo run --release -p studio-understanding --example writer_bench -- catch-up repo=<repo> part=<id> fog=<c1,c2,...> \
//!       out=<dir> label=<name> [effort=low] [model=opus] [dry=1]
//!   cargo run --release -p studio-understanding --example writer_bench -- brief repo=<repo> pr=<n> head=<sha> \
//!       out=<dir> label=<name> [effort=low] [model=opus] [base-ref=origin/main] [dry=1]
//!
//! A brief's text, checks and spec come from `gh` as they are now; its head is the recorded one.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use studio_understanding::ClaudeWriter;
use studio_understanding::part_catch_up::{
    CatchUpStore, build_catch_up_facts, gather_catch_up, write_part_catch_up,
};
use studio_understanding::pr_brief::{
    Brief, BriefRecord, BriefStore, CallKind, build_facts, fetch_meta, gather, write_pr_brief,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let kind = args.next().unwrap_or_default();
    let opts: HashMap<String, String> = args
        .filter_map(|a| {
            a.split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect();
    let need = |k: &str| {
        opts.get(k)
            .cloned()
            .unwrap_or_else(|| panic!("missing {k}=..."))
    };
    let repo = PathBuf::from(need("repo"));
    let out = PathBuf::from(need("out"));
    let label = need("label");
    let effort = opts.get("effort").cloned().unwrap_or_default();
    let model = opts.get("model").cloned().unwrap_or_else(|| "opus".into());
    let dry = opts.get("dry").is_some_and(|d| d == "1");
    let data = out.join(&label);
    std::fs::create_dir_all(&data).expect("the bench's folder");
    let writer = {
        let effort = effort.clone();
        ClaudeWriter::beside(&out.join("writer-log"))
            .expect("the writer's folder")
            .with_brief_effort(move || effort.clone())
    };
    let argv = |call| -> Vec<String> {
        writer
            .brief_command(&model, call, Some("FACTS"), "")
            .0
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    };
    let mut result = json!({ "label": label, "kind": kind, "effort": effort, "model": model,
        "argv": argv(CallKind::Write), "argv_check": argv(CallKind::Check) });

    let (record, brief): (Option<BriefRecord>, Option<Brief>) = match kind.as_str() {
        "catch-up" => {
            let part = need("part");
            let fog: Vec<String> = need("fog").split(',').map(String::from).collect();
            result["target"] = json!(format!("{part}@{}", &fog.last().unwrap()[..9]));
            result["fog_entries"] = json!(fog.len());
            let source = gather_catch_up(&repo, &part, &fog).expect("the catch-up's source");
            let facts = build_catch_up_facts(&source).expect("the catch-up's facts");
            result["facts_bytes"] = json!(facts.budget.total_bytes);
            if dry {
                (None, None)
            } else {
                let store = CatchUpStore::open(&data).expect("the bench's store");
                let started = std::time::Instant::now();
                let written = write_part_catch_up(
                    &writer,
                    &model,
                    &source,
                    &store,
                    |l| eprintln!("{l}"),
                    |stage| eprintln!("stage: {stage:?}"),
                );
                result["wall_ms"] = json!(started.elapsed().as_millis() as u64);
                match written {
                    Ok(stored) => (store.records().pop(), Some(stored.brief)),
                    Err(e) => {
                        result["error"] = json!(e);
                        (store.records().pop(), None)
                    }
                }
            }
        }
        "brief" => {
            let number: u64 = need("pr").parse().expect("pr=<number>");
            let head = need("head");
            result["target"] = json!(format!("#{number}@{}", &head[..9]));
            let mut meta = fetch_meta(&repo, number).expect("gh pr view");
            meta.head = head.clone();
            let base_ref = opts
                .get("base-ref")
                .cloned()
                .unwrap_or_else(|| "origin/main".into());
            let base = git(&repo, &["merge-base", &base_ref, &head]);
            let source = gather(&repo, &base, &head, meta).expect("the brief's source");
            let facts = build_facts(&source).expect("the brief's facts");
            result["facts_bytes"] = json!(facts.budget.total_bytes);
            result["shape"] = json!(facts.shape.label());
            if dry {
                (None, None)
            } else {
                let store = BriefStore::open(&data).expect("the bench's store");
                let started = std::time::Instant::now();
                let written =
                    write_pr_brief(&writer, &model, &source, &store, |l| eprintln!("{l}"));
                result["wall_ms"] = json!(started.elapsed().as_millis() as u64);
                match written {
                    Ok(stored) => (store.records().pop(), Some(stored.brief)),
                    Err(e) => {
                        result["error"] = json!(e);
                        (store.records().pop(), None)
                    }
                }
            }
        }
        _ => panic!("the first word is catch-up or brief"),
    };

    if let Some(r) = &record {
        let calls: Vec<Value> = r
            .calls
            .iter()
            .map(|c| {
                json!({ "role": c.role, "model": c.model, "duration_ms": c.duration_ms, "input_tokens": c.input_tokens,
                        "cache_read_tokens": c.cache_read_tokens, "cache_write_tokens": c.cache_write_tokens,
                        "output_tokens": c.output_tokens, "cost_usd": c.cost_usd })
            })
            .collect();
        result["calls"] = json!(calls);
        result["total_ms"] = json!(r.calls.iter().map(|c| c.duration_ms).sum::<u64>());
        result["cost_usd"] = json!(r.calls.iter().map(|c| c.cost_usd).sum::<f64>());
        result["cache_read_tokens"] =
            json!(r.calls.iter().map(|c| c.cache_read_tokens).sum::<u64>());
        result["code_flags"] = json!(r.flags.len());
        result["claim_flags_before"] = json!(r.claim_flags_before);
        result["retried"] = json!(r.calls.iter().any(|c| c.role == "retry"));
        result["unconfirmed"] = json!(r.unconfirmed);
        result["claim_error"] = json!(r.claim_error);
        result["retry_error"] = json!(r.retry_error);
    }
    if let Some(b) = &brief {
        result["steps"] = json!(b.steps().count());
        result["figures"] = json!(b.steps().filter(|s| s.fig.is_some()).count());
        std::fs::write(
            data.join("brief.json"),
            serde_json::to_vec_pretty(b).unwrap_or_default(),
        )
        .expect("keep the brief");
    }
    let line = result.to_string();
    println!("{line}");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(out.join("results.jsonl"))
        .expect("results.jsonl");
    writeln!(file, "{line}").expect("write results.jsonl");
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
