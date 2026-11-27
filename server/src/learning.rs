//! The learning-layer commands: the writer settings, the turn's summaries and facts, and the questions Daniel asks (spec #720).

use super::*;

// The settings' commands.

/// The settings' `summaries` (danielrispler/Falinks#517, decision 17): the writer's model and effort, and whether a
/// summary is written when Daniel opens its turn instead of at the turn's end.
pub fn summary_settings(
    app: &App,
    model: String,
    effort: String,
    writer: WriterKind,
    on_open: bool,
) {
    app.writer.use_writer(writer);
    // A base URL edited in `settings.json` by hand takes effect with the page's next read of it.
    app.writer.use_base_url(settings::openai_base_url());
    app.writing.settings(
        SummarySetup {
            model,
            effort,
            writer,
        },
        on_open,
    );
}

/// Turns summaries on or off (spec #320, "The Test gate"): the writer's calls, and what the page's writer section reads.
pub fn set_summaries_on(app: &App, on: bool) {
    app.summaries_on.store(on, Ordering::Release);
    app.writer.summaries_on(on);
}

/// What the page's writer section lists (spec #320, "Writer settings on the page"): the writers the login shell's
/// `command -v` finds, whether summaries are on, and whether PR briefs can be written, which needs `claude`.
#[derive(serde::Serialize)]
pub struct Writers {
    pub writers: Vec<studio_understanding::FoundWriter>,
    pub summaries_on: bool,
    pub briefs: bool,
    /// The API-key writers whose key is set; never a key.
    pub keys: Vec<WriterKind>,
    /// The OpenAI-compatible writer's base URL, from `settings.json`; none: OpenAI's own.
    pub base_url: Option<String>,
}

pub fn writers(app: &App) -> Writers {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into());
    let mut writers = studio_understanding::found_writers(shell.as_ref());
    let keys = app.key_set().clone();
    let base_url = settings::openai_base_url();
    // An API-key writer is found when its key is set, the OpenAI-compatible one also when its base URL is; the key itself
    // never reaches the page.
    for writer in writers.iter_mut().filter(|w| w.kind.http()) {
        writer.found = keys.contains(&writer.kind)
            || (writer.kind == WriterKind::OpenAI && base_url.is_some());
    }
    Writers {
        writers,
        summaries_on: app.summaries_on.load(Ordering::Acquire),
        briefs: app.briefs.writes(),
        keys,
        base_url,
    }
}

/// A Test's outcome: `passed` or the failure's kind, the line the page shows, and `settings.json`'s new text when the pass
/// saved the choice.
#[derive(serde::Serialize)]
pub struct WriterTest {
    pub outcome: &'static str,
    pub line: String,
    pub settings: Option<String>,
}

/// The page's Test of `writer` at `model` and `effort`: one tiny call. A pass saves the choice in `settings.json` (`expected`
/// is the text the page last read) and, once saved, turns summaries on, now and at every start; a failure changes nothing.
pub fn test_writer(
    app: &App,
    writer: WriterKind,
    model: String,
    effort: String,
    expected: Option<String>,
) -> WriterTest {
    use studio_understanding::FailureKind;
    if let Err(error) = app.writer.test(writer, &model, &effort) {
        let outcome = match error.kind {
            FailureKind::NotLoggedIn => "not_logged_in",
            FailureKind::Quota => "quota",
            FailureKind::Timeout => "timeout",
            FailureKind::NotInstalled => "not_installed",
            FailureKind::Empty => "empty",
            FailureKind::Other => "other",
        };
        return WriterTest {
            outcome,
            line: error.reason(),
            settings: None,
        };
    }
    let saved = settings::save_writer(expected, writer, &model, &effort).and_then(|text| {
        let [file, _] = settings::paths().ok_or("no home folder")?;
        settings::open_summaries(file.parent().unwrap_or(Path::new("")))?;
        Ok(text)
    });
    // Only a saved choice opens summaries: one not saved would write with a writer the next start does not know.
    if saved.is_ok() {
        set_summaries_on(app, true);
    }
    let name = writer.program();
    match saved {
        Ok(text) => WriterTest {
            outcome: "passed",
            line: format!("Passed: `{name}` answered. Summaries are written with it."),
            settings: Some(text),
        },
        Err(e) => WriterTest {
            outcome: "passed",
            line: format!("Passed: `{name}` answered, but the choice was not saved: {e}"),
            settings: None,
        },
    }
}

/// Saves what the page set for API-key writer `writer` (spec #320, "API-key writers"): the key pasted, in the Keychain,
/// replacing the one set, and given to the writer; for the OpenAI-compatible writer (#367), also its base URL, in
/// `settings.json` (`expected` is the text the page last read; empty: OpenAI's own), and its key is optional. Returns
/// `settings.json`'s new text when it changed. The key is never sent back, logged or written anywhere else.
pub fn set_api_key(
    app: &App,
    writer: WriterKind,
    key: Option<String>,
    base_url: Option<String>,
    expected: Option<String>,
) -> Result<Option<String>, String> {
    if !writer.http() {
        return Err(format!("`{}` takes no API key", writer.program()));
    }
    let key = key
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty());
    let base_url = base_url.filter(|_| writer == WriterKind::OpenAI);
    let url = base_url.as_deref().map(str::trim);
    if let Some(url) = url
        && !url.is_empty()
        && !url.starts_with("http://")
        && !url.starts_with("https://")
    {
        return Err(
            "The base URL starts with http:// or https://, such as http://localhost:11434/v1."
                .into(),
        );
    }
    // The key first: one the store refuses leaves `settings.json` as the page read it.
    if let Some(key) = &key {
        app.keys.set(writer, key)?;
        app.writer.use_key(writer, Some(key.clone()));
        let mut set = app.key_set();
        if !set.contains(&writer) {
            set.push(writer);
        }
    }
    let Some(url) = url else { return Ok(None) };
    let text = settings::save_base_url(expected, Some(url).filter(|url| !url.is_empty()))?;
    app.writer.use_base_url(settings::openai_base_url());
    Ok(Some(text))
}

/// Removes API-key writer `writer`'s key from the Keychain and from the writer, whose calls then fail as having no key; for
/// the OpenAI-compatible writer, its base URL too. Returns `settings.json`'s new text when it changed.
pub fn remove_api_key(
    app: &App,
    writer: WriterKind,
    expected: Option<String>,
) -> Result<Option<String>, String> {
    app.keys.remove(writer)?;
    app.writer.use_key(writer, None);
    app.key_set().retain(|&kind| kind != writer);
    if writer != WriterKind::OpenAI {
        return Ok(None);
    }
    let text = settings::save_base_url(expected, None)?;
    app.writer.use_base_url(None);
    Ok(Some(text))
}

/// The settings' `checks` (danielrispler/plotkeeper#112): each project folder's list of commands that are checks.
pub fn check_lists(app: &App, lists: HashMap<String, Vec<String>>) {
    app.core().set_check_lists(lists);
    app.list_changed();
}

// The turn's commands.

/// A check run's exit code and output tail (danielrispler/plotkeeper#112), read from the event log through
/// its ledger row's offset, off the studio's lock. `None` when the session has no such call.
pub fn check_output(
    app: &App,
    session: String,
    agent: Option<String>,
    call: String,
) -> Result<Option<studio_core::CheckOutput>, String> {
    let row = {
        let studio = app.core();
        let ledger = &studio.session(&session).map(|s| &s.ledger);
        let row = ledger.and_then(|l| l.row(agent.as_deref(), &call)).cloned();
        drop(studio);
        row
    };
    let Some(row) = row else { return Ok(None) };
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    studio_core::check_output(&row, &log)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The recorded facts of an Ask anchor (danielrispler/plotkeeper#125): from the ledger and the event log through its
/// offsets, with no writer in the call. `None` when the session or the thing is not known.
pub fn ask_facts(
    app: &App,
    session: String,
    anchor: studio_core::Anchor,
) -> Result<Option<studio_core::AskFacts>, String> {
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    // The lock is held only to copy what the studio holds: the log is read after it is freed.
    let recorded = app.core().ask_recorded(&session, &anchor);
    Ok(recorded.map(|r| r.read(&log)))
}

/// A file's hunk of turn `turn` (danielrispler/plotkeeper#114), whole for the diff pane (#131) when `all`: `git diff` between the turn's two trees of worktree
/// `root`, off the studio's lock. `None` when the turn has no trees there, or `git` cannot read them.
pub fn turn_file_hunk(
    app: &App,
    session: String,
    turn: usize,
    root: String,
    path: String,
    from: Option<String>,
    all: Option<bool>,
) -> Option<studio_understanding::Hunk> {
    let (start, end) = app.core().turn_trees(&session, turn, &root)?;
    turn_hunk(
        Path::new(&root),
        &start,
        end.as_deref(),
        &path,
        from.as_deref(),
        all.unwrap_or(false),
    )
}

/// Daniel opened turn `id` (danielrispler/Falinks#526): its summary, written now when it waits for him, which takes
/// seconds. `None` when the turn has none. A failed write keeps the turn waiting and is an error, so the window asks again
/// only at the next opening.
pub fn turn_summary(app: &App, id: String) -> Result<Option<StoredSummary>, String> {
    app.writing.opened(&id)
}

/// The whole final reply of turn `index` of session `id` (danielrispler/Falinks#574), which the rail fetches when Daniel
/// opens that turn's folded response; the session list never carries it.
pub fn turn_reply(app: &App, id: String, index: usize) -> Option<String> {
    app.core().turn_reply(&id, index)
}

/// Daniel opened subagent `agent_id` (danielrispler/Falinks#535): the summary of its purpose and result, written now
/// when it has none and has ended, which takes seconds.
/// `None` while it runs or when the call failed, and the rail keeps its first line.
pub fn agent_summary(app: &App, agent_id: String) -> Option<String> {
    let (prompt, message) = {
        let studio = app.core();
        let mut agents = studio.sessions().into_iter().flat_map(|s| &s.agents);
        let agent = agents.find(|a| a.agent.id == agent_id && a.ended_at.is_some());
        let found = agent.map(|agent| (agent.prompt.clone(), agent.message.clone()));
        drop(studio);
        found?
    };
    // A subagent that ended without a last message (killed, or its session ended) has no result to sum up.
    if message.is_empty() {
        return None;
    }
    let setup = app.writing.setup();
    write_agent_summary(
        &app.cards,
        &*app.writer,
        &setup,
        &agent_id,
        &prompt,
        &message,
        report,
    )
}

/// Daniel's side `question` on `item`, a response, card or intention of session `session` (danielrispler/Falinks#461,
/// decision 20): the writer's answer, read with the turn's intentions, or why there is none, which
/// the window shows with a retry. It takes seconds; nothing reaches the session.
pub fn ask(app: &App, session: String, item: String, question: String) -> Result<String, String> {
    // The lock ends with the statement, before the writer runs.
    let transcript = app.core().session(&session).map(|s| s.transcript.clone());
    let answer = studio_understanding::ask_about(
        &*app.writer,
        &transcript.unwrap_or_default(),
        &item,
        &question,
    );
    if let Err(line) = &answer {
        log::error!("no answer on {session}: {}", scrubbed(line));
    }
    answer
}

/// Daniel's `question` on an Ask anchor (danielrispler/plotkeeper#126): the writer's answer, read with the anchor's recorded
/// facts and, for a file, its hunk. The facts stay above it in the window; this is the interpretation.
pub fn ask_anchor(
    app: &App,
    session: String,
    anchor: studio_core::Anchor,
    what: String,
    question: String,
) -> Result<String, String> {
    let log = studio_hook::event_log(&std::env::home_dir().ok_or("no home folder")?);
    let (recorded, transcript, trees) = {
        let studio = app.core();
        let trees = match &anchor {
            studio_core::Anchor::File { turn, root, .. } => {
                studio.turn_trees(&session, *turn, root)
            }
            _ => None,
        };
        let transcript = studio.session(&session).map(|s| s.transcript.clone());
        (studio.ask_recorded(&session, &anchor), transcript, trees)
    };
    // The event log is read with the studio's lock freed.
    let facts = recorded.ok_or("nothing is recorded about this")?.read(&log);
    let hunk = match (&anchor, trees) {
        (
            studio_core::Anchor::File {
                root, path, from, ..
            },
            Some((start, end)),
        ) => turn_hunk(
            Path::new(root),
            &start,
            end.as_deref(),
            path,
            from.as_deref(),
            false,
        )
        .filter(|h| !h.binary)
        .map(|h| {
            format!(
                "{}{}",
                h.text,
                if h.left_out > 0 {
                    format!("... {} more lines", h.left_out)
                } else {
                    String::new()
                }
            )
        }),
        _ => None,
    };
    let item = studio_understanding::ask_item(&what, &facts, hunk.as_deref());
    let answer = studio_understanding::ask_about(
        &*app.writer,
        &transcript.unwrap_or_default(),
        &item,
        &question,
    );
    if let Err(line) = &answer {
        log::error!("no answer on {session}: {}", scrubbed(line));
    }
    answer
}

/// "Why didn't it touch `name`?" in turn `turn` (danielrispler/plotkeeper#126): what was recorded about `name`, and the
/// writer's possible reasons only when something was.
pub fn ask_why_not(
    app: &App,
    session: String,
    turn: usize,
    name: String,
) -> Result<studio_understanding::WhyNot, String> {
    let (facts, transcript) = {
        let studio = app.core();
        (
            studio.ask_named(&session, turn, &name),
            studio.session(&session).map(|s| s.transcript.clone()),
        )
    };
    let facts = facts.ok_or("this turn is not known")?;
    studio_understanding::why_not(&*app.writer, &transcript.unwrap_or_default(), &name, facts)
}
