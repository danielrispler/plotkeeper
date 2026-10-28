// v0 for #315: the guess-and-card loop.
// Tails ../hook/events.jsonl (written by event-hook.sh). At most one guess per wait, which a new prompt resets:
// the first Bash, Edit/Write or ExitPlanMode opens it, and ExitPlanMode replaces a guess not yet submitted.
// Edits and Writes to plan files (any path containing /.claude/plans/) never open one.
// The card is written once the guess is in and the tool's PostToolUse or PostToolUseFailure result has arrived.
// CARD_WRITER=claude uses `claude -p`; anything else shows a fixed fake card.
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{ActivationPolicy, AppHandle, Emitter, Manager, State};
use tauri_nspanel::{tauri_panel, ManagerExt, WebviewWindowExt};

tauri_panel! {
    panel!(GuessPanel {
        config: {
            can_become_key_window: true,
            is_floating_panel: true
        }
    })
}

#[derive(Default)]
struct Wait {
    done: bool,              // a card was already written this wait
    tool: Option<Value>,     // the PreToolUse payload the guess is about
    guess: Option<String>,
    result: Option<Value>,   // its PostToolUse tool_response
}

type Shared = Arc<Mutex<Wait>>;

fn eligible(ev: &Value) -> bool {
    let plan_file = ev["tool_input"]["file_path"].as_str().is_some_and(|p| p.contains("/.claude/plans/"));
    matches!(ev["tool_name"].as_str(), Some("Bash" | "Edit" | "Write" | "ExitPlanMode")) && !plan_file
}

fn file_name(input: &Value) -> String {
    let p = input["file_path"].as_str().unwrap_or("?");
    p.rsplit('/').next().unwrap_or(p).to_string()
}

// The context Daniel reasons from, and a question about the outcome, which is unknown even if the command shows.
fn question(tool: &Value) -> Value {
    let input = &tool["tool_input"];
    let (context, q) = match tool["tool_name"].as_str().unwrap_or("") {
        "Bash" => {
            let cmd = input["command"].as_str().unwrap_or("");
            let q = if cmd.contains("test") || cmd.contains("check") {
                "האם הבדיקות יעברו? אם לא, מה ייכשל?"
            } else {
                "מה הפקודה תראה או תשנה?"
            };
            (cmd.lines().next().unwrap_or("").chars().take(120).collect(), q.to_string())
        }
        "Edit" => (file_name(input), format!("מה ישתנה ב-{}?", file_name(input))),
        "Write" => (file_name(input), format!("מה יהיה בתוך {}?", file_name(input))),
        _ => {
            let plan = input["plan"].as_str().unwrap_or("");
            (plan.lines().next().unwrap_or("").trim_start_matches('#').trim().to_string(),
             "באיזה קובץ התוכנית תיגע ראשון, ומה תשנה בו?".to_string())
        }
    };
    json!({ "context": context, "question": q })
}

fn fake_card(tool: &Value) -> Value {
    let ctx = question(tool)["context"].clone();
    json!({
        "title": "כרטיס לדוגמה: הסוכן שינה תנאי אחד",
        "code": [
            { "line": format!("// {}", ctx.as_str().unwrap_or("")), "label": "" },
            { "line": "if token.expires_at <= now {", "label": "היה <, עכשיו <=" },
            { "line": "    return Err(Expired);", "label": "" },
            { "line": "}", "label": "" }
        ],
        "unchecked": "לא נבדק: מה קורה כשהשעון של השרת לא מסונכרן."
    })
}

fn claude_card(tool: &Value, guess: &str, result: &Value) -> Value {
    let prompt = format!(
        "You write one short explanation card for a developer who just guessed the outcome of an agent's step.\n\
         Rules: Hebrew, plain words; code terms and code stay in English. One idea only, at most four new things. \
         Put explanations as short labels on the code lines themselves. No praise, no score, do not grade the guess. \
         Name one thing that was not checked.\n\
         Reply with JSON only: {{\"title\": str, \"code\": [{{\"line\": str, \"label\": str}}] (at most 8 lines), \"unchecked\": str}}.\n\n\
         Tool call: {}\nResult: {}\nDeveloper's guess: {}",
        tool["tool_input"], result, guess
    );
    // without the API-key variables and without --bare, `claude` uses the subscription login
    let out = Command::new("claude")
        .args(["-p", &prompt])
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .output();
    let (text, err) = match out {
        Ok(o) => (String::from_utf8_lossy(&o.stdout).into_owned(), String::from_utf8_lossy(&o.stderr).into_owned()),
        Err(e) => (String::new(), e.to_string()),
    };
    // the model may wrap the JSON in a fence; take the outermost braces
    let body = text.find('{').zip(text.rfind('}')).map(|(a, b)| &text[a..=b]).unwrap_or("");
    serde_json::from_str(body).unwrap_or_else(|_| {
        json!({ "title": "כותב הכרטיסים נכשל", "code": [{ "line": format!("{text}{err}"), "label": "" }], "unchecked": "" })
    })
}

fn show(app: &AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || h.get_webview_panel("panel").unwrap().show());
}

// Writes the card once both the guess and the result are in.
fn try_card(app: &AppHandle, wait: &Shared) {
    let mut w = wait.lock().unwrap();
    let (Some(tool), Some(guess), Some(result)) = (w.tool.clone(), w.guess.clone(), w.result.clone()) else { return };
    w.done = true;
    w.tool = None;
    drop(w);
    let app = app.clone();
    thread::spawn(move || {
        let card = if std::env::var("CARD_WRITER").as_deref() == Ok("claude") {
            claude_card(&tool, &guess, &result)
        } else {
            fake_card(&tool)
        };
        eprintln!("card {card}");
        let _ = app.emit("card", card);
        show(&app);
    });
}

fn on_event(app: &AppHandle, wait: &Shared, ev: Value) {
    let tool = ev["tool_name"].as_str().unwrap_or("");
    match ev["hook_event_name"].as_str().unwrap_or("") {
        "UserPromptSubmit" => *wait.lock().unwrap() = Wait::default(),
        "PreToolUse" if eligible(&ev) => {
            let mut w = wait.lock().unwrap();
            let pending = w.tool.as_ref().map(|t| t["tool_name"] == "ExitPlanMode");
            let open = !w.done && w.guess.is_none() && (pending.is_none() || (pending == Some(false) && tool == "ExitPlanMode"));
            if open {
                w.tool = Some(ev.clone());
                w.result = None;
                drop(w);
                eprintln!("guess {}", question(&ev));
                let _ = app.emit("guess", question(&ev));
                show(app);
            }
        }
        // a failed tool (a Bash exit code other than 0, a rejected Edit) fires PostToolUseFailure with `error`
        "PostToolUse" | "PostToolUseFailure" => {
            let mut w = wait.lock().unwrap();
            if w.tool.as_ref().is_some_and(|t| t["tool_use_id"] == ev["tool_use_id"]) {
                w.result = Some(if ev["error"].is_null() { ev["tool_response"].clone() } else { json!({ "error": ev["error"] }) });
                drop(w);
                try_card(app, wait);
            }
        }
        _ => {}
    }
}

#[tauri::command]
fn submit_guess(app: AppHandle, wait: State<Shared>, guess: String) {
    wait.lock().unwrap().guess = Some(guess);
    try_card(&app, &wait);
}

#[tauri::command]
fn hide(app: AppHandle) {
    let h = app.clone();
    let _ = app.run_on_main_thread(move || h.get_webview_panel("panel").unwrap().hide());
}

fn main() {
    let events = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../hook/events.jsonl");
    let wait: Shared = Arc::default();
    let tail_wait = wait.clone();
    tauri::Builder::default()
        .plugin(tauri_nspanel::init())
        .manage(wait)
        .invoke_handler(tauri::generate_handler![submit_guess, hide])
        .setup(move |app| {
            app.set_activation_policy(ActivationPolicy::Accessory);
            let panel = app.get_webview_window("panel").unwrap().to_panel::<GuessPanel>()?;
            panel
                .add_style_mask(tauri_nspanel::objc2_app_kit::NSWindowStyleMask::NonactivatingPanel)
                .expect("nonactivating");
            let handle = app.handle().clone();
            // shortcut: polls the file every 200 ms from its end at startup; fine for one user's waits.
            thread::spawn(move || {
                let mut pos = std::fs::metadata(&events).map(|m| m.len()).unwrap_or(0);
                loop {
                    if let Ok(mut f) = std::fs::File::open(&events) {
                        if f.seek(SeekFrom::Start(pos)).is_ok() {
                            let mut r = BufReader::new(f);
                            let mut line = String::new();
                            while r.read_line(&mut line).unwrap_or(0) > 0 && line.ends_with('\n') {
                                pos += line.len() as u64;
                                if let Ok(ev) = serde_json::from_str(&line) {
                                    on_event(&handle, &tail_wait, ev);
                                }
                                line.clear();
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run");
}
