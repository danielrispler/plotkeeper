//! The commands and lookups the server answers about briefs, by project.

use super::{
    BusyGuard, Did, GitReading, Opened, Reading, Writing, lit_parts, on_lookup, on_open,
    on_open_since, since_read, since_view, view_of,
};
use crate::App;
use crate::prs::Ready;
use serde::Serialize;
use std::{path::Path, sync::Arc};
use studio_understanding::pr_brief::{
    BriefStore, BriefWriter, PrMeta, PrSource, fetch_meta, gather,
};

/// The numbers of the ready PRs a brief is waiting for: the brief is stored and Daniel has not read all of it at the PR's current
/// head. A brief read to its end stops counting for the project header's dot and the rail's row (its tray row stays, to read again), and a push
/// after the reading counts it again.
pub fn waiting_numbers(
    store: &BriefStore,
    ready: &[Ready],
    user: Option<&crate::map::UserState>,
) -> Vec<u64> {
    ready
        .iter()
        .filter(|pr| store.latest(pr.number).is_some())
        .filter(|pr| !user.is_some_and(|u| crate::fog::fully_read_at(u, pr.number, &pr.head)))
        .map(|pr| pr.number)
        .collect()
}

/// Daniel's read of `project`'s briefs, as the lookups and opens meet a new head; `None` without a state file.
fn reading_of(project: &str) -> Option<GitReading> {
    Some(GitReading {
        project: project.to_string(),
        file: crate::map::state_file().ok()?,
        reference: crate::map::default_ref(Path::new(project)).map(|(r, _)| r),
        gh: studio_workbench::gh_path().to_string(),
    })
}

fn user_of(project: &str) -> Option<crate::map::UserState> {
    crate::map::state_file()
        .ok()
        .map(|file| crate::map::read_user(&file, project))
}

/// Whether a brief waits for one of `project`'s ready PRs: the project header's quiet dot lights for it.
pub fn any_waiting(project: &str, ready: &[Ready]) -> bool {
    store_of(project)
        .is_some_and(|store| !waiting_numbers(&store, ready, user_of(project).as_ref()).is_empty())
}

/// The ready PRs of the project holding `cwd` that a brief waits for, for the rail's row in the session of one of them.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Waiting {
    pub project: String,
    pub numbers: Vec<u64>,
}

pub fn briefs_waiting(app: &App, cwd: String) -> Option<Waiting> {
    let project = studio_workbench::main_checkout(Path::new(&cwd)).ok()?;
    let ready = app.prs.ready(&project);
    let user = user_of(&project);
    let numbers = store_of(&project).map(|s| waiting_numbers(&s, &ready, user.as_ref()))?;
    Some(Waiting { project, numbers })
}

/// `project`'s own path under the data folder's `kind` folder: its path with each character but a letter or digit made `_`.
pub(crate) fn project_path(kind: &str, project: &str) -> Option<std::path::PathBuf> {
    let home = std::env::home_dir()?;
    let name: String = project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    Some(studio_hook::data_dir(&home).join(kind).join(name))
}

/// A project's briefs live in a folder of their own, since PR numbers repeat across repositories.
pub(crate) fn store_of(project: &str) -> Option<BriefStore> {
    BriefStore::open(&project_path("pr-briefs-by-project", project)?).ok()
}

/// When PR head `head` was committed, in Unix milliseconds, when it is in `project`'s repository.
fn head_time_ms(project: &str, head: &str) -> Option<u64> {
    let at = crate::map::git(Path::new(project), &["show", "-s", "--format=%ct", head]).ok()?;
    Some(at.trim().parse::<u64>().ok()? * 1000)
}

/// What the writer reads for PR `number` at its head, gathered with `gh` and `git` in `project`.
fn source_of(project: &str, number: u64) -> Result<PrSource, String> {
    let repo = Path::new(project);
    let meta = fetch_meta(repo, number)?;
    crate::pre_flight::note_closing(project, &meta);
    let trusted = crate::prs::by_own_login(repo, number);
    let mut source = gathered(project, meta, trusted)?;
    if let Ok(file) = crate::map::state_file() {
        source.tier_overrides = crate::map::read_user(&file, project).tiers;
    }
    source.earlier_decisions = store_of(project)
        .and_then(|s| s.latest(number))
        .map(|b| b.brief.decisions)
        .unwrap_or_default();
    Ok(source)
}

/// What `git` gives of `meta`'s PR in `project`, from its merge base with the default branch to its head: the diff and map facts
/// (`gather`), what the head says of its map (`ratify::head_check`, `trusted` when the PR is by Daniel's own login), and what its Map
/// diff marks (`map_diff::counts`), which a PR that fogs nothing gets a short brief from (#644).
pub(super) fn gathered(project: &str, meta: PrMeta, trusted: bool) -> Result<PrSource, String> {
    let repo = Path::new(project);
    let (reference, _) =
        crate::map::default_ref(repo).ok_or("the repository has no default branch")?;
    let base = crate::map::git(repo, &["merge-base", &reference, &meta.head])?;
    let head = meta.head.clone();
    let mut source = gather(repo, &base, &head, meta)?;
    // Read from the head when the brief is written, never live in Ratify (#499, #641).
    let check = crate::ratify::head_check(repo, &base, &head, trusted);
    source.link_evidence = check.link_evidence;
    source.map_problems = check.map_problems;
    source.map_counts = crate::map_diff::counts(repo, &base, &head);
    Ok(source)
}

/// Whether a brief of `pr`'s head is stored and not stale.
pub(super) fn has_fresh_brief(store: &BriefStore, pr: &Ready) -> bool {
    store.get(pr.number, &pr.head).is_some_and(|b| !b.stale)
}

/// Looks at each ready PR of `project` after a lookup of `prs.rs`: the first brief is written at once, a new head stales it. Each
/// PR goes on a thread of its own, since a write takes minutes; the Map view is told (`map-landed`) when a brief changed. At the
/// first lookup after start only the PRs `Startup` picks are written; the rest are written when opened.
pub fn lookup_later(app: &Arc<App>, project: &str, ready: Vec<Ready>) {
    let Some(store) = store_of(project) else {
        return;
    };
    let allowed = app.briefs.startup.lock().unwrap().allowed(
        project,
        app.prs.listed(project),
        &ready,
        &|pr| head_time_ms(project, &pr.head),
        studio_understanding::millis(),
        crate::settings::brief_backfill().window_ms,
        &|pr| has_fresh_brief(&store, pr),
    );
    // A new head cancels the map text review of an older one.
    for pr in &ready {
        app.reviews.head_now(project, pr.number, &pr.head);
    }
    for pr in ready.into_iter().filter(|pr| allowed.contains(&pr.number)) {
        let (app, project) = (app.clone(), project.to_string());
        std::thread::spawn(move || {
            let briefs = &app.briefs;
            // Without `claude` a PR that changes the map but fogs nothing still gets its short brief and its head's map check.
            let writer = briefs.writer.clone();
            let Some(store) = store_of(&project) else {
                return;
            };
            let Some(claim) = briefs.claim(&project, pr.number) else {
                return;
            };
            let model = briefs.model();
            let seen = briefs.seen_of(&project);
            let reading = reading_of(&project);
            let with = Writing {
                writer: writer.as_deref().map(|w| w as &dyn BriefWriter),
                model: &model,
                store: &store,
                reading: reading.as_ref().map(|r| r as &dyn Reading),
            };
            let did = on_lookup(
                &with,
                &seen,
                pr.number,
                &pr.head,
                || {
                    // A write starts: the tray says so.
                    app.events
                        .emit("map-landed", serde_json::json!(project.clone()));
                    source_of(&project, pr.number)
                },
                |line| log::warn!("pr brief of #{}: {line}", pr.number),
            );
            drop(claim);
            if did != Did::Kept {
                app.events
                    .emit("map-landed", serde_json::json!(project.clone()));
            }
            // A brief stored at this head may start its map text review.
            if matches!(did, Did::Wrote | Did::Carried) {
                app.reviews.brief_stored(&project, pr.number, &pr.head);
            }
        });
    }
}

/// Opening a PR's brief from the tray: the stored one, or, when it is stale or not written, the write starts in the background and
/// the answer says it is being written. A merged PR's brief is read as stored. An open PR whose head has a "Since you read" story
/// (#497) opens it first, written first in the since case, unless `full` asks for the full brief.
pub fn pr_brief(
    app: &Arc<App>,
    project: String,
    number: u64,
    full: bool,
) -> Result<Option<Opened>, String> {
    let briefs = &app.briefs;
    let store = store_of(&project).ok_or("no data folder")?;
    let head = app
        .prs
        .ready(&project)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| r.head);
    let Some(head) = head else {
        return Ok(store
            .latest(number)
            .map(|b| Opened::Brief(Box::new(view_of(&project, &store, b, false)))));
    };
    if !full && let Some((from, story)) = store.since(number, &head) {
        return Ok(Some(Opened::Brief(Box::new(since_view(from, story)))));
    }
    if let Some(stored) = store.get(number, &head).filter(|b| !b.stale) {
        return Ok(Some(Opened::Brief(Box::new(view_of(
            &project, &store, stored, true,
        )))));
    }
    let writer = briefs.writer.clone();
    // Without `claude` a full brief cannot be rewritten, so the latest one opens; a short brief, made by code, is written as ever.
    if writer.is_none()
        && let Some(latest) = store.latest(number).filter(|b| !b.short)
    {
        return Ok(Some(Opened::Brief(Box::new(view_of(
            &project, &store, latest, true,
        )))));
    }
    let write = {
        let (app, project) = (app.clone(), project.clone());
        move |claim: BusyGuard| {
            let briefs = &app.briefs;
            let model = briefs.model();
            let seen = briefs.seen_of(&project);
            let reading = reading_of(&project);
            let with = Writing {
                writer: writer.as_deref().map(|w| w as &dyn BriefWriter),
                model: &model,
                store: &store,
                reading: reading.as_ref().map(|r| r as &dyn Reading),
            };
            let log = |line: String| log::warn!("pr brief of #{number}: {line}");
            // In the since case the story is written, and opens; the full brief is written after it, under a claim of its own so
            // the tray stops saying the story is written. When the story fails, the full brief is written instead.
            let mut claim = Some(claim);
            let story = (!full)
                .then(|| {
                    on_open_since(
                        &with,
                        &seen,
                        number,
                        &head,
                        || source_of(&project, number),
                        || {
                            drop(claim.take());
                            app.events
                                .emit("map-landed", serde_json::json!(project.clone()));
                            claim = briefs.claim(&project, number);
                            claim.is_some()
                        },
                        log,
                    )
                })
                .flatten();
            // A failure is kept in `seen` (the tray row says so) and told to the log.
            if !matches!(story, Some(Ok(_))) {
                on_open(
                    &with,
                    &seen,
                    number,
                    &head,
                    || source_of(&project, number),
                    log,
                )
                .ok();
            }
            drop(claim);
            app.events
                .emit("map-landed", serde_json::json!(project.clone()));
            // A brief stored at this head may start its map text review.
            if store.get(number, &head).is_some() {
                app.reviews.brief_stored(&project, number, &head);
            }
        }
    };
    let opened = briefs.open_in_background(&project, number, write);
    // The tray says the brief is being written.
    app.events
        .emit("map-landed", serde_json::json!(project.clone()));
    Ok(Some(opened))
}

/// Guard with a law's new law (#401): the PR briefs' writer, which has no tools, drafts one from PR `number`'s stored brief, on a part
/// it touched, for Daniel to confirm or edit. The PR's text goes only to that writer; the run gets what Daniel confirms.
pub fn guard_draft(
    app: &App,
    project: String,
    number: u64,
) -> Result<crate::back_a_law::NewLaw, String> {
    let writer = app
        .briefs
        .writer()
        .ok_or("no writer: plotkeeper found no `claude` to ask")?;
    let brief = store_of(&project)
        .and_then(|store| store.latest(number))
        .filter(|brief| brief.guard.is_some())
        .ok_or("that brief offers no Guard with a law")?;
    let map_text = crate::map::read_map(Path::new(&project))
        .source
        .ok_or("this project has no map")?;
    crate::back_a_law::draft(
        writer.as_ref(),
        &crate::settings::pr_briefs_model(),
        &map_text,
        &brief,
    )
}

/// PR `number`'s "Since you read" story at `head` was read to its end (#497): the read carries over to `head` (`since_read`).
pub fn brief_since_done(
    app: &App,
    project: String,
    number: u64,
    head: String,
) -> Result<(), String> {
    let store = store_of(&project).ok_or("no data folder")?;
    let reading = reading_of(&project).ok_or("no state file")?;
    since_read(&store, &reading, number, &head)?;
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(())
}

/// Chapter `chapter` of PR `number`'s brief was read to its end (the page saw its last step, focused, and the check was answered): the
/// read is kept as "read at head", and the fog the PR's merged commits filed leaves every part whose chapters are all read.
pub fn brief_chapter_done(
    app: &App,
    project: String,
    number: u64,
    chapter: usize,
    head: String,
) -> Result<(), String> {
    let stored = store_of(&project)
        .and_then(|store| store.get(number, &head))
        .ok_or("that brief is not stored")?;
    let lit = lit_parts(&stored.brief);
    let file = crate::map::state_file()?;
    let at = studio_workbench::clock::now_ms() / 1000;
    let done = crate::fog::mark_read(&file, &project, number, chapter, &head, lit, at)?;
    crate::fog::clear_pr(Path::new(&project), &file, &project, number, &done)?;
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(())
}
