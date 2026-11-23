//! Pre-flight (spec danielrispler/plotkeeper#378, ticket #394): the parts a piece of work will likely cross, framed on the map before
//! it starts. The pre-flight is stored per project in the data folder, with the issues each PR closes and their parents, noted when
//! the PR's brief is written (`pr_briefs::source_of`), so opening a brief of a matched PR carries "you expected X, Y; it touched X, Z".
//!
//! The prediction is the understanding crate's `pre_flight::predict` with the PR briefs' writer, which has no tools; the issue's body
//! goes only to it (spec #378, "Pre-flight").

use crate::App;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use studio_understanding::pr_brief::{BriefWriter, PrMeta, StoredBrief};
use studio_understanding::pre_flight::{Closing, Expected, Input, PreFlight};

/// What the store's file holds: the project's pre-flight, and the issues each PR closes with their parents.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    current: Option<PreFlight>,
    #[serde(default)]
    closing: BTreeMap<u64, Vec<Closing>>,
}

/// One project's pre-flight, in a file of its own.
pub struct Store {
    file: PathBuf,
}

impl Store {
    pub fn at(file: PathBuf) -> Self {
        Self { file }
    }

    fn read(&self) -> Saved {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, saved: &Saved) -> Result<(), String> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
        let temp = self.file.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &self.file).map_err(|e| e.to_string())
    }

    /// Reads, changes and writes the file under one lock, so a note from a brief's write never loses a pre-flight set meanwhile.
    fn change(&self, edit: impl FnOnce(&mut Saved)) -> Result<(), String> {
        static LOCK: Mutex<()> = Mutex::new(());
        let _held = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut saved = self.read();
        edit(&mut saved);
        self.write(&saved)
    }

    /// The project's pre-flight, if one was run and not cleared.
    pub fn current(&self) -> Option<PreFlight> {
        self.read().current
    }

    /// Keeps `pre` as the project's pre-flight, or clears it with `None`.
    pub fn set(&self, pre: Option<PreFlight>) -> Result<(), String> {
        self.change(|saved| saved.current = pre)
    }

    /// Notes the issues PR `pr` closes, each with its parent.
    pub fn note(&self, pr: u64, closing: Vec<Closing>) -> Result<(), String> {
        self.change(|saved| {
            saved.closing.insert(pr, closing);
        })
    }

    /// The line PR `pr`'s brief carries: the pre-flight's parts against the parts `brief` touched, when the PR matched.
    pub fn expected(&self, pr: u64, brief: &StoredBrief) -> Option<Expected> {
        let saved = self.read();
        let closing = saved.closing.get(&pr)?;
        studio_understanding::pre_flight::expected(saved.current.as_ref()?, closing, brief)
    }
}

/// Asks `writer` which parts `input` will cross on the map `map_text`, and keeps the answer as the project's pre-flight in `store`. A
/// failure stores nothing, and Daniel picks the parts by hand.
pub fn predict_into(
    store: &Store,
    writer: &(dyn BriefWriter + Send + Sync),
    model: &str,
    map_text: &str,
    input: &Input,
    now: u64,
) -> Result<PreFlight, String> {
    let parts = studio_understanding::pre_flight::predict(writer, model, map_text, input)?;
    let (issue, title) = match input {
        Input::Issue { number, title, .. } => (Some(*number), title.to_string()),
        Input::Text(text) => (None, text.to_string()),
    };
    let pre = PreFlight {
        issue,
        title,
        parts,
        predicted: true,
        at: now,
    };
    store.set(Some(pre.clone()))?;
    Ok(pre)
}

/// A project's pre-flight lives in a file of its own in the data folder, named as `pr_briefs::project_path` names a project.
fn store_of(project: &str) -> Option<Store> {
    let path = crate::pr_briefs::project_path("pre-flight-by-project", project)?;
    Some(Store::at(path.with_extension("json")))
}

/// The project's pre-flight, for the Map view's payload.
pub fn current(project: &str) -> Option<PreFlight> {
    store_of(project)?.current()
}

/// The line of PR `pr`'s brief in `project`, when its PR matched the project's pre-flight.
pub fn expected(project: &str, pr: u64, brief: &StoredBrief) -> Option<Expected> {
    store_of(project)?.expected(pr, brief)
}

/// When `project` has a pre-flight of an issue, notes the issues the PR of `meta` closes with each one's parent, asked of `gh` as a
/// normal user. Called as the PR's brief is written; a failure is told to the log and leaves the brief without the line.
pub fn note_closing(project: &str, meta: &PrMeta) {
    let Some(store) = store_of(project) else {
        return;
    };
    if store.current().and_then(|pre| pre.issue).is_none() {
        return;
    }
    let closing = studio_understanding::pre_flight::closings(&meta.closing, |issue| {
        crate::ci_laws::gh(
            Path::new(project),
            &["issue", "view", &issue.to_string(), "--json", "parent"],
        )
    });
    if let Err(why) = store.note(meta.number, closing) {
        log::warn!("pre-flight: note the issues #{} closes: {why}", meta.number);
    }
}

/// An open issue the pre-flight can start from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenIssue {
    pub number: u64,
    pub title: String,
}

/// The project's open issues, newest first, as `gh` lists them.
pub fn pre_flight_issues(project: String) -> Result<Vec<OpenIssue>, String> {
    let text = crate::ci_laws::gh(
        Path::new(&project),
        &[
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title",
        ],
    )?;
    serde_json::from_str(&text).map_err(|e| format!("gh printed no issue list: {e}"))
}

/// Runs a pre-flight of open issue `issue` or of `text`, with the PR briefs' writer and model, on the map of the default branch. The
/// map view is told it changed. An error means Daniel picks the parts by hand.
pub fn pre_flight_predict(
    app: &Arc<App>,
    project: String,
    issue: Option<u64>,
    text: Option<String>,
) -> Result<PreFlight, String> {
    let writer = app
        .briefs
        .writer()
        .ok_or("no writer: plotkeeper found no `claude` to ask")?;
    let store = store_of(&project).ok_or("no data folder")?;
    let map_text = crate::map::read_map(Path::new(&project))
        .source
        .ok_or("this project has no map to predict on")?;
    let model = crate::settings::pr_briefs_model();
    let now = studio_understanding::millis();
    let pre = match (issue, text) {
        (Some(number), _) => {
            let found: serde_json::Value = serde_json::from_str(&crate::ci_laws::gh(
                Path::new(&project),
                &["issue", "view", &number.to_string(), "--json", "title,body"],
            )?)
            .map_err(|e| format!("gh printed no issue: {e}"))?;
            let input = Input::Issue {
                number,
                title: found["title"].as_str().unwrap_or(""),
                body: found["body"].as_str().unwrap_or(""),
            };
            predict_into(&store, writer.as_ref(), &model, &map_text, &input, now)?
        }
        (None, Some(text)) if !text.trim().is_empty() => {
            let input = Input::Text(text.trim());
            predict_into(&store, writer.as_ref(), &model, &map_text, &input, now)?
        }
        _ => return Err("pick an open issue or write what you are about to build".into()),
    };
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(pre)
}

/// Parts picked by hand: a pre-flight with no issue, so no brief carries its line.
pub fn pre_flight_pick(
    app: &App,
    project: String,
    parts: Vec<String>,
) -> Result<PreFlight, String> {
    let store = store_of(&project).ok_or("no data folder")?;
    let pre = PreFlight {
        issue: None,
        title: String::new(),
        parts,
        predicted: false,
        at: studio_understanding::millis(),
    };
    store.set(Some(pre.clone()))?;
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(pre)
}

/// Clears the project's pre-flight: the map frames nothing, and no later brief carries a line.
pub fn pre_flight_clear(app: &App, project: String) -> Result<(), String> {
    store_of(&project).ok_or("no data folder")?.set(None)?;
    app.events.emit("map-landed", serde_json::json!(project));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use studio_understanding::pr_brief::Shape;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::at(dir.path().join("pre-flight.json"));
        (dir, store)
    }

    fn brief(pr: u64) -> StoredBrief {
        StoredBrief {
            pr,
            head: "abc".into(),
            shape: Shape {
                required: vec!["window".into()],
                also: vec!["hook".into()],
                ..Shape::default()
            },
            ..StoredBrief::default()
        }
    }

    fn spec_pre_flight() -> PreFlight {
        PreFlight {
            issue: Some(378),
            title: "Studio: Spec: the modules".into(),
            parts: vec!["hook".into(), "core".into()],
            predicted: true,
            at: 1,
        }
    }

    const MAP: &str = r#"{ "version": 1, "parts": [
      { "id": "hook", "name": "Hook", "caption": "c", "description": "d", "tier": "core", "paths": ["hook/"] },
      { "id": "core", "name": "Core", "caption": "c", "description": "d", "tier": "core", "paths": ["core/"] }],
      "actors": [], "links": [], "ignored": [], "laws": [], "flows": [] }"#;

    struct Canned(Result<String, String>);
    impl BriefWriter for Canned {
        fn write(
            &self,
            _: &str,
            _: &str,
        ) -> Result<studio_understanding::pr_brief::BriefReply, String> {
            self.0
                .clone()
                .map(|text| studio_understanding::pr_brief::BriefReply {
                    text,
                    usage: Default::default(),
                })
        }
    }

    #[test]
    fn an_issue_s_prediction_is_stored_with_its_issue_and_a_failed_one_stores_nothing() {
        let (_dir, store) = store();
        let issue = Input::Issue {
            number: 378,
            title: "Studio: Spec: the modules",
            body: "Pre-flight.",
        };
        let failed = Canned(Err("`claude -p` did not run".into()));
        assert!(predict_into(&store, &failed, "opus", MAP, &issue, 5).is_err());
        assert_eq!(store.current(), None);
        let named = Canned(Ok(r#"{"parts": ["core", "server"]}"#.into()));
        let pre = predict_into(&store, &named, "opus", MAP, &issue, 5).unwrap();
        assert_eq!(
            pre,
            PreFlight {
                issue: Some(378),
                title: "Studio: Spec: the modules".into(),
                parts: vec!["core".into()],
                predicted: true,
                at: 5,
            }
        );
        assert_eq!(store.current(), Some(pre));
        let text = Input::Text("Make the hook faster");
        let named = Canned(Ok(r#"{"parts": ["hook"]}"#.into()));
        let pre = predict_into(&store, &named, "opus", MAP, &text, 6).unwrap();
        assert_eq!(
            (pre.issue, pre.title.as_str()),
            (None, "Make the hook faster"),
            "free text has no issue to match"
        );
    }

    #[test]
    fn a_brief_matches_the_pre_flight_through_every_closing_issue_and_its_parent() {
        let (_dir, store) = store();
        store.set(Some(spec_pre_flight())).unwrap();
        assert_eq!(store.current(), Some(spec_pre_flight()));
        // PR 406 closes #12 (no parent) and #390, a ticket of the spec; PR 407 closes an unrelated issue.
        store
            .note(
                406,
                vec![
                    Closing {
                        number: 12,
                        parent: None,
                    },
                    Closing {
                        number: 390,
                        parent: Some(378),
                    },
                ],
            )
            .unwrap();
        store
            .note(
                407,
                vec![Closing {
                    number: 12,
                    parent: None,
                }],
            )
            .unwrap();
        assert_eq!(
            store.expected(406, &brief(406)),
            Some(Expected {
                expected: vec!["hook".into(), "core".into()],
                touched: vec!["window".into(), "hook".into()],
            })
        );
        assert_eq!(store.expected(407, &brief(407)), None);
        assert_eq!(store.expected(408, &brief(408)), None, "a PR never noted");
        store.set(None).unwrap();
        assert_eq!(
            store.expected(406, &brief(406)),
            None,
            "a cleared pre-flight matches nothing"
        );
    }
}
