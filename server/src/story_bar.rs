//! The end-of-story bar (spec danielrispler/plotkeeper#378, ticket #393): what Daniel gives at the end of a PR brief's chapter or of a part
//! catch-up, an optional 1 to 5 rating of the explanation, an optional short note, or a skip, is kept as one line of the stop-rule log
//! (`stop_rules.rs`) in the data folder, keyed by project and by PR and chapter or by part. The log is its only store: nothing is ever
//! read back to the page, so no rating is shown again. A line is an ordinary `Event` with the bar's fields beside it, which the stop-rule
//! counts read as its kind alone.

use crate::stop_rules::{self, Event, Kind};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

/// The longest note kept, in characters; a longer one is refused, not cut.
pub const NOTE_MAX: usize = 500;

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

/// One line of the log from the bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    #[serde(flatten)]
    pub event: Event,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rating: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Logs what the bar of `about` in `project` was given: `kind` is one of the bar's kinds; a `rating-given` carries a rating of 1 to 5
/// and a `note-given` a note, and no other kind carries either.
pub fn story_bar(
    kind: Kind,
    project: String,
    day: String,
    about: About,
    rating: Option<u8>,
    note: Option<String>,
) -> Result<(), String> {
    bar_in(
        &stop_rules::log_file()?,
        kind,
        &project,
        &day,
        about,
        rating,
        note,
    )
}

/// `story_bar`, into the log `file`.
fn bar_in(
    file: &Path,
    kind: Kind,
    project: &str,
    day: &str,
    about: About,
    rating: Option<u8>,
    note: Option<String>,
) -> Result<(), String> {
    if !matches!(
        kind,
        Kind::RatingOffered | Kind::RatingGiven | Kind::RatingSkipped | Kind::NoteGiven
    ) {
        return Err("not one of the end-of-story bar's kinds".into());
    }
    let chapter = about.pr.is_some() && about.chapter.is_some() && about.part.is_none();
    let part = about.pr.is_none()
        && about.chapter.is_none()
        && about.part.as_deref().is_some_and(|p| !p.is_empty());
    if !chapter && !part {
        return Err("a bar ends a PR brief's chapter or a part catch-up".into());
    }
    if (kind == Kind::RatingGiven) != rating.is_some_and(|r| (1..=5).contains(&r))
        || (kind != Kind::RatingGiven && rating.is_some())
    {
        return Err("a rating is 1 to 5, given only as a rating".into());
    }
    let note = note.map(|n| n.trim().to_string());
    let fits = |n: &String| !n.is_empty() && n.chars().count() <= NOTE_MAX;
    if (kind == Kind::NoteGiven) != note.as_ref().is_some_and(fits)
        || (kind != Kind::NoteGiven && note.is_some())
    {
        return Err(format!(
            "a note is 1 to {NOTE_MAX} characters, given only as a note"
        ));
    }
    let line = Line {
        event: Event {
            at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            day: day.into(),
            kind,
            project: Some(project.into()),
            pr: about.pr,
            module: None,
        },
        chapter: about.chapter,
        part: about.part,
        rating,
        note,
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
    fn a_rating_a_note_and_a_skip_are_kept_in_the_log_keyed_by_project_and_story() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data").join("stop-rules.jsonl");
        let day = "2026-11-23";
        bar_in(
            &file,
            Kind::RatingOffered,
            "/p",
            day,
            chapter(7, 1),
            None,
            None,
        )
        .unwrap();
        bar_in(
            &file,
            Kind::RatingGiven,
            "/p",
            day,
            chapter(7, 1),
            Some(4),
            None,
        )
        .unwrap();
        bar_in(
            &file,
            Kind::NoteGiven,
            "/p",
            day,
            chapter(7, 1),
            None,
            Some("The second step lost me.".into()),
        )
        .unwrap();
        let part = About {
            part: Some("shell".into()),
            ..About::default()
        };
        bar_in(&file, Kind::RatingSkipped, "/p", day, part, None, None).unwrap();
        let kept = lines(&file);
        assert_eq!(kept.len(), 4);
        assert_eq!(kept[0]["kind"], "rating-offered");
        assert_eq!(
            (
                &kept[1]["kind"],
                &kept[1]["project"],
                &kept[1]["pr"],
                &kept[1]["chapter"],
                &kept[1]["rating"]
            ),
            (
                &serde_json::json!("rating-given"),
                &serde_json::json!("/p"),
                &serde_json::json!(7),
                &serde_json::json!(1),
                &serde_json::json!(4)
            )
        );
        assert_eq!(kept[2]["note"], "The second step lost me.");
        assert_eq!(
            (&kept[3]["kind"], &kept[3]["part"]),
            (
                &serde_json::json!("rating-skipped"),
                &serde_json::json!("shell")
            )
        );
        assert!(kept[3].get("pr").is_none());
        // The stop-rule counts read the bar's lines by their kind.
        let facts = stop_rules::facts(&stop_rules::read(&file), "/p", 0);
        assert_eq!(facts.counts.get(&Kind::RatingGiven), Some(&1));
        assert_eq!(facts.counts.get(&Kind::RatingSkipped), Some(&1));
        assert_eq!(facts.counts.get(&Kind::NoteGiven), Some(&1));
    }

    #[test]
    fn a_line_the_bar_does_not_give_is_refused_and_nothing_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stop-rules.jsonl");
        let day = "2026-11-23";
        let part = || About {
            part: Some("shell".into()),
            ..About::default()
        };
        for (kind, about, rating, note) in [
            // Not one of the bar's kinds.
            (Kind::Scrub, chapter(7, 0), None, None),
            // A rating outside 1 to 5, or missing.
            (Kind::RatingGiven, chapter(7, 0), Some(0), None),
            (Kind::RatingGiven, chapter(7, 0), Some(6), None),
            (Kind::RatingGiven, chapter(7, 0), None, None),
            // A note that is empty or too long, or a note or rating on another kind.
            (Kind::NoteGiven, chapter(7, 0), None, Some("  ".to_string())),
            (
                Kind::NoteGiven,
                chapter(7, 0),
                None,
                Some("x".repeat(NOTE_MAX + 1)),
            ),
            (Kind::RatingSkipped, chapter(7, 0), Some(3), None),
            (
                Kind::RatingOffered,
                chapter(7, 0),
                None,
                Some("hi".to_string()),
            ),
            // No story, half a chapter's key, or both a chapter and a part.
            (Kind::RatingOffered, About::default(), None, None),
            (
                Kind::RatingOffered,
                About {
                    pr: Some(7),
                    ..About::default()
                },
                None,
                None,
            ),
            (
                Kind::RatingOffered,
                About {
                    part: Some("shell".into()),
                    ..chapter(7, 0)
                },
                None,
                None,
            ),
            (
                Kind::RatingOffered,
                About {
                    part: Some(String::new()),
                    ..About::default()
                },
                None,
                None,
            ),
        ] {
            let why = format!("{kind:?} {about:?} {rating:?} {note:?}");
            assert!(
                bar_in(&file, kind, "/p", day, about, rating, note).is_err(),
                "{why}"
            );
        }
        assert!(lines(&file).is_empty());
        assert!(bar_in(&file, Kind::RatingOffered, "/p", day, part(), None, None).is_ok());
    }
}
