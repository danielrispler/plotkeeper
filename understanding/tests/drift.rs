//! Drift in Claude Code (#369, decisions 7, 9 and 23): its version, read from a session's transcript
//! and kept beside the event log, and the wake-ups the studio does not recognise, as `weekly` reports
//! them.

use serde_json::json;
use std::{fs, path::Path};
use studio_core::Studio;
use studio_understanding::{Versions, drift};

/// 2025-10-29, 13:20 UTC.
const NOW: u64 = 1_761_744_000_000;
const DAY: u64 = 86_400_000;

/// A transcript whose lines carry Claude Code's `version`, after a first line that does not.
fn transcript(folder: &Path, name: &str, version: &str) -> std::path::PathBuf {
    let path = folder.join(name);
    let lines = [
        json!({ "type": "file-history-snapshot", "messageId": "m1" }),
        json!({ "type": "user", "version": version, "sessionId": "s1" }),
        json!({ "type": "assistant", "version": version, "sessionId": "s1" }),
    ];
    let text: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    fs::write(&path, text.join("\n") + "\n").unwrap();
    path
}

#[test]
fn a_new_version_gives_one_notice_and_one_weekly_line_and_the_same_version_again_nothing() {
    let data = tempfile::tempdir().unwrap();
    let versions = Versions::beside(&data.path().join("events.jsonl"));
    let old = transcript(data.path(), "a.jsonl", "2.1.309");
    let new = transcript(data.path(), "b.jsonl", "2.1.310");
    let again = transcript(data.path(), "c.jsonl", "2.1.310");

    // The first version seen is kept, with nothing to compare it with.
    assert_eq!(versions.check(&old, NOW - 3 * DAY).unwrap(), None);
    let notice = versions.check(&new, NOW - 2 * DAY).unwrap();
    assert_eq!(versions.check(&again, NOW - DAY).unwrap(), None);
    // A restart reads the kept versions back.
    let reopened = Versions::beside(&data.path().join("events.jsonl"));
    assert_eq!(reopened.check(&again, NOW).unwrap(), None);

    assert_eq!(
        notice.as_deref(),
        Some(
            "Claude Code changed from 2.1.309 to 2.1.310: re-record the subagent fixture \
             (core/tests/record-subagent-run.sh)"
        )
    );
    assert_eq!(
        drift(&Studio::default(), &reopened, NOW).to_string(),
        "unrecognised wake-ups: 0\n\
         Claude Code changed from 2.1.309 to 2.1.310 on 2025-10-27\n"
    );
}

#[test]
fn the_version_read_touches_only_a_transcript_never_a_credential_file() {
    let data = tempfile::tempdir().unwrap();
    let versions = Versions::beside(&data.path().join("events.jsonl"));
    transcript(data.path(), "a.jsonl", "2.1.309");
    versions
        .check(&data.path().join("a.jsonl"), NOW - DAY)
        .unwrap();
    // A payload whose `transcript_path` names a credential file, or Claude Code's config.
    let credentials = data.path().join(".credentials.json");
    let config = data.path().join(".claude.json");
    for path in [&credentials, &config] {
        let secret = json!({ "version": "9.9.9", "accessToken": "sk-ant-oat01-…" });
        fs::write(path, secret.to_string()).unwrap();
    }

    for path in [&credentials, &config] {
        assert_eq!(versions.check(path, NOW).unwrap(), None);
    }
    assert_eq!(
        drift(&Studio::default(), &versions, NOW).to_string(),
        "unrecognised wake-ups: 0\n"
    );
}

/// An event of session `s1`, received `days` ago.
fn event(days: u64, fields: serde_json::Value) -> String {
    let mut event = json!({
        "session_id": "5b9e3d71-0c2a-4f8e-b6d4-8a1c3e5f7092",
        "cwd": "/Users/danielrispler/work/Falinks",
        "studio_received_ms": NOW - days * DAY,
    });
    event
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    event.to_string()
}

#[test]
fn weekly_counts_the_last_7_days_unrecognised_wake_ups_and_no_other_prompt() {
    let tasks = json!([{ "id": "bx7k2", "status": "running", "type": "shell" }]);
    let prompt = |days, prompt: &str| {
        event(
            days,
            json!({ "hook_event_name": "UserPromptSubmit", "prompt": prompt }),
        )
    };
    let log = [
        event(
            9,
            json!({ "hook_event_name": "Stop", "background_tasks": tasks }),
        ),
        // Last week's.
        prompt(8, "<task-ref>bx7k2</task-ref>"),
        prompt(2, "<task-ref>bx7k2</task-ref>"),
        // A wake-up, and Daniel's prompt naming the task.
        prompt(
            1,
            "<task-notification><task-id>bx7k2</task-id></task-notification>",
        ),
        prompt(1, "what did bx7k2 print?"),
    ];
    let mut studio = Studio::default();
    for line in &log {
        studio.replay(line);
    }
    let data = tempfile::tempdir().unwrap();
    let versions = Versions::beside(&data.path().join("events.jsonl"));

    assert_eq!(
        drift(&studio, &versions, NOW).to_string(),
        "unrecognised wake-ups: 1\n"
    );
}
