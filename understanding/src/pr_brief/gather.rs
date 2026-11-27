//! Gathering: git and gh fill a `PrSource`.

use super::{PrMeta, PrSource, Spec, TaggedTest, ids};
use std::{collections::BTreeSet, path::Path, time::Duration};
use studio_workbench::run::{self as runner, RunError, run_until};
/// The longest one `git` read or `gh` lookup of the gathering may take.
const GATHER_LIMIT: Duration = Duration::from_secs(30);

/// What `git` or `gh` (the `program`) prints for `args` in `dir`.
fn run(program: &str, dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = match program {
        "git" => runner::git(dir),
        gh => runner::gh_at(gh, dir),
    };
    let out = run_until(command.args(args), GATHER_LIMIT).map_err(|e| match e {
        RunError::Start(e) => format!("{program} did not run: {e}"),
        RunError::Failed(out) => format!(
            "{program} {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ),
        e => format!("{program} {args:?} {e}"),
    })?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The name of the test a `@law:` tag sits above, from `body`'s lines after line `n` (1-based).
fn tagged_test_name(body: &str, n: usize) -> String {
    let test = body
        .lines()
        .skip(n)
        .find(|l| l.contains("fn ") || l.contains("test("))
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    let quoted = test
        .split(['(', '"', '\''])
        .find(|w| w.trim().len() > 6 && !w.contains("fn ") && !w.contains("test"))
        .map(|w| w.trim().to_string());
    quoted
        .or_else(|| {
            test.split("fn ")
                .nth(1)
                .map(|t| t.split('(').next().unwrap_or("").trim().to_string())
        })
        .unwrap_or(test)
}

/// The map text, its diff and its note for the PR from `base` to `head` in `repo`: the head's map, else the map as of its birth
/// for a PR older than the map, else `passed_map`, the map the server read from a checkout or a pending PR in a repository that
/// never committed one. A passed map has no diff to give the brief a map-diff chapter.
fn map_of(
    repo: &Path,
    base: &str,
    head: &str,
    passed_map: Option<&str>,
) -> Result<(String, String, Option<String>), String> {
    let has_map = run(
        "git",
        repo,
        &[
            "cat-file",
            "-e",
            &format!("{head}:{}", system_map::MAP_FILE),
        ],
    )
    .is_ok();
    if has_map {
        let text = run(
            "git",
            repo,
            &["show", &format!("{head}:{}", system_map::MAP_FILE)],
        )?;
        let diff = run(
            "git",
            repo,
            &["diff", base, head, "--", system_map::MAP_FILE],
        )?;
        return Ok((text, diff, None));
    }
    let birth = run(
        "git",
        repo,
        &[
            "log",
            "--diff-filter=A",
            "--format=%H",
            "-1",
            "--",
            system_map::MAP_FILE,
        ],
    )?;
    let birth = birth.trim();
    if birth.is_empty() {
        // A repository that never committed a map: the server's passed-in map (from the checkout or a pending PR) places the
        // brief, and there is no map diff to give it a chapter of its own (#753, "PR briefs while the map is not committed").
        let Some(text) = passed_map else {
            return Err("the repository has no system map".into());
        };
        return Ok((
            text.to_string(),
            String::new(),
            Some("none: the map is not in the repository yet".into()),
        ));
    }
    let text = run(
        "git",
        repo,
        &["show", &format!("{birth}:{}", system_map::MAP_FILE)],
    )?;
    let note = format!(
        "none: this PR is older than the map; the map shown is as of its birth ({})",
        &birth[..birth.len().min(9)]
    );
    Ok((text, String::new(), Some(note)))
}

/// What `git` gives for the PR whose changes run from `base` to `head` in `repo`: the map as of `head` (or its
/// birth for a PR older than the map), its diff, the whole diff and the tagged tests. `meta` is `fetch_meta`'s.
/// `passed_map` is the map the server read from the checkout or a pending PR, for a repository that never committed one.
pub fn gather(
    repo: &Path,
    base: &str,
    head: &str,
    meta: PrMeta,
    passed_map: Option<&str>,
) -> Result<PrSource, String> {
    let (map_text, map_diff, map_note) = map_of(repo, base, head, passed_map)?;
    let diff = run("git", repo, &["diff", "-M", base, head])?;
    let laws: BTreeSet<String> = serde_json::from_str::<serde_json::Value>(&map_text)
        .map(|m| ids(&m, "laws"))
        .unwrap_or_default();
    let mut tagged = Vec::new();
    // `git grep` exits 1 when nothing matches.
    let mut grep = runner::git(repo);
    let grep = match run_until(grep.args(["grep", "-n", "@law:", head]), GATHER_LIMIT) {
        Ok(out) | Err(RunError::Failed(out)) => out,
        Err(RunError::Start(e)) => return Err(format!("git did not run: {e}")),
        Err(e) => return Err(format!("git grep {e}")),
    };
    for line in String::from_utf8_lossy(&grep.stdout).lines() {
        let mut f = line.splitn(4, ':');
        let (_, path, num, text) = (
            f.next(),
            f.next().unwrap_or(""),
            f.next().unwrap_or("0"),
            f.next().unwrap_or(""),
        );
        let Some(at) = text.find("@law:") else {
            continue;
        };
        let Some(law) = text[at + 5..].split_whitespace().next() else {
            continue;
        };
        if !laws.contains(law)
            || path == system_map::MAP_FILE
            || path.ends_with(".md")
            || path.ends_with(".log")
        {
            continue;
        }
        let body = run("git", repo, &["show", &format!("{head}:{path}")])?;
        tagged.push(TaggedTest {
            law: law.to_string(),
            path: path.to_string(),
            test: tagged_test_name(&body, num.parse().unwrap_or(0)),
            changed: false,
        });
    }
    Ok(PrSource {
        meta,
        map_text,
        map_diff,
        map_note,
        diff,
        tagged,
        ..PrSource::default()
    })
}

/// Every issue a PR closes, from `gh pr view --json closingIssuesReferences`, lowest first (ticket #394: pre-flight matches through each).
pub fn closing_numbers(v: &serde_json::Value) -> Vec<u64> {
    let mut numbers: Vec<u64> = v["closingIssuesReferences"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| i["number"].as_u64())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
}

/// What `gh` says of PR `number` in `repo`: its text, head, checks and the lowest-numbered issue it closes.
/// Calls `gh` as a normal user, nothing else.
pub fn fetch_meta(repo: &Path, number: u64) -> Result<PrMeta, String> {
    let n = number.to_string();
    let json = |args: &[&str]| -> Result<serde_json::Value, String> {
        serde_json::from_str(&run("gh", repo, args)?)
            .map_err(|e| format!("gh printed no JSON: {e}"))
    };
    let v = json(&[
        "pr",
        "view",
        &n,
        "--json",
        "title,body,headRefOid,mergedAt,createdAt,statusCheckRollup,closingIssuesReferences",
    ])?;
    let checks = v["statusCheckRollup"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| {
            format!(
                "{} {}",
                c["name"].as_str().unwrap_or(""),
                c["conclusion"].as_str().unwrap_or("")
            )
        })
        .filter(|c| !c.ends_with("SKIPPED"))
        .collect();
    let spec = match v["closingIssuesReferences"]
        .as_array()
        .and_then(|a| a.iter().filter_map(|i| i["number"].as_u64()).min())
    {
        Some(i) => {
            let s = json(&[
                "issue",
                "view",
                &i.to_string(),
                "--json",
                "title,body,createdAt",
            ])?;
            Some(Spec {
                number: i,
                title: s["title"].as_str().unwrap_or("").into(),
                created_at: s["createdAt"].as_str().unwrap_or("").into(),
                body: s["body"].as_str().unwrap_or("").into(),
            })
        }
        None => None,
    };
    Ok(PrMeta {
        number,
        title: v["title"].as_str().unwrap_or("").into(),
        body: v["body"].as_str().unwrap_or("").into(),
        head: v["headRefOid"].as_str().unwrap_or("").into(),
        merged_at: v["mergedAt"].as_str().unwrap_or("").into(),
        created_at: v["createdAt"].as_str().unwrap_or("").into(),
        checks,
        spec,
        bug: closing_numbers(&v).iter().any(|i| {
            json(&["issue", "view", &i.to_string(), "--json", "labels"])
                .is_ok_and(|issue| labelled_bug(&issue))
        }),
        closing: closing_numbers(&v),
    })
}

/// Whether `gh issue view --json labels` says the issue is labelled `bug`.
pub fn labelled_bug(issue: &serde_json::Value) -> bool {
    issue["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|label| label["name"].as_str() == Some("bug"))
}
