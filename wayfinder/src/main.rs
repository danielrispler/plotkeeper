//! Read the live map and ordered ticket metadata, or every open issue with `--all`. `--claim` assigns a ticket
//! and sets up its worktree; `--done` removes them after merge. Nothing else writes.
use serde_json::{Value, json};
use std::{
    env,
    process::{self, Command},
    time::Duration,
};
const REPO: &str = "danielrispler/plotkeeper";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_owned().into())
    }
}

/// Runs `command` with no stdin, killing its process group after `timeout`.
fn command_output(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output> {
    use std::{
        io::Read,
        os::unix::process::CommandExt,
        process::Stdio,
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()?;
    let (send, receive) = mpsc::channel();
    let streams: [(bool, Box<dyn Read + Send>); 2] = [
        (true, Box::new(child.stdout.take().unwrap())),
        (false, Box::new(child.stderr.take().unwrap())),
    ];
    for (stdout, mut stream) in streams {
        let send = send.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stream.read_to_end(&mut bytes).map(|_| bytes);
            let _ = send.send((stdout, result));
        });
    }
    drop(send);
    let group = -(child.id() as i32);
    let kill_group = || {
        // SAFETY: only the separately created group belonging to this child is killed.
        unsafe {
            libc::kill(group, libc::SIGKILL);
        }
    };
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            kill_group();
            let _ = child.wait();
            return Err("controlled command deadline exceeded".into());
        }
        thread::sleep(Duration::from_millis(10));
    };
    let mut stdout = vec![];
    let mut stderr = vec![];
    for _ in 0..2 {
        // Descendants may hold the pipes after the child exits: wait at most the caller's
        // remaining deadline, capped at 5s, before killing the group.
        let left = deadline.saturating_duration_since(Instant::now());
        let wait = left.clamp(Duration::from_millis(100), Duration::from_secs(5));
        let (is_stdout, bytes) = match receive.recv_timeout(wait) {
            Ok(result) => result,
            Err(error) => {
                kill_group();
                return Err(error.into());
            }
        };
        if is_stdout {
            stdout = bytes?;
        } else {
            stderr = bytes?;
        }
    }
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

pub fn open_tickets(pages: &Value) -> Result<Vec<Value>> {
    let mut rows = vec![];
    for page in pages
        .as_array()
        .ok_or("expected paginated ticket metadata")?
    {
        for ticket in page.as_array().ok_or("expected a ticket page")? {
            if ticket["state"] != "open" {
                continue;
            }
            let title = ticket["title"].as_str().ok_or("missing ticket title")?;
            let blockers = ticket["issue_dependencies_summary"]["blocked_by"]
                .as_u64()
                .ok_or_else(|| format!("Unknown blockers: {title}"))?;
            let assignees = logins(&ticket["assignees"])?;
            rows.push(json!({"number":ticket["number"],"title":title,"url":ticket["html_url"],"assignees":assignees,"blocked_by":blockers,"frontier":blockers==0 && assignees.is_empty()}));
        }
    }
    Ok(rows)
}
/// Rows for every open issue, by number, from paginated GraphQL `issues` pages.
/// Truncated assignee or blocker lists are errors, so a row never hides a blocker.
pub fn repo_tickets(pages: &Value) -> Result<Vec<Value>> {
    let mut rows = vec![];
    for page in pages.as_array().ok_or("expected paginated issues")? {
        let issues = page["data"]["repository"]["issues"]["nodes"]
            .as_array()
            .ok_or("expected an issue page")?;
        for issue in issues {
            let number = issue["number"].as_u64().ok_or("missing issue number")?;
            let list = |field: &str, key: &str| -> Result<Vec<Value>> {
                let nodes = issue[field]["nodes"]
                    .as_array()
                    .ok_or_else(|| format!("#{number}: missing {field}"))?;
                require(
                    issue[field]["totalCount"].as_u64() == Some(nodes.len() as u64),
                    &format!("#{number}: {field} list truncated or missing totalCount"),
                )?;
                nodes
                    .iter()
                    .filter(|node| field != "blockedBy" || node["state"] == "OPEN")
                    .map(|node| {
                        (!node[key].is_null())
                            .then(|| node[key].clone())
                            .ok_or_else(|| format!("#{number}: {field} entry without {key}").into())
                    })
                    .collect()
            };
            let assignees = list("assignees", "login")?;
            let blockers = list("blockedBy", "number")?;
            let frontier = blockers.is_empty() && assignees.is_empty();
            rows.push(json!({"number":number,"title":issue["title"],"url":issue["url"],"parent":issue["parent"]["number"],"assignees":assignees,"blocked_by":blockers,"frontier":frontier}));
        }
    }
    rows.sort_by_key(|row| row["number"].as_u64());
    Ok(rows)
}
fn list_maps(maps: &[&Value]) -> String {
    if maps.is_empty() {
        return " none".into();
    }
    maps.iter()
        .map(|map| {
            let closed = if map["state"] == "OPEN" {
                ""
            } else {
                " (closed)"
            };
            format!(
                "\n  #{} {}{closed}",
                map["number"],
                map["title"].as_str().unwrap_or("")
            )
        })
        .collect()
}
/// Pick the requested issue if open, else the newest open `wayfinder:map`; never a closed map.
/// `fetch_maps` fetches the map list; an open requested issue never calls it.
pub fn select_map(
    requested: Option<&Value>,
    fetch_maps: impl FnOnce() -> Result<Value>,
) -> Result<u64> {
    if let Some(issue) = requested
        && issue["state"] == "OPEN"
    {
        return issue["number"]
            .as_u64()
            .ok_or_else(|| "missing map number".into());
    }
    let maps = fetch_maps()?;
    let maps = maps.as_array().ok_or("expected a map list")?;
    let mut open = maps
        .iter()
        .filter(|map| map["state"] == "OPEN")
        .collect::<Vec<_>>();
    open.sort_by_key(|map| std::cmp::Reverse(map["number"].as_u64()));
    if let Some(issue) = requested {
        let number = issue["number"].as_u64().ok_or("missing map number")?;
        return Err(format!("Map #{number} is closed. Open maps:{}", list_maps(&open)).into());
    }
    match open.first() {
        Some(map) => map["number"]
            .as_u64()
            .ok_or_else(|| "missing map number".into()),
        None => Err(format!(
            "No open wayfinder:map issue; pass a map number. Recent maps:{}",
            list_maps(&maps.iter().collect::<Vec<_>>())
        )
        .into()),
    }
}
/// Open blocker numbers from paginated `dependencies/blocked_by` pages.
pub fn open_blockers(pages: &Value) -> Result<Vec<u64>> {
    let mut numbers = vec![];
    for page in pages.as_array().ok_or("expected paginated blockers")? {
        for blocker in page.as_array().ok_or("expected a blocker page")? {
            let number = blocker["number"].as_u64().ok_or("missing blocker number")?;
            if blocker["state"] == "open" {
                numbers.push(number);
            }
        }
    }
    Ok(numbers)
}
/// The login of each person in an `assignees` array.
fn logins(assignees: &Value) -> Result<Vec<&str>> {
    Ok(assignees
        .as_array()
        .ok_or("missing assignees")?
        .iter()
        .map(|person| person["login"].as_str().ok_or("missing assignee login"))
        .collect::<std::result::Result<_, _>>()?)
}
/// Path and branch of each live worktree in `git worktree list --porcelain`. A prunable
/// worktree's directory is gone, so it is skipped: it cannot be reused or removed.
fn live_worktrees(porcelain: &str) -> impl Iterator<Item = (&str, &str)> {
    porcelain.split("\n\n").filter_map(|stanza| {
        let field = |key: &str| stanza.lines().find_map(|line| line.strip_prefix(key));
        match (
            field("worktree "),
            field("branch refs/heads/"),
            field("prunable"),
        ) {
            (Some(path), Some(branch), None) => Some((path, branch)),
            _ => None,
        }
    })
}
#[derive(Debug, PartialEq)]
pub enum Workspace {
    /// A worktree already has the ticket branch checked out; use it as is.
    Reuse,
    /// The ticket branch exists without a worktree; check it out.
    CheckoutBranch,
    /// Only `origin` has the ticket branch (pushed from another clone); track it.
    TrackRemote,
    /// Neither exists; branch from up-to-date `origin/main`.
    Create,
}
#[derive(Debug, PartialEq)]
pub struct ClaimPlan {
    pub assign: bool,
    pub branch: String,
    pub path: String,
    pub workspace: Workspace,
}
/// Decide whether `--claim` may take `issue` and how to set up its worktree.
/// `worktrees` is `git worktree list --porcelain`; `branches` lists local and `origin/` branch names one per line.
pub fn claim_plan(
    issue: &Value,
    open_blockers: &[u64],
    caller: &str,
    root: &str,
    worktrees: &str,
    branches: &str,
) -> Result<ClaimPlan> {
    let number = issue["number"].as_u64().ok_or("missing issue number")?;
    let title = issue["title"].as_str().ok_or("missing issue title")?;
    let slug = title
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("-")
        .to_ascii_lowercase();
    require(issue["state"] == "OPEN", &format!("#{number} is closed"))?;
    let assignees = logins(&issue["assignees"])?;
    let others = assignees
        .iter()
        .filter(|login| **login != caller)
        .copied()
        .collect::<Vec<_>>();
    require(
        others.is_empty(),
        &format!("#{number} is assigned to {}", others.join(", ")),
    )?;
    let blockers = open_blockers
        .iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>();
    require(
        blockers.is_empty(),
        &format!("#{number} is blocked by {}", blockers.join(", ")),
    )?;
    let prefix = format!("issue-{number}-");
    let ours = |name: &str| name.starts_with(&prefix);
    let assign = assignees.is_empty();
    if let Some((path, branch)) = live_worktrees(worktrees).find(|(_, branch)| ours(branch)) {
        return Ok(ClaimPlan {
            assign,
            branch: branch.into(),
            path: path.into(),
            workspace: Workspace::Reuse,
        });
    }
    let names = branches.lines().map(str::trim);
    let local = names.clone().find(|name| ours(name));
    let remote = names
        .filter_map(|name| name.strip_prefix("origin/"))
        .find(|name| ours(name));
    let (branch, workspace) = match (local, remote) {
        (Some(existing), _) => (existing.to_owned(), Workspace::CheckoutBranch),
        (None, Some(pushed)) => (pushed.to_owned(), Workspace::TrackRemote),
        (None, None) => (format!("{prefix}{slug}"), Workspace::Create),
    };
    Ok(ClaimPlan {
        assign,
        path: format!("{root}/.claude/worktrees/{branch}"),
        branch,
        workspace,
    })
}
#[derive(Debug, PartialEq)]
pub struct DonePlan {
    pub branch: String,
    /// The live worktree that has the branch checked out, if any.
    pub worktree: Option<String>,
}
/// Decide whether `--done` may delete the ticket's local branch and worktree: only when a
/// merged PR ended on the branch's current commit, so nothing unmerged is lost.
/// `branches` is `<name> <sha>` per local `issue-<n>-*` branch; `fetch_merged` lists the branch's
/// merged PRs as `headRefName,headRefOid`; `cwd` must lie outside the worktree.
pub fn done_plan(
    number: u64,
    worktrees: &str,
    branches: &str,
    fetch_merged: impl FnOnce(&str) -> Result<Value>,
    cwd: &std::path::Path,
) -> Result<DonePlan> {
    let prefix = format!("issue-{number}-");
    let ours = branches
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(name, _)| name.starts_with(&prefix))
        .collect::<Vec<_>>();
    let [(branch, head)] = ours.as_slice() else {
        let names = ours.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        return Err(format!(
            "#{number}: expected one local {prefix}* branch, found [{}]",
            names.join(", ")
        )
        .into());
    };
    let merged_here = fetch_merged(branch)?
        .as_array()
        .ok_or("expected a merged PR list")?
        .iter()
        .any(|pr| pr["headRefName"] == *branch && pr["headRefOid"] == *head);
    require(
        merged_here,
        &format!("{branch}: no merged PR ends at {head}; it has unmerged work or its PR is open"),
    )?;
    let mut worktree = None;
    for (path, _) in live_worktrees(worktrees).filter(|(_, name)| name == branch) {
        require(
            !cwd.starts_with(path),
            &format!("run --done from outside {path}"),
        )?;
        worktree = Some(path.to_owned());
    }
    Ok(DonePlan {
        branch: (*branch).to_owned(),
        worktree,
    })
}
/// Files `git worktree remove` would lose: `git status --porcelain --ignored` lines other
/// than an ignored Cargo `target/` build directory, at the root or in a nested crate.
pub fn leftovers(status: &str) -> Vec<&str> {
    status
        .lines()
        .filter(|line| {
            !line
                .strip_prefix("!! ")
                .is_some_and(|path| path == "target/" || path.ends_with("/target/"))
        })
        .collect()
}
fn stdout(program: &str, args: &[&str]) -> Result<String> {
    let mut command = Command::new(program);
    command.args(args);
    let output = command_output(command, Duration::from_secs(120))?;
    require(
        output.status.success(),
        &format!(
            "{program} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ),
    )?;
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
/// Assign the ticket, then create or reuse its worktree.
fn claim(number: u64) -> Result<()> {
    let id = number.to_string();
    let issue = gh(&[
        "issue",
        "view",
        &id,
        "--repo",
        REPO,
        "--json",
        "number,state,title,assignees",
    ])?;
    let caller = gh(&["api", "user"])?["login"]
        .as_str()
        .ok_or("missing caller login")?
        .to_owned();
    let common = stdout(
        "git",
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let root = std::path::Path::new(common.trim())
        .parent()
        .ok_or("git common dir has no parent")?
        .to_str()
        .ok_or("non-UTF-8 repo path")?
        .to_owned();
    // Fetch first so a ticket branch pushed from another clone is seen.
    stdout("git", &["fetch", "origin"])?;
    let plan = claim_plan(
        &issue,
        &blockers_of(number)?,
        &caller,
        &root,
        &stdout("git", &["worktree", "list", "--porcelain"])?,
        &stdout(
            "git",
            &[
                "for-each-ref",
                "--format=%(refname:short)",
                &format!("refs/heads/issue-{number}-*"),
                &format!("refs/remotes/origin/issue-{number}-*"),
            ],
        )?,
    )?;
    if plan.assign {
        stdout(
            "gh",
            &[
                "issue",
                "edit",
                &id,
                "--repo",
                REPO,
                "--add-assignee",
                "@me",
            ],
        )?;
    }
    match plan.workspace {
        Workspace::Reuse => {}
        Workspace::CheckoutBranch => {
            // A prunable worktree may still hold the branch; drop its stale record.
            stdout("git", &["worktree", "prune"])?;
            stdout("git", &["worktree", "add", &plan.path, &plan.branch])?;
        }
        Workspace::TrackRemote => {
            stdout(
                "git",
                &[
                    "worktree",
                    "add",
                    "--track",
                    "-b",
                    &plan.branch,
                    &plan.path,
                    &format!("origin/{}", plan.branch),
                ],
            )?;
        }
        Workspace::Create => {
            // --no-track: a plain `git push` must not target main.
            stdout(
                "git",
                &[
                    "worktree",
                    "add",
                    "--no-track",
                    "-b",
                    &plan.branch,
                    &plan.path,
                    "origin/main",
                ],
            )?;
        }
    }
    println!("{}", plan.path);
    Ok(())
}
/// After merge: remove the ticket's worktree and local branch once `done_plan` allows it.
fn done(number: u64) -> Result<()> {
    let branches = stdout(
        "git",
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname)",
            &format!("refs/heads/issue-{number}-*"),
        ],
    )?;
    let plan = done_plan(
        number,
        &stdout("git", &["worktree", "list", "--porcelain"])?,
        &branches,
        |branch| {
            gh(&[
                "pr",
                "list",
                "--repo",
                REPO,
                "--state",
                "merged",
                "--head",
                branch,
                "--json",
                "headRefName,headRefOid",
            ])
        },
        // Resolved, so a symlinked path into the worktree is still caught.
        &env::current_dir()?.canonicalize()?,
    )?;
    if let Some(path) = &plan.worktree {
        let status = stdout("git", &["-C", path, "status", "--porcelain", "--ignored"])?;
        let left = leftovers(&status);
        require(
            left.is_empty(),
            &format!(
                "{path} has uncommitted, untracked or ignored files:\n{}",
                left.join("\n")
            ),
        )?;
        stdout("git", &["worktree", "remove", path])?;
        println!("removed worktree {path}");
    }
    // A prunable worktree record still holds the branch; drop it so the branch can go.
    stdout("git", &["worktree", "prune"])?;
    // -D: a squash merge leaves the branch unmerged in Git's eyes; done_plan checked the PR head.
    stdout("git", &["branch", "-D", &plan.branch])?;
    println!("deleted branch {}", plan.branch);
    Ok(())
}
const USAGE: &str =
    "usage: wayfinder-startup [map-number] [--body] | --all | --claim <issue> | --done <issue>";
const OPEN_ISSUES: &str = "query($endCursor: String) { repository(owner: \"danielrispler\", name: \"plotkeeper\") { issues(states: OPEN, first: 100, after: $endCursor) { pageInfo { hasNextPage endCursor } nodes { number title url parent { number } assignees(first: 20) { totalCount nodes { login } } blockedBy(first: 50) { totalCount nodes { number state } } } } } }";
fn run() -> Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args == ["--help"] {
        println!(
            "{USAGE}\nDefault map: newest open issue labelled wayfinder:map. The map listing only reads GitHub; it prints the map title, URL and ticket rows, and --body appends the map body.\n--all lists every open issue repo-wide, by number, with its parent, assignees, open blockers and frontier, from one paginated GraphQL query.\n--claim <issue> refuses closed, blocked or otherwise-assigned tickets, assigns the issue to you, reuses its worktree, or checks out its local or origin branch, or creates branch issue-<n>-<slug> from origin/main, under .claude/worktrees/, and prints the path.\n--done <issue> runs after merge, from outside the ticket's worktree: it removes that worktree and the ticket's one local issue-<n>-* branch, only if a merged PR ended on the branch's commit and the worktree has no uncommitted, untracked or ignored files apart from target/."
        );
        return Ok(());
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
        return write(number.parse().map_err(|_| USAGE)?);
    }
    if args == ["--all"] {
        let pages = gh(&[
            "api",
            "graphql",
            "--paginate",
            "--slurp",
            "-f",
            &format!("query={OPEN_ISSUES}"),
        ])?;
        for row in repo_tickets(&pages)? {
            println!("{row}");
        }
        return Ok(());
    }
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
    // One thread per blocked ticket; joining in order keeps rows in map order.
    // shortcut: no cap on concurrent `gh api` calls; batch them if a large map hits GitHub's secondary rate limit.
    let blockers = std::thread::scope(|scope| {
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
                Some(thread) => thread.join().map_err(|_| "blocker fetch panicked")?,
            })
            .collect::<Result<Vec<_>>>()
    })?;
    for (ticket, blockers) in tickets.iter_mut().zip(blockers) {
        ticket["blocked_by"] = json!(blockers);
        println!("{ticket}");
    }
    if body {
        println!("\n## Map body\n\n{}", issue["body"].as_str().unwrap_or(""));
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_preserves_order_and_rejects_unknown_blockers() -> Result<()> {
        let free = json!({"number":17,"title":"Free","html_url":"https://example.com/17","state":"open","assignees":[],"issue_dependencies_summary":{"blocked_by":0}});
        let mut claimed = free.clone();
        claimed["number"] = json!(11);
        claimed["assignees"] = json!([{"login":"dev"}]);
        let mut blocked = free.clone();
        blocked["number"] = json!(12);
        blocked["issue_dependencies_summary"]["blocked_by"] = json!(2);
        let mut later = free.clone();
        later["number"] = json!(18);
        let rows = open_tickets(&json!([[{"state":"closed"},claimed,free],[blocked,later]]))?;
        assert_eq!(
            rows.iter()
                .map(|r| r["number"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [11, 17, 12, 18]
        );
        assert_eq!(
            rows.iter()
                .filter(|r| r["frontier"] == true)
                .map(|r| r["number"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [17, 18]
        );
        assert_eq!(rows[0]["assignees"], json!(["dev"]));
        assert!(open_tickets(&json!([[]]))?.is_empty());
        for unknown in [
            json!({}),
            json!({"blocked_by":null}),
            json!({"blocked_by":-1}),
            json!({"blocked_by":true}),
        ] {
            let mut ticket = free.clone();
            ticket["issue_dependencies_summary"] = unknown;
            assert!(open_tickets(&json!([[ticket]])).is_err());
        }
        Ok(())
    }
}
#[cfg(test)]
mod repo_tests {
    use super::*;
    fn issue(number: u64, blockers: Value, logins: Value) -> Value {
        let blockers = blockers.as_array().unwrap().clone();
        let logins = logins.as_array().unwrap().clone();
        json!({"number":number,"title":"T","url":"u","parent":{"number":19},
            "assignees":{"totalCount":logins.len(),"nodes":logins},
            "blockedBy":{"totalCount":blockers.len(),"nodes":blockers}})
    }
    fn page(issues: Vec<Value>) -> Value {
        json!({"data":{"repository":{"issues":{"nodes":issues}}}})
    }
    #[test]
    fn rows_sort_by_number_and_keep_open_blockers_only() {
        let blocked = issue(
            9,
            json!([{"number":3,"state":"CLOSED"},{"number":4,"state":"OPEN"}]),
            json!([]),
        );
        let free = issue(2, json!([{"number":3,"state":"CLOSED"}]), json!([]));
        let claimed = issue(5, json!([]), json!([{"login":"dev"}]));
        let rows = repo_tickets(&json!([page(vec![blocked, claimed]), page(vec![free])])).unwrap();
        let summary = rows
            .iter()
            .map(|r| {
                (
                    r["number"].clone(),
                    r["blocked_by"].clone(),
                    r["frontier"].clone(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                (json!(2), json!([]), json!(true)),
                (json!(5), json!([]), json!(false)),
                (json!(9), json!([4]), json!(false))
            ]
        );
        assert_eq!(rows[0]["parent"], 19);
    }
    #[test]
    fn truncated_blockers_are_an_error() {
        let mut truncated = issue(7, json!([{"number":3,"state":"OPEN"}]), json!([]));
        truncated["blockedBy"]["totalCount"] = json!(60);
        let error = repo_tickets(&json!([page(vec![truncated])]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("#7: blockedBy list truncated"), "{error}");
    }
}
#[cfg(test)]
mod map_tests {
    use super::*;
    fn maps() -> Value {
        json!([
            {"number":1,"state":"CLOSED","title":"Map: old"},
            {"number":30,"state":"OPEN","title":"Map: newer"},
            {"number":20,"state":"OPEN","title":"Map: older"}
        ])
    }
    #[test]
    fn default_map_is_newest_open_map() {
        assert_eq!(select_map(None, || Ok(maps())).unwrap(), 30);
    }
    #[test]
    fn no_open_map_is_an_error_listing_recent_maps() {
        let only_closed = json!([{"number":1,"state":"CLOSED","title":"Map: old"}]);
        let error = select_map(None, || Ok(only_closed))
            .unwrap_err()
            .to_string();
        assert!(error.contains("No open wayfinder:map"), "{error}");
        assert!(error.contains("#1 Map: old (closed)"), "{error}");
    }
    #[test]
    fn explicit_closed_map_is_an_error_naming_open_maps() {
        let closed = json!({"number":1,"state":"CLOSED","title":"Map: old"});
        let error = select_map(Some(&closed), || Ok(maps()))
            .unwrap_err()
            .to_string();
        assert!(error.contains("#1 is closed"), "{error}");
        assert!(error.contains("#30 Map: newer"), "{error}");
        assert!(error.contains("#20 Map: older"), "{error}");
        assert!(!error.contains("Map: old ("), "{error}");
    }
    #[test]
    fn explicit_open_issue_is_used_without_map_label() {
        let spec = json!({"number":19,"state":"OPEN","title":"Spec: x"});
        assert_eq!(
            select_map(Some(&spec), || panic!("fetched the map list")).unwrap(),
            19
        );
    }
    #[test]
    fn blockers_render_open_numbers_only() {
        let pages = json!([
            [{"number":46,"state":"open"},{"number":40,"state":"closed"}],
            [{"number":48,"state":"open"}]
        ]);
        assert_eq!(open_blockers(&pages).unwrap(), [46, 48]);
        assert!(open_blockers(&json!([[{"state":"open"}]])).is_err());
    }
}
#[cfg(test)]
mod claim_tests {
    use super::*;
    const ROOT: &str = "/repo";
    const WORKTREES: &str = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n";
    fn ticket() -> Value {
        json!({"number":47,"state":"OPEN","title":"wayfinder-startup --claim: assign and set up worktree","assignees":[]})
    }
    #[test]
    fn eligible_ticket_is_assigned_and_gets_a_new_worktree_from_main() {
        let plan = claim_plan(&ticket(), &[], "dev", ROOT, WORKTREES, "").unwrap();
        assert_eq!(
            plan,
            ClaimPlan {
                assign: true,
                branch: "issue-47-wayfinder-startup-claim-assign".into(),
                path: "/repo/.claude/worktrees/issue-47-wayfinder-startup-claim-assign".into(),
                workspace: Workspace::Create,
            }
        );
    }
    fn refusal(issue: &Value, blockers: &[u64]) -> String {
        claim_plan(issue, blockers, "dev", ROOT, WORKTREES, "")
            .unwrap_err()
            .to_string()
    }
    #[test]
    fn ticket_assigned_to_another_user_is_refused() {
        let mut issue = ticket();
        issue["assignees"] = json!([{"login":"other"}]);
        let error = refusal(&issue, &[]);
        assert!(error.contains("#47 is assigned to other"), "{error}");
    }
    #[test]
    fn ticket_already_assigned_to_caller_is_not_reassigned() {
        let mut issue = ticket();
        issue["assignees"] = json!([{"login":"dev"}]);
        let plan = claim_plan(&issue, &[], "dev", ROOT, WORKTREES, "").unwrap();
        assert!(!plan.assign);
    }
    #[test]
    fn blocked_ticket_is_refused_naming_open_blockers() {
        let error = refusal(&ticket(), &[46, 48]);
        assert!(error.contains("#47 is blocked by #46, #48"), "{error}");
    }
    #[test]
    fn existing_worktree_for_the_ticket_is_reused_wherever_it_lives() {
        let worktrees = format!(
            "{WORKTREES}\nworktree /repo/.claude/worktrees/issue-470-other\nHEAD def\nbranch refs/heads/issue-470-other\n\nworktree /elsewhere/agent-1\nHEAD 123\nbranch refs/heads/issue-47-older-title\n"
        );
        let plan = claim_plan(&ticket(), &[], "dev", ROOT, &worktrees, "").unwrap();
        assert_eq!(plan.workspace, Workspace::Reuse);
        assert_eq!(plan.path, "/elsewhere/agent-1");
        assert_eq!(plan.branch, "issue-47-older-title");
    }
    #[test]
    fn existing_branch_without_worktree_is_checked_out() {
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            WORKTREES,
            "main\nissue-470-other\nissue-47-older-title\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::CheckoutBranch);
        assert_eq!(plan.branch, "issue-47-older-title");
        assert_eq!(plan.path, "/repo/.claude/worktrees/issue-47-older-title");
    }
    #[test]
    fn prunable_worktree_is_not_reused() {
        let worktrees = format!(
            "{WORKTREES}\nworktree /gone/agent-1\nHEAD 123\nbranch refs/heads/issue-47-older-title\nprunable gitdir file points to non-existent location\n"
        );
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            &worktrees,
            "issue-47-older-title\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::CheckoutBranch);
        assert_eq!(plan.path, "/repo/.claude/worktrees/issue-47-older-title");
    }
    #[test]
    fn remote_only_branch_is_tracked_into_a_new_worktree() {
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            WORKTREES,
            "origin/issue-470-other\norigin/issue-47-pushed-elsewhere\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::TrackRemote);
        assert_eq!(plan.branch, "issue-47-pushed-elsewhere");
        assert_eq!(
            plan.path,
            "/repo/.claude/worktrees/issue-47-pushed-elsewhere"
        );
    }
    #[test]
    fn local_branch_wins_over_remote_branch() {
        let plan = claim_plan(
            &ticket(),
            &[],
            "dev",
            ROOT,
            WORKTREES,
            "origin/issue-47-older-title\nissue-47-older-title\n",
        )
        .unwrap();
        assert_eq!(plan.workspace, Workspace::CheckoutBranch);
    }
    #[test]
    fn closed_ticket_is_refused() {
        let mut issue = ticket();
        issue["state"] = json!("CLOSED");
        let error = refusal(&issue, &[]);
        assert!(error.contains("#47 is closed"), "{error}");
    }
}
#[cfg(test)]
mod done_tests {
    use super::*;
    use std::path::Path;
    const WORKTREES: &str = "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\nworktree /repo/.claude/worktrees/issue-47-x\nHEAD aaa\nbranch refs/heads/issue-47-x\n";
    const BRANCH: &str = "issue-47-x aaa\n";
    fn merged_at(sha: &str) -> Value {
        json!([{"headRefName":"issue-47-x","headRefOid":sha}])
    }
    fn plan(worktrees: &str, branches: &str, merged: &Value, cwd: &str) -> Result<DonePlan> {
        done_plan(
            47,
            worktrees,
            branches,
            |_| Ok(merged.clone()),
            Path::new(cwd),
        )
    }
    #[test]
    fn branch_merged_at_its_head_loses_worktree_and_branch() {
        assert_eq!(
            plan(WORKTREES, BRANCH, &merged_at("aaa"), "/repo").unwrap(),
            DonePlan {
                branch: "issue-47-x".into(),
                worktree: Some("/repo/.claude/worktrees/issue-47-x".into()),
            }
        );
    }
    #[test]
    fn branch_without_worktree_is_still_deleted() {
        let plan = plan(
            "worktree /repo\nbranch refs/heads/main\n",
            BRANCH,
            &merged_at("aaa"),
            "/repo",
        );
        assert_eq!(plan.unwrap().worktree, None);
    }
    /// Commits after the merged head, or no merged PR at all, keep the branch.
    #[test]
    fn unmerged_or_newer_branch_is_kept() {
        for merged in [merged_at("old"), json!([])] {
            let error = plan(WORKTREES, BRANCH, &merged, "/repo").unwrap_err();
            assert!(
                error.to_string().contains("no merged PR ends at aaa"),
                "{error}"
            );
        }
    }
    #[test]
    fn running_inside_the_worktree_is_refused() {
        let error = plan(
            WORKTREES,
            BRANCH,
            &merged_at("aaa"),
            "/repo/.claude/worktrees/issue-47-x/src",
        )
        .unwrap_err();
        assert!(error.to_string().contains("from outside"), "{error}");
    }
    #[test]
    fn missing_or_ambiguous_branch_is_refused_before_asking_github() {
        for branches in ["issue-470-y bbb\n", "issue-47-x aaa\nissue-47-y bbb\n"] {
            let fetch = |_: &str| -> Result<Value> { panic!("fetched merged PRs") };
            assert!(done_plan(47, WORKTREES, branches, fetch, Path::new("/repo")).is_err());
        }
    }
    #[test]
    fn prunable_worktree_is_not_removed() {
        let worktrees =
            format!("{WORKTREES}prunable gitdir file points to non-existent location\n");
        let plan = plan(&worktrees, BRANCH, &merged_at("aaa"), "/repo").unwrap();
        assert_eq!(plan.worktree, None);
    }
    #[test]
    fn only_the_build_directory_may_be_left_behind() {
        assert!(leftovers("!! evaluation/target/\n!! target/\n").is_empty());
        assert_eq!(
            leftovers("!! target/\n!! .env\n!! target.bak/\n?? target/\n M src/lib.rs\n"),
            ["!! .env", "!! target.bak/", "?? target/", " M src/lib.rs"]
        );
    }
}
