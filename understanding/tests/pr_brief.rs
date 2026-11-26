//! The PR brief (#337): the facts builder, the code checks, the stored samples of the trial's v3 replies, the
//! store and the entry, all with a fake writer. No `gh`, no `claude`.

use serde_json::json;
use std::cell::RefCell;
use std::fmt::Write as _;
use std::path::Path;
use studio_understanding::pr_brief::*;

const MAP: &str = r#"{
  "version": 1,
  "parts": [
    { "id": "alpha", "name": "Alpha", "caption": "c", "description": "d", "tier": "core", "paths": ["alpha/"] },
    { "id": "beta", "name": "Beta", "caption": "c", "description": "d", "tier": "core", "paths": ["beta/"] },
    { "id": "gamma", "name": "Gamma", "caption": "c", "description": "d", "tier": "core", "paths": ["gamma/"] },
    { "id": "face", "name": "Face", "caption": "c", "description": "d", "tier": "interface", "paths": ["face/"] },
    { "id": "box", "name": "Box", "caption": "c", "description": "d", "tier": "black-box", "paths": ["box/"] }
  ],
  "actors": [ { "id": "daniel", "name": "Daniel" } ],
  "links": [
    { "id": "alpha-beta", "from": "alpha", "to": "beta" },
    { "id": "beta-gamma", "from": "beta", "to": "gamma" },
    { "id": "daniel-alpha", "from": "daniel", "to": "alpha" }
  ],
  "ignored": [],
  "laws": [
    { "id": "alpha-keeps-order", "on": "alpha", "backing": "check", "text": "Alpha never reorders what it was given." },
    { "id": "seen-gate", "on": "beta", "backing": "trust", "text": "A part is not seen until its chapter is read." }
  ],
  "flows": [
    { "id": "the-flow", "name": "The flow", "hops": [
      { "link": "daniel-alpha", "text": "Daniel asks Alpha." },
      { "link": "alpha-beta", "text": "Alpha asks Beta." },
      { "link": "beta-gamma", "text": "Beta asks Gamma." } ] }
  ]
}"#;

fn file_diff(path: &str, lines: usize) -> String {
    let mut d =
        format!("diff --git a/{path} b/{path}\nindex 1..2 100644\n--- a/{path}\n+++ b/{path}\n");
    // Writing to a `String` cannot fail.
    writeln!(d, "@@ -1,1 +1,{lines} @@").ok();
    for i in 0..lines {
        writeln!(d, "+line {i} of {path}").ok();
    }
    d
}

fn source(paths: &[&str]) -> PrSource {
    PrSource {
        meta: PrMeta {
            number: 7,
            title: "Make the thing".into(),
            body: "The PR body says Alpha now keeps order.".into(),
            head: "abc123".into(),
            checks: vec!["check SUCCESS".into()],
            ..PrMeta::default()
        },
        map_text: MAP.into(),
        diff: paths.iter().map(|p| file_diff(p, 3)).collect(),
        ..PrSource::default()
    }
}

fn section<'a>(facts: &'a Facts, name: &str) -> &'a str {
    let head = format!("=== {name} ===\n");
    let at = facts
        .text
        .find(&head)
        .unwrap_or_else(|| panic!("no section {name}"))
        + head.len();
    let rest = &facts.text[at..];
    &rest[..rest.find("\n=== ").unwrap_or(rest.len())]
}

// ---- the facts builder ----

#[test]
fn one_or_two_fogging_parts_are_a_single_story_and_three_are_chapters() {
    let one = build_facts(&source(&["alpha/a.rs"])).unwrap();
    assert!(!one.shape.chapters);
    assert_eq!(one.shape.required, ["alpha"]);
    assert!(section(&one, "SHAPE (decided by code)").starts_with("single story"));

    let two = build_facts(&source(&["alpha/a.rs", "beta/b.rs"])).unwrap();
    assert!(!two.shape.chapters);
    assert_eq!(two.shape.required.len(), 2);

    let three = build_facts(&source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"])).unwrap();
    assert!(three.shape.chapters);
    assert_eq!(three.shape.required.len(), 3);
    assert!(section(&three, "SHAPE (decided by code)").contains("required parts"));
    assert!(!three.shape.overview, "no flow changed");
}

#[test]
fn an_interface_part_fogs_only_when_the_maps_diff_names_it_and_a_black_box_never() {
    let mut s = source(&["alpha/a.rs", "face/f.rs", "box/x.rs"]);
    let quiet = build_facts(&s).unwrap();
    assert_eq!(quiet.shape.required, ["alpha"]);
    assert_eq!(quiet.shape.also, ["face", "box"]);

    s.map_diff = "-    { \"id\": \"face\", \"name\": \"Face\" }\n+    { \"id\": \"face\", \"name\": \"Face 2\" }".into();
    let named = build_facts(&s).unwrap();
    assert_eq!(named.shape.required.len(), 2);
    assert!(named.shape.required.contains(&"face".to_string()));
    assert_eq!(named.shape.also, ["box"]);

    // Daniel's own tier wins over the map's.
    s.tier_overrides.insert("box".into(), "core".into());
    assert!(build_facts(&s).unwrap().shape.chapters);
}

#[test]
fn the_budget_is_kept_structure_first_and_the_record_says_what_was_cut() {
    let mut s = source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"]);
    // A diff far over 64 KiB: a big core file, and a small one that must still be sent whole.
    s.diff = format!(
        "{}{}{}",
        file_diff("alpha/a.rs", 3000),
        file_diff("beta/b.rs", 5),
        file_diff("gamma/c.rs", 4)
    );
    let facts = build_facts(&s).unwrap();
    assert!(facts.text.len() <= BUDGET, "{} bytes", facts.text.len());
    assert_eq!(facts.budget.total_bytes, facts.text.len());
    assert_eq!(facts.budget.diff_bytes_full, s.diff.len());
    let names: Vec<&str> = facts
        .budget
        .sections
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "SHAPE (decided by code)",
            "SYSTEM MAP",
            "MAP DIFF IN THIS PR",
            "PARTS TABLE",
            "LAW TAGS ADDED",
            "CHECKS",
            "PR",
            "SPEC",
            "DIFF NOT SENT",
            "DIFF"
        ]
    );
    assert_eq!(
        facts.budget.sections.iter().map(|(_, b)| b).sum::<usize>(),
        facts.text.len()
    );
    // The two small files fit whole; the big one is partly sent or cut.
    assert_eq!(facts.budget.files_sent, 2);
    assert_eq!(facts.budget.files_partial + facts.budget.files_cut, 1);
    assert!(section(&facts, "DIFF NOT SENT").contains("alpha/a.rs"));
    assert!(section(&facts, "DIFF").contains("beta/b.rs"));

    let whole = build_facts(&source(&["alpha/a.rs"])).unwrap();
    assert_eq!(whole.budget.files_sent, 1);
    assert!(section(&whole, "DIFF NOT SENT").starts_with("nothing"));
}

#[test]
fn a_brief_written_from_a_cut_diff_stores_the_cut_and_partial_files_and_the_bytes() {
    let mut s = source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"]);
    // A file of many small hunks, partly sent, and one hunk far over the budget, not sent at all.
    let mut hunks = file_diff("alpha/a.rs", 100);
    for h in 1..40 {
        // Writing to a `String` cannot fail.
        writeln!(hunks, "@@ -{n},1 +{n},100 @@", n = h * 200).ok();
        for i in 0..100 {
            writeln!(hunks, "+hunk {h} line {i} of alpha/a.rs").ok();
        }
    }
    s.diff = format!(
        "{hunks}{}{}",
        file_diff("beta/b.rs", 5),
        file_diff("gamma/c.rs", 9000)
    );
    let (_, stored, record, _) = run_entry(vec![Ok(none_unsupported())], &single_story(), &s);
    let cut = stored.partial_facts.cut.expect("the diff was cut");
    assert_eq!(cut.files_partial, ["alpha/a.rs"]);
    assert_eq!(cut.files_cut, ["gamma/c.rs"]);
    assert_eq!(cut.files_partial.len(), record.budget.files_partial);
    assert_eq!(cut.files_cut.len(), record.budget.files_cut);
    assert_eq!(cut.bytes_sent + cut.bytes_cut, s.diff.len());
    assert!(cut.bytes_sent > 0 && cut.bytes_sent < BUDGET, "{cut:?}");

    let (_, whole, _, _) = run_entry(
        vec![Ok(none_unsupported())],
        &single_story(),
        &source(&["alpha/a.rs"]),
    );
    assert_eq!(whole.partial_facts.cut, None);
}

#[test]
fn a_failed_claim_check_stores_its_error_and_the_brief_is_still_stored() {
    for reply in [
        Err("`claude -p` failed".to_string()),
        Ok("I could not decide.".into()),
    ] {
        let (_, stored, record, _) = run_entry(
            vec![reply.clone()],
            &single_story(),
            &source(&["alpha/a.rs"]),
        );
        assert!(record.claim_error.is_some(), "{reply:?}");
        assert_eq!(stored.partial_facts.claim_error, record.claim_error);
    }
    let (_, checked, _, _) = run_entry(
        vec![Ok(none_unsupported())],
        &single_story(),
        &source(&["alpha/a.rs"]),
    );
    assert_eq!(checked.partial_facts.claim_error, None);
}

#[test]
fn a_language_counts_as_unread_by_the_fixed_table_and_the_maps_extractors() {
    let map = |extractors: &str| {
        MAP.replacen(
            "\"version\": 1,",
            &format!("\"version\": 1, {extractors}"),
            1,
        )
        .replace("\"ignored\": []", "\"ignored\": [\"box/\"]")
    };
    let paths = |ps: &[&str]| ps.iter().map(ToString::to_string).collect::<Vec<_>>();
    let none = map("");
    // The table's languages count, each extension once and sorted; files outside any part count too.
    assert_eq!(
        no_extractor(
            &none,
            &paths(&["alpha/a.go", "beta/b.py", "alpha/c.go", "x/y.kt"])
        )
        .unwrap(),
        [".go", ".kt", ".py"]
    );
    // CSS, HTML, JSON, YAML, Markdown and shell never count.
    assert!(
        no_extractor(
            &none,
            &paths(&[
                "alpha/a.css",
                "alpha/a.html",
                "alpha/a.json",
                "alpha/a.yml",
                "alpha/a.yaml",
                "alpha/a.md",
                "alpha/a.sh"
            ])
        )
        .unwrap()
        .is_empty()
    );
    // An ignored file never counts.
    assert!(
        no_extractor(&none, &paths(&["box/a.go"]))
            .unwrap()
            .is_empty()
    );
    // Rust and TypeScript count only when the map lacks their extractor.
    let code = paths(&["alpha/a.rs", "beta/b.ts", "beta/c.tsx", "gamma/d.mjs"]);
    assert_eq!(
        no_extractor(&none, &code).unwrap(),
        [".mjs", ".rs", ".ts", ".tsx"]
    );
    let rust = map("\"extractors\": [\"cargo\", \"rust-modules\"],");
    assert_eq!(no_extractor(&rust, &code).unwrap(), [".mjs", ".ts", ".tsx"]);
    let both = map("\"extractors\": [\"rust-modules\", \"typescript-imports\"],");
    assert!(no_extractor(&both, &code).unwrap().is_empty());
    // `cargo` alone reads crates, not a crate's modules.
    let cargo = map("\"extractors\": [\"cargo\"],");
    assert_eq!(
        no_extractor(&cargo, &paths(&["alpha/a.rs"])).unwrap(),
        [".rs"]
    );
}

#[test]
fn a_brief_stores_the_languages_no_extractor_reads() {
    let s = source(&["alpha/a.rs", "beta/b.go"]);
    let (_, stored, _, _) = run_entry(vec![Ok(none_unsupported())], &single_story(), &s);
    assert_eq!(stored.partial_facts.no_extractor, [".go", ".rs"]);
}

#[test]
fn the_diff_is_sent_required_parts_core_first_then_other_core_then_tests_then_the_rest() {
    let mut s = source(&[
        "alpha/tests/t.rs",
        "zzz/x.rs",
        "alpha/a.rs",
        "face/f.rs",
        "alpha/notes.md",
        "beta/b.rs",
    ]);
    s.tier_overrides.insert("face".into(), "core".into());
    s.tier_overrides.insert("beta".into(), "core".into());
    s.tier_overrides.insert("gamma".into(), "core".into());
    let facts = build_facts(&s).unwrap();
    let diff = section(&facts, "DIFF");
    let at = |p: &str| {
        diff.find(&format!("diff --git a/{p}"))
            .unwrap_or_else(|| panic!("{p} not sent"))
    };
    assert!(at("alpha/a.rs") < at("alpha/tests/t.rs"));
    assert!(at("beta/b.rs") < at("alpha/tests/t.rs"));
    assert!(at("alpha/tests/t.rs") < at("alpha/notes.md"));
    assert!(at("alpha/notes.md") < at("zzz/x.rs"));
}

#[test]
fn the_parts_table_gives_tier_laws_with_their_tests_and_whether_this_pr_changed_one() {
    let mut s = source(&["alpha/a.rs", "alpha/tests/t.rs", "alpha/notes.md"]);
    s.diff = format!(
        "{}{}{}",
        file_diff("alpha/a.rs", 9),
        "diff --git a/alpha/tests/t.rs b/alpha/tests/t.rs\n--- a/alpha/tests/t.rs\n+++ b/alpha/tests/t.rs\n@@ -1,1 +1,3 @@\n+// @law: alpha-keeps-order\n+fn order_is_kept() {}\n",
        file_diff("alpha/notes.md", 1)
    );
    s.tagged = vec![
        TaggedTest {
            law: "alpha-keeps-order".into(),
            path: "alpha/tests/t.rs".into(),
            test: "order_is_kept".into(),
            changed: false,
        },
        TaggedTest {
            law: "seen-gate".into(),
            path: "beta/tests/s.rs".into(),
            test: "gate_holds".into(),
            changed: false,
        },
    ];
    let facts = build_facts(&s).unwrap();
    let table = section(&facts, "PARTS TABLE");
    assert!(table.contains("## alpha (Alpha, tier core)"));
    assert!(table.contains("order_is_kept in alpha/tests/t.rs (changed in this PR)"));
    assert!(table.contains("flows through it: the-flow"));
    // Core first, then tests, then plumbing.
    let at = |needle: &str| table.find(needle).unwrap();
    assert!(at("- core alpha/a.rs +9/-0") < at("- test alpha/tests/t.rs"));
    assert!(at("- test alpha/tests/t.rs") < at("- plumbing alpha/notes.md"));
    assert!(
        section(&facts, "LAW TAGS ADDED").contains("law `alpha-keeps-order` in alpha/tests/t.rs")
    );
    let ctx = &facts.context;
    assert!(
        ctx.tagged
            .iter()
            .any(|t| t.law == "alpha-keeps-order" && t.changed)
    );
    // A law on a part this PR did not touch is not in its table, and its test stays unchanged.
    assert!(!table.contains("gate_holds"));
    assert!(!ctx.tagged.iter().any(|t| t.law == "seen-gate" && t.changed));
}

// ---- Guard with a law (#401): read by code from the facts ----

/// A bug fix's source: it closes an issue labelled `bug` and touches Alpha and Beta, whose law `seen-gate` has a tagged test.
fn bug_fix() -> PrSource {
    let mut s = source(&["alpha/a.rs", "beta/b.rs"]);
    s.meta.bug = true;
    s.tagged = vec![TaggedTest {
        law: "seen-gate".into(),
        path: "beta/tests/s.rs".into(),
        test: "gate_holds".into(),
        changed: false,
    }];
    s
}

#[test]
fn guard_with_a_law_is_offered_for_a_bug_fix_that_adds_or_changes_no_tagged_test() {
    let guard = build_facts(&bug_fix()).unwrap().guard;
    assert_eq!(
        guard,
        Some(Guard {
            parts: vec!["alpha".into(), "beta".into()]
        })
    );

    // No issue labelled `bug` is closed: no offer.
    let mut plain = bug_fix();
    plain.meta.bug = false;
    assert_eq!(build_facts(&plain).unwrap().guard, None);

    // The fix changed a tagged test: no offer.
    let mut tested = bug_fix();
    tested.diff += "diff --git a/beta/tests/s.rs b/beta/tests/s.rs\n--- a/beta/tests/s.rs\n+++ b/beta/tests/s.rs\n@@ -1,1 +1,2 @@\n fn gate_holds() {\n+    assert!(true);\n";
    assert_eq!(build_facts(&tested).unwrap().guard, None);

    // The fix added a tagged test: no offer. The tag is built in two pieces so the map check does not read it as one.
    let mut added = bug_fix();
    // Writing to a `String` cannot fail.
    write!(
        added.diff,
        "diff --git a/alpha/tests/t.rs b/alpha/tests/t.rs\n--- /dev/null\n+++ b/alpha/tests/t.rs\n@@ -0,0 +1,2 @@\n+// {}: alpha-keeps-order\n+fn order_is_kept() {{}}\n",
        concat!("@", "law")
    ).ok();
    assert_eq!(build_facts(&added).unwrap().guard, None);

    // The fix touched no part of the map: there is no law to back and no part to add one on, so no offer.
    let mut unmapped = bug_fix();
    unmapped.diff = file_diff("elsewhere/notes.txt", 3);
    assert_eq!(build_facts(&unmapped).unwrap().guard, None);
}

#[test]
fn a_closing_issue_is_a_bug_when_gh_says_it_is_labelled_bug() {
    assert!(labelled_bug(
        &json!({"labels": [{"name": "studio"}, {"name": "bug"}]})
    ));
    assert!(!labelled_bug(
        &json!({"labels": [{"name": "enhancement"}, {"name": "bugfix"}]})
    ));
    assert!(!labelled_bug(&json!({})));
}

#[test]
fn the_stored_brief_keeps_its_guard_so_it_stays_after_merge() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let writer = Canned::new(vec![
        Ok(single_story().to_string()),
        Ok(none_unsupported()),
        Ok(no_rewrites()),
    ]);
    let mut fix = bug_fix();
    fix.diff = file_diff("alpha/a.rs", 3);
    let stored = write_pr_brief(&writer, "opus", &fix, &store, drop).unwrap();
    let guard = Some(Guard {
        parts: vec!["alpha".into()],
    });
    assert_eq!(stored.guard, guard);
    assert_eq!(store.latest(7).unwrap().guard, guard);
}

#[test]
fn the_spec_is_trimmed_to_user_stories_and_decisions_and_a_ticket_to_what_to_build() {
    let spec = "## Problem\nlong\n## User Stories\nstory\n## Implementation Decisions\ndecision\n## Out of scope\nno";
    assert_eq!(
        trim_spec(spec),
        "## User Stories\nstory\n## Implementation Decisions\ndecision\n"
    );
    let ticket =
        "## Parent\n#1\n## What to build\nbuild\n## Acceptance criteria\n- [ ] a\n## Reading\nx";
    assert_eq!(
        trim_spec(ticket),
        "## What to build\nbuild\n## Acceptance criteria\n- [ ] a\n"
    );
}

// ---- a flow change ----

fn flow_source() -> PrSource {
    let mut s = source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"]);
    s.map_diff = "diff --git a/system-map.json b/system-map.json\n-      { \"link\": \"beta-gamma\", \"text\": \"old\" }\n+ \"the-flow\" now ends at Gamma".into();
    s
}

fn step(k: &str, on: &[&str]) -> serde_json::Value {
    json!({ "k": k, "h": "Alpha keeps order", "b": "Alpha now keeps order.", "on": on,
            "cites": [{ "from": "PR", "quote": "Alpha now keeps order" }] })
}

fn good_brief() -> serde_json::Value {
    json!({
        "title": "Alpha keeps order",
        "overview": [step("Now", &["alpha", "beta"])],
        "chapters": [
            { "behaviour": "order", "parts": ["alpha", "beta"], "title": "Order", "gist": "Alpha now keeps order.", "steps": [step("Before", &["alpha"]), step("Now", &["alpha", "beta"]), step("Could break", &["beta"])] },
            { "behaviour": "reach", "parts": ["gamma"], "title": "Reach", "gist": "Gamma is reached.", "steps": [step("Before", &["gamma"]), step("Now", &["gamma"]), step("Not in the plan", &["gamma"])] }
        ],
        "also": [], "unseen": [], "decisions": []
    })
}

fn checked(facts: &Facts, brief: serde_json::Value) -> Brief {
    let mut brief: Brief = serde_json::from_value(brief).unwrap();
    check_brief(&mut brief, &facts.context);
    brief
}

#[test]
fn a_synthetic_flow_change_yields_an_overview_and_an_order_check_keyed_by_the_flows_hops() {
    let facts = build_facts(&flow_source()).unwrap();
    assert!(facts.shape.overview);
    assert_eq!(facts.shape.flows_changed, ["the-flow"]);
    assert!(
        section(&facts, "SHAPE (decided by code)").contains("overview: yes, for flow(s) the-flow")
    );
    // The writer gave the right parts in the wrong order and the check on the last overview step.
    let mut b = good_brief();
    b["overview"][0]["check"] = json!({ "kind": "order", "q": "Which way?", "key": ["gamma", "beta", "alpha"], "yes": "y", "no": "n", "flow": "the-flow" });
    b["overview"][0]["k"] = json!("Your turn");
    let brief = checked(&facts, b);
    assert_eq!(brief.flagged(), 0, "{:?}", brief.all_flags());
    let key = brief.overview[0].check.as_ref().unwrap().key.clone();
    assert_eq!(
        key,
        json!(["alpha", "beta", "gamma"]),
        "the key is the flow's hops: the actor is left out"
    );
}

#[test]
fn an_order_check_whose_parts_disagree_with_the_flow_is_dropped_and_one_on_an_unchanged_flow_is_flagged()
 {
    let facts = build_facts(&flow_source()).unwrap();
    let mut b = good_brief();
    b["overview"][0]["k"] = json!("Your turn");
    b["overview"][0]["check"] = json!({ "kind": "order", "q": "?", "key": ["alpha", "gamma"], "yes": "y", "no": "n", "flow": "the-flow" });
    let brief = checked(&facts, b);
    assert!(brief.overview[0].check.is_none());
    assert!(
        brief.overview[0]
            .flags
            .iter()
            .any(|f| f.contains("dropped"))
    );

    let plain = build_facts(&source(&["alpha/a.rs"])).unwrap();
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["check"] =
        json!({ "kind": "order", "q": "?", "key": [], "yes": "y", "no": "n", "flow": "the-flow" });
    let brief = checked(&plain, b);
    assert!(
        brief.chapters[0].steps[0]
            .flags
            .iter()
            .any(|f| f.contains("does not change"))
    );
}

// ---- the code checks ----

fn single_story() -> serde_json::Value {
    json!({
        "title": "Alpha keeps order",
        "chapters": [{ "behaviour": "story", "parts": ["alpha"], "title": "Order", "gist": "Alpha now keeps order.",
            "steps": [step("Where", &["alpha"]), step("Before", &["alpha"]), step("Now", &["alpha"])] }],
    })
}

fn story_facts() -> Facts {
    build_facts(&source(&["alpha/a.rs"])).unwrap()
}

fn flags_of(brief: &Brief) -> Vec<String> {
    brief.all_flags()
}

#[test]
fn a_good_brief_has_no_flags() {
    let facts = story_facts();
    assert_eq!(
        flags_of(&checked(&facts, single_story())),
        Vec::<String>::new()
    );
    let facts = build_facts(&source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"])).unwrap();
    assert_eq!(
        flags_of(&checked(&facts, good_brief_without_overview())),
        Vec::<String>::new()
    );
}

fn good_brief_without_overview() -> serde_json::Value {
    let mut b = good_brief();
    b.as_object_mut().unwrap().remove("overview");
    b
}

// ---- the gist (#397) ----

#[test]
fn a_core_chapter_carries_a_one_sentence_gist_and_a_chapter_of_no_core_part_carries_none() {
    // Gamma is Interface-only by Daniel's choice and named in the map's diff, so it fogs without being Core.
    let mut s = source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"]);
    s.tier_overrides.insert("gamma".into(), "interface".into());
    s.map_diff = "+ \"gamma\" moved".into();
    let facts = build_facts(&s).unwrap();
    assert!(
        section(&facts, "SHAPE (decided by code)")
            .contains("core parts (a chapter lighting one carries a gist): alpha, beta"),
        "{}",
        section(&facts, "SHAPE (decided by code)")
    );
    let mut b = good_brief_without_overview();
    b["chapters"][0]["gist"] = json!("Alpha now keeps the order it was given.");
    b["chapters"][1]["gist"] = json!("Gamma is reached.");
    let p = prompt(&facts);
    assert!(
        p.contains("\"gist\": \"only on a chapter that lights a Core part"),
        "the writer's reply schema asks for the gist"
    );
    let brief = checked(&facts, b.clone());
    assert_eq!(flags_of(&brief), Vec::<String>::new());
    assert_eq!(
        brief.chapters[0].gist,
        "Alpha now keeps the order it was given."
    );
    assert_eq!(
        brief.chapters[1].gist, "",
        "a chapter lighting no Core part has no gist"
    );

    // A Core chapter without a gist, or with more than one sentence or a banned word, is flagged on its gist.
    for (gist, needle) in [
        ("", "a Core chapter without a gist"),
        (
            "Alpha keeps order. Beta follows it.",
            "more than one sentence",
        ),
        (
            "Alpha keeps order and it can't lose any.",
            "banned word \"can't\"",
        ),
    ] {
        let mut b = b.clone();
        b["chapters"][0]["gist"] = json!(gist);
        let brief = checked(&facts, b);
        assert!(
            brief.chapters[0]
                .gist_flags
                .iter()
                .any(|f| f.contains(needle)),
            "{gist}: {:?}",
            brief.chapters[0].gist_flags
        );
        assert!(
            flags_of(&brief)
                .iter()
                .any(|f| f.starts_with("gist order: "))
        );
        assert_eq!(brief.flagged(), 1);
    }
}

#[test]
fn every_part_actor_link_law_and_flow_id_must_exist() {
    let facts = story_facts();
    let mut b = single_story();
    let s = &mut b["chapters"][0]["steps"][2];
    s["on"] = json!(["alpha", "nowhere"]);
    s["hops"] = json!([["no-such-link", false]]);
    s["law"] = json!("no-such-law");
    s["risks"] = json!([{ "part": "ghost", "trigger": "t", "check": "c" }]);
    b["chapters"][0]["parts"] = json!(["alpha", "ghost"]);
    let brief = checked(&facts, b);
    let flags = flags_of(&brief).join("\n");
    for needle in [
        "lights `nowhere`",
        "hop `no-such-link`",
        "law `no-such-law`",
        "risk on `ghost`",
        "declares `ghost`",
    ] {
        assert!(flags.contains(needle), "{needle} in\n{flags}");
    }
    // An actor may be lit.
    let mut b = single_story();
    b["chapters"][0]["steps"][2]["on"] = json!(["alpha", "daniel"]);
    assert!(flags_of(&checked(&facts, b)).is_empty());
}

#[test]
fn the_chapters_must_light_every_required_part_and_each_its_declared_parts() {
    let facts = build_facts(&source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"])).unwrap();
    let mut b = good_brief_without_overview();
    b["chapters"][1]["steps"] = json!([
        step("Before", &["alpha"]),
        step("Now", &["alpha"]),
        step("Not in the plan", &["alpha"])
    ]);
    let brief = checked(&facts, b);
    let flags = flags_of(&brief).join("\n");
    assert!(
        flags.contains(
            "required part `gamma` lit by no chapter (in Also touched: it keeps its fog)"
        ),
        "{flags}"
    );
    assert!(
        flags.contains("reach: declares `gamma` but no step lights it"),
        "{flags}"
    );
    assert_eq!(brief.flags.len(), 1);
}

/// The map with ignored paths, for Also touched.
fn ignoring_map() -> String {
    MAP.replace(
        r#""ignored": []"#,
        r#""ignored": ["docs/", "scripts/", ".github/", "Cargo.lock", "README.md"]"#,
    )
}

#[test]
fn also_touched_sorts_the_files_of_no_lit_part_by_kind_the_first_match_winning() {
    let mut s = source(&[
        // Alpha is lit: none of its files, its tests included, is in Also touched.
        "alpha/a.rs",
        "alpha/tests/a.rs",
        // Beta is touched and lit by no chapter.
        "beta/b.rs",
        "beta/tests/order.rs",
        "beta/b.test.mjs",
        "beta/conf.toml",
        "beta/notes.md",
        // Owned by no part and not ignored: No part, whatever its kind.
        "loose/x_test.go",
        // Ignored, of a kind: tagged "ignored".
        "scripts/run.spec.ts",
        ".github/ci.sh",
        "Cargo.lock",
        "docs/guide.txt",
        "README.md",
        // Ignored and of no kind.
        "scripts/tool.sh",
    ]);
    s.map_text = ignoring_map();
    s.diff = s.diff.replace(
        "+line 0 of beta/tests/order.rs",
        "+line 0 of beta/tests/order.rs\n+// @law: seen-gate",
    );
    s.tagged = vec![TaggedTest {
        law: "seen-gate".into(),
        path: "beta/tests/order.rs".into(),
        test: "beta_keeps_order".into(),
        changed: false,
    }];
    let facts = build_facts(&s).unwrap();
    let brief = checked(&facts, single_story());
    // Each group as its kind, its files (path, ignored) and its lines.
    type Group<'a> = (AlsoKind, Vec<(&'a str, bool)>, usize);
    let groups: Vec<Group> = brief
        .also_groups
        .iter()
        .map(|g| {
            (
                g.kind,
                g.files
                    .iter()
                    .map(|f| (f.path.as_str(), f.ignored))
                    .collect(),
                g.lines,
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            (AlsoKind::NoPart, vec![("loose/x_test.go", false)], 3),
            (
                AlsoKind::Tests,
                vec![
                    ("beta/b.test.mjs", false),
                    ("beta/tests/order.rs", false),
                    ("scripts/run.spec.ts", true)
                ],
                10
            ),
            (
                AlsoKind::Config,
                vec![
                    (".github/ci.sh", true),
                    ("Cargo.lock", true),
                    ("beta/conf.toml", false)
                ],
                9
            ),
            (
                AlsoKind::Docs,
                vec![
                    ("README.md", true),
                    ("beta/notes.md", false),
                    ("docs/guide.txt", true)
                ],
                9
            ),
            (AlsoKind::Ignored, vec![("scripts/tool.sh", true)], 3),
            (AlsoKind::Other, vec![("beta/b.rs", false)], 3),
        ]
    );
    let tests = &brief.also_groups[1];
    assert_eq!(
        tests.files[1].laws,
        ["seen-gate"],
        "a tagged test names its law"
    );
    assert_eq!(tests.files[1].part.as_deref(), Some("beta"));
    assert!(tests.files[0].laws.is_empty());
}

#[test]
fn the_caps_are_checked() {
    let facts = build_facts(&source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"])).unwrap();
    let long = "word ".repeat(46);
    let mut b = good_brief_without_overview();
    b["chapters"][0]["steps"][0]["h"] = json!(
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen"
    );
    b["chapters"][0]["steps"][0]["b"] = json!(long);
    b["chapters"][0]["steps"][1]["fig"] = json!({ "kind": "compare", "before": ["a label far over eighteen characters"], "after": [] });
    b["chapters"][0]["steps"][2]["risks"] =
        json!([1, 2, 3, 4].map(|_| json!({ "part": "beta", "trigger": "t", "check": "c" })));
    b["chapters"][1]["steps"]
        .as_array_mut()
        .unwrap()
        .truncate(2);
    let flags = flags_of(&checked(&facts, b)).join("\n");
    for needle in [
        "heading 15 words",
        "body 46 words",
        "figure label",
        "more than 3 risks",
        "reach: 2 steps",
    ] {
        assert!(flags.contains(needle), "{needle} in\n{flags}");
    }
    // Too many chapters and steps in all, and too long a single story.
    let mut b = good_brief_without_overview();
    for _ in 0..3 {
        let c = b["chapters"][0].clone();
        b["chapters"].as_array_mut().unwrap().push(c);
    }
    let flags = flags_of(&checked(&facts, b)).join("\n");
    assert!(
        flags.contains("5 chapters") && flags.contains("15 steps in all"),
        "{flags}"
    );
    let facts = story_facts();
    let mut b = single_story();
    for _ in 0..5 {
        let s = b["chapters"][0]["steps"][0].clone();
        b["chapters"][0]["steps"].as_array_mut().unwrap().push(s);
    }
    assert!(
        flags_of(&checked(&facts, b))
            .join("\n")
            .contains("single story: 8 steps")
    );
    let mut b = single_story();
    b["overview"] = json!([step("Now", &["alpha"])]);
    assert!(
        flags_of(&checked(&facts, b))
            .join("\n")
            .contains("an overview where SHAPE said none")
    );
}

#[test]
fn a_quote_must_be_in_the_facts_whatever_markdown_wraps_it_there() {
    let mut s = source(&["alpha/a.rs"]);
    s.meta.body =
        "The change makes **Worker and check cards** lose their `Quiz` when _the turn_ ended."
            .into();
    let facts = build_facts(&s).unwrap();
    let with = |quote: &str| {
        let mut b = single_story();
        b["chapters"][0]["steps"][0]["cites"] = json!([{ "from": "PR", "quote": quote }]);
        checked(&facts, b).chapters[0].steps[0].flags.clone()
    };
    // The trial's false alarm: the facts have the quote Markdown-bolded, backticked and underscored.
    assert!(with("Worker and check cards lose their Quiz when the turn ended").is_empty());
    assert!(
        with("Worker   and check\ncards lose").is_empty(),
        "whitespace is folded"
    );
    let wrong = with("Worker and check cards keep their Quiz");
    assert_eq!(wrong.len(), 1);
    assert!(wrong[0].contains("unconfirmed"));
    assert!(with("").iter().any(|f| f.contains("unconfirmed")));
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["cites"] = json!([]);
    assert!(
        checked(&facts, b).chapters[0].steps[0]
            .flags
            .contains(&"no cites".to_string())
    );
}

#[test]
fn banned_words_are_flagged_in_the_writers_own_prose_only() {
    let facts = story_facts();
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["b"] = json!("This ensures Alpha keeps order.");
    b["chapters"][0]["steps"][1]["h"] = json!("It cannot fail");
    b["chapters"][0]["steps"][2]["b"] = json!("It always works.");
    b["decisions"] = json!([{ "text": "A guarantee, they say" }]);
    b["title"] = json!("Impossible to lose order");
    let brief = checked(&facts, b);
    let flags = flags_of(&brief).join("\n");
    for needle in [
        "banned word \"ensures\"",
        "banned word \"cannot\"",
        "banned word \"always works\"",
        "banned word \"guarantee\"",
    ] {
        assert!(flags.contains(needle), "{needle} in\n{flags}");
    }
    assert!(brief.flags.iter().any(|f| f.contains("impossible")));
    // A word inside a longer one is no match, and a law's own map text and a cited quote are not the writer's prose.
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["b"] = json!("Ensuring is not a word of ours; unsure either.");
    assert!(flags_of(&checked(&facts, b)).is_empty());
    let mut s = source(&["alpha/a.rs"]);
    s.meta.body = "The PR cannot lose order. Alpha now keeps order.".into();
    let facts = build_facts(&s).unwrap();
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["cites"] =
        json!([{ "from": "PR", "quote": "The PR cannot lose order. Alpha now keeps order." }]);
    assert!(
        flags_of(&checked(&facts, b)).is_empty(),
        "a quote is not the writer's prose"
    );
    let facts = build_facts(&source(&["alpha/a.rs"])).unwrap();
    let mut b = single_story();
    b["chapters"][0]["steps"][1]["law"] = json!("alpha-keeps-order");
    b["chapters"][0]["steps"][1]["b"] = json!("Alpha never reorders what it was given.");
    assert!(flags_of(&checked(&facts, b)).is_empty());
}

#[test]
fn the_word_seen_and_prose_about_the_law_feature_itself_are_no_leak_and_no_coverage_claim() {
    // The trial's two other false alarms: a step about the map's "seen" feature ("not seen"), and one describing
    // laws and their tagged tests with nothing claimed about this PR's tests.
    let facts = story_facts();
    let mut b = single_story();
    b["chapters"][0]["steps"][1]["b"] =
        json!("A part that is not seen yet keeps its fog until its chapter is read.");
    b["chapters"][0]["steps"][2]["b"] =
        json!("The map shows a law, and the test tagged to law X is what a test covers.");
    b["chapters"][0]["steps"][2]["law"] = json!("seen-gate");
    assert!(flags_of(&checked(&facts, b)).is_empty());
}

#[test]
fn covered_by_must_name_a_changed_tagged_test_of_the_steps_law() {
    let mut s = source(&["alpha/a.rs", "alpha/tests/t.rs", "beta/tests/s.rs"]);
    s.diff = format!(
        "{}{}{}",
        file_diff("alpha/a.rs", 2),
        "diff --git a/alpha/tests/t.rs b/alpha/tests/t.rs\n--- a/alpha/tests/t.rs\n+++ b/alpha/tests/t.rs\n@@ -1,1 +1,3 @@\n+// @law: alpha-keeps-order\n+fn order_is_kept() {}\n",
        file_diff("beta/tests/s.rs", 2)
    );
    s.tagged = vec![
        TaggedTest {
            law: "alpha-keeps-order".into(),
            path: "alpha/tests/t.rs".into(),
            test: "order_is_kept".into(),
            changed: false,
        },
        TaggedTest {
            law: "seen-gate".into(),
            path: "beta/tests/s.rs".into(),
            test: "gate_holds".into(),
            changed: false,
        },
    ];
    let facts = build_facts(&s).unwrap();
    let with = |law: Option<&str>, test: &str| {
        let mut b = single_story();
        b["chapters"][0]["steps"][2]["law"] = json!(law);
        b["chapters"][0]["steps"][2]["covered_by"] = json!(test);
        checked(&facts, b).chapters[0].steps[2].flags.clone()
    };
    assert!(with(Some("alpha-keeps-order"), "order_is_kept").is_empty());
    assert!(
        with(
            Some("alpha-keeps-order"),
            "order_is_kept in alpha/tests/t.rs"
        )
        .is_empty()
    );
    assert!(
        !with(Some("alpha-keeps-order"), "gate_holds").is_empty(),
        "another law's test"
    );
    assert!(
        !with(Some("seen-gate"), "gate_holds").is_empty(),
        "a tagged test this PR did not change"
    );
    assert!(!with(None, "order_is_kept").is_empty(), "no law");
    assert!(!with(Some("alpha-keeps-order"), "").is_empty());
}

#[test]
fn leak_phrases_are_flagged_and_your_turn_must_hold_a_check() {
    let facts = story_facts();
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["b"] = json!("The rest of the diff was not sent.");
    b["chapters"][0]["steps"][1]["b"] = json!("Part of it was cut.");
    b["chapters"][0]["steps"][2]["k"] = json!("Your turn");
    let flags = flags_of(&checked(&facts, b)).join("\n");
    assert!(
        flags.contains("\"not sent\"") || flags.contains("was not sent"),
        "{flags}"
    );
    assert!(flags.contains("\"was cut\""), "{flags}");
    assert!(flags.contains("\"Your turn\" with no check"), "{flags}");
}

#[test]
fn a_pick_checks_test_must_be_in_the_diff() {
    let mut s = source(&["alpha/a.rs"]);
    s.diff += "diff --git a/alpha/tests/t.rs b/alpha/tests/t.rs\n--- a/alpha/tests/t.rs\n+++ b/alpha/tests/t.rs\n@@ -1,1 +1,2 @@\n+fn order_is_kept() {}\n";
    let facts = build_facts(&s).unwrap();
    let with = |test: &str| {
        let mut b = single_story();
        b["chapters"][0]["steps"][2]["k"] = json!("Your turn");
        b["chapters"][0]["steps"][2]["check"] = json!({ "kind": "pick", "q": "q", "opts": ["a", "b"], "key": 0, "yes": "y", "no": "n", "test": test });
        checked(&facts, b).chapters[0].steps[2].flags.clone()
    };
    assert!(with("order_is_kept").is_empty());
    assert!(with("alpha/tests/t.rs::order_is_kept()").is_empty());
    assert!(with("never_written")[0].contains("not in the diff"));
    assert!(!with("").is_empty());
}

#[test]
fn at_most_one_check_and_unknown_kinds_are_flagged() {
    let facts = story_facts();
    let mut b = single_story();
    for i in [1, 2] {
        b["chapters"][0]["steps"][i]["check"] =
            json!({ "kind": "poll", "q": "q", "yes": "y", "no": "n" });
    }
    let brief = checked(&facts, b);
    assert!(brief.flags.contains(&"2 checks".to_string()));
    assert!(
        brief.chapters[0].steps[1]
            .flags
            .iter()
            .any(|f| f.contains("check kind"))
    );
}

// ---- the trial's v3 replies, stored ----

fn sample(name: &str) -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored/pr-brief-v3");
    std::fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn sample_context(pr: &str) -> CheckContext {
    let mut ctx: CheckContext =
        serde_json::from_str(&sample(&format!("{pr}-context.json"))).unwrap();
    ctx.facts = sample(&format!("{pr}-facts.txt"));
    ctx
}

// @law: old-stored-files-load
#[test]
fn the_trials_v3_replies_for_274_and_317_parse_and_pass_every_check() {
    for (pr, steps, chapters) in [("274", 11, 3), ("317", 12, 3)] {
        let mut brief = parse_reply(&sample(&format!("{pr}-reply.json"))).unwrap();
        assert_eq!(brief.chapters.len(), chapters, "#{pr}");
        assert_eq!(brief.steps().count(), steps, "#{pr}");
        assert!(
            brief.decisions.len() >= 5 && brief.decisions.iter().all(|d| d.id.is_none()),
            "bare sentences read as decisions"
        );
        assert!(
            brief.chapters.iter().all(|c| c.gist.is_empty()),
            "#{pr}: written before the gist"
        );
        check_brief(&mut brief, &sample_context(pr));
        assert_eq!(
            brief.all_flags(),
            Vec::<String>::new(),
            "#{pr}: the trial's three false alarms are gone"
        );
    }
}

#[test]
fn the_same_replies_as_the_bare_object_or_inside_prose_parse_too() {
    let envelope: serde_json::Value = serde_json::from_str(&sample("274-reply.json")).unwrap();
    let text = envelope["result"].as_str().unwrap();
    let bare = parse_reply(text).unwrap();
    let fenced = parse_reply(&format!("Here it is:\n```json\n{text}\n```\n")).unwrap();
    assert_eq!(bare, fenced);
    assert!(parse_reply("no json here").is_err());
    assert!(parse_reply("{\"title\": ").is_err());
}

#[test]
fn a_reply_with_a_closing_brace_before_the_first_opening_one_is_an_error_not_a_panic() {
    let text = "Sorry :} I need {more context";
    assert!(parse_reply(text).is_err());
    let envelope: serde_json::Value = serde_json::from_str(&sample("274-reply.json")).unwrap();
    let brief = parse_reply(envelope["result"].as_str().unwrap()).unwrap();
    assert!(studio_understanding::pr_brief_check::read_claim_reply(text, &brief).is_err());
}

// ---- the store and the entry ----

struct Canned {
    replies: RefCell<Vec<Result<String, String>>>,
    prompts: RefCell<Vec<(String, String)>>,
    /// How long each call takes, in milliseconds.
    delay: u64,
}

impl Canned {
    fn new(replies: Vec<Result<String, String>>) -> Self {
        Self {
            replies: RefCell::new(replies),
            prompts: RefCell::new(vec![]),
            delay: 0,
        }
    }

    fn slow(self, delay: u64) -> Self {
        Self { delay, ..self }
    }
}

impl BriefWriter for Canned {
    fn write(&self, model: &str, prompt: &str) -> Result<BriefReply, String> {
        self.prompts
            .borrow_mut()
            .push((model.into(), prompt.into()));
        std::thread::sleep(std::time::Duration::from_millis(self.delay));
        self.replies.borrow_mut().remove(0).map(|text| BriefReply {
            text,
            usage: CallUsage {
                model: model.into(),
                input_tokens: 100,
                output_tokens: 50,
                cost_usd: 0.25,
                ..CallUsage::default()
            },
        })
    }
}

fn none_unsupported() -> String {
    json!({ "items": [] }).to_string()
}

fn no_rewrites() -> String {
    json!({ "items": [] }).to_string()
}

fn unsupported(id: &str, phrase: &str, why: &str) -> serde_json::Value {
    json!({ "id": id, "supported": false, "phrase": phrase, "why": why })
}

fn checker(items: Vec<serde_json::Value>) -> String {
    json!({ "items": items }).to_string()
}

// Sync is not needed: the entry takes any writer.
#[test]
fn the_entry_writes_checks_stores_and_records_one_brief() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let mut b = single_story();
    b["decisions"] = json!(["Alpha chose order over speed.", { "text": "Gamma was left alone." }]);
    b["chapters"][0]["steps"][0]["b"] = json!("This ensures order.");
    // The claim check finds nothing; the retry leaves the flagged step as it is.
    let writer = Canned::new(vec![
        Ok(b.to_string()),
        Ok(none_unsupported()),
        Ok(no_rewrites()),
    ]);
    let mut log = Vec::new();
    let stored = write_pr_brief(&writer, "opus", &source(&["alpha/a.rs"]), &store, |l| {
        log.push(l);
    })
    .unwrap();

    assert!(log.is_empty());
    let (model, prompt) = writer.prompts.borrow()[0].clone();
    assert_eq!(model, "opus");
    assert!(prompt.contains("a test tagged to law X covers") && prompt.contains("covered_by"));
    assert!(prompt.contains("more than 3 central behaviours"));
    assert!(prompt.contains("=== SHAPE (decided by code) ==="));

    assert_eq!((stored.pr, stored.head.as_str()), (7, "abc123"));
    assert_eq!(
        stored
            .brief
            .decisions
            .iter()
            .map(|d| d.id.clone().unwrap())
            .collect::<Vec<_>>(),
        ["d1", "d2"]
    );
    assert_eq!(store.get(7, "abc123"), Some(stored.clone()));
    assert_eq!(store.latest(7).unwrap().head, "abc123");
    assert!(store.get(7, "other").is_none() && store.get(8, "abc123").is_none());

    let records = store.records();
    assert_eq!(records.len(), 1);
    let r = &records[0];
    assert_eq!(
        (r.pr, r.head.as_str(), r.shape.as_str()),
        (7, "abc123", "single story")
    );
    assert_eq!(r.budget.files_sent, 1);
    assert!(r.budget.sections.iter().any(|(n, b)| n == "DIFF" && *b > 0));
    assert!(
        r.flags
            .iter()
            .any(|f| f.contains("banned word \"ensures\"")),
        "{:?}",
        r.flags
    );
    assert_eq!(r.unconfirmed, 1);
    assert_eq!(
        (
            r.calls[0].role.as_str(),
            r.calls[0].input_tokens,
            r.calls[0].cost_usd
        ),
        ("writer", 100, 0.25)
    );
}

#[test]
fn the_entry_stores_the_link_evidence_it_was_given_beside_the_brief() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let evidence = vec![LinkEvidence {
        link: "alpha-gamma".into(),
        sites: vec![ImportSite {
            file: "alpha/a.rs".into(),
            line: Some(3),
        }],
    }];
    let given = PrSource {
        link_evidence: evidence.clone(),
        ..source(&["alpha/a.rs"])
    };
    let writer = Canned::new(vec![
        Ok(single_story().to_string()),
        Ok(none_unsupported()),
        Ok(no_rewrites()),
    ]);
    let stored = write_pr_brief(&writer, "opus", &given, &store, |_| {}).unwrap();
    assert_eq!(stored.link_evidence, evidence);
    assert_eq!(store.get(7, "abc123").unwrap().link_evidence, evidence);
}

#[test]
fn the_entry_stores_the_head_s_map_problems_it_was_given_beside_the_brief() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let problems = vec!["stray/s.rs: no part owns it".to_string()];
    let given = PrSource {
        map_problems: problems.clone(),
        ..source(&["alpha/a.rs"])
    };
    let writer = Canned::new(vec![
        Ok(single_story().to_string()),
        Ok(none_unsupported()),
        Ok(no_rewrites()),
    ]);
    let stored = write_pr_brief(&writer, "opus", &given, &store, |_| {}).unwrap();
    assert_eq!(stored.map_problems, problems);
    assert_eq!(store.get(7, "abc123").unwrap().map_problems, problems);
}

#[test]
fn the_record_logs_the_wall_time_of_the_writer_the_claim_check_and_the_retry() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let mut b = single_story();
    b["chapters"][0]["steps"][0]["b"] = json!("This ensures order.");
    // The claim check flags the step, so a retry follows: three calls.
    let flagged = unsupported("c0s0", "ensures order", "the facts say no such thing");
    let writer = Canned::new(vec![
        Ok(b.to_string()),
        Ok(checker(vec![flagged])),
        Ok(no_rewrites()),
    ])
    .slow(30);
    write_pr_brief(&writer, "opus", &source(&["alpha/a.rs"]), &store, |_| {}).unwrap();

    let record = &store.records()[0];
    let timed: Vec<(&str, bool)> = record
        .calls
        .iter()
        .map(|c| (c.role.as_str(), c.duration_ms >= 30))
        .collect();
    assert_eq!(
        timed,
        [("writer", true), ("claim check", true), ("retry", true)]
    );
}

#[test]
fn a_rewrite_keeps_the_ids_of_the_decisions_it_repeats_and_a_stale_brief_is_marked() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let mut b = single_story();
    b["decisions"] = json!([{ "text": "Alpha chose order over speed." }]);
    let first = write_pr_brief(
        &Canned::new(vec![Ok(b.to_string()), Ok(none_unsupported())]),
        "opus",
        &source(&["alpha/a.rs"]),
        &store,
        drop,
    )
    .unwrap();
    let ids = first.brief.decisions.clone();
    assert_eq!(ids[0].id.as_deref(), Some("d1"));

    let mut again = source(&["alpha/a.rs"]);
    again.meta.head = "def456".into();
    again.earlier_decisions = ids;
    let mut b = single_story();
    b["decisions"] = json!([
        { "id": "d1", "text": "Alpha picked order over speed." },
        { "text": "A new decision." },
        { "text": "Alpha chose order over speed." }
    ]);
    let writer = Canned::new(vec![Ok(b.to_string()), Ok(none_unsupported())]);
    let second = write_pr_brief(&writer, "sonnet", &again, &store, drop).unwrap();
    assert!(
        writer.prompts.borrow()[0]
            .1
            .contains("=== EARLIER DECISIONS ===\n- d1: Alpha chose order over speed.")
    );
    let got: Vec<_> = second
        .brief
        .decisions
        .iter()
        .map(|d| d.id.clone().unwrap())
        .collect();
    assert_eq!(
        got,
        ["d1", "d2", "d1"][..2]
            .iter()
            .chain(&["d1"])
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(store.latest(7).unwrap().head, "def456");
    assert_eq!(store.records().len(), 2);

    assert!(store.mark_stale(7, "def456").unwrap());
    assert!(store.get(7, "def456").unwrap().stale);
    assert!(!store.mark_stale(7, "none").unwrap());
}

#[test]
fn a_failed_call_or_an_unreadable_reply_stores_and_records_nothing() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let writer = Canned::new(vec![Err("`claude -p` failed".into()), Ok("sorry".into())]);
    for _ in 0..2 {
        assert!(write_pr_brief(&writer, "opus", &source(&["alpha/a.rs"]), &store, drop).is_err());
    }
    assert!(store.get(7, "abc123").is_none() && store.records().is_empty());
    // A map that does not parse stops before any call.
    let mut bad = source(&["alpha/a.rs"]);
    bad.map_text = "nope".into();
    let writer = Canned::new(vec![]);
    assert!(write_pr_brief(&writer, "opus", &bad, &store, drop).is_err());
    assert!(writer.prompts.borrow().is_empty());
}

#[test]
fn a_briefs_call_runs_at_the_brief_effort_and_at_claudes_own_without_one() {
    use studio_understanding::ClaudeWriter;
    let args = |w: &ClaudeWriter| -> Vec<String> {
        w.brief_command("opus", CallKind::Write, None, "p")
            .0
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    };
    let plain = ClaudeWriter::new("/bin/claude".into(), "/w/card-writer".into());
    assert!(
        !args(&plain).contains(&"--effort".to_string()),
        "{:?}",
        args(&plain)
    );
    let low = ClaudeWriter::new("/bin/claude".into(), "/w/card-writer".into())
        .with_brief_effort(|| "low".into());
    let got = args(&low);
    let at = got.iter().position(|a| a == "--effort");
    assert_eq!(at.map(|i| got[i + 1].as_str()), Some("low"), "{got:?}");
    let unset = ClaudeWriter::new("/bin/claude".into(), "/w/card-writer".into())
        .with_brief_effort(String::new);
    assert!(!args(&unset).contains(&"--effort".to_string()));
}

#[test]
fn a_briefs_call_carries_its_facts_as_appended_system_text_and_its_own_rules_as_the_prompt() {
    use studio_understanding::ClaudeWriter;
    let writer = ClaudeWriter::new("/bin/claude".into(), "/w/card-writer".into());
    let (command, stdin) =
        writer.brief_command("opus", CallKind::Write, Some("FACTS text"), "the rules");
    let args: Vec<String> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let at = args.iter().position(|a| a == "--append-system-prompt");
    assert_eq!(
        at.map(|i| args[i + 1].as_str()),
        Some("FACTS text"),
        "{args:?}"
    );
    assert!(
        !args.contains(&"--system-prompt".to_string()),
        "the default system prompt stays"
    );
    assert_eq!(stdin, "the rules");
}

// No argument can hold a NUL byte: the facts drop theirs, as standard input does, so the spawn starts (#443 review).
#[test]
fn a_briefs_facts_with_a_nul_byte_still_spawn() {
    use studio_understanding::ClaudeWriter;
    let folder = tempfile::tempdir().unwrap();
    let writer = ClaudeWriter::new("/usr/bin/true".into(), folder.path().into());
    let (mut command, _) =
        writer.brief_command("opus", CallKind::Write, Some("FACTS\0 from a diff"), "p");
    let args: Vec<String> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let at = args.iter().position(|a| a == "--append-system-prompt");
    assert_eq!(at.map(|i| args[i + 1].as_str()), Some("FACTS from a diff"));
    let spawned = command.spawn();
    assert!(spawned.is_ok(), "{:?}", spawned.err());
    spawned.unwrap().wait().unwrap();
}

#[test]
fn the_claim_checks_call_runs_at_claudes_own_effort_whatever_the_brief_effort() {
    use studio_understanding::ClaudeWriter;
    let writer = ClaudeWriter::new("/bin/claude".into(), "/w/card-writer".into())
        .with_brief_effort(|| "low".into());
    let effort = |kind| -> Option<String> {
        let args: Vec<String> = writer
            .brief_command("opus", kind, Some("FACTS"), "p")
            .0
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let at = args.iter().position(|a| a == "--effort")?;
        Some(args[at + 1].clone())
    };
    assert_eq!(effort(CallKind::Write), Some("low".into()));
    assert_eq!(effort(CallKind::Check), None);
}

// The brief effort covers the brief and catch-up writer and its retry only: pre-flight and the law draft call plain
// `write`, which runs at `claude`'s own effort (#443 review).
#[test]
fn a_plain_write_runs_at_claudes_own_effort_whatever_the_brief_effort() {
    use std::os::unix::fs::PermissionsExt;
    use studio_understanding::ClaudeWriter;
    let dir = tempfile::tempdir().unwrap();
    let args = dir.path().join("args");
    let claude = dir.path().join("claude");
    std::fs::write(
        &claude,
        format!(
            "#!/bin/sh\ncat > /dev/null\necho \"$@\" > '{}'\necho '{{\"result\":\"ok\"}}'\n",
            args.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let writer = ClaudeWriter::new(claude, dir.path().into()).with_brief_effort(|| "low".into());

    assert_eq!(writer.write("opus", "p").unwrap().text, "ok");
    let got = std::fs::read_to_string(&args).unwrap();
    assert!(got.contains("--model opus"), "{got}");
    assert!(!got.contains("--effort"), "{got}");
}

#[test]
fn a_calls_usage_keeps_the_tokens_read_from_the_cache_and_written_to_it() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored/pr-brief-v3");
    let reply =
        read_envelope(&std::fs::read_to_string(dir.join("317-reply.json")).unwrap()).unwrap();
    let usage = reply.usage;
    assert_eq!(usage.input_tokens, 2 + 29986 + 1760);
    assert_eq!(
        (usage.cache_read_tokens, usage.cache_write_tokens),
        (1760, 29986)
    );
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_reads_in_an_older_or_newer_build_which_ignores_what_it_does_not_know() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored/pr-brief-v3");
    for name in ["stored-brief-minimal.json", "stored-brief-full.json"] {
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        let data = tempfile::tempdir().unwrap();
        let store = BriefStore::open(data.path()).unwrap();
        let from: StoredBrief =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        std::fs::write(
            data.path()
                .join("pr-briefs")
                .join(format!("{}-{}.json", from.pr, from.head)),
            &text,
        )
        .unwrap();
        assert_eq!(store.get(from.pr, &from.head).unwrap(), from, "{name}");
    }
    let full: StoredBrief =
        serde_json::from_str(&std::fs::read_to_string(dir.join("stored-brief-full.json")).unwrap())
            .unwrap();
    assert_eq!((full.pr, full.version, full.stale), (317, 1, true));
    assert_eq!(
        full.brief.chapters[0].steps[0].covered_by.as_deref(),
        Some("a_test")
    );
    assert_eq!(full.brief.decisions[0].id.as_deref(), Some("d1"));
    let minimal: StoredBrief = serde_json::from_str(
        &std::fs::read_to_string(dir.join("stored-brief-minimal.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(minimal.brief, Brief::default());
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_with_gists_reads_with_them_and_a_part_catch_up_with_its_gist() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored/pr-brief-v4");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let full: StoredBrief = serde_json::from_str(&read("stored-brief-full.json")).unwrap();
    assert_eq!(
        full.brief.chapters[0].gist,
        "The shell now keeps a part fogged until its chapter is read."
    );
    assert_eq!(
        full.brief.chapters[0].gist_flags,
        [
            "claim check: \"until its chapter is read\"",
            "not confirmed from the facts"
        ]
    );
    assert_eq!(full.brief.chapters[1].gist, "", "a chapter of no Core part");
    assert_eq!(full.brief.flagged(), 2, "its step and its gist");
    let catch_up: studio_understanding::part_catch_up::StoredCatchUp =
        serde_json::from_str(&read("part-catch-up-full.json")).unwrap();
    assert_eq!(
        (
            catch_up.part.as_str(),
            catch_up.brief.chapters[0].gist.as_str()
        ),
        (
            "shell",
            "The shell keeps a part fogged until its catch-up is read."
        )
    );
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_with_a_guard_reads_with_it_and_an_older_one_has_none() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let guarded: StoredBrief =
        serde_json::from_str(&read("pr-brief-v5/stored-brief-guard.json")).unwrap();
    assert_eq!(
        guarded.guard,
        Some(Guard {
            parts: vec!["hook".into(), "shell".into()]
        })
    );
    let older: StoredBrief =
        serde_json::from_str(&read("pr-brief-v4/stored-brief-full.json")).unwrap();
    assert_eq!(older.guard, None);
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_reads_with_its_partial_facts_and_older_ones_with_nothing_missing() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let partial: StoredBrief =
        serde_json::from_str(&read("pr-brief-v7/stored-brief-partial.json")).unwrap();
    assert_eq!(
        partial.partial_facts,
        PartialFacts {
            cut: Some(DiffCut {
                files_cut: vec!["gamma/c.rs".into()],
                files_partial: vec!["alpha/a.rs".into()],
                bytes_sent: 61440,
                bytes_cut: 20480,
            }),
            claim_error: Some("`claude -p` failed".into()),
            no_extractor: vec![".go".into(), ".py".into()],
        }
    );
    for older in [
        "pr-brief-v3/stored-brief-full.json",
        "pr-brief-v3/stored-brief-minimal.json",
        "pr-brief-v4/stored-brief-full.json",
        "pr-brief-v5/stored-brief-guard.json",
    ] {
        let older: StoredBrief = serde_json::from_str(&read(older)).unwrap();
        assert!(older.partial_facts.is_empty(), "{older:?}");
    }
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_reads_with_its_also_groups_and_older_ones_with_their_also_lines_and_none() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let file = |path: &str, lines: usize| TouchedFile {
        path: path.into(),
        lines,
        ..TouchedFile::default()
    };
    let stored: StoredBrief =
        serde_json::from_str(&read("pr-brief-v8/stored-brief-also.json")).unwrap();
    assert!(stored.brief.also.is_empty());
    assert_eq!(
        stored.brief.also_groups,
        [
            AlsoGroup {
                kind: AlsoKind::NoPart,
                files: vec![file("loose/x.rs", 3)],
                lines: 3,
            },
            AlsoGroup {
                kind: AlsoKind::Tests,
                files: vec![
                    TouchedFile {
                        part: Some("beta".into()),
                        laws: vec!["seen-gate".into()],
                        ..file("beta/tests/order.rs", 4)
                    },
                    TouchedFile {
                        ignored: true,
                        ..file("scripts/run.spec.ts", 2)
                    },
                ],
                lines: 6,
            },
            AlsoGroup {
                kind: AlsoKind::Ignored,
                files: vec![TouchedFile {
                    ignored: true,
                    ..file("scripts/tool.sh", 1)
                }],
                lines: 1,
            },
            AlsoGroup {
                kind: AlsoKind::Other,
                files: vec![TouchedFile {
                    part: Some("beta".into()),
                    ..file("beta/b.rs", 5)
                }],
                lines: 5,
            },
        ]
    );
    // Written before #489: the model's also line still reads, with no groups.
    for older in [
        "pr-brief-v3/stored-brief-full.json",
        "pr-brief-v4/stored-brief-full.json",
    ] {
        let older: StoredBrief = serde_json::from_str(&read(older)).unwrap();
        assert_eq!(older.brief.also[0].line, "a line", "{older:?}");
        assert!(older.brief.also_groups.is_empty());
    }
    for older in [
        "pr-brief-v5/stored-brief-guard.json",
        "pr-brief-v7/stored-brief-partial.json",
    ] {
        let older: StoredBrief = serde_json::from_str(&read(older)).unwrap();
        assert!(older.brief.also_groups.is_empty(), "{older:?}");
    }
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_reads_with_its_link_evidence_and_older_ones_with_none() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let site = |file: &str, line: Option<u32>| ImportSite {
        file: file.into(),
        line,
    };
    let stored: StoredBrief =
        serde_json::from_str(&read("pr-brief-v9/stored-brief-evidence.json")).unwrap();
    assert_eq!(
        stored.link_evidence,
        [
            LinkEvidence {
                link: "alpha-gamma".into(),
                sites: vec![
                    site("alpha/a.rs", Some(3)),
                    site("alpha/a.rs", Some(9)),
                    site("alpha/b.rs", Some(1)),
                ],
            },
            LinkEvidence {
                link: "beta-gamma".into(),
                sites: vec![site("beta/Cargo.toml", None)],
            },
        ]
    );
    for older in [
        "pr-brief-v3/stored-brief-full.json",
        "pr-brief-v3/stored-brief-minimal.json",
        "pr-brief-v4/stored-brief-full.json",
        "pr-brief-v5/stored-brief-guard.json",
        "pr-brief-v7/stored-brief-partial.json",
        "pr-brief-v8/stored-brief-also.json",
    ] {
        let older: StoredBrief = serde_json::from_str(&read(older)).unwrap();
        assert!(older.link_evidence.is_empty(), "{older:?}");
    }
}

// @law: old-stored-files-load
#[test]
fn a_stored_brief_reads_with_its_head_s_map_problems_and_older_ones_with_none() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stored");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let stored: StoredBrief =
        serde_json::from_str(&read("pr-brief-v10/stored-brief-map-problems.json")).unwrap();
    assert_eq!(
        stored.map_problems,
        [
            "stray/s.rs is tracked but no part owns it; add it to a part's paths or to ignored",
            "import link \"alpha-beta\" runs from \"alpha\" to \"beta\", but no import runs that way"
        ]
    );
    for older in [
        "pr-brief-v3/stored-brief-full.json",
        "pr-brief-v3/stored-brief-minimal.json",
        "pr-brief-v4/stored-brief-full.json",
        "pr-brief-v5/stored-brief-guard.json",
        "pr-brief-v7/stored-brief-partial.json",
        "pr-brief-v8/stored-brief-also.json",
        "pr-brief-v9/stored-brief-evidence.json",
    ] {
        let older: StoredBrief = serde_json::from_str(&read(older)).unwrap();
        assert!(older.map_problems.is_empty(), "{older:?}");
    }
}

// @law: old-stored-files-load
#[test]
fn stored_records_read_with_their_calls_cache_tokens_and_older_ones_with_none() {
    let sample = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/stored/pr-brief-v6/pr-brief-records.jsonl");
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    std::fs::copy(sample, data.path().join("pr-briefs/pr-brief-records.jsonl")).unwrap();

    let records = store.records();
    let prs: Vec<u64> = records.iter().map(|r| r.pr).collect();
    assert_eq!(prs, vec![402, 455]);
    // Written before #450: its calls carry no cache tokens and read with none.
    let older: Vec<(u64, u64)> = records[0]
        .calls
        .iter()
        .map(|c| (c.cache_read_tokens, c.cache_write_tokens))
        .collect();
    assert_eq!(older, vec![(0, 0), (0, 0)]);
    let newer: Vec<(&str, u64, u64)> = records[1]
        .calls
        .iter()
        .map(|c| (c.role.as_str(), c.cache_read_tokens, c.cache_write_tokens))
        .collect();
    assert_eq!(
        newer,
        vec![
            ("write", 0, 29986),
            ("check", 29986, 1760),
            ("retry", 29986, 0)
        ]
    );
}

// ---- gather: git only ----

#[test]
fn gather_reads_the_map_the_diff_and_the_tagged_tests_from_git() {
    let (dir, _) = studio_test_support::repo();
    let root = dir.path();
    let write = |path: &str, text: &str| {
        let p = root.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    write("system-map.json", MAP);
    write("alpha/a.rs", "fn a() {}\n");
    write(
        "alpha/tests/t.rs",
        "// @law: alpha-keeps-order\n#[test]\nfn order_is_kept() {}\n// @law: not-a-law\nfn x() {}\n",
    );
    studio_test_support::git(root, &["add", "."]);
    studio_test_support::git(root, &["commit", "-qm", "base"]);
    let base = studio_test_support::git(root, &["rev-parse", "HEAD"]);
    write("alpha/a.rs", "fn a() {}\nfn b() {}\n");
    write(
        "system-map.json",
        &MAP.replace("Alpha\", \"caption", "Alpha two\", \"caption"),
    );
    studio_test_support::git(root, &["commit", "-qam", "change"]);
    let head = studio_test_support::git(root, &["rev-parse", "HEAD"]);

    let meta = PrMeta {
        number: 9,
        head: head.clone(),
        ..PrMeta::default()
    };
    let s = gather(root, &base, &head, meta).unwrap();
    assert!(
        s.map_text.contains("Alpha two")
            && s.map_diff.contains("+")
            && s.map_diff.contains("Alpha two")
    );
    assert!(s.diff.contains("+fn b() {}"));
    assert_eq!(s.tagged.len(), 1);
    assert_eq!(
        (
            s.tagged[0].law.as_str(),
            s.tagged[0].path.as_str(),
            s.tagged[0].test.as_str()
        ),
        ("alpha-keeps-order", "alpha/tests/t.rs", "order_is_kept")
    );
    let facts = build_facts(&s).unwrap();
    assert_eq!(facts.shape.required, ["alpha"]);
    assert!(section(&facts, "MAP DIFF IN THIS PR").contains("Alpha two"));
}

// ---- the claim check and the one retry (#338) ----

const NOT_CONFIRMED: &str = "not confirmed from the facts";

fn run_entry(
    replies: Vec<Result<String, String>>,
    brief: &serde_json::Value,
    src: &PrSource,
) -> (Canned, StoredBrief, BriefRecord, Vec<String>) {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let mut all = vec![Ok(brief.to_string())];
    all.extend(replies);
    let writer = Canned::new(all);
    let mut log = Vec::new();
    let stored = write_pr_brief(&writer, "opus", src, &store, |l| log.push(l)).unwrap();
    assert_eq!(store.get(7, "abc123"), Some(stored.clone()));
    let records = store.records();
    assert_eq!(records.len(), 1, "one line per brief");
    (writer, stored, records[0].clone(), log)
}

fn steps_json(b: &Brief) -> Vec<String> {
    b.steps()
        .map(|s| serde_json::to_string(s).unwrap())
        .collect()
}

#[test]
fn a_flagged_step_is_rewritten_unflagged_steps_are_kept_and_a_step_still_flagged_is_unconfirmed() {
    let mut b = single_story();
    b["chapters"][0]["steps"][1]["b"] = json!("This ensures order.");
    let first: Brief = serde_json::from_value(b.clone()).unwrap();
    let mut fixed = step("Before", &["alpha"]);
    fixed["b"] = json!("Alpha now keeps what it was given in order.");
    // The retry also sends a rewrite of a step nobody flagged: it is ignored.
    let mut sneaky = step("Where", &["alpha"]);
    sneaky["b"] = json!("Changed without being asked.");
    let (writer, stored, record, log) = run_entry(
        vec![
            Ok(checker(vec![
                unsupported(
                    "step 3",
                    "Alpha now keeps order",
                    "the PR never says it is kept",
                ),
                json!({ "id": "step 1", "supported": true }),
            ])),
            Ok(json!({ "items": [
                { "id": "step 2", "step": fixed },
                { "id": "step 1", "step": sneaky }
            ]})
            .to_string()),
        ],
        &b,
        &source(&["alpha/a.rs"]),
    );
    assert!(log.is_empty(), "{log:?}");
    let steps = steps_json(&stored.brief);
    let before = steps_json(&first);
    assert_eq!(steps[0], before[0], "an unflagged step is byte-identical");
    assert!(
        steps[1].contains("keeps what it was given")
            && stored.brief.steps().nth(1).unwrap().flags.is_empty()
    );
    // The third step was flagged by the claim check and the retry gave nothing for it.
    let third = stored.brief.steps().nth(2).unwrap();
    assert_eq!(
        steps_json(&Brief {
            chapters: vec![Chapter {
                steps: vec![Step {
                    flags: vec![],
                    ..third.clone()
                }],
                ..Chapter::default()
            }],
            ..Brief::default()
        })[0],
        before[2]
    );
    assert!(
        third.flags.iter().any(|f| f == NOT_CONFIRMED),
        "{:?}",
        third.flags
    );
    assert!(
        third
            .flags
            .iter()
            .any(|f| f.contains("the PR never says it is kept")),
        "{:?}",
        third.flags
    );
    // The retry request carries both kinds of flag and the claim check saw the facts.
    let prompts = writer.prompts.borrow();
    assert_eq!(prompts.len(), 3);
    assert!(prompts[1].1.contains("=== SHAPE (decided by code) ==="));
    assert!(prompts[2].1.contains("banned word"));
    assert!(prompts[2].1.contains("the PR never says it is kept"));
    // The three calls get the same facts block, byte for byte (#450); a writer with no system text of its own gets it first.
    let block = facts_block(&build_facts(&source(&["alpha/a.rs"])).unwrap());
    for (i, (_, p)) in prompts.iter().enumerate() {
        assert!(p.starts_with(&block), "call {i} starts: {}", &p[..80]);
    }
    assert!(
        prompts.iter().all(|(m, _)| m == "opus"),
        "the PR briefs model, all three calls"
    );
    // The record: before and after, the unconfirmed count and the cost of each call.
    assert!(
        record
            .flags
            .iter()
            .any(|f| f.contains("banned word \"ensures\""))
    );
    assert_eq!(record.claim_flags_before.len(), 1);
    assert!(record.claim_flags_before[0].contains("step 3"));
    assert_eq!(record.claim_flags_after, record.claim_flags_before);
    assert_eq!(record.unconfirmed, 1);
    assert_eq!(
        record
            .calls
            .iter()
            .map(|c| (c.role.as_str(), c.cost_usd))
            .collect::<Vec<_>>(),
        [("writer", 0.25), ("claim check", 0.25), ("retry", 0.25)]
    );
    assert!(record.flags_after.iter().any(|f| f.contains(NOT_CONFIRMED)));
}

#[test]
fn a_retry_that_moves_a_step_to_the_top_leaves_the_claim_flags_on_the_steps_they_were_found_on() {
    let mut src = source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"]);
    src.meta.created_at = "2026-11-20T18:00:00Z".into();
    src.meta.spec = Some(Spec {
        number: 5,
        title: "A spec".into(),
        created_at: "2026-11-20T09:00:00Z".into(),
        body: "## User Stories\nstory\n".into(),
    });
    // Chapter 2 is Before, Now, Could break (global steps 5, 6, 7 after the overview step): the claim check flags 5 and 7.
    let mut b = good_brief();
    b["chapters"][1]["steps"][2] = step("Could break", &["gamma"]);
    let mut moved = step("Not in the plan", &["gamma"]);
    moved["b"] = json!("Gamma was never in the plan.");
    let (_, stored, _, _) = run_entry(
        vec![
            Ok(checker(vec![
                unsupported("step 5", "Before", "not in the facts"),
                unsupported("step 7", "Could break", "not in the facts"),
            ])),
            Ok(json!({ "items": [{ "id": "step 7", "step": moved }] }).to_string()),
        ],
        &b,
        &src,
    );
    let chapter = &stored.brief.chapters[1].steps;
    let kinds: Vec<&str> = chapter.iter().map(|s| s.k.as_str()).collect();
    assert_eq!(kinds, ["Not in the plan", "Before", "Now"]);
    assert!(chapter[0].flags.is_empty(), "{:?}", chapter[0].flags);
    assert!(
        chapter[1]
            .flags
            .iter()
            .any(|f| f.contains("not in the facts")),
        "{:?}",
        chapter[1].flags
    );
}

#[test]
fn a_claim_the_retry_rewrites_leaves_no_claim_flag_after() {
    let b = single_story();
    let mut fixed = step("Now", &["alpha"]);
    fixed["b"] = json!("The PR body says Alpha now keeps order.");
    let (_, stored, record, _) = run_entry(
        vec![
            Ok(checker(vec![unsupported(
                "step 3",
                "Alpha now keeps order",
                "no source",
            )])),
            Ok(json!({ "items": [{ "id": "step 3", "step": fixed }] }).to_string()),
        ],
        &b,
        &source(&["alpha/a.rs"]),
    );
    assert_eq!(stored.brief.flagged(), 0, "{:?}", stored.brief.all_flags());
    assert_eq!(record.claim_flags_before.len(), 1);
    assert!(record.claim_flags_after.is_empty());
    assert_eq!(record.unconfirmed, 0);
}

#[test]
fn the_check_block_and_the_risks_are_part_of_what_the_claim_check_reads_and_what_it_flags() {
    let src = flow_source();
    let mut b = good_brief();
    b["overview"][0]["k"] = json!("Your turn");
    b["overview"][0]["check"] = json!({ "kind": "order", "q": "Which part answers first?", "key": ["gamma", "beta", "alpha"], "yes": "Yes, Gamma answers", "no": "No, Alpha answers", "flow": "the-flow", "opts": ["Alpha first", "Gamma first"] });
    b["chapters"][0]["steps"][2]["risks"] =
        json!([{ "part": "beta", "trigger": "Beta gets an empty list", "check": "Send one" }]);
    let (writer, stored, record, _) = run_entry(
        vec![
            Ok(checker(vec![
                unsupported(
                    "step 1",
                    "Yes, Gamma answers",
                    "the facts do not say Gamma answers",
                ),
                unsupported(
                    "step 4",
                    "Beta gets an empty list",
                    "no such case in the facts",
                ),
            ])),
            Ok(no_rewrites()),
        ],
        &b,
        &src,
    );
    let check_prompt = &writer.prompts.borrow()[1].1;
    for text in [
        "Which part answers first?",
        "Alpha first",
        "Yes, Gamma answers",
        "No, Alpha answers",
        "Beta gets an empty list",
        "Send one",
    ] {
        assert!(
            check_prompt.contains(text),
            "the claim check does not read {text:?}"
        );
    }
    let flagged: Vec<usize> = stored
        .brief
        .steps()
        .enumerate()
        .filter(|(_, s)| s.flags.iter().any(|f| f.contains("claim check")))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(flagged, [0, 3]);
    assert_eq!(record.claim_flags_before.len(), 2);
}

#[test]
fn a_decision_can_be_flagged_and_rewritten_and_a_models_also_line_is_dropped_unchecked() {
    let mut b = single_story();
    b["decisions"] = json!([{ "text": "Alpha chose order over speed." }]);
    // The writer is no longer asked for "also" lines (#477); one it sends anyway is dropped, and code sorts Also touched.
    b["also"] = json!([{ "part": "beta", "line": "Beta changed a little." }]);
    let src = source(&["alpha/a.rs", "beta/b.rs"]);
    let (writer, stored, record, _) = run_entry(
        vec![
            Ok(checker(vec![unsupported(
                "decision 1",
                "chose order over speed",
                "the PR names no such choice",
            )])),
            Ok(json!({ "items": [
                { "id": "decision 1", "text": "Alpha chose to keep order." }
            ]})
            .to_string()),
        ],
        &b,
        &src,
    );
    assert_eq!(stored.brief.decisions[0].text, "Alpha chose to keep order.");
    assert_eq!(
        stored.brief.decisions[0].id.as_deref(),
        Some("d1"),
        "the id stays"
    );
    assert!(stored.brief.decisions[0].flags.is_empty());
    assert!(stored.brief.also.is_empty());
    assert_eq!(stored.brief.also_groups.len(), 1);
    assert_eq!(stored.brief.also_groups[0].files[0].path, "beta/b.rs");
    let prompts = writer.prompts.borrow();
    assert!(
        prompts
            .iter()
            .all(|(_, p)| !p.contains("also 1") && !p.contains("\"also\"")),
        "neither the writer nor the claim check is asked about also lines"
    );
    assert!(stored.brief.steps().all(|s| s.flags.is_empty()));
    assert_eq!(record.claim_flags_before.len(), 1);
}

#[test]
fn the_claim_check_reads_each_gist_and_the_retry_rewrites_a_flagged_or_missing_one() {
    let src = source(&["alpha/a.rs"]);
    // The claim check finds the gist unsupported; the retry rewrites it.
    let (writer, stored, record, _) = run_entry(
        vec![
            Ok(checker(vec![unsupported(
                "gist 1",
                "keeps order",
                "the PR names no order",
            )])),
            Ok(json!({ "items": [{ "id": "gist 1", "gist": "Alpha now keeps the order it was given." }] }).to_string()),
        ],
        &single_story(),
        &src,
    );
    let prompts = writer.prompts.borrow();
    assert!(
        prompts[1].1.contains("\"id\": \"gist 1\"")
            && prompts[1].1.contains("Alpha now keeps order."),
        "the claim check sees the gist"
    );
    assert!(
        prompts[2].1.contains("{ \"id\": \"gist 1\", \"gist\":"),
        "the retry says how to rewrite one"
    );
    assert_eq!(
        stored.brief.chapters[0].gist,
        "Alpha now keeps the order it was given."
    );
    assert!(stored.brief.chapters[0].gist_flags.is_empty());
    assert_eq!(record.claim_flags_before.len(), 1);
    assert!(record.claim_flags_after.is_empty());

    // A Core chapter written without a gist gets one from the retry; a retry that gives none leaves it unconfirmed.
    let mut b = single_story();
    b["chapters"][0]["gist"] = json!("");
    let (_, stored, _, _) = run_entry(
        vec![
            Ok(none_unsupported()),
            Ok(json!({ "items": [{ "id": "gist 1", "gist": "Alpha keeps order." }] }).to_string()),
        ],
        &b,
        &src,
    );
    assert_eq!(stored.brief.chapters[0].gist, "Alpha keeps order.");
    assert!(
        stored.brief.all_flags().is_empty(),
        "{:?}",
        stored.brief.all_flags()
    );
    let (_, stored, _, _) = run_entry(vec![Ok(none_unsupported()), Ok(no_rewrites())], &b, &src);
    assert_eq!(
        stored.brief.chapters[0].gist_flags,
        ["a Core chapter without a gist", NOT_CONFIRMED]
    );
}

#[test]
fn a_malformed_checker_reply_leaves_the_brief_unmarked_and_is_recorded() {
    for reply in [
        Ok("I could not decide.".to_string()),
        Ok("{\"items\": 3}".into()),
        Ok("{\"items\": [{\"id\": \"step 1\"}]}".into()),
        Err("`claude -p` failed".to_string()),
    ] {
        let b = single_story();
        let (writer, stored, record, log) =
            run_entry(vec![reply.clone()], &b, &source(&["alpha/a.rs"]));
        assert_eq!(stored.brief.flagged(), 0, "{reply:?}");
        assert!(record.claim_flags_before.is_empty() && record.claim_flags_after.is_empty());
        assert!(record.claim_error.is_some(), "{reply:?}");
        assert!(log.iter().any(|l| l.contains("claim check")), "{log:?}");
        assert_eq!(
            writer.prompts.borrow().len(),
            2,
            "no retry with nothing flagged"
        );
        assert_eq!(record.unconfirmed, 0);
    }
}

#[test]
fn a_brief_with_nothing_flagged_makes_no_retry_call() {
    let (writer, stored, record, _) = run_entry(
        vec![Ok(none_unsupported())],
        &single_story(),
        &source(&["alpha/a.rs"]),
    );
    assert_eq!(writer.prompts.borrow().len(), 2);
    assert_eq!(stored.brief.flagged(), 0);
    assert_eq!(
        record
            .calls
            .iter()
            .map(|c| c.role.as_str())
            .collect::<Vec<_>>(),
        ["writer", "claim check"]
    );
    assert!(record.retry_error.is_none() && record.claim_error.is_none());
}

#[test]
fn a_failed_or_unreadable_retry_keeps_the_brief_with_its_flagged_items_unconfirmed() {
    for retry in [
        Err("`claude -p` failed".to_string()),
        Ok("no json".to_string()),
    ] {
        let mut b = single_story();
        b["chapters"][0]["steps"][1]["b"] = json!("This ensures order.");
        let (_, stored, record, log) = run_entry(
            vec![Ok(none_unsupported()), retry],
            &b,
            &source(&["alpha/a.rs"]),
        );
        let second = stored.brief.steps().nth(1).unwrap();
        assert!(second.flags.iter().any(|f| f == NOT_CONFIRMED));
        assert_eq!(record.unconfirmed, 1);
        assert!(record.retry_error.is_some());
        assert!(log.iter().any(|l| l.contains("retry")), "{log:?}");
    }
}

#[test]
fn a_your_turn_step_whose_order_check_was_dropped_is_retried_and_may_get_a_right_check() {
    let src = flow_source();
    let wrong = |k: &str| {
        let mut b = good_brief();
        b["overview"][0]["k"] = json!(k);
        b["overview"][0]["check"] = json!({ "kind": "order", "q": "Which way?", "key": ["gamma", "beta"], "yes": "y", "no": "n", "flow": "the-flow" });
        b
    };
    let b = wrong("Your turn");
    // Fixed: the retry gives the step a check whose parts are the flow's.
    let mut fixed = step("Your turn", &["alpha", "beta"]);
    fixed["check"] = json!({ "kind": "order", "q": "Which way?", "key": ["beta", "alpha", "gamma"], "yes": "y", "no": "n", "flow": "the-flow" });
    let (writer, stored, record, _) = run_entry(
        vec![
            Ok(none_unsupported()),
            Ok(json!({ "items": [{ "id": "step 1", "step": fixed }] }).to_string()),
        ],
        &b,
        &src,
    );
    assert!(
        record
            .flags
            .iter()
            .any(|f| f.contains("order check dropped"))
    );
    assert!(writer.prompts.borrow()[2].1.contains("order check dropped"));
    assert_eq!(stored.brief.flagged(), 0, "{:?}", stored.brief.all_flags());
    assert_eq!(
        stored.brief.overview[0].check.as_ref().unwrap().key,
        json!(["alpha", "beta", "gamma"])
    );
    // Still wrong: the step stays "Your turn" with no check, flagged, and carries the mark.
    let (_, stored, record, _) =
        run_entry(vec![Ok(none_unsupported()), Ok(no_rewrites())], &b, &src);
    let s = &stored.brief.overview[0];
    assert!(
        s.check.is_none() && s.flags.iter().any(|f| f == NOT_CONFIRMED),
        "{:?}",
        s.flags
    );
    assert_eq!(record.unconfirmed, 1);
}

#[test]
fn a_not_in_the_plan_step_comes_first_in_its_chapter_when_the_spec_was_written_the_day_of_the_pr() {
    let mut same = source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"]);
    same.meta.created_at = "2026-11-20T18:00:00Z".into();
    same.meta.spec = Some(Spec {
        number: 5,
        title: "A spec".into(),
        created_at: "2026-11-20T09:00:00Z".into(),
        body: "## User Stories\nstory\n".into(),
    });
    let facts = build_facts(&same).unwrap();
    assert!(facts.context.spec_same_day);
    assert!(section(&facts, "SPEC").contains("the same day as the PR"));
    let brief = checked(&facts, good_brief());
    let kinds: Vec<&str> = brief.chapters[1]
        .steps
        .iter()
        .map(|s| s.k.as_str())
        .collect();
    assert_eq!(kinds, ["Not in the plan", "Before", "Now"]);
    // The chapter that holds none keeps its order.
    let kinds: Vec<&str> = brief.chapters[0]
        .steps
        .iter()
        .map(|s| s.k.as_str())
        .collect();
    assert_eq!(kinds, ["Before", "Now", "Could break"]);
    // Another day: the writer's order stands.
    let mut later = same.clone();
    later.meta.spec.as_mut().unwrap().created_at = "2026-11-10T09:00:00Z".into();
    let facts = build_facts(&later).unwrap();
    assert!(!facts.context.spec_same_day);
    let brief = checked(&facts, good_brief());
    let kinds: Vec<&str> = brief.chapters[1]
        .steps
        .iter()
        .map(|s| s.k.as_str())
        .collect();
    assert_eq!(kinds, ["Before", "Now", "Not in the plan"]);
}

#[test]
fn the_step_caps_sit_exactly_where_the_prompt_says() {
    let facts = build_facts(&source(&["alpha/a.rs", "beta/b.rs", "gamma/c.rs"])).unwrap();
    let with_steps = |counts: &[usize]| {
        let mut b = good_brief_without_overview();
        let chapters: Vec<serde_json::Value> = counts
            .iter()
            .map(|n| {
                let steps: Vec<serde_json::Value> = (0..*n)
                    .map(|i| step(["Before", "Now", "Could break", "Must stay true", "Your turn"][i], &["alpha"]))
                    .collect();
                json!({ "behaviour": "order", "parts": ["alpha"], "title": "Order", "steps": steps })
            })
            .collect();
        b["chapters"] = json!(chapters);
        flags_of(&checked(&facts, b)).join("\n")
    };
    // A chapter is 3 or 4 steps.
    assert!(
        !with_steps(&[3, 4]).contains(" steps"),
        "{}",
        with_steps(&[3, 4])
    );
    assert!(with_steps(&[3, 5]).contains("5 steps"));
    assert!(with_steps(&[2, 3]).contains("2 steps"));
    // Twelve steps in all is the most.
    assert!(!with_steps(&[3, 3, 3, 3]).contains("steps in all"));
    assert!(with_steps(&[4, 3, 3, 3]).contains("13 steps in all"));
    // Two to four chapters.
    assert!(with_steps(&[3]).contains("1 chapters"));
}

// ---- Since you read (#497, spec #477 part B) ----

#[test]
fn a_since_story_is_written_from_its_own_facts_and_stored_apart_from_the_full_briefs() {
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let full = Canned::new(vec![Ok(single_story().to_string()), Ok(none_unsupported())]);
    write_pr_brief(&full, "opus", &source(&["alpha/a.rs"]), &store, |_| {}).unwrap();
    let since = PrSource {
        meta: PrMeta {
            head: "def456".into(),
            ..source(&[]).meta
        },
        since: Some("abc123".into()),
        ..source(&["alpha/a.rs"])
    };
    let facts = build_facts(&since).unwrap();
    assert!(
        section(&facts, "SINCE YOU READ").contains("abc123"),
        "the writer is told the story covers only the push since abc123"
    );
    let writer = Canned::new(vec![Ok(single_story().to_string()), Ok(none_unsupported())]);
    let story = write_pr_brief(&writer, "opus", &since, &store, |_| {}).unwrap();
    assert_eq!(story.head, "def456");
    assert_eq!(
        store.since(7, "def456"),
        Some(("abc123".to_string(), story))
    );
    assert!(store.since(7, "abc123").is_none() && store.since(8, "def456").is_none());
    // The full brief is still the latest, and none is stored at the story's head.
    assert_eq!(store.latest(7).unwrap().head, "abc123");
    assert!(store.get(7, "def456").is_none());
    assert_eq!(
        store.records().len(),
        2,
        "the story's write is recorded too"
    );
}
