//! Micro-worlds (spec danielrispler/plotkeeper#378, ticket #399): "Explore this" on a flow or a brief figure builds a small
//! interactive simulator of the mechanism from the real code.
//!
//! - **The run.** A background run of kind micro-world (`runner.rs`): no `Bash`, its working folder the project's files at the
//!   map's commit, to read, and the only place it writes is the output folder there, [`MICRO_WORLD_OUT`]. Its prompt carries
//!   dsh-visual's guidance inline, since `--safe-mode` loads no skill, and only map text ([`Topic`]); the page cites code as
//!   `{{ref:…}}` placeholders.
//! - **The host checks.** When the run ends, plotkeeper runs dsh-visual's `build.py` (the references resolved against git at
//!   the commit), puts the Content-Security-Policy ([`CSP`]) first in the page, and runs `verify.mjs` on that page in headless
//!   Chrome with `micro_world_guard.mjs` loaded first, which aborts every request that is not a local file. The scripts are in
//!   the folder `microWorlds.dshVisual` names; without it, "Explore this" offers only the prompt to copy.
//! - **A failing page** is rewritten once by a second run, which finds the first template and is told what failed; failing
//!   again, or a run that does not finish, falls back to the prompt to copy.
//! - **Kept** per project by subject (a flow's id, or a brief figure) with the commit it was built at, in the data folder; the
//!   page is served only once checked ([`MicroWorlds::page`]). It "may be out of date" once a path of a part on its flow has
//!   changed since that commit.

use crate::runner::{Kind, Limits, MICRO_WORLD_OUT, Request, Runner};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, OnceLock, Weak},
    thread,
    time::Duration,
};
use studio_workbench::run::RunError;

/// The policy put first in every page before it is checked and stored: no network, no other origin, only the page's own inline
/// script, style and `data:` images.
pub const CSP: &str =
    "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:";

/// The network guard `verify.mjs` runs under.
const GUARD: &str = include_str!("micro_world_guard.mjs");
/// How long each host script may take.
const BUILD_LIMIT: Duration = Duration::from_secs(120);
const VERIFY_LIMIT: Duration = Duration::from_secs(300);
/// The runs a page gets: the first and one rewrite.
const ATTEMPTS: u8 = 2;

/// What "Explore this" was pressed on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Subject {
    /// A flow of the map, by id.
    Flow { id: String },
    /// A PR brief's figure: chapter `chapter`'s step `step` of PR `pr`'s stored brief, about the parts that step lights.
    Figure {
        pr: u64,
        chapter: usize,
        step: usize,
    },
}

impl Subject {
    /// The key a micro-world is kept by.
    pub fn key(&self) -> String {
        match self {
            Subject::Flow { id } => format!("flow:{id}"),
            Subject::Figure { pr, chapter, step } => format!("figure:{pr}:{chapter}:{step}"),
        }
    }
}

/// What a micro-world is about, as the map has it: only map text, never a PR's, an issue's or a comment's, since it goes into a
/// run's prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct Topic {
    pub key: String,
    pub name: String,
    /// The parts it crosses, by id, in order.
    pub parts: Vec<String>,
    /// Their paths, each once: a change under one makes the world out of date.
    pub paths: Vec<String>,
    /// The map's words on it: the hops, the parts and their laws.
    pub text: String,
}

type Json = serde_json::Value;

fn items<'a>(map: &'a Json, key: &str) -> impl Iterator<Item = &'a Json> {
    map[key].as_array().into_iter().flatten()
}

fn named<'a>(map: &'a Json, id: &'a str) -> &'a str {
    items(map, "parts")
        .chain(items(map, "actors"))
        .find(|item| item["id"] == id)
        .and_then(|item| item["name"].as_str())
        .unwrap_or(id)
}

/// The topic of the parts `parts` (ids the map has), with `lead` before their purposes and held items.
fn parts_text(map: &Json, parts: &[String], lead: String) -> (Vec<String>, String) {
    let mut paths: Vec<String> = Vec::new();
    let mut text = lead;
    text.push_str("\nThe parts, as the project's map describes them:\n");
    for id in parts {
        let Some(part) = items(map, "parts").find(|part| part["id"] == id.as_str()) else {
            continue;
        };
        let own: Vec<&str> = items(part, "paths").filter_map(Json::as_str).collect();
        for path in &own {
            if !paths.iter().any(|p| p == path) {
                paths.push(path.to_string());
            }
        }
        let field = |key: &str| part[key].as_str().unwrap_or("").to_string();
        // Writing to a `String` cannot fail.
        writeln!(
            text,
            "- {} ({}): {}. {}",
            field("name"),
            own.join(", "),
            field("caption"),
            system_map::part_text(part)
        )
        .ok();
    }
    let laws: Vec<String> = items(map, "laws")
        .filter(|law| parts.iter().any(|id| law["on"] == id.as_str()))
        .map(|law| {
            format!(
                "- {} (on {})\n",
                law["text"].as_str().unwrap_or(""),
                named(map, law["on"].as_str().unwrap_or(""))
            )
        })
        .collect();
    if !laws.is_empty() {
        text.push_str("\nThe laws on them:\n");
        text.extend(laws);
    }
    (paths, text)
}

/// The topic of flow `id` in the map file's text `source`; none when the map does not read or has no such flow.
pub fn flow_topic(source: &str, id: &str) -> Option<Topic> {
    let map: Json = system_map::convert_value(source).ok()?;
    let flow = items(&map, "flows").find(|flow| flow["id"] == id)?;
    let name = flow["name"].as_str().unwrap_or(id).to_string();
    let part_ids: BTreeSet<String> = items(&map, "parts")
        .filter_map(|part| part["id"].as_str().map(String::from))
        .collect();
    let mut parts: Vec<String> = Vec::new();
    let mut lead = format!("The flow \"{name}\", hop by hop:\n");
    let ends = studio_understanding::pr_brief::hop_ends(&map, flow);
    for (n, (hop, ends)) in items(flow, "hops").zip(ends).enumerate() {
        let ends = ends.as_ref().map(|(from, to)| (from.as_str(), to.as_str()));
        let between = ends.map_or(String::new(), |(from, to)| {
            format!(" ({} to {})", named(&map, from), named(&map, to))
        });
        // Writing to a `String` cannot fail.
        writeln!(
            lead,
            "{}. {}{between}",
            n + 1,
            hop["text"].as_str().unwrap_or("")
        )
        .ok();
        for end in ends.into_iter().flat_map(|(from, to)| [from, to]) {
            if part_ids.contains(end) && !parts.iter().any(|p| p == end) {
                parts.push(end.to_string());
            }
        }
    }
    let (paths, text) = parts_text(&map, &parts, lead);
    Some(Topic {
        key: Subject::Flow { id: id.into() }.key(),
        name,
        parts,
        paths,
        text,
    })
}

/// The topic of a brief figure, kept by `key`: the mechanism inside the parts its step lights (`lit`), those the map has. The
/// figure's own labels are the writer's words from a PR, so they never reach the prompt. None when the map does not read or
/// has none of the parts.
pub fn figure_topic(source: &str, key: String, lit: &[String]) -> Option<Topic> {
    let map: Json = system_map::convert_value(source).ok()?;
    let parts: Vec<String> = lit
        .iter()
        .filter(|id| items(&map, "parts").any(|part| part["id"] == id.as_str()))
        .cloned()
        .collect();
    if parts.is_empty() {
        return None;
    }
    let names: Vec<&str> = parts.iter().map(|id| named(&map, id)).collect();
    let name = names.join(", ");
    let lead = format!("The mechanism inside {name}.\n");
    let (paths, text) = parts_text(&map, &parts, lead);
    Some(Topic {
        key,
        name,
        parts,
        paths,
        text,
    })
}

/// `html` with the [`CSP`] as its first element: before anything the run wrote, so it governs all of it. A leading doctype is
/// kept first.
pub fn with_csp(html: &str) -> String {
    let meta = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#);
    let body = html.trim_start_matches('\u{feff}').trim_start();
    let rest = match body.get(..9) {
        Some(head) if head.eq_ignore_ascii_case("<!doctype") => {
            body.find('>').map_or("", |end| &body[end + 1..])
        }
        _ => body,
    };
    format!("<!doctype html>{meta}{rest}")
}

/// dsh-visual's guidance, inline, for a run that loads no skill.
const GUIDANCE: &str = "\
How to write it (dsh-visual's rules; plotkeeper checks the page against them):
- Write one complete HTML page to `.micro-world/template.html`, with inline CSS and JavaScript only. Write nothing anywhere else.
- No request of any kind: no external script, style, font, image, fetch or link to another site. The page is shown with no \
network and its requests are blocked, so one fails the check.
- Never type a source line number or excerpt by hand. Write `{{ref:path|start text}}` or `{{ref:path|start text|end text}}` \
for a line range and `{{excerpt:path|start text|end text}}` for a quoted passage, the path relative to the repository's root \
(your current folder). The start text must match exactly one line of that file; each part is an exact substring of a single \
line. Write `outer>inner` to find `inner` as the first line after the one line matching `outer`; write `\\|` and `\\>` for a \
literal `|` or `>`. plotkeeper resolves them against git, and a placeholder that matches no line, or more than one, fails the \
build.
- Set `:root { color-scheme: light dark }` and give `body` an opaque background that is light in light mode and dark under \
`@media (prefers-color-scheme: dark)`.
- Nothing may overflow sideways at 390px or 1280px wide. Text in an SVG renders at 12px or more. No duplicate ids, no `#` link \
to a missing id, and no script or console error.
- Make it a small simulator: controls (buttons, sliders, steps) that move a model of the mechanism, with prose that says what \
each state means. Label it as an illustrative model, and cite the code each piece rests on with `{{ref:…}}`.";

/// The prompt of a run that builds `topic`, told what failed when it is the rewrite.
fn run_prompt(topic: &Topic, failure: Option<&str>) -> String {
    let mut prompt = format!(
        "Build a micro-world: a small interactive page that simulates how this mechanism of the project works, true to its \
code. Your current folder holds the project's files; read the code of the parts below before you write.\n\n{}\n{GUIDANCE}\n",
        topic.text
    );
    if let Some(failure) = failure {
        // Writing to a `String` cannot fail.
        write!(
            prompt,
            "\nYour first page is in `.micro-world/template.html` and failed plotkeeper's checks:\n{failure}\nFix every failure \
and write the page again to the same file.\n"
        ).ok();
    }
    prompt
}

/// The prompt Daniel copies into his own agent when no run builds the page: it stands alone, in his checkout, with the skill.
pub fn copy_prompt(project: &str, topic: &Topic, commit: &str) -> String {
    format!(
        "In the repository at {project}, at commit {commit}, build a micro-world with the dsh-visual skill: a small interactive \
page that simulates how this mechanism works, true to the code. Read the code of the parts below first.\n\n{}\nWrite it as \
a dsh-visual template, cite the code with `{{{{ref:…}}}}` placeholders, build it with build.py at that commit, and check it \
with verify.mjs. The page needs no network: inline CSS and JavaScript only.\n",
        topic.text
    )
}

/// Where a micro-world stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    /// A run is queued, running or being checked.
    Building,
    /// The page is checked and stored.
    Ready,
    /// The runs did not give a page that passed: the prompt to copy.
    Failed,
}

/// A micro-world as its file keeps it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    project: String,
    key: String,
    name: String,
    paths: Vec<String>,
    /// The commit the building run reads.
    commit: String,
    /// The commit the stored page was built at, when one is stored.
    #[serde(default)]
    built: Option<String>,
    phase: Phase,
    /// 1 for the first run, 2 for the rewrite.
    attempt: u8,
    /// The building run.
    #[serde(default)]
    run: Option<String>,
    #[serde(default)]
    folder: Option<PathBuf>,
    /// The building run's own prompt, which tells the rewrite what failed.
    #[serde(default)]
    topic_text: String,
    /// Why it failed.
    #[serde(default)]
    why: Option<String>,
    /// The prompt to copy.
    prompt: String,
}

/// A micro-world as the map shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    pub phase: Phase,
    /// A checked page is stored, to open (also while a rebuild runs or after it failed).
    pub page: bool,
    /// A path of a part on it changed since the stored page's commit: "may be out of date", with Rebuild.
    pub stale: bool,
    /// Why it failed.
    pub line: Option<String>,
    /// The prompt to copy, once it failed.
    pub prompt: Option<String>,
}

/// A project's micro-worlds on the map, and whether a run can build one (`microWorlds.dshVisual` is set).
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct WorldsState {
    pub on: bool,
    pub worlds: BTreeMap<String, View>,
}

/// What "Explore this" did: started a run, or gave the prompt to copy and why.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Explored {
    pub started: bool,
    pub line: Option<String>,
    pub prompt: Option<String>,
}

type Changed = Box<dyn Fn(&str) + Send + Sync>;

/// Every project's micro-worlds: their runs, their checks and their pages.
pub struct MicroWorlds {
    /// `<data>/micro-worlds`: a folder per project, a file per world.
    store: PathBuf,
    settings: PathBuf,
    runner: Weak<Runner>,
    changed: OnceLock<Changed>,
    /// The runs whose end is being checked, so a run is checked once.
    checking: Mutex<HashSet<String>>,
    /// Serialises the read, change and write of a world's file.
    writing: Mutex<()>,
    me: Weak<MicroWorlds>,
}

fn hex(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

/// The words of a run's end.
pub(crate) fn end_words(end: crate::runner::End) -> String {
    serde_json::to_value(end)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.replace('-', " ")))
        .unwrap_or_default()
}

impl MicroWorlds {
    /// The micro-worlds kept in `store`, built by `runner`'s runs, with `settings` read anew for each.
    pub fn new(store: PathBuf, settings: PathBuf, runner: &Arc<Runner>) -> Arc<MicroWorlds> {
        Arc::new_cyclic(|me| MicroWorlds {
            store,
            settings,
            runner: Arc::downgrade(runner),
            changed: OnceLock::new(),
            checking: Mutex::default(),
            writing: Mutex::default(),
            me: me.clone(),
        })
    }

    /// Sets what is told the project whose micro-worlds changed; the first call wins.
    pub fn on_change(&self, changed: impl Fn(&str) + Send + Sync + 'static) {
        // Set once; a later set keeps the first value, which is the same.
        self.changed.set(Box::new(changed)).ok();
    }

    fn tell(&self, project: &str) {
        if let Some(changed) = self.changed.get() {
            changed(project);
        }
    }

    /// `microWorlds.dshVisual`, read now.
    fn scripts(&self) -> Option<PathBuf> {
        let text = fs::read_to_string(&self.settings).unwrap_or_default();
        let home = std::env::home_dir().unwrap_or_default();
        crate::settings::micro_worlds_dsh_visual_in(&text, &home)
    }

    fn folder(&self, project: &str) -> PathBuf {
        project_folder(&self.store, project)
    }

    fn file(&self, project: &str, key: &str) -> PathBuf {
        self.folder(project).join(format!("{}.json", hex(key)))
    }

    fn page_file(&self, project: &str, key: &str) -> PathBuf {
        self.folder(project).join(format!("{}.html", hex(key)))
    }

    fn read(&self, project: &str, key: &str) -> Option<Record> {
        serde_json::from_slice(&fs::read(self.file(project, key)).ok()?).ok()
    }

    fn write(&self, project: &str, record: &Record) {
        let file = self.file(project, &record.key);
        let saved = fs::create_dir_all(self.folder(project))
            .and_then(|()| studio_workbench::write_atomic(&file, &serde_json::to_vec(record)?));
        if let Err(e) = saved {
            log::error!("save the micro-world {} of {project}: {e}", record.key);
        }
    }

    fn records(&self, project: &str) -> Vec<Record> {
        let files = fs::read_dir(self.folder(project))
            .into_iter()
            .flatten()
            .flatten();
        files
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "json"))
            .filter_map(|path| serde_json::from_slice(&fs::read(path).ok()?).ok())
            .collect()
    }

    /// "Explore this" on `topic`, at `commit` of `project`: starts a run when `microWorlds.dshVisual` is set, else gives the
    /// prompt to copy. A world already building is left to its run. Rebuild is the same call.
    pub fn explore(&self, project: &str, topic: Topic, commit: &str) -> Explored {
        let prompt = copy_prompt(project, &topic, commit);
        let copy = |line: String| Explored {
            started: false,
            line: Some(line),
            prompt: Some(prompt.clone()),
        };
        if self.scripts().is_none() {
            return copy("Set microWorlds.dshVisual for a run to build it".into());
        }
        let guard = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let old = self.read(project, &topic.key);
        if old.as_ref().is_some_and(|old| old.phase == Phase::Building) {
            return Explored {
                started: true,
                line: None,
                prompt: None,
            };
        }
        let record = Record {
            project: project.into(),
            key: topic.key.clone(),
            name: topic.name.clone(),
            paths: topic.paths.clone(),
            commit: commit.into(),
            built: old.and_then(|old| old.built),
            phase: Phase::Building,
            attempt: 1,
            run: None,
            folder: None,
            topic_text: topic.text.clone(),
            why: None,
            prompt: prompt.clone(),
        };
        let started = self.start(project, record, None, None);
        drop(guard);
        // A run that ended before its world was written down is checked now.
        self.runs_changed(project);
        self.tell(project);
        match started {
            Ok(()) => Explored {
                started: true,
                line: None,
                prompt: None,
            },
            Err(line) => copy(line),
        }
    }

    /// Starts `record`'s run, with the template of the failed run in `previous` and what failed; writes the record down, as
    /// building or failed. Called with `writing` held.
    fn start(
        &self,
        project: &str,
        mut record: Record,
        previous: Option<&Path>,
        failure: Option<&str>,
    ) -> Result<(), String> {
        let started = (|| {
            let runner = self.runner.upgrade().ok_or("plotkeeper is stopping")?;
            let folder = runner.new_folder()?;
            fill(Path::new(project), &record.commit, &folder)?;
            if let Some(previous) = previous {
                fs::copy(previous, folder.join(MICRO_WORLD_OUT).join("template.html"))
                    .map_err(|e| e.to_string())?;
            }
            let topic = Topic {
                key: record.key.clone(),
                name: record.name.clone(),
                parts: Vec::new(),
                paths: record.paths.clone(),
                text: record.topic_text.clone(),
            };
            let id = runner
                .submit(Request {
                    kind: Kind::MicroWorld,
                    project: project.into(),
                    folder: folder.clone(),
                    prompt: run_prompt(&topic, failure),
                    limits: Limits::of(Kind::MicroWorld),
                })
                .map_err(|refusal| refusal.line)?;
            Ok::<_, String>((id, folder))
        })();
        let result = match started {
            Ok((id, folder)) => {
                record.run = Some(id);
                record.folder = Some(folder);
                record.phase = Phase::Building;
                Ok(())
            }
            Err(why) => {
                record.phase = Phase::Failed;
                record.why = Some(why.clone());
                Err(why)
            }
        };
        self.write(project, &record);
        result
    }

    /// The runner says `project`'s runs changed: each building world whose run ended is checked, on a thread of its own, and
    /// one whose run is gone (cancelled) failed.
    pub fn runs_changed(&self, project: &str) {
        let Some(runner) = self.runner.upgrade() else {
            return;
        };
        for record in self.records(project) {
            let (Phase::Building, Some(id)) = (record.phase, record.run.clone()) else {
                continue;
            };
            if self.checking.lock().unwrap().contains(&id) {
                continue;
            }
            match runner.outcome(&id) {
                Some(outcome) => {
                    if !self.checking.lock().unwrap().insert(id.clone()) {
                        continue;
                    }
                    let (me, project) = (self.me.clone(), project.to_string());
                    thread::spawn(move || {
                        if let Some(worlds) = me.upgrade() {
                            worlds.finish(&project, &record.key, &id, outcome);
                            worlds.checking.lock().unwrap().remove(&id);
                            // A rewrite's run that ended before its world was written down is checked now.
                            worlds.runs_changed(&project);
                            worlds.tell(&project);
                        }
                    });
                }
                None if !runner.runs(project).runs.iter().any(|run| run.id == id) => {
                    // Explore this and a rewrite hold the lock while their run is submitted, which tells this; each looks
                    // again once it lets go, so a busy lock is left to that look rather than waited on.
                    let Ok(guard) = self.writing.try_lock() else {
                        continue;
                    };
                    if let Some(mut record) = self.read(project, &record.key)
                        && record.run.as_deref() == Some(id.as_str())
                        && record.phase == Phase::Building
                    {
                        record.phase = Phase::Failed;
                        record.why = Some(
                            "The run is gone: it was dismissed, or plotkeeper stopped before its page was checked"
                                .into(),
                        );
                        self.write(project, &record);
                        drop(guard);
                        self.tell(project);
                    }
                }
                None => {}
            }
        }
    }

    /// Run `id` of world `key` ended with `outcome`: a finished run's page is checked on the host and stored when it passes; a
    /// failing page is rewritten once, then falls back to the prompt to copy, as does a run that did not finish.
    fn finish(&self, project: &str, key: &str, id: &str, outcome: crate::runner::Outcome) {
        let Some(record) = self.read(project, key) else {
            return;
        };
        if record.run.as_deref() != Some(id) || record.phase != Phase::Building {
            return;
        }
        // Dismiss waits until the check and a rewrite are done with the run's folder; a run already dismissed is gone, and
        // the next look finds it so.
        let Some(_hold) = self.runner.upgrade().and_then(|runner| runner.hold(id)) else {
            return;
        };
        use crate::runner::End;
        let checked = match (outcome.end, self.scripts(), &record.folder) {
            (End::Done | End::Denied, Some(scripts), Some(folder)) => Some(host_check(
                &scripts,
                folder,
                Path::new(project),
                &record.commit,
            )),
            _ => None,
        };
        let _guard = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Rebuild may have started another run meanwhile.
        let Some(mut record) = self
            .read(project, key)
            .filter(|now| now.run.as_deref() == Some(id))
        else {
            return;
        };
        match checked {
            Some(Ok(page)) => {
                if let Err(e) = fs::write(self.page_file(project, key), page) {
                    log::error!("save the micro-world {key} of {project}: {e}");
                    record.phase = Phase::Failed;
                    record.why = Some(format!("The page could not be saved: {e}"));
                } else {
                    record.phase = Phase::Ready;
                    record.built = Some(record.commit.clone());
                    record.why = None;
                }
                record.run = None;
                self.write(project, &record);
            }
            Some(Err(why)) if record.attempt < ATTEMPTS => {
                let previous = record
                    .folder
                    .as_ref()
                    .map(|folder| folder.join(MICRO_WORLD_OUT).join("template.html"))
                    .filter(|previous| previous.is_file());
                record.attempt += 1;
                // A failed start writes the record down as failed.
                self.start(project, record, previous.as_deref(), Some(&why))
                    .ok();
            }
            Some(Err(why)) => {
                record.phase = Phase::Failed;
                record.why = Some(why);
                record.run = None;
                self.write(project, &record);
            }
            None => {
                record.phase = Phase::Failed;
                record.why = Some(match self.scripts() {
                    None => "microWorlds.dshVisual is no longer set".into(),
                    Some(_) => format!("The run ended {}", end_words(outcome.end)),
                });
                record.run = None;
                self.write(project, &record);
            }
        }
    }

    /// `project`'s micro-worlds as the map shows them, out of date against `current`, the map's commit now.
    pub fn state(&self, project: &str, current: Option<&str>) -> WorldsState {
        let worlds = self.records(project).into_iter().map(|record| {
            let page = self.page_file(project, &record.key).exists();
            let stale = page
                && match (&record.built, current) {
                    (Some(built), Some(current)) => {
                        changed(Path::new(project), built, current, &record.paths)
                    }
                    _ => false,
                };
            let view = View {
                phase: record.phase,
                page,
                stale,
                line: record.why.clone(),
                prompt: (record.phase == Phase::Failed).then(|| record.prompt.clone()),
            };
            (record.key, view)
        });
        WorldsState {
            on: self.scripts().is_some(),
            worlds: worlds.collect(),
        }
    }

    /// The checked page of world `key`, with its policy first; an error when none is stored. Only a page that passed the host
    /// checks is ever stored.
    pub fn page(&self, project: &str, key: &str) -> Result<String, String> {
        fs::read_to_string(self.page_file(project, key)).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "No checked micro-world is stored for this".to_string(),
            _ => format!("The micro-world's page did not read: {e}"),
        })
    }

    /// At a start: each project's building worlds whose run ended meanwhile are checked, and one whose run is gone failed.
    pub fn pick_up(&self) {
        let folders = fs::read_dir(&self.store).into_iter().flatten().flatten();
        let mut projects = BTreeSet::new();
        for folder in folders
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
        {
            let files = fs::read_dir(&folder).into_iter().flatten().flatten();
            for file in files.map(|entry| entry.path()) {
                if let Some(record) = fs::read(&file)
                    .ok()
                    .and_then(|text| serde_json::from_slice::<Record>(&text).ok())
                    && record.phase == Phase::Building
                {
                    projects.insert(record.project);
                }
            }
        }
        projects
            .iter()
            .for_each(|project| self.runs_changed(project));
    }
}

/// The page's "Explore this" (and Rebuild) on `subject` in `project`: its topic read from the map on the default branch, at that
/// map's commit; a figure's parts from PR `pr`'s stored brief.
pub fn micro_world_explore(
    app: &crate::App,
    project: String,
    subject: Subject,
) -> Result<Explored, String> {
    let map = crate::map::read_map(Path::new(&project));
    let (Some(source), Some(commit)) = (map.source, map.commit) else {
        return Err("The project has no map to read".into());
    };
    let topic = match &subject {
        Subject::Flow { id } => flow_topic(&source, id),
        Subject::Figure { pr, chapter, step } => {
            let stored = crate::pr_briefs::store_of(&project).and_then(|store| store.latest(*pr));
            let lit = stored.and_then(|stored| {
                Some(
                    stored
                        .brief
                        .chapters
                        .get(*chapter)?
                        .steps
                        .get(*step)?
                        .on
                        .clone(),
                )
            });
            lit.and_then(|lit| figure_topic(&source, subject.key(), &lit))
        }
    };
    let topic = topic.ok_or("The map has no such flow or part")?;
    Ok(app.worlds.explore(&project, topic, &commit))
}

/// The page's request for world `key`'s checked page, to show in its locked-down frame.
pub fn micro_world_page(app: &crate::App, project: String, key: String) -> Result<String, String> {
    app.worlds.page(&project, &key)
}

/// `project`'s folder in `store`: its path, readable, and a hash of it, so two paths that read alike never share one.
pub(crate) fn project_folder(store: &Path, project: &str) -> PathBuf {
    let name: String = project
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    store.join(format!("{name}-{:016x}", fnv1a(project)))
}

/// FNV-1a of `text`: the same in every build, as a name or hash kept across builds must be.
pub(crate) fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Fills a run's working `folder` with `repo`'s files at `commit` and makes the output folder in it.
fn fill(repo: &Path, commit: &str, folder: &Path) -> Result<(), String> {
    archive(repo, commit, folder)?;
    fs::create_dir(folder.join(MICRO_WORLD_OUT))
        .map_err(|e| format!("the output folder {MICRO_WORLD_OUT}: {e}"))
}

/// Fills `folder` with `repo`'s files at `commit`: `git archive`, so no git data and nothing untracked.
pub(crate) fn archive(repo: &Path, commit: &str, folder: &Path) -> Result<(), String> {
    // Streamed into `tar`, so it is waited on by hand; it reads this machine's repository only.
    let mut archive = studio_workbench::run::git(repo)
        .args(["archive", "--format=tar", commit])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("git archive: {e}"))?;
    let stdout = archive.stdout.take().ok_or("git archive gave no output")?;
    let tar = Command::new("tar")
        .args(["-x", "-C"])
        .arg(folder)
        .stdin(stdout)
        .output()
        .map_err(|e| format!("tar: {e}"))?;
    let status = archive.wait().map_err(|e| e.to_string())?;
    if !status.success() || !tar.status.success() {
        return Err(format!(
            "the project's files at {commit} could not be copied for the run"
        ));
    }
    Ok(())
}

/// Whether a path of `paths` differs between `built` and `current` in `repo`; a commit git no longer has counts as changed.
fn changed(repo: &Path, built: &str, current: &str, paths: &[String]) -> bool {
    if built == current || paths.is_empty() {
        return false;
    }
    let mut diff = studio_workbench::run::git(repo);
    diff.args(["diff", "--quiet", built, current, "--"])
        .args(paths);
    studio_workbench::run::run_until(&mut diff, crate::map::GIT_READ_LIMIT).is_err()
}

/// Runs `command` through the one process runner with its output in `log`, its process group killed past `limit`:
/// whether it exited 0, and what it printed.
fn run_to_log(mut command: Command, log: &Path, limit: Duration) -> Result<(bool, String), String> {
    let out = File::create(log).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    command.stdout(out).stderr(err);
    match studio_workbench::run::run_logged(&mut command, limit, &|| false) {
        Ok(status) => Ok((
            status.success(),
            fs::read_to_string(log).unwrap_or_default(),
        )),
        Err(RunError::TimedOut(_)) => {
            Err(format!("it took longer than {} seconds", limit.as_secs()))
        }
        Err(RunError::Start(e)) => Err(e.to_string()),
        // A logged run gives any exit as its status.
        Err(error @ RunError::Failed(_)) => Err(error.to_string()),
    }
}

/// The last `n` lines of `text`, each cut to 300 characters.
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let from = lines.len().saturating_sub(n);
    lines[from..]
        .iter()
        .map(|line| line.chars().take(300).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The host checks of the page a run wrote in `folder` (its working folder): dsh-visual's `build.py` in `scripts`, resolving its
/// references against `repo` at `commit`; the policy put first; then `verify.mjs` under the network guard. The checked page, or
/// what failed.
pub fn host_check(
    scripts: &Path,
    folder: &Path,
    repo: &Path,
    commit: &str,
) -> Result<String, String> {
    let out = folder.join(MICRO_WORLD_OUT);
    if !out.join("template.html").is_file() {
        return Err("The run wrote no .micro-world/template.html".into());
    }
    let logs = folder.parent().unwrap_or(folder);
    let built = out.join("built.html");
    let mut build = Command::new("python3");
    build
        .arg("-I")
        .arg(scripts.join("build.py"))
        .arg(&out)
        .arg(&built)
        .arg("--repo")
        .arg(repo)
        .args(["--rev", commit])
        .current_dir(&out);
    match run_to_log(build, &logs.join("build.log"), BUILD_LIMIT) {
        Ok((true, _)) => {}
        Ok((false, printed)) => {
            return Err(format!("build.py failed:\n{}", last_lines(&printed, 10)));
        }
        Err(why) => return Err(format!("build.py did not run: {why}")),
    }
    let page =
        with_csp(&fs::read_to_string(&built).map_err(|e| format!("build.py wrote no page: {e}"))?);
    let world = out.join("world.html");
    let guard = logs.join("verify-guard.mjs");
    fs::write(&world, &page)
        .and_then(|()| fs::write(&guard, GUARD))
        .map_err(|e| e.to_string())?;
    let mut verify = Command::new("node");
    verify
        .arg("--import")
        .arg(&guard)
        .arg(scripts.join("verify.mjs"))
        .arg(&world)
        .current_dir(&out);
    match run_to_log(verify, &logs.join("verify.log"), VERIFY_LIMIT) {
        Ok((true, _)) => Ok(page),
        Ok((false, printed)) => {
            let fails: Vec<&str> = printed
                .lines()
                .filter(|line| line.starts_with("FAIL "))
                .collect();
            let said = if fails.is_empty() {
                last_lines(&printed, 10)
            } else {
                last_lines(&fails.join("\n"), 20)
            };
            Err(format!("verify.mjs failed:\n{said}"))
        }
        Err(why) => Err(format!("verify.mjs did not run: {why}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_is_the_first_element_whatever_the_page_starts_with() {
        let meta = format!(r#"<meta http-equiv="Content-Security-Policy" content="{CSP}">"#);
        for page in [
            "<!DOCTYPE html><html><head><title>t</title>",
            "\u{feff}  <!doctype html>\n<script>x()</script>",
            "<script>x()</script>",
        ] {
            let out = with_csp(page);
            assert!(out.starts_with(&format!("<!doctype html>{meta}")), "{out}");
            assert_eq!(
                out.matches("<!doctype").count() + out.matches("<!DOCTYPE").count(),
                1,
                "{out}"
            );
        }
        assert!(with_csp("<script>x()</script>").ends_with("<script>x()</script>"));
    }
}
