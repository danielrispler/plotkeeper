//! Writing a brief: the full one, the "Since you read" story, and when each is written.

use super::{Did, Push, Reading, Seen, carried, push_at};
use std::sync::Mutex;
use studio_understanding::pr_brief::{
    BriefStore, BriefWriter, PrSource, StoredBrief, build_facts, short_brief, write_pr_brief,
};

/// What writes and keeps the briefs: the writer, the model of the "PR briefs" setting and the store.
pub struct Writing<'a> {
    /// None without `claude`: a short brief, made by code, is still stored, and a brief that needs the writer fails.
    pub writer: Option<&'a dyn BriefWriter>,
    pub model: &'a str,
    pub store: &'a BriefStore,
    /// What a new head does to Daniel's read, and carrying it over (spec #477, part B); `None` treats every new head as today.
    pub reading: Option<&'a dyn Reading>,
}

/// Writes the "Since you read" story of the push from `from` to `full`'s head (#497), stored apart (`BriefStore::since`); its
/// failure is told to `log` and returned.
fn write_since(
    with: &Writing,
    full: &PrSource,
    from: &str,
    mut log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    let written = with
        .reading
        .ok_or_else(|| "no read to write a since story from".to_string())
        .and_then(|reading| reading.since_source(full, from))
        .and_then(|source| write_brief(with, &source, &mut log));
    if let Err(why) = &written {
        log(format!("write the since story: {why}"));
    }
    written
}

/// Opening PR `pr` at `head` when the push to it is the since case (#497): its stored "Since you read" story, or one written now
/// from `source`'s PR. A story written now is told to `landed`, so it opens, and when `landed` says to go on the full brief at
/// `head` is written after it, as today (`on_open`). `None`: this head has no story, and the full brief opens (`on_open`), as it
/// does when the story fails.
pub fn on_open_since(
    with: &Writing,
    seen: &Mutex<Seen>,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    landed: impl FnOnce() -> bool,
    log: impl FnMut(String),
) -> Option<Result<StoredBrief, String>> {
    if let Some((_, story)) = with.store.since(pr, head) {
        return Some(Ok(story));
    }
    let Some(Push::Since(from)) = push_at(with, seen, pr, head) else {
        return None;
    };
    let full = match source() {
        Ok(full) => full,
        Err(why) => return Some(Err(why)),
    };
    let mut log = log;
    let story = write_since(with, &full, &from, &mut log);
    if story.is_ok() && landed() {
        // A failure is kept in `seen` (the tray row says so) and told to the log; the story still opens.
        on_open(with, seen, pr, head, move || Ok(full), log).ok();
    }
    Some(story)
}

/// The "Since you read" story of PR `pr` at `head` was read to its end (#497): Daniel's read carries over from the head the story runs
/// from to `head`, every chapter's, the parts they light kept (`Reading::carry`).
pub fn since_read(
    store: &BriefStore,
    reading: &dyn Reading,
    pr: u64,
    head: &str,
) -> Result<(), String> {
    let (from, _) = store
        .since(pr, head)
        .ok_or("that since story is not stored")?;
    reading.carry(pr, &from, head)
}

/// The one call into the understanding crate that writes a brief.
pub fn write_brief(
    with: &Writing,
    source: &PrSource,
    log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    let writer = with
        .writer
        .ok_or("no writer: plotkeeper found no `claude` to ask")?;
    write_pr_brief(writer, with.model, source, with.store, log)
}

/// Whether a PR touches no part whose fog would show at Daniel's tiers (the facts' required parts): such a PR gets no brief.
pub(super) fn fogs_nothing(source: &PrSource) -> Result<bool, String> {
    Ok(build_facts(source)?.shape.required.is_empty())
}

/// Stores the short brief of a PR that fogs nothing (#644), made by code from its Map diff, with no writer call. `Ok(None)`: its
/// Map diff is empty, so it gets no brief.
fn store_short(with: &Writing, source: &PrSource) -> Result<Option<StoredBrief>, String> {
    let Some(short) = short_brief(source) else {
        return Ok(None);
    };
    with.store
        .put(&short)
        .map_err(|e| format!("keep the short brief of #{}: {e}", short.pr))?;
    Ok(Some(short))
}

/// Builds the source and writes; the failure of either is told to `log` and returned. Every brief of a PR's head is stored here: the
/// writer's, or, for a PR that fogs nothing but changes the map, the short brief. `Ok(None)`: the PR would fog nothing and its map did
/// not change.
fn write_now(
    with: &Writing,
    source: impl FnOnce() -> Result<PrSource, String>,
    mut log: impl FnMut(String),
) -> Result<Option<StoredBrief>, String> {
    let written = source().and_then(|source| {
        if fogs_nothing(&source)? {
            return store_short(with, &source);
        }
        write_brief(with, &source, &mut log).map(Some)
    });
    if let Err(why) = &written {
        log(format!("write the PR brief: {why}"));
    }
    written
}

/// One lookup of ready PR `pr` at `head`. `source` builds what the writer reads, and is only called when a write starts. `seen` is
/// locked only around its bookkeeping, never across the write, which takes minutes.
pub fn on_lookup(
    with: &Writing,
    seen: &Mutex<Seen>,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    log: impl FnMut(String),
) -> Did {
    let (again, skip) = {
        let mut seen = seen.lock().unwrap();
        let before = seen.heads.insert(pr, head.to_string());
        let skip = seen.is_quiet(pr, head) || seen.has_failed(pr, head);
        drop(seen);
        (before.as_deref() == Some(head), skip)
    };
    if skip || with.store.get(pr, head).is_some() {
        return Did::Kept;
    }
    let mut log = log;
    if carried(with, seen, pr, head, &mut log).is_some() {
        return Did::Carried;
    }
    if !again && let Some(old) = with.store.latest(pr) {
        return match with.store.mark_stale(pr, &old.head) {
            Ok(_) => Did::Staled,
            // Not kept in `seen.failed`: the next lookup of this head rewrites, as after a mark that worked.
            Err(e) => {
                log(format!("mark the brief stale: {e}"));
                Did::Failed
            }
        };
    }
    // The first brief of a ready PR is written at once; a stale one when the same head is seen twice. The write takes minutes and
    // holds no lock. In the since case the story is written first, since the tray opens it first (#497).
    let built = source();
    // The since story needs the writer; without one only the brief's own write is tried, which a short brief needs none for.
    if let Ok(full) = &built
        && with.writer.is_some()
        && with.store.since(pr, head).is_none()
        && let Some(Push::Since(from)) = push_at(with, seen, pr, head)
    {
        // `write_since` tells `log` its failure; the full brief is written either way.
        write_since(with, full, &from, &mut log).ok();
    }
    match write_now(with, move || built, log) {
        Ok(Some(_)) => Did::Wrote,
        Ok(None) => {
            seen.lock().unwrap().quiet.insert((pr, head.to_string()));
            Did::Quiet
        }
        Err(_) => {
            seen.lock().unwrap().failed.insert(pr, head.to_string());
            Did::Failed
        }
    }
}

/// Opening a PR's brief: the stored one when it is of the current head and not stale, else it is written now. Opening is Daniel
/// asking, so a write that failed at this head is tried again.
pub fn on_open(
    with: &Writing,
    seen: &Mutex<Seen>,
    pr: u64,
    head: &str,
    source: impl FnOnce() -> Result<PrSource, String>,
    log: impl FnMut(String),
) -> Result<StoredBrief, String> {
    match with.store.get(pr, head) {
        Some(stored) if !stored.stale => Ok(stored),
        _ => {
            let mut log = log;
            if let Some(copy) = carried(with, seen, pr, head, &mut log) {
                return Ok(copy);
            }
            seen.lock().unwrap().failed.remove(&pr);
            write_now(with, source, log)
                .inspect_err(|_| {
                    seen.lock().unwrap().failed.insert(pr, head.to_string());
                })?
                .ok_or_else(|| {
                    // The tray row becomes the quiet one, which does not open.
                    seen.lock().unwrap().quiet.insert((pr, head.to_string()));
                    "Nothing you track changed in this PR, so it has no brief.".into()
                })
        }
    }
}
