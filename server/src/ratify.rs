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
    /// Old beside new, for a part or a link.
    pub detail: Option<Detail>,
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
                detail: detail(kind, before.get(id), Some(item)),
            });
        }
        for (id, item) in &before {
            if !after.contains_key(id) {
                found.push(MapItem {
                    kind: kind.into(),
                    id: id.clone(),
                    change: "removed".into(),
                    text: words(kind, item),
                    detail: detail(kind, Some(item), None),
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
    let row = |kind: &str, id: &str, text: &str, change: &str, detail: Option<Detail>| {
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
        }
    };
    let mut rows: Vec<Row> = map
        .iter()
        .map(|m| row(&m.kind, &m.id, &m.text, &m.change, m.detail.clone()))
        .collect();
    rows.extend(
        decisions
            .iter()
            .map(|(id, text)| row(DECISION, id, text, "", None)),
    );
    for record in &project.records {
        let missing = record.pr == pr
            && record.kind == DECISION
            && !record.state.is_unknown()
            && !decisions.iter().any(|(id, _)| *id == record.id);
        if missing && !rows.iter().any(|r| r.kind == DECISION && r.id == record.id) {
            rows.push(row(DECISION, &record.id, &record.text, "", None));
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

struct Landed {
    commit: String,
    at: u64,
    pr: Option<u64>,
}

/// The first-parent commits of `reference`, newest first.
fn first_parents(repo: &Path, reference: &str, max: usize) -> Vec<Landed> {
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
enum MapRead {
    /// The map's text.
    Read(String),
    /// Git answered that the commit has no map: its tree does not list the file.
    Absent,
    /// Git did not answer: it did not start, ran past its limit, or failed to show a map the tree lists.
    Unanswered,
}

impl MapRead {
    fn text(&self) -> Option<&str> {
        match self {
            MapRead::Read(text) => Some(text),
            MapRead::Absent | MapRead::Unanswered => None,
        }
    }
}

/// The map at `commit`. A `git show` that exits non-zero is either an answer (the commit has no map) or a failed read (an
/// object out of reach), so the tree is asked which.
fn map_at(repo: &Path, commit: &str) -> MapRead {
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
const WALK: usize = 500;

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

/// The import sites behind each link that `head` adds to the map at `base` (#499, spec #477 "When it is computed"), read from
/// `head` in commit mode (`system_map::check_at`), so an import the PR did not edit counts and the working tree is never read. Only
/// an `import` link has evidence: a run-time link has none, even beside an import edge between the same parts, and neither has a
/// removed link in v1.
pub fn link_evidence(repo: &Path, base: &str, head: &str) -> Vec<LinkEvidence> {
    let (old, new) = (map_at(repo, base), map_at(repo, head));
    let added: Vec<String> = map_items(old.text(), new.text())
        .into_iter()
        .filter(|item| item.kind == "link" && item.change == "added")
        .map(|item| item.id)
        .collect();
    let Some(map) = new
        .text()
        .and_then(|text| system_map::convert_value(text).ok())
    else {
        return Vec::new();
    };
    if added.is_empty() {
        return Vec::new();
    }
    let edges = system_map::check_at(repo, head).edges;
    let links = map["links"].as_array().cloned().unwrap_or_default();
    added
        .into_iter()
        .filter_map(|id| {
            let link = links
                .iter()
                .find(|link| field(link, "id") == id && field(link, "kind") == "import")?;
            let edge = edges
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
        .collect()
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

    /// A version 2 map of part `a` with `purpose`, own `paths` and `holds`, and a link a-b of `link` (its `kind` and `reason`).
    fn v2(purpose: &str, paths: &str, holds: &str, link: &str) -> String {
        format!(
            r#"{{"version":2,
              "parts":[{{"id":"a","name":"A","caption":"c","purpose":"{purpose}","tier":"core","paths":{paths},"holds":{holds}}},
                       {{"id":"b","name":"B","caption":"c","purpose":"p","tier":"core","paths":["b/"],"holds":[]}}],
              "actors":[],"links":[{{"id":"a-b","from":"a","to":"b"{link}}}],"ignored":[],"laws":[],"flows":[]}}"#
        )
    }

    fn part_detail(items: &[MapItem], id: &str) -> PartDetail {
        match items
            .iter()
            .find(|i| i.kind == "part" && i.id == id)
            .and_then(|i| i.detail.clone())
        {
            Some(Detail::Part(detail)) => detail,
            other => panic!("no part detail for {id}: {other:?}"),
        }
    }

    fn words(before: Option<&str>, after: Option<&str>) -> Words {
        Words {
            before: before.map(String::from),
            after: after.map(String::from),
        }
    }

    #[test]
    fn a_changed_purpose_shows_old_beside_new() {
        let old = v2("Keeps the state.", r#"["a/"]"#, "[]", "");
        let new = v2("Keeps and serves the state.", r#"["a/"]"#, "[]", "");
        let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
        assert_eq!(detail.name, words(Some("A"), Some("A")));
        assert_eq!(
            detail.purpose,
            words(
                Some("Keeps the state."),
                Some("Keeps and serves the state.")
            )
        );
        assert!(detail.holds.is_empty());
    }

    #[test]
    fn a_name_held_twice_shows_each_item_once() {
        let old = v2("p", r#"["a/"]"#, "[]", "");
        let new = v2(
            "p",
            r#"["a/"]"#,
            r#"[{"name":"Fog","text":"A"},{"name":"Fog","text":"B"}]"#,
            "",
        );
        let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
        let texts: Vec<&Words> = detail.holds.iter().map(|h| &h.text).collect();
        assert_eq!(texts, [&words(None, Some("A")), &words(None, Some("B"))]);
    }

    #[test]
    fn held_items_are_matched_by_name_and_each_ones_change_shows() {
        let old = v2(
            "p",
            r#"["a/"]"#,
            r#"[{"name":"Fog","text":"Greys a part.","paths":["a/fog.rs"]},
                {"name":"Locks","text":"One writer."},
                {"name":"Same","text":"Unchanged."}]"#,
            "",
        );
        let new = v2(
            "p",
            r#"["a/"]"#,
            r#"[{"name":"Fog","text":"Greys a part a PR changed.","paths":["a/fog.rs","a/fog/"]},
                {"name":"Same","text":"Unchanged."},
                {"name":"Briefs","text":"Tells a PR's story.","paths":["a/briefs.rs"]}]"#,
            "",
        );
        let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
        let held: Vec<(&str, &Words, &[String], &[String])> = detail
            .holds
            .iter()
            .map(|h| {
                (
                    h.name.as_str(),
                    &h.text,
                    h.paths_added.as_slice(),
                    h.paths_removed.as_slice(),
                )
            })
            .collect();
        let s = |list: &[&str]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
        assert_eq!(
            held,
            [
                (
                    "Fog",
                    &words(Some("Greys a part."), Some("Greys a part a PR changed.")),
                    s(&["a/fog/"]).as_slice(),
                    s(&[]).as_slice()
                ),
                (
                    "Same",
                    &words(Some("Unchanged."), Some("Unchanged.")),
                    s(&[]).as_slice(),
                    s(&[]).as_slice()
                ),
                (
                    "Briefs",
                    &words(None, Some("Tells a PR's story.")),
                    s(&["a/briefs.rs"]).as_slice(),
                    s(&[]).as_slice()
                ),
                (
                    "Locks",
                    &words(Some("One writer."), None),
                    s(&[]).as_slice(),
                    s(&[]).as_slice()
                ),
            ],
            "the new side's order, then what was removed"
        );
    }

    #[test]
    fn a_parts_own_paths_change_shows_as_the_patterns_added_and_removed() {
        let old = v2("p", r#"["a/","old/"]"#, "[]", "");
        let new = v2("p", r#"["a/","new/"]"#, "[]", "");
        let detail = part_detail(&map_items(Some(&old), Some(&new)), "a");
        assert_eq!(detail.paths_added, ["new/"]);
        assert_eq!(detail.paths_removed, ["old/"]);
    }

    #[test]
    fn an_added_part_shows_its_purpose_and_held_items_with_nothing_before() {
        let old = v2("p", r#"["a/"]"#, "[]", "");
        let new = old.replace(
            r#"{"id":"b","#,
            r#"{"id":"c","name":"C","caption":"c","purpose":"Installs the hook.","tier":"core","paths":["c/"],
                "holds":[{"name":"Copies","text":"The hook's copies.","paths":["c/copies.rs"]}]},{"id":"b","#,
        );
        let detail = part_detail(&map_items(Some(&old), Some(&new)), "c");
        assert_eq!(detail.name, words(None, Some("C")));
        assert_eq!(detail.purpose, words(None, Some("Installs the hook.")));
        assert_eq!(detail.holds.len(), 1);
        assert_eq!(detail.holds[0].name, "Copies");
        assert_eq!(
            detail.holds[0].text,
            words(None, Some("The hook's copies."))
        );
        assert_eq!(detail.holds[0].paths_added, ["c/copies.rs"]);
        assert_eq!(detail.paths_added, ["c/"]);
    }

    #[test]
    fn a_links_changed_kind_and_reason_show_old_beside_new() {
        let old = v2("p", r#"["a/"]"#, "[]", r#","kind":"import""#);
        let new = v2(
            "p",
            r#"["a/"]"#,
            "[]",
            r#","kind":"http","reason":"The page calls the server over HTTP.""#,
        );
        let items = map_items(Some(&old), Some(&new));
        let link = items
            .iter()
            .find(|i| i.kind == "link")
            .expect("the link changed");
        assert_eq!(link.change, "changed");
        assert_eq!(
            link.detail,
            Some(Detail::Link(LinkDetail {
                kind: words(Some("import"), Some("http")),
                reason: words(Some(""), Some("The page calls the server over HTTP.")),
            }))
        );
    }

    #[test]
    fn a_version_1_side_is_converted_so_its_description_shows_as_the_old_purpose() {
        // OLD is version 1: part a's description is "d". The same map in version 2 with a new purpose and a held item.
        let new = OLD
            .replace(r#""version":1"#, r#""version":2"#)
            .replace(
                r#""id":"a","name":"A","caption":"c","description":"d","tier":"core","paths":["a/"]"#,
                r#""id":"a","name":"A","caption":"c","purpose":"Keeps the state.","tier":"core","paths":["a/"],"holds":[{"name":"Fog","text":"Greys a part."}]"#,
            )
            .replace(r#""description":"d""#, r#""purpose":"d","holds":[]"#);
        let items = map_items(Some(OLD), Some(&new));
        let changed: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.kind.as_str(), i.id.as_str()))
            .collect();
        assert_eq!(
            changed,
            [("part", "a")],
            "b only moved from description to purpose: no change"
        );
        let detail = part_detail(&items, "a");
        assert_eq!(detail.purpose, words(Some("d"), Some("Keeps the state.")));
        assert_eq!(detail.holds[0].text, words(None, Some("Greys a part.")));
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
        // A record of a newer build comes back as it was: its state this build cannot read, and its extra field.
        let kept = &whole["projects"]["/p"]["records"][1];
        assert_eq!(kept["state"], "snoozed");
        assert_eq!(kept["new"], "field");
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
                extra: BTreeMap::new(),
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
        // The first run's own second counts as before it; the next one does not.
        let at = rows(&project, 7, &map, &[], Some(100));
        assert!(at.iter().all(|r| r.status == Status::Ratified));
        let next = rows(&project, 7, &map, &[], Some(101));
        assert!(next.iter().all(|r| r.status == Status::NotYet));
        // An open PR has not landed: nothing is ratified by a record that is not there.
        let open = rows(&project, 7, &map, &[], None);
        assert!(open.iter().all(|r| r.status == Status::NotYet));
    }

    #[test]
    fn an_added_link_carries_the_evidence_its_brief_stored_and_no_other_row_does() {
        let item = |kind: &str, id: &str, change: &str| MapItem {
            kind: kind.into(),
            id: id.into(),
            change: change.into(),
            text: String::new(),
            detail: None,
        };
        let map = [
            item("link", "a-b", "added"),
            item("link", "b-c", "removed"),
            item("link", "c-d", "added"),
            item("part", "a-b", "added"),
        ];
        let mut found = rows(&Project::default(), 7, &map, &[], None);
        let sites = vec![ImportSite {
            file: "a/x.ts".into(),
            line: Some(4),
        }];
        let stored = [
            LinkEvidence {
                link: "a-b".into(),
                sites: sites.clone(),
            },
            LinkEvidence {
                link: "b-c".into(),
                sites: sites.clone(),
            },
        ];
        with_evidence(&mut found, Some("cafe"), "cafe", &stored);
        let got: Vec<(&str, Option<Evidence>)> = found
            .iter()
            .map(|r| (r.id.as_str(), r.evidence.clone()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "a-b",
                    Some(Evidence {
                        head: "cafe".into(),
                        sites
                    })
                ),
                ("b-c", None),
                ("c-d", None),
                ("a-b", None),
            ]
        );
    }

    #[test]
    fn evidence_stored_at_another_head_than_the_rows_were_read_at_is_not_shown() {
        let map = [MapItem {
            kind: "link".into(),
            id: "a-b".into(),
            change: "added".into(),
            text: String::new(),
            detail: None,
        }];
        let stored = [LinkEvidence {
            link: "a-b".into(),
            sites: vec![ImportSite {
                file: "a/x.ts".into(),
                line: Some(4),
            }],
        }];
        // A push since the brief was written: the rows are the new head's, the evidence the old one's.
        let mut found = rows(&Project::default(), 7, &map, &[], None);
        with_evidence(&mut found, Some("beef"), "cafe", &stored);
        assert_eq!(found[0].evidence, None);
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
            let changes = ChangesCache::default();
            // The first run was between the two.
            let mut project = Project {
                since: 1_050_000_000,
                ..Project::default()
            };
            let got: Vec<(u64, &str, String)> = pending(&changes, repo, &project)
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
                extra: BTreeMap::new(),
            });
            assert!(
                !pending(&changes, repo, &project)
                    .iter()
                    .any(|p| p.id == "l1")
            );
            // A record marked pending by hand shows although PR 1 landed before the first run.
            project.records.push(Record {
                pr: 1,
                kind: "law".into(),
                id: "l2".into(),
                state: Decided::Pending,
                at: 0,
                text: String::new(),
                extra: BTreeMap::new(),
            });
            assert!(
                pending(&changes, repo, &project)
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
            let changes = ChangesCache::default();
            // Open: not on main yet.
            let (open, landed) = pr_changes(&changes, repo, 2, Some(&head));
            assert_eq!(landed, None);
            assert_eq!(open.len(), 3);
            // Landed by a squash commit.
            commit(root, &new_map(), "Change laws (#2)", "1200000000");
            let (done, landed) = pr_changes(&changes, repo, 2, None);
            assert_eq!(landed, Some(1_200_000_000));
            assert_eq!(done.len(), 3);
            // A PR that is not ready and not in the log has not landed: a draft, a closed one, or one not looked up yet. Its items
            // are not ratified.
            assert_eq!(pr_changes(&changes, repo, 99, None), (vec![], None));
            // Only when the walk was cut short may it have landed before the cut: long ago.
            assert_eq!(
                pr_changes_in(&changes, repo, 99, None, 1),
                (vec![], Some(0))
            );
        }

        #[test]
        fn a_map_that_git_failed_to_show_is_read_again_on_the_next_call() {
            let (dir, main) = repo();
            let root = dir.path();
            commit(root, OLD, "Map (#1)", "1000000000");
            commit(root, &new_map(), "Change laws (#2)", "1100000000");
            let repo = Path::new(&main);
            let changes = ChangesCache::default();
            // The new map's object is out of reach for a moment: `git log` still answers, `git show` of the map fails.
            let blob = run(root, &["rev-parse", "HEAD:system-map.json"]);
            let object = root.join(".git/objects").join(&blob[..2]).join(&blob[2..]);
            let aside = root.join("aside");
            std::fs::rename(&object, &aside).unwrap();
            let (failed, landed) = pr_changes(&changes, repo, 2, None);
            assert_eq!(landed, Some(1_100_000_000));
            assert_ne!(failed.len(), 3, "the map was read without its object");
            std::fs::rename(&aside, &object).unwrap();
            let (read, _) = pr_changes(&changes, repo, 2, None);
            assert_eq!(read.len(), 3, "the failed read was kept: {read:?}");
        }

        #[test]
        fn a_commit_with_no_map_is_answered_from_the_cache_on_the_next_call() {
            let (dir, main) = repo();
            let root = dir.path();
            commit(root, OLD, "Map (#1)", "1000000000");
            // PR 2 removes the map: git answers that its commit has none, which is an answer to keep.
            std::fs::remove_file(root.join("system-map.json")).unwrap();
            run(root, &["add", "-A"]);
            run(
                root,
                &[
                    "-c",
                    "user.name=S",
                    "-c",
                    "user.email=s@e.com",
                    "commit",
                    "-q",
                    "-m",
                    "Drop the map (#2)",
                ],
            );
            let head = run(root, &["rev-parse", "HEAD"]);
            let repo = Path::new(&main);
            let changes = ChangesCache::default();
            assert_eq!(changes_of(&changes, repo, &head), vec![]);
            let key = (repo.display().to_string(), head.clone());
            assert!(
                changes.lock().unwrap().contains_key(&key),
                "a commit with no map is read again on every call"
            );
        }

        #[test]
        fn a_commit_dated_before_the_first_run_does_not_end_the_walk_of_what_landed_after_it() {
            let (dir, main) = repo();
            let root = dir.path();
            commit(root, OLD, "Map (#0)", "800000000");
            commit(root, &new_map(), "Change laws (#1)", "1050000000");
            // Landed later but dated earlier (a rebased or backdated commit), and it changes no item.
            commit(
                root,
                &format!("{}\n", new_map()),
                "Reformat (#2)",
                "900000000",
            );
            let project = Project {
                since: 1_000_000_000,
                ..Project::default()
            };
            let got = pending(&ChangesCache::default(), Path::new(&main), &project);
            assert!(
                got.iter().any(|p| p.pr == 1 && p.id == "l3"),
                "PR 1 landed after the first run: {got:?}"
            );
        }

        #[test]
        fn a_sites_snippet_is_read_from_the_commit_around_its_line() {
            let (dir, main) = repo();
            let root = dir.path();
            let text: String = (1..=20).map(|n| format!("line {n}\n")).collect();
            std::fs::create_dir_all(root.join("alpha")).unwrap();
            std::fs::write(root.join("alpha/a.ts"), &text).unwrap();
            commit(root, OLD, "Map (#1)", "1000000000");
            let head = run(root, &["rev-parse", "HEAD"]);
            let head = head.trim();
            // The working tree moved on; the commit's text is the one read.
            std::fs::write(root.join("alpha/a.ts"), "changed\n").unwrap();
            let repo = Path::new(&main);
            let lines = |from: u32, to: u32| -> Vec<String> {
                (from..=to).map(|n| format!("line {n}")).collect()
            };
            let at = site_snippet(repo, head, "alpha/a.ts", Some(10)).unwrap();
            assert_eq!(
                at,
                SiteSnippet {
                    file: "alpha/a.ts".into(),
                    line: Some(10),
                    first: 6,
                    lines: lines(6, 14),
                }
            );
            // Near the top, and with no line: the file's first lines.
            assert_eq!(
                site_snippet(repo, head, "alpha/a.ts", Some(2))
                    .unwrap()
                    .first,
                1
            );
            let top = site_snippet(repo, head, "alpha/a.ts", None).unwrap();
            assert_eq!((top.first, top.lines), (1, lines(1, 9)));
            assert_eq!(
                site_snippet(repo, head, "alpha/a.ts", Some(20))
                    .unwrap()
                    .lines,
                lines(16, 20)
            );
            // A name that is no commit's hex is refused before git sees it.
            assert!(site_snippet(repo, "--output=x", "alpha/a.ts", None).is_err());
            assert!(site_snippet(repo, head, "alpha/none.ts", None).is_err());
            // A directory, or no file at all, is a tree: git would list it, and that is no site's code.
            assert!(site_snippet(repo, head, "alpha", None).is_err());
            assert!(site_snippet(repo, head, "", None).is_err());
        }

        /// A version 2 map of `alpha`, `beta` and `gamma`, read by `typescript-imports`, with `links`.
        fn evidence_map(links: serde_json::Value) -> String {
            let part = |id: &str| {
                serde_json::json!({ "id": id, "name": id, "caption": "c", "purpose": "p", "holds": [], "tier": "core",
                                    "paths": [format!("{id}/")] })
            };
            serde_json::json!({
                "version": 2, "extractors": ["typescript-imports"],
                "parts": [part("alpha"), part("beta"), part("gamma")], "actors": [],
                "links": links, "ignored": ["system-map.json"], "laws": [], "flows": []
            })
            .to_string()
        }

        #[test]
        fn the_evidence_of_each_added_import_link_is_its_import_sites_read_from_the_head() {
            let (dir, main) = repo();
            let root = dir.path();
            let put = |path: &str, text: &str| {
                std::fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
                std::fs::write(root.join(path), text).unwrap();
            };
            let link = |id: &str, from: &str, to: &str, kind: &str| serde_json::json!({ "id": id, "from": from, "to": to, "kind": kind });
            put("beta/b.ts", "export const b = 1;\n");
            put("gamma/g.ts", "export const g = 1;\n");
            // An import the PR does not edit: it is in the head, not in the diff.
            put(
                "alpha/old.ts",
                "// old\nimport { g } from \"../gamma/g\";\n",
            );
            put("alpha/a.ts", "import { b } from \"../beta/b\";\n");
            let ab = link("alpha-beta", "alpha", "beta", "import");
            commit(
                root,
                &evidence_map(serde_json::json!([ab])),
                "Map (#1)",
                "1000000000",
            );
            let base = run(root, &["rev-parse", "HEAD"]);
            put(
                "alpha/a.ts",
                "import { b } from \"../beta/b\";\n\nimport { g } from \"../gamma/g\";\n",
            );
            put(
                "alpha/c.ts",
                "import { g } from \"../gamma/g\";\nexport { g } from \"../gamma/g\";\n",
            );
            let links = serde_json::json!([
                ab,
                link("alpha-gamma", "alpha", "gamma", "import"),
                // Added, but no edge stands behind an http link: no evidence.
                link("gamma-beta", "gamma", "beta", "http"),
                // A run-time link beside the import edge between the same parts: the edge is the import link's, not its.
                link("alpha-gamma-spawn", "alpha", "gamma", "spawn"),
            ]);
            commit(root, &evidence_map(links), "Gamma (#2)", "1100000000");
            let head = run(root, &["rev-parse", "HEAD"]);
            // The working tree is not read: only the head's files count.
            put("alpha/c.ts", "");
            let site = |file: &str, line: u32| ImportSite {
                file: file.into(),
                line: Some(line),
            };
            assert_eq!(
                link_evidence(Path::new(&main), base.trim(), head.trim()),
                [LinkEvidence {
                    link: "alpha-gamma".into(),
                    sites: vec![
                        site("alpha/a.ts", 3),
                        site("alpha/c.ts", 1),
                        site("alpha/c.ts", 2),
                        site("alpha/old.ts", 2),
                    ],
                }]
            );
        }
    }
}
