//! The state a replay made, saved so that a launch replays only the log's tail
//! (danielrispler/plotkeeper#134). The saved state is all of `Studio`'s, fields the pane's JSON
//! leaves out included: those fields say `skip_serializing_if = "saved::unsaved"`, which skips them
//! except while `Studio::save` runs. The shell's own fields (its root lookup, its clock, its
//! settings) are not saved.

use crate::Studio;
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fmt,
};

/// The saved state's format. Bump it when a saved field changes meaning or a field the pane's JSON
/// leaves out is added, so an older file is not read as the new shape: a field the old file lacks
/// makes it unreadable anyway, unless it has a default.
const FORMAT: u32 = 8;

thread_local! {
    static SAVING: Cell<bool> = const { Cell::new(false) };
}

/// Whether a field the pane's JSON leaves out is left out now: all the time, but while `Studio::save`
/// runs.
pub(crate) fn unsaved<T>(_: &T) -> bool {
    !SAVING.get()
}

impl Studio {
    /// The state as it is, to give `resume` at the next launch: the replay's result, so call it
    /// after the replay and before anything live moves it.
    pub fn save(&self) -> Vec<u8> {
        SAVING.set(true);
        let state = serde_json::to_vec(self);
        SAVING.set(false);
        let mut saved = format!("studio-state {FORMAT}\n").into_bytes();
        saved.extend(state.expect("the studio's state is JSON"));
        saved
    }

    /// Takes the state `save` made, for a launch about to replay the event log of `log_len` bytes:
    /// returns the byte offset the replay goes on from. Saved state of another format, one that does
    /// not read, or one beside a log shorter than it read, is ignored, and the replay starts from
    /// 0. Call it on a new studio, before any replay; the shell's root lookup and its setting stay.
    pub fn resume(&mut self, saved: &[u8], log_len: u64) -> u64 {
        let Some(at) = saved.iter().position(|b| *b == b'\n') else {
            return 0;
        };
        if saved[..at] != *format!("studio-state {FORMAT}").as_bytes() {
            return 0;
        }
        match serde_json::from_slice::<Studio>(&saved[at + 1..]) {
            Ok(mut state) if state.log_end <= log_len => {
                state.roots = std::mem::take(&mut self.roots);
                state.summaries_on_open = self.summaries_on_open;
                state.check_lists = std::mem::take(&mut self.check_lists);
                // A session saved before its project folder (spec #659) belongs to its saved folder's project.
                for session in state.sessions.values_mut() {
                    if session.project.is_empty() {
                        session.project = crate::session::project_of(&session.cwd).into();
                    }
                    // A row saved before rows recorded their kind (#723) takes it from its tool's name.
                    let agent = session.capabilities.agent;
                    (session.ledger).classify(|tool| crate::event::kind_of(agent, tool));
                }
                *self = state;
                self.log_end
            }
            _ => 0,
        }
    }
}

/// Everything the replay made, so that a test compares a restored studio with a replayed one field by
/// field. A new field of `Studio` has to be named here, or this does not compile.
impl fmt::Debug for Studio {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Studio {
            agy,
            sessions,
            selected,
            viewed,
            awaiting,
            held,
            now: _,
            roots: _,
            check_lists: _,
            log_end,
            durations,
            rows,
            summaries_on_open: _,
            mismatched,
            listed,
            live_terminals,
            tasks,
        } = self;
        f.debug_struct("Studio")
            .field("agy", &serde_json::to_value(agy).ok())
            .field("sessions", &sessions.iter().collect::<BTreeMap<_, _>>())
            .field("selected", selected)
            .field("viewed", viewed)
            .field("awaiting", awaiting)
            .field("held", held)
            .field("log_end", log_end)
            .field("durations", &durations.sorted())
            .field("rows", &rows.iter().collect::<BTreeMap<_, _>>())
            .field("mismatched", &mismatched.iter().collect::<BTreeMap<_, _>>())
            .field("listed", listed)
            .field(
                "live_terminals",
                &live_terminals.iter().collect::<BTreeSet<_>>(),
            )
            .field("tasks", &tasks.iter().collect::<BTreeMap<_, _>>())
            .finish()
    }
}
