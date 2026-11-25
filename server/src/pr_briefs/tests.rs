use super::*;
use crate::prs::Ready;
use std::cell::{Cell, RefCell};
use std::{collections::BTreeSet, path::Path, sync::Mutex};
use studio_understanding::pr_brief::{BriefReply, CallUsage, ImportSite, LinkEvidence, PrMeta};
use studio_understanding::pr_brief::{BriefStore, BriefWriter, PrSource, StoredBrief};

const MAP: &str = r#"{ "version": 1,
  "parts": [{ "id": "alpha", "name": "Alpha", "caption": "c", "description": "d", "tier": "core", "paths": ["alpha/"] }],
  "actors": [], "links": [], "ignored": [], "laws": [], "flows": [] }"#;

fn source(head: &str) -> PrSource {
    PrSource {
        meta: PrMeta {
            number: 7,
            title: "Make the thing".into(),
            head: head.into(),
            ..PrMeta::default()
        },
        map_text: MAP.into(),
        diff: "diff --git a/alpha/a.rs b/alpha/a.rs\nindex 1..2 100644\n--- a/alpha/a.rs\n+++ b/alpha/a.rs\n@@ -1,1 +1,2 @@\n+one\n".into(),
        ..PrSource::default()
    }
}

/// The map of the stale-base tests: `alpha` is lit; the link `ab` is at it, `bg` is not; `f1` runs through `ab`, `f2` only
/// through `bg`; `law-ab` sits on the link at it.
fn stale_map() -> serde_json::Value {
    serde_json::json!({ "version": 2,
      "parts": [
        { "id": "alpha", "name": "Alpha", "caption": "c", "purpose": "p", "holds": [], "tier": "core", "paths": ["alpha/"] },
        { "id": "beta", "name": "Beta", "caption": "c", "purpose": "p", "holds": [], "tier": "core", "paths": ["beta/"] },
        { "id": "gamma", "name": "Gamma", "caption": "c", "purpose": "p", "holds": [], "tier": "core", "paths": ["gamma/"] }],
      "actors": [],
      "links": [
        { "id": "ab", "from": "alpha", "to": "beta", "kind": "import", "reason": "Alpha calls beta." },
        { "id": "bg", "from": "beta", "to": "gamma", "kind": "import", "reason": "Beta calls gamma." }],
      "ignored": ["tools/"],
      "laws": [
        { "id": "law-a", "on": "alpha", "backing": "trust", "text": "Alpha keeps order." },
        { "id": "law-ab", "on": "ab", "backing": "trust", "text": "The wire agrees." },
        { "id": "law-g", "on": "gamma", "backing": "trust", "text": "Gamma keeps order." }],
      "flows": [
        { "id": "f1", "name": "One", "hops": [{ "link": "ab", "text": "Alpha asks beta." }] },
        { "id": "f2", "name": "Two", "hops": [{ "link": "bg", "text": "Beta asks gamma." }] }] })
}

fn put(dir: &Path, path: &str, text: &str) {
    let file = dir.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

/// A repository whose PR changed `alpha` off the merge base, and whose `main` then wrote `files` and the map `change` made: its
/// folder, and the PR's head.
fn moved_main(
    files: &[&str],
    change: impl FnOnce(&mut serde_json::Value),
) -> (tempfile::TempDir, String) {
    use studio_test_support::{git, repo};
    let (dir, _) = repo();
    let at = dir.path();
    let mut map = stale_map();
    put(at, "system-map.json", &map.to_string());
    for path in [
        "alpha/a.rs",
        "beta/b.rs",
        "beta/package.json",
        "gamma/g.rs",
        "tools/Cargo.toml",
        "Cargo.toml",
        "Cargo.lock",
        "package.json",
        "package-lock.json",
        "rust-toolchain.toml",
        "tsconfig.json",
    ] {
        put(at, path, "one\n");
    }
    git(at, &["add", "-A"]);
    git(at, &["commit", "-q", "-m", "base"]);
    git(at, &["checkout", "-q", "-b", "pr"]);
    put(at, "alpha/a.rs", "the PR\n");
    git(at, &["commit", "-q", "-am", "the PR"]);
    let head = git(at, &["rev-parse", "HEAD"]);
    git(at, &["checkout", "-q", "main"]);
    for path in files {
        put(at, path, "main moved\n");
    }
    change(&mut map);
    put(at, "system-map.json", &map.to_string());
    git(at, &["add", "-A"]);
    git(at, &["commit", "-q", "--allow-empty", "-m", "main moved"]);
    (dir, head)
}

fn stale_of(files: &[&str], change: impl FnOnce(&mut serde_json::Value)) -> Option<StaleBase> {
    let (dir, head) = moved_main(files, change);
    stale_base(dir.path(), &head, &BTreeSet::from(["alpha".to_string()]))
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

#[test]
fn main_changing_a_file_of_a_lit_part_names_the_file() {
    assert_eq!(
        stale_of(&["alpha/new.rs", "beta/b.rs"], |_| {}),
        Some(StaleBase {
            files: names(&["alpha/new.rs"]),
            ..StaleBase::default()
        })
    );
}

#[test]
fn main_changing_the_map_at_a_lit_part_names_each_law_link_flow_and_paths_that_moved() {
    let law = stale_of(&[], |m| {
        m["laws"][0]["text"] = "Alpha keeps a new order.".into();
    });
    assert_eq!(law.map(|s| s.laws), Some(names(&["law-a"])));
    // A law on a link at the lit part is the brief's reading, not fog's.
    let on_link = stale_of(&[], |m| {
        m["laws"][1]["text"] = "The wire agrees again.".into();
    });
    assert_eq!(on_link.map(|s| s.laws), Some(names(&["law-ab"])));
    let link = stale_of(&[], |m| {
        m["links"][0]["reason"] = "Alpha now asks beta.".into();
    });
    assert_eq!(
        link,
        Some(StaleBase {
            links: names(&["ab"]),
            ..StaleBase::default()
        })
    );
    let flow = stale_of(&[], |m| {
        m["flows"][0]["hops"][0]["text"] = "Alpha tells beta.".into();
    });
    assert_eq!(flow.map(|s| s.flows), Some(names(&["f1"])));
    let paths = stale_of(&[], |m| {
        m["parts"][0]["paths"] = serde_json::json!(["alpha/", "extra/"]);
    });
    assert_eq!(paths.map(|s| s.paths), Some(names(&["alpha"])));
    // A law added on the lit part moved too.
    let added = stale_of(&[], |m| {
        m["laws"].as_array_mut().unwrap().push(
            serde_json::json!({ "id": "law-new", "on": "alpha", "backing": "trust", "text": "New." }),
        );
    });
    assert_eq!(added.map(|s| s.laws), Some(names(&["law-new"])));
}

#[test]
fn main_changing_a_root_build_manifest_names_it() {
    assert_eq!(
        stale_of(
            &[
                "Cargo.toml",
                "package.json",
                "rust-toolchain.toml",
                "tsconfig.json"
            ],
            |_| {}
        ),
        Some(StaleBase {
            files: names(&[
                "Cargo.toml",
                "package.json",
                "rust-toolchain.toml",
                "tsconfig.json"
            ]),
            ..StaleBase::default()
        })
    );
}

#[test]
fn lockfiles_and_manifests_below_the_root_are_no_stale_base() {
    assert_eq!(
        stale_of(
            &[
                "Cargo.lock",
                "package-lock.json",
                "tools/Cargo.toml",
                "beta/package.json"
            ],
            |_| {}
        ),
        None
    );
}

#[test]
fn main_moving_only_elsewhere_is_no_stale_base() {
    let elsewhere = stale_of(&["beta/b.rs", "gamma/g.rs"], |m| {
        m["laws"][2]["text"] = "Gamma keeps a new order.".into();
        m["links"][1]["reason"] = "Beta now asks gamma.".into();
        m["flows"][1]["hops"][0]["text"] = "Beta tells gamma.".into();
        m["parts"][1]["paths"] = serde_json::json!(["beta/", "more/"]);
    });
    assert_eq!(elsewhere, None);
}

#[test]
fn a_merged_prs_brief_gets_no_stale_base_and_an_open_ones_does() {
    let (dir, head) = moved_main(&["alpha/new.rs"], |_| {});
    let data = tempfile::tempdir().unwrap();
    let store = BriefStore::open(data.path()).unwrap();
    let mut stored = StoredBrief {
        pr: 7,
        head,
        ..StoredBrief::default()
    };
    stored.brief.chapters = vec![studio_understanding::pr_brief::Chapter {
        steps: vec![studio_understanding::pr_brief::Step {
            on: vec!["alpha".into()],
            ..Default::default()
        }],
        ..Default::default()
    }];
    let project = dir.path().display().to_string();
    assert_eq!(
        view_of(&project, &store, stored.clone(), false).stale_base,
        None
    );
    assert_eq!(
        view_of(&project, &store, stored, true)
            .stale_base
            .map(|s| s.files),
        Some(names(&["alpha/new.rs"]))
    );
}

struct Fake(Cell<usize>, RefCell<Vec<String>>);
impl Fake {
    fn new() -> Self {
        Fake(Cell::new(0), RefCell::new(vec![]))
    }
}
impl BriefWriter for Fake {
    fn write(&self, model: &str, _prompt: &str) -> Result<BriefReply, String> {
        self.0.set(self.0.get() + 1);
        self.1.borrow_mut().push(model.into());
        Ok(BriefReply {
            text: r#"{"title":"Alpha","chapters":[{"behaviour":"story","parts":["alpha"],"title":"Order","steps":[{"k":"Where","h":"Alpha","b":"Alpha is here.","on":["alpha"]}]}]}"#.into(),
            usage: CallUsage::default(),
        })
    }
}

fn lookup(store: &BriefStore, seen: &Mutex<Seen>, fake: &Fake, head: &str) -> Did {
    let with = Writing {
        writer: fake,
        model: "opus",
        store,
        reading: None,
    };
    on_lookup(&with, seen, 7, head, || Ok(source(head)), |_| {})
}

#[test]
fn a_ready_pr_gets_a_stored_brief_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    assert_eq!(lookup(&store, &seen, &fake, "aaa"), Did::Wrote);
    let stored = store.get(7, "aaa").expect("the brief is stored");
    assert_eq!(stored.brief.title, "Alpha");
    assert_eq!(fake.1.borrow()[0], "opus");
    // Another lookup of the same head writes nothing.
    assert_eq!(lookup(&store, &seen, &fake, "aaa"), Did::Kept);
    assert_eq!(store.records().len(), 1);
}

#[test]
fn a_new_head_marks_the_brief_stale_and_a_second_lookup_of_it_rewrites() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    lookup(&store, &seen, &fake, "aaa");
    assert_eq!(lookup(&store, &seen, &fake, "bbb"), Did::Staled);
    assert!(store.get(7, "aaa").unwrap().stale);
    assert!(store.get(7, "bbb").is_none(), "not rewritten at once");
    // A burst of pushes: the head moved again, so the count starts over.
    assert_eq!(lookup(&store, &seen, &fake, "ccc"), Did::Staled);
    assert_eq!(store.records().len(), 1);
    // The same head at two lookups in a row: rewritten.
    assert_eq!(lookup(&store, &seen, &fake, "ccc"), Did::Wrote);
    assert!(store.get(7, "ccc").is_some());
    assert_eq!(store.records().len(), 2);
}

#[test]
fn opening_a_stale_brief_rewrites_it_and_a_fresh_one_is_read_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    lookup(&store, &seen, &fake, "aaa");
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let open = |head: &str| on_open(&with, &seen, 7, head, || Ok(source(head)), |_| {});
    assert_eq!(open("aaa").unwrap().head, "aaa");
    assert_eq!(store.records().len(), 1, "a fresh brief is not rewritten");
    lookup(&store, &seen, &fake, "bbb");
    assert_eq!(open("bbb").unwrap().head, "bbb");
    assert_eq!(store.records().len(), 2);
    assert!(store.get(7, "bbb").is_some());
}

fn quiet_source(map: &str, tiers: &[(&str, &str)], map_diff: &str) -> PrSource {
    let mut source = source("aaa");
    source.map_text = map.into();
    source.map_diff = map_diff.into();
    for (part, tier) in tiers {
        source
            .tier_overrides
            .insert(part.to_string(), tier.to_string());
    }
    source
}

#[test]
fn a_pr_whose_touched_parts_would_not_fog_at_the_tiers_gets_no_brief() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let black = MAP.replace("\"core\"", "\"black-box\"");
    let interface = MAP.replace("\"core\"", "\"interface\"");
    let cases = [
        // The map's own tier is black box.
        (quiet_source(&black, &[], ""), Did::Quiet),
        // Daniel's tier wins over the map's Core.
        (quiet_source(MAP, &[("alpha", "black-box")], ""), Did::Quiet),
        // An Interface part fogs only when the map's diff names it.
        (quiet_source(&interface, &[], ""), Did::Quiet),
        (quiet_source(&interface, &[], "\"alpha\""), Did::Wrote),
        (quiet_source(MAP, &[], ""), Did::Wrote),
    ];
    for (n, (source, want)) in cases.into_iter().enumerate() {
        let pr = 100 + n as u64;
        let mut source = source;
        source.meta.number = pr;
        let did = on_lookup(&with, &seen, pr, "aaa", || Ok(source), |_| {});
        assert_eq!(did, want, "case {n}");
        assert_eq!(store.latest(pr).is_some(), want == Did::Wrote, "case {n}");
    }
    assert_eq!(
        store.records().len(),
        2,
        "only the two that fog were written"
    );
}

#[test]
fn a_quiet_pr_is_not_gathered_again_at_the_same_head() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let black = MAP.replace("\"core\"", "\"black-box\"");
    let gathered = Cell::new(0);
    let look = |head: &str| {
        let source = quiet_source(&black, &[], "");
        on_lookup(
            &with,
            &seen,
            7,
            head,
            || {
                gathered.set(gathered.get() + 1);
                Ok(source)
            },
            |_| {},
        )
    };
    assert_eq!(look("aaa"), Did::Quiet);
    assert_eq!(look("aaa"), Did::Kept);
    assert_eq!(gathered.get(), 1);
    assert_eq!(look("bbb"), Did::Quiet);
    assert_eq!(gathered.get(), 2);
}

#[test]
fn opening_a_pr_that_fogs_nothing_tells_the_tray_it_is_quiet() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let black = MAP.replace("\"core\"", "\"black-box\"");
    let opened = on_open(
        &with,
        &seen,
        7,
        "aaa",
        || Ok(quiet_source(&black, &[], "")),
        |_| {},
    );
    assert!(opened.is_err());
    assert!(
        seen.lock().unwrap().is_quiet(7, "aaa"),
        "the row becomes the quiet one"
    );
}

fn ready(number: u64) -> Ready {
    Ready {
        number,
        title: String::new(),
        head: "aaa".into(),
        branch: String::new(),
        brief: None,
    }
}

#[test]
fn a_brief_is_waiting_for_a_ready_pr_that_has_one_stored() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    on_lookup(&with, &seen, 7, "aaa", || Ok(source("aaa")), |_| {});
    // #8 has none yet; #7 does; the stale one still waits.
    assert_eq!(
        waiting_numbers(&store, &[ready(7), ready(8)], None),
        vec![7]
    );
    store.mark_stale(7, "aaa").unwrap();
    assert_eq!(waiting_numbers(&store, &[ready(7)], None), vec![7]);
    // A PR that is not ready (merged, closed, a draft) is not in the list given, so nothing waits for it.
    assert!(waiting_numbers(&store, &[], None).is_empty());
}

#[test]
fn a_stale_mark_that_fails_is_told_and_not_reported_as_staled() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    lookup(&store, &seen, &fake, "aaa");
    // The store's folder takes no writes, so the mark can't be saved.
    let folder = dir.path().join("pr-briefs");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    let told = RefCell::new(vec![]);
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let did = on_lookup(
        &with,
        &seen,
        7,
        "bbb",
        || Ok(source("bbb")),
        |l| told.borrow_mut().push(l),
    );
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(did, Did::Failed);
    assert!(
        told.borrow()[0].contains("mark the brief stale"),
        "{told:?}"
    );
    assert!(!store.get(7, "aaa").unwrap().stale);
    // The next lookup of the same head rewrites, as after a mark that worked.
    assert_eq!(lookup(&store, &seen, &fake, "bbb"), Did::Wrote);
}

#[test]
fn a_failed_write_is_told_and_stores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let told = RefCell::new(vec![]);
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let did = on_lookup(
        &with,
        &seen,
        7,
        "aaa",
        || Err("no git".into()),
        |l| told.borrow_mut().push(l),
    );
    assert_eq!(did, Did::Failed);
    assert!(told.borrow()[0].contains("no git"));
    assert!(store.latest(7).is_none());
}

fn read_at(chapters: usize, head: &str) -> crate::map::UserState {
    let mut user = crate::map::UserState::default();
    let read = user.fog.reads.entry(7).or_default();
    read.lit = (0..chapters).map(|_| vec!["alpha".to_string()]).collect();
    for i in 0..chapters {
        read.chapters.insert(
            i,
            crate::fog::ChapterRead {
                head: head.into(),
                at: 1,
            },
        );
    }
    user
}

#[test]
fn a_brief_read_to_its_end_stops_waiting_until_the_pr_is_pushed_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    lookup(&store, &seen, &fake, "aaa");
    // Unread, or read in part: waiting.
    assert_eq!(
        waiting_numbers(&store, &[ready(7)], Some(&crate::map::UserState::default())),
        vec![7]
    );
    let mut half = read_at(2, "aaa");
    half.fog.reads.get_mut(&7).unwrap().chapters.remove(&1);
    assert_eq!(waiting_numbers(&store, &[ready(7)], Some(&half)), vec![7]);
    // Read to the end at the PR's head: nothing waits.
    let done = read_at(2, "aaa");
    assert!(waiting_numbers(&store, &[ready(7)], Some(&done)).is_empty());
    // The PR moved on after the reading: waiting again.
    let mut pushed = ready(7);
    pushed.head = "bbb".into();
    assert_eq!(waiting_numbers(&store, &[pushed], Some(&done)), vec![7]);
}

/// A writer that tells when its write starts and blocks until it is released.
struct Blocking {
    started: Mutex<std::sync::mpsc::Sender<()>>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
}
impl BriefWriter for Blocking {
    fn write(&self, _model: &str, _prompt: &str) -> Result<BriefReply, String> {
        self.started.lock().unwrap().send(()).unwrap();
        // A closed channel (the test is done) releases every write, the claim check's too.
        // A closed channel or a timeout both mean carry on.
        self.release.lock().unwrap().recv().ok();
        Fake::new().write("opus", "")
    }
}

#[test]
fn the_tray_is_annotated_while_a_brief_is_being_written() {
    use std::sync::mpsc::{RecvTimeoutError, channel};
    use std::time::Duration;
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let briefs = PrBriefs::new(None);
    let seen = briefs.seen_of("/p");
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel();
    let blocking = Blocking {
        started: Mutex::new(started_tx),
        release: Mutex::new(release_rx),
    };
    let (done_tx, done_rx) = channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            assert!(briefs.busy("/p", 7, true));
            let with = Writing {
                writer: &blocking,
                model: "opus",
                store: &store,
                reading: None,
            };
            on_lookup(&with, &seen, 7, "aaa", || Ok(source("aaa")), |_| {});
            briefs.busy("/p", 7, false);
        });
        started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // The write is running: the Map view's read of the tray must not wait for it.
        scope.spawn(|| {
            let mut tray = [ready(7)];
            briefs.annotate_with("/p", &store, None, &mut tray);
            done_tx.send(tray[0].brief.clone()).unwrap();
        });
        let annotated = done_rx.recv_timeout(Duration::from_secs(3));
        drop(release_tx);
        match annotated {
            Ok(brief) => assert!(brief.unwrap().writing, "the row says it is being written"),
            Err(RecvTimeoutError::Timeout) => panic!("annotate waited for the write"),
            Err(e) => panic!("{e}"),
        }
    });
}

#[test]
fn opening_a_pr_to_write_returns_at_once_and_the_pr_stays_claimed_until_the_write_ends() {
    use std::sync::mpsc::channel;
    use std::time::Duration;
    let briefs = PrBriefs::new(None);
    let (release_tx, release_rx) = channel::<()>();
    let (done_tx, done_rx) = channel();
    let (returned_tx, returned_rx) = channel();
    std::thread::scope(|scope| {
        scope.spawn(|| {
            let opened = briefs.open_in_background("/p", 7, move |claim| {
                // A closed channel or a timeout both mean carry on.
                release_rx.recv_timeout(Duration::from_secs(10)).ok();
                drop(claim);
                done_tx.send(()).unwrap();
            });
            returned_tx.send(opened).unwrap();
        });
        let returned = returned_rx.recv_timeout(Duration::from_secs(3));
        assert!(returned.is_ok(), "the open waited for the write");
        assert!(matches!(
            returned.unwrap(),
            Opened::Writing { writing: true }
        ));
        assert!(
            briefs.claim("/p", 7).is_none(),
            "the PR is claimed while written"
        );
        // A second open meanwhile is told it is being written, and starts nothing.
        assert!(matches!(
            briefs.open_in_background("/p", 7, |_| panic!("a second write")),
            Opened::Writing { writing: true }
        ));
        release_tx.send(()).unwrap();
        done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    });
    // The claim ended with the write.
    assert!(briefs.claim("/p", 7).is_some());
}

#[test]
fn the_tray_says_when_the_write_at_the_head_failed() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let briefs = PrBriefs::new(None);
    let seen = briefs.seen_of("/p");
    let fake = Fake::new();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let failed = |pr: u64, head: &str| {
        let mut tray = [ready_pr(pr, head)];
        briefs.annotate_with("/p", &store, None, &mut tray);
        tray[0].brief.as_ref().map(|b| b.failed)
    };
    on_lookup(&with, &seen, 7, "aaa", || Err("no gh".into()), |_| {});
    assert_eq!(failed(7, "aaa"), Some(true), "a PR with no brief");
    assert_eq!(failed(7, "bbb"), None, "another head is not known to fail");
    // A stored brief whose rewrite failed.
    let mut eight = source("aaa");
    eight.meta.number = 8;
    on_lookup(&with, &seen, 8, "aaa", || Ok(eight), |_| {});
    assert!(on_open(&with, &seen, 8, "bbb", || Err("no gh".into()), |_| {}).is_err());
    assert_eq!(failed(8, "bbb"), Some(true));
    assert_eq!(failed(8, "aaa"), Some(false));
}

#[test]
fn a_failed_write_is_not_retried_at_the_same_head_until_the_brief_is_opened() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: None,
    };
    let sourced = Cell::new(0);
    let failing = || {
        sourced.set(sourced.get() + 1);
        Err("no git".to_string())
    };
    assert_eq!(
        on_lookup(&with, &seen, 7, "aaa", failing, |_| {}),
        Did::Failed
    );
    assert_eq!(
        on_lookup(&with, &seen, 7, "aaa", failing, |_| {}),
        Did::Kept
    );
    assert_eq!(sourced.get(), 1, "the second lookup did not try again");
    // A new head is tried.
    assert_eq!(
        on_lookup(&with, &seen, 7, "bbb", failing, |_| {}),
        Did::Failed
    );
    assert_eq!(sourced.get(), 2);
    // Opening the brief is Daniel asking: it tries, and a success is stored.
    assert!(on_open(&with, &seen, 7, "bbb", || Ok(source("bbb")), |_| {}).is_ok());
    assert!(store.get(7, "bbb").is_some());
}

#[test]
fn the_parts_a_chapter_lights_are_its_steps_not_its_declared_parts() {
    let step = |on: &[&str]| studio_understanding::pr_brief::Step {
        on: on.iter().map(ToString::to_string).collect(),
        ..Default::default()
    };
    let chapter = |parts: &[&str], steps: Vec<_>| studio_understanding::pr_brief::Chapter {
        parts: parts.iter().map(ToString::to_string).collect(),
        steps,
        ..Default::default()
    };
    let brief = studio_understanding::pr_brief::Brief {
        chapters: vec![
            // A single story declares "story" and lights real parts through its steps.
            chapter(&["story"], vec![step(&["alpha"]), step(&["beta", "alpha"])]),
            // A chapter lights a part it did not declare.
            chapter(&["beta"], vec![step(&["beta"]), step(&["writer"])]),
        ],
        ..Default::default()
    };
    assert_eq!(
        lit_parts(&brief),
        vec![vec!["alpha", "beta"], vec!["beta", "writer"]]
    );
}

#[test]
fn a_write_that_panics_leaves_the_pr_free_to_write_again() {
    let briefs = PrBriefs::new(None);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _claim = briefs.claim("/p", 7).expect("free at first");
        assert!(briefs.claim("/p", 7).is_none(), "claimed while written");
        panic!("the writer died");
    }));
    assert!(panicked.is_err());
    assert!(
        briefs.claim("/p", 7).is_some(),
        "the claim ended with the panic"
    );
}

fn ready_pr(number: u64, head: &str) -> Ready {
    Ready {
        number,
        title: format!("PR {number}"),
        head: head.into(),
        branch: String::new(),
        brief: None,
    }
}

const HOUR: u64 = 3_600_000;
const NOW: u64 = 100 * HOUR;

/// The lookup the first start gives `ready`, with each head `ago` hours old, the window 2 hours.
fn at_start(startup: &mut Startup, ready: &[Ready], ago: &[(&str, u64)]) -> Vec<u64> {
    let head_ms = |pr: &Ready| {
        ago.iter()
            .find(|(h, _)| *h == pr.head)
            .map(|(_, hours)| NOW - hours * HOUR)
    };
    startup.allowed("/p", true, ready, &head_ms, NOW, 2 * HOUR, &|_| false)
}

#[test]
fn at_start_at_most_three_ready_prs_whose_head_moved_within_the_window_are_written_newest_head_first()
 {
    let ready: Vec<Ready> = (1..=6).map(|n| ready_pr(n, &format!("h{n}"))).collect();
    // PR 5's head is the newest, then 2, 4, 1; PR 3 is outside the 2-hour window and PR 6 has no known head time.
    let ago = [("h1", 1), ("h2", 0), ("h3", 5), ("h4", 1), ("h5", 0)];
    let mut startup = Startup::default();
    let mut written = at_start(&mut startup, &ready, &ago);
    written.sort();
    // The two heads made now, then the newer PR of the two made an hour ago; 3 is outside the window, 6 has no head time.
    assert_eq!(written, [2, 4, 5]);
    assert_eq!(START_BRIEFS, 3);
}

#[test]
fn a_pr_left_for_opening_at_start_is_not_written_by_later_lookups_but_one_that_becomes_ready_is() {
    let mut startup = Startup::default();
    let first = [ready_pr(1, "h1"), ready_pr(2, "h2")];
    let ago = [("h1", 0), ("h2", 9), ("h4", 0)];
    assert_eq!(at_start(&mut startup, &first, &ago), [1]);
    // The next lookup finds PR 2 again, with a new head: still left for opening; PR 4 became ready while running.
    let second = [ready_pr(1, "h1"), ready_pr(2, "h2b"), ready_pr(4, "h4")];
    let later = startup.allowed("/p", true, &second, &|_| None, NOW, 2 * HOUR, &|_| false);
    assert_eq!(later, [1, 4]);
    // PR 2 leaves the ready list and comes back: it became ready while running.
    startup.allowed(
        "/p",
        true,
        &[ready_pr(1, "h1")],
        &|_| None,
        NOW,
        2 * HOUR,
        &|_| false,
    );
    let back = startup.allowed("/p", true, &second, &|_| None, NOW, 2 * HOUR, &|_| false);
    assert_eq!(back, [1, 2, 4]);
}

#[test]
fn a_pr_whose_brief_is_not_fresh_counts_toward_the_start_cap_and_projects_start_on_their_own() {
    let mut startup = Startup::default();
    // PR 1's brief is of its head; 2, 3, 4 and 6 have a brief of an older head (or one marked stale); 5 has none.
    let ready: Vec<Ready> = (1..=6).map(|n| ready_pr(n, &format!("h{n}"))).collect();
    let ago = [
        ("h1", 9),
        ("h2", 0),
        ("h3", 1),
        ("h4", 1),
        ("h5", 0),
        ("h6", 9),
    ];
    let head_ms = |pr: &Ready| {
        ago.iter()
            .find(|(h, _)| *h == pr.head)
            .map(|(_, hours)| NOW - hours * HOUR)
    };
    let fresh = |pr: &Ready| pr.number == 1;
    // Rewrites count toward the cap: the newest heads 5 and 2, then 4 (the newer of the two an hour old). 3 and 6 stay stale
    // until opened; 1 is looked at and kept.
    let first = startup.allowed("/p", true, &ready, &head_ms, NOW, 2 * HOUR, &fresh);
    assert_eq!(first, [1, 2, 4, 5]);
    let again = startup.allowed("/p", true, &ready, &head_ms, NOW, 2 * HOUR, &fresh);
    assert_eq!(again, [1, 2, 4, 5]);
    // Another project's first lookup is its own start.
    let other = startup.allowed(
        "/q",
        true,
        &[ready_pr(1, "h1")],
        &|_| Some(NOW),
        NOW,
        2 * HOUR,
        &|_| false,
    );
    assert_eq!(other, [1]);
}

#[test]
fn a_pr_left_for_opening_is_looked_at_as_usual_once_its_brief_was_written() {
    let mut startup = Startup::default();
    let ready = [ready_pr(3, "h3")];
    let old = |_: &Ready| Some(NOW - 9 * HOUR);
    assert!(
        startup
            .allowed("/p", true, &ready, &old, NOW, 2 * HOUR, &|_| false)
            .is_empty()
    );
    // Daniel opened it and its brief was written.
    assert_eq!(
        startup.allowed("/p", true, &ready, &old, NOW, 2 * HOUR, &|_| true),
        [3]
    );
    // A later push: the lookups stale and rewrite it as for any PR.
    let pushed = [ready_pr(3, "h3b")];
    assert_eq!(
        startup.allowed("/p", true, &pushed, &old, NOW, 2 * HOUR, &|_| false),
        [3]
    );
}

#[test]
fn a_brief_of_an_older_head_or_marked_stale_is_not_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let store = BriefStore::open(dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    assert!(!has_fresh_brief(&store, &ready_pr(7, "aaa")));
    lookup(&store, &seen, &fake, "aaa");
    assert!(has_fresh_brief(&store, &ready_pr(7, "aaa")));
    assert!(!has_fresh_brief(&store, &ready_pr(7, "bbb")));
    store.mark_stale(7, "aaa").unwrap();
    assert!(!has_fresh_brief(&store, &ready_pr(7, "aaa")));
}

#[test]
fn a_first_lookup_that_gh_did_not_answer_does_not_spend_the_start() {
    let mut startup = Startup::default();
    let ready: Vec<Ready> = (1..=5).map(|n| ready_pr(n, &format!("h{n}"))).collect();
    let old = |_: &Ready| Some(NOW - 9 * HOUR);
    // `gh` did not answer at start: nothing is known to be open, and nothing is written.
    assert!(
        startup
            .allowed("/p", false, &[], &old, NOW, 2 * HOUR, &|_| false)
            .is_empty()
    );
    // The first lookup that answers is the start: every head is old, so all wait to be opened.
    let first = startup.allowed("/p", true, &ready, &old, NOW, 2 * HOUR, &|_| false);
    assert!(first.is_empty(), "the start cap still holds: {first:?}");
}

// ---- Since you read: a new head after a full read (spec #477, part B).

const REPO_MAP: &str = r#"{ "version": 1,
  "parts": [{ "id": "alpha", "name": "Alpha", "caption": "c", "description": "d", "tier": "core", "paths": ["alpha/"] }],
  "actors": [], "links": [], "ignored": ["docs/"], "laws": [], "flows": [] }"#;

/// A repository whose `main` holds the map and alpha's file, and a PR branch `pr` with one commit on alpha.
struct Repo {
    dir: tempfile::TempDir,
    path: String,
}
impl Repo {
    fn new() -> Repo {
        let (dir, path) = studio_test_support::repo();
        let repo = Repo { dir, path };
        repo.commit(&[
            ("system-map.json", REPO_MAP),
            ("alpha/a.rs", "1"),
            ("docs/d.md", "1"),
        ]);
        repo.git(&["checkout", "-q", "-b", "pr"]);
        repo.commit(&[("alpha/a.rs", "2")]);
        repo
    }
    fn git(&self, args: &[&str]) -> String {
        studio_test_support::git(self.dir.path(), args)
    }
    fn commit(&self, files: &[(&str, &str)]) -> String {
        for (name, text) in files {
            let path = self.dir.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "change"]);
        self.head()
    }
    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
    /// Main moves on with a commit of its own, and the branch is rebased onto it.
    fn rebase(&self) -> String {
        self.git(&["checkout", "-q", "main"]);
        self.commit(&[("docs/main.md", "main")]);
        self.git(&["checkout", "-q", "pr"]);
        self.git(&["rebase", "-q", "main"]);
        self.head()
    }
    fn push(&self, read: Option<&crate::fog::PrRead>, old: &str, new: &str) -> Push {
        push_of(
            Path::new(&self.path),
            "main",
            read,
            &Default::default(),
            7,
            old,
            new,
        )
        .0
    }
}

/// A read of a brief of `chapters` chapters, each lighting alpha, with chapter `i` read at `heads[i]` (none for "").
fn read_of(chapters: usize, heads: &[&str]) -> crate::fog::PrRead {
    let mut read = crate::fog::PrRead {
        lit: (0..chapters).map(|_| vec!["alpha".to_string()]).collect(),
        ..Default::default()
    };
    for (i, head) in heads.iter().enumerate().filter(|(_, h)| !h.is_empty()) {
        read.chapters.insert(
            i,
            crate::fog::ChapterRead {
                head: head.to_string(),
                at: 1 + i as u64,
            },
        );
    }
    read
}

#[test]
fn a_push_after_a_full_read_that_fogs_nothing_carries_the_read_over() {
    let repo = Repo::new();
    let h1 = repo.head();
    let h2 = repo.commit(&[("docs/d.md", "2")]);
    let push = repo.push(Some(&read_of(1, &[&h1])), &h1, &h2);
    assert_eq!(push, Push::Quiet);
    assert_eq!(
        push.note().as_deref(),
        Some("New push touched nothing on your map")
    );
}

#[test]
fn a_push_after_a_full_read_that_fogs_something_is_the_since_case() {
    let repo = Repo::new();
    let h1 = repo.head();
    let h2 = repo.commit(&[("alpha/a.rs", "3")]);
    assert_eq!(
        repo.push(Some(&read_of(1, &[&h1])), &h1, &h2),
        Push::Since(h1.clone())
    );
}

#[test]
fn a_pure_rebase_after_a_full_read_carries_the_read_over() {
    let repo = Repo::new();
    let h1 = repo.head();
    let h2 = repo.rebase();
    assert_ne!(h1, h2);
    let push = repo.push(Some(&read_of(1, &[&h1])), &h1, &h2);
    assert_eq!(push, Push::Rebased);
    assert_eq!(
        push.note().as_deref(),
        Some("Rebased onto main, nothing new in the PR")
    );
}

#[test]
fn a_rebase_that_drops_or_changes_a_commit_rewrites_the_brief() {
    let repo = Repo::new();
    repo.commit(&[("alpha/b.rs", "1")]);
    let h1 = repo.head();
    let read = read_of(1, &[&h1]);
    // The branch is rebuilt on a moved main with only its second commit: the counts differ.
    repo.git(&["checkout", "-q", "main"]);
    repo.commit(&[("docs/main.md", "main")]);
    repo.git(&["checkout", "-q", "-B", "pr"]);
    repo.git(&["cherry-pick", &h1]);
    let dropped = repo.head();
    assert_eq!(
        repo.push(Some(&read), &h1, &dropped),
        Push::Rewrite(Some(REBASED.into()))
    );
    // The branch rebuilt with as many commits, one of them a different patch.
    repo.git(&["checkout", "-q", "-B", "pr", "main"]);
    repo.commit(&[("alpha/a.rs", "other")]);
    let changed = repo.commit(&[("alpha/b.rs", "1")]);
    assert_eq!(
        repo.push(Some(&read), &h1, &changed),
        Push::Rewrite(Some(REBASED.into()))
    );
}

#[test]
fn merging_main_into_the_branch_is_judged_by_the_prs_own_commits_not_mains() {
    let repo = Repo::new();
    let h1 = repo.head();
    let read = read_of(1, &[&h1]);
    // Main changes alpha, which would fog, and is merged into the branch: the PR's own commits are as they were.
    repo.git(&["checkout", "-q", "main"]);
    repo.commit(&[("alpha/main.rs", "main")]);
    repo.git(&["checkout", "-q", "pr"]);
    repo.git(&["merge", "-q", "--no-edit", "main"]);
    let merged = repo.head();
    let push = repo.push(Some(&read), &h1, &merged);
    assert_eq!(push, Push::Merged);
    assert_eq!(
        push.note().as_deref(),
        Some("Main merged into the branch, nothing new in the PR")
    );
    // A merge that also edits the PR's own file: its non-merge commits are the same, but the PR's diff is not.
    repo.git(&["reset", "-q", "--hard", &h1]);
    repo.git(&["merge", "-q", "--no-commit", "main"]);
    std::fs::write(repo.dir.path().join("alpha/a.rs"), "edited in the merge").unwrap();
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "--no-edit"]);
    let edited = repo.head();
    assert_eq!(
        repo.push(Some(&read), &h1, &edited),
        Push::Rewrite(Some(MERGED.into()))
    );
    repo.git(&["reset", "-q", "--hard", &merged]);
    // A commit of the PR's own on top of the merge: the full brief is rewritten, saying why.
    let more = repo.commit(&[("alpha/a.rs", "3")]);
    assert_eq!(
        repo.push(Some(&read), &h1, &more),
        Push::Rewrite(Some(MERGED.into()))
    );
}

#[test]
fn a_push_whose_diff_cannot_be_gathered_rewrites_a_read_brief_with_a_note() {
    let repo = Repo::new();
    let h1 = repo.head();
    // The new head's map does not parse, so what the push fogs cannot be told.
    let h2 = repo.commit(&[("system-map.json", "{ not a map")]);
    assert_eq!(
        repo.push(Some(&read_of(1, &[&h1])), &h1, &h2),
        Push::Rewrite(Some("Rewritten after a new push".into()))
    );
}

#[test]
fn a_missing_old_head_rewrites_the_brief() {
    let repo = Repo::new();
    let gone = "0".repeat(40);
    let h2 = repo.head();
    assert_eq!(
        repo.push(Some(&read_of(1, &[&gone])), &gone, &h2),
        Push::Rewrite(Some(REBASED.into()))
    );
}

#[test]
fn a_partial_read_rewrites_the_brief_and_says_how_much_was_read() {
    let repo = Repo::new();
    let h0 = repo.git(&["rev-parse", "HEAD~1"]);
    let h1 = repo.head();
    let h2 = repo.commit(&[("docs/d.md", "2")]);
    let note = |read: &crate::fog::PrRead| repo.push(Some(read), &h1, &h2);
    let one_of_two = Push::Rewrite(Some(
        "Rewritten after a new push; you had read 1 of 2".into(),
    ));
    assert_eq!(note(&read_of(2, &[&h1, ""])), one_of_two);
    // The lit parts did not change, so an earlier chapter's read stayed at its old head: not fully read at H1.
    assert_eq!(note(&read_of(2, &[&h0, &h1])), one_of_two);
    // Nothing read: the rewrite has no note.
    assert_eq!(note(&read_of(2, &[])), Push::Rewrite(None));
    assert_eq!(repo.push(None, &h1, &h2), Push::Rewrite(None));
}

/// Daniel's read kept in a state file, as the server's `Reading`.
struct Rig {
    repo: Repo,
    state: tempfile::TempDir,
    store_dir: tempfile::TempDir,
}
impl Rig {
    fn new() -> Rig {
        Rig {
            repo: Repo::new(),
            state: tempfile::tempdir().unwrap(),
            store_dir: tempfile::tempdir().unwrap(),
        }
    }
    fn reading(&self) -> GitReading {
        GitReading {
            project: self.repo.path.clone(),
            file: self.state.path().join("map-state.json"),
            reference: Some("main".into()),
        }
    }
    fn user(&self) -> crate::map::UserState {
        crate::map::read_user(&self.reading().file, &self.repo.path)
    }
}

#[test]
fn after_a_carry_over_the_pr_stops_waiting_and_the_next_lookup_keeps_the_copied_brief() {
    let rig = Rig::new();
    let store = BriefStore::open(rig.store_dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let reading = rig.reading();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: Some(&reading),
    };
    let look = |head: &str| on_lookup(&with, &seen, 7, head, || Ok(source(head)), |_| {});
    let h1 = rig.repo.head();
    assert_eq!(look(&h1), Did::Wrote);
    let calls = fake.0.get();
    let lit = lit_parts(&store.get(7, &h1).unwrap().brief);
    crate::fog::mark_read(&reading.file, &rig.repo.path, 7, 0, &h1, lit.clone(), 1).unwrap();
    let h2 = rig.repo.rebase();
    assert_eq!(look(&h2), Did::Carried);
    let copied = store
        .get(7, &h2)
        .expect("the brief is copied to the new head");
    assert_eq!((copied.head.as_str(), copied.stale), (h2.as_str(), false));
    assert_eq!(store.latest(7).unwrap().head, h2);
    assert_eq!(rig.user().fog.reads[&7].lit, lit, "the lit parts are kept");
    assert!(waiting_numbers(&store, &[ready_pr(7, &h2)], Some(&rig.user())).is_empty());
    assert_eq!(look(&h2), Did::Kept);
    assert_eq!(look(&h2), Did::Kept);
    assert_eq!(fake.0.get(), calls, "the writer was not asked again");
    // The tray says why the read carried over.
    let briefs = PrBriefs::new(None);
    *briefs.seen_of("/p").lock().unwrap() = std::mem::take(&mut *seen.lock().unwrap());
    let mut tray = [ready_pr(7, &h2)];
    briefs.annotate_with("/p", &store, Some(&rig.user()), &mut tray);
    let info = tray[0].brief.clone().unwrap();
    assert_eq!(
        info.note.as_deref(),
        Some("Rebased onto main, nothing new in the PR")
    );
    assert!(info.read && !info.stale);
}

#[test]
fn a_carried_brief_holds_the_link_evidence_of_the_new_head() {
    let rig = Rig::new();
    let store = BriefStore::open(rig.store_dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let reading = rig.reading();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: Some(&reading),
    };
    let part = |id: &str| {
        serde_json::json!({ "id": id, "name": id, "caption": "c", "purpose": "p", "holds": [], "tier": "core",
                            "paths": [format!("{id}/")] })
    };
    let map = serde_json::json!({
        "version": 2, "extractors": ["typescript-imports"],
        "parts": [part("alpha"), part("beta")], "actors": [],
        "links": [{ "id": "alpha-beta", "from": "alpha", "to": "beta", "kind": "import" }],
        "ignored": ["system-map.json", "docs/"], "laws": [], "flows": []
    })
    .to_string();
    // The PR adds the link and its one import.
    let h1 = rig.repo.commit(&[
        ("system-map.json", &map),
        ("alpha/a.ts", "import { b } from \"../beta/b\";\n"),
        ("beta/b.ts", "export const b = 1;\n"),
    ]);
    let repo = Path::new(&rig.repo.path);
    let at = |head: &str| {
        let mut s = source(head);
        s.link_evidence = crate::ratify::link_evidence(repo, "main", head);
        s
    };
    assert_eq!(
        on_lookup(&with, &seen, 7, &h1, || Ok(at(&h1)), |_| {}),
        Did::Wrote
    );
    let lit = lit_parts(&store.get(7, &h1).unwrap().brief);
    crate::fog::mark_read(&reading.file, &rig.repo.path, 7, 0, &h1, lit, 1).unwrap();
    // Main gains a second import of beta, and the PR is rebased onto it: the read carries over.
    rig.repo.git(&["checkout", "-q", "main"]);
    rig.repo
        .commit(&[("alpha/z.ts", "// z\nimport { b } from \"../beta/b\";\n")]);
    rig.repo.git(&["checkout", "-q", "pr"]);
    rig.repo.git(&["rebase", "-q", "main"]);
    let h2 = rig.repo.head();
    assert_eq!(
        on_lookup(&with, &seen, 7, &h2, || Ok(at(&h2)), |_| {}),
        Did::Carried
    );
    let site = |file: &str, line: u32| ImportSite {
        file: file.into(),
        line: Some(line),
    };
    assert_eq!(
        store.get(7, &h2).unwrap().link_evidence,
        [LinkEvidence {
            link: "alpha-beta".into(),
            sites: vec![site("alpha/a.ts", 1), site("alpha/z.ts", 2)],
        }]
    );
}

#[test]
fn a_push_that_does_not_carry_stales_the_brief_as_before_and_the_tray_says_why() {
    let rig = Rig::new();
    let store = BriefStore::open(rig.store_dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let reading = rig.reading();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: Some(&reading),
    };
    let h1 = rig.repo.head();
    on_lookup(&with, &seen, 7, &h1, || Ok(source(&h1)), |_| {});
    let h2 = rig.repo.rebase();
    // Never read: the brief is rewritten, with no note.
    assert_eq!(
        on_lookup(&with, &seen, 7, &h2, || Ok(source(&h2)), |_| {}),
        Did::Staled
    );
    let opened = on_open(&with, &seen, 7, &h2, || Ok(source(&h2)), |_| {}).unwrap();
    assert_eq!(opened.head, h2);
    // Read in full at H2, then a push the branch does not descend from.
    let lit = lit_parts(&opened.brief);
    crate::fog::mark_read(&reading.file, &rig.repo.path, 7, 0, &h2, lit, 2).unwrap();
    rig.repo.git(&["checkout", "-q", "-B", "pr", "main"]);
    let h3 = rig.repo.commit(&[("alpha/a.rs", "4")]);
    assert_eq!(
        on_lookup(&with, &seen, 7, &h3, || Ok(source(&h3)), |_| {}),
        Did::Staled
    );
    let briefs = PrBriefs::new(None);
    *briefs.seen_of("/p").lock().unwrap() = std::mem::take(&mut *seen.lock().unwrap());
    let mut tray = [ready_pr(7, &h3)];
    briefs.annotate_with("/p", &store, Some(&rig.user()), &mut tray);
    assert_eq!(
        tray[0].brief.clone().unwrap().note.as_deref(),
        Some(REBASED)
    );
}

#[test]
fn a_read_finished_at_the_old_head_after_a_push_was_judged_still_carries_over() {
    let rig = Rig::new();
    let store = BriefStore::open(rig.store_dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let reading = rig.reading();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: Some(&reading),
    };
    let h1 = rig.repo.head();
    on_lookup(&with, &seen, 7, &h1, || Ok(source(&h1)), |_| {});
    let lit = vec![vec!["alpha".to_string()], vec!["alpha".to_string()]];
    let read = |chapter| {
        crate::fog::mark_read(
            &reading.file,
            &rig.repo.path,
            7,
            chapter,
            &h1,
            lit.clone(),
            1,
        )
        .unwrap()
    };
    read(0);
    // A push that fogs nothing, seen while one chapter of two is read: the brief would be rewritten.
    let h2 = rig.repo.commit(&[("docs/d.md", "2")]);
    assert_eq!(
        on_lookup(&with, &seen, 7, &h2, || Ok(source(&h2)), |_| {}),
        Did::Staled
    );
    // Daniel finishes the brief at H1 before it is rewritten: the read carries over.
    read(1);
    assert_eq!(
        on_lookup(&with, &seen, 7, &h2, || Ok(source(&h2)), |_| {}),
        Did::Carried
    );
}

// ---- The since story (#497).

#[test]
fn a_push_that_fogs_something_after_a_full_read_writes_a_since_story_apart_and_the_full_brief_as_before()
 {
    let rig = Rig::new();
    let store = BriefStore::open(rig.store_dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let reading = rig.reading();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: Some(&reading),
    };
    let look = |head: &str| on_lookup(&with, &seen, 7, head, || Ok(source(head)), |_| {});
    let h1 = rig.repo.head();
    assert_eq!(look(&h1), Did::Wrote);
    let lit = lit_parts(&store.get(7, &h1).unwrap().brief);
    crate::fog::mark_read(&reading.file, &rig.repo.path, 7, 0, &h1, lit, 1).unwrap();
    let h2 = rig.repo.commit(&[("alpha/a.rs", "3")]);
    assert_eq!(look(&h2), Did::Staled);
    assert_eq!(look(&h2), Did::Wrote);
    let (from, story) = store.since(7, &h2).expect("the since story is stored");
    assert_eq!(
        (from.as_str(), story.head.as_str()),
        (h1.as_str(), h2.as_str())
    );
    assert_eq!(
        store.latest(7).unwrap().head,
        h2,
        "the latest brief is the full one at H2"
    );
    assert!(store.get(7, &h2).is_some());
    // The tray opens the story first.
    let briefs = PrBriefs::new(None);
    *briefs.seen_of("/p").lock().unwrap() = std::mem::take(&mut *seen.lock().unwrap());
    let mut tray = [ready_pr(7, &h2)];
    briefs.annotate_with("/p", &store, Some(&rig.user()), &mut tray);
    let info = tray[0].brief.clone().unwrap();
    assert_eq!(info.since.as_deref(), Some("Alpha"));
    assert!(info.since_chapters.is_empty(), "a single story has one row");
    // A story of two chapters has a tray row for each.
    let mut two = story.clone();
    let chapter = two.brief.chapters[0].clone();
    two.brief.chapters = vec![
        studio_understanding::pr_brief::Chapter {
            title: "The read".into(),
            ..chapter.clone()
        },
        studio_understanding::pr_brief::Chapter {
            title: "The fog".into(),
            ..chapter
        },
    ];
    store.put_since(&from, &two).unwrap();
    briefs.annotate_with("/p", &store, Some(&rig.user()), &mut tray);
    assert_eq!(
        tray[0].brief.clone().unwrap().since_chapters,
        ["The read", "The fog"]
    );
}

#[test]
fn opening_the_since_case_writes_the_story_first_and_reading_it_to_the_end_carries_every_chapters_read_over()
 {
    let rig = Rig::new();
    let store = BriefStore::open(rig.store_dir.path()).unwrap();
    let (seen, fake) = (Mutex::new(Seen::default()), Fake::new());
    let reading = rig.reading();
    let with = Writing {
        writer: &fake,
        model: "opus",
        store: &store,
        reading: Some(&reading),
    };
    let h1 = rig.repo.head();
    on_lookup(&with, &seen, 7, &h1, || Ok(source(&h1)), |_| {});
    // A brief of two chapters, both read at H1.
    let lit = vec![vec!["alpha".to_string()], vec!["alpha".to_string()]];
    for chapter in 0..2 {
        crate::fog::mark_read(
            &reading.file,
            &rig.repo.path,
            7,
            chapter,
            &h1,
            lit.clone(),
            1,
        )
        .unwrap();
    }
    let h2 = rig.repo.commit(&[("alpha/a.rs", "3")]);
    assert_eq!(
        on_lookup(&with, &seen, 7, &h2, || Ok(source(&h2)), |_| {}),
        Did::Staled
    );
    let landed = Cell::new(false);
    let story = on_open_since(
        &with,
        &seen,
        7,
        &h2,
        || Ok(source(&h2)),
        || {
            // The story opens before the full brief is written.
            assert!(store.since(7, &h2).is_some() && store.get(7, &h2).is_none());
            landed.set(true);
            true
        },
        |_| {},
    )
    .expect("the since case has a story")
    .unwrap();
    assert!(landed.get(), "the story was told before the full brief");
    assert_eq!(store.since(7, &h2).map(|(from, _)| from), Some(h1.clone()));
    assert_eq!(story.head, h2);
    assert!(
        store.get(7, &h2).is_some_and(|b| !b.stale),
        "the full brief at H2 is written after the story"
    );
    // Opened again, the stored story is read as it is.
    let calls = fake.0.get();
    on_open_since(&with, &seen, 7, &h2, || Ok(source(&h2)), || true, |_| {});
    assert_eq!(fake.0.get(), calls);
    // Not read yet: the PR still waits at H2.
    assert!(!crate::fog::fully_read_at(&rig.user(), 7, &h2));
    since_read(&store, &reading, 7, &h2).unwrap();
    let read = &rig.user().fog.reads[&7];
    assert_eq!(read.lit, lit, "the lit parts are kept");
    assert!(
        read.fully_read_at(&h2),
        "both chapters moved to H2: {read:?}"
    );
    // A head with no story carries nothing.
    assert!(since_read(&store, &reading, 7, &h1).is_err());
}
