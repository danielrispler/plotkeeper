// The background terminal process, Orca's model. It owns the PTYs, keeps each terminal's raw output as scrollback,
// and serves the app over a Unix socket in JSON lines. The app starts it in its own session (setsid), so quitting,
// killing or rebuilding the app leaves it and its terminals running.
//
// Requests: list | spawn {id, argv, cwd, cols, rows} | attach {id} | input {id, data} | resize {id, cols, rows}
//           | kill {id} | forget {id}. Data is base64.
// Events:   list {daemon_pid, terms} | replay {id, data, cols, rows, exited} | output {id, data} | exit {id, status}.
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use portable_pty::PtySize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::{BufRead, BufReader, Write},
    os::unix::net::{UnixListener, UnixStream},
    path::PathBuf,
    sync::{Arc, Mutex},
};

// shortcut: raw bytes, cut at a fixed size, replay garbled if the cut lands mid-sequence or after a screen switch;
// upgrade to a headless emulator snapshot if reattach shows it.
const SCROLLBACK: usize = 4 << 20;

struct Term {
    pty: crate::Pty,
    argv: Vec<String>,
    buf: VecDeque<u8>,
    cols: u16,
    rows: u16,
    exited: Option<String>,
}

struct Client {
    stream: UnixStream,
    attached: HashSet<String>,
}

#[derive(Default)]
struct State {
    terms: HashMap<String, Term>,
    clients: HashMap<usize, Client>,
}

type Shared = Arc<Mutex<State>>;

pub fn dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap()).join(".studio-terminal-spike")
}

pub fn socket_path() -> PathBuf {
    dir().join("daemon.sock")
}

pub fn run() {
    let path = socket_path();
    if UnixStream::connect(&path).is_ok() {
        eprintln!("daemon already running");
        return;
    }
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind the daemon socket");
    eprintln!("daemon {} listening on {}", std::process::id(), path.display());
    let state = Shared::default();
    for (n, stream) in listener.incoming().enumerate() {
        let (Ok(stream), state) = (stream, state.clone()) else { continue };
        std::thread::spawn(move || serve(n, stream, state));
    }
}

// Writes to every client attached to `id`, and drops clients whose socket is gone (the app quit or crashed).
fn send(clients: &mut HashMap<usize, Client>, id: &str, msg: &Value) {
    let line = format!("{msg}\n");
    clients.retain(|_, c| !c.attached.contains(id) || c.stream.write_all(line.as_bytes()).is_ok());
}

fn output(state: &Shared, id: &str, bytes: &[u8]) {
    let s = &mut *state.lock().unwrap();
    let Some(t) = s.terms.get_mut(id) else { return };
    t.buf.extend(bytes);
    let excess = t.buf.len().saturating_sub(SCROLLBACK);
    t.buf.drain(..excess);
    send(&mut s.clients, id, &json!({ "ev": "output", "id": id, "data": B64.encode(bytes) }));
}

fn exited(state: &Shared, id: &str, status: String) {
    eprintln!("{id} exited: {status}");
    let s = &mut *state.lock().unwrap();
    if let Some(t) = s.terms.get_mut(id) {
        t.exited = Some(status.clone());
    }
    send(&mut s.clients, id, &json!({ "ev": "exit", "id": id, "status": status }));
}

fn serve(n: usize, stream: UnixStream, state: Shared) {
    let mut out = stream.try_clone().unwrap();
    let client = Client { stream: stream.try_clone().unwrap(), attached: HashSet::new() };
    state.lock().unwrap().clients.insert(n, client);
    eprintln!("client {n} connected");
    for line in BufReader::new(stream).lines() {
        let Ok(req) = line.map(|l| serde_json::from_str::<Value>(&l).unwrap_or_default()) else { break };
        let (op, id) = (req["op"].as_str().unwrap_or(""), req["id"].as_str().unwrap_or("").to_string());
        if op != "input" {
            eprintln!("client {n}: {op} {id}");
        }
        // Every write happens under this lock, so a replay and the live output after it never interleave.
        let s = &mut *state.lock().unwrap();
        match op {
            "list" => {
                let terms: Vec<Value> = s
                    .terms
                    .iter()
                    .map(|(id, t)| {
                        json!({ "id": id, "argv": t.argv, "pid": t.pty.pid, "exited": t.exited,
                                "foreground": crate::foreground(&t.pty), "cols": t.cols, "rows": t.rows,
                                "scrollback_bytes": t.buf.len() })
                    })
                    .collect();
                let _ = writeln!(out, "{}", json!({ "ev": "list", "daemon_pid": std::process::id(), "terms": terms }));
            }
            "spawn" => {
                let argv: Vec<String> = serde_json::from_value(req["argv"].clone()).unwrap_or_default();
                let (cols, rows) = (req["cols"].as_u64().unwrap_or(80) as u16, req["rows"].as_u64().unwrap_or(24) as u16);
                let env = [("STUDIO_TERMINAL_ID".to_string(), id.clone())];
                let (st1, st2, id1, id2) = (state.clone(), state.clone(), id.clone(), id.clone());
                let cwd = req["cwd"].as_str().unwrap_or("/");
                match crate::spawn(&argv, cwd, &env, cols, rows, move |b| output(&st1, &id1, b), move |x| exited(&st2, &id2, x)) {
                    Ok(pty) => {
                        s.terms.insert(id, Term { pty, argv, buf: VecDeque::new(), cols, rows, exited: None });
                    }
                    Err(e) => eprintln!("spawn {id} failed: {e}"),
                }
            }
            "attach" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    let data = B64.encode(t.buf.make_contiguous());
                    let msg = json!({ "ev": "replay", "id": id, "data": data, "cols": t.cols, "rows": t.rows, "exited": t.exited });
                    let _ = writeln!(out, "{msg}");
                    if let Some(c) = s.clients.get_mut(&n) {
                        c.attached.insert(id);
                    }
                }
            }
            "input" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    let _ = t.pty.writer.write_all(&B64.decode(req["data"].as_str().unwrap_or("")).unwrap_or_default());
                }
            }
            "resize" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    (t.cols, t.rows) = (req["cols"].as_u64().unwrap_or(80) as u16, req["rows"].as_u64().unwrap_or(24) as u16);
                    let _ = t.pty.master.resize(PtySize { rows: t.rows, cols: t.cols, pixel_width: 0, pixel_height: 0 });
                }
            }
            "kill" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    let _ = t.pty.killer.kill();
                }
            }
            "forget" => {
                s.terms.remove(&id);
            }
            _ => {}
        }
    }
    state.lock().unwrap().clients.remove(&n);
    eprintln!("client {n} gone");
}
