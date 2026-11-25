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
mod tests {
    use super::*;
    use studio_test_support::{git, repo};

    fn event(day: &str, kind: Kind, project: Option<&str>) -> Event {
        Event {
            at: 1,
            day: day.into(),
            kind,
            project: project.map(String::from),
            pr: None,
            module: None,
        }
    }

    fn brief(day: &str, kind: Kind, pr: u64) -> Event {
        Event {
            pr: Some(pr),
            ..event(day, kind, Some("/p"))
        }
    }

    #[test]
    fn an_append_after_a_torn_line_starts_a_new_line() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        std::fs::write(&file, "{\"at\":1,\"day\":\"2026-1").unwrap();
        let next = event("2026-11-02", Kind::Know, Some("/p"));
        append(&file, &next).unwrap();
        assert_eq!(read(&file), [next]);
    }

    #[test]
    fn merged_prs_count_from_the_projects_own_first_event() {
        let at = |at, project| Event {
            at,
            ..event("2026-11-01", Kind::MapOpen, project)
        };
        let events = [at(5, None), at(10, Some("/a")), at(20, Some("/b"))];
        assert_eq!(began(&events, "/b"), Some(20));
        assert_eq!(began(&events, "/a"), Some(10));
        assert_eq!(began(&events, "/c"), None);
    }

    #[test]
    fn a_project_with_no_events_of_its_own_counts_from_the_logs_first_event() {
        let at = |at, project| Event {
            at,
            ..event("2026-11-01", Kind::Session, project)
        };
        let events = [at(30, None), at(5, None), at(10, Some("/a"))];
        assert_eq!(counted_from(&events, "/c"), Some(5));
        assert_eq!(counted_from(&events, "/a"), Some(10));
        assert_eq!(counted_from(&[], "/a"), None);
    }

    #[test]
    fn events_round_trip_through_the_file_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data").join(LOG_FILE);
        let all = [
            event("2026-11-01", Kind::Session, None),
            event("2026-11-01", Kind::MapOpen, Some("/p")),
            event("2026-11-02", Kind::Know, Some("/p")),
        ];
        for e in &all {
            append(&file, e).unwrap();
        }
        assert_eq!(read(&file), all);
    }

    #[test]
    fn a_missing_older_or_newer_log_is_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        assert!(read(&file).is_empty());
        // An event with no project, a kind and a field of a later build, a torn line and a blank one.
        std::fs::write(
            &file,
            concat!(
                "{\"at\":1,\"day\":\"2026-11-01\",\"kind\":\"session\"}\n",
                "{\"at\":2,\"day\":\"2026-11-01\",\"kind\":\"pr-brief\",\"project\":\"/p\",\"extra\":1}\n",
                "{\"at\":3,\"day\":\"2026-11-01\",\"kind\":\"scrub\",\"project\":\"/p\",\"later\":true}\n",
                "{\"at\":4,\"day\":\"2026-1",
                "\n\n",
            ),
        )
        .unwrap();
        let events = read(&file);
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].kind, Kind::Other);
        let found = facts(&events, "/p", 0);
        assert_eq!(
            (found.active_days, found.scrubs, found.engaged_days),
            (1, 1, 1)
        );
        // A later append goes on after a torn last line without losing the lines before it.
        append(&file, &event("2026-11-02", Kind::Proof, Some("/p"))).unwrap();
        assert!(read(&file).iter().any(|e| e.kind == Kind::Proof));
    }

    #[test]
    fn days_are_counted_by_what_makes_a_day_active_and_per_project() {
        let events = [
            event("2026-11-01", Kind::Session, None),
            event("2026-11-01", Kind::Session, None),
            event("2026-11-02", Kind::MapOpen, Some("/p")),
            event("2026-11-02", Kind::MapOpen, Some("/p")),
            event("2026-11-03", Kind::MapOpen, Some("/other")),
            // Used, but neither a session nor a map open: no active day of its own.
            event("2026-11-05", Kind::Scrub, Some("/p")),
        ];
        let found = facts(&events, "/p", 7);
        assert_eq!(found.active_days, 3);
        assert_eq!(found.merged_prs, 7);
        assert_eq!(found.scrubs, 1);
        assert_eq!(facts(&events, "/other", 0).scrubs, 0);
    }

    #[test]
    fn engaged_days_are_counted_from_the_log_in_a_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data").join(LOG_FILE);
        let briefed = |day, kind| Event {
            pr: Some(7),
            ..event(day, kind, Some("/p"))
        };
        for e in [
            // Open, with no entered action: not engaged.
            event("2026-11-01", Kind::Session, None),
            event("2026-11-01", Kind::MapOpen, Some("/p")),
            // A sheet opened or a session badge pressed.
            event("2026-11-02", Kind::MapOpen, Some("/p")),
            event("2026-11-02", Kind::MapAction, Some("/p")),
            // Two entered actions on one day: one engaged day.
            event("2026-11-03", Kind::MapOpen, Some("/p")),
            event("2026-11-03", Kind::Scrub, Some("/p")),
            event("2026-11-03", Kind::Proof, Some("/p")),
            // A brief entered.
            event("2026-11-04", Kind::Session, None),
            briefed("2026-11-04", Kind::BriefOpened),
            // Another project's action.
            event("2026-11-05", Kind::MapOpen, Some("/other")),
            event("2026-11-05", Kind::MapAction, Some("/other")),
            // Seen, not entered.
            event("2026-11-06", Kind::MapOpen, Some("/p")),
            briefed("2026-11-06", Kind::BriefOffered),
            briefed("2026-11-06", Kind::UnconfirmedSeen),
        ] {
            append(&file, &e).unwrap();
        }
        // A line of the previous build: Home's mark seen reads, and counts for nothing.
        let mut log = std::fs::read_to_string(&file).unwrap();
        log.push_str(
            "{\"at\":1,\"day\":\"2026-11-07\",\"kind\":\"mark-seen\",\"project\":\"/p\"}\n",
        );
        std::fs::write(&file, log).unwrap();
        let found = facts_in(&file, "/p");
        assert_eq!((found.active_days, found.engaged_days), (6, 3));
        assert_eq!(facts_in(&file, "/other").engaged_days, 1);
    }

    #[test]
    fn uses_are_counted_and_a_dismissal_is_per_project() {
        let events = [
            event("2026-11-01", Kind::Proof, Some("/p")),
            event("2026-11-01", Kind::Proof, Some("/p")),
            event("2026-11-01", Kind::Know, Some("/p")),
            event("2026-11-01", Kind::Dismissed, Some("/p")),
        ];
        let found = facts(&events, "/p", 0);
        assert_eq!((found.proofs, found.knows), (2, 1));
        assert!(!found.dismissed.is_empty());
        assert!(facts(&events, "/q", 0).dismissed.is_empty());
    }

    #[test]
    fn a_dismissal_names_its_modules_and_an_older_one_names_the_modules_of_its_time() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        dismiss_in(
            &file,
            &["/p".into(), "/q".into()],
            &["history".into()],
            "2026-11-20",
        )
        .unwrap();
        dismiss_in(&file, &["/p".into()], &["refresher".into()], "2026-12-20").unwrap();
        let events = read(&file);
        assert_eq!(facts(&events, "/p", 0).dismissed, ["history", "refresher"]);
        assert_eq!(facts(&events, "/q", 0).dismissed, ["history"]);
        // A project-wide dismissal of the build before per-module dismissal.
        let mut log = std::fs::read_to_string(&file).unwrap();
        log.push_str(
            "{\"at\":1,\"day\":\"2026-11-07\",\"kind\":\"dismissed\",\"project\":\"/q\"}\n",
        );
        std::fs::write(&file, log).unwrap();
        assert_eq!(
            facts(&read(&file), "/q", 0).dismissed,
            ["briefs", "history", "map", "proof", "skips"]
        );
    }

    #[test]
    fn a_part_catch_up_opened_or_completed_is_an_entered_action_counted_apart_from_i_know_this() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        let line = |day: &str, kind: &str| {
            format!("{{\"at\":1,\"day\":\"{day}\",\"kind\":\"{kind}\",\"project\":\"/p\"}}\n")
        };
        let log = [
            line("2026-11-01", "map-open"),
            line("2026-11-01", "catch-up-offered"),
            line("2026-11-02", "map-open"),
            line("2026-11-02", "catch-up-opened"),
            line("2026-11-03", "map-open"),
            line("2026-11-03", "catch-up-done"),
            line("2026-11-03", "know"),
        ]
        .concat();
        std::fs::write(&file, log).unwrap();
        let found = facts_in(&file, "/p");
        assert_eq!(found.engaged_days, 2, "offered is seen, not entered");
        let counts = serde_json::to_value(&found).unwrap()["counts"].clone();
        assert_eq!(counts["catch-up-offered"], 1);
        assert_eq!(counts["catch-up-opened"], 1);
        assert_eq!(counts["catch-up-done"], 1);
        assert_eq!(counts["know"], 1);
    }

    #[test]
    fn the_refresher_and_an_entry_from_a_stale_part_are_counted_and_each_makes_an_engaged_day() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        let lines = [
            r#"{"at":1,"day":"2026-11-01","kind":"map-open","project":"/p"}"#,
            r#"{"at":2,"day":"2026-11-01","kind":"refresher-entered","project":"/p"}"#,
            r#"{"at":3,"day":"2026-11-02","kind":"map-open","project":"/p"}"#,
            r#"{"at":4,"day":"2026-11-02","kind":"refresher-entered","project":"/p"}"#,
            r#"{"at":5,"day":"2026-11-02","kind":"stale-entered","project":"/p"}"#,
        ];
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        let json = serde_json::to_value(facts_in(&file, "/p")).unwrap();
        assert_eq!(json["counts"]["refresher-entered"], 2);
        assert_eq!(json["counts"]["stale-entered"], 1);
        assert_eq!(json["engaged_days"], 2);
    }

    #[test]
    fn a_back_a_law_run_started_is_counted_and_makes_an_engaged_day() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        let lines = [
            r#"{"at":1,"day":"2026-11-01","kind":"map-open","project":"/p"}"#,
            r#"{"at":2,"day":"2026-11-01","kind":"back-law-started","project":"/p"}"#,
        ];
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        let json = serde_json::to_value(facts_in(&file, "/p")).unwrap();
        assert_eq!(json["counts"]["back-law-started"], 1);
        assert_eq!(json["engaged_days"], 1);
    }

    #[test]
    fn guard_with_a_law_counts_its_offers_and_its_runs_started_and_a_run_makes_an_engaged_day() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        let lines = [
            r#"{"at":1,"day":"2026-11-01","kind":"map-open","project":"/p"}"#,
            r#"{"at":1,"day":"2026-11-02","kind":"map-open","project":"/p"}"#,
            r#"{"at":1,"day":"2026-11-01","kind":"guard-offered","project":"/p","pr":7}"#,
            r#"{"at":2,"day":"2026-11-01","kind":"guard-offered","project":"/p","pr":8}"#,
            r#"{"at":3,"day":"2026-11-02","kind":"guard-started","project":"/p","pr":8}"#,
        ];
        std::fs::write(&file, lines.join("\n") + "\n").unwrap();
        let json = serde_json::to_value(facts_in(&file, "/p")).unwrap();
        assert_eq!(json["counts"]["guard-offered"], 2);
        assert_eq!(json["counts"]["guard-started"], 1);
        assert_eq!(json["engaged_days"], 1, "an offer is seen, not entered");
    }

    #[test]
    fn every_kind_is_counted_per_project_and_the_first_day_is_kept() {
        let at = |at, day, kind, project| Event {
            at,
            ..event(day, kind, project)
        };
        let events = [
            at(5, "2026-10-30", Kind::Session, None),
            at(20, "2026-11-02", Kind::Scrub, Some("/p")),
            at(10, "2026-11-01", Kind::BriefOffered, Some("/p")),
            at(30, "2026-11-03", Kind::BriefOffered, Some("/p")),
            at(40, "2026-11-04", Kind::BriefOffered, Some("/q")),
        ];
        let found = facts(&events, "/p", 0);
        assert_eq!(found.counts.get(&Kind::BriefOffered), Some(&2));
        assert_eq!(found.counts.get(&Kind::Scrub), Some(&1));
        assert_eq!(found.counts.get(&Kind::Session), None);
        assert_eq!(found.began.as_deref(), Some("2026-11-01"));
        assert_eq!(facts(&events, "/r", 0).began, None);
        let json = serde_json::to_value(&found).unwrap();
        assert_eq!(json["counts"]["brief-offered"], 2);
    }

    #[test]
    fn a_merged_pr_is_a_squash_or_a_merge_commit_on_the_first_parent_line() {
        let (dir, path) = repo();
        let at = dir.path();
        for subject in [
            "Studio: a thing (#12)",
            "Merge pull request #13 from a/b",
            "A plain commit",
            "Mentions (#14) mid-line",
            "Ends in a word (#x)",
        ] {
            git(at, &["commit", "-q", "--allow-empty", "-m", subject]);
        }
        // A start of 0 (or any small number) is every commit; git's `--since=0` reads it as "now", so a commit older than the
        // second `git log` runs in would not count.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert_eq!(merged_numbers_since(Path::new(&path), 0).len(), 2);
        // Nothing merged after a future start.
        assert_eq!(
            merged_numbers_since(Path::new(&path), now() + 3600).len(),
            0
        );
    }

    #[test]
    fn a_start_is_inclusive_of_the_commits_own_second() {
        let (dir, path) = repo();
        let stamp = "1700000000 +0000";
        let status = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=S",
                "-c",
                "user.email=s@e.com",
                "commit",
                "-q",
            ])
            .args(["--allow-empty", "-m", "Studio: a thing (#5)"])
            .env("GIT_COMMITTER_DATE", stamp)
            .env("GIT_AUTHOR_DATE", stamp)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(merged_numbers_since(Path::new(&path), 1_700_000_000), [5]);
        assert!(merged_numbers_since(Path::new(&path), 1_700_000_001).is_empty());
    }

    #[test]
    fn a_brief_is_offered_once_per_pr_however_often_its_row_is_seen() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        assert!(append_once(&file, &brief("2026-11-01", Kind::BriefOffered, 7)).unwrap());
        assert!(!append_once(&file, &brief("2026-11-02", Kind::BriefOffered, 7)).unwrap());
        assert!(append_once(&file, &brief("2026-11-02", Kind::BriefOffered, 8)).unwrap());
        assert_eq!(read(&file).len(), 2);
    }

    #[test]
    fn offers_of_one_brief_at_the_same_moment_log_one_line() {
        for _ in 0..20 {
            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join(LOG_FILE);
            let start = std::sync::Barrier::new(8);
            std::thread::scope(|scope| {
                for _ in 0..8 {
                    scope.spawn(|| {
                        start.wait();
                        append_once(&file, &brief("2026-11-01", Kind::BriefOffered, 7)).unwrap();
                    });
                }
            });
            assert_eq!(read(&file).len(), 1);
        }
    }

    #[test]
    fn brief_use_counts_offered_and_opened_among_merged_prs_only() {
        let events = [
            brief("2026-11-01", Kind::BriefOffered, 1),
            brief("2026-11-01", Kind::BriefOffered, 2),
            brief("2026-11-01", Kind::BriefOffered, 3),
            brief("2026-11-01", Kind::BriefOffered, 99),
            brief("2026-11-01", Kind::BriefOpened, 2),
            brief("2026-11-02", Kind::BriefOpened, 2),
            brief("2026-11-02", Kind::BriefOpened, 99),
            // Opened with no offer logged (the log began after the offer): not counted as having had a brief.
            brief("2026-11-02", Kind::BriefOpened, 4),
        ];
        let merged: BTreeSet<u64> = [1, 2, 3, 4].into();
        assert_eq!(brief_use(&events, "/p", &merged), (3, 1));
        assert_eq!(brief_use(&events, "/other", &merged), (0, 0));
    }

    #[test]
    fn chapters_done_are_counted_per_project_beside_knows() {
        let events = [
            brief("2026-11-01", Kind::ChapterDone, 1),
            brief("2026-11-01", Kind::ChapterDone, 1),
            event("2026-11-01", Kind::Know, Some("/p")),
            Event {
                project: Some("/q".into()),
                ..brief("2026-11-01", Kind::ChapterDone, 1)
            },
        ];
        let found = facts(&events, "/p", 0);
        assert_eq!((found.chapters_done, found.knows), (2, 1));
    }

    #[test]
    fn a_merge_subject_names_its_pr() {
        assert_eq!(merge_number("Studio: a thing (#12)"), Some(12));
        assert_eq!(merge_number("Merge pull request #13 from a/b"), Some(13));
        assert_eq!(merge_number("A plain commit"), None);
        assert_eq!(merge_number("Ends in a word (#x)"), None);
    }
}
