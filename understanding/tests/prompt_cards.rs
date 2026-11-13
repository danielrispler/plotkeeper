//! Seams 1 and 2 together (danielrispler/Falinks#369, Testing Decisions): hook events through the core, the fingerprint
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
    Card, CardStore, CardWriter, FakeWriter, Fingerprints, explain_more, fingerprint_changes,
    worktree_root, write_brief, write_stop,
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
    /// The change each "explain more" read.
    mores: RefCell<Vec<String>>,
    /// The subagents whose cards get an empty summary.
    blank: RefCell<Vec<String>>,
    /// Each "New card" notice the shell sent and each rewrite, in turn.
    sent: RefCell<Vec<&'static str>>,
}

impl CardWriter for Recording {
    fn reply(
        &self,
        request: &CardRequest,
        earlier: &[String],
        brief: &str,
    ) -> Result<String, String> {
        self.cards.borrow_mut().push((earlier.into(), brief.into()));
        let reply = FakeWriter.reply(request, earlier, brief)?;
        let agent = request.agent.as_ref();
        if !self.blank.borrow().iter().any(|a| agent == Some(a)) {
            return Ok(reply);
        }
        let mut card: Value = serde_json::from_str(&reply).unwrap();
        card["summary"] = "".into();
        Ok(card.to_string())
    }

    fn more(&self, request: &CardRequest, card: &Card) -> Result<String, String> {
        self.mores.borrow_mut().push(request.diff().into());
        FakeWriter.more(request, card)
    }

    fn rewrite(
        &self,
        prompts: &[&str],
        summaries: &[String],
        diffstat: &str,
    ) -> Result<String, String> {
        self.sent.borrow_mut().push("rewrite");
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
    /// The event log so far, which a restart replays.
    log: Vec<String>,
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
            studio: Studio::with_roots(worktree_root),
            fingerprints: HashMap::new(),
            writer: Recording::default(),
            store: Mutex::new(CardStore::open(data.path().join("cards.json")).unwrap()),
            repo,
            at: 1_793_400_000_000,
            log: Vec::new(),
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
        let changes = fingerprint_changes(looks, &mut self.fingerprints);
        let outputs = self.studio.apply_with_changes(&line, changes);
        self.log.push(line);
        let requested = |output: &Output| {
            matches!(
                output,
                Output::Card(_) | Output::Brief(_) | Output::Promoted { .. }
            )
        };
        outputs.into_iter().filter(requested).collect()
    }

    /// Writes the card or the brief `output` asks for, as the shell's card thread does.
    fn write(&self, output: Output) {
        let session = self.studio.session("s1").unwrap();
        let log = |line| panic!("{line}");
        match output {
            Output::Card(request) => {
                let notice = || self.writer.sent.borrow_mut().push("notice");
                let written = write_stop(&self.store, &self.writer, request, session, notice, log);
                assert!(written)
            }
            Output::Promoted { agent } => self.store.lock().unwrap().promoted(&agent).unwrap(),
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

    /// The app restarts as the shell starts: the store read from disk, the log replayed, the store's
    /// facts and cards applied, and the turns and subagents still open fingerprinted.
    fn restart(&mut self) {
        let path = self._dirs[1].path().join("cards.json");
        self.store = Mutex::new(CardStore::open(path).unwrap());
        self.studio = Studio::with_roots(worktree_root);
        self.log.iter().for_each(|line| self.studio.replay(line));
        self.store.lock().unwrap().restore(&mut self.studio);
        self.fingerprints = HashMap::new();
        fingerprint_changes(self.studio.waiting_folders(), &mut self.fingerprints);
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

    /// What the session's row says now.
    fn line(&self) -> String {
        self.line_at(self.at)
    }

    /// What the session's row says at `now`.
    fn line_at(&self, now: u64) -> String {
        let session = self.studio.session("s1").unwrap();
        self.store.lock().unwrap().line(session, now)
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
    format!("Sample change to `{path}`")
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

/// danielrispler/Falinks#369, decisions 8 and 11: a killed worker has no `SubagentStop`; its end is read from its
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
    let read = own.diff().to_owned();
    assert!(
        read.contains(&format!(
            "- `Implement #378` (completed): {}\n",
            summary("a.rs")
        )),
        "{read}"
    );
    assert!(!read.contains("+++"), "{read}");
    let id = own.id.clone();
    drop(store);
    // "Explain more" reads the workers' text the stored card keeps.
    explain_more(&shell.store, &shell.writer, &id, |line| panic!("{line}")).unwrap();
    assert_eq!(*shell.writer.mores.borrow(), [read]);
}

#[test]
fn a_worker_card_with_an_empty_summary_is_left_out_of_its_parents_input() {
    let mut shell = Shell::new();
    shell.writer.blank.borrow_mut().push("w2".into());
    shell.take(
        "UserPromptSubmit",
        json!({ "prompt": "Implement #378 and #379" }),
    );
    worker_writes_and_ends(&mut shell);
    shell.take("PostToolUse", spawn_child("w2", "Implement #379", true));
    let tree = shell.worktree("w2");
    shell.child("w2", &tree, "SubagentStart", json!({}));
    fs::write(tree.join("b.rs"), "fn b1() {}\n").unwrap();
    shell.child("w2", &tree, "SubagentStop", json!({}));
    shell.take("Stop", json!({}));

    let store = shell.store.lock().unwrap();
    let parent = &store.cards().last().unwrap().request;
    assert_eq!(parent.workers.len(), 2, "{parent:?}");
    let read = parent.diff();
    assert!(read.contains("`Implement #378`"), "{read}");
    assert!(!read.contains("`Implement #379`"), "{read}");
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
    let updated = "The workers came back, and the session itself changed nothing.";
    assert_eq!(shell.brief(), updated);
    assert_eq!(shell.line(), updated);
}

const MINUTE: u64 = 60 * 1000;

/// danielrispler/Falinks#369, decision 11: no timer; a step 20 minutes or more after the last card checks its worktree.
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

/// danielrispler/Falinks#410, decision 23: a fallback look that finds no change restarts the 20-minute wait, so a quiet
/// worktree gets one look, not one per step, and a later change waits for the next fallback.
#[test]
fn a_quiet_worktrees_fallback_look_restarts_its_wait_so_a_later_change_waits_20_minutes_from_it() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let one = shell.worktree("w1");
    shell.child("w1", &one, "SubagentStart", json!({}));
    let read = json!({"tool_name": "Read", "tool_input": {"file_path": "lib.rs"}});

    shell.at += 25 * MINUTE;
    shell.child("w1", &one, "PostToolUse", read.clone());
    fs::write(one.join("a.rs"), "fn a1() {}\n").unwrap();
    shell.at += MINUTE;
    shell.child("w1", &one, "PostToolUse", read.clone());
    shell.at += 18 * MINUTE;
    shell.child("w1", &one, "PostToolUse", read.clone());
    assert_eq!(shell.cards(), []);
    shell.at += MINUTE;
    shell.child("w1", &one, "PostToolUse", read);

    assert_eq!(shell.cards(), [(Some("w1".to_string()), summary("a.rs"))]);
}

/// danielrispler/Falinks#369, decision 11: the fallback is on a step of a worker or of a prompt; after the session's
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

/// danielrispler/Falinks#372's gap: the shell now fingerprints a helper's Bash step, so a command that changes its
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

/// danielrispler/Falinks#369, decision 6: only a change across the helper's own Bash step counts, not what its parent or
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
            (vec!["Sample change to `a.rs`".to_string()], first_brief),
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
    let summary = |path| format!("Sample change to `{path}`");
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
    assert!(brief.text.contains("rewritten"), "{}", brief.text);
    assert_eq!(brief.updates, 5);
}

#[test]
fn the_new_card_notice_goes_out_before_the_rewrite_starts() {
    let mut shell = Shell::new();
    for (prompt, path) in [
        ("Add a", "a.rs"),
        ("Add b", "b.rs"),
        ("Add c", "c.rs"),
        ("Add d", "d.rs"),
        ("Add e", "e.rs"),
    ] {
        shell.prompt(prompt, path);
    }

    assert_eq!(
        *shell.writer.sent.borrow(),
        ["notice", "notice", "notice", "notice", "notice", "rewrite"]
    );
}

#[test]
fn a_stopped_row_shows_the_agents_first_line_then_the_cards_summary_then_the_briefs_last_line() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Rename a" }));
    fs::write(shell.repo.join("lib.rs"), "fn b() {}\n").unwrap();
    assert_eq!(shell.line(), "No step yet");

    let message = "\nRenamed `a` to `b`.\n\nThe tests pass.";
    let outputs = shell.event("Stop", json!({ "last_assistant_message": message }));
    assert_eq!(shell.line(), "Renamed `a` to `b`.");

    shell.write(outputs.into_iter().next().unwrap());
    assert_eq!(shell.line(), "Sample change to `lib.rs`");

    // A turn that changed nothing gets no card: the row shows the brief's latest line.
    shell.take("UserPromptSubmit", json!({ "prompt": "Anything left?" }));
    shell.take("Stop", json!({ "last_assistant_message": "Nothing left." }));
    assert_eq!(shell.line(), "The latest change touched `lib.rs`.");
}

#[test]
fn a_rows_line_is_who_needs_you_then_the_stopped_line_then_who_returned_then_the_last_step() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    assert_eq!(shell.line(), "No step yet");
    shell.take(
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "t1",
        "tool_input": {"command": "cargo test"}}),
    );
    assert_eq!(shell.line(), "Bash cargo test");

    // The session's own agent asks: its last step and how long it has waited.
    let ask = json!({"tool_name": "AskUserQuestion", "tool_use_id": "t2", "tool_input": {}});
    shell.take("PreToolUse", ask.clone());
    let line = shell.line_at(shell.at + 3 * MINUTE);
    assert!(
        line.starts_with("AskUserQuestion") && line.ends_with(" · 3m"),
        "{line}"
    );
    shell.take("PostToolUse", ask);

    // A subagent asks: its name, "asks", and how long it has waited.
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", false));
    let ask = json!({"tool_name": "AskUserQuestion", "tool_use_id": "t3", "tool_input": {}});
    let repo = shell.repo.clone();
    shell.child("w1", &repo, "PreToolUse", ask.clone());
    assert_eq!(
        shell.line_at(shell.at + 2 * MINUTE),
        "Implement #378 asks · 2m"
    );
    shell.child("w1", &repo, "PostToolUse", ask);
    shell.child("w1", &repo, "SubagentStop", json!({}));

    // A wake-up with no step since: the worker that returned.
    let wake_up =
        "<task-notification><task-id>w1</task-id><status>completed</status></task-notification>";
    shell.take("UserPromptSubmit", json!({ "prompt": wake_up }));
    assert_eq!(shell.line(), "worker Implement #378 returned");
    shell.take(
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_use_id": "t4",
        "tool_input": {"command": "git log"}}),
    );
    assert_eq!(shell.line(), "Bash git log");

    // Once the turn is over: the stopped line.
    shell.event("Stop", json!({ "last_assistant_message": "Done." }));
    assert_eq!(shell.line(), "Done.");
}

/// danielrispler/Falinks#434, decisions 37 and 38: after a restart the shell's join still knows the helper promoted live
/// and the worker cards its session collected, from the store on disk.
#[test]
fn after_a_restart_a_promoted_helper_is_a_worker_and_the_stop_writes_its_card_from_the_workers() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    shell.take("UserPromptSubmit", json!({ "prompt": "Implement #378" }));
    worker_writes_and_ends(&mut shell);
    shell.take("PostToolUse", spawn_child("h1", "Format", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    shell.child("h1", &repo, "PreToolUse", bash("cargo fmt"));
    fs::write(repo.join("lib.rs"), "fn a() {}\n\n").unwrap();
    shell.child("h1", &repo, "PostToolUse", bash("cargo fmt"));
    // Its change is undone, so the session's own worktree ends where its turn began.
    git(&repo, &["checkout", "-q", "lib.rs"]);

    shell.restart();
    let kind = |shell: &Shell| shell.studio.session("s1").unwrap().agents[1].kind;
    assert_eq!(kind(&shell), Kind::Worker);
    shell.take("Stop", json!({}));

    let store = shell.store.lock().unwrap();
    let requests: Vec<&CardRequest> = store.cards().iter().map(|kept| &kept.request).collect();
    let [worker, own] = requests[..] else {
        panic!("a worker's card and its parent's: {requests:?}");
    };
    assert_eq!(
        (own.agent.as_deref(), &own.workers[..]),
        (None, std::slice::from_ref(&worker.id))
    );
}

/// The session's own cards, each as the diff it asked the writer about.
fn own_changes(shell: &Shell) -> Vec<String> {
    let store = shell.store.lock().unwrap();
    let own = store
        .cards()
        .iter()
        .filter(|kept| kept.request.agent.is_none());
    own.map(|kept| kept.request.diff().to_string()).collect()
}

/// danielrispler/Falinks#434, decisions 8 to 10: a base per worktree at the turn's first touch, and one joined card.
#[test]
fn a_turn_that_starts_in_one_worktree_and_stops_in_another_gets_one_card_with_both_changes() {
    let mut shell = Shell::new();
    let other = shell.worktree("other");
    // Changes before the turn are no one's.
    fs::write(shell.repo.join("notes.txt"), "mine\n").unwrap();
    fs::write(other.join("notes.txt"), "mine\n").unwrap();

    shell.take("UserPromptSubmit", json!({ "prompt": "Fix both" }));
    fs::write(shell.repo.join("a.rs"), "fn a1() {}\n").unwrap();
    let cd = json!({"tool_name": "Bash", "tool_input": {"command": "cd ../other"}, "cwd": other});
    shell.take("PostToolUse", cd);
    fs::write(other.join("b.rs"), "fn b1() {}\n").unwrap();
    shell.take("Stop", json!({ "cwd": other }));

    let [change] = &own_changes(&shell)[..] else {
        panic!("one card: {:?}", own_changes(&shell));
    };
    assert!(
        change.contains("+++ b/a.rs") && change.contains("+++ b/b.rs"),
        "{change}"
    );
    assert!(!change.contains("notes.txt"), "{change}");
}

/// danielrispler/Falinks#434, decision 9: an `Edit` path in another worktree touches it, with no `cd`.
#[test]
fn an_edit_into_another_worktree_without_a_cd_puts_its_change_on_the_card() {
    let mut shell = Shell::new();
    let other = shell.worktree("other");
    shell.take(
        "UserPromptSubmit",
        json!({ "prompt": "Fix the other tree" }),
    );
    let path = other.join("lib.rs");
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": path}});
    shell.take("PreToolUse", edit.clone());
    fs::write(&path, "fn c() {}\n").unwrap();
    shell.take("PostToolUse", edit);
    shell.take("Stop", json!({}));

    let [change] = &own_changes(&shell)[..] else {
        panic!("one card: {:?}", own_changes(&shell));
    };
    assert!(change.contains("+fn c() {}"), "{change}");
}

/// danielrispler/Falinks#434, decision 10: a worktree whose worker has its own cards is left out of the session's.
#[test]
fn a_worktree_workers_change_is_on_its_own_cards_only_while_the_session_sits_in_its_worktree() {
    let mut shell = Shell::new();
    shell.take("UserPromptSubmit", json!({ "prompt": "Start #378" }));
    shell.take("PostToolUse", spawn_child("w1", "Implement #378", true));
    let tree = shell.worktree("w1");
    shell.child("w1", &tree, "SubagentStart", json!({}));
    shell.take("Stop", json!({}));

    shell.take(
        "UserPromptSubmit",
        json!({ "prompt": "Watch it", "cwd": tree }),
    );
    fs::write(tree.join("a.rs"), "fn a1() {}\n").unwrap();
    shell.child("w1", &tree, "SubagentStop", json!({}));
    shell.take("Stop", json!({ "cwd": tree }));

    let worker = shell
        .cards()
        .into_iter()
        .filter(|(agent, _)| agent.is_some());
    assert_eq!(
        worker.collect::<Vec<_>>(),
        [(Some("w1".to_string()), summary("a.rs"))]
    );
    // The session's own card, if any, is built from its worker's card, not from the diff.
    let own = own_changes(&shell);
    assert!(own.iter().all(|c| !c.contains("+++ b/a.rs")), "{own:?}");
}

/// danielrispler/Falinks#434, decision 11: ownership is decided once, by worktree root, at the subagent's first event.
#[test]
fn a_helper_in_the_sessions_folder_keeps_the_session_as_its_owner_after_the_session_cds_into_sub() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    fs::create_dir(repo.join("sub")).unwrap();
    shell.take("UserPromptSubmit", json!({ "prompt": "Review and fix" }));
    shell.take("PostToolUse", spawn_child("h1", "Review", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    let sub = repo.join("sub");
    let cd = json!({"tool_name": "Bash", "tool_input": {"command": "cd sub"}, "cwd": sub});
    shell.take("PostToolUse", cd);
    fs::write(repo.join("lib.rs"), "fn a() { b() }\n").unwrap();
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": repo.join("lib.rs")}});
    shell.child("h1", &repo, "PostToolUse", edit);
    shell.take("Stop", json!({ "cwd": sub }));

    let store = shell.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    assert_eq!(card.request.agent, None);
    let tagged = BTreeMap::from([("h1".to_string(), BTreeSet::from(["lib.rs".to_string()]))]);
    assert_eq!(card.request.changed_by, tagged);
}

/// danielrispler/Falinks#434, decision 15: "who changed what" keeps only paths inside the owner's folder.
#[test]
fn a_write_outside_the_owners_folder_is_not_in_who_changed_what() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    let scratch = tempfile::tempdir().unwrap();
    shell.take("UserPromptSubmit", json!({ "prompt": "Review and fix" }));
    shell.take("PostToolUse", spawn_child("h1", "Review", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    fs::write(repo.join("lib.rs"), "fn a() { b() }\n").unwrap();
    for path in [repo.join("lib.rs"), scratch.path().join("note.md")] {
        let write = json!({"tool_name": "Write", "tool_input": {"file_path": path}});
        shell.child("h1", &repo, "PostToolUse", write);
    }
    shell.take("Stop", json!({}));

    let store = shell.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    let tagged = BTreeMap::from([("h1".to_string(), BTreeSet::from(["lib.rs".to_string()]))]);
    assert_eq!(card.request.changed_by, tagged);
}

/// A background helper in the session's worktree, spawned in a turn that then stops, edits
/// `lib.rs`.
fn background_helper_edits_after_the_stop(shell: &mut Shell) {
    let repo = shell.repo.clone();
    shell.take(
        "UserPromptSubmit",
        json!({ "prompt": "Fix it in the background" }),
    );
    shell.take("PostToolUse", spawn_child("h1", "Fix", false));
    shell.child("h1", &repo, "SubagentStart", json!({}));
    shell.take("Stop", json!({}));
    fs::write(repo.join("lib.rs"), "fn a() { b() }\n").unwrap();
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": repo.join("lib.rs")}});
    shell.child("h1", &repo, "PostToolUse", edit);
}

/// The one card in the store: the session's, on the helper's edit, tagged with it.
fn the_helpers_card_is_the_sessions(shell: &Shell) {
    let store = shell.store.lock().unwrap();
    let [card] = store.cards() else {
        panic!("one card: {:?}", store.cards());
    };
    assert_eq!(card.request.agent, None);
    assert!(card.request.changed_by.contains_key("h1"));
    assert!(card.request.diff().contains("+fn a() { b() }"));
}

/// danielrispler/Falinks#434, decision 16: a subagent's end outside a turn writes the session's card, tagged with it.
#[test]
fn a_background_helpers_edits_after_the_stop_get_a_card_tagged_with_it_when_it_ends() {
    let mut shell = Shell::new();
    background_helper_edits_after_the_stop(&mut shell);
    assert_eq!(shell.cards(), []);

    let repo = shell.repo.clone();
    shell.child("h1", &repo, "SubagentStop", json!({}));

    the_helpers_card_is_the_sessions(&shell);
}

/// danielrispler/Falinks#434, decision 17: the next turn's start cards what is still uncarded, before its base.
#[test]
fn a_background_helpers_edits_get_a_card_at_the_next_turns_start_if_it_never_ends() {
    let mut shell = Shell::new();
    background_helper_edits_after_the_stop(&mut shell);

    shell.take("UserPromptSubmit", json!({ "prompt": "Next" }));
    shell.take("Stop", json!({}));

    the_helpers_card_is_the_sessions(&shell);
}

/// danielrispler/Falinks#434, decision 17: such edits in a worktree that the next turn touches only after its start
/// still get their card, so that worktree's base never swallows them.
#[test]
fn a_background_helpers_edits_in_another_worktree_get_a_card_when_the_next_turn_moves_there() {
    let mut shell = Shell::new();
    let repo = shell.repo.clone();
    let tree = shell.worktree("b");
    let cd = || json!({"tool_name": "Bash", "tool_input": {"command": "cd b"}, "cwd": tree});
    shell.take("UserPromptSubmit", json!({ "prompt": "Fix it over there" }));
    shell.take("PostToolUse", cd());
    let mut spawn = spawn_child("h1", "Fix", false);
    spawn["cwd"] = json!(tree);
    shell.take("PostToolUse", spawn);
    shell.child("h1", &tree, "SubagentStart", json!({}));
    shell.take("Stop", json!({ "cwd": tree }));
    fs::write(tree.join("lib.rs"), "fn a() { b() }\n").unwrap();
    let edit = json!({"tool_name": "Edit", "tool_input": {"file_path": tree.join("lib.rs")}});
    shell.child("h1", &tree, "PostToolUse", edit);

    shell.take("UserPromptSubmit", json!({ "prompt": "Next", "cwd": repo }));
    shell.take("PostToolUse", cd());
    shell.take("Stop", json!({ "cwd": tree }));

    the_helpers_card_is_the_sessions(&shell);
}
