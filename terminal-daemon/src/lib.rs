//! The studio's terminal daemon (#322, #464): a background process that owns the PTYs, so terminals outlive the app.
//! It is its own crate and binary, so rebuilding the app never rebuilds or restarts it.
//!
//! # Protocol
//!
//! JSON lines over one Unix socket. Terminal bytes travel base64-encoded in `data`. On connect the daemon sends
//! `{"ev":"hello","protocol":N,"daemon_pid":P}`. Additive changes (new optional fields, new requests an older daemon
//! ignores) keep `N`; the app accepts any daemon whose `N` is at or above its minimum ([`Client::connect`]).
//!
//! Requests (`op`), and what the daemon sends back to the requesting client:
//! - `spawn {id, argv, cwd, cols, rows}`: `spawned {id}` or `error {id, message}`. An empty `argv` is a login shell.
//! - `attach {id}`: `replay {id, data, cols, rows, exited}`, then every `output {id, data}` and `exit {id, status}`.
//! - `input {id, data}`, `resize {id, cols, rows}`: no reply.
//! - `kill {id}`: ends the terminal's whole session (`SIGHUP`, then `SIGKILL`); `exit` follows.
//! - `forget {id}`: kills it if needed and drops it from `list`.
//! - `list`: `list {daemon_pid, daemon_rss_kb, terms: [{id, argv, cwd, pid, exited, foreground, cols, rows,
//!   scrollback_bytes, rss_kb, processes: [{pid, command, rss_kb}]}]}`. `processes` is every live process of the
//!   terminal's session, so after `exited` is set it lists what was left behind; `rss_kb` is their sum. `cwd` is
//!   the folder its leader is in now, such as where its shell `cd`-ed to, until it exits. Each row also carries the
//!   program's OSC 7501 root status (#555): `state`, `kind`, `app` and `status_ms` (when the state last changed),
//!   all null until it reports and after `clear` or its exit, and `turn`, which goes up each time the root leaves
//!   `working`.
//!
//! The daemon answers a program's OSC 7501 query (`ESC ] 7501 ; ?`) with the states and kinds it knows, ending with
//! the query's own terminator.
//!
//! - `app`: marks the connection as the app's, not a CLI attach's; no reply. When it drops (a quit or a crash), the
//!   daemon deletes the presence file ([`presence`]), but not when the daemon drops it for falling behind.
//!
//! Unknown requests and fields are ignored.
//!
//! Beside the socket, the daemon keeps a record of the terminals it runs, `[{id, argv, cwd, foreground}]` ([`record`]),
//! rewritten at each spawn, exit, forget and `list`, with `cwd` as `list` gives it and `foreground` as `list` last
//! saw it (#561). It outlives the daemon, so after a crash, a reboot or [`stop`] it lists the
//! terminals that were lost (#470).
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use serde_json::Value;
use std::{
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
};

pub mod attach;
pub mod daemon;
mod status;

/// The protocol version this daemon speaks. Bump it only for a change an older app cannot read.
pub const PROTOCOL: u64 = 1;

/// The record of the terminals the daemon on `socket` runs.
pub fn record(socket: &Path) -> PathBuf {
    socket.with_extension("terminals.json")
}

/// The studio's presence file for Remote Control (#522), which the daemon deletes when the app's connection drops
/// (#555). The socket lives in the studio's data folder, `studio_hook::data_dir`, beside `present`.
pub fn presence(socket: &Path) -> PathBuf {
    socket.with_file_name("present")
}

/// Stops the daemon on `socket`, whatever its protocol, for an upgrade (#322 resolution, point 4): `SIGTERM`, then
/// the hangup and, after 500 ms, `SIGKILL` to every process of its terminals, as `kill` does. Its record stays.
pub fn stop(socket: &Path) -> io::Result<()> {
    let mut client = Client::connect(socket, 0)?;
    client.send(&serde_json::json!({ "op": "list" }))?;
    let list = client.recv_until(|v| v["ev"] == "list")?;
    let terms = list["terms"].as_array().into_iter().flatten();
    let procs = terms.flat_map(|t| t["processes"].as_array().into_iter().flatten());
    let pids: Vec<i32> = procs
        .filter_map(|p| p["pid"].as_i64())
        .map(|p| p as i32)
        .collect();
    unsafe { libc::kill(client.daemon_pid as i32, libc::SIGTERM) };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while UnixStream::connect(socket).is_ok() {
        if std::time::Instant::now() > deadline {
            return Err(io::Error::other("the daemon did not stop"));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    hang_up(|| pids.clone());
    Ok(())
}

/// Sends `SIGHUP` to `pids()`, then, after a grace period for shells to save history, `SIGKILL` to
/// whatever `pids()` then gives, which ignored the hangup.
fn hang_up(pids: impl Fn() -> Vec<i32>) {
    for (sig, grace) in [(libc::SIGHUP, 500), (libc::SIGKILL, 0)] {
        for pid in pids() {
            unsafe { libc::kill(pid, sig) };
        }
        std::thread::sleep(std::time::Duration::from_millis(grace));
    }
}

/// The one socket the studio's daemon listens on.
pub fn default_socket() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
        .join("Library/Application Support/dev.falinks.studio/terminal-daemon.sock")
}

pub fn encode(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn decode(data: &Value) -> Vec<u8> {
    B64.decode(data.as_str().unwrap_or("")).unwrap_or_default()
}

/// A connection to the daemon, as the app or `attach` holds it.
pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    /// The daemon's protocol version, from its `hello`.
    pub protocol: u64,
    /// The daemon's pid, from its `hello`.
    pub daemon_pid: u32,
}

impl Client {
    /// Connects and refuses a daemon older than `min_protocol`.
    pub fn connect(socket: &Path, min_protocol: u64) -> io::Result<Client> {
        let writer = UnixStream::connect(socket)?;
        let mut client = Client {
            reader: BufReader::new(writer.try_clone()?),
            writer,
            protocol: 0,
            daemon_pid: 0,
        };
        let hello = client.recv()?;
        client.protocol = hello["protocol"].as_u64().unwrap_or(0);
        client.daemon_pid = hello["daemon_pid"].as_u64().unwrap_or(0) as u32;
        if hello["ev"] != "hello" || client.protocol < min_protocol {
            return Err(io::Error::other(format!(
                "daemon protocol {} is below {min_protocol}",
                client.protocol
            )));
        }
        Ok(client)
    }

    pub fn send(&mut self, msg: &Value) -> io::Result<()> {
        writeln!(self.writer, "{msg}")
    }

    /// The next event; an error once the daemon is gone.
    pub fn recv(&mut self) -> io::Result<Value> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        serde_json::from_str(&line).map_err(io::Error::other)
    }

    /// The reading half, for a thread that reads events, and the writing half, for requests.
    pub fn split(self) -> (BufReader<UnixStream>, UnixStream) {
        (self.reader, self.writer)
    }

    /// Skips events until one matches.
    pub fn recv_until(&mut self, matches: impl Fn(&Value) -> bool) -> io::Result<Value> {
        loop {
            let ev = self.recv()?;
            if matches(&ev) {
                return Ok(ev);
            }
        }
    }
}
