//! Seam 1 (danielrispler/Falinks#461, Testing Decisions) for the intention and step line (danielrispler/Falinks#472, decision 12): hook events
//! through the core, with transcript fixtures whose content is scrubbed (shape kept, text made up).

use studio_core::{Step, Studio};

const BASH: &str = include_str!("fixtures/bash-steps.jsonl");
const BASH_SESSION: &str = "e4a7c2f9-3b6d-4e1a-8c5f-2d9b7e0a6c13";

#[test]
fn a_bash_step_shows_its_description_and_keeps_the_full_command() {
    let studio = Studio::replayed(BASH.lines().take(8));

    let session = studio.session(BASH_SESSION).expect("the session");
    assert_eq!(
        session.agent.last_step,
        Some(Step {
            tool: "Bash".into(),
            detail: Some("Write the note".into()),
            command: Some(
                "cat > apps/studio/NOTES.md <<'EOF'\nThe fingerprint is taken live.\nEOF".into()
            ),
        })
    );
}

/// A made-up session: a turn whose agent writes a line, chains steps after it, ends with a reply,
/// then a second prompt. Lines 9 to 11 are an attachment, a skill's meta line and a helper's line,
/// none of which is the agent's step or text.
const CHAIN: &str = include_str!("fixtures/transcripts/chain.jsonl");
const SESSION: &str = "5b9e1d3a-7c2f-4a8e-b6d0-3f1a9c7e2b54";

/// A session whose transcript is at `path`, prompted and at its first step.
fn session_at(path: &std::path::Path) -> Studio {
    let path = path.display();
    Studio::replayed([
        format!(
            r#"{{"session_id":"{SESSION}","hook_event_name":"UserPromptSubmit","prompt":"(scrubbed)","transcript_path":"{path}","cwd":"/Users/danielrispler/work/Falinks","studio_received_ms":1793265000000}}"#
        ),
        format!(
            r#"{{"session_id":"{SESSION}","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":"cat probe.txt","description":"Show the probe"}},"tool_use_id":"toolu_1","transcript_path":"{path}","cwd":"/Users/danielrispler/work/Falinks","studio_received_ms":1793265001000}}"#
        ),
    ])
}

/// A transcript file of its own for test `name`, holding `lines`.
fn transcript(name: &str, lines: &str) -> std::path::PathBuf {
    let file = format!("studio-intention-{name}-{}.jsonl", std::process::id());
    let path = std::env::temp_dir().join(file);
    std::fs::write(&path, lines).expect("write the transcript");
    path
}

fn first(lines: usize) -> String {
    CHAIN
        .lines()
        .take(lines)
        .map(|l| format!("{l}\n"))
        .collect()
}

/// Follows the transcripts and returns the stderr lines written.
fn follow(studio: &mut Studio) -> Vec<String> {
    let mut logged = Vec::new();
    studio.follow_transcripts(|line| logged.push(line));
    logged
}

#[test]
fn the_intention_is_the_agents_latest_line_and_ages_with_each_step_after_it() {
    let path = transcript("chain", &first(8));
    let mut studio = session_at(&path);

    assert_eq!(follow(&mut studio), Vec::<String>::new());
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(
        transcript.intention.as_deref(),
        Some("Reading the probe first.")
    );
    assert_eq!(transcript.steps_ago, 2);

    std::fs::write(&path, first(13)).unwrap();
    follow(&mut studio);
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(
        transcript.intention.as_deref(),
        Some("Reading the probe first.")
    );
    assert_eq!(transcript.steps_ago, 3);

    std::fs::write(&path, first(14)).unwrap();
    follow(&mut studio);
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(
        transcript.intention.as_deref(),
        Some("The probe now reads right.")
    );
    assert_eq!(transcript.steps_ago, 0);
    assert_eq!(
        transcript.responses,
        [
            "Reading the probe first.",
            "Fixed the typo.\n\nThe probe now reads right."
        ]
    );
    assert_eq!(transcript.unavailable, None);
}

#[test]
fn a_new_prompt_starts_the_turn_with_no_intention() {
    let path = transcript("next-turn", CHAIN);
    let mut studio = session_at(&path);

    follow(&mut studio);

    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(transcript.intention, None);
    assert_eq!(transcript.responses, Vec::<String>::new());
    assert_eq!(transcript.unavailable, None);
}

/// The fallback: no intention and no responses, a reason, one stderr line however often the file is
/// read, and the step line as before.
fn assert_fallback(studio: &mut Studio, logged: Vec<String>) {
    let logged = [logged, follow(studio)].concat();
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(logged[0].contains(SESSION), "{logged:?}");
    let session = studio.session(SESSION).unwrap();
    assert_eq!(session.transcript.intention, None);
    assert_eq!(session.transcript.responses, Vec::<String>::new());
    assert!(session.transcript.unavailable.is_some());
    assert_eq!(
        session
            .agent
            .last_step
            .as_ref()
            .and_then(|s| s.detail.as_deref()),
        Some("Show the probe")
    );
}

#[test]
fn a_corrupted_line_falls_back_to_the_step_with_one_stderr_line() {
    let path = transcript(
        "corrupted",
        &(first(8) + "{\"type\":\"assistant\",\"mess\n"),
    );
    let mut studio = session_at(&path);

    let logged = follow(&mut studio);

    assert_fallback(&mut studio, logged);
}

#[test]
fn a_missing_field_falls_back_to_the_step() {
    let no_content = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\"}}\n";
    let path = transcript("no-content", &(first(8) + no_content));
    let mut studio = session_at(&path);

    let logged = follow(&mut studio);

    assert_fallback(&mut studio, logged);
}

#[test]
fn a_missing_file_falls_back_to_the_step_with_one_stderr_line() {
    let path = std::env::temp_dir().join("studio-intention-gone.jsonl");
    let mut studio = session_at(&path);

    let logged = follow(&mut studio);

    assert_fallback(&mut studio, logged);
}

/// An assistant line of the main agent, at model `model` and effort `effort`.
fn assistant_at(model: &str, effort: &str) -> String {
    format!(
        r#"{{"isSidechain":false,"type":"assistant","effort":"{effort}","message":{{"model":"{model}","role":"assistant","content":[{{"type":"text","text":"(scrubbed)"}}]}}}}"#
    ) + "\n"
}

#[test]
fn the_session_runs_at_its_latest_model_and_effort_across_turns() {
    // A helper's line and an API error's synthetic line name no model the session runs at.
    let helper = r#"{"isSidechain":true,"type":"assistant","effort":"low","message":{"model":"claude-haiku-5","role":"assistant","content":[{"type":"text","text":"(scrubbed)"}]}}"#;
    let error = r#"{"isSidechain":false,"type":"assistant","message":{"model":"<synthetic>","role":"assistant","content":[{"type":"text","text":"API Error"}]}}"#;
    let prompt = CHAIN.lines().nth(13).unwrap();
    let lines = first(2)
        + &assistant_at("claude-sonnet-5", "high")
        + &assistant_at("claude-opus-5-5", "medium")
        + &format!("{helper}\n{error}\n{prompt}\n");
    let path = transcript("model", &lines);
    let mut studio = session_at(&path);

    follow(&mut studio);
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(transcript.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(transcript.effort.as_deref(), Some("medium"));
}

#[test]
fn a_transcript_without_model_or_effort_has_neither() {
    let path = transcript("no-model", &first(2));
    let mut studio = session_at(&path);

    follow(&mut studio);
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(transcript.model, None);
    assert_eq!(transcript.effort, None);
}

/// An assistant line of the main agent writing `text`.
fn assistant_text(text: &str) -> String {
    let text = serde_json::to_string(text).unwrap();
    format!(
        r#"{{"isSidechain":false,"type":"assistant","message":{{"role":"assistant","content":[{{"type":"text","text":{text}}}]}}}}"#
    ) + "\n"
}

#[test]
fn a_turn_ends_on_its_final_replys_first_line_not_its_footer() {
    let reply = "Fixed the typo.\n\nThe probe now reads right.\n\nNot checked: the CI run.";
    let path = transcript("footer", &(first(13) + &assistant_text(reply)));
    let mut studio = session_at(&path);

    // While the turn runs, the intention is the latest response's last line.
    follow(&mut studio);
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(
        transcript.intention.as_deref(),
        Some("Not checked: the CI run.")
    );

    let path = path.display();
    studio.apply(&format!(
        r#"{{"session_id":"{SESSION}","hook_event_name":"Stop","last_assistant_message":"(scrubbed)","transcript_path":"{path}","cwd":"/Users/danielrispler/work/Falinks","studio_received_ms":1793265009000}}"#
    ));
    follow(&mut studio);
    let transcript = &studio.session(SESSION).unwrap().transcript;
    assert_eq!(transcript.intention.as_deref(), Some("Fixed the typo."));
    assert_eq!(
        transcript.intentions(),
        ["Reading the probe first.", "Fixed the typo."]
    );
}
