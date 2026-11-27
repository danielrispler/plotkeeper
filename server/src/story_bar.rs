//! The end-of-story bar (spec danielrispler/plotkeeper#378, ticket #393): what Daniel gives at the end of a PR brief's chapter or of a part
//! catch-up, on a Core part an explain-back line and its mark against the gist (#397), is kept as one line of the stop-rule log
//! (`stop_rules.rs`) in the data folder, keyed by project and by PR and chapter or by part. The bar's 1 to 5 rating and note are gone
//! (#448, spec #443), and their kinds are refused. The log is its only store: nothing is ever read back to the page. A line is an
//! ordinary `Event` with the bar's fields beside it, which the stop-rule counts read as its kind alone.

use crate::stop_rules::{self, Event, Kind};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The longest explain-back line kept, in characters; a longer one is refused.
pub(crate) const LINE_MAX: usize = 300;

/// What story a bar ends: a PR brief's chapter (`pr` and `chapter`), or a part catch-up (`part`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct About {
    #[serde(default)]
    pub pr: Option<u64>,
    #[serde(default)]
    pub chapter: Option<u32>,
    #[serde(default)]
    pub part: Option<String>,
}

/// What the bar was given with one kind: an explain-back line or its mark (#397); at most the one its kind carries.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub(crate) struct Given {
    #[serde(default)]
    pub line: Option<String>,
    #[serde(default)]
    pub mark: Option<Mark>,
}

/// How Daniel marked his explain-back line against the gist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Mark {
    GotIt,
    Partly,
    Missed,
}

/// One line of the log from the bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Line {
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
pub(crate) fn story_bar(
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
mod tests;
