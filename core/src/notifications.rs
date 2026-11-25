//! The notices the shell shows, combined into one notification per burst.

use std::time::Duration;

/// 3 seconds, a starting guess for "a few seconds" (danielrispler/Falinks#369, decision 22 and Further Notes).
const COMBINE: Duration = Duration::from_secs(3);

/// A notice not shown yet: its title and body, and for "needs you" or "your turn" the session it is
/// about.
struct Pending {
    title: String,
    body: String,
    needs_you: Option<String>,
    your_turn: Option<String>,
}

/// The notices the shell has not shown yet. Those that come within `COMBINE` of the first show as
/// one notification, so a burst never floods Daniel and no notice waits longer than that. Times are
/// on a monotonic clock, time since the app started, so a clock change never holds them back
/// (danielrispler/Falinks#434, decision 28).
pub struct Notifications {
    pending: Vec<Pending>,
    first_at: Duration,
    /// The pending needs-you already sounded, with its toast, before `COMBINE` passed.
    sounded: bool,
    /// The `sounds` setting, on by default (danielrispler/Falinks#556).
    pub sounds: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Notifications {
            pending: Vec::new(),
            first_at: Duration::ZERO,
            sounded: false,
            sounds: true,
        }
    }
}

/// A sound the shell plays (danielrispler/Falinks#556).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    NeedsYou,
    YourTurn,
}

/// What `Notifications::due` gives the shell: a notification, as its title and body, a sound, and
/// the sessions whose turn ended off screen in a focused window, for the window's toast.
#[derive(Debug, Default, PartialEq)]
pub struct Due {
    pub notification: Option<(String, String)>,
    pub sound: Option<Sound>,
    pub finished: Vec<String>,
}

impl Notifications {
    pub fn push(&mut self, title: &str, body: String, now: Duration) {
        self.queue(title, body, None, None, now);
    }

    /// A "needs you" notice on session `session_id`, which shows only if the session still needs
    /// Daniel when it is due (danielrispler/Falinks#434, decision 27).
    pub fn push_needs_you(&mut self, session_id: &str, body: String, now: Duration) {
        self.queue("Needs you", body, Some(session_id.into()), None, now);
    }

    /// A "your turn" notice on session `session_id` (danielrispler/Falinks#517, decision 6).
    pub fn push_your_turn(&mut self, session_id: &str, body: String, now: Duration) {
        self.queue("Your turn", body, None, Some(session_id.into()), now);
    }

    fn queue(
        &mut self,
        title: &str,
        body: String,
        needs_you: Option<String>,
        your_turn: Option<String>,
        now: Duration,
    ) {
        if self.pending.is_empty() {
            self.first_at = now;
        }
        let title = title.into();
        self.pending.push(Pending {
            title,
            body,
            needs_you,
            your_turn,
        });
    }

    /// The notification due at `now`, as its title and body: the pending notices, once `COMBINE`
    /// has passed since the first, less the "needs you" ones whose session `needs_you` says no
    /// longer needs Daniel. A focused window drops them, since Daniel sees the list; a "your turn"
    /// on a session `on_screen` does not name goes to the window's toast instead, as `finished`.
    /// The sound (danielrispler/Falinks#556), with `sounds` on: focused, for needs you on a session
    /// off screen, at once, with the toast that shows it (danielrispler/Falinks#565, decision 3), so
    /// a block that clears quickly sounds as it toasts, else for your turn off screen; unfocused,
    /// only with a notification of needs you or your turn.
    pub fn due(
        &mut self,
        now: Duration,
        focused: impl FnOnce() -> bool,
        needs_you: impl Fn(&str) -> bool,
        on_screen: impl FnOnce() -> Vec<String>,
    ) -> Due {
        let blocked = |p: &Pending| p.needs_you.as_deref().is_some_and(&needs_you);
        let combined = now >= self.first_at + COMBINE;
        if self.pending.is_empty()
            || !combined && (self.sounded || !self.pending.iter().any(blocked))
        {
            return Due::default();
        }
        let focused = focused();
        let on_screen = if focused { on_screen() } else { Vec::new() };
        let off_screen =
            |id: &Option<String>| id.as_ref().is_some_and(|id| !on_screen.contains(id));
        if !combined {
            let sound =
                focused && (self.pending.iter()).any(|p| blocked(p) && off_screen(&p.needs_you));
            self.sounded = sound;
            let sound = sound.then_some(Sound::NeedsYou);
            return Due {
                sound: sound.filter(|_| self.sounds),
                ..Due::default()
            };
        }
        let sounded = std::mem::take(&mut self.sounded);
        let mut pending = std::mem::take(&mut self.pending);
        pending.retain(|p| p.needs_you.as_deref().is_none_or(&needs_you));
        let mut sound = None;
        let mut finished: Vec<String> = Vec::new();
        if focused {
            let ended = pending.iter().filter(|p| off_screen(&p.your_turn));
            for id in ended.filter_map(|p| p.your_turn.clone()) {
                if !finished.contains(&id) {
                    finished.push(id);
                }
            }
            if pending.iter().any(|p| off_screen(&p.needs_you)) {
                sound = Some(Sound::NeedsYou);
            } else if !finished.is_empty() {
                sound = Some(Sound::YourTurn);
            }
            pending.clear();
        } else if pending.iter().any(|p| p.needs_you.is_some()) {
            sound = Some(Sound::NeedsYou);
        } else if pending.iter().any(|p| p.your_turn.is_some()) {
            sound = Some(Sound::YourTurn);
        }
        // A needs-you that sounded with its toast does not sound again with the notification.
        let sound = sound.filter(|s| !(sounded && *s == Sound::NeedsYou));
        let sound = sound.filter(|_| self.sounds);
        if pending.is_empty() {
            return Due {
                notification: None,
                sound,
                finished,
            };
        }
        let pending: Vec<(String, String)> =
            pending.into_iter().map(|p| (p.title, p.body)).collect();
        let notification = Some(match &pending[..] {
            [one] => one.clone(),
            many => {
                let lines: Vec<String> = many.iter().map(|(t, b)| format!("{t}: {b}")).collect();
                (format!("{} notices", many.len()), lines.join("\n"))
            }
        });
        Due {
            notification,
            sound,
            finished,
        }
    }
}
