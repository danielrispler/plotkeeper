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
        .map_err(|_| format!("{} has no map at {}", system_map::MAP_FILE, &full[..7]))?;
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

/// The files that differ between `from` and `to`, each with its diff cut to what the drawer holds.
fn files_between(repo: &Path, from: &str, to: &str) -> Result<Vec<File>, String> {
    let names = git(repo, &["diff", "--no-renames", "--name-only", from, to])?;
    Ok(names
        .lines()
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
    let files = files_between(repo, &parent, &full)?;
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
/// (`git diff base head` from the merge base). `label` names the PR in the drawer.
pub fn pr_files(repo: &Path, head: &str, label: &str) -> Result<Commit, String> {
    let (reference, _) =
        crate::map::default_ref(repo).ok_or("the repository has no default branch")?;
    let full = git(
        repo,
        &["rev-parse", "--verify", &format!("{head}^{{commit}}")],
    )?;
    let base = git(repo, &["merge-base", &reference, &full])?;
    Ok(Commit {
        short: full.chars().take(7).collect(),
        files: files_between(repo, &base, &full)?,
        commit: full,
        label: label.to_string(),
        url: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_test_support::{git as run, repo};

    const MAP: &str = r#"{"version":1,
      "parts":[
        {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}],
      "actors":[],"links":[],"ignored":[],"laws":[]}"#;

    struct Rig {
        dir: tempfile::TempDir,
        main: String,
    }
    impl Rig {
        fn new() -> Rig {
            let (dir, main) = repo();
            Rig { dir, main }
        }
        fn commit(&self, message: &str, files: &[(&str, &str)]) -> String {
            for (name, text) in files {
                let path = self.dir.path().join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, text).unwrap();
            }
            run(self.dir.path(), &["add", "-A"]);
            run(self.dir.path(), &["commit", "-q", "-m", message]);
            run(self.dir.path(), &["rev-parse", "HEAD"])
        }
        fn history(&self, part: &str) -> Vec<Entry> {
            part_history(Path::new(&self.main), part, &HashSet::new())
        }
        fn labels(&self, part: &str) -> Vec<String> {
            self.history(part).into_iter().map(|e| e.label).collect()
        }
    }

    #[test]
    fn a_strip_label_names_its_pr_only_when_it_is_a_squash_merge_label() {
        assert_eq!(pr_of_label("#310 Studio: fog engine"), Some(310));
        assert_eq!(pr_of_label("#7"), Some(7));
        assert_eq!(pr_of_label("Revert (#9) of a thing"), None);
        assert_eq!(pr_of_label("#x thing"), None);
        assert_eq!(pr_of_label("Add a thing"), None);
    }

    #[test]
    fn a_squash_merge_subject_shows_its_pr_number_and_title_else_the_summary() {
        assert_eq!(
            label("Studio: fog engine (#310)"),
            "#310 Studio: fog engine"
        );
        assert_eq!(label("Fix the thing"), "Fix the thing");
        assert_eq!(label("Revert (#9) of a thing"), "Revert (#9) of a thing");
    }

    #[test]
    fn a_github_remote_gives_the_commit_url_in_either_spelling() {
        let url = Some("https://github.com/o/r/commit/abc".to_string());
        assert_eq!(github_commit_url("git@github.com:o/r.git", "abc"), url);
        assert_eq!(github_commit_url("https://github.com/o/r", "abc"), url);
        assert_eq!(github_commit_url("/tmp/origin.git", "abc"), None);
    }

    #[test]
    fn history_lists_only_commits_from_the_maps_birth_that_touched_the_part() {
        let rig = Rig::new();
        rig.commit("Before the map", &[("a/old", "1")]);
        rig.commit("The map is born (#1)", &[("system-map.json", MAP)]);
        rig.commit("Touch b", &[("b/x", "1")]);
        rig.commit("Touch a (#2)", &[("a/x", "1")]);
        rig.commit("Touch a again", &[("a/x", "2"), ("b/y", "1")]);
        assert_eq!(
            rig.labels("a"),
            ["#1 The map is born", "#2 Touch a", "Touch a again"]
        );
        assert_eq!(
            rig.labels("b"),
            ["#1 The map is born", "Touch b", "Touch a again"]
        );
        assert!(rig.labels("nobody").is_empty());
    }

    #[test]
    fn history_follows_first_parents_only() {
        let rig = Rig::new();
        rig.commit("The map", &[("system-map.json", MAP)]);
        run(rig.dir.path(), &["checkout", "-q", "-b", "side"]);
        rig.commit("Side work on a", &[("a/side", "1")]);
        run(rig.dir.path(), &["checkout", "-q", "main"]);
        rig.commit("Main work on b", &[("b/x", "1")]);
        run(
            rig.dir.path(),
            &["merge", "-q", "--no-ff", "-m", "Merge side (#5)", "side"],
        );
        assert_eq!(rig.labels("a"), ["The map", "#5 Merge side"]);
    }

    #[test]
    fn a_part_that_moved_paths_is_touched_by_the_union_of_the_maps() {
        let rig = Rig::new();
        rig.commit("The map", &[("system-map.json", MAP)]);
        let moved = MAP
            .replace(r#""paths":["a/"]"#, r#""paths":[]"#)
            .replace(r#""paths":["b/"]"#, r#""paths":["b/","a/"]"#);
        rig.commit("Hand a/ to b", &[("system-map.json", &moved), ("a/x", "1")]);
        assert_eq!(rig.labels("a"), ["The map", "Hand a/ to b"]);
        assert_eq!(rig.labels("b"), ["The map", "Hand a/ to b"]);
    }

    #[test]
    fn caught_up_commits_are_told_from_fogged_ones() {
        let rig = Rig::new();
        rig.commit("The map", &[("system-map.json", MAP)]);
        let newer = rig.commit("Touch a", &[("a/x", "1")]);
        let found = part_history(Path::new(&rig.main), "a", &HashSet::from([newer.clone()]));
        assert_eq!(
            found.iter().map(|e| e.fogged).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(found[1].commit, newer);
        assert_eq!(found[1].short, newer[..7]);
    }

    #[test]
    fn a_repository_with_no_map_has_no_history() {
        let rig = Rig::new();
        rig.commit("Work", &[("a/x", "1")]);
        assert!(rig.history("a").is_empty());
    }

    #[test]
    fn a_commit_from_before_the_migration_sends_its_map_as_version_2() {
        let rig = Rig::new();
        let old = rig.commit("The map", &[("system-map.json", MAP)]);
        let then = at(Path::new(&rig.main), &old).unwrap();
        let sent: serde_json::Value = serde_json::from_str(&then.source).unwrap();
        assert_eq!(sent["version"], 2);
        assert_eq!(sent["parts"][0]["purpose"], "d");
        assert!(sent["parts"][0].get("description").is_none());
    }

    #[test]
    fn a_historical_map_with_fields_this_build_does_not_know_loads_and_a_deleted_part_renders() {
        let rig = Rig::new();
        let future = MAP.replace(r#""version":1,"#, r#""version":1,"future":{"x":1},"#);
        let born = rig.commit("The map", &[("system-map.json", &future)]);
        let without = MAP.replace(
            r#",
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}"#,
            "",
        );
        rig.commit("Drop b", &[("system-map.json", &without)]);
        let then = at(Path::new(&rig.main), &born).unwrap();
        assert!(
            then.source.contains(r#""id":"b""#),
            "a deleted part is still in its past map"
        );
        assert_eq!(then.label, "The map");
        assert_eq!(then.commit, born);
        let broken = rig.commit("Broken", &[("system-map.json", "not json")]);
        assert!(at(Path::new(&rig.main), &broken).is_err());
        assert!(at(Path::new(&rig.main), "deadbeef").is_err());
    }

    /// `git args` in `dir` with committer time `at`, its author time long before: the committer time is what counts, as the author
    /// time of a rebased or squashed commit stays old.
    fn git_at(dir: &Path, at: u64, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Studio",
                "-c",
                "user.email=studio@example.com",
            ])
            .args(args)
            .env("GIT_AUTHOR_DATE", "@1000000000 +0000")
            .env("GIT_COMMITTER_DATE", format!("@{at} +0000"))
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }

    #[test]
    fn a_parts_birth_is_the_commit_time_its_id_first_entered_the_map_of_any_version() {
        let rig = Rig::new();
        let commit_at = |message: &str, files: &[(&str, &str)], at: u64| {
            for (name, text) in files {
                std::fs::write(rig.dir.path().join(name), text).unwrap();
            }
            run(rig.dir.path(), &["add", "-A"]);
            git_at(rig.dir.path(), at, &["commit", "-q", "-m", message]);
        };
        commit_at("Before the map", &[("x", "1")], 100);
        commit_at("The map", &[("system-map.json", MAP)], 200);
        commit_at("Broken", &[("system-map.json", "not json")], 300);
        let without_b = MAP.replace(
            r#",
        {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}"#,
            "",
        );
        commit_at("Drop b", &[("system-map.json", &without_b)], 400);
        // A version 2 map, with `purpose` and `holds` in place of `description`, adds c and brings b back.
        let v2 = r#"{"version":2,"parts":[
          {"id":"a","name":"A","caption":"c","purpose":"p","holds":[],"tier":"core","paths":["a/"]},
          {"id":"b","name":"B","caption":"c","purpose":"p","holds":[],"tier":"core","paths":["b/"]},
          {"id":"c","name":"C","caption":"c","purpose":"p","holds":[],"tier":"core","paths":["c/"]}],
          "actors":[],"links":[],"ignored":[],"laws":[]}"#;
        commit_at("Add c", &[("system-map.json", v2)], 500);
        commit_at("Unrelated", &[("x", "2")], 600);
        let wanted = ["a", "b", "c", "nobody"].map(String::from);
        let found = part_births(Path::new(&rig.main), &wanted).unwrap();
        let expected: BTreeMap<String, u64> = [("a", 200), ("b", 200), ("c", 500)]
            .into_iter()
            .map(|(id, at)| (id.to_string(), at))
            .collect();
        assert_eq!(found, expected);
    }

    #[test]
    fn a_failed_read_of_births_is_none_not_an_empty_answer() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(part_births(dir.path(), &["a".to_string()]), None);
    }

    #[test]
    fn a_parts_birth_follows_first_parents_only() {
        let rig = Rig::new();
        rig.commit("The map", &[("system-map.json", MAP)]);
        run(rig.dir.path(), &["checkout", "-q", "-b", "side"]);
        let with_c = MAP.replace(
            r#""paths":["b/"]}"#,
            r#""paths":["b/"]},{"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}"#,
        );
        std::fs::write(rig.dir.path().join("system-map.json"), with_c).unwrap();
        run(rig.dir.path(), &["add", "-A"]);
        git_at(rig.dir.path(), 300, &["commit", "-q", "-m", "Side adds c"]);
        run(rig.dir.path(), &["checkout", "-q", "main"]);
        assert!(
            part_births(Path::new(&rig.main), &["c".to_string()])
                .unwrap()
                .is_empty()
        );
        // The side branch's own commit is older; c entered the default branch with the merge.
        git_at(
            rig.dir.path(),
            900,
            &["merge", "-q", "--no-ff", "-m", "Merge side (#5)", "side"],
        );
        assert_eq!(
            part_births(Path::new(&rig.main), &["c".to_string()])
                .unwrap()
                .get("c"),
            Some(&900)
        );
    }

    #[test]
    fn a_commits_files_come_with_their_diffs_and_a_github_link() {
        let rig = Rig::new();
        run(
            rig.dir.path(),
            &["remote", "add", "origin", "git@github.com:o/r.git"],
        );
        rig.commit("The map", &[("system-map.json", MAP), ("a/x", "one\n")]);
        let second = rig.commit("Edit a (#7)", &[("a/x", "two\n"), ("b/new", "n\n")]);
        let found = commit_files(Path::new(&rig.main), &second).unwrap();
        assert_eq!(found.label, "#7 Edit a");
        assert_eq!(
            found.url,
            Some(format!("https://github.com/o/r/commit/{second}"))
        );
        let paths: Vec<_> = found.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a/x", "b/new"]);
        assert!(found.files[0].diff.contains("-one") && found.files[0].diff.contains("+two"));
        // The first commit of a repository diffs against nothing.
        let root = run(rig.dir.path(), &["rev-list", "--max-parents=0", "HEAD"]);
        assert!(
            commit_files(Path::new(&rig.main), &root)
                .unwrap()
                .files
                .is_empty()
        );
    }

    #[test]
    fn a_prs_files_are_what_its_head_changed_since_it_left_the_default_branch() {
        let rig = Rig::new();
        rig.commit("The map", &[("system-map.json", MAP), ("a/x", "one\n")]);
        run(rig.dir.path(), &["checkout", "-q", "-b", "work"]);
        let head = rig.commit("Edit a", &[("a/x", "two\n"), ("b/new", "n\n")]);
        run(rig.dir.path(), &["checkout", "-q", "main"]);
        // The default branch moved on meanwhile; the PR's diff is its own, not that.
        rig.commit("Elsewhere", &[("c/y", "y\n")]);
        let found = pr_files(Path::new(&rig.main), &head, "#7 Edit a").unwrap();
        assert_eq!(
            (found.label.as_str(), found.commit.as_str()),
            ("#7 Edit a", head.as_str())
        );
        let paths: Vec<_> = found.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["a/x", "b/new"]);
        assert!(found.files[0].diff.contains("-one") && found.files[0].diff.contains("+two"));
        assert!(pr_files(Path::new(&rig.main), "deadbeef", "x").is_err());
    }
}
