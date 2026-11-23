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
        "writer_notice" => out(app.writer_notice.lock().unwrap().clone()),
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
        "pr_diff" => res(crate::map::pr_diff(
            arg(a, "project")?,
            arg(a, "number")?,
            arg(a, "head")?,
            arg(a, "title")?,
        )),
        "pr_brief" => res(crate::pr_briefs::pr_brief(
            arc,
            arg(a, "project")?,
            arg(a, "number")?,
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
        "briefs_waiting" => out(crate::pr_briefs::briefs_waiting(app, arg(a, "cwd")?)),
        "pr_ratify" => res(crate::ratify::pr_ratify(
            &app.prs,
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
        "run_cancel" => res(app.runner.cancel(&arg::<String>(a, "id")?)),
        "runs_resume" => done!(app.runner.resume()),
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
            arg(a, "rating")?,
            arg(a, "note")?,
        )),
        "stop_dismiss" => res(stop_rules::stop_dismiss(
            arg(a, "projects")?,
            arg(a, "modules")?,
            arg(a, "day")?,
        )),
        "answer_blocked_command" => res(server_answer::answer_blocked_command(
            arg(a, "session")?,
            arg(a, "nonce")?,
            arg(a, "decision")?,
        )),
        "ask_again" => res(server_answer::ask_again(
            arg(a, "session")?,
            arg(a, "command")?,
            arg(a, "cwd")?,
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
        "remove_project" => res(app
            .projects
            .remove(&arg::<String>(a, "path")?, studio_understanding::millis())),
        "settings_files" => out(crate::settings::settings_files()),
        "refresh_landed_tasks" => done!(crate::refresh_landed_tasks(arc)),
        "page_focus" => {
            page.focus(arg(a, "focused")?);
            out(())
        }
        "log" => {
            let line: String = arg(a, "line")?;
            match arg::<String>(a, "level")?.as_str() {
                "error" => log::error!("page: {line}"),
                _ => log::warn!("page: {line}"),
            }
            out(())
        }
        _ => Err(Failure::Unknown),
    }
}
