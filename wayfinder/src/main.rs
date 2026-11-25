//! Read the live map and ordered ticket metadata, or every open issue with `--all`. `--claim` assigns a ticket
//! and sets up its worktree; `--done` removes them after merge; `--publish` creates a spec's tickets
//! from a folder. Nothing else writes.
use serde_json::{Value, json};
use std::{env, path::Path, process, time::Duration};
use studio_workbench::run::{self, RunError, run_until};
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
/// The backticked spans of the `**Files:**` line of a ticket's `## Touches`, each with the text
/// before it. No such line, none.
fn file_spans(body: &str) -> Vec<(&str, &str)> {
    let line = body.lines().find(|line| line.contains("**Files:**"));
    let after = line
        .and_then(|line| line.split_once("**Files:**"))
        .map_or("", |(_, rest)| rest);
    // Pieces between backticks alternate: text, span, text, span.
    let pieces: Vec<&str> = after.split('`').collect();
    let (pairs, _) = pieces.as_chunks::<2>();
    pairs
        .iter()
        .map(|[before, span]| (*before, span.trim()))
        .filter(|(_, span)| !span.is_empty())
        .collect()
}
/// The paths in the `**Files:**` line of a ticket's `## Touches`: its backticked spans. No such line, none.
pub fn touched_files(body: &str) -> Vec<String> {
    file_spans(body)
        .into_iter()
        .map(|(_, span)| span.to_owned())
        .collect()
}
/// The Touches paths of `body` that `exists` does not find. A span is a path when it has a `/` or a
/// `.`, and no space and no `<placeholder>`. A path the line introduces with the word "new" right
/// before it is one the ticket creates, so it is skipped, and so is one under `local-evidence/`,
/// which only the main checkout holds. A glob is looked up by the folder before its first `*`.
pub fn missing_paths(body: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
    file_spans(body)
        .into_iter()
        .filter(|(before, span)| {
            let new = before
                .split(|c: char| !c.is_alphanumeric())
                .rfind(|word| !word.is_empty())
                .is_some_and(|word| word.eq_ignore_ascii_case("new"));
            let path = span.contains(['/', '.'])
                && !span.contains(|c: char| c.is_whitespace() || c == '<')
                && !span.starts_with("local-evidence/");
            let looked = match span.find('*') {
                Some(star) => span[..star].rfind('/').map_or("", |slash| &span[..=slash]),
                None => span,
            };
            path && !new && !looked.is_empty() && !exists(looked)
        })
        .map(|(_, span)| span.to_owned())
        .collect()
}

/// Two open tickets that may edit the same files at once.
#[derive(Debug)]
pub struct Overlap {
    pub first: u64,
    pub second: u64,
    pub files: Vec<String>,
}

impl Overlap {
    pub fn warning(&self) -> String {
        format!(
            "warning: #{} and #{} both touch {}",
            self.first,
            self.second,
            self.files.join(", ")
        )
    }
}

/// The paths both lists name; a directory (ending in `/`) shares every path under it.
fn shared_files(a: &[String], b: &[String]) -> Vec<String> {
    let mut shared = std::collections::BTreeSet::new();
    for (x, y) in a.iter().flat_map(|x| b.iter().map(move |y| (x, y))) {
        if x == y || (x.ends_with('/') && y.starts_with(x.as_str())) {
            shared.insert(y.clone());
        } else if y.ends_with('/') && x.starts_with(y.as_str()) {
            shared.insert(x.clone());
        }
    }
    shared.into_iter().collect()
}

/// Each open issue's body, by number.
fn issue_bodies(pages: &Value) -> Result<std::collections::HashMap<u64, &str>> {
    let mut bodies = std::collections::HashMap::new();
    for page in pages.as_array().ok_or("expected paginated issues")? {
        for issue in page["data"]["repository"]["issues"]["nodes"]
            .as_array()
            .ok_or("expected an issue page")?
        {
            bodies.insert(
                issue["number"].as_u64().ok_or("missing issue number")?,
                issue["body"].as_str().unwrap_or(""),
            );
        }
    }
    Ok(bodies)
}
/// Whether a `repo_tickets` row is claimed or ready: assigned, or with no open blocker.
fn active(row: &Value) -> bool {
    let filled = |key: &str| row[key].as_array().is_some_and(|list| !list.is_empty());
    filled("assignees") || !filled("blocked_by")
}

/// A claimed or ready ticket whose Touches names paths that are not in the checkout.
#[derive(Debug)]
pub struct Stale {
    pub ticket: u64,
    pub paths: Vec<String>,
}

impl Stale {
    pub fn warning(&self) -> String {
        format!(
            "warning: #{} touches paths that are not in this checkout: {}",
            self.ticket,
            self.paths.join(", ")
        )
    }
}

/// The claimed or ready tickets whose Touches names a path `exists` does not find (`missing_paths`).
/// A blocked ticket is skipped: its files may be ones a blocker creates.
pub fn stale_touches(pages: &Value, exists: impl Fn(&str) -> bool) -> Result<Vec<Stale>> {
    let bodies = issue_bodies(pages)?;
    Ok(repo_tickets(pages)?
        .iter()
        .filter(|row| active(row))
        .filter_map(|row| {
            let ticket = row["number"].as_u64()?;
            let paths = missing_paths(bodies[&ticket], &exists);
            (!paths.is_empty()).then_some(Stale { ticket, paths })
        })
        .collect())
}
/// Whether `path` is in the checkout at `root`.
fn in_checkout(root: &str) -> impl Fn(&str) -> bool + '_ {
    move |path| std::path::Path::new(root).join(path).exists()
}

/// Pairs of open tickets, claimed or ready, that share a Touches file and are either in different
/// specs or in one spec with no blocking edge between them. Tickets with no Touches line are skipped.
pub fn overlaps(pages: &Value) -> Result<Vec<Overlap>> {
    let bodies: std::collections::HashMap<u64, Vec<String>> = issue_bodies(pages)?
        .into_iter()
        .map(|(number, body)| (number, touched_files(body)))
        .collect();
    let rows = repo_tickets(pages)?;
    let numbers = |row: &Value, key: &str| -> Vec<u64> {
        row[key].as_array().map_or(vec![], |list| {
            list.iter().filter_map(Value::as_u64).collect()
        })
    };
    let active: Vec<&Value> = rows
        .iter()
        .filter(|row| {
            let number = row["number"].as_u64().unwrap_or(0);
            !bodies[&number].is_empty() && active(row)
        })
        .collect();
    let mut found = vec![];
    for (i, a) in active.iter().enumerate() {
        for b in &active[i + 1..] {
            let (x, y) = (a["number"].as_u64().unwrap(), b["number"].as_u64().unwrap());
            let same_spec = !a["parent"].is_null() && a["parent"] == b["parent"];
            let ordered =
                numbers(a, "blocked_by").contains(&y) || numbers(b, "blocked_by").contains(&x);
            let files = shared_files(&bodies[&x], &bodies[&y]);
            if !files.is_empty() && !(same_spec && ordered) {
                found.push(Overlap {
                    first: x,
                    second: y,
                    files,
                });
            }
        }
    }
    Ok(found)
}
/// What a ticket body lacks of the context `docs/agents/ticket-authoring.md` requires: `## Reading`,
/// and `## Touches` with its Files and Session lines.
pub fn body_lacks(body: &str) -> Vec<&'static str> {
    let mut lacks = vec![];
    if !body.lines().any(|line| line.trim_end() == "## Reading") {
        lacks.push("## Reading");
    }
    let mut after = body
        .lines()
        .skip_while(|line| line.trim_end() != "## Touches");
    if after.next().is_none() {
        lacks.push("## Touches");
        return lacks;
    }
    let touches: Vec<&str> = after.take_while(|line| !line.starts_with("## ")).collect();
    for (key, lack) in [
        ("**Files:**", "a **Files:** line in ## Touches"),
        ("**Session:**", "a **Session:** line in ## Touches"),
    ] {
        if !touches.iter().any(|line| line.contains(key)) {
            lacks.push(lack);
        }
    }
    lacks
}
/// A spec's tickets to publish, from `tickets.json` in the `--publish` folder.
#[derive(Debug)]
pub struct Manifest {
    pub parent: u64,
    pub tickets: Vec<Ticket>,
}
#[derive(Debug, PartialEq)]
pub struct Ticket {
    /// The ticket's body file, relative to the folder.
    pub file: String,
    pub title: String,
    pub labels: Vec<String>,
    /// The files of the tickets that block this one.
    pub blocked_by: Vec<String>,
}
/// `{"parent": <issue>, "tickets": [{"file", "title", "labels": [..], "blocked_by": [<file>, ..]}]}`;
/// `blocked_by` may be left out.
pub fn parse_manifest(manifest: &Value) -> Result<Manifest> {
    let parent = manifest["parent"]
        .as_u64()
        .ok_or("tickets.json: missing parent issue number")?;
    let mut tickets = vec![];
    for ticket in manifest["tickets"]
        .as_array()
        .ok_or("tickets.json: missing tickets list")?
    {
        let file = ticket["file"]
            .as_str()
            .ok_or("tickets.json: a ticket is missing its file")?;
        let text = |key: &str| {
            ticket[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{file}: missing {key}"))
        };
        let texts = |key: &str, value: &Value| -> Result<Vec<String>> {
            let list = value
                .as_array()
                .ok_or_else(|| format!("{file}: missing {key} list"))?;
            list.iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("{file}: {key} holds a non-string").into())
                })
                .collect()
        };
        let blocked_by = match &ticket["blocked_by"] {
            Value::Null => vec![],
            list => texts("blocked_by", list)?,
        };
        tickets.push(Ticket {
            file: file.to_owned(),
            title: text("title")?,
            labels: texts("labels", &ticket["labels"])?,
            blocked_by,
        });
    }
    Ok(Manifest { parent, tickets })
}
/// The tickets in creation order: blockers first, manifest order otherwise. A cycle is an error
/// naming the files on it. A blocker that names no ticket is `publish_problems`' to report.
pub fn publish_order(manifest: &Manifest) -> Result<Vec<&Ticket>> {
    let mut placed: Vec<&Ticket> = vec![];
    let mut left: Vec<&Ticket> = manifest.tickets.iter().collect();
    while !left.is_empty() {
        let ready = left.iter().position(|ticket| {
            ticket
                .blocked_by
                .iter()
                .all(|blocker| placed.iter().any(|done| done.file == *blocker))
        });
        match ready {
            Some(index) => placed.push(left.remove(index)),
            None => {
                // Drop the stuck tickets that block no other stuck ticket until only the cycle stays.
                while let Some(index) = left.iter().position(|ticket| {
                    !left
                        .iter()
                        .any(|other| other.blocked_by.contains(&ticket.file))
                }) {
                    left.remove(index);
                }
                let files: Vec<&str> = left.iter().map(|ticket| ticket.file.as_str()).collect();
                return Err(format!("blockers form a cycle: {}", files.join(", ")).into());
            }
        }
    }
    Ok(placed)
}
/// Every reason the folder cannot be published, each naming its file; a run creates nothing unless
/// this is empty. `bodies` holds each body file the folder has, `labels` the repository's labels.
pub fn publish_problems(
    manifest: &Manifest,
    bodies: &std::collections::HashMap<String, String>,
    labels: &[String],
) -> Vec<String> {
    let mut problems = vec![];
    let files: Vec<&str> = manifest.tickets.iter().map(|t| t.file.as_str()).collect();
    for (index, ticket) in manifest.tickets.iter().enumerate() {
        let file = &ticket.file;
        if files[..index].contains(&file.as_str()) {
            problems.push(format!("{file}: listed twice in tickets.json"));
            continue;
        }
        match bodies.get(file) {
            None => problems.push(format!("{file}: no such file in the folder")),
            Some(body) => problems.extend(
                body_lacks(body)
                    .into_iter()
                    .map(|lack| format!("{file}: lacks {lack}")),
            ),
        }
        // GitHub matches label names ignoring case.
        let known = |label: &String| labels.iter().any(|name| name.eq_ignore_ascii_case(label));
        for label in ticket.labels.iter().filter(|label| !known(label)) {
            problems.push(format!(
                "{file}: label {label} does not exist in the repository"
            ));
        }
        for blocker in &ticket.blocked_by {
            if !files.contains(&blocker.as_str()) {
                problems.push(format!(
                    "{file}: blocker {blocker} is not a ticket of tickets.json"
                ));
            }
        }
    }
    // An unknown blocker or a duplicate would read as a cycle, so the order is checked last.
    if problems.is_empty()
        && let Err(error) = publish_order(manifest)
    {
        problems.push(error.to_string());
    }
    problems
}
/// One write a `--publish` run makes.
#[derive(Debug, PartialEq)]
pub enum Step<'a> {
    /// Create the ticket's issue from its body file.
    Create(&'a Ticket),
    /// Make the issue a sub-issue of the manifest's parent.
    Link(&'a Ticket),
    /// Mark the issue blocked by the issue of the named file.
    Edge(&'a Ticket, &'a str),
}
/// What a run still has to do, ticket by ticket in `order`, given `published.json` from earlier runs:
/// `{"<file>": {"number": <issue>, "linked": <bool>, "edges": [<blocker file>, ..]}}`.
pub fn publish_steps<'a>(order: &[&'a Ticket], published: &Value) -> Vec<Step<'a>> {
    let mut steps = vec![];
    for ticket in order {
        let record = &published[&ticket.file];
        if record["number"].as_u64().is_none() {
            steps.push(Step::Create(ticket));
        }
        if record["linked"] != true {
            steps.push(Step::Link(ticket));
        }
        let done = record["edges"].as_array();
        for blocker in &ticket.blocked_by {
            if !done.is_some_and(|edges| edges.iter().any(|edge| edge == blocker.as_str())) {
                steps.push(Step::Edge(ticket, blocker));
            }
        }
    }
    steps
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
/// than an ignored Cargo `target/` build directory, at the root or in a nested crate, or the
/// ignored `dist/`, `node_modules/`, `archive/src-tauri/gen/` and `scripts/wire/<name>.ts` that `npm ci`
/// and the check rebuild.
pub fn leftovers(status: &str) -> Vec<&str> {
    status
        .lines()
        .filter(|line| {
            !line.strip_prefix("!! ").is_some_and(|path| {
                path == "target/"
                    || path.ends_with("/target/")
                    || ["dist/", "node_modules/", "archive/src-tauri/gen/"].contains(&path)
                    || path
                        .strip_prefix("scripts/wire/")
                        .is_some_and(|name| name.ends_with(".ts") && !name.contains('/'))
            })
        })
        .collect()
}
/// Refuse while an open PR is based on `branch`: deleting the branch would close it. `open_prs`
/// is `gh pr list --base <branch> --state open --json number`.
pub fn stacked_refusal(branch: &str, open_prs: &Value) -> Result<()> {
    let numbers = open_prs
        .as_array()
        .ok_or("expected an open PR list")?
        .iter()
        .filter_map(|pr| pr["number"].as_u64())
        .collect::<Vec<_>>();
    require(
        numbers.is_empty(),
        &format!(
            "open PRs are based on {branch}; deleting it would close them. Retarget each first:\n{}",
            numbers
                .iter()
                .map(|m| format!("#{m}: gh pr edit {m} --base main"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    )
}
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
/// Assign the ticket, then create or reuse its worktree.
#[allow(
    clippy::too_many_lines,
    reason = "#556 splits this file into modules (spec #548, Group 4), which places this function anew"
)]
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
    // Only warnings: a failed lookup, an overlap or a stale path never blocks the claim.
    let warnings = open_issue_pages().and_then(|pages| {
        let mut lines: Vec<String> = overlaps(&pages)?
            .iter()
            .filter(|overlap| overlap.first == number || overlap.second == number)
            .map(Overlap::warning)
            .collect();
        // The ticket's own worktree is the checkout its implementer will see.
        let stale = stale_touches(&pages, in_checkout(&plan.path))?;
        lines.extend(
            stale
                .iter()
                .filter(|stale| stale.ticket == number)
                .map(Stale::warning),
        );
        Ok(lines)
    });
    match warnings {
        Ok(lines) => lines.iter().for_each(|line| eprintln!("{line}")),
        Err(error) => eprintln!("could not check the ticket's Touches: {error}"),
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
    stacked_refusal(
        &plan.branch,
        &gh(&[
            "pr",
            "list",
            "--repo",
            REPO,
            "--state",
            "open",
            "--base",
            &plan.branch,
            "--json",
            "number",
        ])?,
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
/// Publish the spec's tickets in `dir` once `publish_problems` finds none, recording each write in
/// `published.json` as it lands so a rerun does only what is missing.
#[allow(
    clippy::too_many_lines,
    reason = "#556 splits this file into modules (spec #548, Group 4), which places this function anew"
)]
fn publish(dir: &str) -> Result<()> {
    use std::{collections::HashMap, fs, path::Path};
    let dir = Path::new(dir);
    let manifest = parse_manifest(&serde_json::from_str(&fs::read_to_string(
        dir.join("tickets.json"),
    )?)?)?;
    let bodies: HashMap<String, String> = manifest
        .tickets
        .iter()
        .filter_map(|ticket| {
            let body = fs::read_to_string(dir.join(&ticket.file)).ok()?;
            Some((ticket.file.clone(), body))
        })
        .collect();
    let labels: Vec<String> = gh(&[
        "label", "list", "--repo", REPO, "--limit", "1000", "--json", "name",
    ])?
    .as_array()
    .ok_or("expected a label list")?
    .iter()
    .filter_map(|label| label["name"].as_str().map(str::to_owned))
    .collect();
    let parent = manifest.parent.to_string();
    let parent_state = gh(&["issue", "view", &parent, "--repo", REPO, "--json", "state"]);
    let mut problems = match parent_state {
        Ok(issue) if issue["state"] == "OPEN" => vec![],
        Ok(_) => vec![format!("tickets.json: parent #{parent} is closed")],
        Err(error) => vec![format!("tickets.json: parent #{parent} not found: {error}")],
    };
    problems.extend(publish_problems(&manifest, &bodies, &labels));
    require(
        problems.is_empty(),
        &format!("nothing published:\n{}", problems.join("\n")),
    )?;
    let order = publish_order(&manifest)?;
    let record = dir.join("published.json");
    let mut published: Value = match fs::read_to_string(&record) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error.into()),
    };
    let number = |published: &Value, file: &str| -> Result<u64> {
        published[file]["number"]
            .as_u64()
            .ok_or_else(|| format!("{file}: no issue number in published.json").into())
    };
    // The REST endpoints take an issue's database id, not its number; each is fetched once.
    let mut ids = HashMap::new();
    let mut id = |number: u64| -> Result<String> {
        if let Some(id) = ids.get(&number) {
            return Ok(String::clone(id));
        }
        let id = gh(&["api", &format!("repos/{REPO}/issues/{number}")])?["id"]
            .as_u64()
            .ok_or_else(|| format!("#{number}: missing database id"))?
            .to_string();
        ids.insert(number, id.clone());
        Ok(id)
    };
    for step in publish_steps(&order, &published) {
        match step {
            Step::Create(ticket) => {
                let body = dir.join(&ticket.file);
                let mut args = vec![
                    "issue",
                    "create",
                    "--repo",
                    REPO,
                    "--title",
                    &ticket.title,
                    "--body-file",
                    body.to_str().ok_or("non-UTF-8 body path")?,
                ];
                for label in &ticket.labels {
                    args.extend(["--label", label]);
                }
                let url = stdout("gh", &args)?;
                let created = url
                    .trim()
                    .rsplit('/')
                    .next()
                    .and_then(|n| n.parse::<u64>().ok())
                    .ok_or_else(|| format!("{}: gh issue create printed {url}", ticket.file))?;
                published[&ticket.file] = json!({"number": created, "linked": false, "edges": []});
            }
            Step::Link(ticket) => {
                let child = id(number(&published, &ticket.file)?)?;
                stdout(
                    "gh",
                    &[
                        "api",
                        "--method",
                        "POST",
                        &format!("repos/{REPO}/issues/{}/sub_issues", manifest.parent),
                        "-F",
                        &format!("sub_issue_id={child}"),
                    ],
                )?;
                published[&ticket.file]["linked"] = json!(true);
            }
            Step::Edge(ticket, blocker) => {
                let blocker_id = id(number(&published, blocker)?)?;
                stdout(
                    "gh",
                    &[
                        "api",
                        "--method",
                        "POST",
                        &format!(
                            "repos/{REPO}/issues/{}/dependencies/blocked_by",
                            number(&published, &ticket.file)?
                        ),
                        "-F",
                        &format!("issue_id={blocker_id}"),
                    ],
                )?;
                let edges = published[&ticket.file]["edges"]
                    .as_array_mut()
                    .ok_or_else(|| format!("{}: no edges list in published.json", ticket.file))?;
                edges.push(json!(blocker));
            }
        }
        fs::write(&record, serde_json::to_string_pretty(&published)? + "\n")?;
    }
    for ticket in &order {
        let blockers = ticket
            .blocked_by
            .iter()
            .map(|blocker| number(&published, blocker))
            .collect::<Result<Vec<_>>>()?;
        println!(
            "{}",
            json!({"file": ticket.file, "number": number(&published, &ticket.file)?, "blocked_by": blockers})
        );
    }
    Ok(())
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
#[allow(
    clippy::too_many_lines,
    reason = "#556 splits this file into modules (spec #548, Group 4), which places this function anew"
)]
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
                #[allow(
                    clippy::map_err_ignore,
                    reason = "a panic's payload is no error to show: the panic printed its own message"
                )]
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
    fn rebuilt_output_directories_may_be_left_behind() {
        let ignored = "!! target/\n!! dist/\n!! node_modules/\n!! archive/src-tauri/gen/\n";
        assert!(leftovers(ignored).is_empty());
        assert_eq!(
            leftovers(
                "?? dist/\n M dist/index.html\n?? node_modules/\n!! archive/src-tauri/gen/x\n"
            ),
            [
                "?? dist/",
                " M dist/index.html",
                "?? node_modules/",
                "!! archive/src-tauri/gen/x"
            ]
        );
    }
    #[test]
    fn wire_files_the_check_writes_may_be_left_behind() {
        let written = "!! scripts/wire/core.ts\n!! scripts/wire/understanding.ts\n";
        assert!(leftovers(written).is_empty());
        assert_eq!(
            leftovers("?? scripts/wire/new.ts\n!! scripts/wire/x/a.ts\n!! scripts/wire/a.rs\n"),
            [
                "?? scripts/wire/new.ts",
                "!! scripts/wire/x/a.ts",
                "!! scripts/wire/a.rs"
            ]
        );
    }
    #[test]
    fn open_prs_based_on_the_branch_refuse_with_the_retarget_command() {
        let listed = serde_json::json!([{"number": 272}, {"number": 290}]);
        let message = stacked_refusal("issue-253-x", &listed)
            .unwrap_err()
            .to_string();
        assert!(
            message.contains("#272") && message.contains("#290"),
            "{message}"
        );
        assert!(message.contains("gh pr edit 272 --base main"), "{message}");
        assert!(message.contains("gh pr edit 290 --base main"), "{message}");
        assert!(stacked_refusal("issue-253-x", &serde_json::json!([])).is_ok());
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
#[cfg(test)]
mod touches_tests {
    use super::*;
    fn issue(
        number: u64,
        parent: Option<u64>,
        assignee: bool,
        blockers: &[u64],
        body: &str,
    ) -> Value {
        let logins = if assignee {
            vec![json!({"login":"dev"})]
        } else {
            vec![]
        };
        let blocked: Vec<Value> = blockers
            .iter()
            .map(|n| json!({"number":n,"state":"OPEN"}))
            .collect();
        json!({"number":number,"title":"T","url":"u","parent":parent.map(|n| json!({"number":n})),"body":body,
            "assignees":{"totalCount":logins.len(),"nodes":logins},
            "blockedBy":{"totalCount":blocked.len(),"nodes":blocked}})
    }
    fn pages(issues: Vec<Value>) -> Value {
        json!([{"data":{"repository":{"issues":{"nodes":issues}}}}])
    }
    fn touches(files: &str) -> String {
        format!("## Touches\n\n- **Files:** {files}\n- **Session:** auto mode\n")
    }
    fn found(pages: &Value) -> Vec<(u64, u64, Vec<String>)> {
        overlaps(pages)
            .unwrap()
            .into_iter()
            .map(|o| (o.first, o.second, o.files))
            .collect()
    }

    #[test]
    fn the_files_line_lists_its_backticked_paths() {
        let body = touches("`wayfinder/` (the bin), `docs/agents/issue-tracker.md`, fixtures");
        assert_eq!(
            touched_files(&body),
            ["wayfinder/", "docs/agents/issue-tracker.md"]
        );
    }
    #[test]
    fn a_body_with_no_touches_line_has_no_files() {
        assert!(touched_files("## What to build\n\nSomething.").is_empty());
    }
    #[test]
    fn tickets_of_different_specs_sharing_a_file_overlap_once() {
        let a = issue(
            46,
            Some(43),
            true,
            &[],
            &touches("`core/src/lib.rs`, `a.rs`"),
        );
        let b = issue(
            57,
            Some(53),
            false,
            &[],
            &touches("`b.rs`, `core/src/lib.rs`"),
        );
        assert_eq!(
            found(&pages(vec![a, b])),
            [(46, 57, vec!["core/src/lib.rs".to_owned()])]
        );
    }
    #[test]
    fn a_directory_overlaps_the_files_under_it() {
        let a = issue(1, Some(9), false, &[], &touches("`wayfinder/`"));
        let b = issue(2, Some(8), false, &[], &touches("`wayfinder/src/main.rs`"));
        assert_eq!(
            found(&pages(vec![a, b])),
            [(1, 2, vec!["wayfinder/src/main.rs".to_owned()])]
        );
    }
    #[test]
    fn same_spec_tickets_overlap_unless_one_blocks_the_other() {
        let a = issue(1, Some(9), true, &[], &touches("`x.rs`"));
        let b = issue(2, Some(9), true, &[], &touches("`x.rs`"));
        assert_eq!(found(&pages(vec![a, b])).len(), 1);
        let a = issue(1, Some(9), true, &[], &touches("`x.rs`"));
        let b = issue(2, Some(9), true, &[1], &touches("`x.rs`"));
        assert!(found(&pages(vec![a, b])).is_empty());
    }
    #[test]
    fn a_ticket_that_is_blocked_and_unclaimed_is_not_compared() {
        let a = issue(1, Some(9), false, &[], &touches("`x.rs`"));
        let b = issue(2, Some(8), false, &[5], &touches("`x.rs`"));
        assert!(found(&pages(vec![a, b])).is_empty());
    }
    #[test]
    fn tickets_without_a_touches_line_are_skipped() {
        let a = issue(1, Some(9), true, &[], "no touches here");
        let b = issue(2, Some(8), true, &[], &touches("`x.rs`"));
        assert!(found(&pages(vec![a, b])).is_empty());
    }
    #[test]
    fn a_blocked_ticket_that_is_claimed_is_compared() {
        let a = issue(1, Some(9), false, &[], &touches("`x.rs`"));
        let b = issue(2, Some(8), true, &[5], &touches("`x.rs`"));
        assert_eq!(found(&pages(vec![a, b])).len(), 1);
    }
    fn missing(files: &str, present: &[&str]) -> Vec<String> {
        missing_paths(&touches(files), |path| present.contains(&path))
    }
    #[test]
    fn a_path_that_is_not_in_the_checkout_is_missing() {
        assert_eq!(
            missing(
                "`apps/studio/core/src/lib.rs`, `core/src/lib.rs`, `wayfinder/`",
                &["core/src/lib.rs", "wayfinder/"]
            ),
            ["apps/studio/core/src/lib.rs"]
        );
    }
    #[test]
    fn a_path_introduced_as_new_is_not_missing() {
        assert!(missing("a new `core/src/ledger.rs`, (or a New `ui/x.ts`)", &[]).is_empty());
        // "new" earlier in the text is about something else.
        assert_eq!(
            missing("a new app under `apps/studio/`", &[]),
            ["apps/studio/"]
        );
    }
    #[test]
    fn a_placeholder_and_local_evidence_are_not_looked_up() {
        assert!(missing("`docs/<topic>.md`, `local-evidence/README.md`", &[]).is_empty());
    }
    #[test]
    fn a_span_that_is_no_path_is_not_looked_up() {
        assert!(
            missing(
                "`core/src/lib.rs` (`is_check`, `npm run check`)",
                &["core/src/lib.rs"]
            )
            .is_empty()
        );
    }
    #[test]
    fn a_glob_is_looked_up_by_its_folder() {
        assert!(missing("`ui/tests/*.test.mjs`, `*.md`", &["ui/tests/"]).is_empty());
        assert_eq!(
            missing("`ui/gone/*.test.mjs`", &["ui/tests/"]),
            ["ui/gone/*.test.mjs"]
        );
    }
    #[test]
    fn stale_touches_names_claimed_or_ready_tickets_only() {
        let ready = issue(6, Some(4), false, &[], &touches("`apps/studio/ui/`, `ui/`"));
        let blocked = issue(7, Some(4), false, &[6], &touches("`ui/evidence.ts`"));
        let fine = issue(8, None, true, &[], &touches("`ui/`"));
        let stale =
            stale_touches(&pages(vec![ready, blocked, fine]), |path| path == "ui/").unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(
            stale[0].warning(),
            "warning: #6 touches paths that are not in this checkout: apps/studio/ui/"
        );
    }
    #[test]
    fn warnings_name_both_tickets_and_the_files() {
        let overlap = Overlap {
            first: 46,
            second: 57,
            files: vec!["a.rs".into(), "b.rs".into()],
        };
        assert_eq!(
            overlap.warning(),
            "warning: #46 and #57 both touch a.rs, b.rs"
        );
    }
}
#[cfg(test)]
mod publish_tests {
    use super::*;
    const BODY: &str = "## What to build\n\nX.\n\n## Reading\n\n- None.\n\n## Touches\n\n- **Files:** `a.rs`\n- **Session:** auto mode\n";
    #[test]
    fn a_complete_body_lacks_nothing() {
        assert!(body_lacks(BODY).is_empty());
    }
    #[test]
    fn a_body_names_each_section_and_line_it_lacks() {
        assert_eq!(
            body_lacks("## What to build\n\nX.\n"),
            ["## Reading", "## Touches"]
        );
        let no_lines = "## Reading\n\nNone.\n\n## Touches\n\nTBD.\n\n## Notes\n\n- **Files:** `a.rs`\n- **Session:** auto mode\n";
        assert_eq!(
            body_lacks(no_lines),
            [
                "a **Files:** line in ## Touches",
                "a **Session:** line in ## Touches"
            ]
        );
    }
    fn manifest() -> Value {
        json!({"parent":102,"tickets":[
            {"file":"02.md","title":"Studio: two","labels":["studio"],"blocked_by":["01.md"]},
            {"file":"01.md","title":"Studio: one","labels":["studio","ready-for-agent"]}
        ]})
    }
    #[test]
    fn the_manifest_gives_the_parent_and_each_ticket() {
        let manifest = parse_manifest(&manifest()).unwrap();
        assert_eq!(manifest.parent, 102);
        assert_eq!(
            manifest.tickets[0],
            Ticket {
                file: "02.md".into(),
                title: "Studio: two".into(),
                labels: vec!["studio".into()],
                blocked_by: vec!["01.md".into()],
            }
        );
        assert!(manifest.tickets[1].blocked_by.is_empty());
    }
    #[test]
    fn a_manifest_missing_a_field_names_it() {
        let mut no_parent = manifest();
        no_parent["parent"] = json!(null);
        let error = parse_manifest(&no_parent).unwrap_err().to_string();
        assert!(error.contains("parent"), "{error}");
        let mut no_title = manifest();
        no_title["tickets"][1]["title"] = json!(null);
        let error = parse_manifest(&no_title).unwrap_err().to_string();
        assert!(error.contains("01.md: missing title"), "{error}");
    }
    fn ticket(file: &str, blocked_by: &[&str]) -> Ticket {
        Ticket {
            file: file.into(),
            title: format!("Studio: {file}"),
            labels: vec!["studio".into()],
            blocked_by: blocked_by.iter().map(ToString::to_string).collect(),
        }
    }
    fn files(order: &[&Ticket]) -> Vec<String> {
        order.iter().map(|t| t.file.clone()).collect()
    }
    #[test]
    fn blockers_come_first_and_manifest_order_holds_otherwise() {
        let manifest = Manifest {
            parent: 9,
            tickets: vec![
                ticket("c.md", &["b.md"]),
                ticket("a.md", &[]),
                ticket("b.md", &["a.md"]),
                ticket("d.md", &[]),
            ],
        };
        assert_eq!(
            files(&publish_order(&manifest).unwrap()),
            ["a.md", "b.md", "c.md", "d.md"]
        );
    }
    #[test]
    fn a_cycle_is_an_error_naming_its_files() {
        let manifest = Manifest {
            parent: 9,
            tickets: vec![
                ticket("a.md", &[]),
                ticket("b.md", &["c.md"]),
                ticket("c.md", &["b.md"]),
                // Stuck behind the cycle, but not on it.
                ticket("d.md", &["b.md"]),
            ],
        };
        let error = publish_order(&manifest).unwrap_err().to_string();
        assert!(error.contains("cycle"), "{error}");
        assert!(error.contains("b.md, c.md"), "{error}");
        assert!(
            !error.contains("a.md") && !error.contains("d.md"),
            "{error}"
        );
    }
    fn labels() -> Vec<String> {
        vec!["studio".into(), "ready-for-agent".into()]
    }
    fn bodies(files: &[&str]) -> std::collections::HashMap<String, String> {
        files
            .iter()
            .map(|f| (f.to_string(), BODY.to_owned()))
            .collect()
    }
    #[test]
    fn a_sound_folder_has_no_problems() {
        let manifest = parse_manifest(&manifest()).unwrap();
        assert!(publish_problems(&manifest, &bodies(&["01.md", "02.md"]), &labels()).is_empty());
        let mut shouting = manifest;
        shouting.tickets[0].labels = vec!["Studio".into()];
        assert!(publish_problems(&shouting, &bodies(&["01.md", "02.md"]), &labels()).is_empty());
    }
    #[test]
    fn each_problem_names_its_file() {
        let manifest = Manifest {
            parent: 9,
            tickets: vec![
                ticket("a.md", &["gone.md"]),
                Ticket {
                    labels: vec!["studio".into(), "wishlist".into()],
                    ..ticket("b.md", &[])
                },
                ticket("c.md", &[]),
                ticket("b.md", &[]),
            ],
        };
        let mut bodies = bodies(&["a.md", "b.md"]);
        bodies.insert("a.md".into(), "## Reading\n\nNone.\n".into());
        assert_eq!(
            publish_problems(&manifest, &bodies, &labels()),
            [
                "a.md: lacks ## Touches",
                "a.md: blocker gone.md is not a ticket of tickets.json",
                "b.md: label wishlist does not exist in the repository",
                "c.md: no such file in the folder",
                "b.md: listed twice in tickets.json",
            ]
        );
    }
    #[test]
    fn a_cycle_is_a_problem() {
        let manifest = Manifest {
            parent: 9,
            tickets: vec![ticket("a.md", &["b.md"]), ticket("b.md", &["a.md"])],
        };
        assert_eq!(
            publish_problems(&manifest, &bodies(&["a.md", "b.md"]), &labels()),
            ["blockers form a cycle: a.md, b.md"]
        );
    }
    #[test]
    fn a_first_run_creates_links_and_adds_edges_ticket_by_ticket() {
        let (a, b) = (ticket("a.md", &[]), ticket("b.md", &["a.md"]));
        assert_eq!(
            publish_steps(&[&a, &b], &json!({})),
            [
                Step::Create(&a),
                Step::Link(&a),
                Step::Create(&b),
                Step::Link(&b),
                Step::Edge(&b, "a.md"),
            ]
        );
    }
    #[test]
    fn a_second_run_does_only_what_is_missing() {
        let (a, b, c) = (
            ticket("a.md", &[]),
            ticket("b.md", &["a.md"]),
            ticket("c.md", &["a.md", "b.md"]),
        );
        let published = json!({
            "a.md": {"number": 10, "linked": true, "edges": []},
            "b.md": {"number": 11, "linked": true, "edges": ["a.md"]},
            "c.md": {"number": 12, "linked": false, "edges": ["b.md"]},
        });
        assert_eq!(
            publish_steps(&[&a, &b, &c], &published),
            [Step::Link(&c), Step::Edge(&c, "a.md")]
        );
    }
}
