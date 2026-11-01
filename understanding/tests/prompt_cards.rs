//! Seams 1 and 2 together (#369, Testing Decisions): hook events through the core, the fingerprint
//! on a real repository and a fake writer, joined as the shell joins them.

use serde_json::json;
use std::{cell::Cell, collections::HashMap, fs, path::Path, process::Command};
use studio_core::{CardRequest, Output, Studio};
use studio_understanding::{Card, CardWriter, FakeWriter, Fingerprints, write_card};

/// The fake writer, counting its calls.
#[derive(Default)]
struct Counting(Cell<usize>);

impl CardWriter for Counting {
    fn reply(&self, request: &CardRequest) -> Result<String, String> {
        self.0.set(self.0.get() + 1);
        FakeWriter.reply(request)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Studio",
            "-c",
            "user.email=studio@example.com",
        ])
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// The shell's part: a fingerprint per session where the core asks, then the writer on each card.
struct Shell {
    studio: Studio,
    fingerprints: HashMap<String, Fingerprints>,
    writer: Counting,
    cards: Vec<Card>,
}

impl Shell {
    fn event(&mut self, name: &str, cwd: &Path) {
        let line = json!({"session_id": "s1", "hook_event_name": name, "cwd": cwd}).to_string();
        let found = self.studio.fingerprint_folder(&line);
        let change = found.and_then(|(session, folder)| {
            let prints = self.fingerprints.entry(session).or_default();
            prints.take(Path::new(&folder))
        });
        for output in self.studio.apply_with_change(&line, change) {
            if let Output::Card(request) = output {
                let card = write_card(&self.writer, &request, |line| panic!("{line}"));
                self.cards.push(card.unwrap());
            }
        }
    }
}

#[test]
fn a_prompt_that_changes_nothing_makes_no_writer_call_and_one_that_commits_gets_its_card() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    git(repo, &["init", "-q"]);
    fs::write(repo.join("lib.rs"), "fn a() {}\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "Start"]);
    let mut shell = Shell {
        studio: Studio::default(),
        fingerprints: HashMap::new(),
        writer: Counting::default(),
        cards: Vec::new(),
    };

    // Daniel's own edit before the prompt is not the prompt's change.
    fs::write(repo.join("notes.txt"), "mine\n").unwrap();
    shell.event("UserPromptSubmit", repo);
    shell.event("Stop", repo);
    assert_eq!(shell.writer.0.get(), 0);

    shell.event("UserPromptSubmit", repo);
    fs::write(repo.join("lib.rs"), "fn b() {}\n").unwrap();
    git(repo, &["commit", "-qam", "Rename a to b"]);
    shell.event("Stop", repo);

    assert_eq!(shell.writer.0.get(), 1);
    let renamed = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn b() {}\n";
    assert_eq!(shell.cards[0].code, renamed);
}
