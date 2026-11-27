//! The page's commands over HTTP: `POST /api/<command>` with a JSON body of the command's arguments, in the camel case the page
//! writes them, answered with the command's reply as JSON. Each arm only forwards to the function of the command's name in this
//! crate, as the Tauri shell's handlers did (danielrispler/plotkeeper#325). A command the page no longer has, such as a terminal's,
//! is not here and answers as unknown.

use crate::{App, answer as server_answer, stop_rules};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value};
use std::sync::Arc;

/// Why a command did not answer.
#[derive(Debug)]
pub enum Failure {
    /// No such command.
    Unknown,
    /// An argument that is missing or of the wrong type.
    Bad(String),
    /// The command ran and refused, with its reason.
    Failed(String),
}

type Args = Map<String, Value>;

/// What the page reports about itself, which the state reads through the sink.
pub trait Page {
    /// The page says whether its tab is visible and focused.
    fn focus(&self, focused: bool);
}

/// `camelCase` as `snake_case`: the name a Rust argument has.
fn snake(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn arg<T: DeserializeOwned>(args: &Args, key: &str) -> Result<T, Failure> {
    let value = args.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(value).map_err(|e| Failure::Bad(format!("{key}: {e}")))
}

/// All of a command's arguments as one value, for a command whose arguments are a wire type.
fn whole<T: DeserializeOwned>(args: &Args) -> Result<T, Failure> {
    serde_json::from_value(Value::Object(args.clone())).map_err(|e| Failure::Bad(e.to_string()))
}

fn out<T: Serialize>(reply: T) -> Result<Value, Failure> {
    Ok(serde_json::to_value(reply).expect("a reply serializes"))
}

/// A command with no reply: it runs, and the page gets `null`.
macro_rules! done {
    ($run:expr) => {{
        $run;
        Ok(Value::Null)
    }};
}

fn res<T: Serialize>(reply: Result<T, String>) -> Result<Value, Failure> {
    out(reply.map_err(Failure::Failed)?)
}

/// Runs command `name` over `args`; `page` takes what the page reports about itself.
#[allow(
    clippy::too_many_lines,
    reason = "one arm per page command: the dispatch reads as one table"
)]
pub fn call(app: &Arc<App>, page: &dyn Page, name: &str, args: Value) -> Result<Value, Failure> {
    let args: Args = match args {
        Value::Object(map) => map.into_iter().map(|(k, v)| (snake(&k), v)).collect(),
        Value::Null => Args::new(),
        _ => return Err(Failure::Bad("the arguments are not an object".into())),
    };
    let a = &args;
    let arc = app;
    let app = &**app;
    match name {
        "sessions" => out(crate::sessions(app)),
        "select" => out(crate::select(app, arg(a, "id")?)),
        "open" => out(crate::open(app, arg(a, "agent_id")?)),
        "selected" => out(crate::selected(app)),
        "subagent_log" => out(crate::subagent_log(
            app,
            arg(a, "session")?,
            arg(a, "agent_id")?,
            arg(a, "offset")?,
        )),
        "cards" => out(crate::cards(app)),
        "writer_notice" => out(app.writer_notice().clone()),
        "wired" => out(crate::wired()),
        "about" => out(crate::about()),
        "ask" => res(crate::ask(
            app,
            arg(a, "session")?,
            arg(a, "item")?,
            arg(a, "question")?,
        )),
        "ask_anchor" => res(crate::ask_anchor(
            app,
            arg(a, "session")?,
            arg(a, "anchor")?,
            arg(a, "what")?,
            arg(a, "question")?,
        )),
        "ask_why_not" => res(crate::ask_why_not(
            app,
            arg(a, "session")?,
            arg(a, "turn")?,
            arg(a, "name")?,
        )),
        "rename" => res(crate::rename(arg(a, "id")?, arg(a, "name")?)),
        "delete_session" => res(crate::delete_session(arg(a, "id")?)),
        "rail_shown" => done!(crate::rail_shown(app, arg(a, "shown")?)),
        "mark_unseen" => done!(crate::mark_unseen(app, arg(a, "id")?)),
        "summary_settings" => done!(crate::summary_settings(
            app,
            arg(a, "model")?,
            arg(a, "effort")?,
            arg(a, "writer")?,
            arg(a, "on_open")?,
        )),
        "writers" => out(crate::writers(app)),
        "set_api_key" => res(crate::set_api_key(
            app,
            arg(a, "writer")?,
            arg(a, "key")?,
            arg(a, "base_url")?,
            arg(a, "expected")?,
        )),
        "remove_api_key" => res(crate::remove_api_key(
            app,
            arg(a, "writer")?,
            arg(a, "expected")?,
        )),
        "test_writer" => out(crate::test_writer(
            app,
            arg(a, "writer")?,
            arg(a, "model")?,
            arg(a, "effort")?,
            arg(a, "expected")?,
        )),
        "check_lists" => done!(crate::check_lists(app, arg(a, "lists")?)),
        "check_output" => res(crate::check_output(
            app,
            arg(a, "session")?,
            arg(a, "agent")?,
            arg(a, "call")?,
        )),
        "ask_facts" => res(crate::ask_facts(app, arg(a, "session")?, arg(a, "anchor")?)),
        "turn_file_hunk" => out(crate::turn_file_hunk(
            app,
            arg(a, "session")?,
            arg(a, "turn")?,
            arg(a, "root")?,
            arg(a, "path")?,
            arg(a, "from")?,
            arg(a, "all")?,
        )),
        "turn_summary" => res(crate::turn_summary(app, arg(a, "id")?)),
        "turn_reply" => out(crate::turn_reply(app, arg(a, "id")?, arg(a, "index")?)),
        "agent_summary" => out(crate::agent_summary(app, arg(a, "agent_id")?)),
        "map_state" => res(crate::map_state(arc, arg(a, "project")?, arg(a, "fetch")?)),
        "map_scan" => res(crate::map_scan::map_scan(arg(a, "project")?)),
        "map_scan_edit" => res(crate::map_scan::map_scan_edit(
            arg(a, "project")?,
            arg(a, "map")?,
            arg(a, "edit")?,
        )),
        "map_draft" => out(crate::map_draft::map_draft(app, arg(a, "project")?)),
        "map_draft_start" => res(crate::map_draft::map_draft_start(
            app,
            arg(a, "project")?,
            arg(a, "skeleton")?,
        )),
        "map_draft_retry" => res(app
            .drafts
            .retry(&arg::<String>(a, "project")?, arg(a, "group")?)),
        "map_draft_redraft" => res(app
            .drafts
            .redraft(&arg::<String>(a, "project")?, &arg::<String>(a, "part")?)),
        "map_draft_edit" => res(app.drafts.edit(
            &arg::<String>(a, "project")?,
            &arg::<String>(a, "part")?,
            crate::map_draft::Edit {
                name: arg(a, "name")?,
                caption: arg(a, "caption")?,
                purpose: arg(a, "purpose")?,
                tier: arg(a, "tier")?,
            },
        )),
        "map_draft_discard" => res(app.drafts.discard(&arg::<String>(a, "project")?)),
        "map_draft_accept" => res(crate::map_draft::map_draft_accept(
            app,
            arg(a, "project")?,
            arg(a, "replace")?,
        )),
        "map_draft_pr" => res(app.drafts.open_pr(&arg::<String>(a, "project")?)),
        "map_pending_write" => res(app.drafts.pending_write(
            &arg::<String>(a, "project")?,
            arg::<Option<String>>(a, "replace")?.as_deref(),
        )),
        "map_pending_pr" => res(app.drafts.pending_pr(&arg::<String>(a, "project")?)),
        "map_pending_discard" => res(app.drafts.pending_discard(&arg::<String>(a, "project")?)),
        "map_know" => res(crate::map::map_know(arg(a, "project")?, arg(a, "part")?)),
        "map_refresher" => res(crate::map::map_refresher(
            arg(a, "project")?,
            arg(a, "part")?,
        )),
        "map_history" => res(crate::map::map_history(arg(a, "project")?, arg(a, "part")?)),
        "map_at" => res(crate::map::map_at(arg(a, "project")?, arg(a, "commit")?)),
        "map_commit" => res(crate::map::map_commit(
            arg(a, "project")?,
            arg(a, "commit")?,
        )),
        "pr_diff" => res(crate::map::pr_diff(whole(a)?)),
        "pr_brief" => res(crate::pr_briefs::pr_brief(
            arc,
            arg(a, "project")?,
            arg(a, "number")?,
            arg::<Option<bool>>(a, "full")?.unwrap_or(false),
        )),
        "brief_since_done" => res(crate::pr_briefs::brief_since_done(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
            arg(a, "head")?,
        )),
        "brief_chapter_done" => res(crate::pr_briefs::brief_chapter_done(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
            arg(a, "chapter")?,
            arg(a, "head")?,
        )),
        "part_catch_up" => res(crate::part_catch_ups::part_catch_up(
            arc,
            arg(a, "project")?,
            arg(a, "part")?,
            arg::<Option<bool>>(a, "older")?.unwrap_or(false),
        )),
        "part_catch_up_done" => res(crate::part_catch_ups::part_catch_up_done(
            app,
            arg(a, "project")?,
            arg(a, "part")?,
            arg(a, "fog")?,
        )),
        "pre_flight_issues" => res(crate::pre_flight::pre_flight_issues(arg(a, "project")?)),
        "pre_flight_predict" => res(crate::pre_flight::pre_flight_predict(
            arc,
            arg(a, "project")?,
            arg(a, "issue")?,
            arg(a, "text")?,
        )),
        "pre_flight_pick" => res(crate::pre_flight::pre_flight_pick(
            app,
            arg(a, "project")?,
            arg(a, "parts")?,
        )),
        "pre_flight_clear" => res(crate::pre_flight::pre_flight_clear(app, arg(a, "project")?)),
        "briefs_waiting" => out(crate::pr_briefs::briefs_waiting(app, arg(a, "project")?)),
        "pr_ratify" => res(crate::ratify::pr_ratify(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
        )),
        "pr_map_review" => res(crate::map_review::pr_map_review(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
        )),
        "map_review_start" => res(crate::map_review::map_review_start(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
        )),
        "pr_map_diff" => res(crate::ratify::pr_map_diff(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
        )),
        "ratify_set" => res(crate::ratify::ratify_set(
            arg(a, "project")?,
            arg(a, "pr")?,
            arg(a, "kind")?,
            arg(a, "id")?,
            arg(a, "state")?,
            arg(a, "text")?,
        )),
        "ratify_site" => res(crate::ratify::ratify_site(
            arg(a, "project")?,
            arg(a, "head")?,
            arg(a, "file")?,
            arg(a, "line")?,
        )),
        "failed_check_name" => out(crate::ci_laws::failed_check_name(
            arg(a, "tail")?,
            arg(a, "suite")?,
            arg(a, "worktree")?,
        )),
        "map_marks" => res(crate::map_marks(app, arg(a, "projects")?)),
        "map_proof" => out(crate::map::map_proof(arg(a, "project")?, arg(a, "law")?)),
        "map_save_positions" => res(crate::map::map_save_positions(
            arg(a, "project")?,
            arg(a, "positions")?,
        )),
        "map_save_view" => res(crate::map::map_save_view(
            arg(a, "project")?,
            arg(a, "view")?,
        )),
        "map_set_tier" => res(crate::map::map_set_tier(
            arg(a, "project")?,
            arg(a, "part")?,
            arg(a, "tier")?,
        )),
        "map_place" => out(crate::map::map_place(arg(a, "project")?, arg(a, "groups")?)),
        "micro_world_explore" => res(crate::micro_worlds::micro_world_explore(
            app,
            arg(a, "project")?,
            arg(a, "subject")?,
        )),
        "micro_world_page" => res(crate::micro_worlds::micro_world_page(
            app,
            arg(a, "project")?,
            arg(a, "key")?,
        )),
        "run_cancel" => res(app.runner.cancel(&arg::<String>(a, "id")?)),
        "runs_resume" => done!(app.runner.resume()),
        "back_a_law" => res(app.laws.start_for(
            &arg::<String>(a, "project")?,
            &arg::<String>(a, "law")?,
            arg(a, "guards")?,
        )),
        "new_law" => res(app.laws.start_new(
            &arg::<String>(a, "project")?,
            &arg(a, "law")?,
            arg(a, "guards")?,
        )),
        "guard_draft" => res(crate::pr_briefs::guard_draft(
            app,
            arg(a, "project")?,
            arg(a, "number")?,
        )),
        "law_patch_open" => res(app
            .laws
            .open_pr(&arg::<String>(a, "project")?, &arg::<String>(a, "id")?)),
        "law_patch_discard" => res(app
            .laws
            .discard(&arg::<String>(a, "project")?, &arg::<String>(a, "id")?)),
        "stop_log" => res(stop_rules::stop_log(
            arg(a, "kind")?,
            arg(a, "project")?,
            arg(a, "day")?,
            arg(a, "pr")?,
        )),
        "stop_facts" => res(stop_rules::stop_facts(arg(a, "project")?)),
        "story_bar" => res(crate::story_bar::story_bar(
            arg(a, "kind")?,
            arg(a, "project")?,
            arg(a, "day")?,
            arg(a, "about")?,
            crate::story_bar::Given {
                line: arg(a, "line")?,
                mark: arg(a, "mark")?,
            },
        )),
        "stop_dismiss" => res(stop_rules::stop_dismiss(
            arg(a, "projects")?,
            arg(a, "modules")?,
            arg(a, "day")?,
        )),
        "watch" => res(server_answer::watch(arg(a, "session")?, &app.watching)),
        "alive" => done!(server_answer::alive(&app.watching)),
        "answer" => res(server_answer::answer(
            arg(a, "session")?,
            arg(a, "nonce")?,
            arg(a, "answers")?,
            arg(a, "decision")?,
        )),
        "projects" => out(app.projects.list()),
        "register_folder" => res(crate::register_folder(app, &arg::<String>(a, "folder")?)),
        "remove_project" => res(app
            .projects
            .remove(&arg::<String>(a, "path")?, studio_understanding::millis())),
        "settings_files" => out(crate::settings::settings_files()),
        "set_explanations" => res(crate::settings::save_explanations(
            arg(a, "expected")?,
            arg(a, "on")?,
        )),
        "refresh_landed_tasks" => done!(crate::refresh_landed_tasks(arc)),
        "page_focus" => {
            page.focus(arg(a, "focused")?);
            out(())
        }
        "log" => {
            let line: String = arg(a, "line")?;
            match arg::<String>(a, "level")?.as_str() {
                "error" => log::error!(target: crate::PAGE_TARGET, "page: {line}"),
                _ => log::warn!(target: crate::PAGE_TARGET, "page: {line}"),
            }
            out(())
        }
        _ => Err(Failure::Unknown),
    }
}
