//! Ratifying (spec danielrispler/plotkeeper#334, ticket #343): Daniel accepts or overturns what a PR changed on the map and what its
//! agent decided. Two kinds of item, both read by code: each map or law change, from the map's diff, keyed by (PR, kind, map id);
//! each decision of the brief's `decisions`, keyed by (PR, decision id). Ratifying never blocks anything: the merged map is in
//! effect at once, and the record only says Daniel has looked.
//!
//! The state is `ratify.json` in the data folder, per project: when the project's first run was (`since`) and a record per decided
//! item. No record means ratified for anything that landed on or before `since`, and "not yet ratified" for anything after. A record
//! of state `pending` marks an item not yet ratified by hand, whenever it landed: that is how Daniel's own file keeps the two laws #295
//! left open, with no logic here naming them. A reader ignores what it does not know, and a missing field has its default, so an
//! older or newer build reads the file.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

pub const FILE: &str = "ratify.json";
pub const DECISION: &str = "decision";

/// The map's kinds of item, as `system-map.json` names their lists.
const KINDS: [(&str, &str); 5] = [
    ("parts", "part"),
    ("actors", "actor"),
    ("links", "link"),
    ("laws", "law"),
    ("flows", "flow"),
];

/// What Daniel decided about an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Decided {
    Accepted,
    Overturned,
    /// Not yet ratified, whenever it landed: written by hand.
    Pending,
    /// A state a newer build wrote: read as no record.
    #[serde(other)]
    Unknown,
}

/// One item of a PR: `kind` is a map kind (`part`, `actor`, `link`, `law`, `flow`) or `decision`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub pr: u64,
    pub kind: String,
    pub id: String,
}

impl Item {
    pub fn new(pr: u64, kind: &str, id: &str) -> Item {
        Item {
            pr,
            kind: kind.into(),
            id: id.into(),
        }
    }
    /// The one string an item is known by.
    pub fn key(&self) -> String {
        format!("{}/{}/{}", self.pr, self.kind, self.id)
    }
}

/// A decided item in the file. `text` is what the item said when it was decided, so it can still be shown when a rewritten brief no
/// longer holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    #[serde(default)]
    pub pr: u64,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub id: String,
    pub state: Decided,
    /// When, in seconds since 1970.
    #[serde(default)]
    pub at: u64,
    #[serde(default)]
    pub text: String,
}

impl Record {
    fn item(&self) -> Item {
        Item::new(self.pr, &self.kind, &self.id)
    }
}

/// One project's ratifying.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// When the project's first run was; what landed on or before it is ratified without a record. 0 until the first run.
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub records: Vec<Record>,
}

/// Where an item stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Landed before the first run, with no record.
    Ratified,
    Accepted,
    Overturned,
    NotYet,
}

impl Project {
    fn record(&self, item: &Item) -> Option<&Record> {
        self.records
            .iter()
            .rev()
            .find(|r| r.item() == *item && r.state != Decided::Unknown)
    }

    /// `landed` is when the item's PR landed on the default branch, none while it is open.
    pub fn status(&self, item: &Item, landed: Option<u64>) -> Status {
        match self.record(item).map(|r| r.state) {
            Some(Decided::Accepted) => Status::Accepted,
            Some(Decided::Overturned) => Status::Overturned,
            Some(Decided::Pending) => Status::NotYet,
            _ if landed.is_some_and(|at| at <= self.since) => Status::Ratified,
            _ => Status::NotYet,
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The file.

fn read_all(file: &Path) -> Value {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}

/// The project's state in `file`; a missing or unreadable file, or one of an older format, is an empty state.
pub fn read(file: &Path, project: &str) -> Project {
    read_all(file)
        .get("projects")
        .and_then(|p| p.get(project))
        .and_then(|p| serde_json::from_value(p.clone()).ok())
        .unwrap_or_default()
}

/// Serialises the writes, so two changes never lose one another.
static WRITING: Mutex<()> = Mutex::new(());

/// Changes `project`'s state in `file`, keeping the other projects' and any fields this build does not know.
fn update(
    file: &Path,
    project: &str,
    change: impl FnOnce(&mut Project),
) -> Result<Project, String> {
    let _writing = WRITING.lock().unwrap();
    let mut whole = read_all(file);
    let mut mine = read(file, project);
    change(&mut mine);
    let entry = whole
        .as_object_mut()
        .unwrap()
        .entry("projects")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("ratify.json: projects is not an object")?
        .entry(project)
        .or_insert_with(|| serde_json::json!({}));
    let saved = serde_json::to_value(&mine).map_err(|e| e.to_string())?;
    match entry.as_object_mut() {
        Some(object) => object.extend(saved.as_object().unwrap().clone()),
        None => *entry = saved,
    }
    whole["version"] = 1.into();
    let dir = file.parent().ok_or("no folder for ratify.json")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut temp, &whole).map_err(|e| e.to_string())?;
    temp.persist(file).map_err(|e| e.to_string())?;
    Ok(mine)
}

/// Records the first run of `project` (once), so everything that landed before it counts as ratified.
pub fn start(file: &Path, project: &str, now: u64) -> Result<Project, String> {
    let known = read(file, project);
    if known.since != 0 {
        return Ok(known);
    }
    update(file, project, |p| {
        if p.since == 0 {
            p.since = now;
        }
    })
}

/// Records Daniel's decision on `item`, replacing an earlier record of it.
pub fn decide(
    file: &Path,
    project: &str,
    item: &Item,
    state: Decided,
    text: &str,
    now: u64,
) -> Result<Project, String> {
    update(file, project, |p| {
        p.records.retain(|r| r.item() != *item);
        p.records.push(Record {
            pr: item.pr,
            kind: item.kind.clone(),
            id: item.id.clone(),
            state,
            at: now,
            text: text.into(),
        });
    })
}

pub fn file() -> Result<PathBuf, String> {
    let home = std::env::home_dir().ok_or("no home folder")?;
    Ok(studio_hook::data_dir(&home).join(FILE))
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------------------------------------------
// Reading the map's diff.

/// One map change of a PR.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapItem {
    pub kind: String,
    pub id: String,
    /// `added`, `changed` or `removed`.
    pub change: String,
    /// What the item says, in words.
    pub text: String,
}

fn field<'a>(item: &'a Value, name: &str) -> &'a str {
    item[name].as_str().unwrap_or_default()
}

fn words(kind: &str, item: &Value) -> String {
    match kind {
        "law" => field(item, "text").into(),
        "link" => format!("{} to {}", field(item, "from"), field(item, "to")),
        "flow" => field(item, "name").into(),
        _ => field(item, "name").into(),
    }
}

/// What of an item counts as a change: a part's `tier` and `zone` are Daniel's to set, never a change of the system.
fn significant(kind: &str, item: &Value) -> Value {
    let mut item = item.clone();
    if kind == "part"
        && let Some(object) = item.as_object_mut()
    {
        object.remove("tier");
        object.remove("zone");
    }
    item
}

/// The map items a change from `old` to `new` made, read from the two maps by code. A side that is missing or does not read gives
/// nothing: nothing can be told apart.
pub fn map_items(old: Option<&str>, new: Option<&str>) -> Vec<MapItem> {
    let (Some(old), Some(new)) = (old, new) else {
        return vec![];
    };
    let (Ok(old), Ok(new)) = (
        serde_json::from_str::<Value>(old),
        serde_json::from_str::<Value>(new),
    ) else {
        return vec![];
    };
    let list = |map: &Value, key: &str| -> BTreeMap<String, Value> {
        map[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| Some((item["id"].as_str()?.to_string(), item.clone())))
            .collect()
    };
    let mut found = vec![];
    for (key, kind) in KINDS {
        let (before, after) = (list(&old, key), list(&new, key));
        for (id, item) in &after {
            let change = match before.get(id) {
                None => "added",
                Some(was) if significant(kind, was) != significant(kind, item) => "changed",
                Some(_) => continue,
            };
            found.push(MapItem {
                kind: kind.into(),
                id: id.clone(),
                change: change.into(),
                text: words(kind, item),
            });
        }
        for (id, item) in &before {
            if !after.contains_key(id) {
                found.push(MapItem {
                    kind: kind.into(),
                    id: id.clone(),
                    change: "removed".into(),
                    text: words(kind, item),
                });
            }
        }
    }
    found
}

// ---------------------------------------------------------------------------------------------------------------
// What the brief offers.

/// One row of the brief's ratify step.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub pr: u64,
    pub kind: String,
    pub id: String,
    pub text: String,
    /// `added`, `changed` or `removed` for a map item; empty for a decision.
    pub change: String,
    pub status: Status,
}

/// The items of PR `pr`: its map changes, then its decisions (`(id, text)` from its brief), then the decisions Daniel decided that a
/// rewrite of the brief no longer holds, which keep their record and show the text they had.
pub fn rows(
    project: &Project,
    pr: u64,
    map: &[MapItem],
    decisions: &[(String, String)],
    landed: Option<u64>,
) -> Vec<Row> {
    let row = |kind: &str, id: &str, text: &str, change: &str| {
        let item = Item::new(pr, kind, id);
        Row {
            pr,
            kind: kind.into(),
            id: id.into(),
            text: text.into(),
            change: change.into(),
            status: project.status(&item, landed),
        }
    };
    let mut rows: Vec<Row> = map
        .iter()
        .map(|m| row(&m.kind, &m.id, &m.text, &m.change))
        .collect();
    rows.extend(
        decisions
            .iter()
            .map(|(id, text)| row(DECISION, id, text, "")),
    );
    for record in &project.records {
        let missing = record.pr == pr
            && record.kind == DECISION
            && record.state != Decided::Unknown
            && !decisions.iter().any(|(id, _)| *id == record.id);
        if missing && !rows.iter().any(|r| r.kind == DECISION && r.id == record.id) {
            rows.push(row(DECISION, &record.id, &record.text, ""));
        }
    }
    rows
}

// ---------------------------------------------------------------------------------------------------------------
// The map's changes in git.

use crate::map::{default_ref, git};

/// An item the part sheet says is not yet ratified.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pending {
    pub pr: u64,
    pub kind: String,
    pub id: String,
}

/// `12` for a squash-merge subject `Title (#12)`.
fn pr_of(subject: &str) -> Option<u64> {
    let (_, number) = subject.trim().strip_suffix(')')?.rsplit_once(" (#")?;
    number.parse().ok()
}

struct Landed {
    commit: String,
    at: u64,
    pr: Option<u64>,
}

/// The first-parent commits of `reference`, newest first.
fn first_parents(repo: &Path, reference: &str) -> Vec<Landed> {
    let Ok(log) = git(
        repo,
        &[
            "log",
            "--first-parent",
            "--max-count=500",
            "--format=%H%x09%ct%x09%s",
            reference,
        ],
    ) else {
        return vec![];
    };
    log.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '\t');
            Some(Landed {
                commit: parts.next()?.into(),
                at: parts.next()?.parse().ok()?,
                pr: pr_of(parts.next()?),
            })
        })
        .collect()
}

fn map_at(repo: &Path, commit: &str) -> Option<String> {
    git(
        repo,
        &["show", &format!("{commit}:{}", system_map::MAP_FILE)],
    )
    .ok()
}

type Cache = Mutex<HashMap<(String, String), Vec<MapItem>>>;
static CHANGES: LazyLock<Cache> = LazyLock::new(Mutex::default);

/// The map items `commit` changed, from its map and its parent's. A commit never changes, so the answer is kept.
fn changes_of(repo: &Path, commit: &str) -> Vec<MapItem> {
    let key = (repo.display().to_string(), commit.to_string());
    if let Some(known) = CHANGES.lock().unwrap().get(&key) {
        return known.clone();
    }
    let found = map_items(
        map_at(repo, &format!("{commit}^")).as_deref(),
        map_at(repo, commit).as_deref(),
    );
    CHANGES.lock().unwrap().insert(key, found.clone());
    found
}

/// What the brief of PR `number` offers from the map: when the PR has landed, its squash commit's changes and when it landed;
/// while it is open, the change from where its head left the default branch to its head, landing never.
pub fn pr_changes(
    repo: &Path,
    number: u64,
    open_head: Option<&str>,
) -> (Vec<MapItem>, Option<u64>) {
    let Some((reference, _)) = default_ref(repo) else {
        return (vec![], None);
    };
    if let Some(head) = open_head {
        let items = git(repo, &["merge-base", &reference, head])
            .map(|base| {
                map_items(
                    map_at(repo, &base).as_deref(),
                    map_at(repo, head).as_deref(),
                )
            })
            .unwrap_or_default();
        return (items, None);
    }
    match first_parents(repo, &reference)
        .into_iter()
        .find(|l| l.pr == Some(number))
    {
        Some(l) => (changes_of(repo, &l.commit), Some(l.at)),
        // Too old to find: landed long ago.
        None => (vec![], Some(0)),
    }
}

/// The items not yet ratified, for the part sheet: those marked so by hand, and the map changes of the PRs that landed on the default
/// branch after the project's first run and have no record.
pub fn pending(repo: &Path, project: &Project) -> Vec<Pending> {
    let mut seen = std::collections::HashSet::new();
    let mut found = vec![];
    let mut add = |item: Item| {
        if seen.insert(item.key()) {
            found.push(Pending {
                pr: item.pr,
                kind: item.kind,
                id: item.id,
            });
        }
    };
    for r in &project.records {
        if r.state == Decided::Pending && r.kind != DECISION {
            add(r.item());
        }
    }
    if let Some((reference, _)) = default_ref(repo) {
        for landed in first_parents(repo, &reference) {
            if landed.at <= project.since {
                break;
            }
            let Some(pr) = landed.pr else { continue };
            for change in changes_of(repo, &landed.commit) {
                let item = Item::new(pr, &change.kind, &change.id);
                if project.status(&item, Some(landed.at)) == Status::NotYet {
                    add(item);
                }
            }
        }
    }
    found
}

/// The items the brief of PR `number` offers for accept or overturn, with where each stands.
#[tauri::command(async)]
pub fn pr_ratify(
    project: String,
    number: u64,
    prs: tauri::State<crate::prs::Prs>,
) -> Result<Vec<Row>, String> {
    let repo = Path::new(&project);
    let mine = start(&file()?, &project, now())?;
    let open = prs
        .ready(&project)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| r.head);
    let (map, landed) = pr_changes(repo, number, open.as_deref());
    let decisions: Vec<(String, String)> = crate::pr_briefs::store_of(&project)
        .and_then(|store| store.latest(number))
        .map(|stored| {
            stored
                .brief
                .decisions
                .into_iter()
                .filter_map(|d| Some((d.id?, d.text)))
                .collect()
        })
        .unwrap_or_default();
    Ok(rows(&mine, number, &map, &decisions, landed))
}

/// Daniel's decision on one item: `accepted` or `overturned`, with what the item said.
#[tauri::command(async)]
pub fn ratify_set(
    project: String,
    pr: u64,
    kind: String,
    id: String,
    state: Decided,
    text: String,
) -> Result<(), String> {
    if !matches!(state, Decided::Accepted | Decided::Overturned) {
        return Err("a decision is accepted or overturned".into());
    }
    decide(
        &file()?,
        &project,
        &Item::new(pr, &kind, &id),
        state,
        &text,
        now(),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = r#"{"version":1,
      "parts":[{"id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]},
               {"id":"b","name":"B","caption":"c","description":"d","tier":"core","paths":["b/"]}],
      "actors":[],"links":[{"id":"a-b","from":"a","to":"b"}],"ignored":[],
      "laws":[{"id":"l1","on":"a","backing":"trust","text":"First."},
              {"id":"l2","on":"b","backing":"trust","text":"Second."}],"flows":[]}"#;

    fn new_map() -> String {
        OLD.replace("First.", "First, reworded.")
            .replace(
                r#""laws":["#,
                r#""laws":[{"id":"l3","on":"a","backing":"trust","text":"Third."},"#,
            )
            .replace(r#"{"id":"a-b","from":"a","to":"b"}"#, "")
            .replace(
                r#""tier":"core","paths":["a/"]"#,
                r#""tier":"interface","paths":["a/"]"#,
            )
    }

    #[test]
    fn the_maps_diff_is_read_by_code_into_items_keyed_by_kind_and_id() {
        let items = map_items(Some(OLD), Some(&new_map()));
        let got: Vec<(&str, &str, &str)> = items
            .iter()
            .map(|i| (i.kind.as_str(), i.id.as_str(), i.change.as_str()))
            .collect();
        // l1 reworded, l3 added, the link removed; a's tier is Daniel's own and l2 did not change.
        assert_eq!(
            got,
            [
                ("link", "a-b", "removed"),
                ("law", "l1", "changed"),
                ("law", "l3", "added")
            ]
        );
        assert_eq!(items[1].text, "First, reworded.");
        assert_eq!(items[0].text, "a to b");
        assert!(
            map_items(None, Some(OLD)).is_empty(),
            "no map before: nothing to tell apart"
        );
        assert!(map_items(Some("not json"), Some(OLD)).is_empty());
    }

    #[test]
    fn an_item_is_known_by_its_pr_kind_and_id() {
        assert_eq!(Item::new(7, "law", "l1").key(), "7/law/l1");
        assert_ne!(
            Item::new(7, "law", "l1").key(),
            Item::new(8, "law", "l1").key()
        );
        assert_ne!(
            Item::new(7, "law", "x").key(),
            Item::new(7, "link", "x").key()
        );
        assert_ne!(
            Item::new(7, "law", "d1").key(),
            Item::new(7, DECISION, "d1").key()
        );
    }

    fn dir() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE);
        (dir, file)
    }

    #[test]
    fn a_decision_is_kept_and_read_back_and_another_project_is_left_alone() {
        let (_dir, file) = dir();
        let item = Item::new(7, "law", "l1");
        assert_eq!(read(&file, "/p").records, vec![], "no file: empty");
        start(&file, "/p", 100).unwrap();
        start(&file, "/q", 300).unwrap();
        decide(&file, "/p", &item, Decided::Accepted, "First.", 150).unwrap();
        decide(&file, "/p", &item, Decided::Overturned, "First.", 160).unwrap();
        let mine = read(&file, "/p");
        assert_eq!(mine.since, 100);
        assert_eq!(
            mine.records.len(),
            1,
            "a later decision replaces the earlier"
        );
        assert_eq!(mine.records[0].state, Decided::Overturned);
        assert_eq!(mine.records[0].at, 160);
        assert_eq!(read(&file, "/q").since, 300);
        // The first run is only recorded once.
        assert_eq!(start(&file, "/p", 999).unwrap().since, 100);
    }

    #[test]
    fn a_file_of_an_older_or_newer_format_still_reads() {
        let (_dir, file) = dir();
        // Older: no version, no `since`, a record with only the bare fields. Newer: unknown fields and an unknown state.
        std::fs::write(
            &file,
            r#"{"later":{"x":1},"projects":{"/p":{"extra":true,"records":[
                {"pr":7,"kind":"law","id":"l1","state":"accepted"},
                {"pr":7,"kind":"law","id":"l2","state":"snoozed","at":5,"new":"field"}]}}}"#,
        )
        .unwrap();
        let mine = read(&file, "/p");
        assert_eq!(mine.since, 0);
        assert_eq!(mine.records.len(), 2);
        assert_eq!(
            mine.status(&Item::new(7, "law", "l1"), Some(10)),
            Status::Accepted
        );
        // A state this build does not know is no record.
        assert_eq!(
            mine.status(&Item::new(7, "law", "l2"), Some(10)),
            Status::NotYet
        );
        // Writing keeps what it does not know.
        decide(
            &file,
            "/p",
            &Item::new(8, "law", "l9"),
            Decided::Accepted,
            "",
            1,
        )
        .unwrap();
        let whole: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(whole["later"]["x"], 1);
        assert_eq!(whole["projects"]["/p"]["extra"], true);
        assert_eq!(whole["version"], 1);
        // A file that is not JSON is an empty state.
        std::fs::write(&file, "nope").unwrap();
        assert_eq!(read(&file, "/p"), Project::default());
    }

    #[test]
    fn what_landed_before_the_first_run_is_ratified_and_what_landed_after_is_not_yet() {
        let project = Project {
            since: 100,
            records: vec![Record {
                pr: 3,
                kind: "law".into(),
                id: "kept-open".into(),
                state: Decided::Pending,
                at: 0,
                text: String::new(),
            }],
        };
        let law = |pr, id| Item::new(pr, "law", id);
        assert_eq!(project.status(&law(1, "a"), Some(50)), Status::Ratified);
        assert_eq!(project.status(&law(1, "a"), Some(100)), Status::Ratified);
        assert_eq!(project.status(&law(2, "a"), Some(101)), Status::NotYet);
        assert_eq!(
            project.status(&law(2, "a"), None),
            Status::NotYet,
            "an open PR's item"
        );
        // Marked by hand: not yet, although it landed before the first run.
        assert_eq!(
            project.status(&law(3, "kept-open"), Some(50)),
            Status::NotYet
        );
    }

    #[test]
    fn a_reworded_rewrite_keeps_a_decisions_record() {
        use studio_understanding::pr_brief::{Brief, Decision, assign_decision_ids};
        let (_dir, file) = dir();
        let first = Decision {
            id: Some("d1".into()),
            text: "Briefs are stored per project.".into(),
            flags: vec![],
        };
        decide(
            &file,
            "/p",
            &Item::new(7, DECISION, "d1"),
            Decided::Accepted,
            &first.text,
            5,
        )
        .unwrap();
        // The rewrite words the same decision differently and the writer gives its id back.
        let mut rewrite = Brief {
            decisions: vec![Decision {
                id: Some("d1".into()),
                text: "Each project keeps its own briefs.".into(),
                flags: vec![],
            }],
            ..Brief::default()
        };
        assign_decision_ids(&mut rewrite, &[first]);
        let decisions: Vec<(String, String)> = rewrite
            .decisions
            .iter()
            .map(|d| (d.id.clone().unwrap(), d.text.clone()))
            .collect();
        assert_eq!(decisions[0].0, "d1");
        let listed = rows(&read(&file, "/p"), 7, &[], &decisions, None);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].status, Status::Accepted, "not asked twice");
        assert_eq!(listed[0].text, "Each project keeps its own briefs.");
    }

    #[test]
    fn a_decided_item_missing_from_a_rewrite_keeps_its_record_and_is_still_listed() {
        let (_dir, file) = dir();
        let item = Item::new(7, DECISION, "d2");
        decide(&file, "/p", &item, Decided::Overturned, "Use a timer.", 5).unwrap();
        let listed = rows(
            &read(&file, "/p"),
            7,
            &[],
            &[("d3".into(), "New one.".into())],
            None,
        );
        let ids: Vec<(&str, Status)> = listed.iter().map(|r| (r.id.as_str(), r.status)).collect();
        assert_eq!(ids, [("d3", Status::NotYet), ("d2", Status::Overturned)]);
        assert_eq!(listed[1].text, "Use a timer.");
        // It is another PR's record: not listed there.
        assert!(rows(&read(&file, "/p"), 8, &[], &[], None).is_empty());
        // And the file still holds it.
        assert_eq!(read(&file, "/p").records.len(), 1);
    }

    #[test]
    fn a_prs_map_items_carry_their_status() {
        let project = Project {
            since: 100,
            ..Project::default()
        };
        let map = map_items(Some(OLD), Some(&new_map()));
        let after = rows(&project, 7, &map, &[], Some(200));
        assert!(after.iter().all(|r| r.status == Status::NotYet));
        let before = rows(&project, 7, &map, &[], Some(50));
        assert!(before.iter().all(|r| r.status == Status::Ratified));
    }

    mod git_repo {
        use super::*;
        use studio_test_support::{git as run, repo};

        fn commit(dir: &Path, map: &str, subject: &str, date: &str) {
            std::fs::write(dir.join("system-map.json"), map).unwrap();
            run(dir, &["add", "-A"]);
            let stamp = format!("{date} +0000");
            std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=S",
                    "-c",
                    "user.email=s@e.com",
                    "commit",
                    "-q",
                    "-m",
                    subject,
                ])
                .env("GIT_COMMITTER_DATE", &stamp)
                .env("GIT_AUTHOR_DATE", &stamp)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .current_dir(dir)
                .status()
                .unwrap();
        }

        #[test]
        fn the_sheet_lists_what_landed_after_the_first_run_without_a_record() {
            let (dir, main) = repo();
            let root = dir.path();
            commit(root, OLD, "Map (#1)", "1000000000");
            commit(root, &new_map(), "Change laws (#2)", "1100000000");
            let repo = Path::new(&main);
            // The first run was between the two.
            let mut project = Project {
                since: 1_050_000_000,
                ..Project::default()
            };
            let got: Vec<(u64, &str, String)> = pending(repo, &project)
                .iter()
                .map(|p| (p.pr, p.kind.as_str(), p.id.clone()))
                .map(|(pr, k, id)| {
                    (
                        pr,
                        if k == "law" {
                            "law"
                        } else if k == "link" {
                            "link"
                        } else {
                            "other"
                        },
                        id,
                    )
                })
                .collect();
            assert!(got.contains(&(2, "law", "l1".into())));
            assert!(got.contains(&(2, "law", "l3".into())));
            assert!(got.contains(&(2, "link", "a-b".into())));
            assert!(
                got.iter().all(|(pr, _, _)| *pr == 2),
                "PR 1 landed before the first run"
            );
            // Accepting one takes it off.
            project.records.push(Record {
                pr: 2,
                kind: "law".into(),
                id: "l1".into(),
                state: Decided::Accepted,
                at: 1,
                text: String::new(),
            });
            assert!(!pending(repo, &project).iter().any(|p| p.id == "l1"));
            // A record marked pending by hand shows although PR 1 landed before the first run.
            project.records.push(Record {
                pr: 1,
                kind: "law".into(),
                id: "l2".into(),
                state: Decided::Pending,
                at: 0,
                text: String::new(),
            });
            assert!(
                pending(repo, &project)
                    .iter()
                    .any(|p| p.pr == 1 && p.id == "l2")
            );
        }

        #[test]
        fn a_landed_prs_changes_are_its_squash_commits_and_an_open_prs_are_its_branchs() {
            let (dir, main) = repo();
            let root = dir.path();
            commit(root, OLD, "Map (#1)", "1000000000");
            run(root, &["checkout", "-q", "-b", "work"]);
            commit(root, &new_map(), "work", "1100000000");
            let head = run(root, &["rev-parse", "HEAD"]);
            run(root, &["checkout", "-q", "main"]);
            let repo = Path::new(&main);
            // Open: not on main yet.
            let (open, landed) = pr_changes(repo, 2, Some(&head));
            assert_eq!(landed, None);
            assert_eq!(open.len(), 3);
            // Landed by a squash commit.
            commit(root, &new_map(), "Change laws (#2)", "1200000000");
            let (done, landed) = pr_changes(repo, 2, None);
            assert_eq!(landed, Some(1_200_000_000));
            assert_eq!(done.len(), 3);
            // A PR the log does not hold landed long ago.
            assert_eq!(pr_changes(repo, 99, None), (vec![], Some(0)));
        }
    }
}
