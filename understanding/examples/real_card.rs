//! Real `claude -p` cards on two fixture changes, one per prompt of a session (#375's and #377's
//! checks, #369's Testing Decisions): a structural change, then one that is not. For each it prints
//! the writer's raw reply, then the card and the brief parsed from it; the second card updates the
//! first one's brief. Then it prints a rewrite of the brief from the two prompts, the two summaries
//! and a diffstat, and the second card's "explain more" (#376). It runs the writer as the app does, in
//! the app's neutral folder, and reads no credential file. Run it outside auto mode, from
//! `apps/studio`:
//!
//! `cargo run -q -p studio-understanding --example real_card`

use studio_core::{Base, CardRequest};
use studio_understanding::{Card, CardWriter, ClaudeWriter, find_claude, write_card};

/// A structural change: the brief's rewrite moves out of `lib.rs` into a module of its own.
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

/// A change on logic: a check's exit code counts through a pipe only when `pipefail` is on.
const CHANGE: &str = r#"--- a/apps/studio/core/src/lib.rs
+++ b/apps/studio/core/src/lib.rs
@@ -334,14 +334,22 @@
 /// Whether a check's exit code says whether it passed (#328, module 4). A command exits with its
 /// last statement's code, so the check must be the last statement, and nothing in it may mask the
-/// code: `|| true` exits 0 and `!` flips it.
+/// code: `|| true` exits 0, `!` flips it, and a pipe exits with its last command's code unless an
+/// earlier `set` turns on `pipefail`.
 fn exit_code_counts(command: &str) -> bool {
     let statements = command.split([';', '\n']).map(str::trim);
     let Some(last) = statements.clone().rfind(|s| !s.is_empty()) else {
         return false;
     };
-    is_check(last) && !last.contains("||") && !last.starts_with('!')
+    let pipefail = statements
+        .flat_map(|s| s.split("&&"))
+        .any(|s| s.trim().starts_with("set ") && s.contains("pipefail"));
+    is_check(last)
+        && !last.contains("||")
+        && !last.starts_with('!')
+        && (pipefail || !last.contains('|'))
 }
"#;

/// The two prompts of Daniel's that made the changes.
const PROMPTS: [&str; 2] = [
    "Move the brief's rewrite out of lib.rs into its own module",
    "A check behind a pipe should count only with pipefail",
];

const DIFFSTAT: &str = " apps/studio/core/src/lib.rs              | 12 ++++++++++--
 apps/studio/understanding/src/brief.rs   | 14 ++++++++++++++
 apps/studio/understanding/src/lib.rs     | 18 ++----------------
 3 files changed, 26 insertions(+), 18 deletions(-)
";

/// The real writer, printing each raw reply.
struct Printing(ClaudeWriter);

impl Printing {
    fn print(reply: Result<String, String>) -> Result<String, String> {
        match &reply {
            Ok(text) => println!("--- The writer's reply\n{text}"),
            Err(line) => println!("--- No reply: {line}"),
        }
        reply
    }
}

impl CardWriter for Printing {
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        Self::print(self.0.reply(request, earlier, brief))
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        Self::print(self.0.more(request, card))
    }

    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String> {
        Self::print(self.0.rewrite(prompts, summaries, diffstat))
    }
}

fn request(change: &str, at: u64) -> CardRequest {
    CardRequest {
        session_id: "real-card".into(),
        id: format!("real-card@{at}"),
        base: Base::default(),
        change: change.into(),
        prompt: Some(at),
    }
}

fn main() {
    let home = std::env::home_dir().expect("a home directory");
    let folder = studio_hook::event_log(&home).with_file_name("card-writer");
    std::fs::create_dir_all(&folder).expect("create the card writer's folder");
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let writer = Printing(ClaudeWriter::new(find_claude(shell.as_ref()), folder));
    let log = |line: String| println!("--- Log: {line}");
    let mut brief = String::new();
    let mut summaries = Vec::new();
    let mut last = None;
    for (at, change) in [(1, STRUCTURAL), (2, CHANGE)] {
        let request = request(change, at);
        println!("=== Prompt {at}: {}", PROMPTS[at as usize - 1]);
        let Some((card, updated)) = write_card(&writer, &request, &[], &brief, log) else {
            continue;
        };
        println!(
            "--- The card\n{card:#?}\n--- The brief\n{}",
            updated.as_deref().unwrap_or("(none)")
        );
        brief = updated.unwrap_or(brief);
        summaries.push(card.summary.clone());
        last = Some((request, card));
    }
    println!("=== The brief rewritten");
    let _ = writer.rewrite(&PROMPTS, &summaries, DIFFSTAT);
    if let Some((request, card)) = last {
        println!("=== Explain more on the second card");
        let _ = writer.more(&request, &card);
    }
}
