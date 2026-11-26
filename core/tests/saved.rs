//! Saved state (danielrispler/plotkeeper#134): a launch that takes the state a replay made, and
//! replays only the log's tail, gives what a full replay of the log gives.

use std::{fs, path::PathBuf};
use studio_core::Studio;

/// Every recorded log of the tests: Claude Code's, agy's, Codex's, Qwen Code's, Cursor's, Copilot's and OpenCode's, as the log file holds them, each line
/// ending in a newline.
fn fixtures() -> Vec<(String, Vec<String>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut found = Vec::new();
    for dir in [
        root.clone(),
        root.join("agy"),
        root.join("codex"),
        root.join("cursor"),
        root.join("qwen"),
        root.join("copilot"),
        root.join("opencode"),
    ] {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                let text = fs::read_to_string(&path).unwrap();
                let lines = text.lines().map(String::from).collect();
                found.push((path.display().to_string(), lines));
            }
        }
    }
    assert!(found.len() > 20, "the fixtures are found");
    found
}

fn file(lines: &[String]) -> String {
    lines.iter().map(|l| format!("{l}\n")).collect()
}

fn replayed(file: &str) -> Studio {
    let mut studio = Studio::default();
    file.split_inclusive('\n').for_each(|l| studio.replay(l));
    studio
}

/// A studio that took the state of the log's first `at` lines and replayed what came after.
fn resumed(lines: &[String], at: usize) -> Studio {
    let saved = replayed(&file(&lines[..at])).save();
    let log = file(lines);
    let mut studio = Studio::default();
    let from = studio.resume(&saved, log.len() as u64);
    assert_eq!(
        from as usize,
        file(&lines[..at]).len(),
        "it goes on from where the state stopped"
    );
    log[from as usize..]
        .split_inclusive('\n')
        .for_each(|l| studio.replay(l));
    studio
}

#[test]
fn a_launch_with_saved_state_gives_what_a_full_replay_gives() {
    for (name, lines) in fixtures() {
        let full = format!("{:#?}", replayed(&file(&lines)));
        for at in [1, lines.len() / 2, lines.len().saturating_sub(1)] {
            let restored = format!("{:#?}", resumed(&lines, at));
            assert!(
                restored == full,
                "{name}, restored after {at} of {} lines",
                lines.len()
            );
        }
    }
}

#[test]
fn saved_state_of_another_format_or_beside_a_shorter_log_is_ignored() {
    let (_, lines) = fixtures()
        .into_iter()
        .find(|(n, _)| n.ends_with("subagent-run.jsonl"))
        .unwrap();
    let log = file(&lines);
    let saved = replayed(&log).save();
    let again = |saved: &[u8], len: u64| {
        let mut studio = Studio::default();
        assert_eq!(studio.resume(saved, len), 0);
        // Nothing of it was taken, so a full replay from the start is the whole state.
        log.split_inclusive('\n').for_each(|l| studio.replay(l));
        assert_eq!(format!("{studio:#?}"), format!("{:#?}", replayed(&log)));
    };

    let newline = saved.iter().position(|b| *b == b'\n').unwrap();
    let mut other_format = b"studio-state 0".to_vec();
    other_format.extend(&saved[newline..]);
    again(&other_format, log.len() as u64);
    again(&saved, log.len() as u64 - 1);
    again(&saved[..saved.len() / 2], log.len() as u64);
    again(b"", log.len() as u64);
}

/// The saved state `saved` as the build before spec #659 wrote it: its sessions have no project folder.
fn without_projects(saved: &[u8]) -> Vec<u8> {
    let at = saved.iter().position(|b| *b == b'\n').unwrap() + 1;
    let mut state: serde_json::Value = serde_json::from_slice(&saved[at..]).unwrap();
    let sessions = state["sessions"].as_object_mut().unwrap();
    for session in sessions.values_mut() {
        session.as_object_mut().unwrap().remove("project").unwrap();
    }
    let mut old = saved[..at].to_vec();
    old.extend(serde_json::to_vec(&state).unwrap());
    old
}

/// A saved state of the build before the project folder (spec #659) gives each session the project it showed then: its
/// saved folder, a task's worktree's main checkout.
#[test]
fn saved_state_before_the_project_folder_gives_each_session_the_project_of_its_saved_folder() {
    let log = crate::common::TWO;
    let old = without_projects(&replayed(log).save());
    let mut studio = Studio::default();
    assert_eq!(
        studio.resume(&old, log.len() as u64),
        log.len() as u64,
        "it reads"
    );

    let projects: Vec<(&str, &str)> = studio
        .sessions()
        .iter()
        .map(|s| (s.cwd.as_str(), s.project.as_str()))
        .collect();
    assert_eq!(
        projects,
        [
            (
                "/Users/danielrispler/work/Falinks/.claude/worktrees/issue-322-terminals",
                "/Users/danielrispler/work/Falinks"
            ),
            (
                "/Users/danielrispler/work/Falinks",
                "/Users/danielrispler/work/Falinks"
            ),
        ]
    );
}
