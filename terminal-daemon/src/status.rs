// OSC 7501, the Program Status Protocol (#555): a terminal's root status from the reports its program writes, and
// the reply to the program's query. The protocol and herdr-core's `turn` rule are in `docs/studio/research/herdr.md`,
// "Agent status: OSC 7501".
use serde_json::{Value, json};

const PREFIX: &[u8] = b"\x1b]7501;";
// The spec's limits; a report over any of them is dropped whole.
const MAX_SEQUENCE: usize = 4096;
const MAX_APP: usize = 32;
const MAX_MSG: usize = 2048;
const MAX_TITLE: usize = 192;
const STATES: &[&str] = &["idle", "working", "done", "blocked", "error", "clear"];
const REPLY: &[u8] =
    b"\x1b]7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth";

#[derive(Default)]
pub struct Status {
    // The start of a sequence the next read may finish.
    carry: Vec<u8>,
    // `None` until the program reports, and after `clear` or its exit.
    root: Option<Root>,
    // Goes up each time the root leaves `working`.
    turn: u64,
}

struct Root {
    state: String,
    kind: Option<String>,
    app: Option<String>,
    // When the state last changed, in ms since the epoch.
    status_ms: u128,
}

impl Status {
    /// Reads the program's output; returns the replies to write back to it.
    pub fn read(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut data = std::mem::take(&mut self.carry);
        data.extend_from_slice(bytes);
        let mut replies = Vec::new();
        let mut at = 0;
        while let Some(start) = find(&data[at..]).map(|i| at + i) {
            let body = start + PREFIX.len();
            let stop = data[body..].iter().position(|&b| b == 0x07 || b == 0x1b);
            let Some(stop) = stop.map(|i| body + i) else {
                if data.len() - start <= MAX_SEQUENCE {
                    self.carry = data[start..].to_vec();
                }
                return replies;
            };
            let end = match data.get(stop..stop + 2) {
                _ if data[stop] == 0x07 => stop + 1,
                Some(b"\x1b\\") => stop + 2,
                // A lone ESC at the end of this read: the terminator may follow in the next.
                None if data.len() - start <= MAX_SEQUENCE => {
                    self.carry = data[start..].to_vec();
                    return replies;
                }
                // Not a terminator: the ESC may start the next sequence.
                _ => {
                    at = stop;
                    continue;
                }
            };
            at = end;
            if end - start > MAX_SEQUENCE {
                continue;
            }
            match std::str::from_utf8(&data[body..stop]) {
                Ok("?") => {
                    replies.extend_from_slice(REPLY);
                    replies.extend_from_slice(&data[stop..end]);
                }
                Ok(report) => self.report(report),
                Err(_) => {}
            }
        }
        // Keeps a tail that may be the prefix's start.
        let keep = (1..PREFIX.len())
            .rev()
            .find(|&k| data.ends_with(&PREFIX[..k]))
            .unwrap_or(0);
        self.carry = data[data.len() - keep..].to_vec();
        replies
    }

    fn report(&mut self, report: &str) {
        let field = |key: &str| {
            report
                .split(':')
                .filter_map(|pair| pair.split_once('='))
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.to_string())
        };
        // `title` and `msg` are base64; their limits are on the decoded bytes.
        let decoded = |key| field(key).map_or(0, |v| v.len() / 4 * 3);
        let over = field("app").is_some_and(|a| a.len() > MAX_APP)
            || decoded("msg") > MAX_MSG
            || decoded("title") > MAX_TITLE;
        // shortcut: child records (a non-empty `id`) are ignored; keep them when the studio shows sub-agents.
        let child = field("id").is_some_and(|id| !id.is_empty());
        let Some(state) = field("state").filter(|s| STATES.contains(&s.as_str())) else {
            return;
        };
        if over || child {
            return;
        }
        self.leave_working(state == "working");
        if state == "clear" {
            self.root = None;
            return;
        }
        let status_ms = match &self.root {
            Some(r) if r.state == state => r.status_ms,
            _ => now_ms(),
        };
        self.root = Some(Root {
            state,
            kind: field("kind"),
            app: field("app"),
            status_ms,
        });
    }

    fn leave_working(&mut self, working: bool) {
        if !working && self.root.as_ref().is_some_and(|r| r.state == "working") {
            self.turn += 1;
        }
    }

    /// The program exited: its status is gone.
    pub fn exit(&mut self) {
        self.leave_working(false);
        self.root = None;
    }

    /// The program's name for itself in its root status, for the record (#561).
    pub fn app(&self) -> Option<&str> {
        self.root.as_ref()?.app.as_deref()
    }

    /// Adds `state`, `kind`, `app`, `turn` and `status_ms` to a `list` row; all but `turn` are null without a status.
    pub fn fields(&self, row: &mut Value) {
        let r = self.root.as_ref();
        row["state"] = json!(r.map(|r| &r.state));
        row["kind"] = json!(r.and_then(|r| r.kind.as_ref()));
        row["app"] = json!(r.and_then(|r| r.app.as_ref()));
        row["status_ms"] = json!(r.map(|r| r.status_ms as u64));
        row["turn"] = json!(self.turn);
    }
}

fn find(data: &[u8]) -> Option<usize> {
    data.windows(PREFIX.len()).position(|w| w == PREFIX)
}

fn now_ms() -> u128 {
    let since = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH);
    since.map_or(0, |d| d.as_millis())
}
