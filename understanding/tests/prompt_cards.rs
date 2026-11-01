//! Seams 1 and 2 together (#369, Testing Decisions): hook events through the core, the fingerprint
//! and `git diff --stat` on a real repository, a fake writer and the card store, joined as the shell
//! joins them.

use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use studio_core::{CardRequest, Output, Studio};
use studio_understanding::{Card, CardStore, CardWriter, FakeWriter, Fingerprints, write_stop};

/// What a rewrite read: the prompts, the summaries and the diffstat.
type Rewrite = (Vec<String>, Vec<String>, String);

/// The fake writer, keeping what each call read: a card's earlier summaries and brief, and each
/// rewrite.
#[derive(Default)]
struct Recording {
    cards: RefCell<Vec<(Vec<String>, String)>>,
    rewrites: RefCell<Vec<Rewrite>>,
}

impl CardWriter for Recording {
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        self.cards.borrow_mut().push((earlier.into(), brief.into()));
        FakeWriter.reply(request, earlier, brief)
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        FakeWriter.more(request, card)
    }

    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String> {
        let read = prompts.iter().map(|p| p.to_string()).collect();
        self.rewrites
            .borrow_mut()
            .push((read, summaries.into(), diffstat.into()));
        FakeWriter.rewrite(prompts, summaries, diffstat)
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

/// The shell's part for session `s1` in one repository: a fingerprint where the core asks, then the
/// writer and the store on each card.
struct Shell {
    studio: Studio,
    fingerprints: HashMap<String, Fingerprints>,
    writer: Recording,
    store: Mutex<CardStore>,
    repo: PathBuf,
    /// When the hook received the latest event, one millisecond apart.
    at: u64,
    _dirs: [tempfile::TempDir; 2],
}

impl Shell {
    /// A shell on a new repository with one commit, and an empty store.
    fn new() -> Shell {
        let (code, data) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let repo = code.path().to_owned();
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("lib.rs"), "fn a() {}\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "Start"]);
        Shell {
            studio: Studio::default(),
            fingerprints: HashMap::new(),
            writer: Recording::default(),
            store: Mutex::new(CardStore::open(data.path().join("cards.json")).unwrap()),
            repo,
            at: 1_793_400_000_000,
            _dirs: [code, data],
        }
    }

    /// Takes event `name`, with `fields` added, and returns the cards it asks for.
    fn event(&mut self, name: &str, fields: Value) -> Vec<CardRequest> {
        self.at += 1;
        let mut event = json!({"session_id": "s1", "hook_event_name": name, "cwd": self.repo,
            "studio_received_ms": self.at});
        event
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let line = event.to_string();
        let found = self.studio.fingerprint_folder(&line);
        let change = found.and_then(|(session, folder)| {
            let prints = self.fingerprints.entry(session).or_default();
            prints.take(Path::new(&folder))
        });
        let outputs = self.studio.apply_with_change(&line, change);
        let cards = outputs.into_iter().filter_map(|output| match output {
            Output::Card(request) => Some(request),
            _ => None,
        });
        cards.collect()
    }

    /// Writes the card on `request` and updates the brief, as the shell's card thread does.
    fn write(&self, request: CardRequest) {
        let prompts = &self.studio.session("s1").unwrap().prompts;
        let log = |line| panic!("{line}");
        assert!(write_stop(
            &self.store,
            &self.writer,
            request,
            prompts,
            &self.repo,
            log
        ));
    }

    /// Takes event `name`, with `fields` added, and writes the cards it asks for.
    fn take(&mut self, name: &str, fields: Value) {
        for request in self.event(name, fields) {
            self.write(request);
        }
    }

    /// Daniel's prompt `text`, a new file `path`, committed, and the turn's `Stop`.
    fn prompt(&mut self, text: &str, path: &str) {
        self.take("UserPromptSubmit", json!({ "prompt": text }));
        fs::write(self.repo.join(path), format!("fn {}() {{}}\n", &path[..1])).unwrap();
        git(&self.repo, &["add", "."]);
        git(&self.repo, &["commit", "-qm", path]);
        self.take("Stop", json!({}));
    }

    /// The session's brief.
    fn brief(&self) -> String {
        let store = self.store.lock().unwrap();
        store
            .brief("s1")
            .map(|b| b.text.clone())
            .unwrap_or_default()
    }

    /// What the session's row says while its turn is over.
    fn line(&self) -> Option<String> {
        let session = self.studio.session("s1").unwrap();
        self.store.lock().unwrap().line(session)
    }
}

/// The spawn of worker `a1`, whose return wakes the session.
fn spawn() -> Value {
    json!({"tool_name": "Agent", "tool_input": {"description": "Implement #377"},
        "tool_response": {"agentId": "a1"}})
}

const WAKE_UP: &str =
    "<task-notification><task-id>a1</task-id><status>completed</status></task-notification>";

#[test]
fn a_prompt_that_changes_nothing_makes_no_writer_call_and_one_that_commits_gets_its_card() {
    let mut shell = Shell::new();

    // Daniel's own edit before the prompt is not the prompt's change.
    fs::write(shell.repo.join("notes.txt"), "mine\n").unwrap();
    shell.take("UserPromptSubmit", json!({}));
    shell.take("Stop", json!({}));
    assert_eq!(shell.writer.cards.borrow().len(), 0);

    shell.take("UserPromptSubmit", json!({}));
    fs::write(shell.repo.join("lib.rs"), "fn b() {}\n").unwrap();
    git(&shell.repo, &["commit", "-qam", "Rename a to b"]);
    shell.take("Stop", json!({}));

    assert_eq!(shell.writer.cards.borrow().len(), 1);
    let renamed = "--- a/lib.rs\n+++ b/lib.rs\n@@ -1 +1 @@\n-fn a() {}\n+fn b() {}\n";
    assert_eq!(shell.store.lock().unwrap().cards()[0].card.code, renamed);
}

#[test]
fn a_wake_ups_card_reads_its_prompts_earlier_cards_and_each_card_the_brief_so_far() {
    let mut shell = Shell::new();
    shell.take("SessionStart", json!({ "source": "startup" }));
    shell.take("PostToolUse", spawn());
    shell.prompt("Split the spec into tickets", "a.rs");
    let first_brief = shell.brief();
    shell.prompt(WAKE_UP, "b.rs");
    let second_brief = shell.brief();
    shell.prompt("What did a1 do?", "c.rs");

    let calls = shell.writer.cards.borrow().clone();
    assert_eq!(
        calls,
        [
            (vec![], String::new()),
            // The wake-up's card is on Daniel's prompt before it.
            (vec!["שינוי לדוגמה ב־`a.rs`".to_string()], first_brief),
            (vec![], second_brief.clone()),
        ]
    );
    assert!(second_brief.ends_with("`b.rs`."), "{second_brief}");
    assert!(shell.brief().ends_with("`c.rs`."));
}

#[test]
fn the_5th_update_rewrites_the_brief_from_daniels_prompts_the_summaries_and_the_diffstat() {
    let mut shell = Shell::new();
    shell.take("SessionStart", json!({ "source": "startup" }));
    shell.take("PostToolUse", spawn());
    for (prompt, path) in [
        ("Add a", "a.rs"),
        ("Add b", "b.rs"),
        (WAKE_UP, "c.rs"),
        ("Add d", "d.rs"),
    ] {
        shell.prompt(prompt, path);
    }
    assert_eq!(shell.writer.rewrites.borrow().len(), 0);
    assert!(shell.brief().ends_with("`d.rs`."));

    shell.prompt("Add e", "e.rs");

    let rewrites = shell.writer.rewrites.borrow().clone();
    let [(prompts, summaries, diffstat)] = &rewrites[..] else {
        panic!("one rewrite: {rewrites:?}");
    };
    // No wake-up, and every card's summary.
    assert_eq!(prompts, &["Add a", "Add b", "Add d", "Add e"]);
    let summary = |path| format!("שינוי לדוגמה ב־`{path}`");
    let all: Vec<String> = ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs"].map(summary).into();
    assert_eq!(summaries, &all);
    // The diffstat since the session began, not the raw diff.
    assert!(diffstat.contains(" e.rs | 1 +"), "{diffstat}");
    assert!(
        diffstat.contains("5 files changed, 5 insertions(+)"),
        "{diffstat}"
    );
    assert!(!diffstat.contains("fn "), "{diffstat}");
    let store = shell.store.lock().unwrap();
    let brief = store.brief("s1").unwrap();
    assert!(brief.text.contains("שנכתב מחדש"), "{}", brief.text);
    assert_eq!(brief.updates, 5);
}

#[test]
fn a_stopped_row_shows_the_agents_first_line_then_the_cards_summary_then_the_briefs_last_line() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Rename a" }));
    fs::write(shell.repo.join("lib.rs"), "fn b() {}\n").unwrap();
    assert_eq!(shell.line(), None);

    let message = "\nRenamed `a` to `b`.\n\nThe tests pass.";
    let requests = shell.event("Stop", json!({ "last_assistant_message": message }));
    assert_eq!(shell.line().as_deref(), Some("Renamed `a` to `b`."));

    shell.write(requests.into_iter().next().unwrap());
    assert_eq!(shell.line().as_deref(), Some("שינוי לדוגמה ב־`lib.rs`"));

    // A turn that changed nothing gets no card: the row shows the brief's latest line.
    shell.take("UserPromptSubmit", json!({ "prompt": "Anything left?" }));
    shell.take("Stop", json!({ "last_assistant_message": "Nothing left." }));
    assert_eq!(
        shell.line().as_deref(),
        Some("השינוי האחרון נגע ב־`lib.rs`.")
    );
}
