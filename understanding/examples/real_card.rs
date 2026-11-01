//! One real `claude -p` card on a fixture change (#375's check, #369's Testing Decisions): prints the
//! writer's raw reply, then the card parsed from it, then its "explain more" (#376). It runs the
//! writer as the app does, in the app's neutral folder, and reads no credential file. Run it outside
//! auto mode, from the repository root:
//!
//! `cargo run -q --manifest-path apps/studio/Cargo.toml -p studio-understanding --example real_card`

use studio_core::{Base, CardRequest};
use studio_understanding::{Card, CardWriter, ClaudeWriter, find_claude};

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

fn main() {
    let home = std::env::home_dir().expect("a home directory");
    let folder = studio_hook::event_log(&home).with_file_name("card-writer");
    std::fs::create_dir_all(&folder).expect("create the card writer's folder");
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let writer = ClaudeWriter::new(find_claude(shell.as_ref()), folder);
    let request = CardRequest {
        session_id: "real-card".into(),
        id: "real-card@0".into(),
        base: Base::default(),
        change: CHANGE.into(),
    };
    let reply = writer.reply(&request).expect("run `claude -p`");
    println!("--- The writer's reply\n{reply}");
    match Card::from_reply(&reply) {
        Ok((card, problem)) => {
            println!("--- The card\n{card:#?}");
            if let Some(problem) = problem {
                println!("--- No quiz: {problem}");
            }
            let more = writer.more(&request, &card).expect("run `claude -p`");
            println!("--- Explain more\n{more}");
        }
        Err(line) => println!("--- No card: {line}"),
    }
}
