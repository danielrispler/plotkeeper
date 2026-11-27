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
    sync::Mutex,
};
use studio_understanding::pr_brief::{ImportSite, LinkEvidence};

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Decided {
    Accepted,
    Overturned,
    /// Not yet ratified, whenever it landed: written by hand.
    Pending,
    /// A state a newer build wrote: read as no record, and written back as it was.
    Unknown(String),
}

impl From<String> for Decided {
    fn from(state: String) -> Self {
        match state.as_str() {
            "accepted" => Decided::Accepted,
            "overturned" => Decided::Overturned,
            "pending" => Decided::Pending,
            _ => Decided::Unknown(state),
        }
    }
}

impl From<Decided> for String {
    fn from(state: Decided) -> Self {
        match state {
            Decided::Accepted => "accepted".into(),
            Decided::Overturned => "overturned".into(),
            Decided::Pending => "pending".into(),
            Decided::Unknown(state) => state,
        }
    }
}

impl Decided {
    fn is_unknown(&self) -> bool {
        matches!(self, Decided::Unknown(_))
    }
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
    /// Fields a newer build wrote: kept when this build writes the file.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
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
            .find(|r| r.item() == *item && !r.state.is_unknown())
    }

    /// `landed` is when the item's PR landed on the default branch, none while it is open.
    pub fn status(&self, item: &Item, landed: Option<u64>) -> Status {
        match self.record(item).map(|r| r.state.clone()) {
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

/// The project's state in `file`; a missing or unreadable file, or one of an older format, is an empty state.
pub fn read(file: &Path, project: &str) -> Project {
    crate::json_file::read_project(file, project)
}

/// Serialises the writes, so two changes never lose one another.
static WRITING: Mutex<()> = Mutex::new(());

/// Changes `project`'s state in `file`, keeping the other projects' and any fields this build does not know.
fn update(
    file: &Path,
    project: &str,
    change: impl FnOnce(&mut Project),
) -> Result<Project, String> {
    Ok(crate::json_file::update_json_file(
        file,
        "ratify.json",
        &WRITING,
        project,
        change,
        |whole| whole["version"] = 1.into(),
    )?)
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
            extra: BTreeMap::new(),
        });
    })
}

pub fn file() -> Result<PathBuf, String> {
    let home = std::env::home_dir().ok_or("no home folder")?;
    Ok(studio_hook::data_dir(&home).join(FILE))
}

/// Now, in Unix seconds, as ratify, staleness and the map keep time.
pub fn now() -> u64 {
    studio_workbench::clock::now_ms() / 1000
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
    /// What a changed item said before, in the same words; none for an added or removed one. The Map text review reads it, so
    /// a law whose text stayed is not reviewed again.
    #[serde(skip)]
    pub was: Option<String>,
    /// Old beside new, for a part or a link.
    pub detail: Option<Detail>,
    /// The id of the mark the item's Ratify row lights on the Map diff (#643): `part:<id>`, `actor:<id>`, `law:<id>`, or for a link
    /// the line it is drawn on, `line:<end>|<end>` in sorted order; none for a flow, which the map does not draw.
    pub mark: Option<String>,
}

/// The import sites behind an added link, and the head they were read at, where each site's snippet is read.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    pub head: String,
    pub sites: Vec<ImportSite>,
}

/// Joins the brief's stored link evidence, read at `head`, to the rows of the links it adds; a removed or changed link, and an
/// added one with no evidence stored, keep none. Rows read at an open PR's head `read_at` other than `head` (a push since the
/// brief was written) keep none either, since the new head's imports may differ; a landed PR's rows (`None`) keep the join.
pub fn with_evidence(
    rows: &mut [Row],
    read_at: Option<&str>,
    head: &str,
    evidence: &[LinkEvidence],
) {
    if read_at.is_some_and(|at| at != head) {
        return;
    }
    for row in rows
        .iter_mut()
        .filter(|row| row.kind == "link" && row.change == "added")
    {
        row.evidence = evidence
            .iter()
            .find(|e| e.link == row.id && !e.sites.is_empty())
            .map(|e| Evidence {
                head: head.to_string(),
                sites: e.sites.clone(),
            });
    }
}

/// One text before and after a change: none on the side that has no item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Words {
    pub before: Option<String>,
    pub after: Option<String>,
}

/// What a part or a link said before and after, for the brief's ratify step.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "of", rename_all = "lowercase")]
pub enum Detail {
    Part(PartDetail),
    Link(LinkDetail),
}

/// A part's name, purpose and caption, its held items matched by name, and the patterns its own `paths` gained and lost.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PartDetail {
    pub name: Words,
    pub purpose: Words,
    pub caption: Words,
    pub holds: Vec<HeldDetail>,
    pub paths_added: Vec<String>,
    pub paths_removed: Vec<String>,
}

/// A held item, by its name: its text before and after (none before when added, none after when removed) and its `paths` change.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeldDetail {
    pub name: String,
    pub text: Words,
    pub paths_added: Vec<String>,
    pub paths_removed: Vec<String>,
}

/// A link's kind and reason before and after.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LinkDetail {
    pub kind: Words,
    pub reason: Words,
    /// `from to to` before and after, only when the link's ends swapped (spec #634, Q6).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Words>,
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

/// The text of `name` on each side that has the item.
fn words_of(before: Option<&Value>, after: Option<&Value>, name: &str) -> Words {
    let side = |item: Option<&Value>| item.map(|item| field(item, name).to_string());
    Words {
        before: side(before),
        after: side(after),
    }
}

/// The patterns of `paths` on each side: those only `after` has, then those only `before` has.
fn paths_change(before: Option<&Value>, after: Option<&Value>) -> (Vec<String>, Vec<String>) {
    let list = |item: Option<&Value>| -> Vec<String> {
        item.and_then(|item| item["paths"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|p| Some(p.as_str()?.to_string()))
            .collect()
    };
    let (was, now) = (list(before), list(after));
    let only =
        |of: &[String], not: &[String]| of.iter().filter(|p| !not.contains(p)).cloned().collect();
    (only(&now, &was), only(&was, &now))
}

/// A part's held items on each side, matched by name (a name held twice, by its n-th time): the new side's in its order, then
/// those it removed.
fn holds_change(before: Option<&Value>, after: Option<&Value>) -> Vec<HeldDetail> {
    let list = |part: Option<&Value>| -> Vec<Value> {
        part.and_then(|part| part["holds"].as_array())
            .cloned()
            .unwrap_or_default()
    };
    let (was, now) = (list(before), list(after));
    let find = |among: &[Value], name: &str, n: usize| {
        among
            .iter()
            .filter(|h| field(h, "name") == name)
            .nth(n)
            .cloned()
    };
    // Each name with how many times it came before, on the new side and then on the old.
    let numbered = |items: &[Value]| -> Vec<(String, usize)> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for held in items {
            let name = field(held, "name").to_string();
            let n = out.iter().filter(|(seen, _)| *seen == name).count();
            out.push((name, n));
        }
        out
    };
    let mut names = numbered(&now);
    for key in numbered(&was) {
        if !names.contains(&key) {
            names.push(key);
        }
    }
    names
        .into_iter()
        .map(|(name, n)| {
            let (old, new) = (find(&was, &name, n), find(&now, &name, n));
            let (paths_added, paths_removed) = paths_change(old.as_ref(), new.as_ref());
            HeldDetail {
                text: words_of(old.as_ref(), new.as_ref(), "text"),
                name,
                paths_added,
                paths_removed,
            }
        })
        .collect()
}

/// A link's `from to to` on each side, when its ends swapped; none otherwise.
fn swapped(before: Option<&Value>, after: Option<&Value>) -> Option<Words> {
    let (was, now) = (before?, after?);
    let ends = |link: &Value| {
        (
            field(link, "from").to_string(),
            field(link, "to").to_string(),
        )
    };
    let ((a, b), (c, d)) = (ends(was), ends(now));
    (a != c && a == d && b == c).then(|| Words {
        before: Some(format!("{a} to {b}")),
        after: Some(format!("{c} to {d}")),
    })
}

/// The id of the mark `item` of `kind` lights on the Map diff (`MapItem::mark`).
fn mark_of(kind: &str, item: &Value) -> Option<String> {
    match kind {
        "part" | "actor" | "law" => Some(format!("{kind}:{}", field(item, "id"))),
        "link" => crate::map_diff::ends(item).map(|[a, b]| format!("line:{a}|{b}")),
        _ => None,
    }
}

/// Old beside new for a part or a link; none for another kind.
fn detail(kind: &str, before: Option<&Value>, after: Option<&Value>) -> Option<Detail> {
    let (paths_added, paths_removed) = paths_change(before, after);
    match kind {
        "part" => Some(Detail::Part(PartDetail {
            name: words_of(before, after, "name"),
            purpose: words_of(before, after, "purpose"),
            caption: words_of(before, after, "caption"),
            holds: holds_change(before, after),
            paths_added,
            paths_removed,
        })),
        "link" => Some(Detail::Link(LinkDetail {
            kind: words_of(before, after, "kind"),
            reason: words_of(before, after, "reason"),
            direction: swapped(before, after),
        })),
        _ => None,
    }
}

/// The map items a change from `old` to `new` made, read from the two maps by code. A version 1 side is converted first, so its
/// `description` is its purpose. A side that is missing or does not read gives nothing: nothing can be told apart.
pub fn map_items(old: Option<&str>, new: Option<&str>) -> Vec<MapItem> {
    let (Some(old), Some(new)) = (old, new) else {
        return vec![];
    };
    let read = |text: &str| system_map::convert_value(text).ok();
    let (Some(old), Some(new)) = (read(old), read(new)) else {
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
                was: before.get(id).map(|was| words(kind, was)),
                detail: detail(kind, before.get(id), Some(item)),
                mark: mark_of(kind, item),
            });
        }
        for (id, item) in &before {
            if !after.contains_key(id) {
                found.push(MapItem {
                    kind: kind.into(),
                    id: id.clone(),
                    change: "removed".into(),
                    text: words(kind, item),
                    was: None,
                    detail: detail(kind, Some(item), None),
                    mark: mark_of(kind, item),
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
    /// Old beside new, for a part or a link.
    pub detail: Option<Detail>,
    /// For a link the PR adds, the import sites behind it, stored on its brief when it was written (#499).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    /// The id of the mark the row lights on the Map diff (`MapItem::mark`); none for a decision or a flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mark: Option<String>,
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
    let row = |kind: &str,
               id: &str,
               text: &str,
               change: &str,
               detail: Option<Detail>,
               mark: Option<String>| {
        let item = Item::new(pr, kind, id);
        Row {
            pr,
            kind: kind.into(),
            id: id.into(),
            text: text.into(),
            change: change.into(),
            status: project.status(&item, landed),
            detail,
            evidence: None,
            mark,
        }
    };
    let mut rows: Vec<Row> = map
        .iter()
        .map(|m| {
            row(
                &m.kind,
                &m.id,
                &m.text,
                &m.change,
                m.detail.clone(),
                m.mark.clone(),
            )
        })
        .collect();
    rows.extend(
        decisions
            .iter()
            .map(|(id, text)| row(DECISION, id, text, "", None, None)),
    );
    for record in &project.records {
        let missing = record.pr == pr
            && record.kind == DECISION
            && !record.state.is_unknown()
            && !decisions.iter().any(|(id, _)| *id == record.id);
        if missing && !rows.iter().any(|r| r.kind == DECISION && r.id == record.id) {
            rows.push(row(DECISION, &record.id, &record.text, "", None, None));
        }
    }
    rows
}

// ---------------------------------------------------------------------------------------------------------------
// The map's changes in git.

use crate::map::{GIT_READ_LIMIT, default_ref, git};
use studio_workbench::run::{self, RunError, run_until};

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

pub(crate) struct Landed {
    pub(crate) commit: String,
    at: u64,
    pub(crate) pr: Option<u64>,
}

/// The first-parent commits of `reference`, newest first.
pub(crate) fn first_parents(repo: &Path, reference: &str, max: usize) -> Vec<Landed> {
    let max_count = format!("--max-count={max}");
    let Ok(log) = git(
        repo,
        &[
            "log",
            "--first-parent",
            &max_count,
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

/// What git said of the map at a commit.
pub(crate) enum MapRead {
    /// The map's text.
    Read(String),
    /// Git answered that the commit has no map: its tree does not list the file.
    Absent,
    /// Git did not answer: it did not start, ran past its limit, or failed to show a map the tree lists.
    Unanswered,
}

impl MapRead {
    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            MapRead::Read(text) => Some(text),
            MapRead::Absent | MapRead::Unanswered => None,
        }
    }
}

/// The map at `commit`. A `git show` that exits non-zero is either an answer (the commit has no map) or a failed read (an
/// object out of reach), so the tree is asked which.
pub(crate) fn map_at(repo: &Path, commit: &str) -> MapRead {
    let show = |args: &[&str]| run_until(run::git(repo).args(args), GIT_READ_LIMIT);
    match show(&["show", &format!("{commit}:{}", system_map::MAP_FILE)]) {
        Ok(out) => MapRead::Read(String::from_utf8_lossy(&out.stdout).trim_end().to_string()),
        Err(RunError::Failed(_)) => {
            match show(&["ls-tree", "--name-only", commit, "--", system_map::MAP_FILE]) {
                Ok(out) if out.stdout.iter().all(u8::is_ascii_whitespace) => MapRead::Absent,
                _ => MapRead::Unanswered,
            }
        }
        Err(RunError::Start(_) | RunError::TimedOut(_)) => MapRead::Unanswered,
    }
}

/// The map items each commit changed, by repository and commit (`changes_of`), kept for the server's life on `App`.
pub(crate) type ChangesCache = Mutex<HashMap<(String, String), Vec<MapItem>>>;

/// The map items `commit` changed, from its map and its parent's. A commit never changes, so the answer is kept once git
/// answered both reads, a map that is absent included; a read git did not answer may work the next time, so it is read again.
fn changes_of(cache: &ChangesCache, repo: &Path, commit: &str) -> Vec<MapItem> {
    let key = (repo.display().to_string(), commit.to_string());
    if let Some(known) = cache.lock().unwrap().get(&key) {
        return known.clone();
    }
    let old = map_at(repo, &format!("{commit}^"));
    let new = map_at(repo, commit);
    let found = map_items(old.text(), new.text());
    if !matches!(old, MapRead::Unanswered) && !matches!(new, MapRead::Unanswered) {
        cache.lock().unwrap().insert(key, found.clone());
    }
    found
}

/// What the brief of PR `number` offers from the map: when the PR has landed, its squash commit's changes and when it landed;
/// while it is open, the change from where its head left the default branch to its head, landing never.
pub(crate) fn pr_changes(
    cache: &ChangesCache,
    repo: &Path,
    number: u64,
    open_head: Option<&str>,
) -> (Vec<MapItem>, Option<u64>) {
    pr_changes_in(cache, repo, number, open_head, WALK)
}

/// How many first-parent commits of the default branch are read for what landed.
pub(crate) const WALK: usize = 500;

fn pr_changes_in(
    cache: &ChangesCache,
    repo: &Path,
    number: u64,
    open_head: Option<&str>,
    walk: usize,
) -> (Vec<MapItem>, Option<u64>) {
    let Some((reference, _)) = default_ref(repo) else {
        return (vec![], None);
    };
    if let Some(head) = open_head {
        let items = git(repo, &["merge-base", &reference, head])
            .map(|base| map_items(map_at(repo, &base).text(), map_at(repo, head).text()))
            .unwrap_or_default();
        return (items, None);
    }
    let log = first_parents(repo, &reference, walk);
    let cut_short = log.len() >= walk;
    match log.into_iter().find(|l| l.pr == Some(number)) {
        Some(l) => (changes_of(cache, repo, &l.commit), Some(l.at)),
        // Not in a walk that was cut short: it may have landed long ago. Not in the whole log: it has not landed (a draft, a
        // closed one, one no lookup has seen), so its items are not ratified.
        None if cut_short => (vec![], Some(0)),
        None => (vec![], None),
    }
}

/// The items not yet ratified, for the part sheet: those marked so by hand, and the map changes of the PRs that landed on the default
/// branch after the project's first run and have no record.
pub(crate) fn pending(cache: &ChangesCache, repo: &Path, project: &Project) -> Vec<Pending> {
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
        for landed in first_parents(repo, &reference, WALK) {
            // Commit times do not only grow along the first-parent line (a rebase or a backdated commit), so an old one is passed
            // over, not the end of the walk.
            if landed.at <= project.since {
                continue;
            }
            let Some(pr) = landed.pr else { continue };
            for change in changes_of(cache, repo, &landed.commit) {
                let item = Item::new(pr, &change.kind, &change.id);
                if project.status(&item, Some(landed.at)) == Status::NotYet {
                    add(item);
                }
            }
        }
    }
    found
}

/// What a PR's head says of its map, stored with its brief.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeadCheck {
    pub link_evidence: Vec<LinkEvidence>,
    pub map_problems: Vec<String>,
}

/// What `head` says of its map, from one `system_map::check_at` (#641): the import sites behind each link it adds to the map at
/// `base` (#499, spec #477 "When it is computed"), and the check's problems, kept for every PR, not only one that adds a link. It
/// reads `head` in commit mode, so an import the PR did not edit counts and the working tree is never read. Only an `import` link has
/// evidence: a run-time link has none, even beside an import edge between the same parts, and neither has a removed link in v1. A
/// head with no map has nothing. An untrusted head (a PR not by Daniel's login) is checked without its toolchain file.
pub fn head_check(repo: &Path, base: &str, head: &str, trusted: bool) -> HeadCheck {
    let (old, new) = (map_at(repo, base), map_at(repo, head));
    let Some(map) = new
        .text()
        .and_then(|text| system_map::convert_value(text).ok())
    else {
        return HeadCheck::default();
    };
    let report = system_map::check_at(repo, head, trusted);
    let links = map["links"].as_array().cloned().unwrap_or_default();
    let link_evidence = map_items(old.text(), new.text())
        .into_iter()
        .filter(|item| item.kind == "link" && item.change == "added")
        .filter_map(|item| {
            let id = item.id;
            let link = links
                .iter()
                .find(|link| field(link, "id") == id && field(link, "kind") == "import")?;
            let edge = report
                .edges
                .iter()
                .find(|e| e.from == field(link, "from") && e.to == field(link, "to"))?;
            let mut sites: Vec<ImportSite> = Vec::new();
            for site in edge.pairs.iter().flat_map(|pair| &pair.sites) {
                let site = ImportSite {
                    file: site.file.clone(),
                    line: site.line,
                };
                if !sites.contains(&site) {
                    sites.push(site);
                }
            }
            Some(LinkEvidence { link: id, sites })
        })
        .collect();
    HeadCheck {
        link_evidence,
        map_problems: report.problems,
    }
}

/// The lines of an import site's snippet on each side of its line.
const AROUND: usize = 4;

/// An import site's code, read from the commit its evidence was read at (#499): the lines around it, numbered from `first`, and
/// the site's own line to mark, none when the site has no line, which shows the file's first lines.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SiteSnippet {
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub first: u32,
    pub lines: Vec<String>,
}

/// The snippet of `file` at `line` as commit `head` holds it, read with `git cat-file blob <head>:<file>`, not from the PR's diff, so an
/// import the PR did not edit shows too. `head` must be a commit's hex name.
pub fn site_snippet(
    repo: &Path,
    head: &str,
    file: &str,
    line: Option<u32>,
) -> Result<SiteSnippet, String> {
    if head.len() < 7 || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("a site is read at a commit's hex name".into());
    }
    // `cat-file blob`, not `show`: a directory, or an empty `file` (the root), is a tree, which `show` would list.
    let text = git(repo, &["cat-file", "blob", &format!("{head}:{file}")])?;
    let all: Vec<&str> = text.lines().collect();
    let at = line.map_or(1, |n| n as usize);
    let first = at.saturating_sub(AROUND).max(1);
    let end = (at + AROUND).max(first + 2 * AROUND).min(all.len());
    Ok(SiteSnippet {
        file: file.to_string(),
        line,
        first: first as u32,
        lines: all
            .get(first - 1..end)
            .unwrap_or_default()
            .iter()
            .map(ToString::to_string)
            .collect(),
    })
}

/// `site_snippet` of PR head `head` in `project`, for the brief's ratify step.
pub fn ratify_site(
    project: String,
    head: String,
    file: String,
    line: Option<u32>,
) -> Result<SiteSnippet, String> {
    site_snippet(Path::new(&project), &head, &file, line)
}

/// The items the brief of PR `number` offers for accept or overturn, with where each stands.
pub fn pr_ratify(app: &crate::App, project: String, number: u64) -> Result<Vec<Row>, String> {
    let prs = &app.prs;
    let repo = Path::new(&project);
    let mine = start(&file()?, &project, now())?;
    let open = prs
        .ready(&project)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| r.head);
    let (map, landed) = pr_changes(&app.changes, repo, number, open.as_deref());
    let stored = crate::pr_briefs::store_of(&project).and_then(|store| store.latest(number));
    let decisions: Vec<(String, String)> = stored
        .iter()
        .flat_map(|stored| &stored.brief.decisions)
        .filter_map(|d| Some((d.id.clone()?, d.text.clone())))
        .collect();
    let mut found = rows(&mine, number, &map, &decisions, landed);
    if let Some(stored) = &stored {
        with_evidence(
            &mut found,
            open.as_deref(),
            &stored.head,
            &stored.link_evidence,
        );
    }
    Ok(found)
}

/// The Map diff of PR `number` (`map_diff.rs`, #640), compared as `pr_ratify` compares: none when the PR changed nothing on the map.
pub fn pr_map_diff(
    app: &crate::App,
    project: String,
    number: u64,
) -> Result<Option<crate::map_diff::MapDiff>, String> {
    let open = app
        .prs
        .ready(&project)
        .into_iter()
        .find(|r| r.number == number)
        .map(|r| r.head);
    Ok(crate::map_diff::of(
        Path::new(&project),
        number,
        open.as_deref(),
    ))
}

/// Daniel's decision on one item: `accepted` or `overturned`, with what the item said.
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
mod tests;
