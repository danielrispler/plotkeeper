use super::*;
use studio_test_support::{git, repo};

fn event(day: &str, kind: Kind, project: Option<&str>) -> Event {
    Event {
        at: 1,
        day: day.into(),
        kind,
        project: project.map(String::from),
        pr: None,
        module: None,
    }
}

fn brief(day: &str, kind: Kind, pr: u64) -> Event {
    Event {
        pr: Some(pr),
        ..event(day, kind, Some("/p"))
    }
}

#[test]
fn an_append_after_a_torn_line_starts_a_new_line() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    std::fs::write(&file, "{\"at\":1,\"day\":\"2026-1").unwrap();
    let next = event("2026-11-02", Kind::Know, Some("/p"));
    append(&file, &next).unwrap();
    assert_eq!(read(&file), [next]);
}

#[test]
fn merged_prs_count_from_the_projects_own_first_event() {
    let at = |at, project| Event {
        at,
        ..event("2026-11-01", Kind::MapOpen, project)
    };
    let events = [at(5, None), at(10, Some("/a")), at(20, Some("/b"))];
    assert_eq!(began(&events, "/b"), Some(20));
    assert_eq!(began(&events, "/a"), Some(10));
    assert_eq!(began(&events, "/c"), None);
}

#[test]
fn a_project_with_no_events_of_its_own_counts_from_the_logs_first_event() {
    let at = |at, project| Event {
        at,
        ..event("2026-11-01", Kind::Session, project)
    };
    let events = [at(30, None), at(5, None), at(10, Some("/a"))];
    assert_eq!(counted_from(&events, "/c"), Some(5));
    assert_eq!(counted_from(&events, "/a"), Some(10));
    assert_eq!(counted_from(&[], "/a"), None);
}

#[test]
fn events_round_trip_through_the_file_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data").join(LOG_FILE);
    let all = [
        event("2026-11-01", Kind::Session, None),
        event("2026-11-01", Kind::MapOpen, Some("/p")),
        event("2026-11-02", Kind::Know, Some("/p")),
    ];
    for e in &all {
        append(&file, e).unwrap();
    }
    assert_eq!(read(&file), all);
}

#[test]
fn a_missing_older_or_newer_log_is_tolerated() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    assert!(read(&file).is_empty());
    // An event with no project, a kind and a field of a later build, a torn line and a blank one.
    std::fs::write(
        &file,
        concat!(
            "{\"at\":1,\"day\":\"2026-11-01\",\"kind\":\"session\"}\n",
            "{\"at\":2,\"day\":\"2026-11-01\",\"kind\":\"pr-brief\",\"project\":\"/p\",\"extra\":1}\n",
            "{\"at\":3,\"day\":\"2026-11-01\",\"kind\":\"scrub\",\"project\":\"/p\",\"later\":true}\n",
            "{\"at\":4,\"day\":\"2026-1",
            "\n\n",
        ),
    )
    .unwrap();
    let events = read(&file);
    assert_eq!(events.len(), 3);
    assert_eq!(events[1].kind, Kind::Other);
    let found = facts(&events, "/p", 0);
    assert_eq!(
        (found.active_days, found.scrubs, found.engaged_days),
        (1, 1, 1)
    );
    // A later append goes on after a torn last line without losing the lines before it.
    append(&file, &event("2026-11-02", Kind::Proof, Some("/p"))).unwrap();
    assert!(read(&file).iter().any(|e| e.kind == Kind::Proof));
}

#[test]
fn days_are_counted_by_what_makes_a_day_active_and_per_project() {
    let events = [
        event("2026-11-01", Kind::Session, None),
        event("2026-11-01", Kind::Session, None),
        event("2026-11-02", Kind::MapOpen, Some("/p")),
        event("2026-11-02", Kind::MapOpen, Some("/p")),
        event("2026-11-03", Kind::MapOpen, Some("/other")),
        // Used, but neither a session nor a map open: no active day of its own.
        event("2026-11-05", Kind::Scrub, Some("/p")),
    ];
    let found = facts(&events, "/p", 7);
    assert_eq!(found.active_days, 3);
    assert_eq!(found.merged_prs, 7);
    assert_eq!(found.scrubs, 1);
    assert_eq!(facts(&events, "/other", 0).scrubs, 0);
}

#[test]
fn engaged_days_are_counted_from_the_log_in_a_data_folder() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("data").join(LOG_FILE);
    let briefed = |day, kind| Event {
        pr: Some(7),
        ..event(day, kind, Some("/p"))
    };
    for e in [
        // Open, with no entered action: not engaged.
        event("2026-11-01", Kind::Session, None),
        event("2026-11-01", Kind::MapOpen, Some("/p")),
        // A sheet opened or a session badge pressed.
        event("2026-11-02", Kind::MapOpen, Some("/p")),
        event("2026-11-02", Kind::MapAction, Some("/p")),
        // Two entered actions on one day: one engaged day.
        event("2026-11-03", Kind::MapOpen, Some("/p")),
        event("2026-11-03", Kind::Scrub, Some("/p")),
        event("2026-11-03", Kind::Proof, Some("/p")),
        // A brief entered.
        event("2026-11-04", Kind::Session, None),
        briefed("2026-11-04", Kind::BriefOpened),
        // Another project's action.
        event("2026-11-05", Kind::MapOpen, Some("/other")),
        event("2026-11-05", Kind::MapAction, Some("/other")),
        // Seen, not entered.
        event("2026-11-06", Kind::MapOpen, Some("/p")),
        briefed("2026-11-06", Kind::BriefOffered),
        briefed("2026-11-06", Kind::UnconfirmedSeen),
    ] {
        append(&file, &e).unwrap();
    }
    // A line of the previous build: Home's mark seen reads, and counts for nothing.
    let mut log = std::fs::read_to_string(&file).unwrap();
    log.push_str("{\"at\":1,\"day\":\"2026-11-07\",\"kind\":\"mark-seen\",\"project\":\"/p\"}\n");
    std::fs::write(&file, log).unwrap();
    let found = facts_in(&file, "/p");
    assert_eq!((found.active_days, found.engaged_days), (6, 3));
    assert_eq!(facts_in(&file, "/other").engaged_days, 1);
}

#[test]
fn uses_are_counted_and_a_dismissal_is_per_project() {
    let events = [
        event("2026-11-01", Kind::Proof, Some("/p")),
        event("2026-11-01", Kind::Proof, Some("/p")),
        event("2026-11-01", Kind::Know, Some("/p")),
        event("2026-11-01", Kind::Dismissed, Some("/p")),
    ];
    let found = facts(&events, "/p", 0);
    assert_eq!((found.proofs, found.knows), (2, 1));
    assert!(!found.dismissed.is_empty());
    assert!(facts(&events, "/q", 0).dismissed.is_empty());
}

#[test]
fn a_dismissal_names_its_modules_and_an_older_one_names_the_modules_of_its_time() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    dismiss_in(
        &file,
        &["/p".into(), "/q".into()],
        &["history".into()],
        "2026-11-20",
    )
    .unwrap();
    dismiss_in(&file, &["/p".into()], &["refresher".into()], "2026-12-20").unwrap();
    let events = read(&file);
    assert_eq!(facts(&events, "/p", 0).dismissed, ["history", "refresher"]);
    assert_eq!(facts(&events, "/q", 0).dismissed, ["history"]);
    // A project-wide dismissal of the build before per-module dismissal.
    let mut log = std::fs::read_to_string(&file).unwrap();
    log.push_str("{\"at\":1,\"day\":\"2026-11-07\",\"kind\":\"dismissed\",\"project\":\"/q\"}\n");
    std::fs::write(&file, log).unwrap();
    assert_eq!(
        facts(&read(&file), "/q", 0).dismissed,
        ["briefs", "history", "map", "proof", "skips"]
    );
}

#[test]
fn a_part_catch_up_opened_or_completed_is_an_entered_action_counted_apart_from_i_know_this() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    let line = |day: &str, kind: &str| {
        format!("{{\"at\":1,\"day\":\"{day}\",\"kind\":\"{kind}\",\"project\":\"/p\"}}\n")
    };
    let log = [
        line("2026-11-01", "map-open"),
        line("2026-11-01", "catch-up-offered"),
        line("2026-11-02", "map-open"),
        line("2026-11-02", "catch-up-opened"),
        line("2026-11-03", "map-open"),
        line("2026-11-03", "catch-up-done"),
        line("2026-11-03", "know"),
    ]
    .concat();
    std::fs::write(&file, log).unwrap();
    let found = facts_in(&file, "/p");
    assert_eq!(found.engaged_days, 2, "offered is seen, not entered");
    let counts = serde_json::to_value(&found).unwrap()["counts"].clone();
    assert_eq!(counts["catch-up-offered"], 1);
    assert_eq!(counts["catch-up-opened"], 1);
    assert_eq!(counts["catch-up-done"], 1);
    assert_eq!(counts["know"], 1);
}

#[test]
fn the_refresher_and_an_entry_from_a_stale_part_are_counted_and_each_makes_an_engaged_day() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    let lines = [
        r#"{"at":1,"day":"2026-11-01","kind":"map-open","project":"/p"}"#,
        r#"{"at":2,"day":"2026-11-01","kind":"refresher-entered","project":"/p"}"#,
        r#"{"at":3,"day":"2026-11-02","kind":"map-open","project":"/p"}"#,
        r#"{"at":4,"day":"2026-11-02","kind":"refresher-entered","project":"/p"}"#,
        r#"{"at":5,"day":"2026-11-02","kind":"stale-entered","project":"/p"}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let json = serde_json::to_value(facts_in(&file, "/p")).unwrap();
    assert_eq!(json["counts"]["refresher-entered"], 2);
    assert_eq!(json["counts"]["stale-entered"], 1);
    assert_eq!(json["engaged_days"], 2);
}

#[test]
fn a_back_a_law_run_started_is_counted_and_makes_an_engaged_day() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    let lines = [
        r#"{"at":1,"day":"2026-11-01","kind":"map-open","project":"/p"}"#,
        r#"{"at":2,"day":"2026-11-01","kind":"back-law-started","project":"/p"}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let json = serde_json::to_value(facts_in(&file, "/p")).unwrap();
    assert_eq!(json["counts"]["back-law-started"], 1);
    assert_eq!(json["engaged_days"], 1);
}

#[test]
fn guard_with_a_law_counts_its_offers_and_its_runs_started_and_a_run_makes_an_engaged_day() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    let lines = [
        r#"{"at":1,"day":"2026-11-01","kind":"map-open","project":"/p"}"#,
        r#"{"at":1,"day":"2026-11-02","kind":"map-open","project":"/p"}"#,
        r#"{"at":1,"day":"2026-11-01","kind":"guard-offered","project":"/p","pr":7}"#,
        r#"{"at":2,"day":"2026-11-01","kind":"guard-offered","project":"/p","pr":8}"#,
        r#"{"at":3,"day":"2026-11-02","kind":"guard-started","project":"/p","pr":8}"#,
    ];
    std::fs::write(&file, lines.join("\n") + "\n").unwrap();
    let json = serde_json::to_value(facts_in(&file, "/p")).unwrap();
    assert_eq!(json["counts"]["guard-offered"], 2);
    assert_eq!(json["counts"]["guard-started"], 1);
    assert_eq!(json["engaged_days"], 1, "an offer is seen, not entered");
}

#[test]
fn every_kind_is_counted_per_project_and_the_first_day_is_kept() {
    let at = |at, day, kind, project| Event {
        at,
        ..event(day, kind, project)
    };
    let events = [
        at(5, "2026-10-30", Kind::Session, None),
        at(20, "2026-11-02", Kind::Scrub, Some("/p")),
        at(10, "2026-11-01", Kind::BriefOffered, Some("/p")),
        at(30, "2026-11-03", Kind::BriefOffered, Some("/p")),
        at(40, "2026-11-04", Kind::BriefOffered, Some("/q")),
    ];
    let found = facts(&events, "/p", 0);
    assert_eq!(found.counts.get(&Kind::BriefOffered), Some(&2));
    assert_eq!(found.counts.get(&Kind::Scrub), Some(&1));
    assert_eq!(found.counts.get(&Kind::Session), None);
    assert_eq!(found.began.as_deref(), Some("2026-11-01"));
    assert_eq!(facts(&events, "/r", 0).began, None);
    let json = serde_json::to_value(&found).unwrap();
    assert_eq!(json["counts"]["brief-offered"], 2);
}

#[test]
fn a_merged_pr_is_a_squash_or_a_merge_commit_on_the_first_parent_line() {
    let (dir, path) = repo();
    let at = dir.path();
    for subject in [
        "Studio: a thing (#12)",
        "Merge pull request #13 from a/b",
        "A plain commit",
        "Mentions (#14) mid-line",
        "Ends in a word (#x)",
    ] {
        git(at, &["commit", "-q", "--allow-empty", "-m", subject]);
    }
    // A start of 0 (or any small number) is every commit; git's `--since=0` reads it as "now", so a commit older than the
    // second `git log` runs in would not count.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    assert_eq!(merged_numbers_since(Path::new(&path), 0).len(), 2);
    // Nothing merged after a future start.
    assert_eq!(
        merged_numbers_since(Path::new(&path), now() + 3600).len(),
        0
    );
}

#[test]
fn a_start_is_inclusive_of_the_commits_own_second() {
    let (dir, path) = repo();
    let stamp = "1700000000 +0000";
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=S",
            "-c",
            "user.email=s@e.com",
            "commit",
            "-q",
        ])
        .args(["--allow-empty", "-m", "Studio: a thing (#5)"])
        .env("GIT_COMMITTER_DATE", stamp)
        .env("GIT_AUTHOR_DATE", stamp)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .current_dir(dir.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(merged_numbers_since(Path::new(&path), 1_700_000_000), [5]);
    assert!(merged_numbers_since(Path::new(&path), 1_700_000_001).is_empty());
}

#[test]
fn a_brief_is_offered_once_per_pr_however_often_its_row_is_seen() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(LOG_FILE);
    assert!(append_once(&file, &brief("2026-11-01", Kind::BriefOffered, 7)).unwrap());
    assert!(!append_once(&file, &brief("2026-11-02", Kind::BriefOffered, 7)).unwrap());
    assert!(append_once(&file, &brief("2026-11-02", Kind::BriefOffered, 8)).unwrap());
    assert_eq!(read(&file).len(), 2);
}

#[test]
fn offers_of_one_brief_at_the_same_moment_log_one_line() {
    for _ in 0..20 {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(LOG_FILE);
        let start = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    start.wait();
                    append_once(&file, &brief("2026-11-01", Kind::BriefOffered, 7)).unwrap();
                });
            }
        });
        assert_eq!(read(&file).len(), 1);
    }
}

#[test]
fn brief_use_counts_offered_and_opened_among_merged_prs_only() {
    let events = [
        brief("2026-11-01", Kind::BriefOffered, 1),
        brief("2026-11-01", Kind::BriefOffered, 2),
        brief("2026-11-01", Kind::BriefOffered, 3),
        brief("2026-11-01", Kind::BriefOffered, 99),
        brief("2026-11-01", Kind::BriefOpened, 2),
        brief("2026-11-02", Kind::BriefOpened, 2),
        brief("2026-11-02", Kind::BriefOpened, 99),
        // Opened with no offer logged (the log began after the offer): not counted as having had a brief.
        brief("2026-11-02", Kind::BriefOpened, 4),
    ];
    let merged: BTreeSet<u64> = [1, 2, 3, 4].into();
    assert_eq!(brief_use(&events, "/p", &merged), (3, 1));
    assert_eq!(brief_use(&events, "/other", &merged), (0, 0));
}

#[test]
fn chapters_done_are_counted_per_project_beside_knows() {
    let events = [
        brief("2026-11-01", Kind::ChapterDone, 1),
        brief("2026-11-01", Kind::ChapterDone, 1),
        event("2026-11-01", Kind::Know, Some("/p")),
        Event {
            project: Some("/q".into()),
            ..brief("2026-11-01", Kind::ChapterDone, 1)
        },
    ];
    let found = facts(&events, "/p", 0);
    assert_eq!((found.chapters_done, found.knows), (2, 1));
}

#[test]
fn a_merge_subject_names_its_pr() {
    assert_eq!(merge_number("Studio: a thing (#12)"), Some(12));
    assert_eq!(merge_number("Merge pull request #13 from a/b"), Some(13));
    assert_eq!(merge_number("A plain commit"), None);
    assert_eq!(merge_number("Ends in a word (#x)"), None);
}
