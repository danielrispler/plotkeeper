//! `--publish`: a spec's tickets from a folder, checked, then created blockers first.

use crate::{REPO, Result, gh, require, stdout};
use serde_json::{Value, json};

/// The global installs a worker in auto mode is blocked from running mid-run, so a body must not
/// ask for them (#720's 6a and 6c would each have installed `cargo-llvm-cov`).
const INSTALLS: [&str; 7] = [
    "cargo install",
    "npm install -g",
    "npm install --global",
    "npm i -g",
    "npm i --global",
    "brew install",
    "rustup component add",
];
/// What is wrong with a ticket body against the context `docs/agents/ticket-authoring.md` requires:
/// `## Reading`, and `## Touches` with its Files, Session and Route lines, the route well-formed and
/// sound for its files; and no global install to run.
fn body_problems(body: &str) -> Vec<String> {
    let mut problems = vec![];
    if !body.lines().any(|line| line.trim_end() == "## Reading") {
        problems.push("lacks ## Reading".to_owned());
    }
    for install in INSTALLS.iter().filter(|install| body.contains(*install)) {
        problems.push(format!(
            "says to run `{install}`, a global install auto mode blocks: hand Daniel the install before /implement-spec starts"
        ));
    }
    let Some(touches) = touches(body) else {
        problems.push("lacks ## Touches".to_owned());
        return problems;
    };
    let line = |key: &str| touches.iter().copied().find(|line| line.contains(key));
    for key in ["**Files:**", "**Session:**", "**Route:**"] {
        if line(key).is_none() {
            problems.push(format!("lacks a {key} line in ## Touches"));
        }
    }
    if let Some((_, route)) = line("**Route:**").and_then(|line| line.split_once("**Route:**")) {
        let files = line("**Files:**").map_or(vec![], crate::touches::touched_files);
        problems.extend(route_problems(route, &files));
    }
    problems
}
/// The lines of a body's `## Touches` section, or nothing when it has none.
fn touches(body: &str) -> Option<Vec<&str>> {
    let mut after = body
        .lines()
        .skip_while(|line| line.trim_end() != "## Touches");
    after.next()?;
    Some(after.take_while(|line| !line.starts_with("## ")).collect())
}
/// Whether a Touches path is UI, which is never `cheap` (`docs/agents/issue-tracker.md`, Routing,
/// rule 3): under `ui/` or `scripts/snap/`, or an `.html` or `.css` file.
fn is_ui(path: &str) -> bool {
    path.starts_with("ui/")
        || path.starts_with("scripts/snap/")
        || path.ends_with(".html")
        || path.ends_with(".css")
}
/// Whether a Touches path counts toward rule 5's source files: not Markdown, not under a `tests/`
/// folder, and not a `tests.rs` or `.test.` file.
fn is_source(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    !path.ends_with(".md")
        && !path.starts_with("tests/")
        && !path.contains("/tests/")
        && name != "tests.rs"
        && !name.contains(".test.")
}
/// The terms one of which an `effort high` route's reason must name: a risk area of rule 3, a
/// risky-UI trigger, or an edge case the stated tests cannot check (`docs/agents/issue-tracker.md`,
/// Routing, "Effort"). The check catches a forgotten reason, not a weak one.
const RISKS: [&str; 10] = [
    "persistence",
    "stored",
    "queue",
    "concurrency",
    "wire",
    "redraw",
    "layout",
    "handler",
    "trust boundary",
    "untested edge case",
];
/// What is wrong with a Route line's text after `**Route:**`, which reads
/// `<tier> · effort <effort> — <reason>` (#592, "The route line"), for a ticket whose Touches
/// **Files** line names `files`.
fn route_problems(route: &str, files: &[String]) -> Vec<String> {
    let shape = route
        .trim()
        .split_once(" — ")
        .filter(|(_, reason)| !reason.trim().is_empty())
        .and_then(|(head, reason)| Some((head.split_once(" · ")?, reason)))
        .and_then(|((tier, effort), reason)| Some((tier, effort.strip_prefix("effort ")?, reason)));
    let Some((tier, effort, reason)) = shape else {
        return vec![format!(
            "**Route:** reads `{}`, not `<tier> · effort <effort> — <reason>`",
            route.trim()
        )];
    };
    let mut problems = vec![];
    if !["strong", "medium", "cheap"].contains(&tier) {
        problems.push(format!(
            "**Route:** tier {tier} is not strong, medium or cheap"
        ));
    }
    if !["medium", "high"].contains(&effort) {
        problems.push(format!("**Route:** effort {effort} is not medium or high"));
    }
    let reason = reason.to_lowercase();
    if effort == "high" && !RISKS.iter().any(|risk| reason.contains(risk)) {
        problems.push(format!(
            "**Route:** effort high, but its reason names no risk ({}); effort is medium otherwise (issue-tracker.md, Routing)",
            RISKS.join(", ")
        ));
    }
    let ui: Vec<String> = files
        .iter()
        .filter(|path| is_ui(path))
        .map(|path| format!("`{path}`"))
        .collect();
    if tier == "cheap" && !ui.is_empty() {
        problems.push(format!(
            "**Route:** cheap, but **Files:** names UI ({}), and UI is never cheap (issue-tracker.md, Routing, rule 3)",
            ui.join(", ")
        ));
    }
    let source: Vec<String> = files
        .iter()
        .filter(|path| crate::touches::is_path(path) && is_source(path))
        .map(|path| format!("`{path}`"))
        .collect();
    if tier == "cheap" && source.len() > 3 {
        problems.push(format!(
            "**Route:** cheap, but **Files:** names {} source files ({}), over the about three of rule 5 (issue-tracker.md, Routing)",
            source.len(),
            source.join(", ")
        ));
    }
    problems
}
/// A spec's tickets to publish, from `tickets.json` in the `--publish` folder.
#[derive(Debug)]
pub(crate) struct Manifest {
    parent: u64,
    tickets: Vec<Ticket>,
}
#[derive(Debug, PartialEq)]
pub(crate) struct Ticket {
    /// The ticket's body file, relative to the folder.
    file: String,
    title: String,
    labels: Vec<String>,
    /// The files of the tickets that block this one.
    blocked_by: Vec<String>,
}
/// `{"parent": <issue>, "tickets": [{"file", "title", "labels": [..], "blocked_by": [<file>, ..]}]}`;
/// `blocked_by` may be left out.
fn parse_manifest(manifest: &Value) -> Result<Manifest> {
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
fn publish_order(manifest: &Manifest) -> Result<Vec<&Ticket>> {
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
fn publish_problems(
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
                body_problems(body)
                    .into_iter()
                    .map(|problem| format!("{file}: {problem}")),
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
    // An unknown blocker or a duplicate would read as a cycle, so the order is checked after them.
    if problems.is_empty()
        && let Err(error) = publish_order(manifest)
    {
        problems.push(error.to_string());
    }
    problems.extend(unordered_shares(manifest, bodies));
    problems
}
/// The paths of a body's **Files** that two tickets can meet in: Markdown and `system-map.json` are
/// left out, since most tickets edit them and the Shared docs rule covers the shared ones
/// (`ticket-authoring.md`).
fn shareable_files(body: &str) -> Vec<String> {
    crate::touches::touched_files(body)
        .into_iter()
        .filter(|path| {
            crate::touches::is_path(path) && !path.ends_with(".md") && path != "system-map.json"
        })
        .collect()
}
/// Each pair of tickets whose **Files** share a path (a folder shares every path under it) with no
/// blocker path between them in either direction, so parallel implementers would meet in it.
fn unordered_shares(
    manifest: &Manifest,
    bodies: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let tickets: Vec<(&Ticket, Vec<String>)> = manifest
        .tickets
        .iter()
        .filter(|ticket| seen.insert(ticket.file.as_str()))
        .filter_map(|ticket| Some((ticket, shareable_files(bodies.get(&ticket.file)?))))
        .collect();
    // Whether `from` waits on `to`, directly or through other tickets.
    let waits = |from: &Ticket, to: &str| {
        let mut stack: Vec<&str> = from.blocked_by.iter().map(String::as_str).collect();
        let mut visited = std::collections::HashSet::new();
        while let Some(file) = stack.pop() {
            if file == to {
                return true;
            }
            if visited.insert(file)
                && let Some(blocker) = manifest.tickets.iter().find(|t| t.file == file)
            {
                stack.extend(blocker.blocked_by.iter().map(String::as_str));
            }
        }
        false
    };
    let mut problems = vec![];
    for (i, (a, a_files)) in tickets.iter().enumerate() {
        for (b, b_files) in &tickets[i + 1..] {
            let shared = crate::touches::shared_files(a_files, b_files);
            if !shared.is_empty() && !waits(a, &b.file) && !waits(b, &a.file) {
                let shared: Vec<String> = shared.iter().map(|path| format!("`{path}`")).collect();
                problems.push(format!(
                    "{}, {}: both name {} in **Files:**, with no blocker path between them in either direction",
                    a.file,
                    b.file,
                    shared.join(", ")
                ));
            }
        }
    }
    problems
}
/// What a publish prints without stopping: each `strong` route that justifies nothing, then the
/// longest blocker chain. Only for a folder `publish_problems` passed.
fn publish_warnings(
    manifest: &Manifest,
    bodies: &std::collections::HashMap<String, String>,
) -> Vec<String> {
    let mut warnings: Vec<String> = manifest
        .tickets
        .iter()
        .filter(|ticket| bodies.get(&ticket.file).is_some_and(|body| unjustified_strong(body)))
        .map(|ticket| {
            format!(
                "{}: **Route:** strong, but its reason names no risk ({}) and no open choice ({}); file count, a new module and hand-checked trials raise the tier to medium only (issue-tracker.md, Routing)",
                ticket.file,
                RISKS.join(", "),
                CHOICES.join(", ")
            )
        })
        .collect();
    warnings.extend(longest_chain(manifest, bodies));
    warnings
}
/// The terms one of which a `strong` route's reason names when the ticket holds a choice left open
/// (`docs/agents/issue-tracker.md`, Routing): besides `RISKS`, the only other reason for `strong`.
/// Each is matched as a whole word of the reason, so "OpenCode" or "reopens" names no open choice.
const CHOICES: [&str; 5] = ["open", "choice", "choices", "undecided", "daniel"];
/// Whether a body's Route line in `## Touches`, the one `body_problems` checks, is `strong` with a
/// reason naming neither a risk nor an open choice. The check matches words, so it warns and never
/// refuses.
fn unjustified_strong(body: &str) -> bool {
    let route = touches(body)
        .unwrap_or_default()
        .into_iter()
        .find_map(|line| line.split_once("**Route:**"));
    let Some(route) = route else {
        return false;
    };
    let Some((head, reason)) = route.1.trim().split_once(" — ") else {
        return false;
    };
    let reason = reason.to_lowercase();
    let mut words = reason.split(|c: char| !c.is_alphanumeric());
    head.starts_with("strong ·")
        && !RISKS.iter().any(|risk| reason.contains(risk))
        && !words.any(|word| CHOICES.contains(&word))
}
/// The longest blocker chain, each edge marked with the **Files** paths its two tickets share, or
/// nothing when no ticket waits on another. Of equally long chains it ends at the earliest ticket in
/// publish order, and goes through the first blocker in `blocked_by` order.
fn longest_chain(
    manifest: &Manifest,
    bodies: &std::collections::HashMap<String, String>,
) -> Option<String> {
    let order = publish_order(manifest).ok()?;
    // Each ticket's chain length, and the blocker its longest chain comes through.
    let mut depth: std::collections::HashMap<&str, (usize, Option<&str>)> = Default::default();
    for ticket in &order {
        let best = ticket
            .blocked_by
            .iter()
            .map(|blocker| (depth[blocker.as_str()].0, blocker.as_str()))
            .fold(None, |best: Option<(usize, &str)>, next| match best {
                Some(best) if best.0 >= next.0 => Some(best),
                _ => Some(next),
            });
        depth.insert(
            &ticket.file,
            best.map_or((1, None), |(length, blocker)| (length + 1, Some(blocker))),
        );
    }
    let mut end = None;
    for ticket in &order {
        let length = depth[ticket.file.as_str()].0;
        if end.is_none_or(|(best, _)| length > best) {
            end = Some((length, ticket.file.as_str()));
        }
    }
    let (length, mut file) = end.filter(|(length, _)| *length > 1)?;
    let mut chain = vec![file];
    while let Some(blocker) = depth[file].1 {
        chain.push(blocker);
        file = blocker;
    }
    chain.reverse();
    let files = |file: &str| {
        bodies
            .get(file)
            .map_or(vec![], |body| shareable_files(body))
    };
    let mut steps = vec![chain[0].to_owned()];
    let mut shares_any = false;
    for pair in chain.windows(2) {
        let shared = crate::touches::shared_files(&files(pair[0]), &files(pair[1]));
        if shared.is_empty() {
            steps.push(pair[1].to_owned());
        } else {
            shares_any = true;
            let shared: Vec<String> = shared.iter().map(|path| format!("`{path}`")).collect();
            steps.push(format!("{} (shares {})", pair[1], shared.join(", ")));
        }
    }
    let hint = if shares_any {
        "; an edge that only shares a file can often go by moving the shared piece to the other ticket (ticket-authoring.md, Edges)"
    } else {
        ""
    };
    Some(format!(
        "longest chain ({length} tickets): {}{hint}",
        steps.join(" → ")
    ))
}
/// One write a `--publish` run makes.
#[derive(Debug, PartialEq)]
pub(crate) enum Step<'a> {
    /// Create the ticket's issue from its body file.
    Create(&'a Ticket),
    /// Make the issue a sub-issue of the manifest's parent.
    Link(&'a Ticket),
    /// Mark the issue blocked by the issue of the named file.
    Edge(&'a Ticket, &'a str),
}
/// What a run still has to do, ticket by ticket in `order`, given `published.json` from earlier runs:
/// `{"<file>": {"number": <issue>, "linked": <bool>, "edges": [<blocker file>, ..]}}`.
fn publish_steps<'a>(order: &[&'a Ticket], published: &Value) -> Vec<Step<'a>> {
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
/// Publish the spec's tickets in `dir` once `publish_problems` finds none, recording each write in
/// `published.json` as it lands so a rerun does only what is missing.
pub(crate) fn publish(dir: &str) -> Result<()> {
    use std::{collections::HashMap, fs, path::Path};
    let dir = Path::new(dir);
    let manifest = parse_manifest(&serde_json::from_str(&fs::read_to_string(
        dir.join("tickets.json"),
    )?)?)?;
    let bodies = check_publish(dir, &manifest)?;
    for warning in publish_warnings(&manifest, &bodies) {
        eprintln!("warning: {warning}");
    }
    let order = publish_order(&manifest)?;
    let record = dir.join("published.json");
    let mut published: Value = match fs::read_to_string(&record) {
        Ok(text) => serde_json::from_str(&text)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(error.into()),
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
                let child = id(published_number(&published, &ticket.file)?)?;
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
                let blocker_id = id(published_number(&published, blocker)?)?;
                stdout(
                    "gh",
                    &[
                        "api",
                        "--method",
                        "POST",
                        &format!(
                            "repos/{REPO}/issues/{}/dependencies/blocked_by",
                            published_number(&published, &ticket.file)?
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
    print_published(&order, &published)
}

/// Checks the parent is open and every ticket's body, labels and blockers, before anything is
/// written; any problem stops the publish. Returns the bodies it read, by file.
fn check_publish(
    dir: &std::path::Path,
    manifest: &Manifest,
) -> Result<std::collections::HashMap<String, String>> {
    use std::{collections::HashMap, fs};
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
    problems.extend(publish_problems(manifest, &bodies, &labels));
    require(
        problems.is_empty(),
        &format!("nothing published:\n{}", problems.join("\n")),
    )?;
    Ok(bodies)
}

/// The issue number `published.json` records for ticket `file`.
fn published_number(published: &Value, file: &str) -> Result<u64> {
    published[file]["number"]
        .as_u64()
        .ok_or_else(|| format!("{file}: no issue number in published.json").into())
}

/// One row per ticket: its file, issue number and blockers.
fn print_published(order: &[&Ticket], published: &Value) -> Result<()> {
    for ticket in order {
        let blockers = ticket
            .blocked_by
            .iter()
            .map(|blocker| published_number(published, blocker))
            .collect::<Result<Vec<_>>>()?;
        println!(
            "{}",
            json!({"file": ticket.file, "number": published_number(published, &ticket.file)?, "blocked_by": blockers})
        );
    }
    Ok(())
}
#[cfg(test)]
mod publish_tests;
