//! The session list, what Daniel selected and the subagent he opened, and the subagent logs.

use super::*;

/// What the session view shows: the session Daniel selected, and the subagent of it he opened since,
/// if any. The pane draws the pressed row and the view from it, and keeps no selection of its own.
#[derive(Debug, PartialEq, Serialize)]
pub struct Selection<'a> {
    pub session: &'a Session,
    pub agent: Option<&'a str>,
}

impl Studio {
    /// Session `id`'s row: its own, or the row it continues after a `/clear`.
    pub fn session(&self, id: &str) -> Option<&Session> {
        self.sessions
            .get(self.rows.get(id).map_or(id, String::as_str))
    }

    /// The session list: every session Daniel has not deleted, grouped in `State` order, the newest first in each group.
    /// A session with no folder and no prompt of an agent whose capabilities say it starts
    /// sessions from an empty window: the one Cursor.app's empty window starts at its launch, until its
    /// first prompt (#427).
    pub fn sessions(&self) -> Vec<&Session> {
        let empty_window = |s: &Session| {
            s.capabilities.empty_window_starts && s.cwd.is_empty() && s.prompts.is_empty()
        };
        let mut sessions: Vec<&Session> = (self.sessions.values())
            .filter(|s| !s.deleted && !empty_window(s))
            .collect();
        sessions.sort_by_key(|s| (s.state, Reverse(s.agent.started_at), &s.agent.id));
        sessions
    }

    /// Daniel clicked session `id`'s row, whose own view shows. Only this changes the selection.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.into());
        let id = self.rows.get(id).map_or(id, String::as_str);
        if let Some(session) = self.sessions.get_mut(id) {
            session.seen_at_selection = session.seen;
            session.update_unseen();
        }
        self.viewed = None;
        self.held = None;
    }

    /// Daniel went Home (spec #53): no session is selected or viewed.
    pub fn select_none(&mut self) {
        self.selected = None;
        self.viewed = None;
        self.held = None;
    }

    /// Daniel looks at the selected session (danielrispler/Falinks#556): while it is unseen, and only if `shown`, the
    /// window focused with the story rail showing it, its ended turns count as seen. Returns its row's id and the turns seen when that
    /// changed, for the card store to keep.
    pub fn see_selected(&mut self, shown: bool) -> Option<(String, usize)> {
        let id = self.selected()?.agent.id.clone();
        let session = self.sessions.get_mut(&id).filter(|s| s.unseen)?;
        if !shown || self.held.as_ref() == Some(&id) {
            return None;
        }
        session.seen = session.turns.len();
        session.update_unseen();
        Some((id, session.seen))
    }

    /// Daniel marked session `id` unseen (danielrispler/Falinks#556): its last turn is unseen again, until he selects it
    /// again and looks. Returns its row's id and the turns seen, for the card store to keep.
    pub fn mark_unseen(&mut self, id: &str) -> Option<(String, usize)> {
        let id = self.session(id)?.agent.id.clone();
        let session = self.sessions.get_mut(&id)?;
        session.seen = session.turns.len().saturating_sub(1);
        session.seen_at_selection = session.seen;
        session.update_unseen();
        self.held = Some(id.clone());
        Some((id, session.seen))
    }

    /// Daniel opened subagent `id`'s line, so its view shows, with its own cards and brief, and a
    /// failed helper folds into its parent's line. Returns whether the opening counts, which the
    /// card store records: only a line whose agent had already failed (danielrispler/Falinks#434, decision 39).
    pub fn open(&mut self, id: &str) -> bool {
        self.viewed = Some(id.into());
        let agents = self.sessions.values_mut().flat_map(|s| &mut s.agents);
        let mut opened = false;
        for agent in agents.filter(|a| a.agent.id == id && a.failed()) {
            agent.opened = true;
            opened = true;
        }
        opened
    }

    /// The session Daniel selected, which the session view shows; `None` until his first click.
    pub fn selected(&self) -> Option<&Session> {
        self.session(self.selected.as_deref()?)
    }

    /// The log of subagent `agent` of session `session`, from byte `from`: the entries after it and
    /// the offset read to (danielrispler/plotkeeper#54). `None` is "no log": an unknown session or subagent, no
    /// file, or a transcript path that is no `.jsonl`. Stateless: the caller keeps the offset. It reads
    /// the file, so a caller that holds a lock on the studio takes `subagent_log_file` and reads after.
    pub fn subagent_log(&self, session: &str, agent: &str, from: u64) -> Option<SubagentLog> {
        self.subagent_log_file(session, agent, from)?.read()
    }

    /// Where the log of subagent `agent` of session `session` is and how to read it from byte `from`, with
    /// no file read (spec danielrispler/plotkeeper#548). A Claude subagent's file is in the parent's
    /// transcript folder, `<transcript without .jsonl>/subagents/agent-<id>.jsonl`; an agy subagent's
    /// is its own conversation's transcript. `None` for an unknown session or subagent, or a transcript
    /// path that is no `.jsonl`.
    pub fn subagent_log_file(
        &self,
        session: &str,
        agent: &str,
        from: u64,
    ) -> Option<SubagentLogFile> {
        let session = self.session(session)?;
        session.subagent(agent)?;
        let transcript = std::path::Path::new(&session.transcript_file.path);
        if transcript.extension()? != "jsonl" {
            return None;
        }
        let format = session.capabilities.transcript_format;
        let path = match format {
            TranscriptFormat::Claude => transcript
                .with_extension("")
                .join("subagents")
                .join(format!("agent-{agent}.jsonl")),
            TranscriptFormat::Agy => agy::subagent_transcript(transcript, agent)?,
            TranscriptFormat::Unread => return None,
        };
        Some(SubagentLogFile {
            path,
            agent: agent.into(),
            format,
            from,
            cwd: session.cwd.clone(),
        })
    }

    /// The selected session and the subagent of it Daniel opened; `None` until his first click.
    pub fn selection(&self) -> Option<Selection<'_>> {
        let session = self.selected()?;
        Some(Selection {
            session,
            agent: self.viewed.as_deref(),
        })
    }

    /// Whose cards and brief the session view shows: the subagent Daniel opened since he selected
    /// its session, else that session; `None` until his first click.
    pub fn viewed(&self) -> Option<&str> {
        self.viewed.as_deref().or(self.selected.as_deref())
    }
}
