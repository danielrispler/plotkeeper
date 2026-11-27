use super::*;
use crate::fog::{Strength, effective, refresh};
use crate::map::read_user;
use studio_test_support::{git as run, repo};

const MAP: &str = r#"{"version":1,
  "parts":[
    {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
    {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}],
  "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":[],"laws":[]}"#;

struct Rig {
    dir: tempfile::TempDir,
    main: String,
    state: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        Rig::with(MAP, &[("a/x", "1"), ("b/x", "1")])
    }
    fn with(map: &str, files: &[(&str, &str)]) -> Rig {
        let (dir, main) = repo();
        let rig = Rig {
            dir,
            main,
            state: tempfile::tempdir().unwrap(),
        };
        let mut all = vec![("system-map.json", map)];
        all.extend_from_slice(files);
        rig.commit("Start the map", &all);
        refresh(Path::new(&rig.main), &rig.file(), &rig.main, true).unwrap();
        rig
    }
    fn file(&self) -> std::path::PathBuf {
        self.state.path().join("map-state.json")
    }
    fn commit(&self, subject: &str, files: &[(&str, &str)]) {
        for (name, text) in files {
            let path = self.dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        run(self.dir.path(), &["add", "-A"]);
        run(self.dir.path(), &["commit", "-q", "-m", subject]);
    }
    /// The part's fog entries after a walk.
    fn fog(&self, part: &str) -> Vec<Entry> {
        refresh(Path::new(&self.main), &self.file(), &self.main, false).unwrap();
        read_user(&self.file(), &self.main)
            .fog
            .parts
            .get(part)
            .cloned()
            .unwrap_or_default()
    }
    fn offer(&self, part: &str, lit_of: &dyn Fn(u64) -> Option<Vec<Vec<String>>>) -> Offer {
        offer(Path::new(&self.main), &self.fog(part), part, lit_of)
    }
}

const CATCH_UP: Offer = Offer::CatchUp {
    ready: false,
    writing: false,
    failed: false,
    stage: None,
    outline: None,
    older: None,
};
fn lit(chapters: &[&[&str]]) -> Option<Vec<Vec<String>>> {
    Some(
        chapters
            .iter()
            .map(|c| c.iter().map(ToString::to_string).collect())
            .collect(),
    )
}

#[test]
fn a_part_with_two_fog_entries_gets_a_catch_up_even_when_each_pr_has_a_brief() {
    let rig = Rig::new();
    rig.commit("First (#7)", &[("a/x", "2")]);
    rig.commit("Second (#8)", &[("a/x", "3")]);
    assert_eq!(rig.offer("a", &|_| lit(&[&["a"]])), CATCH_UP);
}

#[test]
fn a_direct_push_gets_a_catch_up() {
    let rig = Rig::new();
    rig.commit("fix pushed to main", &[("a/x", "2")]);
    assert_eq!(rig.offer("a", &|_| lit(&[&["a"]])), CATCH_UP);
}

#[test]
fn a_pr_whose_brief_is_missing_or_lights_no_chapter_on_the_part_gets_a_catch_up() {
    let rig = Rig::new();
    rig.commit("Thing (#7)", &[("a/x", "2"), ("b/x", "2")]);
    assert_eq!(rig.offer("a", &|_| None), CATCH_UP);
    assert_eq!(rig.offer("a", &|_| lit(&[&["b"]])), CATCH_UP);
}

#[test]
fn a_part_fogged_by_one_pr_whose_brief_lights_it_opens_that_chapter() {
    let rig = Rig::new();
    rig.commit("Thing (#7)", &[("a/x", "2"), ("b/x", "2")]);
    let asked = std::cell::Cell::new(0);
    let lit_of = |pr: u64| {
        asked.set(pr);
        lit(&[&["b"], &["a", "b"]])
    };
    assert_eq!(
        rig.offer("a", &lit_of),
        Offer::Chapter { pr: 7, chapter: 1 }
    );
    assert_eq!(asked.get(), 7);
    assert_eq!(
        rig.offer("b", &lit_of),
        Offer::Chapter { pr: 7, chapter: 0 }
    );
    // The map shows both parts fogged meanwhile.
    let user = read_user(&rig.file(), &rig.main);
    assert_eq!(effective(&user, MAP).get("a"), Some(&Strength::Thin));
}

#[test]
fn completing_a_catch_up_clears_the_parts_fog_and_completes_no_brief_and_the_header_dot_goes_out() {
    let rig = Rig::new();
    rig.commit("Thing (#7)", &[("a/x", "2"), ("b/x", "2")]);
    rig.commit("fix pushed to main", &[("a/x", "3")]);
    let covered: Vec<String> = rig.fog("a").into_iter().map(|e| e.commit).collect();
    assert_eq!(covered.len(), 2);
    let user = read_user(&rig.file(), &rig.main);
    assert!(crate::fog::core_fogged(&user, MAP));

    complete(&rig.file(), &rig.main, "a", &covered).unwrap();
    assert!(rig.fog("a").is_empty());
    let user = read_user(&rig.file(), &rig.main);
    assert!(
        !crate::fog::fully_read(&user, 7),
        "the PR's brief is not completed"
    );
    assert!(user.fog.reads.is_empty());
    assert!(crate::fog::core_fogged(&user, MAP), "b is still fogged");

    let b: Vec<String> = rig.fog("b").into_iter().map(|e| e.commit).collect();
    complete(&rig.file(), &rig.main, "b", &b).unwrap();
    let user = read_user(&rig.file(), &rig.main);
    assert!(
        !crate::fog::core_fogged(&user, MAP),
        "no Core part stays fogged"
    );
}

#[test]
fn an_entry_that_landed_after_the_catch_up_was_written_keeps_its_fog() {
    let rig = Rig::new();
    rig.commit("fix pushed to main", &[("a/x", "2")]);
    let covered: Vec<String> = rig.fog("a").into_iter().map(|e| e.commit).collect();
    rig.commit("another push", &[("a/x", "3")]);
    assert_eq!(rig.fog("a").len(), 2);
    complete(&rig.file(), &rig.main, "a", &covered).unwrap();
    assert_eq!(rig.fog("a").len(), 1);
}

#[test]
fn a_stored_catch_up_is_read_as_it_is_only_for_the_fog_list_it_covers() {
    let data = tempfile::tempdir().unwrap();
    let store = CatchUpStore::open(data.path()).unwrap();
    let fog = |c: &[&str]| c.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert!(fresh(&store, "a", &fog(&["c1"])).is_none());
    let stored = StoredCatchUp {
        part: "a".into(),
        fog: fog(&["c1", "c2"]),
        ..StoredCatchUp::default()
    };
    store.put(&stored).unwrap();
    assert_eq!(fresh(&store, "a", &fog(&["c1", "c2"])), Some(stored));
    assert!(
        fresh(&store, "a", &fog(&["c1", "c2", "c3"])).is_none(),
        "a new entry"
    );
    assert!(
        fresh(&store, "a", &fog(&["c2"])).is_none(),
        "part of it cleared"
    );
    assert!(fresh(&store, "b", &fog(&["c1", "c2"])).is_none());
}

#[test]
fn a_ready_offer_carries_the_catch_ups_title_and_step_headlines_and_never_its_gist() {
    use studio_understanding::pr_brief::{Brief, Chapter, Step};
    let data = tempfile::tempdir().unwrap();
    let store = CatchUpStore::open(data.path()).unwrap();
    let fog = vec!["c1".to_string(), "c2".to_string()];
    let step = |k: &str, h: &str| Step {
        k: k.into(),
        h: h.into(),
        b: format!("the body of {k}"),
        ..Step::default()
    };
    store
        .put(&StoredCatchUp {
            part: "a".into(),
            fog: fog.clone(),
            brief: Brief {
                title: "Store writes go atomic".into(),
                chapters: vec![Chapter {
                    title: "the chapter".into(),
                    gist: "THE GIST: writes rename a temp file".into(),
                    steps: vec![
                        step("Where", "The store keeps the state"),
                        step("Before", "Writes went straight in"),
                        step("Now", "Each write renames a temp file"),
                    ],
                    ..Chapter::default()
                }],
                ..Brief::default()
            },
            ..StoredCatchUp::default()
        })
        .unwrap();
    let catch_ups = CatchUps::default();
    let offered = catch_up_offer(Some(&store), &catch_ups, "p", "a", &fog);
    let Offer::CatchUp { ready, outline, .. } = &offered else {
        panic!("a catch-up: {offered:?}");
    };
    assert!(ready);
    let line = |k: &str, h: &str| OutlineStep {
        k: k.into(),
        h: h.into(),
    };
    assert_eq!(
        outline.as_ref(),
        Some(&Outline {
            title: "Store writes go atomic".into(),
            steps: vec![
                line("Where", "The store keeps the state"),
                line("Before", "Writes went straight in"),
                line("Now", "Each write renames a temp file"),
            ],
        })
    );
    let wire = serde_json::to_string(&offered).unwrap();
    assert!(!wire.contains("GIST"), "no gist on the wire: {wire}");
    assert!(!wire.contains("the body of"), "no step body: {wire}");

    let stale = catch_up_offer(Some(&store), &catch_ups, "p", "a", &fog[..1]);
    assert!(
        matches!(
            stale,
            Offer::CatchUp {
                ready: false,
                outline: None,
                ..
            }
        ),
        "a stale one offers no outline: {stale:?}"
    );
}

/// A stored catch-up of part `a` over `fog`, titled `title`, of one step.
fn put_catch_up(store: &CatchUpStore, fog: &[&str], title: &str) {
    use studio_understanding::pr_brief::{Brief, Chapter, Step};
    store
        .put(&StoredCatchUp {
            part: "a".into(),
            fog: fog.iter().map(ToString::to_string).collect(),
            brief: Brief {
                title: title.into(),
                chapters: vec![Chapter {
                    gist: "THE GIST".into(),
                    steps: vec![Step {
                        k: "Now".into(),
                        h: "Each write renames a temp file".into(),
                        ..Step::default()
                    }],
                    ..Chapter::default()
                }],
                ..Brief::default()
            },
            ..StoredCatchUp::default()
        })
        .unwrap();
}

fn list(c: &[&str]) -> Vec<String> {
    c.iter().map(ToString::to_string).collect()
}

#[test]
fn a_part_whose_stored_catch_up_covers_entries_all_still_fogged_offers_it_as_older_and_one_with_a_cleared_entry_does_not()
 {
    let data = tempfile::tempdir().unwrap();
    let store = CatchUpStore::open(data.path()).unwrap();
    put_catch_up(&store, &["c1", "c2"], "The older story");
    let catch_ups = CatchUps::default();
    let grown = catch_up_offer(
        Some(&store),
        &catch_ups,
        "p",
        "a",
        &list(&["c1", "c2", "c3"]),
    );
    let Offer::CatchUp {
        ready,
        outline,
        older,
        ..
    } = &grown
    else {
        panic!("a catch-up: {grown:?}");
    };
    assert!(!ready);
    assert_eq!(outline, &None);
    assert_eq!(
        older.as_ref(),
        Some(&Older {
            outline: Outline {
                title: "The older story".into(),
                steps: vec![OutlineStep {
                    k: "Now".into(),
                    h: "Each write renames a temp file".into(),
                }],
            },
            fog: list(&["c1", "c2"]),
        })
    );
    let wire = serde_json::to_string(&grown).unwrap();
    assert!(!wire.contains("GIST"), "no gist on the wire: {wire}");

    // `c1` was cleared since: the stored one tells of a change Daniel already caught up on.
    let cleared = catch_up_offer(Some(&store), &catch_ups, "p", "a", &list(&["c2", "c3"]));
    assert!(
        matches!(cleared, Offer::CatchUp { older: None, .. }),
        "{cleared:?}"
    );
    // A fresh one is ready, not older.
    let fresh = catch_up_offer(Some(&store), &catch_ups, "p", "a", &list(&["c1", "c2"]));
    assert!(
        matches!(
            fresh,
            Offer::CatchUp {
                ready: true,
                older: None,
                ..
            }
        ),
        "{fresh:?}"
    );
}

// ---- written when the map shows ----

const FIVE: &str = r#"{"version":1,
  "parts":[
    {"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
    {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]},
    {"id":"c","name":"C","caption":"c","description":"d","tier":"core","paths":["c/"]},
    {"id":"d","name":"D","caption":"c","description":"d","tier":"core","paths":["d/"]},
    {"id":"e","name":"E","caption":"c","description":"d","tier":"core","paths":["e/"]},
    {"id":"f","name":"F","caption":"c","description":"d","tier":"interface","paths":["f/"]}],
  "actors":[],"links":[],"ignored":[],"laws":[]}"#;

/// A fake writer: a catch-up of 3 steps on the part its prompt names, an empty claim check; it counts the catch-ups it wrote
/// per part, and waits while its gate is shut.
#[derive(Default)]
struct Fake {
    wrote: Mutex<Vec<String>>,
    gate: (Mutex<bool>, std::sync::Condvar),
    /// Past its gate, the writer panics instead of writing.
    panics: bool,
    /// The claim check finds the first step unsupported, so a retry runs.
    flags: bool,
    /// Read as each call starts: the stage the part's state carries then.
    probe: Option<Arc<Mutex<Stages>>>,
    seen: Mutex<Vec<Option<Stage>>>,
}

impl Fake {
    fn shut() -> Arc<Fake> {
        let fake = Arc::new(Fake::default());
        *fake.gate.0.lock().unwrap() = true;
        fake
    }
    fn open(&self) {
        *self.gate.0.lock().unwrap() = false;
        self.gate.1.notify_all();
    }
    fn wrote(&self) -> Vec<String> {
        self.wrote.lock().unwrap().clone()
    }
}

impl BriefWriter for Fake {
    fn write(
        &self,
        _model: &str,
        prompt: &str,
    ) -> Result<studio_understanding::pr_brief::BriefReply, String> {
        if let Some(stages) = &self.probe {
            let stage = stages.lock().unwrap().values().next().cloned();
            self.seen.lock().unwrap().push(stage);
        }
        let text = if prompt.contains("You write a part catch-up") && !prompt.contains("RETRY") {
            let mut shut = self.gate.0.lock().unwrap();
            while *shut {
                shut = self.gate.1.wait(shut).unwrap();
            }
            drop(shut);
            assert!(!self.panics, "the writer broke");
            let part = prompt
                .split("steps about part ")
                .nth(1)
                .and_then(|rest| rest.split(',').next())
                .unwrap_or_default()
                .to_string();
            self.wrote.lock().unwrap().push(part.clone());
            let step = |k: &str| serde_json::json!({ "k": k, "h": "h", "b": "b", "on": [part], "cites": [] });
            serde_json::json!({ "title": "t", "chapters": [ { "behaviour": "story", "parts": [part], "title": "t",
                "steps": [step("Where"), step("Before"), step("Now")] } ], "also": [], "unseen": [], "decisions": [] })
            .to_string()
        } else if self.flags && !prompt.contains("RETRY") {
            r#"{"items":[{"id":"step 1","supported":false,"phrase":"h","why":"the facts say nothing"}]}"#
                .to_string()
        } else {
            r#"{"items":[]}"#.to_string()
        };
        Ok(studio_understanding::pr_brief::BriefReply {
            text,
            usage: Default::default(),
        })
    }
}

struct Shows {
    map: &'static str,
    rig: Rig,
    data: tempfile::TempDir,
    catch_ups: CatchUps,
    ended: std::sync::mpsc::Sender<()>,
    ends: std::sync::mpsc::Receiver<()>,
    /// How many times a turn's end read CI again, opened the catch-up store, and opened the PR brief store.
    ci_reads: std::cell::Cell<usize>,
    stores_opened: std::cell::Cell<usize>,
    briefs_opened: std::cell::Cell<usize>,
}

impl Shows {
    fn new() -> Shows {
        Shows::over(FIVE)
    }
    /// Over `map`, each of whose parts owns the folder named by its id.
    fn over(map: &'static str) -> Shows {
        let parsed: serde_json::Value = serde_json::from_str(map).unwrap();
        let files: Vec<(String, &str)> = parsed["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (format!("{}/x", p["id"].as_str().unwrap()), "1"))
            .collect();
        let files: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), *t)).collect();
        let (ended, ends) = std::sync::mpsc::channel();
        Shows {
            map,
            rig: Rig::with(map, &files),
            data: tempfile::tempdir().unwrap(),
            catch_ups: CatchUps::default(),
            ended,
            ends,
            ci_reads: std::cell::Cell::new(0),
            stores_opened: std::cell::Cell::new(0),
            briefs_opened: std::cell::Cell::new(0),
        }
    }
    fn push(&self, part: &str, text: &str) {
        self.rig
            .commit(&format!("push to {part}"), &[(&format!("{part}/x"), text)]);
    }
    fn store(&self) -> CatchUpStore {
        CatchUpStore::open(self.data.path()).unwrap()
    }
    /// The map is shown: fog is refreshed, then the writes start.
    fn show(&self, writer: &Arc<Fake>) -> Vec<String> {
        refresh(
            Path::new(&self.rig.main),
            &self.rig.file(),
            &self.rig.main,
            true,
        )
        .unwrap();
        let user = read_user(&self.rig.file(), &self.rig.main);
        let ended = self.ended.clone();
        let writer: SharedWriter = writer.clone();
        show(
            &self.catch_ups,
            &writer,
            "opus",
            &self.rig.main,
            &user,
            self.map,
            &|_| None,
            &self.store(),
            move || ended.send(()).unwrap(),
        )
    }
    fn wait(&self, n: usize) {
        for _ in 0..n {
            self.ends
                .recv_timeout(std::time::Duration::from_secs(30))
                .expect("a write ends");
        }
    }
    fn fog(&self, part: &str) -> Vec<String> {
        commits(&self.rig.fog(part))
    }
}

#[test]
fn a_part_that_is_not_core_is_written_only_when_clicked() {
    let shows = Shows::new();
    shows.push("f", "2");
    shows.push("a", "2");
    let writer = Arc::new(Fake::default());
    assert_eq!(shows.show(&writer), ["a"]);
    shows.wait(1);
    assert!(shows.store().get("f").is_none());

    // The interface part is written when clicked.
    let ended = shows.ended.clone();
    let writer_: SharedWriter = writer.clone();
    let opened = ask(
        &shows.catch_ups,
        Some(writer_),
        "opus".into(),
        &shows.rig.main,
        "f",
        shows.fog("f"),
        false,
        shows.store(),
        move || ended.send(()).unwrap(),
    )
    .unwrap();
    assert!(matches!(opened, Opened::Writing { writing: true }));
    shows.wait(1);
    assert!(fresh(&shows.store(), "f", &shows.fog("f")).is_some());
    assert_eq!(writer.wrote(), ["a", "f"]);
}

#[test]
fn reading_the_older_catch_up_while_the_new_one_is_written_clears_only_what_it_covers() {
    let shows = Shows::new();
    shows.push("a", "2");
    let writer = Arc::new(Fake::default());
    assert_eq!(shows.show(&writer), ["a"]);
    shows.wait(1);
    let first = shows.fog("a");
    // A new change lands in `a`: its stored catch-up covers the earlier list only.
    shows.push("a", "3");
    let grown = shows.fog("a");
    assert_eq!(grown.len(), 2);
    let ask_for = |older: bool| {
        let ended = shows.ended.clone();
        let writer_: SharedWriter = Fake::shut();
        ask(
            &shows.catch_ups,
            Some(writer_),
            "opus".into(),
            &shows.rig.main,
            "a",
            shows.fog("a"),
            older,
            shows.store(),
            move || ended.send(()).unwrap(),
        )
        .unwrap()
    };
    let Opened::CatchUp(opened) = ask_for(true) else {
        panic!("the older catch-up opens");
    };
    assert_eq!(opened.fog, first);

    // Read to its end, it clears what it covers; the newer entry stays fogged.
    complete(&shows.rig.file(), &shows.rig.main, "a", &opened.fog).unwrap();
    assert_eq!(shows.fog("a"), grown[1..]);
    // Its first entry is cleared now: asked for again, it is not handed over, and the new one is written as any click.
    assert!(matches!(ask_for(true), Opened::Writing { writing: true }));
}

/// Ten Core parts, `c0` to `c9`, and two interface parts, `i0` and `i1`.
const TWELVE: &str = r#"{"version":1,
  "parts":[
    {"id":"c0","name":"C0","caption":"c","description":"d","tier":"core","paths":["c0/"]},
    {"id":"c1","name":"C1","caption":"c","description":"d","tier":"core","paths":["c1/"]},
    {"id":"c2","name":"C2","caption":"c","description":"d","tier":"core","paths":["c2/"]},
    {"id":"c3","name":"C3","caption":"c","description":"d","tier":"core","paths":["c3/"]},
    {"id":"c4","name":"C4","caption":"c","description":"d","tier":"core","paths":["c4/"]},
    {"id":"c5","name":"C5","caption":"c","description":"d","tier":"core","paths":["c5/"]},
    {"id":"c6","name":"C6","caption":"c","description":"d","tier":"core","paths":["c6/"]},
    {"id":"c7","name":"C7","caption":"c","description":"d","tier":"core","paths":["c7/"]},
    {"id":"c8","name":"C8","caption":"c","description":"d","tier":"core","paths":["c8/"]},
    {"id":"c9","name":"C9","caption":"c","description":"d","tier":"core","paths":["c9/"]},
    {"id":"i0","name":"I0","caption":"c","description":"d","tier":"interface","paths":["i0/"]},
    {"id":"i1","name":"I1","caption":"c","description":"d","tier":"interface","paths":["i1/"]}],
  "actors":[],"links":[],"ignored":[],"laws":[]}"#;

#[test]
fn showing_the_map_writes_eight_fogged_core_parts_oldest_fog_first_none_of_the_others_and_none_again()
 {
    let shows = Shows::over(TWELVE);
    // The interface parts' fog is the oldest; then the Core parts, c7's fog the oldest of them.
    for part in [
        "i0", "i1", "c7", "c2", "c9", "c0", "c5", "c1", "c8", "c3", "c6", "c4",
    ] {
        shows.push(part, "2");
    }
    let writer = Fake::shut();
    let started = shows.show(&writer);
    assert_eq!(started, ["c7", "c2", "c9", "c0", "c5", "c1", "c8", "c3"]);
    // Showing the map again while they are written starts none.
    assert_eq!(shows.show(&writer), Vec::<String>::new());
    writer.open();
    shows.wait(8);
    let mut wrote = writer.wrote();
    wrote.sort();
    assert_eq!(wrote, ["c0", "c1", "c2", "c3", "c5", "c7", "c8", "c9"]);
    // Nor once they are stored.
    assert_eq!(shows.show(&writer), Vec::<String>::new());
    assert_eq!(writer.wrote().len(), 8);
}

#[test]
fn a_new_fog_entry_makes_the_stored_catch_up_stale_and_the_next_show_rewrites_it() {
    let shows = Shows::new();
    shows.push("a", "2");
    let writer = Arc::new(Fake::default());
    assert_eq!(shows.show(&writer), ["a"]);
    shows.wait(1);
    // An unchanged fog list is never written again.
    assert_eq!(shows.show(&writer), Vec::<String>::new());
    assert_eq!(writer.wrote(), ["a"]);

    shows.push("a", "3");
    let list = shows.fog("a");
    assert_eq!(list.len(), 2);
    assert!(fresh(&shows.store(), "a", &list).is_none(), "stale");
    assert_eq!(shows.show(&writer), ["a"]);
    shows.wait(1);
    assert_eq!(writer.wrote(), ["a", "a"]);
    assert_eq!(shows.store().get("a").unwrap().fog, list);
}

#[test]
fn a_click_for_a_longer_fog_list_while_the_shorter_one_is_written_gets_its_write_once_that_one_ends()
 {
    let shows = Shows::new();
    shows.push("a", "2");
    let writer = Fake::shut();
    assert_eq!(shows.show(&writer), ["a"]);
    shows.push("a", "3");
    let list = shows.fog("a");
    assert_eq!(list.len(), 2);
    let writer_: SharedWriter = writer.clone();
    let ended = shows.ended.clone();
    let opened = ask(
        &shows.catch_ups,
        Some(writer_),
        "opus".into(),
        &shows.rig.main,
        "a",
        list.clone(),
        false,
        shows.store(),
        move || ended.send(()).unwrap(),
    )
    .unwrap();
    assert!(matches!(opened, Opened::Writing { writing: true }));
    writer.open();
    shows.wait(1);
    assert_eq!(writer.wrote(), ["a", "a"]);
    assert_eq!(shows.store().get("a").unwrap().fog, list);
    assert!(!shows.catch_ups.writing(&shows.rig.main, "a"));
}

#[test]
fn showing_the_map_twice_in_a_row_starts_one_write_of_each_catch_up() {
    let shows = Shows::new();
    shows.push("a", "2");
    shows.push("b", "2");
    let writer = Fake::shut();
    assert_eq!(shows.show(&writer), ["a", "b"]);
    assert_eq!(shows.show(&writer), Vec::<String>::new());
    writer.open();
    shows.wait(2);
    let mut wrote = writer.wrote();
    wrote.sort();
    assert_eq!(wrote, ["a", "b"]);
    assert_eq!(shows.show(&writer), Vec::<String>::new());
}

#[test]
fn the_catch_ups_state_carries_the_stage_as_each_call_starts() {
    let shows = Shows::new();
    shows.push("a", "2");
    let writer = Arc::new(Fake {
        flags: true,
        probe: Some(shows.catch_ups.stages.clone()),
        ..Default::default()
    });
    assert_eq!(shows.show(&writer), ["a"]);
    shows.wait(1);
    let seen = writer.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 3, "writer, claim check, retry: {seen:?}");
    assert_eq!(seen[0], Some(Stage::Writing));
    assert_eq!(seen[1], Some(Stage::Checking));
    assert!(
        matches!(seen[2], Some(Stage::Fixing { claims }) if claims >= 1),
        "{seen:?}"
    );
    assert_eq!(shows.catch_ups.stage(&shows.rig.main, "a"), None);
}

// A claim that ended keeps its hands off the part: a new write claimed before the old guard drops keeps its stage
// (#443 review).
#[test]
fn an_ended_claim_dropping_late_keeps_the_new_writes_stage() {
    let catch_ups = CatchUps::default();
    let fog = vec!["c1".to_string()];
    let mut old = catch_ups.claim("p", "a", &fog).unwrap();
    assert_eq!(old.next(), None);
    let _new = catch_ups.claim("p", "a", &fog).unwrap();
    let key = ("p".to_string(), "a".to_string());
    catch_ups.stages.lock().unwrap().insert(key, Stage::Writing);
    drop(old);
    assert_eq!(catch_ups.stage("p", "a"), Some(Stage::Writing));
    assert!(catch_ups.writing("p", "a"));
}

#[test]
fn a_write_that_panics_tells_the_page_and_marks_the_newest_fog_list_failed() {
    let shows = Shows::new();
    shows.push("a", "2");
    let writer = Arc::new(Fake {
        panics: true,
        ..Default::default()
    });
    *writer.gate.0.lock().unwrap() = true;
    assert_eq!(shows.show(&writer), ["a"]);
    // A longer list is asked for while the first is written: it waits its turn.
    shows.push("a", "3");
    let list = shows.fog("a");
    let writer_: SharedWriter = writer.clone();
    let ended = shows.ended.clone();
    ask(
        &shows.catch_ups,
        Some(writer_),
        "opus".into(),
        &shows.rig.main,
        "a",
        list.clone(),
        false,
        shows.store(),
        move || ended.send(()).unwrap(),
    )
    .unwrap();
    writer.open();
    // The page hears the write ended, and the sheet offers a retry of the newest list.
    shows.wait(1);
    assert!(!shows.catch_ups.writing(&shows.rig.main, "a"));
    assert!(shows.catch_ups.failed(&shows.rig.main, "a", &list));
}

// ---- written at a turn's end ----

impl Shows {
    /// Gives `part` a thick change: its paths in the map gain a folder.
    fn thicken(&self, part: &str) {
        let path = self.rig.dir.path().join("system-map.json");
        let map = std::fs::read_to_string(&path).unwrap();
        let from = format!(r#""paths":["{part}/"]"#);
        assert!(map.contains(&from), "{part} has one folder");
        let to = format!(r#""paths":["{part}/","{part}-more/"]"#);
        self.rig.commit(
            &format!("widen {part}"),
            &[("system-map.json", &map.replace(&from, &to))],
        );
    }
    /// A session's turn ended: the fetch runs through `fetches`, counted in `fetched`.
    fn turn(
        &self,
        writer: &Arc<Fake>,
        fetches: &crate::map::Fetches,
        fetched: &std::cell::Cell<usize>,
    ) -> Option<Vec<String>> {
        self.turn_lit(writer, fetches, fetched, &|_| None)
    }
    /// As `turn`, with what each PR's stored brief lights.
    fn turn_lit(
        &self,
        writer: &Arc<Fake>,
        fetches: &crate::map::Fetches,
        fetched: &std::cell::Cell<usize>,
        lit_of: &dyn Fn(u64) -> Option<Vec<Vec<String>>>,
    ) -> Option<Vec<String>> {
        let ended = self.ended.clone();
        let writer: SharedWriter = writer.clone();
        turn_end(
            &self.catch_ups,
            Some(&writer),
            "opus",
            fetches,
            || {
                fetched.set(fetched.get() + 1);
                Ok(())
            },
            || self.ci_reads.set(self.ci_reads.get() + 1),
            &self.rig.main,
            &self.rig.file(),
            || {
                self.briefs_opened.set(self.briefs_opened.get() + 1);
                lit_of
            },
            || {
                self.stores_opened.set(self.stores_opened.get() + 1);
                Some(self.store())
            },
            move || ended.send(()).unwrap(),
        )
    }
}

#[test]
fn a_turns_end_writes_the_fogged_core_and_interface_parts_with_no_map_shown() {
    let shows = Shows::new();
    shows.push("a", "2");
    shows.thicken("f");
    let writer = Arc::new(Fake::default());
    let fetched = std::cell::Cell::new(0);
    let started = shows.turn(&writer, &crate::map::Fetches::default(), &fetched);
    assert_eq!(started, Some(vec!["a".to_string(), "f".to_string()]));
    shows.wait(2);
    assert!(fresh(&shows.store(), "a", &shows.fog("a")).is_some());
    assert!(fresh(&shows.store(), "f", &shows.fog("f")).is_some());
    assert_eq!(fetched.get(), 1);
}

#[test]
fn a_turns_end_in_a_project_whose_map_was_never_opened_fetches_and_writes_nothing() {
    let shows = Shows::new();
    // The map was never opened: no fog started.
    std::fs::remove_file(shows.rig.file()).unwrap();
    shows.push("a", "2");
    shows.thicken("f");
    let writer = Arc::new(Fake::default());
    let fetched = std::cell::Cell::new(0);
    let started = shows.turn(&writer, &crate::map::Fetches::default(), &fetched);
    assert_eq!(started, None);
    assert_eq!(fetched.get(), 0);
    let user = read_user(&shows.rig.file(), &shows.rig.main);
    assert_eq!(user.fog.processed, None, "the fog stays unstarted");
    assert!(writer.wrote().is_empty());
    // No data folder is made for it, neither the catch-ups' nor the PR briefs', and CI is not read.
    assert_eq!(shows.stores_opened.get(), 0);
    assert_eq!(shows.briefs_opened.get(), 0);
    assert_eq!(shows.ci_reads.get(), 0);
}

#[test]
fn a_turns_end_writes_eight_fogged_core_and_interface_parts_oldest_fog_first_and_none_again() {
    let shows = Shows::over(TWELVE);
    let fogged = [
        "c7", "i0", "c2", "i1", "c9", "c0", "c5", "c1", "c8", "c3", "c6", "c4",
    ];
    for part in fogged {
        match part {
            "i0" | "i1" => shows.thicken(part),
            _ => shows.push(part, "2"),
        }
    }
    let writer = Fake::shut();
    let (fetches, fetched) = (crate::map::Fetches::default(), std::cell::Cell::new(0));
    let started = shows.turn(&writer, &fetches, &fetched).unwrap();
    assert_eq!(started, ["c7", "i0", "c2", "i1", "c9", "c0", "c5", "c1"]);
    // The project header's dot still counts Core parts only.
    let user = read_user(&shows.rig.file(), &shows.rig.main);
    let core = crate::fog::core_fogged_parts(&user, TWELVE);
    assert!(core.iter().all(|p| p.starts_with('c')), "{core:?}");
    // Another turn's end while they are written starts none.
    assert_eq!(shows.turn(&writer, &fetches, &fetched), Some(vec![]));
    writer.open();
    shows.wait(8);
    assert_eq!(writer.wrote().len(), 8);
    // Nor once they are stored.
    assert_eq!(shows.turn(&writer, &fetches, &fetched), Some(vec![]));
    assert_eq!(writer.wrote().len(), 8);
}

#[test]
fn a_turns_end_skips_a_part_that_opens_its_one_prs_chapter_and_one_whose_write_failed() {
    let shows = Shows::new();
    shows.rig.commit("Thing (#7)", &[("a/x", "2")]);
    shows.thicken("f");
    let writer = Arc::new(Fake {
        panics: true,
        ..Default::default()
    });
    let (fetches, fetched) = (crate::map::Fetches::default(), std::cell::Cell::new(0));
    let lights_a = |pr: u64| (pr == 7).then(|| vec![vec!["a".to_string()]]);
    let started = shows.turn_lit(&writer, &fetches, &fetched, &lights_a);
    assert_eq!(started, Some(vec!["f".to_string()]));
    shows.wait(1);
    assert!(
        shows
            .catch_ups
            .failed(&shows.rig.main, "f", &shows.fog("f"))
    );
    // The failed write waits for a click.
    let started = shows.turn_lit(&writer, &fetches, &fetched, &lights_a);
    assert_eq!(started, Some(vec![]));
}

#[test]
fn many_turn_ends_within_the_cooldown_fetch_once() {
    let shows = Shows::new();
    let writer = Arc::new(Fake::default());
    let (fetches, fetched) = (crate::map::Fetches::default(), std::cell::Cell::new(0));
    for _ in 0..5 {
        assert!(shows.turn(&writer, &fetches, &fetched).is_some());
    }
    assert_eq!(fetched.get(), 1);
    // The fetch that ran reads CI again, as the map's open does, since its cooldown now skips the map's own fetch.
    assert_eq!(shows.ci_reads.get(), 1);
}
