use super::*;

fn chapter(pr: u64, chapter: u32) -> About {
    About {
        pr: Some(pr),
        chapter: Some(chapter),
        part: None,
    }
}

fn lines(file: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn an_explain_back_line_and_its_mark_are_kept_and_counted_and_anything_else_with_them_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("stop-rules.jsonl");
    let day = "2026-11-23";
    let part = || About {
        part: Some("shell".into()),
        ..About::default()
    };
    let line = |l: &str| Given {
        line: Some(l.into()),
        ..Given::default()
    };
    let mark = |m: Mark| Given {
        mark: Some(m),
        ..Given::default()
    };
    bar_in(
        &file,
        Kind::ExplainBackGiven,
        "/p",
        day,
        chapter(7, 0),
        line(" The shell keeps fog until I read it. "),
    )
    .unwrap();
    for m in [Mark::GotIt, Mark::Partly, Mark::Missed] {
        bar_in(&file, Kind::ExplainBackMarked, "/p", day, part(), mark(m)).unwrap();
    }
    let kept = lines(&file);
    assert_eq!(
        (&kept[0]["kind"], &kept[0]["line"], &kept[0]["pr"]),
        (
            &serde_json::json!("explain-back-given"),
            &serde_json::json!("The shell keeps fog until I read it."),
            &serde_json::json!(7)
        )
    );
    assert_eq!(
        kept[1..]
            .iter()
            .map(|l| l["mark"].clone())
            .collect::<Vec<_>>(),
        [
            serde_json::json!("got-it"),
            serde_json::json!("partly"),
            serde_json::json!("missed")
        ]
    );
    assert_eq!(kept[1]["part"], "shell");
    let facts = stop_rules::facts(&stop_rules::read(&file), "/p", 0);
    assert_eq!(facts.counts.get(&Kind::ExplainBackGiven), Some(&1));
    assert_eq!(facts.counts.get(&Kind::ExplainBackMarked), Some(&3));

    let other = dir.path().join("other.jsonl");
    for (kind, given) in [
        (Kind::ExplainBackGiven, line("   ")),
        (Kind::ExplainBackGiven, line(&"x".repeat(LINE_MAX + 1))),
        (Kind::ExplainBackGiven, Given::default()),
        (Kind::ExplainBackMarked, Given::default()),
        (
            Kind::ExplainBackMarked,
            Given {
                line: Some("a line".into()),
                ..mark(Mark::Partly)
            },
        ),
    ] {
        let why = format!("{kind:?} {given:?}");
        assert!(
            bar_in(&other, kind, "/p", day, part(), given).is_err(),
            "{why}"
        );
    }
    assert!(lines(&other).is_empty());
}

#[test]
fn the_removed_rating_and_note_kinds_are_refused_and_nothing_is_kept() {
    // #448 removed the bar's rating and note (spec #443): their kinds, as the window would send them, are refused.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("stop-rules.jsonl");
    for kind in [
        "rating-offered",
        "rating-given",
        "rating-skipped",
        "note-given",
    ] {
        let parsed: Kind = serde_json::from_value(serde_json::json!(kind)).unwrap();
        assert!(
            bar_in(
                &file,
                parsed,
                "/p",
                "2026-11-24",
                chapter(7, 0),
                Given::default()
            )
            .is_err(),
            "{kind}"
        );
    }
    assert!(lines(&file).is_empty());
}

#[test]
fn a_line_the_bar_does_not_give_is_refused_and_nothing_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("stop-rules.jsonl");
    let day = "2026-11-23";
    let said = || Given {
        line: Some("It keeps fog.".into()),
        ..Given::default()
    };
    let part = || About {
        part: Some("shell".into()),
        ..About::default()
    };
    for (kind, about) in [
        // Not one of the bar's kinds.
        (Kind::Scrub, chapter(7, 0)),
        // No story, half a chapter's key, or both a chapter and a part.
        (Kind::ExplainBackGiven, About::default()),
        (
            Kind::ExplainBackGiven,
            About {
                pr: Some(7),
                ..About::default()
            },
        ),
        (
            Kind::ExplainBackGiven,
            About {
                part: Some("shell".into()),
                ..chapter(7, 0)
            },
        ),
        (
            Kind::ExplainBackGiven,
            About {
                part: Some(String::new()),
                ..About::default()
            },
        ),
    ] {
        let why = format!("{kind:?} {about:?}");
        assert!(
            bar_in(&file, kind, "/p", day, about, said()).is_err(),
            "{why}"
        );
    }
    assert!(lines(&file).is_empty());
    assert!(bar_in(&file, Kind::ExplainBackGiven, "/p", day, part(), said()).is_ok());
}
