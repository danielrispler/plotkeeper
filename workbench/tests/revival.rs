//! #561: an agent other than Claude Code comes back from revival with its own resume command.

use studio_workbench::{resume, resumes_latest};

fn argv(words: &[&str]) -> Option<Vec<String>> {
    Some(words.iter().map(|w| w.to_string()).collect())
}

#[test]
fn each_supported_agent_resumes_its_latest_conversation_without_an_id() {
    assert_eq!(resume("agy", None), argv(&["agy", "--continue"]));
    assert_eq!(resume("agent", None), argv(&["agent", "--resume"]));
    assert_eq!(
        resume("cursor-agent", None),
        argv(&["cursor-agent", "--resume"])
    );
    assert_eq!(resume("codex", None), argv(&["codex", "resume", "--last"]));
}

#[test]
fn each_supported_agent_resumes_a_known_conversation_by_its_id() {
    assert_eq!(
        resume("agy", Some("c-1")),
        argv(&["agy", "--conversation", "c-1"])
    );
    assert_eq!(
        resume("agent", Some("c-1")),
        argv(&["agent", "--resume", "c-1"])
    );
    assert_eq!(
        resume("codex", Some("c-1")),
        argv(&["codex", "resume", "c-1"])
    );
}

#[test]
fn an_unknown_program_has_no_resume_command_so_revives_as_a_plain_shell() {
    assert_eq!(resume("vim", None), None);
    assert_eq!(resume("zsh", Some("c-1")), None);
}

#[test]
fn a_command_that_resumes_the_latest_conversation_is_told_apart() {
    assert!(resumes_latest(&argv(&["agy", "--continue"]).unwrap()));
    assert!(!resumes_latest(
        &argv(&["agy", "--conversation", "c-1"]).unwrap()
    ));
    assert!(!resumes_latest(&argv(&["agy"]).unwrap()));
    assert!(!resumes_latest(&[]));
}
