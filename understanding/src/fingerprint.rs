//! The worktree fingerprint (danielrispler/Falinks#328, module 5; danielrispler/Falinks#369, decision 10): `HEAD`, and the files `git status
//! --porcelain` lists in a worktree, with their content. The worktree changed when a file listed
//! before or now, or one a commit since changed, has other content, or now exists or is gone.
//! Ignored files are never listed, and a commit alone leaves every file's content as it was. A file
//! over 1 MiB is compared by size and modification time, and its diff is one line that names it, so
//! neither the record nor the card writer gets it. Only the agent's own commits count; a pull, a
//! rebase, a checkout, a reset or a merge moves the base instead (danielrispler/Falinks#434, decisions 12 to 14). Files
//! the operating system writes, such as `.DS_Store`, are never listed (danielrispler/Falinks#568).

use similar::TextDiff;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};
use studio_core::{Backfill, Base, CardRequest, Change, Fingerprint, Purpose};

/// A worktree's `HEAD`, and the files `git status` listed, by path from the worktree's root, with
/// their content: `None` for a file that is gone.
#[derive(Clone)]
struct Record {
    head: Option<String>,
    files: BTreeMap<String, Option<Content>>,
    /// The length in bytes of `HEAD`'s reflog, so the next change reads only the moves since.
    log: u64,
}

/// A file's content, or a large file's size and modification time.
#[derive(Clone, PartialEq)]
enum Content {
    Bytes(Vec<u8>),
    Large(u64, Option<SystemTime>),
}

/// The size above which a file counts as large: 1 MiB.
const LARGE: usize = 1 << 20;

/// The file at `path`, or `None` when it is gone.
fn read(path: &Path) -> Option<Content> {
    let meta = fs::metadata(path).ok()?;
    if meta.len() > LARGE as u64 {
        return Some(Content::Large(meta.len(), meta.modified().ok()));
    }
    fs::read(path).ok().map(Content::Bytes)
}

/// Takes the fingerprints `looks` asks for, each in its key's own `Fingerprints` in `prints`, and
/// returns the changes they found by key, as `Studio::apply_with_changes` takes them. A key's
/// changes in several worktrees join into one, with the first one's base (danielrispler/Falinks#434, decision 10); a
/// base's change is no one's, so it is left out.
pub fn fingerprint_changes(
    looks: Vec<Fingerprint>,
    prints: &mut HashMap<String, Fingerprints>,
) -> BTreeMap<String, Change> {
    let mut changes = BTreeMap::<String, Change>::new();
    for look in looks {
        let print = prints.entry(look.key.clone()).or_default();
        let found = print.look_since(Path::new(&look.folder), look.purpose, look.began);
        let Some(found) = found.filter(|_| look.purpose != Purpose::Base) else {
            continue;
        };
        match changes.get_mut(&look.key) {
            Some(joined) => joined.diff += &found.diff,
            None => _ = changes.insert(look.key, found),
        }
    }
    changes
}

/// The root of the git worktree that holds `folder`, spelled as `git` spells it (symlinks resolved, as
/// the hook's roots are), or `None` when it is in none. A folder that does not exist yet, such as a new file's, is looked up from its nearest
/// existing parent, unless that parent is a `worktrees` folder, where the folder of a removed
/// worktree was: only a listed worktree is a root (danielrispler/plotkeeper#205), as the hook's
/// rule has it.
pub fn worktree_root(folder: &str) -> Option<String> {
    let existing = Path::new(folder).ancestors().find(|p| p.is_dir())?;
    if existing != Path::new(folder) && existing.file_name().is_some_and(|n| n == "worktrees") {
        return None;
    }
    let root = git(existing, &["rev-parse", "--show-toplevel"])?;
    let root = String::from_utf8(root).ok()?;
    Some(root.trim_end().trim_end_matches('/').to_owned()).filter(|root| !root.is_empty())
}

/// `roots` without the entries that map a folder to another root than the one now holding it: a
/// folder in a removed worktree that names a listed root, or one a listed worktree has taken. An
/// entry that names a removed worktree as its own root stays (danielrispler/plotkeeper#205). `git`
/// lists each repository's worktrees once.
pub fn live_roots(roots: HashMap<String, String>) -> HashMap<String, String> {
    let mut lists: HashMap<String, Vec<String>> = HashMap::new();
    let mut roots = roots;
    roots.retain(|folder, root| {
        if !Path::new(root).join(".git").exists() {
            return true;
        }
        if !lists.contains_key(root) {
            let list = listed(Path::new(root));
            // Each worktree of the repository shares the one listing.
            let owners = list.iter().chain([&*root]);
            let owners: Vec<String> = owners.cloned().collect();
            lists.extend(owners.into_iter().map(|r| (r, list.clone())));
        }
        let list = &lists[root.as_str()];
        resolved(folder, list).as_ref() == Some(root)
    });
    roots
}

/// The roots `git worktree list` gives for the repository of the worktree at `root`.
fn listed(root: &Path) -> Vec<String> {
    let Some(out) = git(root, &["worktree", "list", "--porcelain"]) else {
        return Vec::new();
    };
    let text = String::from_utf8(out).unwrap_or_default();
    let roots = text.lines().filter_map(|l| l.strip_prefix("worktree "));
    roots.map(|r| r.trim_end_matches('/').to_owned()).collect()
}

/// The deepest of the listed `roots` that holds `folder`, looked up from its nearest existing
/// parent unless that parent is a `worktrees` folder, where the folder of a removed worktree was.
fn resolved(folder: &str, roots: &[String]) -> Option<String> {
    let existing = Path::new(folder).ancestors().find(|p| p.is_dir())?;
    if existing != Path::new(folder) && existing.file_name().is_some_and(|n| n == "worktrees") {
        return None;
    }
    let real = existing.canonicalize().ok()?;
    let at = real.to_str()?;
    let holds = |r: &&String| {
        at.strip_prefix(r.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    };
    roots.iter().filter(holds).max_by_key(|r| r.len()).cloned()
}

/// Each worktree's latest record, by the worktree's root.
#[derive(Default)]
pub struct Fingerprints {
    records: HashMap<PathBuf, Record>,
    /// Each folder's worktree root, once a lookup found one; a folder with none is asked again.
    roots: HashMap<PathBuf, PathBuf>,
    /// The Unix second of the first look, which a worktree made later counts from its creation
    /// (danielrispler/plotkeeper#204).
    began: Option<u64>,
}

impl Fingerprints {
    /// Records the worktree that holds `folder` and returns its change since its previous record,
    /// which is the change's base. `None` when nothing changed, when this is the worktree's first
    /// record, or when `folder` is in no git worktree.
    pub fn take(&mut self, folder: &Path) -> Option<Change> {
        self.look(folder, Purpose::Base)
    }

    /// Like `take`, for a look with `purpose`: the record becomes the next change's base unless
    /// it is a commit check and `HEAD` stayed.
    pub fn look(&mut self, folder: &Path, purpose: Purpose) -> Option<Change> {
        self.look_since(folder, purpose, None)
    }

    /// Like `look`, for the turn or worker that began at `began` (Unix milliseconds): a worktree made
    /// before then is no one's creation to count from. Without it, the first look's time is taken.
    pub fn look_since(
        &mut self,
        folder: &Path,
        purpose: Purpose,
        since: Option<u64>,
    ) -> Option<Change> {
        let root = match self.roots.get(folder) {
            Some(root) => root.clone(),
            None => {
                let root = git(folder, &["rev-parse", "--show-toplevel"])?;
                let root = PathBuf::from(String::from_utf8(root).ok()?.trim_end());
                self.roots.insert(folder.to_owned(), root.clone());
                root
            }
        };
        let head = git(&root, &["rev-parse", "HEAD"]);
        let head = head.and_then(|sha| Some(String::from_utf8(sha).ok()?.trim_end().to_owned()));
        // Untracked files one by one rather than their folders; a rename as a deletion and an addition.
        let status = [
            "status",
            "--porcelain",
            "-z",
            "--untracked-files=all",
            "--no-renames",
        ];
        // Each entry is `XY path`, with the path from the root.
        let files = split_paths(&git(&root, &status)?, 3)
            .map(|path| {
                let content = read(&root.join(&path));
                (path, content)
            })
            .collect();
        let log = reflog(&root).and_then(|log| fs::metadata(log).ok());
        let log = log.map_or(0, |meta| meta.len());
        let record = Record { head, files, log };
        let first = *self.began.get_or_insert_with(now);
        let began = since.map_or(first, |ms| ms / 1000);
        // A worktree made since the first look has no record: its base is the commit it was created at.
        let made = (!self.records.contains_key(&root))
            .then(|| created(&root).filter(|made| made.at > began))
            .flatten();
        let made = made.map(|made| Record {
            head: Some(made.commit),
            files: BTreeMap::new(),
            log: made.log,
        });
        let previous = self.records.get(&root).or(made.as_ref());
        let found = previous.and_then(|previous| change(&root, previous, &record));
        // A commit check keeps its record only when the agent committed since its base, or when
        // `HEAD` moved and nothing is left to show, so a step that only edits does not eat its card.
        let keep = purpose != Purpose::Commit
            || found.as_ref().map_or_else(
                || previous.is_none_or(|p| p.head != record.head),
                |c| c.head != c.base.head,
            );
        // A base that finds a worktree's edit since its creation keeps the creation's record, so the
        // card at the turn's or worker's end still sees the edit (danielrispler/plotkeeper#204).
        let record = match (purpose, &made, &found) {
            (Purpose::Base, Some(made), Some(_)) => made.clone(),
            _ => record,
        };
        if keep {
            self.records.insert(root, record);
        }
        found
    }
}

/// The Unix second now.
fn now() -> u64 {
    let since = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
    since.map_or(0, |since| since.as_secs())
}

/// The first line of a worktree's reflog: the commit it was created at, when, and how many bytes
/// the line takes.
struct Created {
    commit: String,
    at: u64,
    log: u64,
}

/// The first line of the reflog of `HEAD` in the worktree at `root`; `None` when it has none.
fn created(root: &Path) -> Option<Created> {
    let log = fs::read_to_string(reflog(root)?).ok()?;
    let line = log.split_inclusive('\n').next()?;
    // `<sha> <sha> <name> <<email>> <seconds> <zone>`, then a tab and the subject, when it has one.
    let who = line.split('\t').next()?.trim_end();
    let commit = who.split(' ').nth(1)?.into();
    let at = who.rsplit(' ').nth(1)?.parse().ok()?;
    Some(Created {
        commit,
        at,
        log: line.len() as u64,
    })
}

/// The path of the reflog of `HEAD` in the worktree at `root`.
pub(crate) fn reflog(root: &Path) -> Option<PathBuf> {
    let path = git(root, &["rev-parse", "--git-path", "logs/HEAD"])?;
    Some(root.join(String::from_utf8(path).ok()?.trim_end()))
}

/// The moves of `HEAD` in the worktree at `root` since its reflog was `from` bytes long, oldest
/// first: each one's sha before, sha after, and subject. Empty when the reflog was cut since.
fn moves(root: &Path, from: u64) -> Vec<(String, String, String)> {
    let log = reflog(root).and_then(|log| fs::read(log).ok());
    let new = log
        .as_deref()
        .and_then(|log| log.get(usize::try_from(from).ok()?..));
    let new = String::from_utf8_lossy(new.unwrap_or_default());
    let moves = new.lines().filter_map(|line| {
        let (who, subject) = line.split_once('\t')?;
        let mut shas = who.split(' ');
        let (old, new) = (shas.next()?.into(), shas.next()?.into());
        Some((old, new, subject.into()))
    });
    moves.collect()
}

/// Whether a reflog `subject` is the agent's own commit, not a move of `HEAD` that brings in other work.
pub(crate) fn is_own(subject: &str) -> bool {
    let own = ["commit:", "commit (amend):", "commit (initial):"];
    own.iter().any(|own| subject.starts_with(own))
}

/// The change in the worktree at `root` from record `previous`, its base, to `record`. Only the
/// agent's own commits count (danielrispler/Falinks#434, decisions 12 to 14): any other move of `HEAD` since the base
/// moves the base to where it left `HEAD`, keeping the change of the agent's commits before it, and
/// a conflicted merge adds its resolution.
fn change(root: &Path, previous: &Record, record: &Record) -> Option<Change> {
    let mut start = previous.head.clone();
    let mut diff = String::new();
    let mut moved = false;
    if previous.head != record.head {
        for (old, new, subject) in moves(root, previous.log) {
            if is_own(&subject) {
                continue;
            }
            let unborn = old.bytes().all(|byte| byte == b'0');
            if !unborn && start.as_deref() != Some(&old) {
                diff += &between(root, start.as_deref(), &old);
            }
            if subject.starts_with("commit (merge):") {
                let resolution = ["show", "--remerge-diff", "--format=", &new];
                diff += &String::from_utf8_lossy(&git(root, &resolution).unwrap_or_default());
            }
            start = Some(new);
            moved = true;
        }
    }
    // The files whose content in `previous` is no base: those a move changed, and on an unborn base,
    // every committed file, so the first commit is all of its files.
    let touched: BTreeSet<String> = match (&start, &record.head) {
        (Some(start), _) if moved => names(root, previous.head.as_deref(), start),
        (None, Some(now)) => names(root, None, now),
        _ => Vec::new(),
    }
    .into_iter()
    .collect();
    // A file edited and committed since lists in neither record.
    let committed = match &record.head {
        Some(now) if start.as_ref() != Some(now) => names(root, start.as_deref(), now),
        _ => Vec::new(),
    };
    let listed = previous.files.keys().chain(record.files.keys());
    let paths: BTreeSet<&String> = listed.chain(&committed).collect();
    let rest: String = paths
        .into_iter()
        .filter_map(|path| {
            // A file the previous record does not list was clean then: as its `HEAD` had it.
            let kept = previous.files.get(path).filter(|_| !touched.contains(path));
            let before = kept
                .cloned()
                .unwrap_or_else(|| show(root, start.as_deref(), path));
            let after = record.files.get(path).cloned();
            let after = after.unwrap_or_else(|| read(&root.join(path)));
            (before != after).then(|| unified(path, before, after))
        })
        .collect();
    diff += &rest;
    let base = Base {
        head: start,
        files: previous.files.keys().cloned().collect(),
    };
    let head = record.head.clone();
    (!diff.is_empty()).then_some(Change { base, diff, head })
}

/// The card request of a turn the app was down for (danielrispler/Falinks#486), with the diffs between its worktrees'
/// trees joined as its change; `None` when nothing differs.
pub fn backfilled(backfill: Backfill) -> Option<CardRequest> {
    let trees = backfill.trees.iter();
    let diff: String = trees
        .map(|t| between(Path::new(&t.root), Some(&t.start), &t.end))
        .collect();
    let request = CardRequest {
        change: Some(diff),
        ..backfill.request
    };
    (!request.diff().is_empty()).then_some(request)
}

/// The paths that differ between commits, or trees, `from` and `to`; every path of `to` when `from` is `None`.
pub(crate) fn names(root: &Path, from: Option<&str>, to: &str) -> Vec<String> {
    let out = match from {
        Some(from) => git(
            root,
            &["diff", "--name-only", "-z", "--no-renames", from, to],
        ),
        None => git(root, &["ls-tree", "-r", "--name-only", "-z", to]),
    };
    out.map_or_else(Vec::new, |out| split_paths(&out, 0).collect())
}

/// The file at `path` in commit `sha`, or `None` when it has none.
fn show(root: &Path, sha: Option<&str>, path: &str) -> Option<Content> {
    git(root, &["show", &format!("{}:{path}", sha?)]).map(Content::Bytes)
}

/// The change between commits `from` and `to`, file by file.
fn between(root: &Path, from: Option<&str>, to: &str) -> String {
    let names = names(root, from, to).into_iter();
    let changed = names.filter_map(|path| {
        let (before, after) = (show(root, from, &path), show(root, Some(to), &path));
        (before != after).then(|| unified(&path, before, after))
    });
    changed.collect()
}

/// The paths in `git`'s `-z` output, each after `skip` bytes, without the operating system's own
/// files, which are not the agent's work (danielrispler/Falinks#568).
fn split_paths(out: &[u8], skip: usize) -> impl Iterator<Item = String> {
    let entries = out.split(|&byte| byte == 0);
    let entries = entries.filter_map(move |entry| entry.get(skip..).filter(|p| !p.is_empty()));
    let paths = entries.map(|path| String::from_utf8_lossy(path).into_owned());
    paths.filter(|path| !os_file(path))
}

/// Whether `path` is a file the operating system writes: `.DS_Store`, `._*`, `Thumbs.db`, or one
/// in `.Spotlight-V100` or `.Trashes`.
fn os_file(path: &str) -> bool {
    path.split('/').any(|part| {
        matches!(
            part,
            ".DS_Store" | "Thumbs.db" | ".Spotlight-V100" | ".Trashes"
        ) || part.starts_with("._")
    })
}

/// `git`'s output in `folder`, or `None` when it fails. It takes no optional locks, so it never
/// holds up the agent's own `git` commands in the same worktree.
pub(crate) fn git(folder: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(args)
        .current_dir(folder)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

/// One file's change as a unified diff, as `git diff` shows it.
fn unified(path: &str, before: Option<Content>, after: Option<Content>) -> String {
    // A large committed file reaches here as its bytes, from `git show`.
    let bytes = |content: Option<Content>| match content {
        None => Some(Vec::new()),
        Some(Content::Bytes(bytes)) if bytes.len() <= LARGE => Some(bytes),
        Some(_) => None,
    };
    let (Some(before), Some(after)) = (bytes(before), bytes(after)) else {
        return format!("Large file {path} changed\n");
    };
    let (Ok(before), Ok(after)) = (str::from_utf8(&before), str::from_utf8(&after)) else {
        return format!("Binary file {path} changed\n");
    };
    TextDiff::from_lines(before, after)
        .unified_diff()
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}
