//! Drift in Claude Code (danielrispler/Falinks#369, decisions 7, 9 and 23): its version, read from a session's transcript
//! and kept beside the event log, and the wake-ups the studio does not recognise, as `weekly` reports
//! them.

use serde_json::json;
use std::{fs, path::Path};
use studio_core::Studio;
use studio_understanding::{AgyVersions, Versions, drift, dropped_events};

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
    let text: Vec<String> = lines.iter().map(ToString::to_string).collect();
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
fn two_sessions_alternating_between_two_versions_seen_give_no_notice_and_no_line() {
    let data = tempfile::tempdir().unwrap();
    let versions = Versions::beside(&data.path().join("events.jsonl"));
    let old = transcript(data.path(), "a.jsonl", "2.1.309");
    let new = transcript(data.path(), "b.jsonl", "2.1.310");
    versions.check(&old, NOW - 3 * DAY).unwrap();
    versions.check(&new, NOW - 2 * DAY).unwrap();

    // Two terminals, one on each version, stop in turn.
    for (at, transcript) in [&old, &new, &old, &new].into_iter().enumerate() {
        assert_eq!(
            versions.check(transcript, NOW - DAY + at as u64).unwrap(),
            None
        );
    }

    assert_eq!(
        drift(&Studio::default(), &versions, NOW).to_string(),
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
    let studio = Studio::replayed(&log);
    let data = tempfile::tempdir().unwrap();
    let versions = Versions::beside(&data.path().join("events.jsonl"));

    assert_eq!(
        drift(&studio, &versions, NOW).to_string(),
        "unrecognised wake-ups: 1\n"
    );
}

#[test]
fn weekly_counts_the_events_the_hook_dropped_in_the_last_7_days() {
    let data = tempfile::tempdir().unwrap();
    let log = data.path().join("events.jsonl");
    assert_eq!(dropped_events(&log, NOW), 0);

    let day = |days| NOW - days * DAY;
    let lines = [
        format!("{} PostToolUse t-1", day(8)),
        format!("{} Stop t-1", day(2)),
        format!("{} PreToolUse -", day(1)),
    ];
    fs::write(studio_hook::drops_file(&log), lines.join("\n") + "\n").unwrap();

    assert_eq!(dropped_events(&log, NOW), 2);
}

#[test]
fn weekly_counts_what_the_agy_adapter_did_not_recognise() {
    let at = NOW - DAY;
    let line = |event: &str| {
        json!({ "conversationId": "a1", "hookEventName": event, "studio_received_ms": at })
            .to_string()
    };
    let studio = Studio::replayed([line("Teleport"), line("Teleport")]);
    let data = tempfile::tempdir().unwrap();
    let versions = Versions::beside(&data.path().join("events.jsonl"));

    assert_eq!(
        drift(&studio, &versions, NOW).to_string(),
        "unrecognised wake-ups: 0\nunknown agy hook events: 2\n"
    );
}

#[test]
fn an_agy_version_equal_to_the_fixtures_gives_no_notice_and_a_different_one_gives_one() {
    let data = tempfile::tempdir().unwrap();
    let log = data.path().join("events.jsonl");
    let versions = AgyVersions::beside(&log);

    assert_eq!(versions.check("1.5.9", "1.5.9\n", NOW - DAY).unwrap(), None);
    let notice = versions.check("1.6.0", "1.5.9\n", NOW).unwrap();
    assert_eq!(
        notice.as_deref(),
        Some("agy is 1.6.0, the fixture was recorded with 1.5.9: re-record the agy fixture")
    );
    // Seen again, also after a restart: no second notice, no second line.
    let reopened = AgyVersions::beside(&log);
    assert_eq!(reopened.check("1.6.0", "1.5.9\n", NOW).unwrap(), None);
    let kept = fs::read_to_string(data.path().join("agy-versions.txt")).unwrap();
    assert_eq!(kept, format!("{} 1.5.9\n{NOW} 1.6.0\n", NOW - DAY));
}
