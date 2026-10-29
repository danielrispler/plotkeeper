// The app: a window of xterm.js tabs. Daemon tabs talk to the background process; tmux, herdr and plain local tabs
// run a PTY inside this process (for tmux and herdr, their attach client), which dies with the app.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use portable_pty::PtySize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    os::unix::{net::UnixStream, process::CommandExt},
    process::{Command, Stdio},
    sync::Mutex,
};
use tauri::{AppHandle, Emitter, State};

#[derive(Default)]
struct App {
    daemon: Mutex<Option<UnixStream>>,
    local: Mutex<HashMap<String, crate::Pty>>,
}

fn connect_daemon() -> Result<UnixStream, String> {
    let path = crate::daemon::socket_path();
    if let Ok(s) = UnixStream::connect(&path) {
        return Ok(s);
    }
    std::fs::create_dir_all(crate::daemon::dir()).map_err(|e| e.to_string())?;
    let log = std::fs::OpenOptions::new().create(true).append(true).open(crate::daemon::dir().join("daemon.log"));
    let log = log.map_err(|e| e.to_string())?;
    let mut cmd = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    cmd.arg("--daemon").stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log);
    // Its own session: no SIGHUP or Ctrl-C from the terminal that ran `cargo run`, and no tie to the app's life.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    for _ in 0..50 {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Ok(s) = UnixStream::connect(&path) {
            return Ok(s);
        }
    }
    Err("the daemon did not start; see ~/.studio-terminal-spike/daemon.log".into())
}

#[tauri::command]
fn config() -> Value {
    let spike = env!("CARGO_MANIFEST_DIR");
    json!({
        "shell": std::env::var("SHELL").unwrap_or("/bin/zsh".into()),
        "cwd": std::env::var("SPIKE_CWD").unwrap_or(std::env::current_dir().unwrap().display().to_string()),
        "plugin": format!("{spike}/hook"),
        "tmux_conf": format!("{spike}/tmux.conf"),
        "app_pid": std::process::id(),
        "autospawn": std::env::var("SPIKE_AUTOSPAWN").unwrap_or_default(),
    })
}

// Connects (starting the daemon if needed) and forwards everything it sends as `daemon` events.
#[tauri::command]
fn daemon_connect(app: AppHandle, st: State<App>) -> Result<(), String> {
    let stream = connect_daemon()?;
    let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    *st.daemon.lock().unwrap() = Some(stream);
    std::thread::spawn(move || {
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                let _ = app.emit("daemon", v);
            }
        }
        let _ = app.emit("daemon", json!({ "ev": "lost" }));
    });
    Ok(())
}

#[tauri::command]
fn daemon_send(msg: Value, st: State<App>) -> Result<(), String> {
    let mut d = st.daemon.lock().unwrap();
    let s = d.as_mut().ok_or("not connected")?;
    writeln!(s, "{msg}").map_err(|e| e.to_string())
}

#[tauri::command]
fn local_spawn(
    app: AppHandle,
    st: State<App>,
    id: String,
    argv: Vec<String>,
    env: HashMap<String, String>,
    cwd: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let (a1, a2, id1, id2) = (app.clone(), app, id.clone(), id.clone());
    let env: Vec<_> = env.into_iter().collect();
    let pty = crate::spawn(
        &argv,
        &cwd,
        &env,
        cols,
        rows,
        move |b| drop(a1.emit("local", json!({ "ev": "output", "id": id1, "data": B64.encode(b) }))),
        move |status| drop(a2.emit("local", json!({ "ev": "exit", "id": id2, "status": status }))),
    )?;
    st.local.lock().unwrap().insert(id, pty);
    Ok(())
}

#[tauri::command]
fn local_input(id: String, data: String, st: State<App>) {
    if let Some(p) = st.local.lock().unwrap().get_mut(&id) {
        let _ = p.writer.write_all(&B64.decode(data).unwrap_or_default());
    }
}

#[tauri::command]
fn local_resize(id: String, cols: u16, rows: u16, st: State<App>) {
    if let Some(p) = st.local.lock().unwrap().get(&id) {
        let _ = p.master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
    }
}

#[tauri::command]
fn local_kill(id: String, st: State<App>) {
    if let Some(mut p) = st.local.lock().unwrap().remove(&id) {
        let _ = p.killer.kill();
    }
}

// shortcut: runs any program the page asks for (tmux and herdr queries); fine for a local throwaway, never ship it.
#[tauri::command]
fn exec(argv: Vec<String>) -> String {
    match Command::new(&argv[0]).args(&argv[1..]).output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned() + &String::from_utf8_lossy(&o.stderr),
        Err(e) => e.to_string(),
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(App::default())
        .invoke_handler(tauri::generate_handler![
            config,
            daemon_connect,
            daemon_send,
            local_spawn,
            local_input,
            local_resize,
            local_kill,
            exec
        ])
        .run(tauri::generate_context!())
        .expect("run the app");
}
