//! Plan usage per agent (danielrispler/plotkeeper#163; spec #156, "Usage for every agent"): one shape for every agent's limits,
//! so a reader for another agent returns the same thing. No reader opens a credential.

use serde::Serialize;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// One window of the plan: its label (`5h`, `week`, or its length such as `30d`), the share used (0 to 100) and when it resets
/// (Unix seconds).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageWindow {
    pub label: String,
    pub used_percentage: f64,
    pub resets_at: i64,
}

/// A named set of limits that share their windows, such as one of agy's model groups. The name is empty for an agent with one group.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LimitGroup {
    pub name: String,
    pub windows: Vec<UsageWindow>,
}

/// An agent's usage: its limit groups, when they were read and from where; or, when they could not be read, one line saying why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentUsage {
    pub agent: String,
    pub groups: Vec<LimitGroup>,
    /// When the data was read, in Unix seconds.
    pub read_at: Option<i64>,
    /// Where it came from, such as "last Claude session".
    pub source: Option<String>,
    pub reason: Option<String>,
}

impl AgentUsage {
    /// An agent whose usage could not be read, and why.
    pub fn failed(agent: &str, reason: &str) -> Self {
        AgentUsage {
            agent: agent.into(),
            groups: Vec::new(),
            read_at: None,
            source: None,
            reason: Some(reason.into()),
        }
    }
}

/// The newest `*.json` snapshot in `dir`, parsed, with when it was saved (Unix seconds).
pub(crate) fn newest_snapshot(dir: &Path) -> Option<(serde_json::Value, i64)> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .max_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok())?;
    let saved = newest.metadata().ok()?.modified().ok()?;
    let saved = saved.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    let snapshot = serde_json::from_slice(&std::fs::read(newest.path()).ok()?).ok()?;
    Some((snapshot, saved))
}

/// Claude's usage, from the status line's newest snapshot in `dir`; its age is the snapshot's time.
pub fn claude_usage(dir: &Path) -> AgentUsage {
    let Some((snapshot, saved)) = newest_snapshot(dir) else {
        return AgentUsage::failed("claude", "No Claude session has saved its usage yet");
    };
    let window = |name: &str, label: &str| {
        let window = snapshot.get("rate_limits")?.get(name)?;
        Some(UsageWindow {
            label: label.into(),
            used_percentage: window.get("used_percentage")?.as_f64()?,
            resets_at: window.get("resets_at")?.as_i64()?,
        })
    };
    let windows: Vec<UsageWindow> = [window("five_hour", "5h"), window("seven_day", "week")]
        .into_iter()
        .flatten()
        .collect();
    if windows.is_empty() {
        return AgentUsage::failed("claude", "Claude's last session reported no plan limits");
    }
    AgentUsage {
        agent: "claude".into(),
        groups: vec![LimitGroup {
            name: String::new(),
            windows,
        }],
        read_at: Some(saved),
        source: Some("last Claude session".into()),
        reason: None,
    }
}

// Codex and agy (danielrispler/plotkeeper#165; #161's spike): each is read by running its CLI as a normal user, with a timeout,
// and the child killed after it. No credential file or token store is read.

/// How long Codex's and agy's commands may take. They answer in about 2 s and 3 s.
const CODEX_TIMEOUT: Duration = Duration::from_secs(20);
const AGY_TIMEOUT: Duration = Duration::from_secs(60);

/// A window's label from its length: `5h` and `week` as Claude's, else the length as `30d`, `12h` or `45m`.
fn window_label(minutes: i64) -> String {
    match minutes {
        300 => "5h".into(),
        10080 => "week".into(),
        m if m % 1440 == 0 => format!("{}d", m / 1440),
        m if m % 60 == 0 => format!("{}h", m / 60),
        m => format!("{m}m"),
    }
}

/// Codex's usage from the reply to `account/rateLimits/read`: each non-null window's kind comes from its length, never its place.
pub fn parse_codex(reply: &serde_json::Value, now: i64) -> AgentUsage {
    let unreadable =
        |why: &str| AgentUsage::failed("codex", &format!("Codex's usage could not be read: {why}"));
    if let Some(error) = reply.get("error") {
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("it returned an error");
        return unreadable(message);
    }
    let Some(limits) = reply.pointer("/result/rateLimits") else {
        return unreadable("the reply had no rate limits");
    };
    let mut windows: Vec<(i64, UsageWindow)> = ["primary", "secondary"]
        .iter()
        .filter_map(|key| {
            let window = limits.get(key)?;
            let minutes = window.get("windowDurationMins")?.as_i64()?;
            Some((
                minutes,
                UsageWindow {
                    label: window_label(minutes),
                    used_percentage: window.get("usedPercent")?.as_f64()?,
                    resets_at: window.get("resetsAt")?.as_i64()?,
                },
            ))
        })
        .collect();
    if windows.is_empty() {
        return unreadable("the reply had no windows");
    }
    windows.sort_by_key(|(minutes, _)| *minutes);
    AgentUsage {
        agent: "codex".into(),
        groups: vec![LimitGroup {
            name: String::new(),
            windows: windows.into_iter().map(|(_, w)| w).collect(),
        }],
        read_at: Some(now),
        source: Some("codex app-server".into()),
        reason: None,
    }
}

/// Unix seconds of an ISO 8601 UTC time such as `2026-11-17T18:11:50Z`.
fn iso_seconds(time: &str) -> Option<i64> {
    let time = time
        .strip_suffix('Z')
        .or_else(|| time.strip_suffix("+00:00"))?;
    let (date, clock) = time.split_once('T')?;
    let mut date = date.split('-').map(|n| n.parse::<i64>());
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    // Fractions of a second are dropped.
    let clock = clock.split_once('.').map_or(clock, |(whole, _)| whole);
    let mut clock = clock.split(':').map(|n| n.parse::<i64>());
    let (hour, minute, second) = (
        clock.next()?.ok()?,
        clock.next()?.ok()?,
        clock.next()?.ok()?,
    );
    // Days since 1970-01-01 of a civil date.
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146097 + day_of_era - 719468;
    Some(days * 86400 + hour * 3600 + minute * 60 + second)
}

/// agy's usage from its `/usage` reply: one limit group per model group, named as agy names it, each bucket a window.
pub fn parse_agy(reply: &serde_json::Value, now: i64) -> AgentUsage {
    let unreadable =
        |why: &str| AgentUsage::failed("agy", &format!("agy's usage could not be read: {why}"));
    if reply.get("status").and_then(|s| s.as_str()) != Some("SUCCESS") {
        return unreadable("agy did not report success");
    }
    let Some(groups) = reply
        .pointer("/command/data/groups")
        .and_then(|g| g.as_array())
    else {
        return unreadable("the reply had no limit groups");
    };
    let groups: Vec<LimitGroup> = groups
        .iter()
        .filter_map(|group| {
            let windows: Vec<UsageWindow> = group
                .get("buckets")?
                .as_array()?
                .iter()
                .filter_map(|bucket| {
                    let label = match bucket.get("window")?.as_str()? {
                        "weekly" => "week",
                        other => other,
                    };
                    Some(UsageWindow {
                        label: label.into(),
                        used_percentage: ((1.0 - bucket.get("remaining_fraction")?.as_f64()?)
                            * 100.0)
                            .clamp(0.0, 100.0),
                        resets_at: iso_seconds(bucket.get("reset_time")?.as_str()?)?,
                    })
                })
                .collect();
            let name = group.get("name")?.as_str()?.to_owned();
            (!windows.is_empty()).then_some(LimitGroup { name, windows })
        })
        .collect();
    if groups.is_empty() {
        return unreadable("the reply had no windows");
    }
    AgentUsage {
        agent: "agy".into(),
        groups,
        read_at: Some(now),
        source: Some("agy /usage".into()),
        reason: None,
    }
}

/// Runs `command` and gives what it printed, killing it when `timeout` passes, however long its output stays open: the whole read
/// is bounded, even when a helper it started holds its stdout. `stdin` is written, then closed. `path` is the `PATH` it runs with.
fn run_cli(
    mut command: Command,
    path: Option<&str>,
    stdin: &str,
    timeout: Duration,
) -> Result<String, String> {
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("it did not start ({e})"))?;
    if let Some(mut input) = child.stdin.take() {
        // A command that exits without reading its input is not an error here; its exit says so.
        let _ = input.write_all(stdin.as_bytes());
    }
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = sender.send(out);
    });
    let deadline = Instant::now() + timeout;
    let stop = |child: &mut std::process::Child, why: String| {
        let _ = child.kill();
        let _ = child.wait();
        Err(why)
    };
    let Ok(out) = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) else {
        return stop(&mut child, "it did not answer in time".into());
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(out),
            Ok(Some(status)) => return Err(format!("it exited with {status}")),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => return stop(&mut child, "it did not answer in time".into()),
            Err(e) => return stop(&mut child, format!("it could not be waited on ({e})")),
        }
    }
}

/// Codex's usage, from `codex app-server`: `initialize`, `initialized`, then `account/rateLimits/read` as newline-delimited
/// JSON-RPC, skipping every message but the answer to the read.
pub fn read_codex(program: &str, path: Option<&str>, timeout: Duration, now: i64) -> AgentUsage {
    let mut command = Command::new(program);
    command.args(["-s", "read-only", "-a", "never", "app-server"]);
    let requests = concat!(
        r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"plotkeeper-studio","version":"0"}}}"#,
        "\n",
        r#"{"method":"initialized"}"#,
        "\n",
        r#"{"id":2,"method":"account/rateLimits/read"}"#,
        "\n",
    );
    match run_cli(command, path, requests, timeout) {
        Err(why) => AgentUsage::failed("codex", &format!("Codex's usage could not be read: {why}")),
        Ok(out) => {
            let answer = out
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find(|message| message.get("id") == Some(&serde_json::json!(2)));
            match answer {
                Some(reply) => parse_codex(&reply, now),
                None => AgentUsage::failed(
                    "codex",
                    "Codex's usage could not be read: it gave no answer",
                ),
            }
        }
    }
}

/// agy's usage, from `agy -p /usage --output-format json` in an empty temporary folder, which costs no conversation.
pub fn read_agy(program: &str, path: Option<&str>, timeout: Duration, now: i64) -> AgentUsage {
    let Ok(dir) = tempfile::tempdir() else {
        return AgentUsage::failed("agy", "agy's usage could not be read: no temporary folder");
    };
    let mut command = Command::new(program);
    command
        .args(["-p", "/usage", "--output-format", "json"])
        .env("AGY_CLI_DISABLE_AUTO_UPDATE", "true")
        .current_dir(dir.path());
    match run_cli(command, path, "", timeout) {
        Err(why) => AgentUsage::failed("agy", &format!("agy's usage could not be read: {why}")),
        Ok(out) => match serde_json::from_str(&out) {
            Ok(reply) => parse_agy(&reply, now),
            Err(_) => AgentUsage::failed(
                "agy",
                "agy's usage could not be read: its reply was not JSON",
            ),
        },
    }
}

/// The usage of each CLI agent of `agents` (`codex`, `agy`) that `find` locates; an agent it does not find is not installed and
/// has no entry. `find` is the login shell's PATH lookup, so a test can stub it.
pub fn cli_usages(
    agents: &[&str],
    find: &(dyn Fn(&str) -> Option<String> + Sync),
    path: Option<&str>,
    now: i64,
) -> Vec<AgentUsage> {
    std::thread::scope(|scope| {
        let reads: Vec<_> = agents
            .iter()
            .filter_map(|&agent| {
                let program = find(agent)?;
                Some(scope.spawn(move || match agent {
                    "codex" => read_codex(&program, path, CODEX_TIMEOUT, now),
                    _ => read_agy(&program, path, AGY_TIMEOUT, now),
                }))
            })
            .collect();
        reads
            .into_iter()
            .filter_map(|read| read.join().ok())
            .collect()
    })
}

/// Where `program` is among the directories of `path`; `None` when it is not there.
pub fn find_in_path(program: &str, path: &str) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    path.split(':')
        .filter(|dir| !dir.is_empty())
        .map(|dir| Path::new(dir).join(program))
        .find(|candidate| {
            candidate
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
        .and_then(|found| found.to_str().map(str::to_owned))
}

/// The `PATH` a login shell gives, read by running `shell` once with `-lc`; `None` when it fails or takes longer than `timeout`.
pub fn login_shell_path(shell: &str, timeout: Duration) -> Option<String> {
    let mut command = Command::new(shell);
    command.args(["-lc", "printf '\\n%s' \"$PATH\""]);
    let out = run_cli(command, None, "", timeout).ok()?;
    out.lines()
        .last()
        .map(str::to_owned)
        .filter(|path| path.contains('/'))
}
