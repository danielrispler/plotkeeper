use super::*;
use studio_test_support::{git, origin, repo};

const MAP: &str = r#"{"version":2,"parts":[],"actors":[],"links":[],"ignored":[],"laws":[]}"#;

#[test]
fn a_gone_folder_gets_no_header_mark_and_its_map_is_not_read() {
    let data = tempfile::tempdir().unwrap();
    let gone = data.path().join("pokedex").display().to_string();
    let file = data.path().join("ci-last.json");
    std::fs::write(
        &file,
        serde_json::json!({ &gone: { "status": "broken", "parts": ["shell"] } }).to_string(),
    )
    .unwrap();
    let cache = crate::ci_laws::CiCache::load(file);
    assert!(cache.broken(&gone));
    assert_eq!(
        map_marks(vec![gone], &cache, &crate::prs::Prs::default()).unwrap(),
        []
    );
}

fn commit_file(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", name]);
}

/// One case of the read order: what each source holds, and what the Map view is told.
struct ReadCase {
    name: &'static str,
    /// `system-map.json` pushed to `origin`'s default branch.
    default: Option<&'static str>,
    /// `system-map.json` written into the main checkout, uncommitted.
    checkout: Option<&'static str>,
    /// A file committed to the checkout's local branch, never pushed: tracked, so `check` sees it.
    tracked: Option<&'static str>,
    /// The accepted map waiting on draft PR #12, in the data folder.
    pending: Option<&'static str>,
    /// The project's folder is a git repository.
    git: bool,
    status: Status,
    from: Option<ReadFrom>,
    /// A word of the note, when one is wanted.
    note: Option<&'static str>,
    /// A word of the first problem of the banner, or none for no banner.
    problem: Option<&'static str>,
}

/// A case with no source holding a map, in a git repository.
const NOTHING: ReadCase = ReadCase {
    name: "",
    default: None,
    checkout: None,
    tracked: None,
    pending: None,
    git: true,
    status: Status::None,
    from: None,
    note: None,
    problem: None,
};

/// The read order's cases (#753's ticket #757): each source alone, each beside a later one, each unreadable, a checkout map failing
/// `check`, none, and a folder in no git repository.
fn read_cases() -> Vec<ReadCase> {
    use ReadFrom::{Checkout, DefaultBranch, PendingPr};
    let read = |name, from| ReadCase {
        name,
        status: Status::Loaded,
        from: Some(from),
        ..NOTHING
    };
    let unread = |name, from| ReadCase {
        name,
        status: Status::Unreadable,
        from: Some(from),
        note: Some("does not read"),
        ..NOTHING
    };
    vec![
        ReadCase {
            default: Some(MAP),
            ..read("default branch only", DefaultBranch)
        },
        ReadCase {
            checkout: Some(MAP),
            ..read("checkout only", Checkout)
        },
        ReadCase {
            pending: Some(MAP),
            ..read("pending only", PendingPr)
        },
        // The working tree's own copy differs, and the default branch wins: no banner for its problems.
        ReadCase {
            default: Some(MAP),
            checkout: Some("not the map"),
            pending: Some(MAP),
            ..read("default beside a checkout file", DefaultBranch)
        },
        ReadCase {
            checkout: Some(MAP),
            pending: Some("not the map"),
            ..read("checkout beside a pending map", Checkout)
        },
        ReadCase {
            default: Some("{ nope"),
            checkout: Some(MAP),
            ..unread("unreadable default branch file", DefaultBranch)
        },
        ReadCase {
            checkout: Some("{ nope"),
            pending: Some(MAP),
            ..unread("unreadable checkout file", Checkout)
        },
        ReadCase {
            pending: Some("{ nope"),
            ..unread("unreadable pending map", PendingPr)
        },
        // A tracked file no part owns: `check` fails, and the map is drawn with the banner.
        ReadCase {
            checkout: Some(MAP),
            tracked: Some("lib.rs"),
            problem: Some("lib.rs"),
            ..read("readable checkout file failing check", Checkout)
        },
        ReadCase {
            name: "no map anywhere",
            ..NOTHING
        },
        ReadCase {
            name: "a non-git folder",
            checkout: Some(MAP),
            pending: Some(MAP),
            git: false,
            status: Status::NotGit,
            note: Some("not a git repository"),
            ..NOTHING
        },
    ]
}

#[test]
fn the_map_reads_the_default_branch_else_the_checkout_else_a_pending_pr() {
    for c in read_cases() {
        let o = origin();
        let plain = tempfile::tempdir().unwrap();
        let repo = if c.git {
            o.main.clone()
        } else {
            plain.path().to_path_buf()
        };
        if let Some(text) = c.default {
            commit_file(&o.seed, MAP_FILE, text);
            git(&o.seed, &["push", "-q", "origin", "HEAD"]);
            git(&o.main, &["fetch", "-q", "origin"]);
        }
        if let Some(name) = c.tracked {
            commit_file(&repo, name, "fn main() {}");
        }
        if let Some(text) = c.checkout {
            std::fs::write(repo.join(MAP_FILE), text).unwrap();
        }
        let data = tempfile::tempdir().unwrap();
        let pending = data.path().join("pending.json");
        if let Some(text) = c.pending {
            let file = serde_json::json!({ "number": 12, "branch": "first-map/1", "map": text });
            std::fs::write(&pending, file.to_string()).unwrap();
        }
        let state = read_checked(&repo, |_| Some(pending.clone()));
        let name = c.name;
        assert_eq!(state.status, c.status, "{name}: status");
        assert_eq!(state.from, c.from, "{name}: from");
        let want = (c.status == Status::Loaded).then(|| MAP.to_string());
        assert_eq!(state.source, want, "{name}: source");
        if let Some(word) = c.note {
            assert!(
                state.note.as_deref().is_some_and(|n| n.contains(word)),
                "{name}: note {:?}",
                state.note
            );
        }
        match c.problem {
            Some(word) => assert!(
                state.problems.first().is_some_and(|p| p.contains(word)),
                "{name}: banner {:?}",
                state.problems
            ),
            None => assert_eq!(state.problems, Vec::<String>::new(), "{name}: banner"),
        }
        let pending = (c.from == Some(ReadFrom::PendingPr)).then(|| PendingPr {
            number: 12,
            closed: false,
            path: Path::new(&studio_workbench::main_checkout(&repo).unwrap())
                .join(MAP_FILE)
                .display()
                .to_string(),
            pr_muted: None,
        });
        assert_eq!(state.pending, pending, "{name}: pending PR");
        if c.git {
            // The default branch's ref and commit stay known whichever source the map came from.
            assert_eq!(
                state.reference.as_deref(),
                Some("origin/trunk"),
                "{name}: reference"
            );
            assert_eq!(
                state.commit,
                Some(git(&o.main, &["rev-parse", "origin/trunk"])),
                "{name}: commit"
            );
        }
    }
}

#[test]
fn a_group_of_paths_is_placed_on_every_part_it_touches() {
    let o = origin();
    let map = r#"{"version":1,"parts":[
        {"id":"core","name":"Core","caption":"","description":"","tier":"core","paths":["core/"]},
        {"id":"window","name":"Window","caption":"","description":"","tier":"core","paths":["ui/"]}],
        "actors":[],"links":[],"ignored":["docs/"],"laws":[]}"#;
    commit_file(&o.seed, MAP_FILE, map);
    git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    git(&o.main, &["fetch", "-q", "origin"]);
    let paths = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    let groups = BTreeMap::from([
        ("both".to_string(), paths(&["ui/a.ts", "core/b.rs"])),
        ("docs".to_string(), paths(&["docs/x.md"])),
        ("none".to_string(), paths(&[])),
    ]);
    let placed = place(&o.main, &groups);
    assert_eq!(placed["both"], ["core", "window"]);
    assert!(placed["docs"].is_empty());
    assert!(placed["none"].is_empty());
    // With no map on the branch, nothing is placed.
    let bare = origin();
    assert!(place(&bare.main, &groups)["both"].is_empty());
}

#[test]
fn see_the_code_of_a_step_keeps_its_parts_files_before_the_cap_of_80() {
    let o = origin();
    let map = r#"{"version":1,"parts":[
        {"id":"a","name":"A","caption":"","description":"","tier":"core","paths":["a/"]},
        {"id":"z","name":"Z","caption":"","description":"","tier":"core","paths":["z/"]},
        {"id":"zz","name":"ZZ","caption":"","description":"","tier":"core","paths":["z/own/"]}],
        "actors":[],"links":[],"ignored":["z/docs/"],"laws":[]}"#;
    commit_file(&o.seed, MAP_FILE, map);
    git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    git(&o.main, &["fetch", "-q", "origin"]);
    git(&o.main, &["checkout", "-q", "-b", "work", "origin/trunk"]);
    // 85 files of part a sort before part z's, so the cap of 80 would cut z's away.
    for i in 0..85 {
        let dir = o.main.join("a");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("f{i:02}")), "x\n").unwrap();
    }
    for name in ["z/one", "z/two", "z/own/three", "z/docs/four"] {
        let path = o.main.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x\n").unwrap();
    }
    git(&o.main, &["add", "-A"]);
    git(&o.main, &["commit", "-q", "-m", "work"]);
    let head = git(&o.main, &["rev-parse", "HEAD"]);
    let diff = |parts: Option<&[&str]>| {
        let found = pr_diff(PrDiffArgs {
            project: o.main.display().to_string(),
            number: 7,
            head: head.clone(),
            title: "Work".into(),
            parts: parts.map(|p| p.iter().map(ToString::to_string).collect()),
            paths: None,
            base: None,
        })
        .unwrap();
        found.files.into_iter().map(|f| f.path).collect::<Vec<_>>()
    };
    // A later part's pattern wins its files, and an ignored file is no part's.
    assert_eq!(diff(Some(&["z"])), ["z/one", "z/two"]);
    assert_eq!(diff(Some(&["zz", "nobody"])), ["z/own/three"]);
    assert!(diff(Some(&[])).is_empty());
    assert_eq!(diff(None).len(), 80);
    // An Also touched group's files (#492), by path, before the cap too; a path the PR did not change is not shown.
    let found = pr_diff(PrDiffArgs {
        project: o.main.display().to_string(),
        number: 7,
        head: head.clone(),
        title: "Work".into(),
        parts: None,
        paths: Some(vec!["z/docs/four".into(), "z/one".into(), "nowhere".into()]),
        base: None,
    })
    .unwrap();
    let shown: Vec<_> = found.files.into_iter().map(|f| f.path).collect();
    assert_eq!(shown, ["z/docs/four", "z/one"]);
}

#[test]
fn see_the_code_of_a_step_places_files_by_the_map_at_the_prs_head() {
    let o = origin();
    let map = r#"{"version":1,"parts":[
        {"id":"a","name":"A","caption":"","description":"","tier":"core","paths":["a/"]}],
        "actors":[],"links":[],"ignored":[],"laws":[]}"#;
    commit_file(&o.seed, MAP_FILE, map);
    git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    git(&o.main, &["fetch", "-q", "origin"]);
    git(&o.main, &["checkout", "-q", "-b", "work", "origin/trunk"]);
    // The PR adds part n, and moves a/new.rs to it, as its brief's chapters light them.
    let at_head = r#"{"version":1,"parts":[
        {"id":"a","name":"A","caption":"","description":"","tier":"core","paths":["a/"]},
        {"id":"n","name":"N","caption":"","description":"","tier":"core","paths":["n/","a/new.rs"]}],
        "actors":[],"links":[],"ignored":[],"laws":[]}"#;
    for (name, text) in [
        (MAP_FILE, at_head),
        ("n/one", "x\n"),
        ("a/new.rs", "x\n"),
        ("a/old.rs", "x\n"),
    ] {
        let path = o.main.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    git(&o.main, &["add", "-A"]);
    git(&o.main, &["commit", "-q", "-m", "work"]);
    let head = git(&o.main, &["rev-parse", "HEAD"]);
    let diff = |part: &str| {
        pr_diff(PrDiffArgs {
            project: o.main.display().to_string(),
            number: 7,
            head: head.clone(),
            title: "Work".into(),
            parts: Some(vec![part.into()]),
            paths: None,
            base: None,
        })
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.path)
        .collect::<Vec<_>>()
    };
    assert_eq!(diff("n"), ["a/new.rs", "n/one"]);
    assert_eq!(diff("a"), ["a/old.rs"]);
}

#[test]
fn see_the_code_of_a_step_places_files_by_the_default_branchs_map_when_the_head_has_none() {
    let o = origin();
    let map = r#"{"version":1,"parts":[
        {"id":"a","name":"A","caption":"","description":"","tier":"core","paths":["a/"]}],
        "actors":[],"links":[],"ignored":[],"laws":[]}"#;
    commit_file(&o.seed, MAP_FILE, map);
    git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    git(&o.main, &["fetch", "-q", "origin"]);
    // A PR branched before the map was born: its head has no map.
    git(&o.main, &["checkout", "-q", "-b", "work", "origin/trunk~1"]);
    std::fs::create_dir_all(o.main.join("a")).unwrap();
    commit_file(&o.main, "a/one.rs", "x\n");
    let head = git(&o.main, &["rev-parse", "HEAD"]);
    let found = pr_diff(PrDiffArgs {
        project: o.main.display().to_string(),
        number: 7,
        head,
        title: "Work".into(),
        parts: Some(vec!["a".into()]),
        paths: None,
        base: None,
    })
    .unwrap();
    let shown: Vec<_> = found.files.into_iter().map(|f| f.path).collect();
    assert_eq!(shown, ["a/one.rs"]);
}

#[test]
fn see_the_code_of_a_since_story_runs_from_the_head_it_was_read_at() {
    let o = origin();
    let map = r#"{"version":1,"parts":[
        {"id":"a","name":"A","caption":"","description":"","tier":"core","paths":["a/"]}],
        "actors":[],"links":[],"ignored":[],"laws":[]}"#;
    commit_file(&o.seed, MAP_FILE, map);
    git(&o.seed, &["push", "-q", "origin", "HEAD"]);
    git(&o.main, &["fetch", "-q", "origin"]);
    git(&o.main, &["checkout", "-q", "-b", "work", "origin/trunk"]);
    std::fs::create_dir_all(o.main.join("a")).unwrap();
    commit_file(&o.main, "a/read.rs", "x\n");
    let read_at = git(&o.main, &["rev-parse", "HEAD"]);
    commit_file(&o.main, "a/new.rs", "x\n");
    let head = git(&o.main, &["rev-parse", "HEAD"]);
    let diff = |base: Option<&str>| {
        pr_diff(PrDiffArgs {
            project: o.main.display().to_string(),
            number: 7,
            head: head.clone(),
            title: "Work".into(),
            parts: Some(vec!["a".into()]),
            paths: None,
            base: base.map(String::from),
        })
        .unwrap()
        .files
        .into_iter()
        .map(|f| f.path)
        .collect::<Vec<_>>()
    };
    assert_eq!(diff(None), ["a/new.rs", "a/read.rs"]);
    assert_eq!(diff(Some(&read_at)), ["a/new.rs"]);
}

#[test]
fn a_version_1_map_is_stored_and_sent_as_version_2() {
    let (dir, main) = repo();
    let old = r#"{"version":1,"parts":[{"id":"core","name":"Core","caption":"c","description":"Plain Rust.","tier":"core","paths":["core/"]}],"actors":[],"links":[],"ignored":[],"laws":[]}"#;
    commit_file(dir.path(), MAP_FILE, old);
    let state = read_map(Path::new(&main));
    assert_eq!(state.status, Status::Loaded);
    let sent: serde_json::Value = serde_json::from_str(&state.source.unwrap()).unwrap();
    assert_eq!(sent["version"], 2);
    assert_eq!(sent["parts"][0]["purpose"], "Plain Rust.");
    assert_eq!(sent["parts"][0]["holds"], serde_json::json!([]));
}

#[test]
fn an_unset_origin_head_falls_back_to_local_main_with_a_quiet_note() {
    let (dir, main) = repo();
    commit_file(dir.path(), MAP_FILE, MAP);
    let state = read_map(Path::new(&main));
    assert_eq!(state.status, Status::Loaded);
    assert_eq!(state.reference.as_deref(), Some("main"));
    assert!(state.note.is_some_and(|n| n.contains("origin/HEAD")));
}

#[test]
fn local_master_is_read_when_there_is_no_main() {
    let (dir, main) = repo();
    git(dir.path(), &["branch", "-m", "master"]);
    commit_file(dir.path(), MAP_FILE, MAP);
    assert_eq!(default_ref(Path::new(&main)).unwrap().0, "master");
}

#[test]
fn a_map_with_fields_this_build_does_not_know_still_loads() {
    let (dir, main) = repo();
    let newer =
        r#"{"version":1,"future":true,"parts":[],"actors":[],"links":[],"ignored":[],"laws":[]}"#;
    commit_file(dir.path(), MAP_FILE, newer);
    assert_eq!(read_map(Path::new(&main)).status, Status::Loaded);
}

#[test]
fn a_fetch_waits_out_its_cooldown_per_repository() {
    let fetches = Fetches::default();
    let start = Instant::now();
    let mut count = 0;
    let mut go = |repo: &str, at: Instant| {
        fetches.run(repo, at, || {
            count += 1;
            Ok(())
        })
    };
    assert_eq!(go("a", start), Fetched::Done);
    assert_eq!(go("a", start + Duration::from_secs(59)), Fetched::Skipped);
    assert_eq!(go("b", start + Duration::from_secs(59)), Fetched::Done);
    assert_eq!(go("a", start + COOLDOWN), Fetched::Done);
    assert_eq!(count, 3);
}

#[test]
fn a_fetch_in_flight_blocks_another_and_a_failure_is_reported() {
    let fetches = Fetches::default();
    let start = Instant::now();
    let inner = std::cell::Cell::new(None);
    let outer = fetches.run("a", start, || {
        // A second trigger while the first runs, long past any cooldown.
        inner.set(Some(fetches.run(
            "a",
            start + Duration::from_secs(600),
            || Ok(()),
        )));
        Err("offline".into())
    });
    assert_eq!(inner.take(), Some(Fetched::Skipped));
    assert_eq!(outer, Fetched::Failed("offline".into()));
    // A failed fetch still counts toward the cooldown, so a dead network is not hammered.
    assert_eq!(
        fetches.run("a", start + Duration::from_secs(1), || Ok(())),
        Fetched::Skipped
    );
}

#[test]
fn the_pending_pr_is_followed_after_a_fetch_that_passed_whichever_path_fetched() {
    let fetches = Fetches::default();
    let start = Instant::now();
    let followed = std::cell::Cell::new(None);
    let open = |at: Instant, fetch: Result<(), String>| {
        followed.set(None);
        fetch_and_follow(
            &fetches,
            "a",
            at,
            || fetch,
            |passed| followed.set(Some(passed)),
        );
        followed.take()
    };
    // The Map view's own fetch passes, or fails.
    assert_eq!(open(start, Ok(())), Some(true));
    assert_eq!(open(start + COOLDOWN, Err("offline".into())), Some(false));
    // A session's turn end fetched just before the Map opens, so the view's fetch waits out the cooldown: the PR is still followed,
    // told that fetch passed, so a merged PR's copy can go.
    let later = start + COOLDOWN * 2;
    assert_eq!(fetches.run("a", later, || Ok(())), Fetched::Done);
    assert_eq!(open(later + Duration::from_secs(1), Ok(())), Some(true));
    // Within the follow's own cooldown it is not asked again.
    assert_eq!(open(later + Duration::from_secs(2), Ok(())), None);

    // A fetch after a merge (`merged`) follows nothing and leaves the follow's cooldown alone: the Map open right after still asks.
    let fetches = Fetches::default();
    let followed = std::cell::Cell::new(None);
    let now = Instant::now();
    fetch(&fetches, "/no/such/repository");
    fetch_and_follow(
        &fetches,
        "/no/such/repository",
        now,
        || Ok(()),
        |passed| followed.set(Some(passed)),
    );
    assert_eq!(followed.take(), Some(false));
}

#[test]
fn user_state_round_trips_and_keeps_other_projects() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("map-state.json");
    assert_eq!(read_user(&file, "/p"), UserState::default());
    update_user(&file, "/p", |u| {
        u.positions.insert("core".into(), [10.0, 20.5]);
        u.tiers.insert("hook".into(), "black-box".into());
    })
    .unwrap();
    update_user(&file, "/q", |u| {
        u.positions.insert("x".into(), [1.0, 2.0]);
    })
    .unwrap();
    let p = read_user(&file, "/p");
    assert_eq!(p.positions["core"], [10.0, 20.5]);
    assert_eq!(p.tiers["hook"], "black-box");
    assert_eq!(read_user(&file, "/q").positions.len(), 1);
}

#[test]
fn a_projects_view_is_saved_beside_its_positions_and_read_back_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("map-state.json");
    update_user(&file, "/p", |u| {
        u.positions.insert("core".into(), [10.0, 20.0]);
    })
    .unwrap();
    let view = View {
        scale: 0.75,
        x: -120.5,
        y: 40.0,
    };
    save_view(&file, "/p", view).unwrap();
    // A restart keeps nothing in memory: the view is read back from the file alone.
    let read = read_user(&file, "/p");
    assert_eq!(read.view, Some(view));
    assert_eq!(read.positions["core"], [10.0, 20.0], "positions are kept");
    assert_eq!(
        read_user(&file, "/q").view,
        None,
        "another project has its own view"
    );
    let moved = View { scale: 2.0, ..view };
    save_view(&file, "/p", moved).unwrap();
    assert_eq!(read_user(&file, "/p").view, Some(moved));
    let bad = View {
        scale: f64::NAN,
        ..view
    };
    assert!(save_view(&file, "/p", bad).is_err());
    assert!(save_view(&file, "/p", View { scale: 0.0, ..view }).is_err());
    assert_eq!(
        read_user(&file, "/p").view,
        Some(moved),
        "a refused view changes nothing"
    );
}

#[test]
fn an_older_or_newer_state_file_is_tolerated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("map-state.json");
    // No tiers yet, and a field a later build wrote.
    std::fs::write(
        &file,
        r#"{"projects":{"/p":{"positions":{"core":[1,2]},"fog":{"core":[]}}},"version":9}"#,
    )
    .unwrap();
    assert_eq!(read_user(&file, "/p").positions["core"], [1.0, 2.0]);
    assert!(read_user(&file, "/p").tiers.is_empty());
    // A change keeps what it does not know.
    update_user(&file, "/p", |u| {
        u.tiers.insert("core".into(), "interface".into());
    })
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("\"fog\"") && text.contains("\"version\""),
        "{text}"
    );
}

#[test]
fn only_a_lookup_that_turns_a_task_merged_is_a_transition() {
    let facts = |merged| {
        Some(studio_core::TaskFacts {
            merged,
            ..Default::default()
        })
    };
    assert!(became_merged(facts(false), facts(true)));
    // First seen merged (a restart), still merged, or not merged: no fetch.
    assert!(!became_merged(None, facts(true)));
    assert!(!became_merged(facts(true), facts(true)));
    assert!(!became_merged(facts(false), facts(false)));
    assert!(!became_merged(facts(false), None));
}

#[test]
fn a_panicking_fetch_does_not_leave_the_repository_locked() {
    let fetches = Fetches::default();
    let now = Instant::now();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        fetches.run("/p", now, || panic!("git panicked"))
    }));
    assert!(panicked.is_err());
    assert_eq!(fetches.run("/p", now + COOLDOWN, || Ok(())), Fetched::Done);
}

#[test]
fn a_file_that_does_not_parse_is_an_empty_state() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("map-state.json");
    std::fs::write(&file, "{ broken").unwrap();
    assert_eq!(read_user(&file, "/p"), UserState::default());
}
