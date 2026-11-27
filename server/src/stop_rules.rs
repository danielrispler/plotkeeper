//! Always-on stop-rule tracking (spec danielrispler/plotkeeper#306, ticket #314): the window tells the shell what Daniel uses, and the
//! shell appends one line per event to `stop-rules.jsonl` in the data folder (no setting, nothing shown). The window decides whether the
//! one-time review is due (`ui/stop-rules.ts`) from the counts `facts` gives it.
//!
//! A day is a local calendar day the window names (`2026-11-03`); the shell has no clock zone of its own. An active day is one with a
//! `session` or a `map-open` event; an engaged day is an active day with an entered action on the project's map (spec #380, "Stop
//! rules"), so a day the map was only open on is not one. Merged PRs are counted on the default branch's first-parent commits since the first event was
//! logged, so the count starts when tracking does.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// The log's file, in the data folder.
const LOG_FILE: &str = "stop-rules.jsonl";

/// The modules an older `dismissed` line with no module dismissed: the review's modules when dismissal was one flag per project.
const LEGACY_MODULES: [&str; 5] = ["map", "history", "proof", "briefs", "skips"];

/// What happened. A kind this build does not know reads as `Other` and counts for nothing, so an older build tolerates a newer log, and
/// the kinds of the bar's rating and note, removed by #448 (`rating-offered`, `rating-given`, `rating-skipped`, `note-given`), read as
/// `Other` from an older log and are refused when sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The session list had a session on the day.
    Session,
    /// The Map view opened.
    MapOpen,
    /// An entered action on the map with no event of its own: a part's sheet opened, a session badge or a row of its list pressed (once a day).
    MapAction,
    /// A part's history strip was scrubbed to a commit.
    Scrub,
    /// "See the proof", a CI log or a commit's files opened in the drawer.
    Proof,
    /// "I know this" cleared a part's fog; tracked apart, with no threshold in project 2.
    Know,
    /// The review of one module (`module`) was dismissed for the project; an older line with no module dismissed the modules of its
    /// time (`LEGACY_MODULES`).
    Dismissed,
    /// A PR's brief had a row in the tray and the row was seen (once per PR).
    BriefOffered,
    /// A tray row opened the PR's brief on the map.
    BriefOpened,
    /// A chapter of a brief was read to its end.
    ChapterDone,
    /// A brief's check was answered, right or wrong.
    CheckAnswered,
    /// Accept or Overturn was pressed on an item to ratify.
    Ratify,
    /// A step's "not confirmed from the facts" mark was seen.
    UnconfirmedSeen,
    /// A fogged part's sheet offered its part catch-up (#391).
    CatchUpOffered,
    /// A part catch-up was opened from the part's sheet.
    CatchUpOpened,
    /// A part catch-up was read to its end, which cleared the part's fog.
    CatchUpDone,
    /// An explain-back line was given in the bar, on a Core part's story (#397); the line carries it. Kept if at least 3.
    ExplainBackGiven,
    /// The explain-back line was marked got it, partly or missed against the gist; the line carries the mark.
    ExplainBackMarked,
    /// A part's refresher was entered (#392).
    RefresherEntered,
    /// A refresher or a part catch-up was entered from a part with a staleness band (#392); logged beside the entry's own kind.
    StaleEntered,
    /// A pre-flight framed its parts on the map, predicted or picked by hand (#394).
    PreFlightRun,
    /// A micro-world's page was opened in its overlay (#399). Kept if at least 2.
    MicroWorldOpened,
    /// A back a law run was started from a law on trust in a part's sheet (#398). Kept if at least 1.
    BackLawStarted,
    /// A brief's Guard with a law row was seen (#401), once per PR. Guard with a law is judged only after 3 offers.
    GuardOffered,
    /// A run was started from a brief's Guard with a law: backing a law on trust or adding a new one. Kept if at least 1.
    GuardStarted,
    #[serde(other)]
    Other,
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Unix seconds.
    pub at: u64,
    /// The local calendar day, `2026-11-03`.
    pub day: String,
    pub kind: Kind,
    #[serde(default)]
    pub project: Option<String>,
    /// The PR a brief event is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<u64>,
    /// The module a `dismissed` event is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
}

/// What the window needs of one project to decide on the review. Counts only; the window shows none of them.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct Facts {
    pub active_days: u32,
    pub merged_prs: u32,
    /// Active days with at least one entered action on the project's map.
    pub engaged_days: u32,
    pub scrubs: u32,
    pub proofs: u32,
    pub knows: u32,
    /// The modules whose review was dismissed, sorted.
    pub dismissed: Vec<String>,
    /// Merged PRs of the project whose brief was offered, and how many of those had it opened.
    pub briefs_offered: u32,
    pub briefs_opened: u32,
    /// Chapters read to their end, to set against `knows`.
    pub chapters_done: u32,
    /// Every kind's count for the project (kinds with none are left out), for the review table's bars and offer counts.
    pub counts: BTreeMap<Kind, u32>,
    /// The local day of the project's first event, where a module's calendar wait starts at the latest.
    pub began: Option<String>,
    /// When the project's staleness baselines were set (unix seconds; `staleness.rs`), where the refresher's and staleness bands'
    /// calendar wait starts; none before its first map with that build.
    pub stale_since: Option<u64>,
}

/// Serialises appends, so two events never share a line.
static WRITING: Mutex<()> = Mutex::new(());

pub(crate) fn log_file() -> Result<PathBuf, String> {
    let home = std::env::home_dir().ok_or("no home folder")?;
    Ok(studio_hook::data_dir(&home).join(LOG_FILE))
}

/// Appends `event` to `file`, creating it and its folder.
pub fn append(file: &Path, event: &Event) -> Result<(), String> {
    let _writing = WRITING.lock().unwrap();
    append_held(file, event)
}

/// Appends `line`, any line that reads as an `Event` (one with fields of its own beside them), to `file`.
pub(crate) fn append_line(file: &Path, line: &impl Serialize) -> Result<(), String> {
    let _writing = WRITING.lock().unwrap();
    append_held(file, line)
}

/// `append`, with `WRITING` held by the caller.
fn append_held(file: &Path, event: &impl Serialize) -> Result<(), String> {
    let line = serde_json::to_string(event).map_err(|e| e.to_string())?;
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // A torn last line (a crash mid-write) must not swallow this event: start a new line.
    let torn = std::fs::read(file).is_ok_and(|bytes| bytes.last().is_some_and(|b| *b != b'\n'));
    let mut out = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .map_err(|e| e.to_string())?;
    if torn {
        writeln!(out).map_err(|e| e.to_string())?;
    }
    writeln!(out, "{line}").map_err(|e| e.to_string())
}

/// Every event of `file`, oldest first. A missing file is no events; a line that does not read (torn, or from a format this build
/// does not know) is skipped and the rest are kept.
pub fn read(file: &Path) -> Vec<Event> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Whether `kind` is an entered action on the map, not something only seen or opened: a part's sheet opened or a session badge pressed
/// (`MapAction`), a scrub, a proof, a brief entered and what is done inside a sheet or a brief.
fn entered(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::MapAction
            | Kind::Scrub
            | Kind::Proof
            | Kind::Know
            | Kind::BriefOpened
            | Kind::ChapterDone
            | Kind::CheckAnswered
            | Kind::Ratify
            | Kind::CatchUpOpened
            | Kind::CatchUpDone
            | Kind::RefresherEntered
            | Kind::StaleEntered
            | Kind::PreFlightRun
            | Kind::MicroWorldOpened
            | Kind::BackLawStarted
            | Kind::GuardStarted
    )
}

/// When `project`'s first event was logged: merged PRs count from then, not from the first event of any project.
fn began(events: &[Event], project: &str) -> Option<u64> {
    events
        .iter()
        .filter(|e| e.project.as_deref() == Some(project))
        .map(|e| e.at)
        .min()
}

/// Where merged PRs are counted from for `project`: its own first event, else the Studio-wide start of tracking (the log's first
/// event); `None` for an empty log.
fn counted_from(events: &[Event], project: &str) -> Option<u64> {
    began(events, project).or_else(|| events.iter().map(|e| e.at).min())
}

/// The counts for `project`; `merged_prs` is read from git by the caller.
pub fn facts(events: &[Event], project: &str, merged_prs: u32) -> Facts {
    let count = |kind| {
        events
            .iter()
            .filter(|e| e.kind == kind && e.project.as_deref() == Some(project))
            .count() as u32
    };
    let active: BTreeSet<&str> = events
        .iter()
        .filter(|e| matches!(e.kind, Kind::Session | Kind::MapOpen))
        .map(|e| e.day.as_str())
        .collect();
    let engaged: BTreeSet<&str> = events
        .iter()
        .filter(|e| entered(e.kind) && e.project.as_deref() == Some(project))
        .map(|e| e.day.as_str())
        .filter(|day| active.contains(day))
        .collect();
    Facts {
        active_days: active.len() as u32,
        merged_prs,
        engaged_days: engaged.len() as u32,
        scrubs: count(Kind::Scrub),
        proofs: count(Kind::Proof),
        knows: count(Kind::Know),
        dismissed: dismissed(events, project),
        briefs_offered: 0,
        briefs_opened: 0,
        chapters_done: count(Kind::ChapterDone),
        counts: events
            .iter()
            .filter(|e| e.kind != Kind::Other && e.project.as_deref() == Some(project))
            .fold(BTreeMap::new(), |mut counts, e| {
                *counts.entry(e.kind).or_insert(0) += 1;
                counts
            }),
        began: events
            .iter()
            .filter(|e| e.project.as_deref() == Some(project))
            .min_by_key(|e| e.at)
            .map(|e| e.day.clone()),
        stale_since: None,
    }
}

/// The modules whose review `project` dismissed, sorted.
fn dismissed(events: &[Event], project: &str) -> Vec<String> {
    let mut modules = BTreeSet::new();
    for e in events
        .iter()
        .filter(|e| e.kind == Kind::Dismissed && e.project.as_deref() == Some(project))
    {
        match &e.module {
            Some(module) => {
                modules.insert(module.clone());
            }
            None => modules.extend(LEGACY_MODULES.map(String::from)),
        }
    }
    modules.into_iter().collect()
}

/// Of the merged PRs `merged` (numbers), how many had a brief offered for `project`, and how many of those had it opened.
pub fn brief_use(events: &[Event], project: &str, merged: &BTreeSet<u64>) -> (u32, u32) {
    let of = |kind| -> BTreeSet<u64> {
        events
            .iter()
            .filter(|e| e.kind == kind && e.project.as_deref() == Some(project))
            .filter_map(|e| e.pr)
            .filter(|pr| merged.contains(pr))
            .collect()
    };
    let offered = of(Kind::BriefOffered);
    let opened = of(Kind::BriefOpened);
    (
        offered.len() as u32,
        offered.intersection(&opened).count() as u32,
    )
}

/// Appends `event` unless the log already holds one of its kind for the same project and PR: a PR's brief is offered once.
pub fn append_once(file: &Path, event: &Event) -> Result<bool, String> {
    let same = |e: &Event| e.kind == event.kind && e.project == event.project && e.pr == event.pr;
    // Held across the read and the append, so two offers at once log one line.
    let _writing = WRITING.lock().unwrap();
    if read(file).iter().any(same) {
        return Ok(false);
    }
    append_held(file, event).map(|()| true)
}

/// The PRs merged on `repo`'s default branch at or after `since` (unix seconds), newest first: its first-parent commits whose
/// subject ends `(#N)` (a squash) or starts `Merge pull request #N`.
pub fn merged_numbers_since(repo: &Path, since: u64) -> Vec<u64> {
    let Some((reference, _)) = crate::map::default_ref(repo) else {
        return Vec::new();
    };
    // Not `git log --since=N`: git reads a small N as a relative date ("0" is now), so the cut is made here on `%ct`.
    let log = crate::map::git(
        repo,
        &["log", "--first-parent", "--format=%ct %s", &reference],
    )
    .unwrap_or_default();
    log.lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(at, _)| at.parse::<u64>().is_ok_and(|at| at >= since))
        .filter_map(|(_, subject)| merge_number(subject))
        .collect()
}

/// The PR a squash (`... (#N)`) or merge (`Merge pull request #N ...`) subject lands.
fn merge_number(subject: &str) -> Option<u64> {
    let digits = |n: &str| {
        (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            .then(|| n.parse().ok())
            .flatten()
    };
    let squash = subject
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once("(#"))
        .and_then(|(_, n)| digits(n));
    squash.or_else(|| {
        let rest = subject.strip_prefix("Merge pull request #")?;
        digits(rest.split(' ').next().unwrap_or(""))
    })
}

/// Now, in Unix seconds, as the stop rules' log keeps time.
fn now() -> u64 {
    studio_workbench::clock::now_ms() / 1000
}

/// Logs that Daniel used something. A `session` event names no project.
pub fn stop_log(
    kind: Kind,
    project: Option<String>,
    day: String,
    pr: Option<u64>,
) -> Result<(), String> {
    if matches!(kind, Kind::Other | Kind::Dismissed) {
        return Err("not an event the window logs".into());
    }
    let event = Event {
        at: now(),
        day,
        kind,
        project,
        pr,
        module: None,
    };
    let file = log_file()?;
    if matches!(kind, Kind::BriefOffered | Kind::GuardOffered) {
        return append_once(&file, &event).map(drop);
    }
    append(&file, &event)
}

/// The counts the review is decided from, for `project`.
pub fn stop_facts(project: String) -> Result<Facts, String> {
    let mut found = facts_in(&log_file()?, &project);
    found.stale_since = crate::map::read_user(&crate::map::state_file()?, &project)
        .staleness
        .since;
    Ok(found)
}

/// The counts for `project` from the log `file`, with its merged PRs read from git.
fn facts_in(file: &Path, project: &str) -> Facts {
    let events = read(file);
    let from = counted_from(&events, project).unwrap_or(now());
    let merged = merged_numbers_since(Path::new(project), from);
    let mut found = facts(&events, project, merged.len() as u32);
    (found.briefs_offered, found.briefs_opened) =
        brief_use(&events, project, &merged.into_iter().collect());
    found
}

/// Dismisses the review of `modules` in each of `projects`; a module not named is still reviewed when it falls due, and the log goes on.
pub fn stop_dismiss(
    projects: Vec<String>,
    modules: Vec<String>,
    day: String,
) -> Result<(), String> {
    dismiss_in(&log_file()?, &projects, &modules, &day)
}

/// `stop_dismiss`, into the log `file`.
fn dismiss_in(
    file: &Path,
    projects: &[String],
    modules: &[String],
    day: &str,
) -> Result<(), String> {
    for project in projects {
        for module in modules {
            append(
                file,
                &Event {
                    at: now(),
                    day: day.into(),
                    kind: Kind::Dismissed,
                    project: Some(project.clone()),
                    pr: None,
                    module: Some(module.clone()),
                },
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
