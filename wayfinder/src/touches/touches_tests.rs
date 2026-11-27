use super::*;
use serde_json::json;
fn issue(number: u64, parent: Option<u64>, assignee: bool, blockers: &[u64], body: &str) -> Value {
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
    let stale = stale_touches(&pages(vec![ready, blocked, fine]), |path| path == "ui/").unwrap();
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
