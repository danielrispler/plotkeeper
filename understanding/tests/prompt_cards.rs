//! Seams 1 and 2 together (#369, Testing Decisions): hook events through the core, the fingerprint
//! and `git diff --stat` on a real repository, a fake writer and the card store, joined as the shell
//! joins them.

use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::Mutex,
};
use studio_core::{CardRequest, Kind, Output, Studio};
use studio_understanding::{
    Card, CardStore, CardWriter, FakeWriter, Fingerprints, write_brief, write_stop,
};

/// What a rewrite read: the prompts, the summaries and the diffstat.
type Rewrite = (Vec<String>, Vec<String>, String);

/// The fake writer, keeping what each call read: a card's earlier summaries and brief, each
/// rewrite, and each brief updated from workers' cards.
#[derive(Default)]
struct Recording {
    cards: RefCell<Vec<(Vec<String>, String)>>,
    rewrites: RefCell<Vec<Rewrite>>,
    updates: RefCell<Vec<(String, String)>>,
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

    fn update(&self, brief: &str, workers: &str) -> Result<String, String> {
        self.updates
            .borrow_mut()
            .push((brief.into(), workers.into()));
        FakeWriter.update(brief, workers)
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

    /// Takes event `name`, with `fields` added, and returns the cards and briefs it asks for.
    fn event(&mut self, name: &str, fields: Value) -> Vec<Output> {
        self.at += 1;
        let mut event = json!({"session_id": "s1", "hook_event_name": name, "cwd": self.repo,
            "studio_received_ms": self.at});
        event
            .as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let line = event.to_string();
        let looks = self.studio.fingerprint_folders(&line);
        let changes = looks.into_iter().filter_map(|look| {
            let prints = self.fingerprints.entry(look.key.clone()).or_default();
            Some((look.key, prints.look(Path::new(&look.folder), look.keep)?))
        });
        let outputs = self.studio.apply_with_changes(&line, changes.collect());
        let requested = |output: &Output| matches!(output, Output::Card(_) | Output::Brief(_));
        outputs.into_iter().filter(requested).collect()
    }

    /// Writes the card or the brief `output` asks for, as the shell's card thread does.
    fn write(&self, output: Output) {
        let session = self.studio.session("s1").unwrap();
        let log = |line| panic!("{line}");
        match output {
            Output::Card(request) => {
                assert!(write_stop(&self.store, &self.writer, request, session, log))
            }
            Output::Brief(request) => {
                assert!(write_brief(
                    &self.store,
                    &self.writer,
                    request,
                    session,
                    log
                ))
            }
            _ => {}
        }
    }

    /// Takes event `name`, with `fields` added, and writes the cards and briefs it asks for.
    fn take(&mut self, name: &str, fields: Value) {
        for output in self.event(name, fields) {
            self.write(output);
        }
    }

    /// Subagent `agent`'s event `name` in folder `cwd`, with `fields` added, written as `take` does.
    fn child(&mut self, agent: &str, cwd: &Path, name: &str, fields: Value) {
        let mut child = json!({ "agent_id": agent, "cwd": cwd });
        let fields = fields.as_object().unwrap().clone();
        child.as_object_mut().unwrap().extend(fields);
        self.take(name, child);
    }

    /// A new worktree of the repository, outside it, on its own branch `name`.
    fn worktree(&self, name: &str) -> PathBuf {
        let tree = self._dirs[1].path().join(name);
        git(
            &self.repo,
            &["worktree", "add", "-qb", name, tree.to_str().unwrap()],
        );
        tree
    }

    /// The cards in the store, oldest first: whose each is and its summary.
    fn cards(&self) -> Vec<(Option<String>, String)> {
        let store = self.store.lock().unwrap();
        let cards = store.cards().iter();
        cards
            .map(|kept| (kept.request.agent.clone(), kept.card.summary.clone()))
            .collect()
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

/// The spawn of subagent `id` with `description`, in a worktree of its own when `isolated`.
fn spawn_child(id: &str, description: &str, isolated: bool) -> Value {
    let isolation = if isolated { "worktree" } else { "" };
    json!({"tool_name": "Agent", "tool_response": {"agentId": id},
        "tool_input": {"description": description, "isolation": isolation, "prompt": "…"}})
}

/// A Bash step's result.
fn bash(command: &str) -> Value {
    json!({"tool_name": "Bash", "tool_input": {"command": command}})
}

/// The summary the fake writer gives a change to `path`.
fn summary(path: &str) -> String {
    format!("שינוי לדוגמה ב־`{path}`")
}

#[test]
fn a_worker_gets_a_card_at_each_commit_in_its_worktree_and_at_its_end_with_its_own_brief() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = shell.worktree("w1");
    shell.child("w1", &tree, "SubagentStart", json!({}));

    fs::write(tree.join("a.rs"), "fn a1() {}\n").unwrap();
    shell.child("w1", &tree, "PostToolUse", bash("cargo fmt"));
    assert_eq!(shell.cards(), []);
    git(&tree, &["add", "."]);
    git(&tree, &["commit", "-qm", "Add a1"]);
    shell.child("w1", &tree, "PostToolUse", bash("git commit -m 'Add a1'"));
    fs::write(tree.join("b.rs"), "fn b1() {}\n").unwrap();
    shell.child("w1", &tree, "SubagentStop", json!({}));

    let w1 = Some("w1".to_string());
    assert_eq!(
        shell.cards(),
        [(w1.clone(), summary("a.rs")), (w1, summary("b.rs"))]
    );
    let store = shell.store.lock().unwrap();
    let brief = store.brief("w1").expect("the worker's brief");
    assert!(brief.text.ends_with("`b.rs`."), "{}", brief.text);
    assert_eq!(store.brief("s1"), None);
}

/// #369, decisions 8 and 11: a killed worker has no `SubagentStop`; its end is read from its
/// wake-up, which also opens the session's next turn and takes that turn's base.
#[test]
fn a_killed_worker_gets_its_end_card_at_the_wake_up_that_reports_it() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = shell.worktree("w1");
    shell.child("w1", &tree, "SubagentStart", json!({}));
    shell.take("Stop", json!({}));
    fs::write(tree.join("a.rs"), "fn a1() {}\n").unwrap();
    // Daniel's own edit before the wake-up is not the woken turn's.
    fs::write(shell.repo.join("notes.txt"), "mine\n").unwrap();

    let killed =
        "<task-notification><task-id>w1</task-id><status>killed</status></task-notification>";
    shell.take("UserPromptSubmit", json!({ "prompt": killed }));
    shell.take("Stop", json!({}));

    assert_eq!(shell.cards(), [(Some("w1".into()), summary("a.rs"))]);
    assert_eq!(shell.writer.updates.borrow().len(), 1);
}

/// Worker `w1` in a worktree of its own writes `a.rs` and ends, with one card.
fn worker_writes_and_ends(shell: &mut Shell) {
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = shell.worktree("w1");
    shell.child("w1", &tree, "SubagentStart", json!({}));
    fs::write(tree.join("a.rs"), "fn a1() {}\n").unwrap();
    shell.child("w1", &tree, "SubagentStop", json!({}));
}

#[test]
fn a_parent_that_changed_nothing_itself_gets_one_card_from_its_workers_summaries_not_diffs() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    worker_writes_and_ends(&mut shell);
    shell.take("Stop", json!({}));

    let store = shell.store.lock().unwrap();
    let own: Vec<&CardRequest> = store.cards().iter().map(|kept| &kept.request).collect();
    let [worker, own] = own[..] else {
        panic!("a worker's card and its parent's: {own:?}");
    };
    assert_eq!(own.agent, None);
    assert_eq!(own.workers, std::slice::from_ref(&worker.id));
    let read = &own.change;
    assert!(
        read.contains(&format!(
            "- `Implement #378` (completed): {}\n",
            summary("a.rs")
        )),
        "{read}"
    );
    assert!(!read.contains("+++"), "{read}");
}

#[test]
fn a_wake_up_turn_that_changed_nothing_itself_updates_the_brief_and_writes_no_card() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    shell.take("Stop", json!({}));
    worker_writes_and_ends(&mut shell);
    let wake_up =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    shell.take("UserPromptSubmit", json!({ "prompt": wake_up }));
    shell.take("Stop", json!({ "last_assistant_message": "w1 is done." }));

    assert_eq!(shell.cards(), [(Some("w1".into()), summary("a.rs"))]);
    let updates = shell.writer.updates.borrow().clone();
    let [(brief, workers)] = &updates[..] else {
        panic!("one update: {updates:?}");
    };
    assert_eq!(brief, "");
    assert!(workers.contains(&summary("a.rs")), "{workers}");
    let updated = "העובדים חזרו, והסשן עצמו לא שינה דבר.";
    assert_eq!(shell.brief(), updated);
    assert_eq!(shell.line().as_deref(), Some(updated));
}

const MINUTE: u64 = 60 * 1000;

/// #369, decision 11: no timer; a step 20 minutes or more after the last card checks its worktree.
#[test]
fn after_25_minutes_of_uncommitted_change_the_next_step_gets_a_card_and_one_with_no_change_none() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    shell.take(
        "UserPromptSubmit",
        json!({ "prompt": "Implement #378 and #379" }),
    );
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    shell.take("PostToolUse", spawn_child("w2", "Implement #379", true));
    let (one, two) = (shell.worktree("w1"), shell.worktree("w2"));
    shell.child("w1", &one, "SubagentStart", json!({}));
    shell.child("w2", &two, "SubagentStart", json!({}));
    let read = json!({"tool_name": "Read", "tool_input": {"file_path": "lib.rs"}});

    fs::write(one.join("a.rs"), "fn a1() {}\n").unwrap();
    fs::write(repo.join("lib.rs"), "fn b() {}\n").unwrap();
    shell.child("w1", &one, "PostToolUse", read.clone());
    assert_eq!(shell.cards(), []);
    shell.at += 25 * MINUTE;
    shell.child("w1", &one, "PostToolUse", read.clone());
    shell.child("w2", &two, "PostToolUse", read.clone());
    // The session's own prompt too.
    shell.take("PostToolUse", read);

    let (w1, own) = (Some("w1".to_string()), None);
    assert_eq!(
        shell.cards(),
        [(w1, summary("a.rs")), (own, summary("lib.rs"))]
    );
    assert_eq!(shell.writer.cards.borrow().len(), 2);
}

/// #369, decision 11: the fallback is on a step of a worker or of a prompt; after the session's
/// `Stop` there is no prompt, so Daniel's own edit is no one's card.
#[test]
fn a_background_helpers_step_after_the_stop_writes_no_card_for_the_session_after_25_minutes() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    shell.take(
        "UserPromptSubmit",
        json!({ "prompt": "Review in the background" }),
    );
    shell.take("PostToolUse", spawn_child("h1", "Review", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    shell.take("Stop", json!({}));

    fs::write(repo.join("lib.rs"), "fn b() {}\n").unwrap();
    shell.at += 25 * MINUTE;
    let read = json!({"tool_name": "Read", "tool_input": {"file_path": "lib.rs"}});
    shell.child("h1", &repo, "PostToolUse", read);

    assert_eq!(shell.cards(), []);
}

#[test]
fn a_helpers_edit_in_its_parents_worktree_shows_on_the_parents_card_tagged_with_the_helper() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    shell.take("UserPromptSubmit", json!({ "prompt": "Review and fix" }));
    shell.take("PostToolUse", spawn_child("h1", "Review the diff", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    fs::write(repo.join("lib.rs"), "fn a() { b() }\n").unwrap();
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": repo.join("lib.rs")}});
    shell.child("h1", &repo, "PostToolUse", edit);
    shell.take("Stop", json!({}));

    let store = shell.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    assert_eq!(card.request.agent, None);
    let tagged = BTreeMap::from([("h1".to_string(), BTreeSet::from(["lib.rs".to_string()]))]);
    assert_eq!(card.request.changed_by, tagged);
}

/// #372's gap: the shell now fingerprints a helper's Bash step, so a command that changes its
/// worktree makes it a worker live, not only in the core's test.
#[test]
fn a_helper_whose_command_changes_its_worktree_becomes_a_worker() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    shell.take("UserPromptSubmit", json!({ "prompt": "Format the code" }));
    shell.take("PostToolUse", spawn_child("h1", "Format", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    let kind = |shell: &Shell| shell.studio.session("s1").unwrap().agents[0].kind;

    shell.child("h1", &repo, "PreToolUse", bash("cargo fmt --check"));
    shell.child("h1", &repo, "PostToolUse", bash("cargo fmt --check"));
    assert_eq!(kind(&shell), Kind::Helper);
    shell.child("h1", &repo, "PreToolUse", bash("cargo fmt"));
    fs::write(repo.join("lib.rs"), "fn a() {}\n\n").unwrap();
    shell.child("h1", &repo, "PostToolUse", bash("cargo fmt"));

    assert_eq!(kind(&shell), Kind::Worker);
}

/// #369, decision 6: only a change across the helper's own Bash step counts, not what its parent or
/// Daniel changed in the shared worktree before it.
#[test]
fn a_helper_whose_command_changes_nothing_stays_a_helper_after_others_change_its_worktree() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    shell.take("UserPromptSubmit", json!({ "prompt": "Review the change" }));
    shell.take("PostToolUse", spawn_child("h1", "Review", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    let kind = |shell: &Shell| shell.studio.session("s1").unwrap().agents[0].kind;

    fs::write(repo.join("lib.rs"), "fn b() {}\n").unwrap();
    shell.child("h1", &repo, "PreToolUse", bash("cargo test"));
    shell.child("h1", &repo, "PostToolUse", bash("cargo test"));

    assert_eq!(kind(&shell), Kind::Helper);
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
    let outputs = shell.event("Stop", json!({ "last_assistant_message": message }));
    assert_eq!(shell.line().as_deref(), Some("Renamed `a` to `b`."));

    shell.write(outputs.into_iter().next().unwrap());
    assert_eq!(shell.line().as_deref(), Some("שינוי לדוגמה ב־`lib.rs`"));

    // A turn that changed nothing gets no card: the row shows the brief's latest line.
    shell.take("UserPromptSubmit", json!({ "prompt": "Anything left?" }));
    shell.take("Stop", json!({ "last_assistant_message": "Nothing left." }));
    assert_eq!(
        shell.line().as_deref(),
        Some("השינוי האחרון נגע ב־`lib.rs`.")
    );
}
