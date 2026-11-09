//! The writer's timing (#471's harness, kept for #518): `--runs` timed `write_card` calls with the
//! real `ClaudeWriter`, as the app runs it in its neutral folder, on `real_card.rs`'s structural
//! change with an empty brief. It prints JSON on standard output: each run's seconds and whether it
//! returned a card, then the median and the slowest. The writer's log lines go to standard error.
//! It reads no credential file. Run it outside auto mode, from `apps/studio`:
//!
//! `cargo run -q -p studio-understanding --example writer_timing -- --runs 5`

use std::time::Instant;
use studio_core::{Base, CardRequest};
use studio_understanding::{ClaudeWriter, write_card};

/// `real_card.rs`'s structural change: the brief's rewrite moves out of `lib.rs` into a module.
const STRUCTURAL: &str = r#"--- a/apps/studio/understanding/src/lib.rs
+++ b/apps/studio/understanding/src/lib.rs
@@ -8,8 +8,10 @@
+mod brief;
 mod drift;
 mod fingerprint;
 mod store;
 mod week;
+pub use brief::{REWRITE_EVERY, rewrite_input};
 pub use drift::{Drift, Versions, drift};
@@ -120,22 +122,6 @@
-/// The input of a brief's rewrite: Daniel's prompts, the card summaries and the diffstat.
-pub fn rewrite_input(prompts: &[&str], summaries: &[String], diffstat: &str) -> String {
-    let prompts: String = prompts.iter().map(|p| format!("<prompt>\n{p}\n</prompt>\n")).collect();
-    let summaries: String = summaries.iter().map(|s| format!("- {s}\n")).collect();
-    format!("{prompts}\n{summaries}\n{diffstat}")
-}
-
-/// Every 5th update of a brief rewrites it from scratch.
-pub const REWRITE_EVERY: u32 = 5;
--- /dev/null
+++ b/apps/studio/understanding/src/brief.rs
@@ -0,0 +1,14 @@
+//! A session's rolling brief (#369, decision 16): when it is rewritten from scratch, and from what.
+
+/// Every 5th update of a brief rewrites it from scratch.
+pub const REWRITE_EVERY: u32 = 5;
+
+/// The input of a brief's rewrite: Daniel's prompts, the card summaries and the diffstat, never the
+/// raw diff.
+pub fn rewrite_input(prompts: &[&str], summaries: &[String], diffstat: &str) -> String {
+    let prompts: String = prompts.iter().map(|p| format!("<prompt>\n{p}\n</prompt>\n")).collect();
+    let summaries: String = summaries.iter().map(|s| format!("- {s}\n")).collect();
+    format!("{prompts}\n{summaries}\n{diffstat}")
+}
"#;

/// `real_card.rs`'s request for its first prompt.
fn request() -> CardRequest {
    CardRequest {
        session_id: "real-card".into(),
        id: "real-card@1".into(),
        base: Base::default(),
        change: Some(STRUCTURAL.into()),
        prompt: Some(1),
        ..CardRequest::default()
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let runs: usize = match args.iter().position(|a| a == "--runs") {
        Some(at) => args
            .get(at + 1)
            .and_then(|n| n.parse().ok())
            .filter(|&n| n > 0)
            .expect("--runs takes a positive number"),
        None => 5,
    };
    let home = std::env::home_dir().expect("a home directory");
    let log = studio_hook::event_log(&home);
    let writer = ClaudeWriter::beside(&log).expect("create the card writer's folder");
    let request = request();
    let mut times = Vec::new();
    let mut each = Vec::new();
    for run in 1..=runs {
        let start = Instant::now();
        let card = write_card(&writer, &request, &[], "", |line| eprintln!("{line}")).is_some();
        let seconds = start.elapsed().as_secs_f64();
        eprintln!("run {run}: {seconds:.1} s, card: {card}");
        times.push(seconds);
        each.push(serde_json::json!({ "seconds": seconds, "card": card }));
    }
    times.sort_by(f64::total_cmp);
    // With an even count this is the upper middle run, never an average.
    let median = times[times.len() / 2];
    let slowest = times[times.len() - 1];
    let report = serde_json::json!({ "runs": each, "median": median, "slowest": slowest });
    println!("{report:#}");
}
