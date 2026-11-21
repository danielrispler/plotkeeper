// Seam 4 (danielrispler/Falinks#461): the answer hook's contract. The binary gets a hook payload on stdin, a scratch `HOME`
// in which the test plays the studio (its watching file and its answer file), and is checked on
// stdout.
use crate::common::{feed, scratch as scratch_home};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use studio_hook::answer;

const SESSION: &str = "s-1";

/// How long a hook waits for an answer in a test that answers it. Long, so a test that has the hook
/// return "at once" can tell that from waiting out the timeout however slowly a loaded machine
/// starts a process: it checks the hook came back in under `PROMPTLY`, half of it.
const WAIT_MS: u64 = 60_000;
const PROMPTLY: Duration = Duration::from_millis(WAIT_MS / 2);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn scratch(name: &str) -> PathBuf {
    scratch_home(&format!("answer-{name}"))
}

fn data(home: &Path) -> PathBuf {
    studio_hook::event_log(home).parent().unwrap().to_owned()
}

/// The studio watches `session`: its pid and a heartbeat `age_ms` old. An age of 0 is a heartbeat
/// that stays fresh for the whole test, however slowly a loaded machine runs it: a test can take
/// longer than the staleness limit, and it is not testing that limit.
fn watch(home: &Path, session: &str, pid: u32, age_ms: u64) {
    let heartbeat = if age_ms == 0 {
        now_ms() + 600_000
    } else {
        now_ms() - age_ms
    };
    let watching = json!({ "session_id": session, "pid": pid, "heartbeat_ms": heartbeat });
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
    let child = start(&home, WAIT_MS, payload);
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
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_studio-answer"));
    cmd.env("HOME", home)
        .env("STUDIO_ANSWER_TIMEOUT_MS", timeout_ms.to_string())
        .stdout(Stdio::piped());
    feed(cmd, payload)
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
    let until = Instant::now() + PROMPTLY;
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
    let child = start(&home, WAIT_MS, &asked());
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
        let child = start(&home, WAIT_MS, &payload);
        reply(&home, json!({ "nonce": request(&home)["nonce"] }));
        finish(child, since);
    }
    assert!(!log.exists(), "a terminal answer was logged");

    let since = Instant::now();
    let child = start(&home, WAIT_MS, &permission());
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
            let (out, took) = finish(start(&home, WAIT_MS, &payload), since);
            assert_eq!(out, "", "{kind}, {case}");
            assert!(took < PROMPTLY, "{kind}, {case}: took {took:?}");
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
        let (out, took) = finish(start(&home, 150, &payload), since);
        assert_eq!(out, "", "{kind}");
        assert!(
            took >= Duration::from_millis(150),
            "{kind}: returned before its timeout: {took:?}"
        );

        let since = Instant::now();
        let child = start(&home, WAIT_MS, &payload);
        let ask = request(&home);
        assert_eq!(ask["hook_event_name"], payload["hook_event_name"], "{kind}");
        assert_eq!(ask["tool_input"], payload["tool_input"], "{kind}");
        reply(&home, json!({ "nonce": ask["nonce"] }));
        let (out, took) = finish(child, since);
        assert_eq!(out, "", "{kind}");
        assert!(took < PROMPTLY, "{kind}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn an_answer_with_a_wrong_nonce_or_open_to_others_is_refused() {
    let home = scratch("nonce");
    let path = answer::answer_file(&data(&home), SESSION);
    for (kind, payload, answer) in kinds() {
        // A fresh heartbeat, as the studio's every second.
        watch(&home, SESSION, std::process::id(), 0);
        let since = Instant::now();
        let child = start(&home, WAIT_MS, &payload);
        let ask = request(&home);
        assert_ne!(ask["nonce"], "", "{kind}");
        let mut wrong = answer.clone();
        wrong["nonce"] = "0123456789abcdef0123456789abcdef".into();
        reply(&home, wrong);
        // The hook removes each answer it reads, accepted or not, so a refused one is gone while the
        // hook still waits; a wrongly accepted one would end the wait with a decision.
        removed(&path, kind);

        // The right nonce in a file others may write is refused too.
        let mut open = answer;
        open["nonce"] = ask["nonce"].clone();
        let temp = path.with_extension("open");
        fs::write(&temp, open.to_string()).unwrap();
        fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)).unwrap();
        fs::rename(&temp, &path).unwrap();
        removed(&path, kind);

        // So is a link to a private file, which could point anywhere.
        let mut linked = answer_with(&payload);
        linked["nonce"] = ask["nonce"].clone();
        let target = path.with_extension("target");
        answer::write_private(&target, &linked).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        removed(&path, kind);

        // The hook is still waiting; a nonce-only answer ends it with no decision.
        reply(&home, json!({ "nonce": ask["nonce"] }));
        assert_eq!(finish(child, since).0, "", "{kind}");
    }
    let _ = fs::remove_dir_all(&home);
}

/// Waits until the hook has removed the answer file `path`, a link included.
fn removed(path: &Path, kind: &str) {
    let until = Instant::now() + PROMPTLY;
    while path.symlink_metadata().is_ok() {
        assert!(Instant::now() < until, "{kind}: the answer was not removed");
        std::thread::sleep(Duration::from_millis(20));
    }
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
        let child = start(&home, WAIT_MS, &payload);
        let ask = request(&home);
        assert_eq!(ask["agent_id"], agent);
        reply(&home, json!({ "nonce": ask["nonce"] }));
        assert_eq!(finish(child, since).0, "");
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
        let (out, took) = finish(start(&home, WAIT_MS, &payload), since);
        assert_eq!(out, "", "{payload}");
        assert!(took < PROMPTLY, "{payload}");
    }
    let _ = fs::remove_dir_all(&home);
}

// Blocked commands (danielrispler/plotkeeper#170, #171).

const COMMAND: &str = "claude -p \"summarise the repo\" > /tmp/out.txt";
const CWD: &str = "/work/repo";

/// What auto mode's `PermissionDenied` hook gets for a blocked Bash command.
fn denied() -> Value {
    json!({
        "session_id": SESSION,
        "hook_event_name": "PermissionDenied",
        "tool_name": "Bash",
        "tool_use_id": "toolu_9",
        "tool_input": { "command": COMMAND, "description": "Summarise" },
        "cwd": CWD,
        "reason": "Nested agent runs are blocked.",
        "agent_id": "a-7"
    })
}

/// The retry's `PreToolUse` and `PermissionRequest` for `command` in `cwd`.
fn retried(event: &str, command: &str, cwd: &str, session: &str) -> Value {
    json!({
        "session_id": session, "hook_event_name": event, "tool_name": "Bash",
        "tool_use_id": "toolu_10", "tool_input": { "command": command }, "cwd": cwd
    })
}

/// The blocked-command request files the hook left, parsed.
fn blocked_requests(home: &Path) -> Vec<Value> {
    let dir = answer::blocked_dir(&data(home), SESSION);
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut found: Vec<Value> = entries
        .filter_map(|e| e.ok())
        .filter(|e| !e.file_name().to_string_lossy().contains(".answer."))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str(&t).ok())
        .collect();
    found.sort_by_key(|r| r["asked_ms"].as_u64());
    found
}

/// The one blocked-command request, once it appears.
fn blocked_request(home: &Path) -> Value {
    let until = Instant::now() + PROMPTLY;
    while Instant::now() < until {
        if let Some(r) = blocked_requests(home).into_iter().next() {
            return r;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("no blocked-command request appeared");
}

fn blocked_reply(home: &Path, nonce: &Value, decision: &str) {
    let path = answer::blocked_dir(&data(home), SESSION)
        .join(format!("{}.answer.json", nonce.as_str().unwrap()));
    answer::write_private(&path, &json!({ "nonce": nonce, "decision": decision })).unwrap();
}

#[test]
fn allowing_a_blocked_command_from_the_card_tells_the_agent_to_retry() {
    let home = scratch("blocked-allow");
    // Any session may be the one the studio shows.
    watch(&home, "s-other", std::process::id(), 0);
    let since = Instant::now();
    let child = start(&home, WAIT_MS, &denied());
    let request = blocked_request(&home);
    for (field, expected) in [
        ("tool_use_id", json!("toolu_9")),
        ("cwd", json!(CWD)),
        ("reason", json!("Nested agent runs are blocked.")),
        ("agent_id", json!("a-7")),
        ("hook_event_name", json!("PermissionDenied")),
    ] {
        assert_eq!(request[field], expected, "{field}");
    }
    assert_eq!(request["tool_input"]["command"], COMMAND);
    // The hook's pid tells the studio whether the hook still waits.
    assert!(request["pid"].as_u64().is_some_and(|pid| pid > 0));
    assert_ne!(request["nonce"], "");
    blocked_reply(&home, &request["nonce"], "allow");
    let out: Value = serde_json::from_str(&finish(child, since).0).unwrap();
    assert_eq!(
        out,
        json!({ "hookSpecificOutput": { "hookEventName": "PermissionDenied", "retry": true } })
    );
    // Answered, so the request is gone and the answer is in the event log.
    assert_eq!(blocked_requests(&home), Vec::<Value>::new());
    let log = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    let line: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(line["hook_event_name"], "StudioAnswer");
    assert_eq!(line["studio_asked_by"], "PermissionDenied");
    assert_eq!(line["studio_answer"], json!({ "decision": "allow" }));
    assert_eq!(line["tool_input"]["command"], COMMAND);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn declining_a_blocked_command_gives_no_decision_and_is_logged() {
    let home = scratch("blocked-decline");
    watch(&home, SESSION, std::process::id(), 0);
    let since = Instant::now();
    let child = start(&home, WAIT_MS, &denied());
    blocked_reply(&home, &blocked_request(&home)["nonce"], "deny");
    let (out, took) = finish(child, since);
    assert_eq!(out, "");
    assert!(took < PROMPTLY);
    assert_eq!(blocked_requests(&home), Vec::<Value>::new());
    let log = fs::read_to_string(studio_hook::event_log(&home)).unwrap();
    let line: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(line["studio_answer"], json!({ "decision": "deny" }));
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_block_nobody_watches_returns_at_once_and_leaves_its_request_for_the_card() {
    let home = scratch("blocked-away");
    let gone = Command::new("true").spawn().unwrap();
    let dead = gone.id();
    gone.wait_with_output().unwrap();
    let me = std::process::id();
    let cases: [(&str, Option<(u32, u64)>); 3] = [
        ("no watching file", None),
        ("a dead pid", Some((dead, 0))),
        ("an old heartbeat", Some((me, 6_000))),
    ];
    for (case, watching) in cases {
        let _ = fs::remove_file(answer::watching_file(&data(&home)));
        if let Some((pid, age)) = watching {
            watch(&home, SESSION, pid, age);
        }
        let since = Instant::now();
        let (out, took) = finish(start(&home, WAIT_MS, &denied()), since);
        assert_eq!(out, "", "{case}");
        assert!(took < PROMPTLY, "{case}");
        let requests = blocked_requests(&home);
        assert_eq!(requests.len(), 1, "{case}");
        assert_eq!(requests[0]["tool_input"]["command"], COMMAND, "{case}");
        let _ = fs::remove_dir_all(answer::blocked_dir(&data(&home), SESSION));
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_block_waits_for_the_card_while_no_session_is_selected() {
    let home = scratch("blocked-none");
    let heartbeat = now_ms() + 600_000;
    let none = json!({ "session_id": null, "pid": std::process::id(), "heartbeat_ms": heartbeat });
    answer::write_private(&answer::watching_file(&data(&home)), &none).unwrap();
    let since = Instant::now();
    let child = start(&home, WAIT_MS, &denied());
    blocked_reply(&home, &blocked_request(&home)["nonce"], "allow");
    let out: Value = serde_json::from_str(&finish(child, since).0).unwrap();
    assert_eq!(out["hookSpecificOutput"]["retry"], true);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_decision_that_could_not_be_logged_keeps_the_request_so_the_card_keeps_its_buttons() {
    let home = scratch("blocked-unlogged");
    watch(&home, SESSION, std::process::id(), 0);
    // The log cannot be appended to: a directory stands where the file should be.
    fs::create_dir_all(studio_hook::event_log(&home)).unwrap();
    let since = Instant::now();
    let child = start(&home, WAIT_MS, &denied());
    blocked_reply(&home, &blocked_request(&home)["nonce"], "allow");
    let out: Value = serde_json::from_str(&finish(child, since).0).unwrap();
    assert_eq!(out["hookSpecificOutput"]["retry"], true);
    assert_eq!(blocked_requests(&home).len(), 1);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_blocked_command_waits_600_seconds_and_every_other_call_120() {
    assert_eq!(answer::wait_for(&denied()), Duration::from_secs(600));
    assert_eq!(answer::wait_for(&asked()), Duration::from_secs(120));
}

#[test]
fn a_block_nobody_answers_by_the_timeout_returns_no_decision_and_keeps_its_request() {
    let home = scratch("blocked-timeout");
    watch(&home, SESSION, std::process::id(), 0);
    let since = Instant::now();
    let (out, took) = finish(start(&home, 300, &denied()), since);
    assert_eq!(out, "");
    assert!(took < PROMPTLY);
    assert_eq!(blocked_requests(&home).len(), 1);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn other_denials_pass_through_at_once() {
    let home = scratch("blocked-other");
    watch(&home, SESSION, std::process::id(), 0);
    let mut web = denied();
    web["tool_name"] = "WebFetch".into();
    let since = Instant::now();
    let (out, took) = finish(start(&home, WAIT_MS, &web), since);
    assert_eq!(out, "");
    assert!(took < PROMPTLY);
    assert_eq!(blocked_requests(&home), Vec::<Value>::new());
    let _ = fs::remove_dir_all(&home);
}

/// What the retry's two hooks print, with a pass for `COMMAND` in `CWD` granted at `granted_ms`,
/// when the retry is `command` in `cwd` of `session`. The permission request comes with a claim
/// first unless `claim` is false.
fn retry(
    name: &str,
    granted_ms: u64,
    command: &str,
    cwd: &str,
    session: &str,
    claim: bool,
) -> (String, String) {
    let home = scratch(name);
    // The studio shows another session, so an uncovered permission request is not held for its card.
    watch(&home, "s-other", std::process::id(), 0);
    answer::write_pass(&data(&home), SESSION, COMMAND, CWD, granted_ms).unwrap();
    let run = |payload: Value| {
        let since = Instant::now();
        let out = finish(start(&home, WAIT_MS, &payload), since);
        assert!(out.1 < PROMPTLY);
        out.0
    };
    let claimed = if claim {
        run(retried("PreToolUse", command, cwd, session))
    } else {
        String::new()
    };
    let permitted = run(retried("PermissionRequest", command, cwd, session));
    let _ = fs::remove_dir_all(&home);
    (claimed, permitted)
}

#[test]
fn a_retry_of_the_exact_command_is_claimed_asked_and_allowed_once() {
    let home = scratch("pass-flow");
    // The studio shows another session, so an uncovered permission request is not held for its card.
    watch(&home, "s-other", std::process::id(), 0);
    answer::write_pass(&data(&home), SESSION, COMMAND, CWD, now_ms()).unwrap();
    let pass_files = || {
        fs::read_dir(answer::pass_dir(&data(&home), SESSION))
            .map(|d| d.count())
            .unwrap_or(0)
    };
    assert_eq!(pass_files(), 1);
    let mode = fs::read_dir(answer::pass_dir(&data(&home), SESSION))
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(
        fs::metadata(mode.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let run = |payload: Value| {
        let since = Instant::now();
        let out = finish(start(&home, WAIT_MS, &payload), since);
        assert!(out.1 < PROMPTLY);
        out.0
    };
    // Before any claim the permission request is not covered.
    assert_eq!(run(retried("PermissionRequest", COMMAND, CWD, SESSION)), "");
    let claimed: Value =
        serde_json::from_str(&run(retried("PreToolUse", COMMAND, CWD, SESSION))).unwrap();
    assert_eq!(
        claimed,
        json!({ "hookSpecificOutput": { "hookEventName": "PreToolUse", "permissionDecision": "ask" } })
    );
    let allowed: Value =
        serde_json::from_str(&run(retried("PermissionRequest", COMMAND, CWD, SESSION))).unwrap();
    assert_eq!(
        allowed,
        json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest",
            "decision": { "behavior": "allow" } } })
    );
    assert_eq!(pass_files(), 0, "the pass is spent");
    // A second run has no pass.
    assert_eq!(run(retried("PreToolUse", COMMAND, CWD, SESSION)), "");
    assert_eq!(run(retried("PermissionRequest", COMMAND, CWD, SESSION)), "");
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_retry_that_is_not_the_approved_call_is_not_claimed_or_allowed() {
    let requoted = "claude -p 'summarise the repo' > /tmp/out.txt";
    let cases = [
        ("a re-quoted command", now_ms(), requoted, CWD, SESSION),
        ("another cwd", now_ms(), COMMAND, "/work/other", SESSION),
        ("another session", now_ms(), COMMAND, CWD, "s-2"),
        (
            "an expired pass",
            now_ms() - answer::PASS_TTL_MS - 1_000,
            COMMAND,
            CWD,
            SESSION,
        ),
    ];
    for (case, granted, command, cwd, session) in cases {
        let (claimed, permitted) = retry("pass-miss", granted, command, cwd, session, true);
        assert_eq!((claimed.as_str(), permitted.as_str()), ("", ""), "{case}");
    }
}

#[test]
fn a_pass_file_of_the_wrong_mode_is_refused() {
    let home = scratch("pass-mode");
    // The studio shows another session, so an uncovered permission request is not held for its card.
    watch(&home, "s-other", std::process::id(), 0);
    answer::write_pass(&data(&home), SESSION, COMMAND, CWD, now_ms()).unwrap();
    let file = fs::read_dir(answer::pass_dir(&data(&home), SESSION))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    for event in ["PreToolUse", "PermissionRequest"] {
        let since = Instant::now();
        let (out, _) = finish(
            start(&home, WAIT_MS, &retried(event, COMMAND, CWD, SESSION)),
            since,
        );
        assert_eq!(out, "", "{event}");
    }
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_retry_run_after_cd_into_the_subagents_folder_matches_the_pass_for_that_folder() {
    let home = scratch("pass-cd");
    watch(&home, "s-other", std::process::id(), 0);
    // The subagent was blocked in its own folder; the session ran `cd` there as its own call, so the retry's cwd is that folder.
    answer::write_pass(&data(&home), SESSION, COMMAND, "/work/sub", now_ms()).unwrap();
    let run = |payload: Value| {
        let since = Instant::now();
        let out = finish(start(&home, WAIT_MS, &payload), since);
        assert!(out.1 < PROMPTLY);
        out.0
    };
    // Run from the session's own folder it is not the approved call.
    assert_eq!(run(retried("PreToolUse", COMMAND, CWD, SESSION)), "");
    let claimed: Value =
        serde_json::from_str(&run(retried("PreToolUse", COMMAND, "/work/sub", SESSION))).unwrap();
    assert_eq!(claimed["hookSpecificOutput"]["permissionDecision"], "ask");
    let allowed: Value = serde_json::from_str(&run(retried(
        "PermissionRequest",
        COMMAND,
        "/work/sub",
        SESSION,
    )))
    .unwrap();
    assert_eq!(
        allowed["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn asking_again_leaves_one_fresh_pass_so_the_command_runs_once() {
    let home = scratch("pass-renew");
    watch(&home, "s-other", std::process::id(), 0);
    // The first approval was claimed by a retry that never reached its permission request; Daniel asks again.
    answer::write_pass(&data(&home), SESSION, COMMAND, CWD, now_ms() - 40 * 60_000).unwrap();
    answer::write_pass(&data(&home), SESSION, COMMAND, CWD, now_ms()).unwrap();
    answer::renew_pass(&data(&home), SESSION, COMMAND, CWD, now_ms()).unwrap();
    answer::renew_pass(&data(&home), SESSION, COMMAND, CWD, now_ms()).unwrap();
    let run = |payload: Value| {
        let since = Instant::now();
        let out = finish(start(&home, WAIT_MS, &payload), since);
        assert!(out.1 < PROMPTLY);
        out.0
    };
    assert!(run(retried("PreToolUse", COMMAND, CWD, SESSION)).contains("ask"));
    assert!(run(retried("PermissionRequest", COMMAND, CWD, SESSION)).contains("allow"));
    assert_eq!(
        run(retried("PreToolUse", COMMAND, CWD, SESSION)),
        "",
        "a second run has no pass"
    );
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn scanning_a_sessions_passes_deletes_the_expired_ones() {
    let home = scratch("pass-expired");
    watch(&home, "s-other", std::process::id(), 0);
    let old = now_ms() - answer::PASS_TTL_MS - 1_000;
    answer::write_pass(&data(&home), SESSION, "other command", CWD, old).unwrap();
    let dir = answer::pass_dir(&data(&home), SESSION);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    let since = Instant::now();
    let (out, _) = finish(
        start(
            &home,
            WAIT_MS,
            &retried("PreToolUse", COMMAND, CWD, SESSION),
        ),
        since,
    );
    assert_eq!(out, "");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    let _ = fs::remove_dir_all(&home);
}

#[test]
fn a_claimed_pass_whose_retry_is_blocked_again_allows_nothing_later() {
    let home = scratch("pass-blocked-again");
    watch(&home, "s-other", std::process::id(), 0);
    answer::write_pass(&data(&home), SESSION, COMMAND, CWD, now_ms()).unwrap();
    let run = |payload: Value| {
        let since = Instant::now();
        let out = finish(start(&home, WAIT_MS, &payload), since);
        assert!(out.1 < PROMPTLY);
        out.0
    };
    assert_ne!(run(retried("PreToolUse", COMMAND, CWD, SESSION)), "");
    // The retry is denied again while nobody watches, so its hook returns at once.
    fs::remove_file(answer::watching_file(&data(&home))).unwrap();
    assert_eq!(run(denied()), "");
    watch(&home, "s-other", std::process::id(), 0);
    assert_eq!(run(retried("PermissionRequest", COMMAND, CWD, SESSION)), "");
    let _ = fs::remove_dir_all(&home);
}
