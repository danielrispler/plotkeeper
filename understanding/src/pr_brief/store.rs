//! Keeping briefs and records.

use super::{Brief, BudgetRecord, CallUsage, Guard, LinkEvidence, PartialFacts, Shape};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};
/// A brief as stored: per PR and head, in the data folder. A reader ignores fields it does not know, and a
/// missing one has its default, so an older or newer build reads it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StoredBrief {
    #[serde(default)]
    pub version: u32,
    pub pr: u64,
    pub head: String,
    #[serde(default)]
    pub written_at: u64,
    /// A newer head was pushed: rewritten when opened (#334).
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub shape: Shape,
    #[serde(default)]
    pub brief: Brief,
    /// Guard with a law's offer (#401), from the facts it was written from; kept after the PR merges.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<Guard>,
    /// What it was written without (#477), when anything was missing.
    #[serde(default, skip_serializing_if = "PartialFacts::is_empty")]
    pub partial_facts: PartialFacts,
    /// The import sites behind each link the PR adds (#499), from its head; none in a brief stored before it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub link_evidence: Vec<LinkEvidence>,
}

/// One line per brief written, for tuning prompts, caps and models from real data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BriefRecord {
    pub pr: u64,
    pub head: String,
    pub at: u64,
    pub shape: String,
    pub budget: BudgetRecord,
    /// The code checks' flags.
    #[serde(default)]
    pub flags: Vec<String>,
    /// Items still flagged after the retry, each carrying "not confirmed from the facts".
    #[serde(default)]
    pub unconfirmed: usize,
    /// The claim check's flags, before the retry.
    #[serde(default)]
    pub claim_flags_before: Vec<String>,
    /// The claim check's flags that still stand after the retry (those of items the retry did not rewrite).
    #[serde(default)]
    pub claim_flags_after: Vec<String>,
    /// The flags the stored brief carries in the end.
    #[serde(default)]
    pub flags_after: Vec<String>,
    /// Why the claim check or the retry gave nothing (a failed call or a reply that does not read).
    #[serde(default)]
    pub claim_error: Option<String>,
    #[serde(default)]
    pub retry_error: Option<String>,
    /// The writer's call, the claim check's and the retry's, in that order, each when it was made.
    #[serde(default)]
    pub calls: Vec<CallUsage>,
}

/// The briefs and their records in the studio's data folder.
pub struct BriefStore {
    dir: PathBuf,
}

impl BriefStore {
    /// The store in `data`, a folder `pr-briefs/` and the file `pr-brief-records.jsonl` inside it.
    pub fn open(data: &Path) -> std::io::Result<Self> {
        let dir = data.join("pr-briefs");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, pr: u64, head: &str) -> PathBuf {
        let head: String = head.chars().filter(char::is_ascii_alphanumeric).collect();
        self.dir.join(format!("{pr}-{head}.json"))
    }

    /// The brief of `pr` at `head`.
    pub fn get(&self, pr: u64, head: &str) -> Option<StoredBrief> {
        serde_json::from_slice(&std::fs::read(self.path(pr, head)).ok()?).ok()
    }

    /// The newest brief of `pr`, whichever head.
    pub fn latest(&self, pr: u64) -> Option<StoredBrief> {
        let prefix = format!("{pr}-");
        std::fs::read_dir(&self.dir)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
            .filter_map(|e| {
                serde_json::from_slice::<StoredBrief>(&std::fs::read(e.path()).ok()?).ok()
            })
            .filter(|b| b.pr == pr)
            .max_by_key(|b| b.written_at)
    }

    /// The "Since you read" stories (#497) live in `since/`, one per `{pr}-{from}-{to}.json`, so `latest` only ever finds a full brief.
    fn since_path(&self, pr: u64, from: &str, to: &str) -> PathBuf {
        let clean = |h: &str| {
            h.chars()
                .filter(char::is_ascii_alphanumeric)
                .collect::<String>()
        };
        self.dir
            .join("since")
            .join(format!("{pr}-{}-{}.json", clean(from), clean(to)))
    }

    /// The "Since you read" story of `pr` that ends at `head`, with the head it runs from; the newest when there are several.
    pub fn since(&self, pr: u64, head: &str) -> Option<(String, StoredBrief)> {
        let head: String = head.chars().filter(char::is_ascii_alphanumeric).collect();
        let (prefix, suffix) = (format!("{pr}-"), format!("-{head}.json"));
        std::fs::read_dir(self.dir.join("since"))
            .ok()?
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let from = name
                    .strip_prefix(&prefix)?
                    .strip_suffix(&suffix)?
                    .to_string();
                let brief: StoredBrief =
                    serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
                (brief.pr == pr && !from.is_empty()).then_some((from, brief))
            })
            .max_by_key(|(_, b)| b.written_at)
    }

    /// Writes the "Since you read" story `brief`, which runs from head `from` to its own.
    pub fn put_since(&self, from: &str, brief: &StoredBrief) -> std::io::Result<()> {
        let path = self.since_path(brief.pr, from, &brief.head);
        std::fs::create_dir_all(path.parent().unwrap_or(&self.dir))?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(brief).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }

    /// Writes `brief`, through a temporary file so a crash leaves the old one.
    pub fn put(&self, brief: &StoredBrief) -> std::io::Result<()> {
        let path = self.path(brief.pr, &brief.head);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(brief).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }

    /// Marks the brief of `pr` at `head` stale; false when there is none.
    pub fn mark_stale(&self, pr: u64, head: &str) -> std::io::Result<bool> {
        match self.get(pr, head) {
            Some(mut b) => {
                b.stale = true;
                self.put(&b).map(|()| true)
            }
            None => Ok(false),
        }
    }

    /// Appends one record line.
    pub fn record(&self, record: &BriefRecord) -> std::io::Result<()> {
        let mut line = serde_json::to_string(record).map_err(std::io::Error::other)?;
        line.push('\n');
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("pr-brief-records.jsonl"))?
            .write_all(line.as_bytes())
    }

    /// The records written so far, oldest first.
    pub fn records(&self) -> Vec<BriefRecord> {
        let text =
            std::fs::read_to_string(self.dir.join("pr-brief-records.jsonl")).unwrap_or_default();
        text.lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
}
