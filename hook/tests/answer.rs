// Seam 4 (danielrispler/Falinks#461): the answer hook's contract. The binary gets a hook payload on stdin, a scratch `HOME`
// in which the test plays the studio (its watching file and its answer file), and is checked on
// stdout.
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use studio_hook::answer;

const SESSION: &str = "s-1";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn scratch(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("studio-answer-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&home);
    fs::create_dir_all(&home).unwrap();
    home
}

fn data(home: &Path) -> PathBuf {
    studio_hook::event_log(home).parent().unwrap().to_owned()
}

/// The studio watches `session`: its pid and a heartbeat `age_ms` old.
fn watch(home: &Path, session: &str, pid: u32, age_ms: u64) {
    let watching = json!({ "session_id": session, "pid": pid, "heartbeat_ms": now_ms() - age_ms });
    answer::write_private(&answer::watching_file(&data(home)), &watching).unwrap();
}

fn asked() -> Value {
    json!({
        "session_id": SESSION,
        "hook_event_name": "PreToolUse",
        "tool_name": "AskUserQuestion",
        "tool_use_id": "toolu_1",
        "tool_input": { "questions": [
            { "question": "Which framework?", "header": "Framework", "multiSelect": false,
              "options": [{ "label": "React", "description": "Component library" }, { "label": "Vue" }] },
            { "question": "Which checks?", "header": "Checks", "multiSelect": true,
              "options": [{ "label": "Lint" }, { "label": "Tests" }, { "label": "Docs" }] }
        ] }
    })
}

fn planned() -> Value {
    json!({
        "session_id": SESSION,
        "hook_event_name": "PreToolUse",
        "tool_name": "ExitPlanMode",
        "tool_use_id": "toolu_2",
        "tool_input": { "plan": "## Refactor auth\n1. Extract the token check", "planFilePath": "/tmp/plans/auth.md" }
    })
}

/// A permission request: it names no call id.
fn permission() -> Value {
    json!({
        "session_id": SESSION,
        "hook_event_name": "PermissionRequest",
        "tool_name": "Bash",
        "tool_input": { "command": "rm -rf node_modules", "description": "Remove node_modules" },
        "permission_suggestions": []
    })
}

/// Each kind of call the hook answers, with an answer from the card that decides it.
fn kinds() -> [(&'static str, Value, Value); 3] {
    [
        (
            "a question",
            asked(),
            json!({ "answers": { "Which framework?": "Vue" } }),
        ),
        ("a plan", planned(), json!({ "decision": "allow" })),
        ("a permission", permission(), json!({ "decision": "allow" })),
    ]
}

/// What the hook prints once the card answers `payload` with `answer`, its nonce added.
fn answered(name: &str, payload: &Value, answer: Value) -> String {
    let home = scratch(name);
    watch(&home, SESSION, std::process::id(), 0);
    let since = Instant::now();
    let child = start(&home, 10_000, payload);
    let mut answer = answer;
    answer["nonce"] = request(&home)["nonce"].clone();
    reply(&home, answer);
    let out = finish(child, since).0;
    let _ = fs::remove_dir_all(&home);
    out
}

fn decided(name: &str, payload: &Value, answer: Value) -> Value {
    serde_json::from_str(&answered(name, payload, answer)).unwrap()
}

fn start(home: &Path, timeout_ms: u64, payload: &Value) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_studio-answer"))
        .env("HOME", home)
        .env("STUDIO_ANSWER_TIMEOUT_MS", timeout_ms.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(payload.to_string().as_bytes()).unwrap();
    child
}

/// What the hook printed, and how long it took from `since`.
fn finish(child: Child, since: Instant) -> (String, Duration) {
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    (String::from_utf8(out.stdout).unwrap(), since.elapsed())
}

/// The request the hook leaves for the studio, once it appears.
fn request(home: &Path) -> Value {
    let path = answer::ask_file(&data(home), SESSION);
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if let Ok(text) = fs::read_to_string(&path) {
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            return serde_json::from_str(&text).unwrap();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no request appeared");
}

fn reply(home: &Path, answer: Value) {
    answer::write_private(&answer::answer_file(&data(home), SESSION), &answer).unwrap();
}

#[test]
fn an_answer_allows_the_call_with_the_questions_and_each_chosen_label() {
    let home = scratch("answer");
    watch(&home, SESSION, std::process::id(), 0);
    let since = Instant::now();
    let child = start(&home, 10_000, &asked());
    let ask = request(&home);
    assert_eq!(ask["tool_input"], asked()["tool_input"]);
    let answers = json!({ "Which framework?": "React", "Which checks?": ["Lint", "Tests"] });
    reply(&home, json!({ "nonce": ask["nonce"], "answers": answers }));
    let (out, _) = finish(child, since);
    let out: Value = serde_json::from_str(&out).unwrap();
    let mut input = asked()["tool_input"].clone();
    input["answers"] = json!({ "Which framework?": "React", "Which checks?": "Lint, Tests" });
    assert_eq!(
        out,
        json!({ "hookSpecificOutput": {
            "hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": input } })
    );
    // The request is gone once answered.
    assert!(!answer::ask_file(&data(&home), SESSION).exists());
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_approved_plan_allows_it_with_its_input_and_a_rejected_one_denies_it() {
    let approved = decided("plan-allow", &planned(), json!({ "decision": "allow" }));
    assert_eq!(
        approved,
        json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse",
            "permissionDecision": "allow", "updatedInput": planned()["tool_input"] } })
    );
    let rejected = decided("plan-deny", &planned(), json!({ "decision": "deny" }));
    let out = &rejected["hookSpecificOutput"];
    assert_eq!(out["hookEventName"], "PreToolUse");
    assert_eq!(out["permissionDecision"], "deny");
    let reason = out["permissionDecisionReason"].as_str().unwrap();
    assert!(reason.contains("studio card"), "{reason}");
    assert_eq!(out.get("updatedInput"), None);
}

#[test]
fn an_allowed_and_a_denied_permission_each_give_their_decision() {
    let allowed = decided("perm-allow", &permission(), json!({ "decision": "allow" }));
    assert_eq!(
        allowed,
        json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest",
            "decision": { "behavior": "allow" } } })
    );
    let denied = decided("perm-deny", &permission(), json!({ "decision": "deny" }));
    let decision = &denied["hookSpecificOutput"]["decision"];
    assert_eq!(
        denied["hookSpecificOutput"]["hookEventName"],
        "PermissionRequest"
    );
    assert_eq!(decision["behavior"], "deny");
    let message = decision["message"].as_str().unwrap();
    assert!(message.contains("studio card"), "{message}");
}

#[test]
fn a_question_takes_no_decision_and_a_plan_or_permission_no_answers() {
    let cases = [
        (
            "plan answers",
            planned(),
            json!({ "answers": { "Which framework?": "Vue" } }),
        ),
        ("question decision", asked(), json!({ "decision": "allow" })),
        ("odd decision", permission(), json!({ "decision": "yes" })),
    ];
    for (case, payload, answer) in cases {
        assert_eq!(answered("mismatch", &payload, answer), "", "{case}");
    }
}

#[test]
fn a_card_answer_goes_in_the_event_log_and_a_terminal_one_does_not() {
    let home = scratch("logged");
    watch(&home, SESSION, std::process::id(), 0);
    let log = studio_hook::event_log(&home);
    for payload in [permission(), planned(), asked()] {
        let since = Instant::now();
        let child = start(&home, 10_000, &payload);
        reply(&home, json!({ "nonce": request(&home)["nonce"] }));
        finish(child, since);
    }
    assert!(!log.exists(), "a terminal answer was logged");

    let since = Instant::now();
    let child = start(&home, 10_000, &permission());
    let nonce = request(&home)["nonce"].clone();
    reply(&home, json!({ "nonce": nonce, "decision": "deny" }));
    finish(child, since);
    let text = fs::read_to_string(&log).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 1);
    let line = &lines[0];
    assert_eq!(line["hook_event_name"], "StudioAnswer");
    assert_eq!(line["studio_asked_by"], "PermissionRequest");
    assert_eq!(line["session_id"], SESSION);
    assert_eq!(line["tool_name"], "Bash");
    assert_eq!(line["tool_input"], permission()["tool_input"]);
    assert_eq!(line["studio_answer"], json!({ "decision": "deny" }));
    assert!(line["studio_received_ms"].as_u64().unwrap() > 0);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn nobody_watching_or_a_stale_watching_file_returns_at_once_with_no_decision() {
    let home = scratch("stale");
    let gone = Command::new("true").spawn().unwrap();
    let dead = gone.id();
    gone.wait_with_output().unwrap();
    let me = std::process::id();
    // Each case's watching file: the session watched, the pid and the heartbeat's age.
    type Watching<'a> = Option<(&'a str, u32, u64)>;
    let cases: [(&str, Watching); 4] = [
        ("no watching file", None),
        ("another session watched", Some(("s-2", me, 0))),
        ("a dead pid", Some((SESSION, dead, 0))),
        ("an old heartbeat", Some((SESSION, me, 6_000))),
    ];
    for (case, watching) in cases {
        let _ = fs::remove_file(answer::watching_file(&data(&home)));
        if let Some((session, pid, age)) = watching {
            watch(&home, session, pid, age);
        }
        for (kind, payload, _) in kinds() {
            let since = Instant::now();
            let (out, took) = finish(start(&home, 10_000, &payload), since);
            assert_eq!(out, "", "{kind}, {case}");
            assert!(
                took < Duration::from_secs(2),
                "{kind}, {case}: took {took:?}"
            );
            let ask = answer::ask_file(&data(&home), SESSION);
            assert!(!ask.exists(), "{kind}, {case}");
        }
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_timeout_or_answer_in_the_terminal_returns_no_decision() {
    let home = scratch("terminal");
    for (kind, payload, _) in kinds() {
        watch(&home, SESSION, std::process::id(), 0);
        let since = Instant::now();
        let (out, took) = finish(start(&home, 600, &payload), since);
        assert_eq!(out, "", "{kind}");
        assert!(
            took >= Duration::from_millis(600),
            "{kind}: returned before its timeout: {took:?}"
        );

        let since = Instant::now();
        let child = start(&home, 10_000, &payload);
        let ask = request(&home);
        assert_eq!(ask["hook_event_name"], payload["hook_event_name"], "{kind}");
        assert_eq!(ask["tool_input"], payload["tool_input"], "{kind}");
        reply(&home, json!({ "nonce": ask["nonce"] }));
        let (out, took) = finish(child, since);
        assert_eq!(out, "", "{kind}");
        assert!(took < Duration::from_secs(5), "{kind}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_answer_with_a_wrong_nonce_or_open_to_others_is_refused() {
    let home = scratch("nonce");
    for (kind, payload, answer) in kinds() {
        // A fresh heartbeat, as the studio's every second.
        watch(&home, SESSION, std::process::id(), 0);
        let since = Instant::now();
        let child = start(&home, 1_500, &payload);
        let ask = request(&home);
        assert_ne!(ask["nonce"], "", "{kind}");
        let mut wrong = answer.clone();
        wrong["nonce"] = "0123456789abcdef0123456789abcdef".into();
        reply(&home, wrong);
        assert_eq!(finish(child, since).0, "", "{kind}");

        // The right nonce in a file others may write is refused too.
        let since = Instant::now();
        let child = start(&home, 1_500, &payload);
        let mut open = answer;
        open["nonce"] = request(&home)["nonce"].clone();
        let path = answer::answer_file(&data(&home), SESSION);
        let temp = path.with_extension("open");
        fs::write(&temp, open.to_string()).unwrap();
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)).unwrap();
        fs::rename(&temp, &path).unwrap();
        assert_eq!(finish(child, since).0, "", "{kind}");

        // So is a link to a private file, which could point anywhere.
        let since = Instant::now();
        let child = start(&home, 1_500, &payload);
        let mut linked = answer_with(&payload);
        linked["nonce"] = request(&home)["nonce"].clone();
        let target = path.with_extension("target");
        answer::write_private(&target, &linked).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert_eq!(finish(child, since).0, "", "{kind}");
        let _ = fs::remove_file(&path);
    }
    let _ = fs::remove_dir_all(&home);
}

/// The card's answer that decides `payload`, as `kinds` gives it.
fn answer_with(payload: &Value) -> Value {
    let (_, _, answer) = kinds().into_iter().find(|(_, p, _)| p == payload).unwrap();
    answer
}

#[test]
fn a_request_names_the_agent_that_asks_so_the_studio_can_tell_whose_card_it_is() {
    let home = scratch("agent");
    let mut subagent = asked();
    subagent["agent_id"] = "a-7".into();
    for (payload, agent) in [(subagent, json!("a-7")), (asked(), Value::Null)] {
        watch(&home, SESSION, std::process::id(), 0);
        let since = Instant::now();
        let child = start(&home, 1_000, &payload);
        assert_eq!(request(&home)["agent_id"], agent);
        finish(child, since);
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn other_calls_pass_through_at_once() {
    let home = scratch("other");
    watch(&home, SESSION, std::process::id(), 0);
    let mut bash = asked();
    bash["tool_name"] = "Bash".into();
    // A question or a plan asks at its PreToolUse, so its permission request needs no second wait.
    let mut planning = permission();
    planning["tool_name"] = "ExitPlanMode".into();
    let mut notice = permission();
    notice["hook_event_name"] = "Notification".into();
    for payload in [bash, planning, notice] {
        let since = Instant::now();
        let (out, took) = finish(start(&home, 10_000, &payload), since);
        assert_eq!(out, "", "{payload}");
        assert!(took < Duration::from_secs(2), "{payload}");
    }
    let _ = fs::remove_dir_all(&home);
}
