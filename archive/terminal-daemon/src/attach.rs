// `attach <terminal>`: the escape hatch (danielrispler/Falinks#322, point 2). Connects the terminal it runs in, any terminal app, to a
// daemon terminal, so a studio build that cannot start never cuts Daniel off from running agents.
use crate::{Client, PROTOCOL, decode, encode};
use serde_json::json;
use std::{
    io::{Read, Write},
    path::Path,
    sync::mpsc,
    time::Duration,
};

/// Ctrl-], as in telnet: leaves the terminal running and returns to the shell.
const DETACH: u8 = 0x1d;

fn size() -> Option<(u16, u16)> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    (unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) } == 0).then_some((ws.ws_col, ws.ws_row))
}

pub fn run(socket: &Path, id: &str) -> Result<(), String> {
    let connect =
        || Client::connect(socket, PROTOCOL).map_err(|e| format!("{}: {e}", socket.display()));
    let (mut events, mut requests) = (connect()?, connect()?);
    events
        .send(&json!({ "op": "attach", "id": id }))
        .map_err(|e| e.to_string())?;
    let replay = events
        .recv_until(|v| v["id"] == id)
        .map_err(|e| e.to_string())?;
    if replay["ev"] != "replay" {
        return Err(format!(
            "{id}: {}",
            replay["message"].as_str().unwrap_or("no such terminal")
        ));
    }
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(0, &mut saved) } != 0 {
        return Err("attach needs a terminal".into());
    }
    let mut raw = saved;
    unsafe {
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(&decode(&replay["data"]));
    let _ = out.flush();
    let (done, finished) = mpsc::channel::<String>();

    let ended = done.clone();
    std::thread::spawn(move || {
        let why = loop {
            match events.recv() {
                Ok(ev) if ev["ev"] == "output" => {
                    let _ = out.write_all(&decode(&ev["data"]));
                    let _ = out.flush();
                }
                Ok(ev) if ev["ev"] == "exit" => {
                    break format!("exited: {}", ev["status"].as_str().unwrap_or(""));
                }
                Ok(_) => {}
                Err(_) => break "the daemon is gone".into(),
            }
        };
        let _ = ended.send(why);
    });
    if let Some(status) = replay["exited"].as_str() {
        let _ = done.send(format!("exited: {status}"));
    }

    // The last resize wins, so this terminal takes the size of the window attach runs in, and follows it.
    let (mut resizes, resized) = (connect()?, id.to_string());
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            let now = size();
            if now != last
                && let Some((cols, rows)) = now
            {
                let _ = resizes
                    .send(&json!({ "op": "resize", "id": resized, "cols": cols, "rows": rows }));
            }
            last = now;
            // shortcut: polls the window size instead of handling SIGWINCH; fine for an escape hatch.
            std::thread::sleep(Duration::from_millis(250));
        }
    });

    let id = id.to_string();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let why = loop {
            let n = match std::io::stdin().read(&mut buf) {
                Ok(0) | Err(_) => break "stdin closed",
                Ok(n) => n,
            };
            let (data, detach) = match buf[..n].iter().position(|&b| b == DETACH) {
                Some(at) => (&buf[..at], true),
                None => (&buf[..n], false),
            };
            if !data.is_empty()
                && requests
                    .send(&json!({ "op": "input", "id": id, "data": encode(data) }))
                    .is_err()
            {
                break "the daemon is gone";
            }
            if detach {
                break "detached";
            }
        };
        let _ = done.send(why.into());
    });

    let why = finished.recv().unwrap_or_default();
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    eprintln!("\r\n[{why}]");
    Ok(())
}
