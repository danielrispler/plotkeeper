//! The files a ticket's Touches names: overlaps between tickets and paths missing from a checkout.

use crate::Result;
use crate::issues::repo_tickets;
use serde_json::Value;

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
pub(crate) fn touched_files(body: &str) -> Vec<String> {
    file_spans(body)
        .into_iter()
        .map(|(_, span)| span.to_owned())
        .collect()
}
/// The Touches paths of `body` that `exists` does not find. A span is a path when it has a `/` or a
/// `.`, and no space and no `<placeholder>`. A path the line introduces with the word "new" right
/// before it is one the ticket creates, so it is skipped, and so is one under `local-evidence/`,
/// which only the main checkout holds. A glob is looked up by the folder before its first `*`.
fn missing_paths(body: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
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
pub(crate) struct Overlap {
    pub(crate) first: u64,
    pub(crate) second: u64,
    files: Vec<String>,
}

impl Overlap {
    pub(crate) fn warning(&self) -> String {
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
pub(crate) struct Stale {
    pub(crate) ticket: u64,
    paths: Vec<String>,
}

impl Stale {
    pub(crate) fn warning(&self) -> String {
        format!(
            "warning: #{} touches paths that are not in this checkout: {}",
            self.ticket,
            self.paths.join(", ")
        )
    }
}

/// The claimed or ready tickets whose Touches names a path `exists` does not find (`missing_paths`).
/// A blocked ticket is skipped: its files may be ones a blocker creates.
pub(crate) fn stale_touches(pages: &Value, exists: impl Fn(&str) -> bool) -> Result<Vec<Stale>> {
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
pub(crate) fn in_checkout(root: &str) -> impl Fn(&str) -> bool + '_ {
    move |path| std::path::Path::new(root).join(path).exists()
}

/// Pairs of open tickets, claimed or ready, that share a Touches file and are either in different
/// specs or in one spec with no blocking edge between them. Tickets with no Touches line are skipped.
pub(crate) fn overlaps(pages: &Value) -> Result<Vec<Overlap>> {
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
#[cfg(test)]
mod touches_tests {
    use super::*;
    use serde_json::json;
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
        format!(
            "## Touches\n\n- **Files:** {files}\n- **Session:** auto mode\n- **Route:** strong · effort high — a reason\n"
        )
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
