//! First map's drafting run (spec #753, "The drafting run", ticket #762): Draft the map splits the skeleton's parts into at most
//! four groups and starts one batch of read-only writers, a writer per group, that name and describe the parts. When a group's
//! writer lands, its reviewer joins the batch and checks the group's purposes with the Map text review's prompt
//! (`map_review::prompt`, First map's wording). What they give back is kept in the **Map draft**, `<data>/first-map/<project>.json`,
//! until Discard (or Accept, a later ticket) deletes it, so it survives a restart.
//!
//! - **Ends.** The runner tells each First map run's end ([`Runner::on_end`]) while its batch still holds the project's slot, so
//!   the reviewer joined then never waits behind another request. The run's answer is read, kept in its group, and the run is
//!   dismissed: its folder only holds a copy of the repository.
//! - **Failure.** A writer or reviewer that ends badly, or answers no JSON, fails its group with why; the other groups stand.
//!   Retry runs the failed step again, joining the batch. A login or limit failure pauses the runner's queues, so a retry then
//!   waits for Resume.
//! - **Redraft this part** adds a group of one part, whose writer is given the part's flag, then its reviewer.
//! - **The lock.** One lock serialises every read, change and write of a Map draft, and is held across a submission, so an end
//!   told meanwhile waits until the run is written down. The runner never calls in here with its own lock held.

use crate::map_review::{self, ItemKind, Judged, ReviewItem, VERDICTS, Verdict, Wording};
use crate::records::Notice;
use crate::runner::{End, Kind, Limits, Outcome, Request, Runner};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cmp::Reverse,
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};

/// The most writer groups a draft has, so the writers read the same files as few times as they can (#753, Q24).
const MAX_GROUPS: usize = 4;
/// A group with more parts than this splits by size while there are fewer than [`MAX_GROUPS`].
const GROUP_PARTS: usize = 5;
/// The Map draft's format.
const VERSION: u32 = 1;
/// The tiers a part may be set to.
const TIERS: [&str; 3] = ["core", "interface", "black-box"];

/// Where the Map drafts are kept.
pub struct Config {
    /// `<data>/first-map`: a file per project, beside its `.pending.json`.
    pub store: PathBuf,
}

/// Where a group stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GroupState {
    /// Its writer is queued or running.
    Writing,
    /// Its writer landed and its reviewer is queued or running: "being checked".
    Checking,
    /// Its reviewer answered on every purpose, or it wrote none.
    Done,
    /// Its writer failed, timed out or answered no JSON: Retry runs the writer again.
    WriterFailed,
    /// Its reviewer failed, timed out or left a purpose without a verdict: Retry runs the reviewer again.
    ReviewerFailed,
}

/// The text a writer gave one part; a field it left out keeps the skeleton's placeholder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PartText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// The lines the purpose rests on, as `path:start-end`.
    #[serde(default)]
    pub cites: Vec<String>,
}

/// What a writer left out as ungrounded, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotWritten {
    pub what: String,
    pub why: String,
}

/// The flag a redraft's writer is given: the reviewer's verdict on the part's last purpose, why, and the lines it cites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Flag {
    pub verdict: String,
    pub why: String,
    #[serde(default)]
    pub cites: Vec<String>,
}

/// The person's own edit of one part, over what the writers gave; a field left out is not edited.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Edit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    /// `core`, `interface` or `black-box`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
}

/// One writer group as the Map draft keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Group {
    /// Its parts' ids, in the skeleton's order.
    parts: Vec<String>,
    state: GroupState,
    /// The run of its writer or reviewer while one is queued or running.
    #[serde(default)]
    run: Option<String>,
    /// Why it failed.
    #[serde(default)]
    why: Option<String>,
    /// A Redraft this part, of its one part.
    #[serde(default)]
    redraft: bool,
    /// The flag its redraft's writer is given.
    #[serde(default)]
    flag: Option<Flag>,
    /// What its writer gave, by part id.
    #[serde(default)]
    text: BTreeMap<String, PartText>,
    #[serde(default)]
    not_written: Vec<NotWritten>,
    /// Its reviewer's verdicts on its `part:<id>/purpose` items.
    #[serde(default)]
    verdicts: Vec<Verdict>,
}

/// A project's Map draft as its file keeps it. A reader ignores a field it does not know.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Draft {
    version: u32,
    /// The main checkout.
    project: String,
    /// The commit whose files the runs read.
    head: String,
    /// The skeleton: a version 2 map's text, as the scan screen gave it.
    skeleton: String,
    /// The batch its runs join, named by its first run ([`Runner::submit_batch`]).
    #[serde(default)]
    batch: Option<String>,
    /// The writer groups, then a group per Redraft this part, in the order they were started.
    groups: Vec<Group>,
    /// The person's edits, by part id.
    #[serde(default)]
    edits: BTreeMap<String, Edit>,
}

/// A group as the page shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GroupView {
    pub parts: Vec<String>,
    pub state: GroupState,
    pub why: Option<String>,
    pub redraft: bool,
}

/// A project's Map draft as the page shows it (`map_draft`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapDraftView {
    /// The map as it stands: the skeleton with each part's latest written text and the person's edits, as text the page parses.
    pub map: String,
    /// The groups, by index: what `map_draft_retry` takes.
    pub groups: Vec<GroupView>,
    /// The reviewer's verdict on each part's purpose as `map` shows it, by part id; none while it is being checked.
    pub verdicts: BTreeMap<String, Judged>,
    /// What the writers left out as ungrounded.
    pub not_written: Vec<NotWritten>,
    /// Whether every group is done, so Accept may unlock.
    pub ready: bool,
}

/// Every project's Map draft and the runs that fill it.
pub struct MapDrafts {
    config: Config,
    runner: Weak<Runner>,
    /// Serialises the read, change and write of a Map draft.
    writing: Mutex<()>,
    changed: Notice,
}

impl MapDrafts {
    /// The Map drafts kept in `config.store`, filled by `runner`'s runs: it is told each run's end, so make it before the
    /// runner's `pick_up`.
    pub fn new(config: Config, runner: &Arc<Runner>) -> Arc<MapDrafts> {
        let drafts = Arc::new(MapDrafts {
            config,
            runner: Arc::downgrade(runner),
            writing: Mutex::default(),
            changed: Notice::default(),
        });
        let me = Arc::downgrade(&drafts);
        // Told on the thread that saw the end, while the batch still holds the slot: a reviewer joins it here.
        runner.on_end(move |ended| {
            if ended.kind != Kind::FirstMap {
                return;
            }
            if let Some(drafts) = me.upgrade() {
                drafts.settle(&ended.project, &ended.id);
            }
        });
        drafts
    }

    /// Sets what is told the project whose Map draft changed; the first call wins.
    pub fn on_change(&self, changed: impl Fn(&str) + Send + Sync + 'static) {
        self.changed.set(changed);
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.writing.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn runner(&self) -> Result<Arc<Runner>, String> {
        self.runner
            .upgrade()
            .ok_or_else(|| "plotkeeper is stopping".into())
    }

    /// The Map draft's file of the main checkout `main`: its path with each character but a letter or digit made `_`.
    fn file(&self, main: &str) -> PathBuf {
        let name: String = main
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        self.config.store.join(format!("{name}.json"))
    }

    fn read(&self, main: &str) -> Option<Draft> {
        let text = fs::read(self.file(main)).ok()?;
        match serde_json::from_slice::<Draft>(&text) {
            Ok(draft) => Some(draft),
            Err(e) => {
                log::warn!("first map: the Map draft of {main} does not read: {e}");
                None
            }
        }
    }

    fn write(&self, draft: &Draft) -> Result<(), String> {
        let saved = fs::create_dir_all(&self.config.store).and_then(|()| {
            let text = serde_json::to_vec(draft).map_err(io::Error::other)?;
            studio_workbench::write_atomic(&self.file(&draft.project), &text)
        });
        saved.map_err(|e| {
            log::error!("save the Map draft of {}: {e}", draft.project);
            format!("the Map draft could not be saved: {e}")
        })
    }

    /// Draft the map on `project`'s main checkout, from `skeleton` (a version 2 map's text, as the scan screen has it), or a
    /// fresh scan when the page gives none: one batch of writers, a writer per group. Refused when a Map draft is there
    /// already, when the repository has a map, or when the runner refuses the batch.
    pub fn start(&self, project: &str, skeleton: Option<String>) -> Result<MapDraftView, String> {
        let main = studio_workbench::main_checkout(Path::new(project))?;
        let root = Path::new(&main);
        let runner = self.runner()?;
        let guard = self.lock();
        if self.file(&main).exists() {
            return Err("A Map draft is there already: accept or discard it first".into());
        }
        if crate::map::read_map(root).status != crate::map::Status::None {
            return Err("This repository has a map already".into());
        }
        let skeleton = match skeleton {
            Some(skeleton) => skeleton,
            None => crate::map_scan::scan_of(root, true)?.map,
        };
        let map = skeleton_of(&skeleton)?;
        let head = crate::map::git(root, &["rev-parse", "HEAD"])
            .map(|head| head.trim().to_string())
            .map_err(|e| format!("The repository has no commit to draft from: {e}"))?;
        let files: BTreeMap<String, usize> =
            system_map::check_text(root, system_map::tracked_files(root)?, &skeleton)
                .parts
                .into_iter()
                .map(|part| (part.id, part.files.len()))
                .collect();
        let sized: Vec<Sized> = parts_of(&map)
            .map(|part| Sized {
                id: str_of(part, "id").to_string(),
                zone: part["zone"].as_str().map(str::to_string),
                files: files.get(str_of(part, "id")).copied().unwrap_or(0),
            })
            .collect();
        let groups = group_parts(&sized);
        let ignored = ignored_of(&map);
        let mut requests = Vec::new();
        for parts in &groups {
            let folder = runner.new_folder()?;
            fill(root, &head, &folder, &skeleton)?;
            requests.push(Request {
                kind: Kind::FirstMap,
                project: main.clone(),
                folder,
                prompt: writer_prompt(parts, &ignored, None),
                limits: Limits::of(Kind::FirstMap),
            });
        }
        let ids = runner
            .submit_batch(requests)
            .map_err(|refusal| refusal.line)?;
        let draft = Draft {
            version: VERSION,
            project: main.clone(),
            head,
            skeleton,
            batch: ids.first().cloned(),
            groups: groups
                .into_iter()
                .zip(&ids)
                .map(|(parts, id)| Group {
                    parts,
                    state: GroupState::Writing,
                    run: Some(id.clone()),
                    why: None,
                    redraft: false,
                    flag: None,
                    text: BTreeMap::new(),
                    not_written: Vec::new(),
                    verdicts: Vec::new(),
                })
                .collect(),
            edits: BTreeMap::new(),
        };
        if let Err(e) = self.write(&draft) {
            drop(guard);
            if let Some(batch) = &draft.batch
                && let Err(why) = runner.cancel_batch(batch)
            {
                log::warn!("first map: the batch of {main} was not cancelled: {why}");
            }
            return Err(e);
        }
        drop(guard);
        // A run that could not start ended with no end told.
        ids.iter().for_each(|id| self.settle(&main, id));
        self.changed.tell(&main);
        self.view_of(&main)
            .ok_or_else(|| "The Map draft is gone".into())
    }

    /// `project`'s Map draft as the page shows it; none when there is none, as for a folder in no git repository.
    pub fn view(&self, project: &str) -> Option<MapDraftView> {
        let main = studio_workbench::main_checkout(Path::new(project)).ok()?;
        self.view_of(&main)
    }

    fn view_of(&self, main: &str) -> Option<MapDraftView> {
        let draft = self.read(main)?;
        let mut map: Value = serde_json::from_str(&draft.skeleton).ok()?;
        let mut verdicts = BTreeMap::new();
        if let Some(parts) = map["parts"].as_array_mut() {
            for part in parts {
                let id = str_of(part, "id").to_string();
                if let Some(group) = shown(&draft, &id) {
                    apply(part, &group.text[&id]);
                    let item = format!("part:{id}/purpose");
                    if let Some(verdict) = group.verdicts.iter().find(|v| v.id == item) {
                        verdicts.insert(id.clone(), judged(verdict));
                    }
                }
                if let Some(edit) = draft.edits.get(&id) {
                    apply_edit(part, edit);
                }
            }
        }
        Some(MapDraftView {
            map: map.to_string(),
            groups: draft
                .groups
                .iter()
                .map(|group| GroupView {
                    parts: group.parts.clone(),
                    state: group.state,
                    why: group.why.clone(),
                    redraft: group.redraft,
                })
                .collect(),
            verdicts,
            not_written: draft
                .groups
                .iter()
                .flat_map(|group| group.not_written.iter().cloned())
                .collect(),
            ready: draft
                .groups
                .iter()
                .all(|group| group.state == GroupState::Done),
        })
    }

    /// Run `id` of `main`'s Map draft ended: its answer is kept in its group, a writer's reviewer is started, and the run is
    /// dismissed. Nothing for a run that is not ended, or that no group waits on, so an end read twice is read once.
    fn settle(&self, main: &str, id: &str) {
        let Ok(runner) = self.runner() else {
            return;
        };
        let Some(outcome) = runner.outcome(id) else {
            return;
        };
        let guard = self.lock();
        let Some(mut draft) = self.read(main) else {
            return;
        };
        let Some(at) = draft
            .groups
            .iter()
            .position(|group| group.run.as_deref() == Some(id))
        else {
            return;
        };
        draft.groups[at].run = None;
        match draft.groups[at].state {
            GroupState::Writing => self.wrote(&runner, &mut draft, at, &outcome),
            GroupState::Checking => reviewed(&mut draft.groups[at], &outcome),
            GroupState::Done | GroupState::WriterFailed | GroupState::ReviewerFailed => {}
        }
        // A draft that cannot be saved is logged; the run is kept, for its prompt to copy.
        let saved = self.write(&draft);
        drop(guard);
        if saved.is_ok()
            && let Err(e) = runner.cancel(id)
        {
            log::warn!("first map: run {id} was not dismissed: {e}");
        }
        self.changed.tell(main);
    }

    /// Group `at`'s writer ended with `outcome`: a finished run's text is kept and its reviewer joins the batch, or the group
    /// is done when it wrote no purpose. Any other end, or an answer with no JSON, fails the group.
    fn wrote(&self, runner: &Runner, draft: &mut Draft, at: usize, outcome: &Outcome) {
        let group = &mut draft.groups[at];
        if !matches!(outcome.end, End::Done | End::Denied) {
            fail(group, GroupState::WriterFailed, ended("writer", outcome));
            return;
        }
        let Some((text, not_written)) = read_writing(&outcome.result, &group.parts) else {
            fail(
                group,
                GroupState::WriterFailed,
                "The writer's answer held no JSON".into(),
            );
            return;
        };
        if group.redraft {
            // The redrafted text replaces the person's edits of it; a tier stays theirs.
            for id in &group.parts {
                if let Some(edit) = draft.edits.get_mut(id) {
                    *edit = Edit {
                        tier: edit.tier.take(),
                        ..Edit::default()
                    };
                }
            }
        }
        let group = &mut draft.groups[at];
        group.text = text;
        group.not_written = not_written;
        group.verdicts.clear();
        self.check(runner, draft, at);
    }

    /// Starts group `at`'s reviewer over its purposes, joining the batch: the group is being checked, done when it wrote no
    /// purpose, or failed when the run cannot start.
    fn check(&self, runner: &Runner, draft: &mut Draft, at: usize) {
        let items = items_of(&draft.groups[at]);
        if items.is_empty() {
            let group = &mut draft.groups[at];
            group.state = GroupState::Done;
            group.why = None;
            return;
        }
        let submitted = (|| {
            let folder = runner.new_folder()?;
            let root = Path::new(&draft.project);
            let only = map_with(&draft.skeleton, &draft.groups[at].text);
            fill(root, &draft.head, &folder, &only)?;
            self.join(
                runner,
                draft,
                Request {
                    kind: Kind::FirstMap,
                    project: draft.project.clone(),
                    folder,
                    prompt: map_review::prompt(&items, Wording::DRAFT),
                    limits: Limits::of(Kind::FirstMap),
                },
            )
        })();
        let group = &mut draft.groups[at];
        match submitted {
            Ok(id) => {
                group.state = GroupState::Checking;
                group.run = Some(id);
                group.why = None;
            }
            Err(why) => fail(group, GroupState::ReviewerFailed, why),
        }
    }

    /// Queues `request` in the draft's batch: at once while the batch holds the slot, else in its turn.
    fn join(&self, runner: &Runner, draft: &Draft, request: Request) -> Result<String, String> {
        match &draft.batch {
            Some(batch) => runner.join(batch, request),
            None => runner.submit(request),
        }
        .map_err(|refusal| refusal.line)
    }

    /// Starts group `at`'s writer, joining the batch.
    fn write_group(&self, runner: &Runner, draft: &mut Draft, at: usize) -> Result<String, String> {
        let folder = runner.new_folder()?;
        let root = Path::new(&draft.project);
        fill(root, &draft.head, &folder, &draft.skeleton)?;
        let map = skeleton_of(&draft.skeleton)?;
        let group = &draft.groups[at];
        let prompt = writer_prompt(&group.parts, &ignored_of(&map), group.flag.as_ref());
        self.join(
            runner,
            draft,
            Request {
                kind: Kind::FirstMap,
                project: draft.project.clone(),
                folder,
                prompt,
                limits: Limits::of(Kind::FirstMap),
            },
        )
    }

    /// Retry of failed group `group` (its index in the view): its writer or its reviewer runs again as a new run, joining the
    /// batch; the other groups stand.
    pub fn retry(&self, project: &str, group: usize) -> Result<MapDraftView, String> {
        let main = studio_workbench::main_checkout(Path::new(project))?;
        let runner = self.runner()?;
        let guard = self.lock();
        let mut draft = self.read(&main).ok_or("There is no Map draft")?;
        let state = draft
            .groups
            .get(group)
            .map(|group| group.state)
            .ok_or("There is no such group")?;
        match state {
            GroupState::WriterFailed => {
                let id = self.write_group(&runner, &mut draft, group)?;
                let kept = &mut draft.groups[group];
                kept.state = GroupState::Writing;
                kept.run = Some(id);
                kept.why = None;
            }
            GroupState::ReviewerFailed => {
                self.check(&runner, &mut draft, group);
                if let Some(why) = draft.groups[group].why.clone() {
                    return Err(why);
                }
            }
            _ => return Err("That group has not failed".into()),
        }
        let run = draft.groups[group].run.clone();
        self.write(&draft)?;
        drop(guard);
        if let Some(id) = run {
            self.settle(&main, &id);
        }
        self.changed.tell(&main);
        self.view_of(&main)
            .ok_or_else(|| "The Map draft is gone".into())
    }

    /// Redraft this part: one writer over `part`, given its flag when its purpose has one, then that part's reviewer. Accept is
    /// locked until it answers. Refused while the part is being written or checked.
    pub fn redraft(&self, project: &str, part: &str) -> Result<MapDraftView, String> {
        let main = studio_workbench::main_checkout(Path::new(project))?;
        let runner = self.runner()?;
        let guard = self.lock();
        let mut draft = self.read(&main).ok_or("There is no Map draft")?;
        let map = skeleton_of(&draft.skeleton)?;
        if !parts_of(&map).any(|each| str_of(each, "id") == part) {
            return Err(format!("The skeleton has no part {part}"));
        }
        let busy = draft.groups.iter().any(|group| {
            group.parts.iter().any(|id| id == part)
                && matches!(group.state, GroupState::Writing | GroupState::Checking)
        });
        if busy {
            return Err("This part is being written or checked".into());
        }
        let item = format!("part:{part}/purpose");
        let flag = shown(&draft, part)
            .and_then(|group| group.verdicts.iter().find(|v| v.id == item))
            .filter(|verdict| verdict.verdict != VERDICTS[0])
            .map(|verdict| Flag {
                verdict: verdict.verdict.clone(),
                why: verdict.why.clone(),
                cites: verdict.cites.clone(),
            });
        draft.groups.push(Group {
            parts: vec![part.to_string()],
            state: GroupState::Writing,
            run: None,
            why: None,
            redraft: true,
            flag,
            text: BTreeMap::new(),
            not_written: Vec::new(),
            verdicts: Vec::new(),
        });
        let at = draft.groups.len() - 1;
        let id = self.write_group(&runner, &mut draft, at)?;
        draft.groups[at].run = Some(id.clone());
        self.write(&draft)?;
        drop(guard);
        self.settle(&main, &id);
        self.changed.tell(&main);
        self.view_of(&main)
            .ok_or_else(|| "The Map draft is gone".into())
    }

    /// The person's edit of `part`: each field given replaces what the map shows, and a redraft of the part replaces it again.
    pub fn edit(&self, project: &str, part: &str, edit: Edit) -> Result<MapDraftView, String> {
        let main = studio_workbench::main_checkout(Path::new(project))?;
        let guard = self.lock();
        let mut draft = self.read(&main).ok_or("There is no Map draft")?;
        let map = skeleton_of(&draft.skeleton)?;
        if !parts_of(&map).any(|each| str_of(each, "id") == part) {
            return Err(format!("The skeleton has no part {part}"));
        }
        if let Some(tier) = &edit.tier
            && !TIERS.contains(&tier.as_str())
        {
            return Err(format!("{tier} is not a tier"));
        }
        let kept = draft.edits.entry(part.to_string()).or_default();
        for (field, given) in [
            (&mut kept.name, edit.name),
            (&mut kept.caption, edit.caption),
            (&mut kept.purpose, edit.purpose),
            (&mut kept.tier, edit.tier),
        ] {
            if given.is_some() {
                *field = given;
            }
        }
        self.write(&draft)?;
        drop(guard);
        self.changed.tell(&main);
        self.view_of(&main)
            .ok_or_else(|| "The Map draft is gone".into())
    }

    /// Discard: every run of the draft's batch is cancelled, the queued ones dropped and the running ones stopped, which frees
    /// the project's slot, and the Map draft is deleted. Refused, keeping the draft, when a run cannot be stopped.
    pub fn discard(&self, project: &str) -> Result<(), String> {
        let main = studio_workbench::main_checkout(Path::new(project))?;
        let runner = self.runner()?;
        let guard = self.lock();
        let Some(draft) = self.read(&main) else {
            let file = self.file(&main);
            // A draft that does not read goes too.
            return match fs::remove_file(&file) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        };
        if let Some(batch) = &draft.batch {
            runner.cancel_batch(batch)?;
        }
        fs::remove_file(self.file(&main)).map_err(|e| e.to_string())?;
        drop(guard);
        self.changed.tell(&main);
        Ok(())
    }

    /// At a start, after the runner's `pick_up`, which told the ends of the runs that ended meanwhile: a group whose run the
    /// runner no longer knows (gone with plotkeeper, or never written down) fails, retryable, and the groups already landed
    /// stay as they are.
    pub fn pick_up(&self) {
        let Ok(runner) = self.runner() else {
            return;
        };
        let files = fs::read_dir(&self.config.store)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                name.ends_with(".json") && !name.ends_with(".pending.json")
            });
        for file in files {
            let Some(main) = fs::read(&file)
                .ok()
                .and_then(|text| serde_json::from_slice::<Draft>(&text).ok())
                .map(|draft| draft.project)
            else {
                continue;
            };
            let guard = self.lock();
            let Some(mut draft) = self.read(&main) else {
                continue;
            };
            let known: Vec<String> = runner
                .runs(&main)
                .runs
                .into_iter()
                .map(|run| run.id)
                .collect();
            let mut lost = false;
            let mut ended = Vec::new();
            for group in &mut draft.groups {
                let failed = match group.state {
                    GroupState::Writing => GroupState::WriterFailed,
                    GroupState::Checking => GroupState::ReviewerFailed,
                    _ => continue,
                };
                match &group.run {
                    Some(id) if known.contains(id) => ended.push(id.clone()),
                    _ => {
                        fail(
                            group,
                            failed,
                            "Its run was lost when plotkeeper stopped: retry it".into(),
                        );
                        lost = true;
                    }
                }
            }
            if lost {
                // Logged when it fails.
                self.write(&draft).ok();
            }
            drop(guard);
            ended.iter().for_each(|id| self.settle(&main, id));
            self.changed.tell(&main);
        }
    }
}

/// Fails `group` at `state` with `why`.
fn fail(group: &mut Group, state: GroupState, why: String) {
    group.state = state;
    group.run = None;
    group.why = Some(why);
}

/// Why a `role` run that did not finish ended as it did.
fn ended(role: &str, outcome: &Outcome) -> String {
    match outcome.end {
        End::TimedOut => format!("The {role} stopped at its time limit"),
        End::Died => format!("The {role}'s process went away before it finished"),
        End::NotLoggedIn => {
            format!("Claude is not logged in: log in, Resume the runs, then retry the {role}")
        }
        End::RateLimited => {
            format!("Claude hit its usage or rate limit: Resume the runs, then retry the {role}")
        }
        End::Done | End::Denied | End::Failed | End::SetupFailed => {
            let first = outcome
                .result
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or_default();
            match cut(first, 200) {
                why if why.is_empty() => format!("The {role} failed"),
                why => format!("The {role} failed: {why}"),
            }
        }
    }
}

/// Group's reviewer ended with `outcome`: its verdicts are kept, and the group is done when every purpose has one. Any other
/// end fails it.
fn reviewed(group: &mut Group, outcome: &Outcome) {
    if !matches!(outcome.end, End::Done | End::Denied) {
        fail(
            group,
            GroupState::ReviewerFailed,
            ended("reviewer", outcome),
        );
        return;
    }
    let items = items_of(group);
    group.verdicts = map_review::read_answer(&outcome.result, &items);
    let left = items.len() - group.verdicts.len();
    if left > 0 {
        let why = format!(
            "The review gave no verdict for {left} of {} purposes",
            items.len()
        );
        fail(group, GroupState::ReviewerFailed, why);
    } else {
        group.state = GroupState::Done;
        group.why = None;
    }
}

/// The group whose text the map shows for part `id`: the latest that wrote some.
fn shown<'a>(draft: &'a Draft, id: &str) -> Option<&'a Group> {
    draft
        .groups
        .iter()
        .rev()
        .find(|group| group.text.contains_key(id))
}

fn judged(verdict: &Verdict) -> Judged {
    Judged {
        id: verdict.id.clone(),
        verdict: verdict.verdict.clone(),
        why: verdict.why.clone(),
        cites: verdict.cites.clone(),
        flagged: verdict.verdict != VERDICTS[0],
    }
}

/// The review items of `group`: the `part:<id>/purpose` of each part it wrote a purpose for.
fn items_of(group: &Group) -> Vec<ReviewItem> {
    group
        .parts
        .iter()
        .filter_map(|id| {
            let purpose = group.text.get(id)?.purpose.as_deref()?;
            Some(ReviewItem {
                id: format!("part:{id}/purpose"),
                hash: map_review::hash(purpose),
                kind: ItemKind::Changed,
            })
        })
        .collect()
}

/// The skeleton `text` as a version 2 map with parts, or why not.
fn skeleton_of(text: &str) -> Result<Value, String> {
    let refused = |why: String| format!("The skeleton does not read: {why}");
    system_map::parse_lenient(text).map_err(refused)?;
    let map: Value = serde_json::from_str(text).map_err(|e| refused(e.to_string()))?;
    if map["version"] != 2 {
        return Err(refused("it is not a version 2 map".into()));
    }
    if parts_of(&map).next().is_none() {
        return Err(refused("it has no part".into()));
    }
    Ok(map)
}

fn parts_of(map: &Value) -> impl Iterator<Item = &Value> {
    map["parts"].as_array().into_iter().flatten()
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn ignored_of(map: &Value) -> Vec<String> {
    map["ignored"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|path| path.as_str().map(str::to_string))
        .collect()
}

/// `part` with `text`'s fields in place of its own.
fn apply(part: &mut Value, text: &PartText) {
    for (field, given) in [
        ("name", &text.name),
        ("caption", &text.caption),
        ("purpose", &text.purpose),
    ] {
        if let Some(given) = given {
            part[field] = given.as_str().into();
        }
    }
}

fn apply_edit(part: &mut Value, edit: &Edit) {
    for (field, given) in [
        ("name", &edit.name),
        ("caption", &edit.caption),
        ("purpose", &edit.purpose),
        ("tier", &edit.tier),
    ] {
        if let Some(given) = given {
            part[field] = given.as_str().into();
        }
    }
}

/// The skeleton with `text` alone written in, the other parts' placeholders kept: a reviewer's map, which never holds the
/// drafts of other groups.
fn map_with(skeleton: &str, text: &BTreeMap<String, PartText>) -> String {
    let Ok(mut map) = serde_json::from_str::<Value>(skeleton) else {
        return skeleton.to_string();
    };
    if let Some(parts) = map["parts"].as_array_mut() {
        for part in parts {
            if let Some(text) = text.get(str_of(part, "id")) {
                apply(part, text);
            }
        }
    }
    map.to_string()
}

/// Fills a run's working `folder` with the repository's files at `head` and `map` as its `system-map.json`.
fn fill(root: &Path, head: &str, folder: &Path, map: &str) -> Result<(), String> {
    crate::micro_worlds::archive(root, head, folder)?;
    fs::write(folder.join(system_map::MAP_FILE), map)
        .map_err(|e| format!("the run's map could not be written: {e}"))
}

/// The text cut to `n` characters, trimmed.
fn cut(text: &str, n: usize) -> String {
    text.trim().chars().take(n).collect()
}

/// The text a writer's answer `result` gives `parts`: the JSON object in it, each part's name, caption and purpose (cut to
/// their limits, with room to spare) and its cites, and what it left out. None when it holds no JSON object with `parts`.
fn read_writing(
    result: &str,
    parts: &[String],
) -> Option<(BTreeMap<String, PartText>, Vec<NotWritten>)> {
    let json = match (result.find('{'), result.rfind('}')) {
        (Some(from), Some(to)) if from < to => &result[from..=to],
        _ => return None,
    };
    let answer: Value = serde_json::from_str(json).ok()?;
    let written = answer["parts"].as_object()?;
    let field = |entry: &Value, key: &str, n: usize| {
        entry[key]
            .as_str()
            .map(|text| cut(text, n))
            .filter(|text| !text.is_empty())
    };
    let text = parts
        .iter()
        .filter_map(|id| {
            let entry = written.get(id)?;
            let text = PartText {
                name: field(entry, "name", 80),
                caption: field(entry, "caption", 80),
                purpose: field(entry, "purpose", 400),
                cites: entry["purpose_cites"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|cite| Some(cut(cite.as_str()?, 200)))
                    .take(8)
                    .collect(),
            };
            let any = text.name.is_some() || text.caption.is_some() || text.purpose.is_some();
            any.then(|| (id.clone(), text))
        })
        .collect();
    let not_written = answer["not_written"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(NotWritten {
                what: field(entry, "what", 300)?,
                why: field(entry, "why", 300).unwrap_or_default(),
            })
        })
        .take(20)
        .collect();
    Some((text, not_written))
}

/// The writer's brief, as the trial of 2026-11-27 measured it (`trial/scripts/brief-top.md`): `{parts}`, `{ignored}` and
/// `{flag}` are filled in by [`writer_prompt`].
const BRIEF: &str = "\
You write the top layer of a repository's system map for a few of its parts: each part's name, caption and purpose. Other \
writers may do the other parts at the same time; a separate reviewer later checks every sentence you write against the code.

Your current folder is the repository. Its skeleton map is `system-map.json`: each part's `id`, `paths` (what it owns), \
`tier` and `zone` are settled by the repository's owner. Don't change them. Its `actors` are the outside systems. Its names, \
captions and purposes are placeholders.

Your parts: {parts}
{flag}
## Rules

- **Read only.** You can only read. Open no `.env` file{ignored}.
- **Cite every claim.** Each purpose carries citations `path:start-end` of lines you opened that show it. Docs, READMEs, \
`CLAUDE.md`, ADRs and code comments are not proof; cite code, config or manifests. A claim about a folder's contents may cite \
a directory listing as `path/ (listing)`.
- **Don't guess.** If you can't ground a sentence in code you read, leave it out and add it to `not_written` with the reason. \
A short, true purpose beats a full, guessed one. Count every claim you are less than sure of in `guesses`; aim for none.
- **Limits.** A `name` is a few plain words. A `caption` is at most 6 words. A `purpose` is at most 2 sentences and 240 \
characters. Plain words; say what it does, not how clever it is.
- **Scope.** Open just enough of what your parts own to say what each one is for. Don't write held items, links or laws.

## Answer

Reply with this JSON only, nothing before or after it:

```json
{
  \"parts\": {
    \"<id>\": { \"name\": \"...\", \"caption\": \"...\", \"purpose\": \"...\", \"purpose_cites\": [\"path:1-20\"] }
  },
  \"not_written\": [{ \"what\": \"...\", \"why\": \"...\" }],
  \"guesses\": [{ \"text\": \"...\", \"why\": \"...\" }],
  \"files_opened\": 0
}
```
";

/// A writer's prompt over `parts`: the brief, with the reference copies it must not read and, for a redraft, the flag on the
/// part's last purpose.
fn writer_prompt(parts: &[String], ignored: &[String], flag: Option<&Flag>) -> String {
    let named = |items: &[String]| {
        items
            .iter()
            .map(|item| format!("`{item}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let ignored = match ignored {
        [] => String::new(),
        paths => format!(
            ", and don't read {} (reference copies, ignored)",
            named(paths)
        ),
    };
    let flag = flag.map_or(String::new(), |flag| {
        let cites = match flag.cites.as_slice() {
            [] => String::new(),
            cites => format!(" It cited {}.", named(cites)),
        };
        format!(
            "\nThis is a redraft. A reviewer judged this part's last purpose `{}`: {}{cites} Read the code again and write the \
part's name, caption and purpose so that every claim holds.\n",
            flag.verdict, flag.why
        )
    });
    BRIEF
        .replace("{parts}", &named(parts))
        .replace("{ignored}", &ignored)
        .replace("{flag}", &flag)
}

/// A part as the grouping sees it: its zone and how many tracked files it owns.
struct Sized {
    id: String,
    zone: Option<String>,
    files: usize,
}

/// The writer groups of `parts`, at most [`MAX_GROUPS`], by zone then size (#753, Q24): a group per zone, in the order the
/// zones first come; while there are too many, the two with the fewest files merge; then, while there are fewer than the
/// most, the group with the most files that has more than [`GROUP_PARTS`] parts splits in two of even size. Each group keeps
/// the skeleton's order.
fn group_parts(parts: &[Sized]) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut zones: Vec<Option<&str>> = Vec::new();
    for (at, part) in parts.iter().enumerate() {
        match zones.iter().position(|zone| *zone == part.zone.as_deref()) {
            Some(group) => groups[group].push(at),
            None => {
                zones.push(part.zone.as_deref());
                groups.push(vec![at]);
            }
        }
    }
    let files = |group: &[usize]| group.iter().map(|&at| parts[at].files).sum::<usize>();
    while groups.len() > MAX_GROUPS {
        let mut order: Vec<usize> = (0..groups.len()).collect();
        order.sort_by_key(|&group| (files(&groups[group]), group));
        let (into, from) = (order[0].min(order[1]), order[0].max(order[1]));
        let moved = groups.remove(from);
        groups[into].extend(moved);
        groups[into].sort_unstable();
    }
    while groups.len() < MAX_GROUPS {
        let Some(at) = (0..groups.len())
            .filter(|&group| groups[group].len() > GROUP_PARTS)
            .max_by_key(|&group| (files(&groups[group]), Reverse(group)))
        else {
            break;
        };
        let mut by_size = groups.remove(at);
        by_size.sort_by_key(|&part| (Reverse(parts[part].files), part));
        let (mut one, mut two): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
        for part in by_size {
            let lighter = if (files(&two), two.len()) < (files(&one), one.len()) {
                &mut two
            } else {
                &mut one
            };
            lighter.push(part);
        }
        one.sort_unstable();
        two.sort_unstable();
        groups.insert(at, two);
        groups.insert(at, one);
    }
    groups
        .into_iter()
        .map(|group| group.into_iter().map(|at| parts[at].id.clone()).collect())
        .collect()
}

/// `map_draft`: `project`'s Map draft; none when there is none.
pub fn map_draft(app: &crate::App, project: String) -> Option<MapDraftView> {
    app.drafts.view(&project)
}

/// `map_draft_start`: Draft the map, from the page's `skeleton` or a fresh scan.
pub fn map_draft_start(
    app: &crate::App,
    project: String,
    skeleton: Option<String>,
) -> Result<MapDraftView, String> {
    app.drafts.start(&project, skeleton)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A part: its id, zone and file count.
    type Of<'a> = (&'a str, Option<&'a str>, usize);
    /// A case: its name, the parts, and the groups they make.
    type Case<'a> = (&'a str, Vec<Of<'a>>, Vec<Vec<&'a str>>);

    fn sized(parts: &[Of]) -> Vec<Sized> {
        parts
            .iter()
            .map(|(id, zone, files)| Sized {
                id: (*id).into(),
                zone: zone.map(str::to_string),
                files: *files,
            })
            .collect()
    }

    #[test]
    fn parts_split_into_at_most_four_groups_by_zone_then_size() {
        let ids: Vec<String> = (1..=15).map(|n| format!("p{n}")).collect();
        let fifteen: Vec<Of> = ids.iter().map(|id| (id.as_str(), None, 1)).collect();
        let cases: [Case; 5] = [
            (
                // observability-sdk's shape: five parts, one writer.
                "five parts in no zone",
                vec![
                    ("a", None, 3),
                    ("b", None, 1),
                    ("c", None, 9),
                    ("d", None, 2),
                    ("e", None, 1),
                ],
                vec![vec!["a", "b", "c", "d", "e"]],
            ),
            (
                "a zone apiece",
                vec![
                    ("a", Some("x"), 1),
                    ("b", Some("y"), 1),
                    ("c", Some("y"), 1),
                ],
                vec![vec!["a"], vec!["b", "c"]],
            ),
            (
                // The two with the fewest files merge, until four are left.
                "six zones",
                vec![
                    ("a", Some("u"), 10),
                    ("b", Some("v"), 1),
                    ("c", Some("w"), 8),
                    ("d", Some("x"), 2),
                    ("e", Some("y"), 3),
                    ("f", Some("z"), 9),
                ],
                vec![vec!["a"], vec!["b", "d", "e"], vec!["c"], vec!["f"]],
            ),
            (
                // Six parts are too many for one writer: two of even size, each in the skeleton's order.
                "six parts by size",
                vec![
                    ("a", None, 10),
                    ("b", None, 1),
                    ("c", None, 1),
                    ("d", None, 1),
                    ("e", None, 1),
                    ("f", None, 6),
                ],
                vec![vec!["a"], vec!["b", "c", "d", "e", "f"]],
            ),
            (
                "fifteen parts",
                fifteen,
                vec![
                    vec!["p1", "p5", "p9", "p13"],
                    vec!["p3", "p7", "p11", "p15"],
                    vec!["p2", "p6", "p10", "p14"],
                    vec!["p4", "p8", "p12"],
                ],
            ),
        ];
        for (name, parts, want) in cases {
            assert_eq!(group_parts(&sized(&parts)), want, "{name}");
        }
    }

    #[test]
    fn a_redrafts_writer_is_given_the_flag_and_every_writer_the_reference_copies_to_leave() {
        let flag = Flag {
            verdict: "contradicted".into(),
            why: "It writes to stdout.".into(),
            cites: vec!["src/logger.ts:37".into()],
        };
        let prompt = writer_prompt(&["logger".into()], &["vendor/".into()], Some(&flag));
        for said in [
            "Your parts: `logger`",
            "don't read `vendor/` (reference copies, ignored)",
            "judged this part's last purpose `contradicted`: It writes to stdout. It cited `src/logger.ts:37`.",
        ] {
            assert!(prompt.contains(said), "{said}\n{prompt}");
        }
        let plain = writer_prompt(&["a".into(), "b".into()], &[], None);
        assert!(
            plain.contains("Your parts: `a`, `b`\n\n## Rules"),
            "{plain}"
        );
        assert!(plain.contains("Open no `.env` file.\n"), "{plain}");
        assert!(!plain.contains("redraft"), "{plain}");
    }
}
