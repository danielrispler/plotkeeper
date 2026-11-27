//! Taking one event log line: live, with its fingerprints' changes, or replayed at start.

use super::*;

impl Studio {
    /// Takes one live event log line that the shell took no fingerprint for, and returns what the
    /// shell must do.
    pub fn apply(&mut self, line: &str) -> Vec<Output> {
        self.apply_with_changes(line, BTreeMap::new())
    }

    /// Takes one live event log line, with what the shell's fingerprints before it found: the change
    /// in each worktree `fingerprint_folders` named since its base, by the fingerprint's key, with no
    /// entry when nothing changed or the shell took none. Returns what the shell must do. A line that
    /// is not a hook event is skipped.
    pub fn apply_with_changes(
        &mut self,
        line: &str,
        changes: BTreeMap<String, Change>,
    ) -> Vec<Output> {
        self.take(line, changes, false)
    }

    /// `apply_with_changes`, which on a replayed line keeps the turn it ends for `backfill`.
    pub(super) fn take(
        &mut self,
        line: &str,
        mut changes: BTreeMap<String, Change>,
        replay: bool,
    ) -> Vec<Output> {
        let events = self.events(line);
        // A line's newline is not part of it; one the caller left off is counted all the same.
        let len = line.strip_suffix('\n').unwrap_or(line).len() as u64;
        let at = Bytes {
            offset: self.log_end,
            len,
        };
        self.log_end += len + 1;
        let mut outputs = Vec::new();
        for mut event in events {
            event.line = at;
            outputs.extend(self.take_event(event, &mut changes, replay));
        }
        outputs
    }

    /// The events of one log line, read by the agent its hook entry named (#355): Claude's, agy's,
    /// Codex's, Qwen Code's, or none and drift for an agent plotkeeper cannot read or a line its agent's parser
    /// cannot read. A line that names none is Claude's by its shape, or else agy's.
    pub(super) fn events(&mut self, line: &str) -> Vec<Event> {
        match claude::parse(line) {
            Ok(event) => vec![event],
            Err(_) => match Named::of(line) {
                Named::Agy => self.agy.events(line).unwrap_or_default(),
                Named::Parsed(parse, at) => match parse(line) {
                    Ok(event) => vec![event],
                    Err(_) => {
                        self.agy.drift(at, AgyDriftKind::Agent);
                        Vec::new()
                    }
                },
                Named::Other(at) => {
                    self.agy.drift(at, AgyDriftKind::Agent);
                    Vec::new()
                }
            },
        }
    }

    /// The main event `line` would give, with agy's state left as it is.
    pub(super) fn peek(&self, line: &str) -> Option<Event> {
        match claude::parse(line) {
            Ok(event) => Some(event),
            Err(_) => match Named::of(line) {
                Named::Agy => self.agy.peek(line),
                Named::Parsed(parse, _) => parse(line).ok(),
                Named::Other(_) => None,
            },
        }
    }

    /// Sets session `id`'s estimate from the start of its phase, from the sessions of its kind that
    /// started before it and have ended (danielrispler/Falinks#461, decision 14): a paused session's time may be cut short.
    pub(super) fn estimate(&mut self, id: &str) {
        let Some(session) = self.sessions.get(id) else {
            return;
        };
        let work = &session.work;
        let past: Vec<&Work> = self
            .sessions
            .values()
            .filter(|s| {
                s.agent.started_at < session.agent.started_at
                    && work.kind.is_some()
                    && s.work.kind == work.kind
                    && s.ended
            })
            .map(|s| &s.work)
            .collect();
        let range = work.phase().and_then(|p| range(&past, p, MIN_HISTORY));
        if let Some(session) = self.sessions.get_mut(id) {
            session.work.estimate.range = range;
        }
    }

    /// A new studio with `lines` replayed into it, in order.
    pub fn replayed<S: AsRef<str>>(lines: impl IntoIterator<Item = S>) -> Self {
        let mut studio = Self::default();
        lines
            .into_iter()
            .for_each(|line| studio.replay(line.as_ref()));
        studio
    }

    /// Takes one event log line replayed at app start. It updates state exactly like `apply` and
    /// requests nothing, so turns that ended while the app was down get no summaries and no notices
    /// here.
    pub fn replay(&mut self, line: &str) {
        self.take(line, BTreeMap::new(), true);
    }
}
