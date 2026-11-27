use super::*;
use studio_test_support::{git, repo};

fn commit(dir: &Path, map: &str, subject: &str) -> String {
    std::fs::write(dir.join("system-map.json"), map).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "--allow-empty", "-m", subject]);
    git(dir, &["rev-parse", "HEAD"])
}

const PR500: [&str; 2] = [
    include_str!("../../tests/map-diff/pr500-before.json"),
    include_str!("../../tests/map-diff/pr500-after.json"),
];

/// A repository where PR `number` landed by a squash commit from `before` to `after`.
fn landed(before: &str, after: &str, number: u64) -> (tempfile::TempDir, String) {
    let (dir, main) = repo();
    commit(dir.path(), before, "Map (#1)");
    commit(dir.path(), after, &format!("The change (#{number})"));
    (dir, main)
}

fn tagged(tags: &BTreeMap<String, Tag>, tag: Tag) -> Vec<&str> {
    tags.iter()
        .filter(|(_, t)| **t == tag)
        .map(|(id, _)| id.as_str())
        .collect()
}

fn lines(diff: &MapDiff, tag: Tag) -> Vec<String> {
    let mut found: Vec<String> = diff
        .lines
        .iter()
        .filter(|l| l.change == tag)
        .map(|l| l.ends.join("|"))
        .collect();
    found.sort();
    found
}

#[test]
fn pr_500s_split_shows_its_two_new_parts_its_rewritten_parts_as_edited_and_its_new_and_gone_lines()
{
    let (_dir, main) = landed(PR500[0], PR500[1], 500);
    let diff = of(Path::new(&main), 500, None).expect("PR 500 changed the map");
    // The prototype's tags (local-evidence/2026-11-26-step4-pr-map-grilling/prototype, build_data.py).
    assert_eq!(
        tagged(&diff.parts, Tag::Added),
        ["install", "map-briefs-runs"]
    );
    assert_eq!(
        tagged(&diff.parts, Tag::Edited),
        [
            "core",
            "hook",
            "plugins",
            "server",
            "understanding",
            "window",
            "workbench"
        ]
    );
    assert!(tagged(&diff.parts, Tag::Removed).is_empty());
    assert!(diff.actors.is_empty());
    assert_eq!(
        lines(&diff, Tag::Removed),
        ["agents|server", "plugins|server"]
    );
    assert_eq!(
        lines(&diff, Tag::Added),
        [
            "agents|install",
            "agents|map-briefs-runs",
            "core|map-briefs-runs",
            "core|understanding",
            "core|workbench",
            "hook|install",
            "hook|map-briefs-runs",
            "hook|understanding",
            "install|plugins",
            "install|server",
            "install|workbench",
            "map-briefs-runs|server",
            "map-briefs-runs|understanding",
            "map-briefs-runs|workbench",
        ]
    );
    // Two laws moved `on`: changed, so untagged; the flow is not drawn.
    assert!(diff.laws.is_empty());
}

const PR351: [&str; 2] = [
    include_str!("../../tests/map-diff/pr351-before.json"),
    include_str!("../../tests/map-diff/pr351-after.json"),
];

fn drawn(diff: &MapDiff, key: &str) -> Vec<String> {
    let map: Value = serde_json::from_str(&diff.source).unwrap();
    by_id(&map, key).keys().map(ToString::to_string).collect()
}

#[test]
fn pr_351s_removal_keeps_the_gone_parts_their_lines_and_laws_on_todays_map_as_ghosts() {
    let (_dir, main) = landed(PR351[0], PR351[1], 351);
    let diff = of(Path::new(&main), 351, None).expect("PR 351 changed the map");
    assert_eq!(
        tagged(&diff.parts, Tag::Removed),
        ["shell", "terminal-daemon"]
    );
    assert!(tagged(&diff.parts, Tag::Added).is_empty());
    assert!(tagged(&diff.parts, Tag::Edited).is_empty());
    assert_eq!(
        lines(&diff, Tag::Removed),
        [
            "core|shell",
            "hook|shell",
            "server|shell",
            "shell|terminal-daemon",
            "shell|understanding",
            "shell|window",
            "shell|workbench",
        ]
    );
    assert!(lines(&diff, Tag::Added).is_empty());
    assert_eq!(
        tagged(&diff.laws, Tag::Removed),
        ["no-notices-lock-on-main-thread", "terminals-outlive-app"]
    );
    // Today's map (the squash commit's) lacks them; the document draws them from the base.
    let parts = drawn(&diff, "parts");
    assert!(parts.contains(&"shell".into()) && parts.contains(&"terminal-daemon".into()));
    let map: Value = serde_json::from_str(&diff.source).unwrap();
    assert!(lines_of(&map).contains_key(&["shell".to_string(), "terminal-daemon".to_string()]));
    assert!(drawn(&diff, "laws").contains(&"terminals-outlive-app".into()));
}

const PR558: [&str; 2] = [
    include_str!("../../tests/map-diff/pr558-before.json"),
    include_str!("../../tests/map-diff/pr558-after.json"),
];

#[test]
fn pr_558s_held_item_edits_tag_nothing_and_only_its_own_purpose_change_is_edited() {
    let (_dir, main) = landed(PR558[0], PR558[1], 558);
    let diff = of(Path::new(&main), 558, None).expect("PR 558 changed the map");
    // Held items changed on six more parts, and `map-briefs-runs`'s paths: no tag on them.
    assert_eq!(
        diff.parts,
        BTreeMap::from([("workbench".into(), Tag::Edited)])
    );
    assert_eq!(
        lines(&diff, Tag::Added),
        ["hook|workbench", "understanding|workbench"]
    );
    assert!(lines(&diff, Tag::Removed).is_empty());
    // `wire-types-agree`'s text changed: untagged.
    assert_eq!(
        diff.laws,
        BTreeMap::from([("git-and-gh-go-through-the-runner".into(), Tag::Added)])
    );
}

/// A version 2 map of parts `a`, `b` and `c` (`a`'s held item saying `held`), with `links` and `laws` as JSON.
fn small(held: &str, links: &str, laws: &str) -> String {
    let part = |id: &str, holds: &str| {
        format!(
            r#"{{"id":"{id}","name":"{id}","caption":"c","purpose":"p","tier":"core","paths":["{id}/"],"holds":{holds}}}"#
        )
    };
    format!(
        r#"{{"version":2,"parts":[{},{},{}],"actors":[],"links":{links},"ignored":[],"laws":{laws},"flows":[]}}"#,
        part("a", &format!(r#"[{{"name":"H","text":"{held}"}}]"#)),
        part("b", "[]"),
        part("c", "[]"),
    )
}

#[test]
fn a_held_items_edit_alone_tags_nothing_but_still_gives_a_document() {
    let (_dir, main) = landed(&small("Old.", "[]", "[]"), &small("New.", "[]", "[]"), 7);
    let diff = of(Path::new(&main), 7, None).expect("a held item changed");
    assert!(diff.parts.is_empty() && diff.lines.is_empty() && diff.laws.is_empty());
}

#[test]
fn a_flipped_link_is_one_line_on_both_sides_so_it_is_untagged() {
    let before = small(
        "h",
        r#"[{"id":"a-b","from":"a","to":"b","kind":"import"}]"#,
        "[]",
    );
    let after = small(
        "h",
        r#"[{"id":"b-a","from":"b","to":"a","kind":"import"}]"#,
        "[]",
    );
    let (_dir, main) = landed(&before, &after, 7);
    let diff = of(Path::new(&main), 7, None).expect("the link changed");
    assert!(diff.lines.is_empty(), "{:?}", diff.lines);
}

#[test]
fn an_open_prs_counts_say_what_its_map_diff_marks_per_kind_and_what_it_marks_nothing_for() {
    let (dir, main) = repo();
    let root = dir.path();
    let before = small(
        "Old.",
        r#"[{"id":"a-b","from":"a","to":"b","reason":"A asks b."}]"#,
        "[]",
    );
    let base = commit(root, &before, "Map (#1)");
    git(root, &["checkout", "-q", "-b", "work"]);
    let mut after: Value = serde_json::from_str(&small(
        "New.",
        r#"[{"id":"a-b","from":"a","to":"b","reason":"A tells b."},{"id":"a-d","from":"a","to":"d"}]"#,
        r#"[{"id":"l1","on":"a","backing":"trust","text":"First."}]"#,
    ))
    .unwrap();
    let parts = after["parts"].as_array_mut().unwrap();
    // Part c goes, d comes, and b's own purpose changes; a's held item and a-b's reason change too, which draw no mark.
    parts.retain(|p| p["id"] != "c");
    parts[1]["purpose"] = "B, now plainer.".into();
    parts.push(serde_json::json!(
        {"id":"d","name":"D","caption":"c","purpose":"p","tier":"core","paths":["d/"],"holds":[]}
    ));
    let head = commit(root, &after.to_string(), "work");
    let found = counts(Path::new(&main), &base, &head).expect("the PR changed the map");
    assert_eq!(
        found.sentence(),
        "1 part added, 1 removed, 1 edited, 1 link added, 1 law added, and 2 changes with no mark on the map."
    );
    // What the short brief's step lights: each marked part, a marked line's ends and a marked law's part.
    assert_eq!(found.lit, ["a", "b", "c", "d"]);
    // A reformatted map counts nothing.
    let pretty =
        serde_json::to_string_pretty(&serde_json::from_str::<Value>(&before).unwrap()).unwrap();
    git(root, &["checkout", "-q", "main"]);
    let reformatted = commit(root, &pretty, "Pretty (#2)");
    assert_eq!(counts(Path::new(&main), &base, &reformatted), None);
}

#[test]
fn links_are_counted_per_line_drawn_so_two_links_on_one_new_pair_are_one_link_added() {
    let (dir, main) = repo();
    let root = dir.path();
    let base = commit(
        root,
        &small(
            "h",
            r#"[{"id":"a-b","from":"a","to":"b"},{"id":"b-a","from":"b","to":"a"}]"#,
            "[]",
        ),
        "Map (#1)",
    );
    git(root, &["checkout", "-q", "-b", "work"]);
    // Two links come on the pair a and c, and both of a and b's go: one line drawn new, one gone.
    let head = commit(
        root,
        &small(
            "h",
            r#"[{"id":"a-c","from":"a","to":"c"},{"id":"c-a","from":"c","to":"a"}]"#,
            "[]",
        ),
        "work",
    );
    let found = counts(Path::new(&main), &base, &head).expect("the PR changed the map");
    assert_eq!(found.sentence(), "1 link added, 1 removed.");
    assert_eq!(found.lit, ["a", "b", "c"]);
}

#[test]
fn a_reformatted_map_has_no_document() {
    let map = small("h", r#"[{"id":"a-b","from":"a","to":"b"}]"#, "[]");
    let pretty =
        serde_json::to_string_pretty(&serde_json::from_str::<Value>(&map).unwrap()).unwrap();
    let (_dir, main) = landed(&map, &pretty, 7);
    assert_eq!(of(Path::new(&main), 7, None), None);
    // A PR that is not on the default branch and not open has nothing to compare either.
    assert_eq!(of(Path::new(&main), 8, None), None);
}

#[test]
fn an_open_pr_compares_its_merge_base_with_its_head_and_main_moving_after_it_shows_untagged() {
    let (dir, main) = repo();
    let root = dir.path();
    let law = r#"[{"id":"l1","on":"a","backing":"trust","text":"First."}]"#;
    commit(root, &small("h", "[]", "[]"), "Map (#1)");
    git(root, &["checkout", "-q", "-b", "work"]);
    // The PR adds a link a-c and a law.
    let head = commit(
        root,
        &small("h", r#"[{"id":"a-c","from":"a","to":"c"}]"#, law),
        "work",
    );
    git(root, &["checkout", "-q", "main"]);
    // Main gains part d after the branch left it.
    let mut moved: Value = serde_json::from_str(&small("h", "[]", "[]")).unwrap();
    moved["parts"].as_array_mut().unwrap().push(serde_json::json!(
        {"id":"d","name":"D","caption":"c","purpose":"p","tier":"core","paths":["d/"],"holds":[]}
    ));
    commit(root, &moved.to_string(), "Part d (#2)");
    let diff = of(Path::new(&main), 3, Some(&head)).expect("the PR changed the map");
    assert_eq!(diff.head, head);
    assert_eq!(
        diff.base,
        git(root, &["rev-parse", "HEAD~1"]),
        "the merge base, not main's tip"
    );
    assert_eq!(lines(&diff, Tag::Added), ["a|c"]);
    assert_eq!(diff.laws, BTreeMap::from([("l1".into(), Tag::Added)]));
    // Today's map: main's part d is there, untagged; the PR's link and law are drawn on it.
    assert!(drawn(&diff, "parts").contains(&"d".into()));
    assert!(!diff.parts.contains_key("d"));
    assert!(drawn(&diff, "links").contains(&"a-c".into()));
    assert!(drawn(&diff, "laws").contains(&"l1".into()));
    // Once it lands by a squash commit, it compares that commit with its parent, and today's map already holds the change.
    commit(
        root,
        &{
            let mut landed: Value = moved.clone();
            landed["links"] = serde_json::json!([{"id":"a-c","from":"a","to":"c"}]);
            landed["laws"] = serde_json::from_str(law).unwrap();
            landed.to_string()
        },
        "The PR (#3)",
    );
    let done = of(Path::new(&main), 3, None).expect("the landed PR changed the map");
    assert_eq!(done.head, git(root, &["rev-parse", "HEAD"]));
    assert_eq!(done.base, git(root, &["rev-parse", "HEAD~1"]));
    assert_eq!(lines(&done, Tag::Added), ["a|c"]);
    assert_eq!(drawn(&done, "links"), ["a-c"], "drawn once");
}
