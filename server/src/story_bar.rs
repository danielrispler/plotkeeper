//! The end-of-story bar (spec danielrispler/plotkeeper#378, ticket #393): what Daniel gives at the end of a PR brief's chapter or of a part
//! catch-up, on a Core part an explain-back line and its mark against the gist (#397), is kept as one line of the stop-rule log
//! (`stop_rules.rs`) in the data folder, keyed by project and by PR and chapter or by part. The bar's 1 to 5 rating and note are gone
//! (#448, spec #443), and their kinds are refused. The log is its only store: nothing is ever read back to the page. A line is an
//! ordinary `Event` with the bar's fields beside it, which the stop-rule counts read as its kind alone.

use crate::stop_rules::{self, Event, Kind};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The longest explain-back line kept, in characters; a longer one is refused.
pub const LINE_MAX: usize = 300;

/// What story a bar ends: a PR brief's chapter (`pr` and `chapter`), or a part catch-up (`part`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct About {
    #[serde(default)]
    pub pr: Option<u64>,
    #[serde(default)]
    pub chapter: Option<u32>,
    #[serde(default)]
    pub part: Option<String>,
}

/// What the bar was given with one kind: an explain-back line or its mark (#397); at most the one its kind carries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Given {
    #[serde(default)]
    pub line: Option<String>,
    #[serde(default)]
    pub mark: Option<Mark>,
}

/// How Daniel marked his explain-back line against the gist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mark {
    GotIt,
    Partly,
    Missed,
}

/// One line of the log from the bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    #[serde(flatten)]
    pub event: Event,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    /// Daniel's explain-back line (#397).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<Mark>,
}

/// Logs what the bar of `about` in `project` was given: `kind` is one of the bar's kinds; an `explain-back-given` carries Daniel's line
/// and an `explain-back-marked` its mark, and neither carries the other's.
pub fn story_bar(
    kind: Kind,
    project: String,
    day: String,
    about: About,
    given: Given,
) -> Result<(), String> {
    bar_in(&stop_rules::log_file()?, kind, &project, &day, about, given)
}

/// `story_bar`, into the log `file`.
fn bar_in(
    file: &Path,
    kind: Kind,
    project: &str,
    day: &str,
    about: About,
    given: Given,
) -> Result<(), String> {
    let Given { line: said, mark } = given;
    if !matches!(kind, Kind::ExplainBackGiven | Kind::ExplainBackMarked) {
        return Err("not one of the end-of-story bar's kinds".into());
    }
    let chapter = about.pr.is_some() && about.chapter.is_some() && about.part.is_none();
    let part = about.pr.is_none()
        && about.chapter.is_none()
        && about.part.as_deref().is_some_and(|p| !p.is_empty());
    if !chapter && !part {
        return Err("a bar ends a PR brief's chapter or a part catch-up".into());
    }
    let said = said.map(|l| l.trim().to_string());
    let fits = |l: &String| !l.is_empty() && l.chars().count() <= LINE_MAX;
    if (kind == Kind::ExplainBackGiven) != said.as_ref().is_some_and(fits)
        || (kind != Kind::ExplainBackGiven && said.is_some())
    {
        return Err(format!(
            "an explain-back line is 1 to {LINE_MAX} characters, given only as one"
        ));
    }
    if (kind == Kind::ExplainBackMarked) != mark.is_some() {
        return Err("a mark is got it, partly or missed, given only as a mark".into());
    }
    let line = Line {
        event: Event {
            at: studio_workbench::clock::now_ms() / 1000,
            day: day.into(),
            kind,
            project: Some(project.into()),
            pr: about.pr,
            module: None,
        },
        chapter: about.chapter,
        part: about.part,
        line: said,
        mark,
    };
    stop_rules::append_line(file, &line)
}

#[cfg(test)]
mod tests {
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
    fn an_explain_back_line_and_its_mark_are_kept_and_counted_and_anything_else_with_them_refused()
    {
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
}
