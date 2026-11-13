// The daemon itself, built from the danielrispler/Falinks#322 spike (`terminal-spike/src/daemon.rs`). It owns the PTYs, keeps each
// terminal's raw output as scrollback, and serves clients over the socket (protocol in lib.rs).
use crate::{PROTOCOL, decode, encode, status::Status};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::Permissions,
    io::{BufRead, BufReader, Read, Write},
    net::Shutdown,
    os::{
        fd::AsRawFd,
        unix::fs::PermissionsExt,
        unix::net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{SyncSender, TrySendError, sync_channel},
    },
};

// shortcut: raw bytes, cut at a fixed size, replay garbled if the cut lands mid-sequence or after a screen switch;
// upgrade to a headless emulator snapshot if reattach shows it.
pub const SCROLLBACK: usize = 4 << 20;

// Variables that leak in from whatever started the app (Ghostty, tmux, a Claude Code Bash tool) are dropped. Terminal
// ones make programs assume the wrong terminal. Claude Code's mark a nested session (transcripts off) and carry its
// session id and messaging token. Terminals start as login shells, so anything set in Daniel's profile comes back.
const SCRUB: &[&str] = &[
    "TERM_PROGRAM",
    "TERM_SESSION_ID",
    "LC_TERMINAL",
    "ITERM_",
    "GHOSTTY_",
    "KITTY_",
    "WEZTERM_",
    "ALACRITTY_",
    "ZED_",
    "VSCODE_",
    "TMUX",
    "HERDR_",
    "STUDIO_",
    "CLAUDE",
];

struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    pid: Option<u32>,
}

struct Term {
    pty: Pty,
    argv: Vec<String>,
    /// Where it was spawned; `Term::cwd` gives where it is now.
    cwd: String,
    buf: VecDeque<u8>,
    cols: u16,
    rows: u16,
    exited: Option<String>,
    status: Status,
    /// The program in its foreground as `list` last saw it, for the record (danielrispler/Falinks#561).
    foreground: Option<String>,
}

impl Term {
    // The folder its leader is in now, such as where a shell `cd`-ed to, so a revived shell starts there (danielrispler/Falinks#470); the
    // spawn's folder once it exited, as its pid may name another process by then.
    fn cwd(&self) -> String {
        let pid = self.pty.pid.filter(|_| self.exited.is_none());
        pid.and_then(current_dir)
            .unwrap_or_else(|| self.cwd.clone())
    }
}

// Process `pid`'s current folder, from the kernel.
fn current_dir(pid: u32) -> Option<String> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of_val(&info) as i32;
    let flavor = libc::PROC_PIDVNODEPATHINFO;
    let read = unsafe { libc::proc_pidinfo(pid as i32, flavor, 0, (&raw mut info).cast(), size) };
    if read != size {
        return None;
    }
    let path = unsafe { std::ffi::CStr::from_ptr(info.pvi_cdir.vip_path.as_ptr().cast()) };
    Some(path.to_string_lossy().into_owned()).filter(|p| !p.is_empty())
}

// A client further behind than this many messages (each `output` at most 16 KB) is dropped, so one that stops
// reading never holds up the others.
const BACKLOG: usize = 1024;

// Nothing writes to a client's socket under the state lock: its messages go to a queue its own thread writes out.
struct Conn {
    queue: SyncSender<String>,
    stream: UnixStream,
    attached: HashSet<String>,
    /// Whether it was dropped for falling behind, not gone: an app's window may still be focused.
    lagged: Arc<AtomicBool>,
}

impl Conn {
    // Queues `msg`; false when the client is gone or too far behind, and then closes its socket, which ends its
    // reading thread.
    fn send(&self, msg: &Value) -> bool {
        let sent = self.queue.try_send(format!("{msg}\n"));
        if let Err(TrySendError::Full(_)) = sent {
            self.lagged.store(true, Ordering::Relaxed);
        }
        let sent = sent.is_ok();
        if !sent {
            let _ = self.stream.shutdown(Shutdown::Both);
        }
        sent
    }
}

#[derive(Default)]
struct State {
    terms: HashMap<String, Term>,
    conns: HashMap<usize, Conn>,
    /// Where it records the terminals it runs (`crate::record`).
    record: Arc<Record>,
    /// The studio's presence file (`crate::presence`).
    presence: PathBuf,
}

impl State {
    // Sends `msg` to client `n`, and drops it when it is gone or too far behind.
    fn reply(&mut self, n: usize, msg: &Value) {
        if self.conns.get(&n).is_some_and(|c| !c.send(msg)) {
            self.conns.remove(&n);
        }
    }

    // Takes the record's newest copy under the lock; the caller writes it with `Record::write` after releasing it.
    fn save(&self) -> Arc<Record> {
        let running = self.terms.iter().filter(|(_, t)| t.exited.is_none());
        let terms: Vec<Value> = running
            .map(|(id, t)| {
                json!({ "id": id, "argv": t.argv, "cwd": t.cwd(), "foreground": t.foreground,
                                       "app": t.status.app() })
            })
            .collect();
        *self.record.newest.lock().unwrap() = Some(json!(terms).to_string());
        self.record.clone()
    }
}

// The record of the terminals (`crate::record`), written outside the state lock so output never waits on disk
// (danielrispler/Falinks#555, row 6). Only the newest copy is written: a writer takes whatever is newest when its turn comes.
#[derive(Default)]
struct Record {
    path: PathBuf,
    newest: Mutex<Option<String>>,
    // Held while writing, so copies reach the disk in the order they were taken.
    writing: Mutex<()>,
}

impl Record {
    fn write(&self) {
        let _writing = self.writing.lock().unwrap();
        let Some(text) = self.newest.lock().unwrap().take() else {
            return;
        };
        // Written aside and renamed, so a crash mid-write leaves the last whole record.
        let aside = self.path.with_extension("tmp");
        let saved = std::fs::write(&aside, text).and_then(|()| std::fs::rename(&aside, &self.path));
        if let Err(e) = saved {
            eprintln!("record the terminals: {e}");
        }
    }
}

type Shared = Arc<Mutex<State>>;

pub fn serve(socket: &Path) -> Result<(), String> {
    // Its own session, so no hangup or Ctrl-C from whatever started it reaches it.
    unsafe {
        libc::setsid();
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }
    // Only Daniel's account may reach the socket (danielrispler/Falinks#555): the folder is 0700, also when it already existed.
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::set_permissions(dir, Permissions::from_mode(0o700)))
            .map_err(|e| e.to_string())?;
    }
    // One daemon at a time: the lock is held for the daemon's whole life and freed by the kernel when it dies.
    let lock = std::fs::File::create(socket.with_extension("lock")).map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(format!("a daemon already serves {}", socket.display()));
    }
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)
        .and_then(|l| std::fs::set_permissions(socket, Permissions::from_mode(0o600)).map(|()| l))
        .map_err(|e| e.to_string())?;
    eprintln!(
        "daemon {} (protocol {PROTOCOL}) listening on {}",
        std::process::id(),
        socket.display()
    );
    let state = Shared::new(Mutex::new(State {
        record: Arc::new(Record {
            path: crate::record(socket),
            ..Record::default()
        }),
        presence: crate::presence(socket),
        ..State::default()
    }));
    for (n, stream) in listener.incoming().enumerate() {
        let (Ok(stream), state) = (stream, state.clone()) else {
            continue;
        };
        std::thread::spawn(move || serve_conn(n, stream, state));
    }
    drop(lock);
    Ok(())
}

// Sends to every client attached to `id`, and drops clients that are gone (the app quit or crashed) or too far
// behind.
fn broadcast(conns: &mut HashMap<usize, Conn>, id: &str, msg: &Value) {
    conns.retain(|_, c| !c.attached.contains(id) || c.send(msg));
}

fn error(id: &str, message: &str) -> Value {
    json!({ "ev": "error", "id": id, "message": message })
}

fn on_output(state: &Shared, id: &str, bytes: &[u8]) {
    let s = &mut *state.lock().unwrap();
    let Some(t) = s.terms.get_mut(id) else { return };
    let replies = t.status.read(bytes);
    if !replies.is_empty() {
        let _ = t.pty.writer.write_all(&replies);
    }
    t.buf.extend(bytes);
    let excess = t.buf.len().saturating_sub(SCROLLBACK);
    t.buf.drain(..excess);
    broadcast(
        &mut s.conns,
        id,
        &json!({ "ev": "output", "id": id, "data": encode(bytes) }),
    );
}

fn on_exit(state: &Shared, id: &str, status: String) {
    eprintln!("{id} exited: {status}");
    let record = {
        let s = &mut *state.lock().unwrap();
        if let Some(t) = s.terms.get_mut(id) {
            t.exited = Some(status.clone());
            t.status.exit();
        }
        broadcast(
            &mut s.conns,
            id,
            &json!({ "ev": "exit", "id": id, "status": status }),
        );
        s.save()
    };
    record.write();
}

fn serve_conn(n: usize, stream: UnixStream, state: Shared) {
    let (Ok(mut out), Ok(shut)) = (stream.try_clone(), stream.try_clone()) else {
        return;
    };
    let (queue, lines) = sync_channel::<String>(BACKLOG);
    std::thread::spawn(move || {
        for line in lines {
            if out.write_all(line.as_bytes()).is_err() {
                break;
            }
        }
    });
    let lagged = Arc::new(AtomicBool::new(false));
    let conn = Conn {
        queue,
        stream: shut,
        attached: HashSet::new(),
        lagged: lagged.clone(),
    };
    conn.send(&json!({ "ev": "hello", "protocol": PROTOCOL, "daemon_pid": std::process::id() }));
    state.lock().unwrap().conns.insert(n, conn);
    let mut app = false;
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let req: Value = serde_json::from_str(&line).unwrap_or_default();
        let (op, id) = (
            req["op"].as_str().unwrap_or(""),
            req["id"].as_str().unwrap_or("").to_string(),
        );
        let size = |d: (u64, u64)| {
            (
                req["cols"].as_u64().unwrap_or(d.0) as u16,
                req["rows"].as_u64().unwrap_or(d.1) as u16,
            )
        };
        if op == "app" {
            app = true;
            continue;
        }
        if op == "list" {
            // Outside the lock: `ps` is slow, and output must keep flowing meanwhile.
            let msg = list(&state);
            state.lock().unwrap().reply(n, &msg);
            continue;
        }
        // Every message is queued under this lock, so a replay and the live output after it never interleave.
        let mut guard = state.lock().unwrap();
        let s = &mut *guard;
        let mut saved = None;
        match op {
            "spawn" if s.terms.contains_key(&id) => s.reply(n, &error(&id, "the id is taken")),
            "spawn" => {
                let argv: Vec<String> =
                    serde_json::from_value(req["argv"].clone()).unwrap_or_default();
                let cwd = req["cwd"].as_str().unwrap_or("/").to_string();
                let (cols, rows) = size((80, 24));
                let (st1, st2, id1, id2) = (state.clone(), state.clone(), id.clone(), id.clone());
                let started = spawn(
                    &argv,
                    &cwd,
                    &id,
                    cols,
                    rows,
                    move |b| on_output(&st1, &id1, b),
                    move |x| on_exit(&st2, &id2, x),
                );
                let reply = match started {
                    Ok(pty) => {
                        s.terms.insert(
                            id.clone(),
                            Term {
                                pty,
                                argv,
                                cwd,
                                buf: VecDeque::new(),
                                cols,
                                rows,
                                exited: None,
                                status: Status::default(),
                                foreground: None,
                            },
                        );
                        saved = Some(s.save());
                        json!({ "ev": "spawned", "id": id })
                    }
                    Err(message) => error(&id, &message),
                };
                s.reply(n, &reply);
            }
            "attach" => match s.terms.get_mut(&id) {
                Some(t) => {
                    let msg = json!({ "ev": "replay", "id": id, "data": encode(t.buf.make_contiguous()),
                                      "cols": t.cols, "rows": t.rows, "exited": t.exited });
                    s.reply(n, &msg);
                    if let Some(c) = s.conns.get_mut(&n) {
                        c.attached.insert(id);
                    }
                }
                None => s.reply(n, &error(&id, "no such terminal")),
            },
            "input" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    let _ = t.pty.writer.write_all(&decode(&req["data"]));
                }
            }
            "resize" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    (t.cols, t.rows) = size((t.cols as u64, t.rows as u64));
                    let _ = t.pty.master.resize(PtySize {
                        rows: t.rows,
                        cols: t.cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
            }
            "kill" => {
                if let Some(t) = s.terms.get_mut(&id) {
                    kill(t);
                }
            }
            "forget" => {
                if let Some(mut t) = s.terms.remove(&id) {
                    kill(&mut t);
                    saved = Some(s.save());
                }
            }
            _ => {}
        }
        drop(guard);
        if let Some(record) = saved {
            record.write();
        }
    }
    let s = &mut *state.lock().unwrap();
    s.conns.remove(&n);
    // The app quit or crashed, so nobody is looking at the studio (danielrispler/Falinks#555). An app dropped for lag
    // reconnects, and its window may still be focused.
    if app && !lagged.load(Ordering::Relaxed) {
        let _ = std::fs::remove_file(&s.presence);
    }
}

fn spawn(
    argv: &[String],
    cwd: &str,
    id: &str,
    cols: u16,
    rows: u16,
    mut on_output: impl FnMut(&[u8]) + Send + 'static,
    on_exit: impl FnOnce(String) + Send + 'static,
) -> Result<Pty, String> {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| e.to_string())?;
    // Everything runs under a login shell, so the profile sets `PATH` even when the studio was started from the Dock
    // with launchd's bare one.
    let shell = std::env::var("SHELL").unwrap_or("/bin/zsh".into());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    if !argv.is_empty() {
        cmd.args(["-c", "exec \"$@\"", "studio"]);
        cmd.args(argv);
    }
    cmd.cwd(cwd);
    for (k, _) in std::env::vars() {
        if SCRUB.iter().any(|p| k.starts_with(p)) {
            cmd.env_remove(k);
        }
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // launchd gives a Dock-started app no locale; terminal apps set one, and Claude Code needs UTF-8.
    if ["LANG", "LC_ALL", "LC_CTYPE"]
        .iter()
        .all(|k| std::env::var_os(k).is_none())
    {
        cmd.env("LANG", "en_US.UTF-8");
    }
    cmd.env("STUDIO_TERMINAL_ID", id);
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let (killer, pid) = (child.clone_killer(), child.process_id());
    std::thread::spawn(move || {
        let mut buf = [0u8; 16384];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            on_output(&buf[..n]);
        }
    });
    // A separate waiter, because a background job can keep the PTY open after the shell itself exits.
    std::thread::spawn(move || {
        let status = match child.wait() {
            Ok(s) => s
                .signal()
                .map(|sig| format!("signal {sig}"))
                .unwrap_or(format!("code {}", s.exit_code())),
            Err(err) => err.to_string(),
        };
        on_exit(status);
    });
    Ok(Pty {
        master: pair.master,
        writer,
        killer,
        pid,
    })
}

// The terminal's shell leads its own session (portable-pty calls setsid), so the session id is the shell's pid, and
// it stays reserved while any process of the session lives, even after the shell exits.
fn kill(t: &mut Term) {
    let _ = t.pty.killer.kill();
    let Some(sid) = t.pty.pid else { return };
    let pids = move || {
        session(&processes(), sid)
            .iter()
            .map(|p| p.pid as i32)
            .collect()
    };
    std::thread::spawn(move || crate::hang_up(pids));
}

struct Proc {
    pid: u32,
    rss_kb: u64,
    command: String,
}

// Every process on the machine, from one `ps`.
fn processes() -> Vec<Proc> {
    let out = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,rss=,comm="])
        .output();
    let out = out
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (pid, rss_kb) = (f.next()?.parse().ok()?, f.next()?.parse().ok()?);
            let command = f
                .collect::<Vec<_>>()
                .join(" ")
                .rsplit('/')
                .next()?
                .to_string();
            Some(Proc {
                pid,
                rss_kb,
                command,
            })
        })
        .collect()
}

// The live processes of session `sid`.
fn session(all: &[Proc], sid: u32) -> Vec<&Proc> {
    all.iter()
        .filter(|p| unsafe { libc::getsid(p.pid as i32) } == sid as i32)
        .collect()
}

fn list(state: &Shared) -> Value {
    // `ps` runs before the lock and the sessions' lookups after it, so output keeps flowing meanwhile.
    let all = processes();
    let (terms, record) = {
        let mut s = state.lock().unwrap();
        let rows: Vec<(Value, Option<u32>)> = s
            .terms
            .iter_mut()
            .map(|(id, t)| {
                // The program in the terminal's foreground, such as `zsh` or `claude`.
                let leader = t.pty.master.process_group_leader().map(|p| p as u32);
                t.foreground = leader
                    .and_then(|f| all.iter().find(|p| p.pid == f))
                    .map(|p| p.command.clone());
                let mut row = json!({ "id": id, "argv": t.argv, "cwd": t.cwd(), "pid": t.pty.pid, "exited": t.exited,
                                  "foreground": t.foreground, "cols": t.cols, "rows": t.rows,
                                  "scrollback_bytes": t.buf.len() });
                t.status.fields(&mut row);
                (row, t.pty.pid)
            })
            .collect();
        // The record follows the shells' folders and programs too; `stop` lists just before it stops the daemon.
        (rows, s.save())
    };
    record.write();
    let me = std::process::id();
    let terms: Vec<Value> = terms
        .into_iter()
        .map(|(mut row, pid)| {
            let procs = pid.map(|sid| session(&all, sid)).unwrap_or_default();
            row["rss_kb"] = json!(procs.iter().map(|p| p.rss_kb).sum::<u64>());
            row["processes"] = procs
                .iter()
                .map(|p| json!({ "pid": p.pid, "command": p.command, "rss_kb": p.rss_kb }))
                .collect();
            row
        })
        .collect();
    let daemon_rss_kb = all.iter().find(|p| p.pid == me).map_or(0, |p| p.rss_kb);
    json!({ "ev": "list", "daemon_pid": me, "daemon_rss_kb": daemon_rss_kb, "terms": terms })
}
