//! The part catch-up (spec #378, ticket #391): one story, in the one-part PR brief's shape, of everything that landed in one
//! fogged part since Daniel last caught up. It reuses the PR brief's reply, budget, code checks, claim check and retry; only its
//! facts and its instructions are its own.
//!
//! Public API, in the order a caller meets it:
//! - `gather_catch_up` (`git` only) fills a `CatchUpSource`, plain data, so a test builds one by hand;
//! - `build_catch_up_facts` turns it into the PR brief's `Facts`: structure first (the map at the newest entry, the map's diff
//!   over the fog range, the part's laws with their tagged tests, the flows through it), then each fog entry's PR title and body
//!   or commit subject, then one combined diff of the part's paths, cut first when over budget;
//! - `check_catch_up` is the PR brief's `check_brief` plus the catch-up's own step count;
//! - `CatchUpStore` keeps one catch-up per part, with the fog list it covers, and the records;
//! - `write_part_catch_up` does all of it with a `BriefWriter`.

use crate::millis;
use crate::pr_brief::{
    BUDGET, BriefRecord, BriefWriter, BudgetRecord, CUT_NOTE_ROOM, CheckContext, Facts, PrMeta,
    Shape, TaggedTest, check_brief, cut_to, facts_block, flow_key, gather, ids, parse_reply,
    prompt, section, split_diff, timed_write_facts,
};
use crate::pr_brief::{Brief, CallKind, INSTRUCTIONS, PartialFacts};
use crate::pr_brief_check::Stage;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The stored catch-up's format version; a reader ignores fields it does not know.
pub const STORED_VERSION: u32 = 1;

/// One fog entry of the part: a commit, with its PR's title and body when it came from one (a squash merge carries them), else
/// its subject.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FogEntry {
    pub commit: String,
    pub pr: Option<u64>,
    pub title: String,
    pub body: String,
}

/// Everything the facts builder reads, gathered.
#[derive(Clone, Debug, Default)]
pub struct CatchUpSource {
    /// The part's id.
    pub part: String,
    /// The fog list the catch-up covers: the commits of the part's fog entries, oldest first.
    pub fog: Vec<String>,
    pub entries: Vec<FogEntry>,
    /// `system-map.json` at the newest entry.
    pub map_text: String,
    /// The map's diff from before the oldest entry to the newest, empty when it did not change.
    pub map_diff: String,
    /// The whole diff over the same range; the facts keep the part's own files.
    pub diff: String,
    /// Every `@law:` tag at the newest entry with its test, `changed` not yet set.
    pub tagged: Vec<TaggedTest>,
}

/// The catch-up's own head; the PR brief's step, figure and check rules follow it (from "A step:" on), so a fix to them reaches
/// catch-ups too.
const HEAD: &str = r#"You write a part catch-up: a short, stepped, plain-language story of what changed in ONE part of a system since its owner last caught up on it, for the system's owner. He directs AI agents that write the code; he does not read code. The story exists to keep his mental model of that part accurate: what it does now, how behaviour flows through it, what must stay true, what could break. It is not a changelog and not a file tour. The story plays on a map of the system: each step lights parts and may send a light along links.

You get FACTS, in the system prompt: the part's structure first (THE PART, with its laws, their tagged tests and the flows through it), then FOG ENTRIES, every PR or commit that landed in the part since he last caught up, oldest first, then one combined DIFF of the part's files over that whole range. SHAPE says which part; follow it exactly.

SELECTION. Tell ONE story over all the entries, ordered by behaviour, never one step per entry or per commit. Find the part's central changes, the ones the owner would name. Apply the deletion test: "if this change were removed, would the part do something different for the owner?" Keep those; leave out plumbing, renames, test scaffolding, docs and anything that only moves code around. Choose, do not list.

EVIDENCE LADDER. Say only what the facts support, at the strength they support it:
- "a test tagged to law X covers ..." ONLY when THE PART marks one of law X's tagged tests "changed in the fog range", and then the step names that test in "covered_by"; otherwise you may still name the law, as "the map's law X says ...", but never claim a test covers this change;
- "the PR says ..." for anything only an entry's text states;
- otherwise leave it out. Do not hedge, do not guess, do not explain why something was left out.
Never write these words in your own prose: guarantee(s/d), ensure(s/d), cannot, can't, impossible, always works, never fails. Say what is covered, not what is promised.
Never mention in the story what you did not see or what was not sent. Put each gap you noticed in "unseen", one short line each.

FIGURES. A step that tells a state change (something goes from one state to another), an ordered flow (things happen one after another) or a before/after gives it the matching figure from "Figure kinds" below: "states", "sequence" or "compare". The figure shows only what the step's words say, with the facts' own names.

CITES. Every step has "cites": a list of {"from": "ENTRIES" | "MAP" | "DIFF" | "TESTS", "quote": "..."}, where quote is an exact phrase of 3 to 15 words copied from that section of FACTS. Code checks each quote; a step whose quote is not in the facts is marked unconfirmed.

Reply with ONE JSON object and nothing else:

{
  "title": "a plain title, at most 9 words",
  "chapters": [ { "behaviour": "story", "parts": [ "the part's id" ], "title": "at most 6 words", "gist": "required: the story's whole point in ONE plain sentence, at most 30 words, that the owner could say back from memory; it says only what the steps say", "steps": [ step, ... ] } ],
  "also": [],
  "unseen": [ "a gap you noticed in the facts" ],
  "decisions": []
}

The one chapter is 3 or 4 steps: Where (the part, first), Before, Now, and at most one of Must stay true / Could break / Your turn. This overrides the brief's step counts below; "Not in the plan" does not apply, since a catch-up has no spec.

"#;

/// The catch-up's instructions: its head, then the PR brief's rules from "A step:" on.
fn instructions() -> String {
    let rules = INSTRUCTIONS
        .find("A step:")
        .map_or(INSTRUCTIONS, |at| &INSTRUCTIONS[at..]);
    format!("{HEAD}{rules}")
}

/// The files of `diff` the part owns, by the map's ownership (the union of the newest map and none else: the range's files
/// moved out of the part stay where the newest map puts them).
fn part_files(map: &system_map::Map, part: &str, paths: &[String]) -> Vec<String> {
    system_map::touched_for_map(map, paths)
        .parts
        .into_iter()
        .filter(|p| p.id == part)
        .flat_map(|p| p.files)
        .collect()
}

/// Assembles the writer's facts: the part's structure, then the fog entries, then one combined diff of the part's paths inside
/// the PR brief's `BUDGET`; the code is cut first, and "DIFF NOT SENT" says what was.
pub fn build_catch_up_facts(source: &CatchUpSource) -> Result<Facts, String> {
    let map = system_map::parse_lenient(&source.map_text)?;
    // The map comes from git as it was committed, so a version 1 map is read converted.
    let mj: serde_json::Value =
        serde_json::from_str(&system_map::convert(&source.map_text)?).map_err(|e| e.to_string())?;
    let part = source.part.as_str();
    let all = split_diff(&source.diff);
    let paths: Vec<String> = all.iter().map(|f| f.path.clone()).collect();
    let mine = part_files(&map, part, &paths);
    let files: Vec<_> = all.iter().filter(|f| mine.contains(&f.path)).collect();

    // The tagged tests, and whether the range changed one (wherever the test lives).
    let mut tagged = source.tagged.clone();
    for t in &mut tagged {
        t.changed = all.iter().any(|fd| {
            fd.path == t.path
                && fd.hunks.iter().any(|h| {
                    (!t.test.is_empty() && h.contains(&t.test))
                        || h.contains(&format!("@law: {}", t.law))
                })
        });
    }

    let links: Vec<String> = mj["links"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["from"] == part || l["to"] == part)
        .filter_map(|l| l["id"].as_str().map(String::from))
        .collect();
    let through = |flow: &serde_json::Value| {
        flow["hops"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|h| links.iter().any(|k| h["link"] == k.as_str()))
    };
    let flows: Vec<&serde_json::Value> = mj["flows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| through(f))
        .collect();
    let map_diff = if source.map_diff.is_empty() {
        "none".to_string()
    } else {
        source.map_diff.clone()
    };
    // An order check only when a flow's hops through the part changed: the flows the map's diff names among those through it.
    let all_parts = ids(&mj, "parts");
    let flows_changed: BTreeMap<String, Vec<String>> = flows
        .iter()
        .filter_map(|f| {
            let id = f["id"].as_str()?;
            map_diff
                .contains(&format!("\"{id}\""))
                .then(|| (id.to_string(), flow_key(&mj, f, &all_parts)))
        })
        .collect();

    let laws: Vec<String> = mj["laws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["on"] == part || links.iter().any(|k| l["on"] == k.as_str()))
        .map(|l| {
            let id = l["id"].as_str().unwrap_or("");
            let tests: Vec<String> = tagged
                .iter()
                .filter(|t| t.law == id)
                .map(|t| {
                    format!(
                        "{} in {} ({})",
                        t.test,
                        t.path,
                        if t.changed {
                            "changed in the fog range"
                        } else {
                            "not changed in the fog range"
                        }
                    )
                })
                .collect();
            format!(
                "{} ({}): {} [tagged tests: {}]",
                id,
                l["backing"].as_str().unwrap_or(""),
                l["text"].as_str().unwrap_or(""),
                if tests.is_empty() {
                    "none".into()
                } else {
                    tests.join("; ")
                }
            )
        })
        .collect();
    let pj = mj["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == part)
        .cloned()
        .unwrap_or_default();
    let mut the_part = format!(
        "## {part} ({})\n{}\nlaws:\n  {}\nflows through it: {}\n",
        pj["name"].as_str().unwrap_or(""),
        system_map::part_text(&pj),
        if laws.is_empty() {
            "none".into()
        } else {
            laws.join("\n  ")
        },
        if flows.is_empty() {
            "none".into()
        } else {
            flows
                .iter()
                .filter_map(|f| f["id"].as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    for f in &files {
        the_part += &format!("- {} +{}/-{}\n", f.path, f.added, f.removed);
    }

    let entries: Vec<String> = source
        .entries
        .iter()
        .map(|e| {
            let short = &e.commit[..e.commit.len().min(9)];
            match e.pr {
                Some(n) => format!("- PR #{n} {} ({short})\n{}", e.title, e.body.trim_end()),
                None => format!("- commit {short}, pushed with no PR: {}", e.title),
            }
        })
        .collect();
    let shape_text = format!(
        "part catch-up: a single story (one chapter with part \"story\") of 3 or 4 steps about part {part}, over {} fog entries, ordered by behaviour",
        source.entries.len()
    );
    let mut sections: Vec<(String, String)> = vec![
        ("SHAPE (decided by code)".into(), shape_text),
        ("SYSTEM MAP".into(), source.map_text.clone()),
        ("MAP DIFF OVER THE FOG RANGE".into(), map_diff),
        ("THE PART".into(), the_part),
        ("FOG ENTRIES".into(), entries.join("\n\n")),
    ];
    // Only when the structure and the entries alone leave no room for the diff's note are the entries cut.
    let used = |sections: &[(String, String)]| {
        sections
            .iter()
            .map(|(n, t)| section(n, t).len())
            .sum::<usize>()
    };
    let over = (used(&sections) + CUT_NOTE_ROOM).saturating_sub(BUDGET);
    if over > 0
        && let Some((_, text)) = sections.iter_mut().find(|(n, _)| n == "FOG ENTRIES")
    {
        *text = cut_to(text, text.len().saturating_sub(over));
    }

    // The diff, the part's files only: smaller files first, hunk by hunk in the room that is left.
    let mut order = files.clone();
    order.sort_by_key(|f| f.added + f.removed);
    let mut room = BUDGET.saturating_sub(used(&sections) + CUT_NOTE_ROOM);
    let mut diff_text = String::new();
    let mut cut: Vec<String> = Vec::new();
    let mut budget = BudgetRecord {
        diff_bytes_full: files
            .iter()
            .map(|f| f.header.len() + f.hunks.iter().map(String::len).sum::<usize>())
            .sum(),
        ..BudgetRecord::default()
    };
    for f in order {
        let mut sent = 0;
        let mut text = String::new();
        if f.header.len() < room {
            text += &f.header;
            for h in &f.hunks {
                if text.len() + h.len() <= room {
                    text += h;
                    sent += 1;
                } else {
                    break;
                }
            }
        }
        if sent > 0 || (f.hunks.is_empty() && !text.is_empty()) {
            room -= text.len();
            diff_text += &text;
        }
        if sent == f.hunks.len() && !text.is_empty() {
            budget.files_sent += 1;
        } else {
            if sent > 0 {
                budget.files_partial += 1;
            } else {
                budget.files_cut += 1;
            }
            cut.push(format!(
                "- {}: +{}/-{}, {} of {} hunks not sent",
                f.path,
                f.added,
                f.removed,
                f.hunks.len() - sent,
                f.hunks.len()
            ));
        }
    }
    let cut_text = if cut.is_empty() {
        "nothing: the whole diff of the part is below".to_string()
    } else {
        let mut t = format!(
            "{} files not fully sent; do not guess what they hold:\n",
            cut.len()
        );
        for (i, line) in cut.iter().enumerate() {
            if t.len() + line.len() + 1 > CUT_NOTE_ROOM - 200 {
                t += &format!("- ... and {} more\n", cut.len() - i);
                break;
            }
            t += line;
            t += "\n";
        }
        t
    };
    sections.push(("DIFF NOT SENT".into(), cut_text));
    sections.push(("DIFF".into(), diff_text.clone()));
    let text: String = sections.iter().map(|(n, t)| section(n, t)).collect();
    budget.total_bytes = text.len();
    budget.sections = sections
        .iter()
        .map(|(n, t)| (n.clone(), section(n, t).len()))
        .collect();

    let required = vec![part.to_string()];
    let context = CheckContext {
        facts: text.clone(),
        // A pick check's test must be in what the writer saw of the range.
        diff: diff_text,
        chapters_mode: false,
        required: required.clone(),
        also: vec![],
        overview: false,
        parts: all_parts,
        actors: ids(&mj, "actors"),
        links: ids(&mj, "links"),
        laws: ids(&mj, "laws"),
        flows: ids(&mj, "flows"),
        law_texts: mj["laws"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["text"].as_str().map(str::to_lowercase))
            .collect(),
        flows_changed: flows_changed.clone(),
        tagged,
        spec_same_day: false,
        // Every part catch-up carries a gist (#397), whatever the part's tier; the window offers explain-back on Core parts only.
        core: required.clone(),
    };
    Ok(Facts {
        text,
        shape: Shape {
            chapters: false,
            required,
            also: vec![],
            overview: false,
            flows_changed: flows_changed.into_keys().collect(),
        },
        budget,
        context,
        instructions: Some(instructions()),
        guard: None,
        partial_facts: PartialFacts::default(),
    })
}

/// The PR brief's code checks, plus the catch-up's own: its one story is 3 or 4 steps. Returns how many items are flagged.
pub fn check_catch_up(brief: &mut Brief, ctx: &CheckContext) -> usize {
    check_brief(brief, ctx);
    for c in &mut brief.chapters {
        let n = c.steps.len();
        if !(3..=4).contains(&n) {
            c.flags.push(format!("part catch-up: {n} steps"));
        }
    }
    brief.flagged()
}

// ---------------------------------------------------------------------------------------------------------------
// Keeping catch-ups.

/// A catch-up as stored: one per part of a project, with the fog list it covers. A reader ignores fields it does not know.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StoredCatchUp {
    #[serde(default)]
    pub version: u32,
    pub part: String,
    /// The commits of the part's fog entries it covers, oldest first: a fog list that differs makes it stale.
    #[serde(default)]
    pub fog: Vec<String>,
    #[serde(default)]
    pub written_at: u64,
    #[serde(default)]
    pub model: String,
    /// The writer saw only part of the part's diff.
    #[serde(default)]
    pub partial: bool,
    #[serde(default)]
    pub brief: Brief,
    /// Over more than `FEW` fog entries: the line saying the story summarises the main shifts, and the full fog list behind it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<Summary>,
}

/// More fog entries than this and a catch-up says it summarises the main shifts (spec #378, "Reply").
pub const FEW: usize = 3;

/// The summarising line and the full fog list, over more than `FEW` entries. The list keeps each entry's PR and title, not its
/// body.
fn summary(entries: &[FogEntry]) -> Option<Summary> {
    (entries.len() > FEW).then(|| Summary {
        line: format!(
            "This catch-up summarises the main shifts of {} fog entries; the full list is below.",
            entries.len()
        ),
        entries: entries
            .iter()
            .map(|e| FogEntry {
                body: String::new(),
                ..e.clone()
            })
            .collect(),
    })
}

/// A long catch-up's summarising line and the full fog list it links, oldest first (each entry's PR and title or its subject).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub line: String,
    #[serde(default)]
    pub entries: Vec<FogEntry>,
}

/// The catch-ups and their records in a project's data folder: `part-catch-ups/`, one file per part, and
/// `part-catch-up-records.jsonl`, in the PR brief's record format.
#[derive(Clone)]
pub struct CatchUpStore {
    dir: PathBuf,
}

impl CatchUpStore {
    pub fn open(data: &Path) -> std::io::Result<Self> {
        let dir = data.join("part-catch-ups");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self, part: &str) -> PathBuf {
        let name: String = part
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{name}.json"))
    }

    /// The catch-up of `part`, whatever fog list it covers.
    pub fn get(&self, part: &str) -> Option<StoredCatchUp> {
        serde_json::from_slice::<StoredCatchUp>(&std::fs::read(self.path(part)).ok()?)
            .ok()
            .filter(|c| c.part == part)
    }

    /// Writes `catch_up`, through a temporary file so a crash leaves the old one.
    pub fn put(&self, catch_up: &StoredCatchUp) -> std::io::Result<()> {
        let path = self.path(&catch_up.part);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(catch_up).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(tmp, path)
    }

    pub fn record(&self, record: &BriefRecord) -> std::io::Result<()> {
        let mut line = serde_json::to_string(record).map_err(std::io::Error::other)?;
        line.push('\n');
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.dir.join("part-catch-up-records.jsonl"))?
            .write_all(line.as_bytes())
    }

    /// The records written so far, oldest first.
    pub fn records(&self) -> Vec<BriefRecord> {
        std::fs::read_to_string(self.dir.join("part-catch-up-records.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The entry.

/// Writes the catch-up of `source`'s part: builds the facts, asks the writer once, parses and checks the reply, runs the PR
/// brief's claim check and one retry, stores it and appends its record line. `log` hears one line per problem that does not
/// stop the write; `stage` hears each call as it starts (#450).
pub fn write_part_catch_up(
    writer: &(impl BriefWriter + ?Sized),
    model: &str,
    source: &CatchUpSource,
    store: &CatchUpStore,
    mut log: impl FnMut(String),
    mut stage: impl FnMut(Stage),
) -> Result<StoredCatchUp, String> {
    let facts = build_catch_up_facts(source)?;
    stage(Stage::Writing);
    let reply = timed_write_facts(
        writer,
        model,
        CallKind::Write,
        &facts_block(&facts),
        &prompt(&facts),
    )?;
    let mut brief = parse_reply(&reply.text)?;
    // A catch-up ratifies nothing and has no overview.
    brief.decisions.clear();
    brief.overview.clear();
    check_catch_up(&mut brief, &facts.context);
    let mut record = BriefRecord {
        pr: 0,
        head: source.fog.last().cloned().unwrap_or_default(),
        at: millis(),
        shape: format!("part catch-up of {}", source.part),
        budget: facts.budget.clone(),
        flags: brief.all_flags(),
        ..BriefRecord::default()
    };
    let mut usage = reply.usage;
    usage.role = "writer".into();
    record.calls.push(usage);
    crate::pr_brief_check::claim_check_and_retry(
        writer,
        model,
        &facts,
        &facts.context,
        &mut brief,
        &mut record,
        &mut log,
        &mut stage,
    );
    // The retry's code check is the brief's; the catch-up's own step count is checked again.
    for c in &mut brief.chapters {
        let n = c.steps.len();
        if !(3..=4).contains(&n) && !c.flags.iter().any(|f| f.starts_with("part catch-up:")) {
            c.flags.push(format!("part catch-up: {n} steps"));
        }
    }
    let stored = StoredCatchUp {
        version: STORED_VERSION,
        part: source.part.clone(),
        fog: source.fog.clone(),
        written_at: millis(),
        model: model.to_string(),
        partial: facts.budget.files_cut + facts.budget.files_partial > 0,
        brief,
        summary: summary(&source.entries),
    };
    store
        .put(&stored)
        .map_err(|e| format!("keep the catch-up of {}: {e}", stored.part))?;
    record.at = stored.written_at;
    record.flags_after = stored.brief.all_flags();
    record.unconfirmed = stored
        .brief
        .steps()
        .filter(|s| {
            s.flags
                .iter()
                .any(|f| f == crate::pr_brief_check::UNCONFIRMED)
        })
        .count();
    if let Err(e) = store.record(&record) {
        log(format!("record the catch-up of {}: {e}", stored.part));
    }
    Ok(stored)
}

// ---------------------------------------------------------------------------------------------------------------
// Gathering: git only.

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git did not run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The PR a squash-merge subject `Title (#N)` came from, and the title without it.
fn split_subject(subject: &str) -> (Option<u64>, String) {
    let subject = subject.trim_end();
    subject
        .strip_suffix(')')
        .and_then(|s| s.rsplit_once(" (#"))
        .and_then(|(title, n)| Some((Some(n.parse().ok()?), title.to_string())))
        .unwrap_or((None, subject.to_string()))
}

/// What `git` gives for the catch-up of `part` over `fog` (its fog entries' commits, oldest first) in `repo`: the map at the
/// newest, the map's diff and the whole diff from the oldest's parent, the tagged tests, and each entry's PR or subject.
pub fn gather_catch_up(repo: &Path, part: &str, fog: &[String]) -> Result<CatchUpSource, String> {
    let (Some(oldest), Some(newest)) = (fog.first(), fog.last()) else {
        return Err("the part has no fog".into());
    };
    // The oldest entry's parent; a root commit has none, so the empty tree stands in.
    let base = git(repo, &["rev-parse", &format!("{oldest}^")])
        .map(|s| s.trim().to_string())
        .or_else(|_| {
            git(repo, &["hash-object", "-t", "tree", "/dev/null"]).map(|s| s.trim().to_string())
        })?;
    let pr = gather(repo, &base, newest, PrMeta::default())?;
    let mut entries = Vec::new();
    for commit in fog {
        let subject = git(repo, &["log", "-1", "--format=%s", commit])?;
        let (number, title) = split_subject(&subject);
        let body = match number {
            Some(_) => git(repo, &["log", "-1", "--format=%b", commit])?,
            None => String::new(),
        };
        entries.push(FogEntry {
            commit: commit.clone(),
            pr: number,
            title,
            body: body.trim_end().to_string(),
        });
    }
    Ok(CatchUpSource {
        part: part.to_string(),
        fog: fog.to_vec(),
        entries,
        map_text: pr.map_text,
        map_diff: pr.map_diff,
        diff: pr.diff,
        tagged: pr.tagged,
    })
}

#[cfg(test)]
mod tests {
    use super::split_subject;

    #[test]
    fn a_squash_subject_names_its_pr_and_a_plain_one_does_not() {
        assert_eq!(
            split_subject("Studio: thing (#12)\n"),
            (Some(12), "Studio: thing".to_string())
        );
        assert_eq!(split_subject("fix typo"), (None, "fix typo".to_string()));
        assert_eq!(split_subject("odd (#x)"), (None, "odd (#x)".to_string()));
    }
}
