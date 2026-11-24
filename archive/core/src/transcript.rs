// Removed from core/src/transcript.rs in #416: the context's window and share from a session's status line snapshot, which no session saves since the terminal went (#335). `Transcript` kept the snapshot across messages with `let snapshot = self.context.as_mut().and_then(|c| c.snapshot.take()); self.context = Some(Context::new(input_tokens(usage), snapshot));`. Fragments, kept verbatim.

/// How full the context is, as Claude Code's status line measures it: from the session's status line
/// snapshot when there is one, else the input tokens of the agent's latest message with no share
/// (danielrispler/Falinks#517, decisions 1 and 2; danielrispler/Falinks#519).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Context {
    pub tokens: u64,
    pub window: Option<u64>,
    /// The percent of the window used, Claude's own `used_percentage`, else rounded to nearest.
    pub share: Option<u64>,
    /// The percent left, Claude's own `remaining_percentage`, else `100 - share`.
    pub left: Option<u64>,
    /// The transcript's tokens, which count when the snapshot has no `current_usage` yet.
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) measured: u64,
    /// The snapshot's `context_window`, which the shell sets (`Studio::set_snapshot`).
    #[serde(skip_serializing_if = "crate::saved::unsaved", default)]
    pub(crate) snapshot: Option<Value>,
}

impl Context {
    pub(crate) fn new(measured: u64, snapshot: Option<Value>) -> Self {
        let field = |key: &str| snapshot.as_ref().and_then(|s| s[key].as_u64());
        let window = field("context_window_size").filter(|&w| w > 0);
        // `current_usage` is null before the session's first API call.
        let usage = snapshot.as_ref().map(|s| &s["current_usage"]);
        let tokens = match usage {
            Some(usage) if window.is_some() && usage.is_object() => input_tokens(usage),
            _ => measured,
        };
        let share =
            window.map(|w| field("used_percentage").unwrap_or_else(|| (100 * tokens + w / 2) / w));
        let left = share.map(|s| field("remaining_percentage").unwrap_or(100 - s.min(100)));
        Context {
            tokens,
            window,
            share,
            left,
            measured,
            snapshot,
        }
    }
}
