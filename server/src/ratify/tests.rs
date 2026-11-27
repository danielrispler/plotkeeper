use super::*;

const OLD: &str = r#"{"version":1,
  "parts":[{"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
           {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}],
  "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":[],
  "laws":[{"id":"l1","on":"a","backing":"trust","text":"First."},
          {"id":"l2","on":"b","backing":"trust","text":"Second."}],"flows":[]}"#;

fn new_map() -> String {
    OLD.replace("First.", "First, reworded.")
        .replace(
            r#""laws":["#,
            r#""laws":[{"id":"l3","on":"a","backing":"trust","text":"Third."},"#,
        )
        .replace(r#"{"id":"a-b","from":"a","to":"b"}"#, "")
        .replace(
            r#""tier":"core","paths":["a/"]"#,
            r#""tier":"interface","paths":["a/"]"#,
        )
}

#[test]
fn the_maps_diff_is_read_by_code_into_items_keyed_by_kind_and_id() {
    let items = map_items(Some(OLD), Some(&new_map()));
    let got: Vec<(&str, &str, &str)> = items
        .iter()
        .map(|i| (i.kind.as_str(), i.id.as_str(), i.change.as_str()))
        .collect();
    // l1 reworded, l3 added, the link removed; a's tier is Daniel's own and l2 did not change.
    assert_eq!(
        got,
        [
            ("link", "a-b", "removed"),
            ("law", "l1", "changed"),
            ("law", "l3", "added")
        ]
    );
    assert_eq!(items[1].text, "First, reworded.");
    assert_eq!(items[0].text, "a to b");
    assert!(
        map_items(None, Some(OLD)).is_empty(),
        "no map before: nothing to tell apart"
    );
    assert!(map_items(Some("not json"), Some(OLD)).is_empty());
}

/// A version 2 map of part `a` with `purpose`, own `paths` and `holds`, and a link a-b of `link` (its `kind` and `reason`).
fn v2(purpose: &str, paths: &str, holds: &str, link: &str) -> String {
    format!(
        r#"{{"version":2,
          "parts":[{{"id":"a","name":"A","caption":"c","purpose":"{purpose}","tier":"core","paths":{paths},"holds":{holds}}},
                   {{"id":"b","name":"B","caption":"c","purpose":"p","tier":"core","paths":["b/"],"holds":[]}}],
          "actors":[],"links":[{{"id":"a-b","from":"a","to":"b"{link}}}],"ignored":[],"laws":[],"flows":[]}}"#
    )
}

fn part_detail(items: &[MapItem], id: &str) -> PartDetail {
    match items
        .iter()
        .find(|i| i.kind == "part" && i.id == id)
        .and_then(|i| i.detail.clone())
    {
        Some(Detail::Part(detail)) => detail,
        other => panic!("no part detail for {id}: {other:?}"),
    }
}

fn words(before: Option<&str>, after: Option<&str>) -> Words {
    Words {
        before: before.map(String::from),
        after: after.map(String::from),
    }
}

#[test]
fn a_changed_purpose_shows_old_beside_new() {
    let old = v2("Keeps the state.", r#"["a/"]"#, "[]", "");
    let new = v2("Keeps and serves the state.", r#"["a/"]"#, "[]", "");
    let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
    assert_eq!(detail.name, words(Some("A"), Some("A")));
    assert_eq!(
        detail.purpose,
        words(
            Some("Keeps the state."),
            Some("Keeps and serves the state.")
        )
    );
    assert!(detail.holds.is_empty());
}

#[test]
fn a_name_held_twice_shows_each_item_once() {
    let old = v2("p", r#"["a/"]"#, "[]", "");
    let new = v2(
        "p",
        r#"["a/"]"#,
        r#"[{"name":"Fog","text":"A"},{"name":"Fog","text":"B"}]"#,
        "",
    );
    let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
    let texts: Vec<&Words> = detail.holds.iter().map(|h| &h.text).collect();
    assert_eq!(texts, [&words(None, Some("A")), &words(None, Some("B"))]);
}

#[test]
fn held_items_are_matched_by_name_and_each_ones_change_shows() {
    let old = v2(
        "p",
        r#"["a/"]"#,
        r#"[{"name":"Fog","text":"Greys a part.","paths":["a/fog.rs"]},
            {"name":"Locks","text":"One writer."},
            {"name":"Same","text":"Unchanged."}]"#,
        "",
    );
    let new = v2(
        "p",
        r#"["a/"]"#,
        r#"[{"name":"Fog","text":"Greys a part a PR changed.","paths":["a/fog.rs","a/fog/"]},
            {"name":"Same","text":"Unchanged."},
            {"name":"Briefs","text":"Tells a PR's story.","paths":["a/briefs.rs"]}]"#,
        "",
    );
    let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
    let held: Vec<(&str, &Words, &[String], &[String])> = detail
        .holds
        .iter()
        .map(|h| {
            (
                h.name.as_str(),
                &h.text,
                h.paths_added.as_slice(),
                h.paths_removed.as_slice(),
            )
        })
        .collect();
    let s = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(
        held,
        [
            (
                "Fog",
                &words(Some("Greys a part."), Some("Greys a part a PR changed.")),
                s(&["a/fog/"]).as_slice(),
                s(&[]).as_slice()
            ),
            (
                "Same",
                &words(Some("Unchanged."), Some("Unchanged.")),
                s(&[]).as_slice(),
                s(&[]).as_slice()
            ),
            (
                "Briefs",
                &words(None, Some("Tells a PR's story.")),
                s(&["a/briefs.rs"]).as_slice(),
                s(&[]).as_slice()
            ),
            (
                "Locks",
                &words(Some("One writer."), None),
                s(&[]).as_slice(),
                s(&[]).as_slice()
            ),
        ],
        "the new side's order, then what was removed"
    );
}

#[test]
fn a_parts_own_paths_change_shows_as_the_patterns_added_and_removed() {
    let old = v2("p", r#"["a/","old/"]"#, "[]", "");
    let new = v2("p", r#"["a/","new/"]"#, "[]", "");
    let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
    assert_eq!(detail.paths_added, ["new/"]);
    assert_eq!(detail.paths_removed, ["old/"]);
}

#[test]
fn an_added_part_shows_its_purpose_and_held_items_with_nothing_before() {
    let old = v2("p", r#"["a/"]"#, "[]", "");
    let new = old.replace(
        r#"{"id":"b","#,
        r#"{"id":"c","name":"C","caption":"c","purpose":"Installs the hook.","tier":"core","paths":["c/"],
            "holds":[{"name":"Copies","text":"The hook's copies.","paths":["c/copies.rs"]}]},{"id":"b","#,
    );
    let detail = part_detail(&map_items(Some(&old), Some(&new)), "c");
    assert_eq!(detail.name, words(None, Some("C")));
    assert_eq!(detail.purpose, words(None, Some("Installs the hook.")));
    assert_eq!(detail.holds.len(), 1);
    assert_eq!(detail.holds[0].name, "Copies");
    assert_eq!(
        detail.holds[0].text,
        words(None, Some("The hook's copies."))
    );
    assert_eq!(detail.holds[0].paths_added, ["c/copies.rs"]);
    assert_eq!(detail.paths_added, ["c/"]);
}

#[test]
fn a_links_changed_kind_and_reason_show_old_beside_new() {
    let old = v2("p", r#"["a/"]"#, "[]", r#","kind":"import""#);
    let new = v2(
        "p",
        r#"["a/"]"#,
        "[]",
        r#","kind":"http","reason":"The page calls the server over HTTP.""#,
    );
    let items = map_items(Some(&old), Some(&new));
    let link = items
        .iter()
        .find(|i| i.kind == "link")
        .expect("the link changed");
    assert_eq!(link.change, "changed");
    assert_eq!(
        link.detail,
        Some(Detail::Link(LinkDetail {
            kind: words(Some("import"), Some("http")),
            reason: words(Some(""), Some("The page calls the server over HTTP.")),
            direction: None,
        }))
    );
}

#[test]
fn a_version_1_side_is_converted_so_its_description_shows_as_the_old_purpose() {
    // OLD is version 1: part a's description is "d". The same map in version 2 with a new purpose and a held item.
    let new = OLD
        .replace(r#""version":1"#, r#""version":2"#)
        .replace(
            r#""id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]"#,
            r#""id":"a","name":"A","caption":"c","purpose":"Keeps the state.","tier":"core","paths":["a/"],"holds":[{"name":"Fog","text":"Greys a part."}]"#,
        )
        .replace(r#""description":"d""#, r#""purpose":"d","holds":[]"#);
    let items = map_items(Some(OLD), Some(&new));
    let changed: Vec<(&str, &str)> = items
        .iter()
        .map(|i| (i.kind.as_str(), i.id.as_str()))
        .collect();
    assert_eq!(
        changed,
        [("part", "a")],
        "b only moved from description to purpose: no change"
    );
    let detail = part_detail(&items, "a");
    assert_eq!(detail.purpose, words(Some("d"), Some("Keeps the state.")));
    assert_eq!(detail.holds[0].text, words(None, Some("Greys a part.")));
}

#[test]
fn a_changed_caption_shows_old_beside_new() {
    let old = v2("p", r#"["a/"]"#, "[]", "");
    let new = old.replacen(r#""caption":"c""#, r#""caption":"Keeps it""#, 1);
    let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
    assert_eq!(detail.caption, words(Some("c"), Some("Keeps it")));
}

#[test]
fn a_link_whose_ends_swapped_shows_its_direction_old_beside_new() {
    let old = v2("p", r#"["a/"]"#, "[]", "");
    let new = old.replace(r#""from":"a","to":"b""#, r#""from":"b","to":"a""#);
    let items = map_items(Some(&old), Some(&new));
    let Some(Detail::Link(link)) = items
        .iter()
        .find(|i| i.kind == "link")
        .and_then(|i| i.detail.clone())
    else {
        panic!("the link changed: {items:?}");
    };
    assert_eq!(link.direction, Some(words(Some("a to b"), Some("b to a"))));
    // A link whose ends did not swap has no direction to tell.
    let reasoned = v2("p", r#"["a/"]"#, "[]", r#","reason":"Calls it.""#);
    let items = map_items(Some(&old), Some(&reasoned));
    let Some(Detail::Link(link)) = items.iter().find_map(|i| i.detail.clone()) else {
        panic!("the link changed: {items:?}");
    };
    assert_eq!(link.direction, None);
}

#[test]
fn each_map_item_names_the_mark_it_lights_on_the_map_diff() {
    let old = v2("p", r#"["a/"]"#, "[]", "")
        .replace(
            r#""laws":[]"#,
            r#""laws":[{"id":"l1","on":"a","backing":"trust","text":"First."}]"#,
        )
        .replace(r#""actors":[]"#, r#""actors":[{"id":"x","name":"X"}]"#)
        .replace(
            r#""flows":[]"#,
            r#""flows":[{"id":"f","name":"F","steps":[]}]"#,
        );
    // Every item changes: a's purpose, the link flips (the line keeps its sorted ends), the law, the actor and the flow.
    let new = old
        .replace(
            r#""purpose":"p","tier":"core","paths":["a/"]"#,
            r#""purpose":"q","tier":"core","paths":["a/"]"#,
        )
        .replace(r#""from":"a","to":"b""#, r#""from":"b","to":"a""#)
        .replace("First.", "First, reworded.")
        .replace(r#""name":"X""#, r#""name":"Y""#)
        .replace(r#""name":"F""#, r#""name":"G""#);
    let marks: Vec<(String, Option<String>)> = map_items(Some(&old), Some(&new))
        .into_iter()
        .map(|i| (i.kind, i.mark))
        .collect();
    let s = |kind: &str, mark: Option<&str>| (kind.to_string(), mark.map(String::from));
    assert_eq!(
        marks,
        [
            s("part", Some("part:a")),
            s("actor", Some("actor:x")),
            s("link", Some("line:a|b")),
            s("law", Some("law:l1")),
            s("flow", None),
        ]
    );
}

#[test]
fn an_item_is_known_by_its_pr_kind_and_id() {
    assert_eq!(Item::new(7, "law", "l1").key(), "7/law/l1");
    assert_ne!(
        Item::new(7, "law", "l1").key(),
        Item::new(8, "law", "l1").key()
    );
    assert_ne!(
        Item::new(7, "law", "x").key(),
        Item::new(7, "link", "x").key()
    );
    assert_ne!(
        Item::new(7, "law", "d1").key(),
        Item::new(7, DECISION, "d1").key()
    );
}

fn dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(FILE);
    (dir, file)
}

#[test]
fn a_decision_is_kept_and_read_back_and_another_project_is_left_alone() {
    let (_dir, file) = dir();
    let item = Item::new(7, "law", "l1");
    assert_eq!(read(&file, "/p").records, vec![], "no file: empty");
    start(&file, "/p", 100).unwrap();
    start(&file, "/q", 300).unwrap();
    decide(&file, "/p", &item, Decided::Accepted, "First.", 150).unwrap();
    decide(&file, "/p", &item, Decided::Overturned, "First.", 160).unwrap();
    let mine = read(&file, "/p");
    assert_eq!(mine.since, 100);
    assert_eq!(
        mine.records.len(),
        1,
        "a later decision replaces the earlier"
    );
    assert_eq!(mine.records[0].state, Decided::Overturned);
    assert_eq!(mine.records[0].at, 160);
    assert_eq!(read(&file, "/q").since, 300);
    // The first run is only recorded once.
    assert_eq!(start(&file, "/p", 999).unwrap().since, 100);
}

#[test]
fn a_file_of_an_older_or_newer_format_still_reads() {
    let (_dir, file) = dir();
    // Older: no version, no `since`, a record with only the bare fields. Newer: unknown fields and an unknown state.
    std::fs::write(
        &file,
        r#"{"later":{"x":1},"projects":{"/p":{"extra":true,"records":[
            {"pr":7,"kind":"law","id":"l1","state":"accepted"},
            {"pr":7,"kind":"law","id":"l2","state":"snoozed","at":5,"new":"field"}]}}}"#,
    )
    .unwrap();
    let mine = read(&file, "/p");
    assert_eq!(mine.since, 0);
    assert_eq!(mine.records.len(), 2);
    assert_eq!(
        mine.status(&Item::new(7, "law", "l1"), Some(10)),
        Status::Accepted
    );
    // A state this build does not know is no record.
    assert_eq!(
        mine.status(&Item::new(7, "law", "l2"), Some(10)),
        Status::NotYet
    );
    // Writing keeps what it does not know.
    decide(
        &file,
        "/p",
        &Item::new(8, "law", "l9"),
        Decided::Accepted,
        "",
        1,
    )
    .unwrap();
    let whole: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(whole["later"]["x"], 1);
    assert_eq!(whole["projects"]["/p"]["extra"], true);
    assert_eq!(whole["version"], 1);
    // A record of a newer build comes back as it was: its state this build cannot read, and its extra field.
    let kept = &whole["projects"]["/p"]["records"][1];
    assert_eq!(kept["state"], "snoozed");
    assert_eq!(kept["new"], "field");
    // A file that is not JSON is an empty state.
    std::fs::write(&file, "nope").unwrap();
    assert_eq!(read(&file, "/p"), Project::default());
}

#[test]
fn what_landed_before_the_first_run_is_ratified_and_what_landed_after_is_not_yet() {
    let project = Project {
        since: 100,
        records: vec![Record {
            pr: 3,
            kind: "law".into(),
            id: "kept-open".into(),
            state: Decided::Pending,
            at: 0,
            text: String::new(),
            extra: BTreeMap::new(),
        }],
    };
    let law = |pr, id| Item::new(pr, "law", id);
    assert_eq!(project.status(&law(1, "a"), Some(50)), Status::Ratified);
    assert_eq!(project.status(&law(1, "a"), Some(100)), Status::Ratified);
    assert_eq!(project.status(&law(2, "a"), Some(101)), Status::NotYet);
    assert_eq!(
        project.status(&law(2, "a"), None),
        Status::NotYet,
        "an open PR's item"
    );
    // Marked by hand: not yet, although it landed before the first run.
    assert_eq!(
        project.status(&law(3, "kept-open"), Some(50)),
        Status::NotYet
    );
}

#[test]
fn a_reworded_rewrite_keeps_a_decisions_record() {
    use studio_understanding::pr_brief::{Brief, Decision, assign_decision_ids};
    let (_dir, file) = dir();
    let first = Decision {
        id: Some("d1".into()),
        text: "Briefs are stored per project.".into(),
        flags: vec![],
    };
    decide(
        &file,
        "/p",
        &Item::new(7, DECISION, "d1"),
        Decided::Accepted,
        &first.text,
        5,
    )
    .unwrap();
    // The rewrite words the same decision differently and the writer gives its id back.
    let mut rewrite = Brief {
        decisions: vec![Decision {
            id: Some("d1".into()),
            text: "Each project keeps its own briefs.".into(),
            flags: vec![],
        }],
        ..Brief::default()
    };
    assign_decision_ids(&mut rewrite, &[first]);
    let decisions: Vec<(String, String)> = rewrite
        .decisions
        .iter()
        .map(|d| (d.id.clone().unwrap(), d.text.clone()))
        .collect();
    assert_eq!(decisions[0].0, "d1");
    let listed = rows(&read(&file, "/p"), 7, &[], &decisions, None);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].status, Status::Accepted, "not asked twice");
    assert_eq!(listed[0].text, "Each project keeps its own briefs.");
}

#[test]
fn a_decided_item_missing_from_a_rewrite_keeps_its_record_and_is_still_listed() {
    let (_dir, file) = dir();
    let item = Item::new(7, DECISION, "d2");
    decide(&file, "/p", &item, Decided::Overturned, "Use a timer.", 5).unwrap();
    let listed = rows(
        &read(&file, "/p"),
        7,
        &[],
        &[("d3".into(), "New one.".into())],
        None,
    );
    let ids: Vec<(&str, Status)> = listed.iter().map(|r| (r.id.as_str(), r.status)).collect();
    assert_eq!(ids, [("d3", Status::NotYet), ("d2", Status::Overturned)]);
    assert_eq!(listed[1].text, "Use a timer.");
    // It is another PR's record: not listed there.
    assert!(rows(&read(&file, "/p"), 8, &[], &[], None).is_empty());
    // And the file still holds it.
    assert_eq!(read(&file, "/p").records.len(), 1);
}

#[test]
fn a_prs_map_items_carry_their_status() {
    let project = Project {
        since: 100,
        ..Project::default()
    };
    let map = map_items(Some(OLD), Some(&new_map()));
    let after = rows(&project, 7, &map, &[], Some(200));
    assert!(after.iter().all(|r| r.status == Status::NotYet));
    let before = rows(&project, 7, &map, &[], Some(50));
    assert!(before.iter().all(|r| r.status == Status::Ratified));
    // The first run's own second counts as before it; the next one does not.
    let at = rows(&project, 7, &map, &[], Some(100));
    assert!(at.iter().all(|r| r.status == Status::Ratified));
    let next = rows(&project, 7, &map, &[], Some(101));
    assert!(next.iter().all(|r| r.status == Status::NotYet));
    // An open PR has not landed: nothing is ratified by a record that is not there.
    let open = rows(&project, 7, &map, &[], None);
    assert!(open.iter().all(|r| r.status == Status::NotYet));
}

#[test]
fn an_added_link_carries_the_evidence_its_brief_stored_and_no_other_row_does() {
    let item = |kind: &str, id: &str, change: &str| MapItem {
        kind: kind.into(),
        id: id.into(),
        change: change.into(),
        text: String::new(),
        was: None,
        detail: None,
        mark: None,
    };
    let map = [
        item("link", "a-b", "added"),
        item("link", "b-c", "removed"),
        item("link", "c-d", "added"),
        item("part", "a-b", "added"),
    ];
    let mut found = rows(&Project::default(), 7, &map, &[], None);
    let sites = vec![ImportSite {
        file: "a/x.ts".into(),
        line: Some(4),
    }];
    let stored = [
        LinkEvidence {
            link: "a-b".into(),
            sites: sites.clone(),
        },
        LinkEvidence {
            link: "b-c".into(),
            sites: sites.clone(),
        },
    ];
    with_evidence(&mut found, Some("cafe"), "cafe", &stored);
    let got: Vec<(&str, Option<Evidence>)> = found
        .iter()
        .map(|r| (r.id.as_str(), r.evidence.clone()))
        .collect();
    assert_eq!(
        got,
        [
            (
                "a-b",
                Some(Evidence {
                    head: "cafe".into(),
                    sites
                })
            ),
            ("b-c", None),
            ("c-d", None),
            ("a-b", None),
        ]
    );
}

#[test]
fn evidence_stored_at_another_head_than_the_rows_were_read_at_is_not_shown() {
    let map = [MapItem {
        kind: "link".into(),
        id: "a-b".into(),
        change: "added".into(),
        text: String::new(),
        was: None,
        detail: None,
        mark: None,
    }];
    let stored = [LinkEvidence {
        link: "a-b".into(),
        sites: vec![ImportSite {
            file: "a/x.ts".into(),
            line: Some(4),
        }],
    }];
    // A push since the brief was written: the rows are the new head's, the evidence the old one's.
    let mut found = rows(&Project::default(), 7, &map, &[], None);
    with_evidence(&mut found, Some("beef"), "cafe", &stored);
    assert_eq!(found[0].evidence, None);
}

mod git_repo {
    use super::*;
    use studio_test_support::{git as run, repo};

    fn commit(dir: &Path, map: &str, subject: &str, date: &str) {
        std::fs::write(dir.join("system-map.json"), map).unwrap();
        run(dir, &["add", "-A"]);
        let stamp = format!("{date} +0000");
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=S",
                "-c",
                "user.email=s@e.com",
                "commit",
                "-q",
                "-m",
                subject,
            ])
            .env("GIT_COMMITTER_DATE", &stamp)
            .env("GIT_AUTHOR_DATE", &stamp)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[test]
    fn the_sheet_lists_what_landed_after_the_first_run_without_a_record() {
        let (dir, main) = repo();
        let root = dir.path();
        commit(root, OLD, "Map (#1)", "1000000000");
        commit(root, &new_map(), "Change laws (#2)", "1100000000");
        let repo = Path::new(&main);
        let changes = ChangesCache::default();
        // The first run was between the two.
        let mut project = Project {
            since: 1_050_000_000,
            ..Project::default()
        };
        let got: Vec<(u64, &str, String)> = pending(&changes, repo, &project)
            .iter()
            .map(|p| (p.pr, p.kind.as_str(), p.id.clone()))
            .map(|(pr, k, id)| {
                (
                    pr,
                    if k == "law" {
                        "law"
                    } else if k == "link" {
                        "link"
                    } else {
                        "other"
                    },
                    id,
                )
            })
            .collect();
        assert!(got.contains(&(2, "law", "l1".into())));
        assert!(got.contains(&(2, "law", "l3".into())));
        assert!(got.contains(&(2, "link", "a-b".into())));
        assert!(
            got.iter().all(|(pr, _, _)| *pr == 2),
            "PR 1 landed before the first run"
        );
        // Accepting one takes it off.
        project.records.push(Record {
            pr: 2,
            kind: "law".into(),
            id: "l1".into(),
            state: Decided::Accepted,
            at: 1,
            text: String::new(),
            extra: BTreeMap::new(),
        });
        assert!(
            !pending(&changes, repo, &project)
                .iter()
                .any(|p| p.id == "l1")
        );
        // A record marked pending by hand shows although PR 1 landed before the first run.
        project.records.push(Record {
            pr: 1,
            kind: "law".into(),
            id: "l2".into(),
            state: Decided::Pending,
            at: 0,
            text: String::new(),
            extra: BTreeMap::new(),
        });
        assert!(
            pending(&changes, repo, &project)
                .iter()
                .any(|p| p.pr == 1 && p.id == "l2")
        );
    }

    #[test]
    fn a_landed_prs_changes_are_its_squash_commits_and_an_open_prs_are_its_branchs() {
        let (dir, main) = repo();
        let root = dir.path();
        commit(root, OLD, "Map (#1)", "1000000000");
        run(root, &["checkout", "-q", "-b", "work"]);
        commit(root, &new_map(), "work", "1100000000");
        let head = run(root, &["rev-parse", "HEAD"]);
        run(root, &["checkout", "-q", "main"]);
        let repo = Path::new(&main);
        let changes = ChangesCache::default();
        // Open: not on main yet.
        let (open, landed) = pr_changes(&changes, repo, 2, Some(&head));
        assert_eq!(landed, None);
        assert_eq!(open.len(), 3);
        // Landed by a squash commit.
        commit(root, &new_map(), "Change laws (#2)", "1200000000");
        let (done, landed) = pr_changes(&changes, repo, 2, None);
        assert_eq!(landed, Some(1_200_000_000));
        assert_eq!(done.len(), 3);
        // A PR that is not ready and not in the log has not landed: a draft, a closed one, or one not looked up yet. Its items
        // are not ratified.
        assert_eq!(pr_changes(&changes, repo, 99, None), (vec![], None));
        // Only when the walk was cut short may it have landed before the cut: long ago.
        assert_eq!(
            pr_changes_in(&changes, repo, 99, None, 1),
            (vec![], Some(0))
        );
    }

    #[test]
    fn a_map_that_git_failed_to_show_is_read_again_on_the_next_call() {
        let (dir, main) = repo();
        let root = dir.path();
        commit(root, OLD, "Map (#1)", "1000000000");
        commit(root, &new_map(), "Change laws (#2)", "1100000000");
        let repo = Path::new(&main);
        let changes = ChangesCache::default();
        // The new map's object is out of reach for a moment: `git log` still answers, `git show` of the map fails.
        let blob = run(root, &["rev-parse", "HEAD:system-map.json"]);
        let object = root.join(".git/objects").join(&blob[..2]).join(&blob[2..]);
        let aside = root.join("aside");
        std::fs::rename(&object, &aside).unwrap();
        let (failed, landed) = pr_changes(&changes, repo, 2, None);
        assert_eq!(landed, Some(1_100_000_000));
        assert_ne!(failed.len(), 3, "the map was read without its object");
        std::fs::rename(&aside, &object).unwrap();
        let (read, _) = pr_changes(&changes, repo, 2, None);
        assert_eq!(read.len(), 3, "the failed read was kept: {read:?}");
    }

    #[test]
    fn a_commit_with_no_map_is_answered_from_the_cache_on_the_next_call() {
        let (dir, main) = repo();
        let root = dir.path();
        commit(root, OLD, "Map (#1)", "1000000000");
        // PR 2 removes the map: git answers that its commit has none, which is an answer to keep.
        std::fs::remove_file(root.join("system-map.json")).unwrap();
        run(root, &["add", "-A"]);
        run(
            root,
            &[
                "-c",
                "user.name=S",
                "-c",
                "user.email=s@e.com",
                "commit",
                "-q",
                "-m",
                "Drop the map (#2)",
            ],
        );
        let head = run(root, &["rev-parse", "HEAD"]);
        let repo = Path::new(&main);
        let changes = ChangesCache::default();
        assert_eq!(changes_of(&changes, repo, &head), vec![]);
        let key = (repo.display().to_string(), head.clone());
        assert!(
            changes.lock().unwrap().contains_key(&key),
            "a commit with no map is read again on every call"
        );
    }

    #[test]
    fn a_commit_dated_before_the_first_run_does_not_end_the_walk_of_what_landed_after_it() {
        let (dir, main) = repo();
        let root = dir.path();
        commit(root, OLD, "Map (#0)", "800000000");
        commit(root, &new_map(), "Change laws (#1)", "1050000000");
        // Landed later but dated earlier (a rebased or backdated commit), and it changes no item.
        commit(
            root,
            &format!("{}\n", new_map()),
            "Reformat (#2)",
            "900000000",
        );
        let project = Project {
            since: 1_000_000_000,
            ..Project::default()
        };
        let got = pending(&ChangesCache::default(), Path::new(&main), &project);
        assert!(
            got.iter().any(|p| p.pr == 1 && p.id == "l3"),
            "PR 1 landed after the first run: {got:?}"
        );
    }

    #[test]
    fn a_sites_snippet_is_read_from_the_commit_around_its_line() {
        let (dir, main) = repo();
        let root = dir.path();
        let text: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        std::fs::create_dir_all(root.join("alpha")).unwrap();
        std::fs::write(root.join("alpha/a.ts"), &text).unwrap();
        commit(root, OLD, "Map (#1)", "1000000000");
        let head = run(root, &["rev-parse", "HEAD"]);
        let head = head.trim();
        // The working tree moved on; the commit's text is the one read.
        std::fs::write(root.join("alpha/a.ts"), "changed\n").unwrap();
        let repo = Path::new(&main);
        let lines = |from: u32, to: u32| -> Vec<String> {
            (from..=to).map(|n| format!("line {n}")).collect()
        };
        let at = site_snippet(repo, head, "alpha/a.ts", Some(10)).unwrap();
        assert_eq!(
            at,
            SiteSnippet {
                file: "alpha/a.ts".into(),
                line: Some(10),
                first: 6,
                lines: lines(6, 14),
            }
        );
        // Near the top, and with no line: the file's first lines.
        assert_eq!(
            site_snippet(repo, head, "alpha/a.ts", Some(2))
                .unwrap()
                .first,
            1
        );
        let top = site_snippet(repo, head, "alpha/a.ts", None).unwrap();
        assert_eq!((top.first, top.lines), (1, lines(1, 9)));
        assert_eq!(
            site_snippet(repo, head, "alpha/a.ts", Some(20))
                .unwrap()
                .lines,
            lines(16, 20)
        );
        // A name that is no commit's hex is refused before git sees it.
        assert!(site_snippet(repo, "--output=x", "alpha/a.ts", None).is_err());
        assert!(site_snippet(repo, head, "alpha/none.ts", None).is_err());
        // A directory, or no file at all, is a tree: git would list it, and that is no site's code.
        assert!(site_snippet(repo, head, "alpha", None).is_err());
        assert!(site_snippet(repo, head, "", None).is_err());
    }

    /// A version 2 map of `alpha`, `beta` and `gamma`, read by `typescript-imports`, with `links`.
    fn evidence_map(links: serde_json::Value) -> String {
        let part = |id: &str| {
            serde_json::json!({ "id": id, "name": id, "caption": "c", "purpose": "p", "holds": [], "tier": "core",
                                "paths": [format!("{id}/")] })
        };
        serde_json::json!({
            "version": 2, "extractors": ["typescript-imports"],
            "parts": [part("alpha"), part("beta"), part("gamma")], "actors": [],
            "links": links, "ignored": ["system-map.json"], "laws": [], "flows": []
        })
        .to_string()
    }

    #[test]
    fn the_evidence_of_each_added_import_link_is_its_import_sites_read_from_the_head() {
        let (dir, main) = repo();
        let root = dir.path();
        let put = |path: &str, text: &str| {
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), text).unwrap();
        };
        let link = |id: &str, from: &str, to: &str, kind: &str| serde_json::json!({ "id": id, "from": from, "to": to, "kind": kind });
        put("beta/b.ts", "export const b = 1;\n");
        put("gamma/g.ts", "export const g = 1;\n");
        // An import the PR does not edit: it is in the head, not in the diff.
        put(
            "alpha/old.ts",
            "// old\nimport { g } from \"../gamma/g\";\n",
        );
        put("alpha/a.ts", "import { b } from \"../beta/b\";\n");
        let ab = link("alpha-beta", "alpha", "beta", "import");
        commit(
            root,
            &evidence_map(serde_json::json!([ab])),
            "Map (#1)",
            "1000000000",
        );
        let base = run(root, &["rev-parse", "HEAD"]);
        put(
            "alpha/a.ts",
            "import { b } from \"../beta/b\";\n\nimport { g } from \"../gamma/g\";\n",
        );
        put(
            "alpha/c.ts",
            "import { g } from \"../gamma/g\";\nexport { g } from \"../gamma/g\";\n",
        );
        let links = serde_json::json!([
            ab,
            link("alpha-gamma", "alpha", "gamma", "import"),
            // Added, but no edge stands behind an http link: no evidence.
            link("gamma-beta", "gamma", "beta", "http"),
            // A run-time link beside the import edge between the same parts: the edge is the import link's, not its.
            link("alpha-gamma-spawn", "alpha", "gamma", "spawn"),
        ]);
        commit(root, &evidence_map(links), "Gamma (#2)", "1100000000");
        let head = run(root, &["rev-parse", "HEAD"]);
        // The working tree is not read: only the head's files count.
        put("alpha/c.ts", "");
        let site = |file: &str, line: u32| ImportSite {
            file: file.into(),
            line: Some(line),
        };
        assert_eq!(
            head_check(Path::new(&main), base.trim(), head.trim(), true).link_evidence,
            [LinkEvidence {
                link: "alpha-gamma".into(),
                sites: vec![
                    site("alpha/a.ts", 3),
                    site("alpha/c.ts", 1),
                    site("alpha/c.ts", 2),
                    site("alpha/old.ts", 2),
                ],
            }]
        );
    }

    #[test]
    fn the_head_s_map_check_keeps_its_problems_when_the_pr_adds_no_link() {
        let (dir, main) = repo();
        let root = dir.path();
        let put = |path: &str, text: &str| {
            std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            std::fs::write(root.join(path), text).unwrap();
        };
        for part in ["alpha", "beta", "gamma"] {
            put(&format!("{part}/a.ts"), "export const a = 1;\n");
        }
        commit(
            root,
            &evidence_map(serde_json::json!([])),
            "Map (#1)",
            "1000000000",
        );
        let base = run(root, &["rev-parse", "HEAD"]);
        let repo = Path::new(&main);
        let at = |head: &str, trusted| head_check(repo, base.trim(), head.trim(), trusted);
        assert_eq!(
            at(&base, true),
            HeadCheck::default(),
            "the map holds at its base"
        );
        // A file no part owns, and no link added: the check still runs at the head.
        put("stray/s.ts", "export const s = 1;\n");
        commit(
            root,
            &evidence_map(serde_json::json!([])),
            "Stray (#2)",
            "1100000000",
        );
        let head = run(root, &["rev-parse", "HEAD"]);
        for trusted in [true, false] {
            let check = at(&head, trusted);
            assert!(check.link_evidence.is_empty());
            assert_eq!(check.map_problems.len(), 1, "{:?}", check.map_problems);
            assert!(
                check.map_problems[0].contains("stray/s.ts"),
                "{:?}",
                check.map_problems
            );
        }
    }

    #[test]
    fn a_head_with_no_map_has_no_map_check() {
        let (dir, main) = repo();
        let root = dir.path();
        std::fs::write(root.join("a.ts"), "export const a = 1;\n").unwrap();
        run(root, &["add", "-A"]);
        run(root, &["commit", "-q", "-m", "Code (#1)"]);
        let head = run(root, &["rev-parse", "HEAD"]);
        assert_eq!(
            head_check(Path::new(&main), head.trim(), head.trim(), false),
            HeadCheck::default()
        );
    }
}
