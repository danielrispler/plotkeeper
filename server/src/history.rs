//! A part's history on the Map view (spec danielrispler/plotkeeper#306, ticket #313): the first-parent commits on the default
//! branch, from the one that introduced `system-map.json` onward, that touched the part; the map as of any such commit, read
//! leniently so a field of another time never breaks it; and a commit's files and diff for the drawer.
//!
//! Scrubbing is the window's `scrubTo` (`ui/map-history.ts`); the shell side of one scrub is `map_at` (`at`), the one place a
//! later ticket hooks to see that Daniel looked back.

use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use crate::map::{default_ref, git};

/// One commit of a part's history. `fogged` marks a commit Daniel has not caught up on (fog), so the strip shows it apart.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Entry {
    pub commit: String,
    pub short: String,
    pub label: String,
    pub fogged: bool,
    /// The PR whose stored brief this commit offers (`map_history` fills it).
    pub brief: Option<u64>,
}

/// The PR number a strip label names: `12` for `#12 Title`, none for any other label.
pub fn pr_of_label(label: &str) -> Option<u64> {
    label.strip_prefix('#')?.split(' ').next()?.parse().ok()
}

/// The map as of a commit: its text as the window parses it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct At {
    pub commit: String,
    pub short: String,
    pub label: String,
    pub source: String,
}

/// One file of a commit with its diff.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct File {
    pub path: String,
    pub diff: String,
}

/// A commit's files and diffs, and where it is on GitHub when `origin` is.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Commit {
    pub commit: String,
    pub short: String,
    pub label: String,
    pub url: Option<String>,
    pub files: Vec<File>,
}

/// A commit subject as the strip shows it: `#12 Title` for a squash-merge subject `Title (#12)`, else the subject.
pub fn label(subject: &str) -> String {
    let subject = subject.trim();
    if let Some(rest) = subject.strip_suffix(')')
        && let Some((title, number)) = rest.rsplit_once(" (#")
        && !title.is_empty()
        && !number.is_empty()
        && number.bytes().all(|b| b.is_ascii_digit())
    {
        return format!("#{number} {title}");
    }
    subject.to_string()
}

/// `https://github.com/<owner>/<repo>/commit/<sha>` for a GitHub remote url, in either of its spellings.
pub fn github_commit_url(remote: &str, commit: &str) -> Option<String> {
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    path.split('/')
        .filter(|part| !part.is_empty())
        .count()
        .eq(&2)
        .then(|| format!("https://github.com/{path}/commit/{commit}"))
}

/// The empty tree, which the first commit of a repository is diffed against.
const NOTHING: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

fn parent_of(repo: &Path, commit: &str) -> String {
    let parent = format!("{commit}^");
    git(repo, &["rev-parse", "--verify", "-q", &parent]).unwrap_or_else(|_| NOTHING.to_string())
}

fn map_text(repo: &Path, commit: &str) -> Option<String> {
    git(
        repo,
        &["show", &format!("{commit}:{}", system_map::MAP_FILE)],
    )
    .ok()
}

fn has_part(text: Option<&str>, part: &str) -> bool {
    text.and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .is_some_and(|map| {
            map["parts"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|p| p["id"] == part))
        })
}

/// Whether `commit` touched `part`: a file the part owns in either the map before or the map after changed, or the part was
/// added or removed in the map.
fn touches(repo: &Path, commit: &str, part: &str) -> bool {
    let parent = parent_of(repo, commit);
    let (old, new) = (map_text(repo, &parent), map_text(repo, commit));
    if has_part(old.as_deref(), part) != has_part(new.as_deref(), part) {
        return true;
    }
    let Ok(changed) = git(
        repo,
        &["diff", "--no-renames", "--name-only", &parent, commit],
    ) else {
        return false;
    };
    let changed: Vec<String> = changed.lines().map(String::from).collect();
    [old, new]
        .iter()
        .flatten()
        .filter_map(|text| system_map::parse_lenient(text).ok())
        .any(|map| {
            system_map::touched_for_map(&map, &changed)
                .parts
                .iter()
                .any(|p| p.id == part)
        })
}

/// The first-parent commits of `repo`'s default branch that touched `part`, oldest to newest, from the commit that introduced
/// `system-map.json`; `fogged` holds the commits Daniel has not caught up on.
pub fn part_history(repo: &Path, part: &str, fogged: &HashSet<String>) -> Vec<Entry> {
    let Some((reference, _)) = default_ref(repo) else {
        return Vec::new();
    };
    let Ok(born) = git(
        repo,
        &[
            "rev-list",
            "--first-parent",
            "--reverse",
            &reference,
            "--",
            system_map::MAP_FILE,
        ],
    ) else {
        return Vec::new();
    };
    let Some(born) = born.lines().next() else {
        return Vec::new();
    };
    let Ok(log) = git(
        repo,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H%x1f%s",
            &reference,
        ],
    ) else {
        return Vec::new();
    };
    log.lines()
        .skip_while(|line| !line.starts_with(born))
        .filter_map(|line| line.split_once('\u{1f}'))
        .filter(|(commit, _)| touches(repo, commit, part))
        .map(|(commit, subject)| Entry {
            commit: commit.to_string(),
            short: commit.chars().take(7).collect(),
            label: label(subject),
            fogged: fogged.contains(commit),
            brief: None,
        })
        .collect()
}

/// When each part id of `wanted` first appeared in `system-map.json` on `repo`'s default branch: the committer time (unix seconds) of
/// the first first-parent commit whose map lists it. Only `parts[].id` is read, so a map of any version counts, and a commit whose map
/// does not parse is passed over. An id never seen is left out; none when git's history cannot be read.
pub fn part_births(repo: &Path, wanted: &[String]) -> Option<BTreeMap<String, u64>> {
    let mut born = BTreeMap::new();
    let (reference, _) = default_ref(repo)?;
    let log = git(
        repo,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H %ct",
            &reference,
            "--",
            system_map::MAP_FILE,
        ],
    )
    .ok()?;
    for (commit, at) in log.lines().filter_map(|line| line.split_once(' ')) {
        if wanted.iter().all(|id| born.contains_key(id)) {
            break;
        }
        let (Ok(at), Some(text)) = (at.parse::<u64>(), map_text(repo, commit)) else {
            continue;
        };
        let Ok(map) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        for id in map["parts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["id"].as_str())
        {
            if wanted.iter().any(|w| w == id) {
                born.entry(id.to_string()).or_insert(at);
            }
        }
    }
    Some(born)
}

fn subject_of(repo: &Path, commit: &str) -> Result<(String, String), String> {
    let full = git(
        repo,
        &["rev-parse", "--verify", &format!("{commit}^{{commit}}")],
    )?;
    let subject = git(repo, &["show", "-s", "--format=%s", &full])?;
    Ok((full, label(&subject)))
}

/// The map as of `commit`, read leniently. This is the one place a scrub reaches the shell.
pub fn at(repo: &Path, commit: &str) -> Result<At, String> {
    let (full, label) = subject_of(repo, commit)?;
    let source = git(repo, &["show", &format!("{full}:{}", system_map::MAP_FILE)])
        .map_err(|e| format!("{} has no map at {}: {e}", system_map::MAP_FILE, &full[..7]))?;
    // A map from before version 2 is sent converted, as `read_map` stores it.
    let source = system_map::convert(&source)?;
    system_map::parse_lenient(&source)?;
    Ok(At {
        short: full.chars().take(7).collect(),
        commit: full,
        label,
        source,
    })
}

/// The most lines of one file's diff the drawer holds.
const DIFF_LINES: usize = 400;
/// The most files of a commit the drawer lists.
const FILES: usize = 80;

/// The files that differ between `from` and `to`, each with its diff cut to what the drawer holds. `keep` picks among all their paths
/// before the cap of [`FILES`], so a file it keeps is never cut for sorting late.
fn files_between(
    repo: &Path,
    from: &str,
    to: &str,
    keep: impl FnOnce(Vec<String>) -> Vec<String>,
) -> Result<Vec<File>, String> {
    let names = git(repo, &["diff", "--no-renames", "--name-only", from, to])?;
    Ok(keep(names.lines().map(String::from).collect())
        .iter()
        .take(FILES)
        .map(|path| {
            let diff =
                git(repo, &["diff", "--no-renames", from, to, "--", path]).unwrap_or_default();
            let lines: Vec<&str> = diff.lines().collect();
            let diff = if lines.len() > DIFF_LINES {
                format!(
                    "{}\n… {} more lines",
                    lines[..DIFF_LINES].join("\n"),
                    lines.len() - DIFF_LINES
                )
            } else {
                diff
            };
            File {
                path: path.to_string(),
                diff,
            }
        })
        .collect())
}

/// The files `commit` changed against its first parent, each with its diff.
pub fn commit_files(repo: &Path, commit: &str) -> Result<Commit, String> {
    let (full, label) = subject_of(repo, commit)?;
    let parent = parent_of(repo, &full);
    let files = files_between(repo, &parent, &full, |all| all)?;
    let url = git(repo, &["remote", "get-url", "origin"])
        .ok()
        .and_then(|remote| github_commit_url(&remote, &full));
    Ok(Commit {
        short: full.chars().take(7).collect(),
        commit: full,
        label,
        url,
        files,
    })
}

/// What a PR changed: its files against where its head left the default branch, each with its diff, as the brief's writer read them
/// (`git diff base head` from the merge base), or against `since` when given (a "Since you read" story's head, #497). `label` names
/// the PR in the drawer; `keep` picks the files to show, before the cap.
pub fn pr_files(
    repo: &Path,
    head: &str,
    since: Option<&str>,
    label: &str,
    keep: impl FnOnce(Vec<String>) -> Vec<String>,
) -> Result<Commit, String> {
    let full = git(
        repo,
        &["rev-parse", "--verify", &format!("{head}^{{commit}}")],
    )?;
    let base = match since {
        Some(since) => git(
            repo,
            &["rev-parse", "--verify", &format!("{since}^{{commit}}")],
        )?,
        None => {
            let (reference, _) =
                crate::map::default_ref(repo).ok_or("the repository has no default branch")?;
            git(repo, &["merge-base", &reference, &full])?
        }
    };
    Ok(Commit {
        short: full.chars().take(7).collect(),
        files: files_between(repo, &base, &full, keep)?,
        commit: full,
        label: label.to_string(),
        url: None,
    })
}

#[cfg(test)]
mod tests;
