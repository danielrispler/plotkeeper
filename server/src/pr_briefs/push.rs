//! What a new head does to Daniel's read of a brief: the push since, by patch-ids, and its note.

use super::{Writing, fogs_nothing};
use crate::ratify::HeadCheck;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};
use studio_understanding::pr_brief::{MapCounts, PrSource, StoredBrief, gather, recount};

/// The heads the lookups saw last, per PR, for "the same head at two lookups in a row".
#[derive(Default)]
pub struct Seen {
    pub(super) heads: HashMap<u64, String>,
    /// The PRs and heads found to touch no part that would fog at Daniel's tiers: they get no brief and are not gathered again.
    pub(super) quiet: HashSet<(u64, String)>,
    /// The head whose write failed, per PR: it is not retried at every lookup, only when the head changes or Daniel opens the brief.
    pub(super) failed: HashMap<u64, String>,
    /// The PRs and heads whose brief needs the writer, which plotkeeper does not have (no `claude`): no failure, and not gathered
    /// again.
    pub(super) unwritten: HashSet<(u64, String)>,
    /// What the push to the head each PR is at did to Daniel's read (spec #477, part B), and whether that is settled: a settled
    /// verdict is worked out once, an unsettled one again at each ask. Kept in memory, as the rest of `Seen`: after a restart the
    /// tray's note is gone, while a carried read and its copied brief are on disk.
    pushes: HashMap<u64, (String, Push, bool)>,
}

impl Seen {
    pub fn is_quiet(&self, pr: u64, head: &str) -> bool {
        self.quiet.contains(&(pr, head.to_string()))
    }

    pub(super) fn has_failed(&self, pr: u64, head: &str) -> bool {
        self.failed.get(&pr).is_some_and(|h| h == head)
    }

    /// The tray's note for PR `pr` at `head`.
    pub(super) fn note(&self, pr: u64, head: &str) -> Option<String> {
        self.pushes
            .get(&pr)
            .filter(|(h, _, _)| h == head)
            .and_then(|(_, push, _)| push.note())
    }
}

/// What a new head H2 does to the brief Daniel read at H1 (spec #477, part B, "Since you read").
#[derive(Debug, Clone, PartialEq)]
pub enum Push {
    /// Fully read at H1, H2 descends from it, and H1..H2 fogs nothing: the read carries over.
    Quiet,
    /// Fully read at H1, which still resolves, and the old and new ranges hold the same patches: the read carries over.
    Rebased,
    /// Fully read at H1, and H2 only merged the default branch into it: the PR's own commits hold the same patches, so the read
    /// carries over.
    Merged,
    /// Fully read at H1 (held here), H2 descends from it, and H1..H2 fogs something: a "Since you read" story is written from the
    /// diff H1..H2 (#497), and the full brief at H2 as before.
    Since(String),
    /// The full brief is rewritten, with the note that says why; none when nothing of it was read.
    Rewrite(Option<String>),
}

impl Push {
    /// The note the tray row shows while the PR stays at H2.
    pub fn note(&self) -> Option<String> {
        match self {
            Push::Quiet => Some("New push touched nothing on your map".into()),
            Push::Rebased => Some("Rebased onto main, nothing new in the PR".into()),
            Push::Merged => Some("Main merged into the branch, nothing new in the PR".into()),
            Push::Since(_) => None,
            Push::Rewrite(note) => note.clone(),
        }
    }

    fn carries(&self) -> bool {
        matches!(self, Push::Quiet | Push::Rebased | Push::Merged)
    }
}

/// Daniel's read of the briefs, as a new head meets it.
pub trait Reading {
    /// What the push from `old`'s head to `head` does to the read of `old`, and whether that is settled (`push_of`).
    fn push(&self, old: &StoredBrief, head: &str) -> (Push, bool);
    /// Moves the read of PR `pr` from head `from` to `to` (`fog::carry_read`).
    fn carry(&self, pr: u64, from: &str, to: &str) -> Result<(), String>;
    /// What the since story of the push from `from` reads (#497): `full`'s PR, with the diff, map diff and tags from `from` to its head.
    fn since_source(&self, full: &PrSource, from: &str) -> Result<PrSource, String>;
    /// What PR `pr`'s head `head` says of its map (`ratify::head_check`, from its merge base with the default branch), for a brief
    /// carried to `head`; nothing when the base is unknown.
    fn head_check(&self, pr: u64, head: &str) -> HeadCheck;
    /// What `head`'s Map diff marks, from its merge base with the default branch (`map_diff::counts`), for a short brief carried to
    /// it (#644); none when its map did not change.
    fn map_counts(&self, head: &str) -> Option<MapCounts>;
}

/// The server's `Reading`: Daniel's state in `file`, `git` in `project`, its default branch `reference`, and the `gh` that says
/// whether a PR is by Daniel's own login (`prs::by_own_login_with`): his own in the server, a fake in the tests, which never run
/// the real one.
pub struct GitReading {
    pub project: String,
    pub file: std::path::PathBuf,
    pub reference: Option<String>,
    pub gh: String,
}

impl Reading for GitReading {
    fn push(&self, old: &StoredBrief, head: &str) -> (Push, bool) {
        let user = crate::map::read_user(&self.file, &self.project);
        let read = user.fog.reads.get(&old.pr);
        let Some(reference) = &self.reference else {
            let settled = read.is_some_and(|r| r.fully_read_at(&old.head));
            let note = partial_note(read, &old.head).or(Some(REBASED.into()));
            return (Push::Rewrite(note), settled);
        };
        push_of(
            Path::new(&self.project),
            reference,
            read,
            &user.tiers,
            old.pr,
            &old.head,
            head,
        )
    }

    fn carry(&self, pr: u64, from: &str, to: &str) -> Result<(), String> {
        crate::fog::carry_read(&self.file, &self.project, pr, from, to)
    }

    fn since_source(&self, full: &PrSource, from: &str) -> Result<PrSource, String> {
        let repo = Path::new(&self.project);
        let passed = crate::map::uncommitted_map(repo);
        let mut source = gather(
            repo,
            from,
            &full.meta.head,
            full.meta.clone(),
            passed.as_deref(),
        )?;
        source.tier_overrides = full.tier_overrides.clone();
        source.earlier_decisions = full.earlier_decisions.clone();
        // The same head as the full brief's, so the same map check (#641).
        source.map_problems = full.map_problems.clone();
        source.since = Some(from.to_string());
        Ok(source)
    }

    fn head_check(&self, pr: u64, head: &str) -> HeadCheck {
        let repo = Path::new(&self.project);
        let Some(base) = self
            .reference
            .as_deref()
            .and_then(|r| crate::map::git(repo, &["merge-base", r, head]).ok())
        else {
            return HeadCheck::default();
        };
        let trusted = crate::prs::by_own_login_with(&self.gh, repo, pr);
        crate::ratify::head_check(repo, base.trim(), head, trusted)
    }

    fn map_counts(&self, head: &str) -> Option<MapCounts> {
        let repo = Path::new(&self.project);
        let base = crate::map::git(repo, &["merge-base", self.reference.as_deref()?, head]).ok()?;
        crate::map_diff::counts(repo, base.trim(), head)
    }
}

/// The note of a rewrite after Daniel had read only part of the brief at `old`, or none when he had read none of it.
fn partial_note(read: Option<&crate::fog::PrRead>, old: &str) -> Option<String> {
    let read = read?;
    let n = read.chapters.values().filter(|c| c.head == old).count();
    (n > 0).then(|| format!("{PUSHED}; you had read {n} of {}", read.lit.len()))
}

pub(super) const REBASED: &str = "Rewritten: the branch was rebased";
const PUSHED: &str = "Rewritten after a new push";
pub(super) const MERGED: &str = "Rewritten: main was merged into the branch";

/// What the push of PR `pr` from `old` to `new` does to `read`, by `git` in `repo`: `reference` is the default branch the
/// patch-ids' ranges start from, `tiers` Daniel's, which decide whether H1..H2 fogs something. With it, whether the verdict is
/// settled: one from a brief not fully read, or from git failing, can change while the PR stays at `new`, so it is worked out again.
pub fn push_of(
    repo: &Path,
    reference: &str,
    read: Option<&crate::fog::PrRead>,
    tiers: &std::collections::BTreeMap<String, String>,
    pr: u64,
    old: &str,
    new: &str,
) -> (Push, bool) {
    if !read.is_some_and(|r| r.fully_read_at(old)) {
        return (Push::Rewrite(partial_note(read, old)), false);
    }
    let git = |args: &[&str]| crate::map::git(repo, args);
    if git(&["cat-file", "-e", &format!("{old}^{{commit}}")]).is_err() {
        return (Push::Rewrite(Some(REBASED.into())), true);
    }
    // The PR's own commits at `old` and `new`: the same patches carry the read as `same`; unsettled when git cannot tell.
    let compare = |same: Push, note: &str| match (
        patches(repo, reference, old),
        patches(repo, reference, new),
    ) {
        (Some(before), Some(after)) if before == after => (same, true),
        (Some(_), Some(_)) => (Push::Rewrite(Some(note.into())), true),
        _ => (Push::Rewrite(Some(note.into())), false),
    };
    if git(&["merge-base", "--is-ancestor", old, new]).is_ok() {
        // The default branch merged into the PR: H1..H2 holds main's changes, so the PR's own commits are compared instead.
        let base = |head: &str| git(&["merge-base", reference, head]).ok();
        if base(old) != base(new) {
            // A merge commit's own edits are in no non-merge patch: the PR's whole diff must match too.
            return match compare(Push::Merged, MERGED) {
                (Push::Merged, true) => match (
                    whole_patch(repo, reference, old),
                    whole_patch(repo, reference, new),
                ) {
                    (Some(before), Some(after)) if before == after => (Push::Merged, true),
                    (Some(_), Some(_)) => (Push::Rewrite(Some(MERGED.into())), true),
                    _ => (Push::Rewrite(Some(MERGED.into())), false),
                },
                other => other,
            };
        }
        let meta = studio_understanding::pr_brief::PrMeta {
            number: pr,
            head: new.to_string(),
            ..Default::default()
        };
        // A repository that never committed a map is placed by the one in its checkout or pending PR, as its brief is.
        let passed = crate::map::uncommitted_map(repo);
        return match gather(repo, old, new, meta, passed.as_deref()).and_then(|mut source| {
            source.tier_overrides = tiers.clone();
            fogs_nothing(&source)
        }) {
            Ok(true) => (Push::Quiet, true),
            Ok(false) => (Push::Since(old.to_string()), true),
            Err(_) => (Push::Rewrite(Some(PUSHED.into())), false),
        };
    }
    compare(Push::Rebased, REBASED)
}

/// The PR's commits from the merge base with `reference` to `head`, merges left out: their count, and the multiset of their
/// `git patch-id --stable`, sorted. A commit with an empty diff has no patch-id, so the count is kept apart.
fn patches(repo: &Path, reference: &str, head: &str) -> Option<(usize, Vec<String>)> {
    let base = crate::map::git(repo, &["merge-base", reference, head]).ok()?;
    let range = format!("{base}..{head}");
    let count = crate::map::git(repo, &["rev-list", "--count", "--no-merges", &range])
        .ok()?
        .parse()
        .ok()?;
    let mut ids = patch_ids(
        repo,
        &[
            "log",
            "-p",
            "--no-merges",
            "--no-color",
            "--no-ext-diff",
            &range,
        ],
    )?;
    ids.sort();
    Some((count, ids))
}

/// The `git patch-id --stable` of the PR's whole diff, from its merge base with `reference` to `head`; "" when the diff is empty.
fn whole_patch(repo: &Path, reference: &str, head: &str) -> Option<String> {
    let base = crate::map::git(repo, &["merge-base", reference, head]).ok()?;
    let ids = patch_ids(repo, &["diff", "--no-color", "--no-ext-diff", &base, head])?;
    Some(ids.into_iter().next().unwrap_or_default())
}

/// The patch-ids `git patch-id --stable` reads from the output of `git args`, in order.
fn patch_ids(repo: &Path, args: &[&str]) -> Option<Vec<String>> {
    use studio_workbench::run::{git, run_until, run_until_with_input};
    let limit = crate::map::GIT_READ_LIMIT;
    let log = run_until(git(repo).args(args), limit).ok()?;
    let mut ids = git(repo);
    let out = run_until_with_input(ids.args(["patch-id", "--stable"]), &log.stdout, limit).ok()?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| line.split_whitespace().next().map(String::from))
            .collect(),
    )
}

/// When the PR moved from the newest stored brief's head to `head`, what that push does to Daniel's read, kept in
/// `seen` (for the tray's note, and for the since story once the full brief at `head` is stored too): worked out once per head when
/// settled, else again (`push_of`).
pub(super) fn push_at(with: &Writing, seen: &Mutex<Seen>, pr: u64, head: &str) -> Option<Push> {
    let known = seen
        .lock()
        .unwrap()
        .pushes
        .get(&pr)
        .filter(|(h, _, settled)| h == head && *settled)
        .map(|(_, p, _)| p.clone());
    if known.is_some() {
        return known;
    }
    let reading = with.reading?;
    let old = with.store.latest(pr).filter(|old| old.head != head)?;
    let (push, settled) = reading.push(&old, head);
    seen.lock()
        .unwrap()
        .pushes
        .insert(pr, (head.to_string(), push.clone(), settled));
    Some(push)
}

/// A push that carries the read over (`push_at`) moves it and copies the brief to `head`, which is returned. `None`: the brief
/// is rewritten, as before.
pub(super) fn carried(
    with: &Writing,
    seen: &Mutex<Seen>,
    pr: u64,
    head: &str,
    mut log: impl FnMut(String),
) -> Option<StoredBrief> {
    let reading = with.reading?;
    if !push_at(with, seen, pr, head)?.carries() {
        return None;
    }
    let old = with.store.latest(pr).filter(|old| old.head != head)?;
    // The read moves first: a copy whose read did not move would be a brief to read again. Its link evidence and map check are
    // read again at `head`, since a rebase or a quiet push can move an import site, remove its file or break the map (#499, #641).
    let check = reading.head_check(pr, head);
    let mut copy = StoredBrief {
        head: head.to_string(),
        stale: false,
        written_at: studio_understanding::millis().max(old.written_at + 1),
        link_evidence: check.link_evidence,
        map_problems: check.map_problems,
        ..old.clone()
    };
    // A short brief counts the new head's Map diff (#644); one whose map no longer changes is no brief, so it is not carried.
    if copy.short {
        recount(&mut copy, &reading.map_counts(head)?);
    }
    match reading.carry(pr, &old.head, head).and_then(|()| {
        with.store
            .put(&copy)
            .map_err(|e| format!("keep the brief of #{pr}: {e}"))
    }) {
        Ok(()) => Some(copy),
        Err(why) => {
            log(format!("carry the read over: {why}"));
            None
        }
    }
}
