//! Plan usage per agent (danielrispler/plotkeeper#163; spec #156, "Usage for every agent"): one shape for every agent's limits,
//! so a reader for another agent returns the same thing. No reader opens a credential.

use serde::Serialize;
use std::path::Path;

/// One window of the plan: the share used (0 to 100) and when it resets (Unix seconds).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UsageWindow {
    pub used_percentage: f64,
    pub resets_at: i64,
}

/// An agent's usage: its windows, when they were read and from where; or, when they could not be read, one line saying why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AgentUsage {
    pub agent: String,
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
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
            five_hour: None,
            seven_day: None,
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
    let window = |name: &str| {
        let window = snapshot.get("rate_limits")?.get(name)?;
        Some(UsageWindow {
            used_percentage: window.get("used_percentage")?.as_f64()?,
            resets_at: window.get("resets_at")?.as_i64()?,
        })
    };
    let (five_hour, seven_day) = (window("five_hour"), window("seven_day"));
    if five_hour.is_none() && seven_day.is_none() {
        return AgentUsage::failed("claude", "Claude's last session reported no plan limits");
    }
    AgentUsage {
        agent: "claude".into(),
        five_hour,
        seven_day,
        read_at: Some(saved),
        source: Some("last Claude session".into()),
        reason: None,
    }
}
