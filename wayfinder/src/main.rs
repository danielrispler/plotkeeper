//! Read the live map and ordered ticket metadata, or every open issue with `--all`. `--claim` assigns a ticket
//! and sets up its worktree; `--done` removes them after merge; `--publish` creates a spec's tickets
//! from a folder. Nothing else writes.

mod claim;
mod done;
mod issues;
mod publish;
mod touches;

use claim::claim;
use done::done;
use issues::{open_blockers, open_tickets, repo_tickets, select_map};
use publish::publish;
use serde_json::{Value, json};
use std::{env, path::Path, process, time::Duration};
use studio_workbench::run::{self, RunError, run_until};
use touches::{in_checkout, overlaps, stale_touches};
const REPO: &str = "danielrispler/plotkeeper";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}

/// The longest one `git` or `gh` call may take: a fetch and a paginated listing wait on the network.
const COMMAND_LIMIT: Duration = Duration::from_secs(120);

fn stdout(program: &str, args: &[&str]) -> Result<String> {
    let here = Path::new(".");
    let mut command = match program {
        "git" => run::git(here),
        gh => run::gh_at(gh, here),
    };
    let output = run_until(command.args(args), COMMAND_LIMIT).map_err(|e| match e {
        RunError::Failed(output) => format!(
            "{program} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ),
        e => format!("{program} {}: {e}", args.join(" ")),
    })?;
    Ok(String::from_utf8(output.stdout)?)
}
fn gh(args: &[&str]) -> Result<Value> {
    Ok(serde_json::from_str(&stdout("gh", args)?)?)
}
fn blockers_of(number: impl std::fmt::Display) -> Result<Vec<u64>> {
    open_blockers(&gh(&[
        "api",
        "--paginate",
        "--slurp",
        &format!("repos/{REPO}/issues/{number}/dependencies/blocked_by"),
    ])?)
}
const USAGE: &str = "usage: wayfinder-startup [map-number] [--body] | --all | --claim <issue> | --done <issue> | --publish <dir>";
fn open_issue_pages() -> Result<Value> {
    gh(&[
        "api",
        "graphql",
        "--paginate",
        "--slurp",
        "-f",
        &format!("query={OPEN_ISSUES}"),
    ])
}
const OPEN_ISSUES: &str = "query($endCursor: String) { repository(owner: \"danielrispler\", name: \"plotkeeper\") { issues(states: OPEN, first: 100, after: $endCursor) { pageInfo { hasNextPage endCursor } nodes { number title url body parent { number } assignees(first: 20) { totalCount nodes { login } } blockedBy(first: 50) { totalCount nodes { number state } } } } } }";
fn run() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] {
        println!(
            "{USAGE}\nDefault map: newest open issue labelled wayfinder:map. The map listing only reads GitHub; it prints the map title, URL and ticket rows, and --body appends the map body.\n--all lists every open issue repo-wide, by number, with its parent, assignees, open blockers and frontier, from one paginated GraphQL query. It also warns on stderr, once per pair, when two claimed or ready tickets share a file in their Touches Files line and are in different specs or in one spec with no blocking edge between them, and once per claimed or ready ticket whose Touches names a path that is not in this checkout (a path with \"new\" right before it is one the ticket creates).\n--claim <issue> refuses closed, blocked or otherwise-assigned tickets, assigns the issue to you, reuses its worktree, or checks out its local or origin branch, or creates branch issue-<n>-<slug> from origin/main, under .claude/worktrees/, and prints the path. It warns on stderr, never blocking, when another claimed or ready ticket's Touches files overlap this one's, or when its own Touches names a path that is not in its worktree.\n--done <issue> runs after merge, from outside the ticket's worktree: it removes that worktree and the ticket's one local issue-<n>-* branch, only if a merged PR ended on the branch's commit and the worktree has no uncommitted, untracked or ignored files apart from the ignored build output target/, dist/, node_modules/ and archive/src-tauri/gen/. It also refuses, removing nothing, while an open PR has the branch as its base (deleting the branch would close it), naming each such PR and printing gh pr edit <m> --base main for it.\n--publish <dir> creates a spec's tickets from a folder: one Markdown body per ticket and tickets.json, {{\"parent\": <issue>, \"tickets\": [{{\"file\": \"01-x.md\", \"title\": \"...\", \"labels\": [\"studio\", \"ready-for-agent\"], \"blocked_by\": [\"<file>\", ...]}}]}}. It first checks that the parent is open, every body for ## Reading and ## Touches with Files and Session lines, every label against the repository, and every blocker for a manifest file and for cycles; any problem stops it before an issue exists. Then it creates the issues blockers first, links each as a sub-issue of the parent and adds its blocked-by edges, recording each write in <dir>/published.json, so a rerun after a failure does only what is missing. It prints one row per ticket: its file, issue number and blockers."
        );
        return Ok(());
    }
    if args.first().is_some_and(|flag| flag == "--publish") {
        let [_, dir] = args.as_slice() else {
            return Err(USAGE.into());
        };
        return publish(dir);
    }
    let write: Option<fn(u64) -> Result<()>> = match args.first().map(String::as_str) {
        Some("--claim") => Some(claim),
        Some("--done") => Some(done),
        _ => None,
    };
    if let Some(write) = write {
        let [_, number] = args.as_slice() else {
            return Err(USAGE.into());
        };
        #[allow(
            clippy::map_err_ignore,
            reason = "a number that does not parse is a usage error, and the usage says what to type"
        )]
        let number = number.parse().map_err(|_| USAGE)?;
        return write(number);
    }
    if args == ["--all"] {
        return list_all();
    }
    list_map(&args)
}

/// `--all`: every open issue repo-wide as one JSON row each, with the warnings on stderr.
fn list_all() -> Result<()> {
    let pages = open_issue_pages()?;
    for row in repo_tickets(&pages)? {
        println!("{row}");
    }
    // Warnings go to stderr, beside the JSON rows; they never fail the listing.
    for overlap in overlaps(&pages)? {
        eprintln!("{}", overlap.warning());
    }
    let root = stdout("git", &["rev-parse", "--show-toplevel"])?;
    for stale in stale_touches(&pages, in_checkout(root.trim()))? {
        eprintln!("{}", stale.warning());
    }
    Ok(())
}

/// The map listing: map `args` names, or the newest open one, with its open tickets in map order
/// and, with `--body`, its body.
fn list_map(args: &[String]) -> Result<()> {
    let body = args.iter().any(|arg| arg == "--body");
    let args = args
        .iter()
        .filter(|arg| *arg != "--body")
        .collect::<Vec<_>>();
    require(args.len() <= 1, USAGE)?;
    let requested = args
        .first()
        .map(|number| number.parse::<u64>())
        .transpose()?;
    require(requested != Some(0), "map issue number must be positive")?;
    let maps_in = |state: &str, limit: &str| {
        gh(&[
            "issue",
            "list",
            "--repo",
            REPO,
            "--label",
            "wayfinder:map",
            "--state",
            state,
            "--limit",
            limit,
            "--json",
            "number,title,state",
        ])
    };
    let open_maps = || maps_in("open", "100");
    let view = |number: u64| {
        gh(&[
            "issue",
            "view",
            &number.to_string(),
            "--repo",
            REPO,
            "--json",
            "number,state,title,url,body",
        ])
    };
    let (map, issue) = match requested {
        Some(number) => {
            let issue = view(number).map_err(|error| {
                let open = select_map(None, open_maps)
                    .map(|newest| format!("; newest open map is #{newest}"))
                    .unwrap_or_default();
                format!("Map #{number} not found: {error}{open}")
            })?;
            (select_map(Some(&issue), open_maps)?, issue)
        }
        None => {
            // Fetched up front so a failed fetch errs rather than falling through to the closed list.
            let maps = open_maps()?;
            // With no open map, the closed-only list always errs, naming recent maps.
            let map = select_map(None, || Ok(maps))
                .or_else(|_| select_map(None, || maps_in("closed", "20")))?;
            (map, view(map)?)
        }
    };
    let mut tickets = open_tickets(&gh(&[
        "api",
        "--paginate",
        "--slurp",
        &format!("repos/{REPO}/issues/{map}/sub_issues"),
    ])?)?;
    println!(
        "# {}\n{}\n\n## Open tickets (map order)\n",
        issue["title"].as_str().unwrap_or(""),
        issue["url"].as_str().unwrap_or("")
    );
    let blockers = blockers_of_each(&tickets)?;
    for (ticket, blockers) in tickets.iter_mut().zip(blockers) {
        ticket["blocked_by"] = json!(blockers);
        println!("{ticket}");
    }
    if body {
        println!("\n## Map body\n\n{}", issue["body"].as_str().unwrap_or(""));
    }
    Ok(())
}

/// Each ticket's open blockers, fetched only for a blocked one.
fn blockers_of_each(tickets: &[Value]) -> Result<Vec<Vec<u64>>> {
    // One thread per blocked ticket; joining in order keeps rows in map order.
    // shortcut: no cap on concurrent `gh api` calls; batch them if a large map hits GitHub's secondary rate limit.
    std::thread::scope(|scope| {
        let fetches = tickets
            .iter()
            .map(|ticket| {
                (ticket["blocked_by"] != 0)
                    .then(|| scope.spawn(move || blockers_of(&ticket["number"])))
            })
            .collect::<Vec<_>>();
        fetches
            .into_iter()
            .map(|fetch| match fetch {
                None => Ok(vec![]),
                #[allow(
                    clippy::map_err_ignore,
                    reason = "a panic's payload is no error to show: the panic printed its own message"
                )]
                Some(thread) => thread.join().map_err(|_| "blocker fetch panicked")?,
            })
            .collect::<Result<Vec<_>>>()
    })
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
