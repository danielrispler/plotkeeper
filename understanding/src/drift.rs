//! Drift in Claude Code under the studio (#369, decisions 7, 9 and 23): its version, read from a
//! session's transcript at its first `Stop` and kept beside the event log, so that a change asks
//! Daniel to re-record the subagent fixture; and the wake-ups the studio does not recognise.

use crate::week::{WEEK, date};
use serde_json::Value;
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
use studio_core::{Sender, Studio};

/// The versions of Claude Code seen, in `claude-versions.txt` beside the event log: a line
/// `<Unix ms> <version>` each time it changed, oldest first. The last line is the last version seen.
pub struct Versions {
    path: PathBuf,
}

impl Versions {
    pub fn beside(log: &Path) -> Versions {
        Versions {
            path: log.with_file_name("claude-versions.txt"),
        }
    }

    /// Reads Claude Code's version from `transcript`, seen at `at`, and keeps it when it changed.
    /// Returns the one-line notice when it differs from the last version seen; the first version
    /// seen has nothing to compare with.
    pub fn check(&self, transcript: &Path, at: u64) -> io::Result<Option<String>> {
        let Some(version) = transcript_version(transcript) else {
            return Ok(None);
        };
        let last = self.seen().pop().map(|(_, version)| version);
        if last.as_ref() == Some(&version) {
            return Ok(None);
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{at} {version}")?;
        Ok(last.map(|last| {
            format!(
                "Claude Code changed from {last} to {version}: re-record the subagent fixture \
                 (core/tests/record-subagent-run.sh)"
            )
        }))
    }

    /// Each version kept, with when it was first seen; none when the file is missing.
    fn seen(&self) -> Vec<(u64, String)> {
        let text = fs::read_to_string(&self.path).unwrap_or_default();
        let lines = text.lines().filter_map(|line| line.split_once(' '));
        lines
            .filter_map(|(at, version)| Some((at.parse().ok()?, version.into())))
            .collect()
    }
}

/// The version on the first line of `transcript` that carries one (`"version":"2.1.309"`). Only a
/// `.jsonl` transcript is opened, so a payload that names a credential file or a CLI's config, such
/// as `~/.claude.json`, reads nothing (`apps/studio/AGENTS.md`, "Hard rules").
fn transcript_version(transcript: &Path) -> Option<String> {
    if transcript.extension()? != "jsonl" {
        return None;
    }
    let lines = BufReader::new(File::open(transcript).ok()?).lines();
    lines.map_while(Result::ok).find_map(|line| {
        let line: Value = serde_json::from_str(&line).ok()?;
        line["version"].as_str().map(Into::into)
    })
}

/// The signs in the 7 days to a time that Claude Code changed under the studio.
#[derive(Clone, Debug, PartialEq)]
pub struct Drift {
    /// The prompts that named a known task id in markup the studio does not know.
    pub unrecognised_wake_ups: usize,
    /// Each version change: when it was seen, the old version and the new one.
    pub version_changes: Vec<(u64, String, String)>,
}

/// The drift in the 7 days to `now`, from the replayed event log and the versions kept.
pub fn drift(studio: &Studio, versions: &Versions, now: u64) -> Drift {
    let recent = |at: u64| at + WEEK >= now;
    let prompts = studio.sessions().into_iter().flat_map(|s| &s.prompts);
    let unrecognised = prompts.filter(|p| matches!(p.sender, Sender::Unrecognised(_)));
    let seen = versions.seen();
    let changes = seen.windows(2).filter(|pair| recent(pair[1].0));
    Drift {
        unrecognised_wake_ups: unrecognised.filter(|p| recent(p.at)).count(),
        version_changes: changes
            .map(|pair| (pair[1].0, pair[0].1.clone(), pair[1].1.clone()))
            .collect(),
    }
}

impl fmt::Display for Drift {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "unrecognised wake-ups: {}", self.unrecognised_wake_ups)?;
        for (at, from, to) in &self.version_changes {
            writeln!(
                f,
                "Claude Code changed from {from} to {to} on {}",
                date(*at)
            )?;
        }
        Ok(())
    }
}
