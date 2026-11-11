//! Seam 1 (#470): hook events replayed through the core, with the daemon's `list` of terminals, give
//! the upgrade's idle rule (#322 resolution, point 4): idle exactly when no shell runs a program and
//! no Claude session is in a turn or has background tasks. The same answer names how each terminal
//! comes back: a Claude terminal with `claude --resume <session_id>`, a shell as a shell.

use serde_json::{Value, json};
use studio_core::{Studio, Terminal};

const SESSION: &str = "5b1f0c2e-8d3a-4e7b-9a6c-1f2e3d4c5b6a";
const CWD: &str = "/Users/danielrispler/work/Falinks";

fn event(name: &str, extra: Value) -> String {
    let mut event = json!({
        "session_id": SESSION,
        "hook_event_name": name,
        "cwd": CWD,
        "prompt_id": "p-1",
        "studio_terminal_id": "claude-1",
        "studio_received_ms": 1_793_262_000_000u64,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    event.to_string()
}

/// A row of the daemon's `list`: the terminal's leader is `leader`, its foreground `foreground`.
fn listed(id: &str, argv: &[&str], leader: &str, foreground: &str) -> Terminal {
    serde_json::from_value(json!({
        "id": id, "argv": argv, "cwd": CWD, "pid": 100, "exited": null, "foreground": foreground,
        "cols": 80, "rows": 24, "scrollback_bytes": 10, "rss_kb": 1,
        "processes": [{ "pid": 100, "command": leader, "rss_kb": 1 }, { "pid": 101, "command": foreground, "rss_kb": 1 }],
    }))
    .unwrap()
}

fn shell(foreground: &str) -> Terminal {
    listed("shell", &[], "-zsh", foreground)
}

fn claude() -> Terminal {
    listed(
        "claude-1",
        &["env", "CLAUDE_CODE_PLUGIN_DIRS=/p", "claude"],
        "claude",
        "claude",
    )
}

fn replayed(lines: &[String]) -> Studio {
    Studio::replayed(lines)
}

/// Whether a daemon restart now ends nothing, as the window asks it: no terminal is `busy`.
fn idle(studio: &Studio, terms: &[Terminal]) -> bool {
    studio.revival(terms).iter().all(|r| r.busy.is_none())
}

fn started() -> Vec<String> {
    vec![event("SessionStart", json!({ "source": "startup" }))]
}

fn in_turn() -> Vec<String> {
    let mut lines = started();
    lines.push(event("UserPromptSubmit", json!({ "prompt": "Rename it." })));
    lines
}

fn stopped(tasks: Value) -> Vec<String> {
    let mut lines = in_turn();
    lines.push(event(
        "Stop",
        json!({ "background_tasks": tasks, "last_assistant_message": "Done." }),
    ));
    lines
}

#[test]
fn idle_when_the_shell_is_at_its_prompt_and_the_session_between_turns() {
    let studio = replayed(&stopped(json!([])));
    assert!(idle(&studio, &[shell("-zsh"), claude()]));
    // A session that started and has had no prompt yet is between turns too.
    assert!(idle(&replayed(&started()), &[shell("-zsh"), claude()]));
    // No terminal at all is idle.
    assert!(idle(&studio, &[]));
}

#[test]
fn a_program_in_a_shell_is_not_idle() {
    let studio = replayed(&stopped(json!([])));
    assert!(!idle(&studio, &[shell("cargo"), claude()]));
    let revival = studio.revival(&[shell("cargo")]);
    assert_eq!(revival[0].busy.as_deref(), Some("cargo"));
}

#[test]
fn a_session_in_a_turn_is_not_idle() {
    assert!(!idle(&replayed(&in_turn()), &[shell("-zsh"), claude()]));
}

#[test]
fn a_session_with_background_tasks_is_not_idle_until_a_stop_lists_none() {
    let tasks = json!([{ "id": "b1", "type": "local_bash", "status": "running" }]);
    let mut lines = stopped(tasks);
    assert!(!idle(&replayed(&lines), &[claude()]));
    let revival = replayed(&lines).revival(&[claude()]);
    assert_eq!(revival[0].busy.as_deref(), Some("background tasks"));
    // Its wake-up opens a turn, whose `Stop` lists no task.
    lines.push(event(
        "UserPromptSubmit",
        json!({ "prompt": "<agent-message from=\"b1\">done</agent-message>", "prompt_id": "p-2" }),
    ));
    lines.push(event(
        "Stop",
        json!({ "background_tasks": [], "prompt_id": "p-2" }),
    ));
    assert!(idle(&replayed(&lines), &[claude()]));
}

#[test]
fn an_exited_terminal_is_idle_and_not_revived() {
    let studio = replayed(&in_turn());
    let mut gone = claude();
    gone.exited = Some("code 0".into());
    assert!(idle(&studio, &[gone.clone()]));
    assert!(studio.revival(&[gone]).is_empty());
}

#[test]
fn a_claude_terminal_with_no_session_yet_is_busy() {
    let studio = replayed(&[]);
    assert!(!idle(&studio, &[claude()]));
}

#[test]
fn a_claude_terminal_comes_back_resumed_and_a_shell_as_a_shell() {
    // Even after the hangup ended the session: the daemon's own record of what ran says it was lost.
    let mut lines = stopped(json!([]));
    lines.push(event("SessionEnd", json!({ "reason": "other" })));
    let studio = replayed(&lines);
    let lost: Vec<Terminal> = serde_json::from_value(json!([
        { "id": "shell", "argv": [], "cwd": "/tmp" },
        { "id": "claude-1", "argv": ["env", "CLAUDE_CODE_PLUGIN_DIRS=/p", "claude"], "cwd": CWD },
    ]))
    .unwrap();
    let revival = studio.revival(&lost);
    let found: Vec<(&str, &str, Option<&str>)> = revival
        .iter()
        .map(|r| (r.id.as_str(), r.cwd.as_str(), r.session.as_deref()))
        .collect();
    assert_eq!(
        found,
        [("shell", "/tmp", None), ("claude-1", CWD, Some(SESSION))]
    );
}

#[test]
fn a_shell_that_ran_another_agent_comes_back_as_that_agent_and_any_other_as_a_shell() {
    // #561: the dead daemon's record keeps each shell's foreground program.
    let studio = replayed(&[]);
    let lost: Vec<Terminal> = serde_json::from_value(json!([
        { "id": "t-agy", "argv": [], "cwd": "/tmp", "foreground": "agy" },
        { "id": "t-vim", "argv": [], "cwd": "/tmp", "foreground": "vim" },
        { "id": "t-old", "argv": [], "cwd": "/tmp" },
        { "id": "t-again", "argv": ["agy", "--continue"], "cwd": "/tmp", "foreground": "agy" },
    ]))
    .unwrap();
    let argvs: Vec<Vec<String>> = studio.revival(&lost).into_iter().map(|r| r.argv).collect();
    let agy = vec!["agy".to_string(), "--continue".into()];
    assert_eq!(argvs, [agy.clone(), vec![], vec![], agy]);
}

#[test]
fn a_shell_whose_agent_shows_as_node_comes_back_by_its_7501_app() {
    // #561: a node-launched CLI's foreground is `node`; the record keeps the name it reports itself by.
    let studio = replayed(&[]);
    let lost: Vec<Terminal> = serde_json::from_value(json!([
        { "id": "t-codex", "argv": [], "cwd": "/tmp", "foreground": "node", "app": "codex" },
        { "id": "t-node", "argv": [], "cwd": "/tmp", "foreground": "node" },
    ]))
    .unwrap();
    let argvs: Vec<Vec<String>> = studio.revival(&lost).into_iter().map(|r| r.argv).collect();
    let codex: Vec<String> = ["codex", "resume", "--last"].map(String::from).into();
    assert_eq!(argvs, [codex, vec![]]);
}
