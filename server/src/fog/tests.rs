use super::*;
use studio_test_support::{git as run, repo};

fn classify(
    old: Option<&str>,
    new: Option<&str>,
    changed: &[String],
) -> BTreeMap<String, Strength> {
    explain(old, new, changed)
        .into_iter()
        .map(|(id, change)| (id, change.strength))
        .collect()
}

const BASE: &str = r#"{"version":1,
  "parts":[
    {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
    {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
    {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}],
  "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":["ign/"],
  "laws":[{"id":"l1","on":"a","backing":"trust","text":"t"}]}"#;

struct Rig {
    dir: tempfile::TempDir,
    main: String,
    state: tempfile::TempDir,
}
impl Rig {
    fn new() -> Rig {
        let (dir, main) = repo();
        let rig = Rig {
            dir,
            main,
            state: tempfile::tempdir().unwrap(),
        };
        rig.commit(&[
            ("system-map.json", BASE),
            ("a/x", "1"),
            ("b/x", "1"),
            ("c/x", "1"),
            ("ign/x", "1"),
        ]);
        rig
    }
    fn file(&self) -> std::path::PathBuf {
        self.state.path().join("map-state.json")
    }
    fn commit(&self, files: &[(&str, &str)]) {
        for (name, text) in files {
            let path = self.dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        run(self.dir.path(), &["add", "-A"]);
        run(self.dir.path(), &["commit", "-q", "-m", "change"]);
    }
    fn open(&self) {
        refresh(Path::new(&self.main), &self.file(), &self.main, true).unwrap();
    }
    fn fog(&self) -> BTreeMap<String, Strength> {
        refresh(Path::new(&self.main), &self.file(), &self.main, false).unwrap();
        let user = read_user(&self.file(), &self.main);
        effective(
            &user,
            &run(
                self.dir.path(),
                &["show", &format!("main:{}", "system-map.json")],
            ),
        )
    }
    fn raw(&self) -> FogState {
        read_user(&self.file(), &self.main).fog
    }
}

fn strengths(pairs: &[(&str, Strength)]) -> BTreeMap<String, Strength> {
    pairs.iter().map(|(id, s)| (id.to_string(), *s)).collect()
}

#[test]
fn the_first_open_starts_clear_and_later_commits_fog() {
    let rig = Rig::new();
    assert!(rig.fog().is_empty());
    assert_eq!(rig.raw().first, None, "without an open there is no start");
    rig.open();
    assert!(rig.fog().is_empty());
    assert!(rig.raw().first.is_some());
    rig.commit(&[("a/x", "2")]);
    assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
}

#[test]
fn internal_edits_are_thin_and_never_count() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[("a/x", "2")]);
    rig.commit(&[("a/x", "3"), ("a/y", "1")]);
    rig.commit(&[("c/x", "2")]);
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thin), ("c", Strength::Thin)])
    );
    assert_eq!(rig.raw().parts["a"].len(), 2);
}

#[test]
fn a_thick_change_beats_thin() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[("a/x", "2")]);
    rig.commit(&[("system-map.json", &BASE.replace("\"t\"", "\"changed\""))]);
    assert_eq!(rig.fog(), strengths(&[("a", Strength::Thick)]));
}

#[test]
fn a_new_link_is_thick_on_both_ends_and_a_new_flow_on_the_parts_it_crosses() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[(
        "system-map.json",
        &BASE.replace(
            r#""links":[{"id":"a-b","from":"a","to":"b"}]"#,
            r#""links":[{"id":"a-b","from":"a","to":"b"},{"id":"b-c","from":"b","to":"c"}]"#,
        ),
    )]);
    assert_eq!(
        rig.fog(),
        strengths(&[("b", Strength::Thick), ("c", Strength::Thick)])
    );
    let rig = Rig::new();
    rig.open();
    let flow = r#""flows":[{"id":"f","name":"F","hops":[{"link":"a-b","text":"x"}]}],"laws""#;
    rig.commit(&[("system-map.json", &BASE.replace(r#""laws""#, flow))]);
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thick), ("b", Strength::Thick)])
    );
}

#[test]
fn moving_a_path_between_parts_touches_both_by_the_union_of_the_maps() {
    let rig = Rig::new();
    rig.open();
    // `a` hands `a/` to `c`, and a file in it changes in the same commit.
    let moved = BASE
        .replace(r#""paths":["a/"]"#, r#""paths":[]"#)
        .replace(r#""paths":["c/"]"#, r#""paths":["c/","a/"]"#);
    rig.commit(&[("system-map.json", &moved), ("a/x", "2")]);
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thick), ("c", Strength::Thick)])
    );
}

#[test]
fn a_file_moved_without_changing_the_map_fogs_its_source_and_destination() {
    let rig = Rig::new();
    rig.open();
    run(rig.dir.path(), &["mv", "a/x", "b/moved"]);
    run(rig.dir.path(), &["commit", "-q", "-m", "move"]);
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
    );
}

#[test]
fn a_part_added_is_thick_and_one_removed_drops_out() {
    let rig = Rig::new();
    rig.open();
    let added = BASE.replace(r#""parts":["#, r#""parts":[{"id":"d","name":"D","caption":"c","description":"d","tier":"core","paths":["d/"]},"#);
    rig.commit(&[("system-map.json", &added), ("d/x", "1")]);
    assert_eq!(rig.fog(), strengths(&[("d", Strength::Thick)]));
    // Removing `c`: its own files go too, and it shows nothing.
    let removed = added.replace(
        r#",
    {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}"#,
        "",
    );
    assert_ne!(removed, added);
    run(rig.dir.path(), &["rm", "-q", "-r", "c"]);
    rig.commit(&[("system-map.json", &removed)]);
    assert!(!rig.fog().contains_key("c"));
}

#[test]
fn a_new_caption_or_description_is_thin_and_zone_ignored_and_tier_are_nothing() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[(
        "system-map.json",
        &BASE.replace(
            r#""id":"c","name":"C","caption":"c""#,
            r#""id":"c","name":"C","caption":"new words""#,
        ),
    )]);
    assert_eq!(rig.fog(), strengths(&[("c", Strength::Thin)]));
    let rig = Rig::new();
    rig.open();
    let quiet = BASE
        .replace(r#""paths":["b/"]}"#, r#""paths":["b/"],"zone":"left"}"#)
        .replace(r#""ignored":["ign/"]"#, r#""ignored":["ign/","other/"]"#)
        .replace(
            r#""id":"c","name":"C","caption":"c","description":"d","tier":"core""#,
            r#""id":"c","name":"C","caption":"c","description":"d","tier":"interface""#,
        );
    rig.commit(&[("system-map.json", &quiet), ("ign/x", "2")]);
    assert!(rig.fog().is_empty(), "{:?}", rig.fog());
}

/// `BASE` as version 2: each `description` a `purpose`, with no held items.
fn base_v2() -> String {
    BASE.replace(r#""version":1,"#, r#""version":2,"#)
        .replace(r#""description":"d""#, r#""purpose":"d","holds":[]"#)
}

#[test]
fn an_edited_description_in_a_version_1_map_is_thin() {
    let edited = BASE.replacen(r#""description":"d""#, r#""description":"new words""#, 1);
    assert_eq!(
        classify(Some(BASE), Some(&edited), &[]),
        strengths(&[("a", Strength::Thin)])
    );
}

#[test]
fn an_edited_purpose_or_held_item_is_thin() {
    let v2 = base_v2();
    let purpose = v2.replacen(r#""purpose":"d""#, r#""purpose":"new words""#, 1);
    assert_eq!(
        classify(Some(&v2), Some(&purpose), &[]),
        strengths(&[("a", Strength::Thin)])
    );
    let held = v2.replacen(
        r#""holds":[]"#,
        r#""holds":[{"name":"Reader","text":"Reads."}]"#,
        1,
    );
    assert_eq!(
        classify(Some(&v2), Some(&held), &[]),
        strengths(&[("a", Strength::Thin)])
    );
    let paths = held.replace(r#""text":"Reads."}"#, r#""text":"Reads.","paths":["a/x"]}"#);
    assert_eq!(
        classify(Some(&held), Some(&paths), &[]),
        strengths(&[("a", Strength::Thin)])
    );
}

#[test]
fn a_link_s_new_kind_or_reason_is_thin_on_both_its_ends() {
    let link = r#"{"id":"a-b","from":"a","to":"b"}"#;
    let v2 = base_v2().replace(link, r#"{"id":"a-b","from":"a","to":"b","kind":"import"}"#);
    let kind = v2.replace(
        r#""kind":"import""#,
        r#""kind":"http","reason":"A asks b over HTTP.""#,
    );
    assert_eq!(
        classify(Some(&v2), Some(&kind), &[]),
        strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
    );
    let reason = kind.replace("A asks b over HTTP.", "A asks b for its list.");
    assert_eq!(
        classify(Some(&kind), Some(&reason), &[]),
        strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
    );
    // A version 1 link, with no kind, given one.
    assert_eq!(
        classify(Some(BASE), Some(&v2), &[]),
        strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
    );
}

#[test]
fn moving_a_map_to_version_2_with_the_same_words_fogs_nothing() {
    assert!(classify(Some(BASE), Some(&base_v2()), &[]).is_empty());
    let reworded = base_v2().replacen(r#""purpose":"d""#, r#""purpose":"Short.""#, 1);
    assert_eq!(
        classify(Some(BASE), Some(&reworded), &[]),
        strengths(&[("a", Strength::Thin)])
    );
}

#[test]
fn a_map_with_fields_this_build_does_not_know_still_walks() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[
        (
            "system-map.json",
            &BASE.replace(r#""version":1,"#, r#""version":1,"future":{"x":1},"#),
        ),
        ("a/x", "2"),
    ]);
    assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
}

#[test]
fn daniels_tier_wins_and_black_box_and_interface_filter() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[("a/x", "2"), ("b/x", "2")]);
    rig.commit(&[("system-map.json", &BASE.replace("\"t\"", "\"changed\""))]);
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thick), ("b", Strength::Thin)])
    );
    let source = run(rig.dir.path(), &["show", "main:system-map.json"]);
    let mut user = read_user(&rig.file(), &rig.main);
    // The map says core; Daniel says black box for a, interface for b.
    user.tiers.insert("a".into(), "black-box".into());
    user.tiers.insert("b".into(), "interface".into());
    assert!(effective(&user, &source).is_empty());
    assert!(!core_fogged(&user, &source));
    // Interface shows a thick entry.
    user.tiers.insert("a".into(), "interface".into());
    assert_eq!(
        effective(&user, &source),
        strengths(&[("a", Strength::Thick)])
    );
    assert!(
        !core_fogged(&user, &source),
        "an interface part is not Core"
    );
    // Daniel's core over the map's own black box.
    let black = source.replace(
        r#""id":"b","name":"B","caption":"c","description":"d","tier":"core""#,
        r#""id":"b","name":"B","caption":"c","description":"d","tier":"black-box""#,
    );
    user.tiers.clear();
    assert_eq!(
        effective(&user, &black),
        strengths(&[("a", Strength::Thick)])
    );
    user.tiers.insert("b".into(), "core".into());
    assert_eq!(
        effective(&user, &black),
        strengths(&[("a", Strength::Thick), ("b", Strength::Thin)])
    );
    assert!(core_fogged(&user, &black));
}

#[test]
fn i_know_this_clears_a_part_and_the_clear_persists() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[("a/x", "2"), ("b/x", "2")]);
    assert_eq!(rig.fog().len(), 2);
    clear(&rig.file(), &rig.main, "a").unwrap();
    assert_eq!(rig.fog(), strengths(&[("b", Strength::Thin)]));
    // Persisted, and a later walk does not bring back what was cleared.
    assert!(rig.raw().parts.get("a").is_none_or(Vec::is_empty));
    assert_eq!(rig.fog(), strengths(&[("b", Strength::Thin)]));
    rig.commit(&[("a/x", "3")]);
    assert_eq!(rig.fog().len(), 2);
}

#[test]
fn a_part_with_no_tier_is_core_for_the_header_dot_as_it_is_for_the_map() {
    let source = r#"{"parts":[{"id":"a","name":"A","paths":[]}]}"#;
    let mut user = UserState::default();
    user.fog.parts.insert(
        "a".into(),
        vec![Entry {
            commit: "x".into(),
            strength: Strength::Thin,
            pr: None,
            why: Vec::new(),
        }],
    );
    assert!(effective(&user, source).contains_key("a"));
    assert!(core_fogged(&user, source));
}

#[test]
fn an_overlapping_walk_files_nothing_twice_and_does_not_undo_a_clear() {
    let walk = |commit: &str| {
        let thin = Change {
            strength: Strength::Thin,
            why: Vec::new(),
        };
        vec![(
            commit.to_string(),
            None,
            BTreeMap::from([("a".to_string(), thin)]),
        )]
    };
    let mut fog = FogState {
        processed: Some("d".into()),
        ..FogState::default()
    };
    // Two walks from the same start: the first files, the second finds the walk moved and is dropped.
    apply(&mut fog, "d", "h".into(), walk("x"));
    apply(&mut fog, "d", "h".into(), walk("x"));
    assert_eq!(fog.parts["a"].len(), 1);
    // "I know this" in between is not brought back by the slower walk.
    fog.parts.remove("a");
    apply(&mut fog, "d", "h".into(), walk("x"));
    assert!(!fog.parts.contains_key("a"));
    assert_eq!(fog.processed.as_deref(), Some("h"));
}

/// The PR and the reasons of the last entry filed on `part`.
fn why(rig: &Rig, part: &str) -> (Option<u64>, Vec<String>) {
    rig.fog();
    let entry = rig.raw().parts[part].last().cloned().unwrap();
    (entry.pr, entry.why)
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn a_fog_entry_keeps_its_pr_and_why_for_each_kind_of_change() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[("a/x", "2"), ("a/y", "1"), ("b/x", "2")]);
    assert_eq!(why(&rig, "a"), (None, words(&["2 files"])));
    assert_eq!(why(&rig, "b"), (None, words(&["1 file"])));
    // A squash-merge names its PR; a changed law on a.
    rig.branch();
    rig.commit(&[("system-map.json", LAW_CHANGED)]);
    rig.squash();
    assert_eq!(why(&rig, "a"), (Some(7), words(&["law l1"])));
    // A new link and a flow across it.
    let linked = LAW_CHANGED
        .replace(
            r#""links":[{"id":"a-b","from":"a","to":"b"}]"#,
            r#""links":[{"id":"a-b","from":"a","to":"b"},{"id":"b-c","from":"b","to":"c"}]"#,
        )
        .replace(
            r#""laws""#,
            r#""flows":[{"id":"f","name":"F","hops":[{"link":"b-c","text":"x"}]}],"laws""#,
        );
    rig.commit(&[("system-map.json", &linked)]);
    assert_eq!(why(&rig, "c"), (None, words(&["link b-c", "flow f"])));
    // Paths, and a part added.
    let moved = linked
        .replace(r#""paths":["c/"]"#, r#""paths":["c/","d/"]"#)
        .replace(r#""parts":["#, r#""parts":[{"id":"e","name":"E","caption":"c","description":"d","tier":"core","paths":["e/"]},"#);
    rig.commit(&[("system-map.json", &moved)]);
    assert_eq!(why(&rig, "c"), (None, words(&["paths"])));
    assert_eq!(why(&rig, "e"), (None, words(&["added"])));
    // Only its words: the field that changed.
    rig.commit(&[(
        "system-map.json",
        &moved.replace(
            r#""name":"B","caption":"c""#,
            r#""name":"B","caption":"new""#,
        ),
    )]);
    assert_eq!(why(&rig, "b"), (None, words(&["caption"])));
}

#[test]
fn an_entry_filed_before_it_kept_why_still_loads() {
    let entry: Entry = serde_json::from_str(r#"{"commit":"abc","strength":"thick"}"#).unwrap();
    assert_eq!((entry.pr, entry.why), (None, Vec::new()));
}

#[test]
fn an_older_state_file_without_fog_reads() {
    let rig = Rig::new();
    std::fs::write(
        rig.file(),
        format!(r#"{{"projects":{{"{}":{{"positions":{{}}}}}}}}"#, rig.main),
    )
    .unwrap();
    assert_eq!(rig.raw(), FogState::default());
}

// ---- Reading a brief clears the fog it would cause (spec #334, ticket #342).

impl Rig {
    fn head(&self) -> String {
        run(self.dir.path(), &["rev-parse", "HEAD"])
    }
    /// Starts a PR branch from main.
    fn branch(&self) {
        run(self.dir.path(), &["checkout", "-q", "-b", "pr"]);
    }
    /// Squash-merges the PR branch into main, as `Title (#7)`.
    fn squash(&self) {
        run(self.dir.path(), &["checkout", "-q", "main"]);
        run(self.dir.path(), &["merge", "--squash", "pr"]);
        run(self.dir.path(), &["commit", "-q", "-m", "Title (#7)"]);
    }
    fn read(&self, chapter: usize, head: &str, lit: &[&[&str]], at: u64) -> BTreeSet<String> {
        let lit = lit
            .iter()
            .map(|c| c.iter().map(ToString::to_string).collect())
            .collect();
        mark_read(&self.file(), &self.main, 7, chapter, head, lit, at).unwrap()
    }
    /// "I know this" on every part.
    fn clear_all(&self) {
        for part in ["a", "b", "c"] {
            clear(&self.file(), &self.main, part).unwrap();
        }
    }
    fn set(&self, parts: &[&str]) -> BTreeSet<String> {
        parts.iter().map(ToString::to_string).collect()
    }
}

const LAW_CHANGED: &str = r#"{"version":1,
  "parts":[
    {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
    {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
    {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]}],
  "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":["ign/"],
  "laws":[{"id":"l1","on":"a","backing":"trust","text":"changed"}]}"#;

#[test]
fn clearing_a_set_of_commits_from_a_set_of_parts_leaves_the_rest() {
    let rig = Rig::new();
    rig.open();
    rig.commit(&[("a/x", "2"), ("b/x", "2"), ("c/x", "2")]);
    let first = rig.head();
    rig.commit(&[("a/x", "3"), ("b/x", "3")]);
    rig.fog();
    let commits: HashSet<String> = [first.clone()].into();
    clear_commits(&rig.file(), &rig.main, &commits, &rig.set(&["a", "c"])).unwrap();
    let raw = rig.raw();
    assert_eq!(raw.parts["a"].len(), 1, "only that commit left a");
    assert_ne!(raw.parts["a"][0].commit, first);
    assert!(!raw.parts.contains_key("c"), "c had only that commit");
    assert_eq!(raw.parts["b"].len(), 2, "b was not in the set");
}

#[test]
fn a_part_lit_by_two_chapters_clears_only_after_both_are_read() {
    let rig = Rig::new();
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", "2"), ("b/x", "2")]);
    let x = rig.head();
    rig.squash();
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thin), ("b", Strength::Thin)])
    );
    let lit: &[&[&str]] = &[&["a", "b"], &["a"]];
    let done = rig.read(0, &x, lit, 1);
    assert_eq!(done, rig.set(&["b"]), "a still waits for chapter 1");
    clear_pr(Path::new(&rig.main), &rig.file(), &rig.main, 7, &done).unwrap();
    assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
    let done = rig.read(1, &x, lit, 2);
    assert_eq!(done, rig.set(&["a", "b"]));
    clear_pr(Path::new(&rig.main), &rig.file(), &rig.main, 7, &done).unwrap();
    assert!(rig.fog().is_empty());
    assert!(fully_read(&read_user(&rig.file(), &rig.main), 7));
}

#[test]
fn a_part_no_chapter_lights_keeps_its_fog() {
    let rig = Rig::new();
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", "2"), ("c/x", "2")]);
    let x = rig.head();
    rig.squash();
    let done = rig.read(0, &x, &[&["a"]], 1);
    clear_pr(Path::new(&rig.main), &rig.file(), &rig.main, 7, &done).unwrap();
    assert_eq!(rig.fog(), strengths(&[("c", Strength::Thin)]));
}

#[test]
fn reading_before_the_merge_with_no_later_push_files_nothing() {
    let rig = Rig::new();
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", "2"), ("b/x", "2")]);
    let x = rig.head();
    rig.read(0, &x, &[&["a", "b"]], 1);
    assert!(rig.fog().is_empty());
    rig.squash();
    assert!(rig.fog().is_empty(), "{:?}", rig.fog());
}

#[test]
fn a_push_after_the_read_fogs_only_the_part_it_touched_thin_or_thick_by_its_diff() {
    let rig = Rig::new();
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", "2"), ("b/x", "2"), ("c/x", "2")]);
    let x = rig.head();
    rig.read(0, &x, &[&["a", "b", "c"]], 1);
    // One push edits b's files; another changes the law on a.
    rig.commit(&[("b/x", "3")]);
    rig.commit(&[("system-map.json", LAW_CHANGED)]);
    rig.squash();
    assert_eq!(
        rig.fog(),
        strengths(&[("a", Strength::Thick), ("b", Strength::Thin)])
    );
}

#[test]
fn main_merged_into_the_branch_after_the_read_fogs_nothing() {
    let rig = Rig::new();
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", "2")]);
    let x = rig.head();
    rig.read(0, &x, &[&["a", "b", "c"]], 1);
    // Main moves on (its own commit fogs c), and the branch merges it in.
    run(rig.dir.path(), &["checkout", "-q", "main"]);
    rig.commit(&[("c/x", "9")]);
    run(rig.dir.path(), &["checkout", "-q", "pr"]);
    run(rig.dir.path(), &["merge", "-q", "-m", "sync", "main"]);
    rig.squash();
    assert_eq!(rig.fog(), strengths(&[("c", Strength::Thin)]));
    assert_eq!(rig.raw().parts["c"].len(), 1, "only main's own commit");
}

#[test]
fn a_brief_whose_head_is_unknown_to_git_leaves_the_fog_as_filed() {
    let rig = Rig::new();
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", "2")]);
    rig.read(0, &"0".repeat(40), &[&["a"]], 1);
    rig.squash();
    assert_eq!(rig.fog(), strengths(&[("a", Strength::Thin)]));
}

#[test]
fn carrying_a_read_over_moves_every_chapter_to_the_new_head_and_keeps_what_they_light() {
    let rig = Rig::new();
    let lit: &[&[&str]] = &[&["a", "b"], &["c"]];
    rig.read(0, "h1", lit, 1);
    rig.read(1, "h1", lit, 2);
    carry_read(&rig.file(), &rig.main, 7, "h1", "h2").unwrap();
    let user = read_user(&rig.file(), &rig.main);
    let read = &user.fog.reads[&7];
    assert!(read.fully_read_at("h2"), "{read:?}");
    assert_eq!(read.lit, vec![vec!["a", "b"], vec!["c"]]);
    assert_eq!(read.chapters[&1].at, 2, "the time of the read is kept");
    assert!(!fully_read_at(&user, 7, "h1"));
}

#[test]
fn fog_at_merge_after_a_rebase_carried_over_runs_from_the_new_head() {
    let rig = Rig::new();
    let lines = |first: &str, last: &str| format!("{first}\n2\n3\n4\n5\n6\n7\n8\n9\n{last}\n");
    rig.commit(&[("a/x", &lines("1", "10"))]);
    rig.open();
    rig.branch();
    rig.commit(&[("a/x", &lines("pr", "10"))]);
    let h1 = rig.head();
    rig.read(0, &h1, &[&["a"]], 1);
    // Main edits the other end of the PR's file, and the branch is rebased onto it: the same patch.
    run(rig.dir.path(), &["checkout", "-q", "main"]);
    rig.commit(&[("a/x", &lines("1", "main"))]);
    rig.fog();
    rig.clear_all();
    run(rig.dir.path(), &["checkout", "-q", "pr"]);
    run(rig.dir.path(), &["rebase", "-q", "main"]);
    let h2 = rig.head();
    carry_read(&rig.file(), &rig.main, 7, &h1, &h2).unwrap();
    rig.squash();
    // From H1, main's own edit of a/x would fog a again; from H2 nothing changed since the read.
    assert!(rig.fog().is_empty(), "{:?}", rig.fog());
}
