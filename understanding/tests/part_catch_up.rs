//! The part catch-up (#391): its facts builder, its reply's parsing and code checks, the claim check and retry, and the store,
//! all with a fake writer. No `gh`, no `claude`.

use serde_json::json;
use std::cell::RefCell;
use studio_understanding::part_catch_up::*;
use studio_understanding::pr_brief::{BriefReply, BriefWriter, CallUsage, TaggedTest};

const MAP: &str = r#"{
  "version": 1,
  "parts": [
    { "id": "alpha", "name": "Alpha", "caption": "c", "description": "d", "tier": "core", "paths": ["alpha/"] },
    { "id": "beta", "name": "Beta", "caption": "c", "description": "d", "tier": "core", "paths": ["beta/"] }
  ],
  "actors": [],
  "links": [ { "id": "alpha-beta", "from": "alpha", "to": "beta" } ],
  "ignored": [],
  "laws": [ { "id": "alpha-keeps-order", "on": "alpha", "backing": "check", "text": "Alpha never reorders what it was given." } ],
  "flows": [ { "id": "the-flow", "name": "The flow", "hops": [ { "link": "alpha-beta", "text": "Alpha asks Beta." } ] } ]
}"#;

fn file_diff(path: &str, lines: usize) -> String {
    let mut d =
        format!("diff --git a/{path} b/{path}\nindex 1..2 100644\n--- a/{path}\n+++ b/{path}\n");
    d += &format!("@@ -1,1 +1,{lines} @@\n");
    for i in 0..lines {
        d += &format!("+line {i} of {path}\n");
    }
    d
}

fn source() -> CatchUpSource {
    CatchUpSource {
        part: "alpha".into(),
        fog: vec!["c1".into(), "c2".into()],
        entries: vec![
            FogEntry {
                commit: "c1".into(),
                pr: Some(7),
                title: "Alpha keeps order".into(),
                body: "The PR body says Alpha now keeps order.".into(),
            },
            FogEntry {
                commit: "c2".into(),
                pr: None,
                title: "Quick fix pushed straight to main".into(),
                body: String::new(),
            },
        ],
        map_text: MAP.into(),
        map_diff: String::new(),
        diff: format!(
            "{}{}",
            file_diff("alpha/a.rs", 3),
            file_diff("beta/b.rs", 3)
        ),
        tagged: vec![TaggedTest {
            law: "alpha-keeps-order".into(),
            path: "alpha/tests.rs".into(),
            test: "keeps_order".into(),
            changed: false,
        }],
    }
}

fn section<'a>(text: &'a str, name: &str) -> &'a str {
    let head = format!("=== {name} ===\n");
    let at = text
        .find(&head)
        .unwrap_or_else(|| panic!("no section {name}"))
        + head.len();
    let rest = &text[at..];
    &rest[..rest.find("\n=== ").unwrap_or(rest.len())]
}

fn at(text: &str, name: &str) -> usize {
    text.find(&format!("=== {name} ===")).unwrap_or(usize::MAX)
}

// ---- the facts builder ----

#[test]
fn the_facts_put_the_parts_structure_before_the_entries_and_the_entries_before_the_code() {
    let facts = build_catch_up_facts(&source()).unwrap();
    let t = &facts.text;
    let order = [
        at(t, "SHAPE (decided by code)"),
        at(t, "SYSTEM MAP"),
        at(t, "MAP DIFF OVER THE FOG RANGE"),
        at(t, "THE PART"),
        at(t, "FOG ENTRIES"),
        at(t, "DIFF NOT SENT"),
        at(t, "DIFF"),
    ];
    assert!(order.iter().all(|i| *i != usize::MAX), "{t}");
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    let part = section(t, "THE PART");
    assert!(part.contains("alpha-keeps-order") && part.contains("keeps_order"));
    assert!(part.contains("the-flow"), "the flows through the part");
    let entries = section(t, "FOG ENTRIES");
    assert!(entries.contains("PR #7 Alpha keeps order"));
    assert!(entries.contains("The PR body says Alpha now keeps order."));
    assert!(entries.contains("Quick fix pushed straight to main"));
    // Code decides the shape: one story over the one part.
    assert!(!facts.shape.chapters);
    assert_eq!(facts.shape.required, ["alpha"]);
    assert!(facts.context.also.is_empty());
}

#[test]
fn the_facts_hold_one_combined_diff_of_the_parts_paths_only() {
    let facts = build_catch_up_facts(&source()).unwrap();
    let diff = section(&facts.text, "DIFF");
    assert_eq!(diff.matches("diff --git").count(), 1, "{diff}");
    assert!(diff.contains("alpha/a.rs"));
    assert!(!facts.text.contains("beta/b.rs"), "another part's file");
    assert_eq!(facts.text.matches("=== DIFF ===").count(), 1);
}

#[test]
fn over_budget_the_code_is_cut_first_and_the_facts_say_so() {
    let mut s = source();
    s.diff = file_diff("alpha/a.rs", 9000);
    s.entries[0].body = "A long PR body. ".repeat(200);
    let facts = build_catch_up_facts(&s).unwrap();
    assert!(facts.text.len() <= studio_understanding::pr_brief::BUDGET);
    // The entries are whole; the code is what was cut.
    assert!(
        section(&facts.text, "FOG ENTRIES").contains("A long PR body. ".repeat(200).trim_end())
    );
    assert!(section(&facts.text, "DIFF NOT SENT").contains("alpha/a.rs"));
    assert_eq!(facts.budget.files_partial + facts.budget.files_cut, 1);
}

#[test]
fn the_check_follows_the_brief_rule_a_changed_tagged_test_allows_a_pick_and_a_changed_flow_an_order()
 {
    let quiet = build_catch_up_facts(&source()).unwrap();
    assert!(!quiet.context.tagged.iter().any(|t| t.changed));
    assert!(quiet.context.flows_changed.is_empty());

    let mut s = source();
    s.diff += &file_diff("alpha/tests.rs", 2)
        .replace("+line 0", "+// @law: alpha-keeps-order\n+fn keeps_order");
    s.map_diff = "-      { \"link\": \"alpha-beta\", \"text\": \"Alpha asks Beta.\" }\n+      { \"link\": \"alpha-beta\", \"text\": \"Alpha tells Beta.\" } ] } ]\n  \"the-flow\"".into();
    let lively = build_catch_up_facts(&s).unwrap();
    assert!(lively.context.tagged.iter().any(|t| t.changed));
    assert!(section(&lively.text, "THE PART").contains("changed in the fog range"));
    assert_eq!(
        lively.context.flows_changed.get("the-flow"),
        Some(&vec!["alpha".to_string(), "beta".to_string()])
    );
}

#[test]
fn the_prompt_is_the_catch_ups_and_keeps_the_briefs_step_rules() {
    let facts = build_catch_up_facts(&source()).unwrap();
    let p = studio_understanding::pr_brief::prompt(&facts);
    assert!(p.starts_with("You write a part catch-up"), "{}", &p[..80]);
    assert!(p.contains("\"Must stay true\""), "the brief's step kinds");
    assert!(p.contains("\"kind\": \"pick\""), "the brief's checks");
    assert!(!p.contains("You write a PR brief"));
}

// ---- the reply ----

fn step(k: &str, h: &str, quote: &str) -> serde_json::Value {
    json!({ "k": k, "h": h, "b": "Alpha keeps the order it was given.", "on": ["alpha"], "cites": [ { "from": "ENTRIES", "quote": quote } ] })
}

/// A catch-up reply as the writer gives it (hand-made in the brief's reply format; no live writer runs in tests).
fn reply() -> serde_json::Value {
    json!({
        "title": "Alpha now keeps order",
        "chapters": [ { "behaviour": "story", "parts": ["alpha"], "title": "Alpha keeps order", "steps": [
            step("Where", "Alpha is where requests arrive", "Alpha keeps order"),
            step("Before", "Alpha could shuffle requests", "The PR body says Alpha now keeps order"),
            step("Now", "Alpha keeps the order it was given", "Quick fix pushed straight to main")
        ] } ],
        "also": [],
        "unseen": [],
        "decisions": []
    })
}

#[test]
fn a_recorded_catch_up_reply_parses_and_passes_the_code_checks() {
    let facts = build_catch_up_facts(&source()).unwrap();
    let mut brief = studio_understanding::pr_brief::parse_reply(&reply().to_string()).unwrap();
    assert_eq!(
        check_catch_up(&mut brief, &facts.context),
        0,
        "{:?}",
        brief.all_flags()
    );
}

#[test]
fn a_reply_breaking_the_briefs_rules_is_flagged() {
    let facts = build_catch_up_facts(&source()).unwrap();
    // Two steps only, a banned word, a part not in the map, and a pick check whose test is not in the diff.
    let mut r = reply();
    let steps = r["chapters"][0]["steps"].as_array_mut().unwrap();
    steps.pop();
    steps[0]["b"] = json!("This ensures order.");
    steps[1]["on"] = json!(["nowhere"]);
    steps[1]["k"] = json!("Your turn");
    steps[1]["check"] = json!({ "kind": "pick", "q": "q", "opts": ["a", "b"], "key": 0, "yes": "y", "no": "n", "test": "made_up_test" });
    let mut brief = studio_understanding::pr_brief::parse_reply(&r.to_string()).unwrap();
    check_catch_up(&mut brief, &facts.context);
    let flags = brief.all_flags().join("\n");
    assert!(flags.contains("2 steps"), "{flags}");
    assert!(flags.contains("banned word \"ensures\""), "{flags}");
    assert!(
        flags.contains("lights `nowhere`, not in the map"),
        "{flags}"
    );
    assert!(
        flags.contains("pick check cites test `made_up_test`, not in the diff"),
        "{flags}"
    );
}

// ---- the entry, with a fake writer ----

struct Canned {
    replies: RefCell<Vec<String>>,
    prompts: RefCell<Vec<String>>,
}

impl Canned {
    fn new(replies: Vec<String>) -> Self {
        Self {
            replies: RefCell::new(replies),
            prompts: RefCell::new(vec![]),
        }
    }
}

impl BriefWriter for Canned {
    fn write(&self, _model: &str, prompt: &str) -> Result<BriefReply, String> {
        self.prompts.borrow_mut().push(prompt.into());
        Ok(BriefReply {
            text: self.replies.borrow_mut().remove(0),
            usage: CallUsage::default(),
        })
    }
}

#[test]
fn the_entry_runs_the_claim_check_and_one_retry_and_stores_the_catch_up_by_part_and_fog() {
    let data = tempfile::tempdir().unwrap();
    let store = CatchUpStore::open(data.path()).unwrap();
    let flagged = json!({ "items": [ { "id": "step 3", "supported": false, "phrase": "keeps the order", "why": "the facts say nothing" } ] });
    let mut fixed = step("Now", "Alpha keeps order now", "Alpha keeps order");
    fixed["b"] = json!("The PR says Alpha keeps order.");
    let retry = json!({ "items": [ { "id": "step 3", "step": fixed } ] });
    let writer = Canned::new(vec![
        reply().to_string(),
        flagged.to_string(),
        retry.to_string(),
    ]);
    let stored = write_part_catch_up(&writer, "opus", &source(), &store, |_| {}).unwrap();

    let prompts = writer.prompts.borrow();
    assert_eq!(prompts.len(), 3, "writer, claim check, retry");
    assert!(prompts[1].contains("You check a written PR brief"));
    assert!(prompts[2].starts_with("You write a part catch-up") && prompts[2].contains("RETRY"));
    assert_eq!(stored.brief.chapters[0].steps[2].h, "Alpha keeps order now");
    assert!(
        stored.brief.all_flags().is_empty(),
        "{:?}",
        stored.brief.all_flags()
    );
    assert_eq!(
        (stored.part.as_str(), stored.fog.clone()),
        ("alpha", vec!["c1".to_string(), "c2".to_string()])
    );
    assert_eq!(store.get("alpha"), Some(stored.clone()));
    assert!(store.get("beta").is_none());
    let records = store.records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].claim_flags_before.len(), 1);
}
