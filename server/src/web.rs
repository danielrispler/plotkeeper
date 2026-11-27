//! The local page's transport (danielrispler/plotkeeper#325, spec #319 "Security and lifecycle"): the built page, `POST
//! /api/<command>` for the commands ([`api`](crate::api)) and `GET /events`, one server-sent event stream.
//!
//! The server listens on 127.0.0.1 only. A start has a random token; the browser opens `/?token=<t>`, the server answers with a
//! cookie and sends it on to `/` without the token. Every other request needs the cookie, a Host of `127.0.0.1:<port>` or
//! `localhost:<port>`, and, when it has an Origin, one of those too.
//!
//! A second `plotkeeper` calls the running one's `register_folder` as the page does, with the token as the cookie ([`register_with`]).

use crate::{
    App, Sink,
    api::{self, Failure, Page},
};
use serde_json::Value;
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
use tiny_http::{Header, Method, Request, Response, Server};

/// The built page's files: a published path and its bytes.
pub type Assets = &'static [(&'static str, &'static [u8])];

/// The port tried first, then the ones after it.
pub const DEFAULT_PORT: u16 = 47800;
const COOKIE: &str = "plotkeeper";
/// A request body past this is refused.
const BODY_LIMIT: u64 = 8 << 20;
const KEEPALIVE: Duration = Duration::from_secs(15);

/// The event stream's subscribers, and what the page last said of its focus.
#[derive(Default)]
struct Bus {
    subscribers: Mutex<Vec<mpsc::Sender<String>>>,
    focused: AtomicBool,
}

impl Bus {
    fn publish(&self, frame: String) {
        let mut subscribers = self.subscribers.lock().unwrap();
        subscribers.retain(|s| s.send(frame.clone()).is_ok());
    }

    fn subscribe(&self) -> mpsc::Receiver<String> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers.lock().unwrap().push(sender);
        receiver
    }
}

impl Page for Bus {
    fn focus(&self, focused: bool) {
        self.focused.store(focused, Ordering::Release);
    }
}

/// The state's sink for the page: events go to the stream; the page is focused when it says so and a tab listens. A due
/// notice goes as a `notification` event; the title count is the page's own, and there are no sounds or badge (danielrispler/plotkeeper#331).
struct WebSink(Arc<Bus>);

impl Sink for WebSink {
    fn emit(&self, name: &str, payload: Value) {
        self.0
            .publish(format!("event: {name}\ndata: {payload}\n\n"));
    }
    fn focused(&self) -> bool {
        self.0.focused.load(Ordering::Acquire) && !self.0.subscribers.lock().unwrap().is_empty()
    }
    fn replayed(&self) {}
}

/// A running server.
pub struct Web {
    server: Arc<Server>,
    port: u16,
    bus: Arc<Bus>,
    thread: Option<JoinHandle<()>>,
}

/// 64 hex digits from the system's random source.
pub fn new_token() -> String {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("read the system's random bytes");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct Context {
    app: Arc<App>,
    assets: Assets,
    token: String,
    port: u16,
    bus: Arc<Bus>,
}

impl Web {
    /// Serves `assets` and the commands of `app` on 127.0.0.1, at `preferred` or the next free port after it.
    pub fn start(app: Arc<App>, assets: Assets, token: String, preferred: u16) -> io::Result<Web> {
        let (server, port) = (preferred..=preferred.saturating_add(100))
            .find_map(|port| Server::http(("127.0.0.1", port)).ok().map(|s| (s, port)))
            .ok_or_else(|| io::Error::new(io::ErrorKind::AddrInUse, "no free port"))?;
        let server = Arc::new(server);
        let bus = Arc::new(Bus::default());
        let context = Arc::new(Context {
            app,
            assets,
            token,
            port,
            bus: bus.clone(),
        });
        let listening = server.clone();
        let thread = thread::spawn(move || {
            for request in listening.incoming_requests() {
                let context = context.clone();
                thread::spawn(move || handle(&context, request));
            }
        });
        Ok(Web {
            server,
            port,
            bus,
            thread: Some(thread),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The sink the state sends to: events to this server's stream.
    pub fn sink(&self) -> Arc<dyn Sink> {
        Arc::new(WebSink(self.bus.clone()))
    }

    /// Stops accepting requests.
    pub fn stop(mut self) {
        self.server.unblock();
        if let Some(thread) = self.thread.take() {
            // A panic in it was already reported by the panic hook.
            thread.join().ok();
        }
    }
}

/// How long a second start waits on the running server's answer: its git lookup may take up to 30 s.
const REGISTER_WAIT: Duration = Duration::from_secs(40);

/// A second start's call (spec #753, "Projects"): sends `folder` to the `register_folder` command of the plotkeeper serving on `port`,
/// with its `token` as the page's cookie, and returns the project it added, as [`crate::register_folder`] does.
pub fn register_with(port: u16, token: &str, folder: &str) -> Result<Option<String>, String> {
    let body = serde_json::json!({ "folder": folder }).to_string();
    let request = format!(
        "POST /api/register_folder HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nCookie: {COOKIE}={token}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let mut raw = String::new();
    TcpStream::connect(("127.0.0.1", port))
        .and_then(|mut stream| {
            stream.set_read_timeout(Some(REGISTER_WAIT))?;
            stream.write_all(request.as_bytes())?;
            stream.read_to_string(&mut raw)
        })
        .map_err(|e| e.to_string())?;
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head.split(' ').nth(1).unwrap_or_default();
    if status != "200" {
        return Err(format!("it answered {status}: {}", body.trim()));
    }
    serde_json::from_str(body).map_err(|e| e.to_string())
}

fn header(request: &Request, name: &'static str) -> Option<String> {
    (request.headers().iter())
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str().to_string())
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

fn refuse(request: Request, status: u16, why: &str) {
    // The client may have hung up; there is no one left to answer.
    request
        .respond(Response::from_string(why).with_status_code(status))
        .ok();
}

fn json(request: Request, status: u16, body: &Value) {
    let response = Response::from_string(body.to_string())
        .with_status_code(status)
        .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
    // The client may have hung up; there is no one left to answer.
    request.respond(response).ok();
}

fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn handle(context: &Context, mut request: Request) {
    let port = context.port;
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if !header(&request, "Host").is_some_and(|h| hosts.contains(&h)) {
        return refuse(request, 403, "Host not allowed");
    }
    if let Some(origin) = header(&request, "Origin")
        && !hosts.iter().any(|h| origin == format!("http://{h}"))
    {
        return refuse(request, 403, "Origin not allowed");
    }
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((&url, ""));
    let has_cookie = header(&request, "Cookie").is_some_and(|cookies| {
        (cookies.split(';'))
            .filter_map(|c| {
                c.trim()
                    .strip_prefix(&format!("{COOKIE}="))
                    .map(str::to_string)
            })
            .any(|value| same(&value, &context.token))
    });
    let given = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="));
    // A valid token in the link always leaves the URL, whether or not the browser already holds the cookie.
    if *request.method() == Method::Get
        && path == "/"
        && given.is_some_and(|t| same(t, &context.token))
    {
        let response = Response::empty(302)
            .with_header(Header::from_bytes("Location", "/").unwrap())
            .with_header(
                Header::from_bytes(
                    "Set-Cookie",
                    format!(
                        "{COOKIE}={}; HttpOnly; SameSite=Strict; Path=/",
                        context.token
                    ),
                )
                .unwrap(),
            );
        // The client may have hung up; there is no one left to answer.
        request.respond(response).ok();
        return;
    }
    if !has_cookie {
        return refuse(request, 403, "Open plotkeeper from its own link");
    }
    match (request.method().clone(), path) {
        (Method::Get, "/events") => events(context, request),
        (Method::Post, command) if command.starts_with("/api/") => {
            let mut body = String::new();
            if request
                .as_reader()
                .take(BODY_LIMIT)
                .read_to_string(&mut body)
                .is_err()
            {
                return refuse(request, 400, "Unreadable body");
            }
            let args = if body.trim().is_empty() {
                Value::Null
            } else {
                match serde_json::from_str(&body) {
                    Ok(args) => args,
                    Err(e) => return json(request, 400, &Value::String(e.to_string())),
                }
            };
            match api::call(&context.app, &*context.bus, &command[5..], args) {
                Ok(reply) => json(request, 200, &reply),
                Err(Failure::Unknown) => {
                    json(request, 404, &Value::String("no such command".into()));
                }
                Err(Failure::Bad(why)) => json(request, 400, &Value::String(why)),
                Err(Failure::Failed(why)) => json(request, 500, &Value::String(why)),
            }
        }
        (Method::Get, file) => {
            let file = file.trim_start_matches('/');
            let file = if file.is_empty() { "index.html" } else { file };
            if file.split('/').any(|part| part == ".." || part.is_empty()) {
                return refuse(request, 404, "Not found");
            }
            match context.assets.iter().find(|(name, _)| *name == file) {
                Some((name, bytes)) => {
                    // No frame of the page loads a URL: a micro-world's `srcdoc` frame, sandboxed to scripts, can still
                    // navigate itself, and the policy is what keeps it from sending what it read to the network.
                    let response = Response::from_data(*bytes)
                        .with_header(
                            Header::from_bytes("Content-Type", content_type(name)).unwrap(),
                        )
                        .with_header(
                            Header::from_bytes("Content-Security-Policy", "frame-src 'none'")
                                .unwrap(),
                        );
                    // The client may have hung up; there is no one left to answer.
                    request.respond(response).ok();
                }
                None => refuse(request, 404, "Not found"),
            }
        }
        _ => refuse(request, 405, "Method not allowed"),
    }
}

/// The event stream: each event as `event: <name>` and `data: <json>`, and a comment every 15 s, which also finds a closed tab.
fn events(context: &Context, request: Request) {
    let receiver = context.bus.subscribe();
    let mut out = request.into_writer();
    let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n: open\n\n";
    if out
        .write_all(head.as_bytes())
        .and_then(|_| out.flush())
        .is_err()
    {
        return;
    }
    loop {
        let frame = match receiver.recv_timeout(KEEPALIVE) {
            Ok(frame) => frame,
            Err(mpsc::RecvTimeoutError::Timeout) => ": keepalive\n\n".to_string(),
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if out
            .write_all(frame.as_bytes())
            .and_then(|_| out.flush())
            .is_err()
        {
            break;
        }
    }
    // The closed tab may have been the one that said it is focused; the tabs left say so again when their focus changes.
    context.bus.focused.store(false, Ordering::Release);
}
