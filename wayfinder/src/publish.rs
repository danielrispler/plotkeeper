//! `--publish`: a spec's tickets from a folder, checked, then created blockers first.

use crate::{REPO, Result, gh, require, stdout};
use serde_json::{Value, json};

/// What a ticket body lacks of the context `docs/agents/ticket-authoring.md` requires: `## Reading`,
/// and `## Touches` with its Files and Session lines.
fn body_lacks(body: &str) -> Vec<&'static str> {
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
    check_publish(dir, &manifest)?;
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
/// written; any problem stops the publish.
fn check_publish(dir: &std::path::Path, manifest: &Manifest) -> Result<()> {
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
    Ok(())
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
mod publish_tests {
    use super::*;
    use serde_json::json;
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
