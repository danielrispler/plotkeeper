//! Each folder's worktree root, which events touch, and the fingerprints taken of them.

use super::*;

type FindRoot = dyn Fn(&str) -> Option<String> + Send;

/// Each folder's worktree root (danielrispler/Falinks#434, decision 11), found once per folder by the shell's lookup. With
/// none found, a folder is its own root.
#[derive(Default)]
pub(crate) struct Roots {
    find: Option<Box<FindRoot>>,
    known: std::cell::RefCell<HashMap<String, String>>,
}

impl Roots {
    pub(crate) fn of(&self, folder: &str) -> String {
        if folder.is_empty() {
            return String::new();
        }
        let mut known = self.known.borrow_mut();
        if let Some(root) = known.get(folder) {
            return root.clone();
        }
        let found = self.find.as_ref().and_then(|find| find(folder));
        let root = found.unwrap_or_else(|| folder.into());
        known.insert(folder.into(), root.clone());
        root
    }

    /// `path` relative to worktree `root`, when it lies inside it. A root is spelled as git spells
    /// it, a path as its tool does: one through a symlink (`/tmp` for `/private/tmp`) is inside the
    /// root whose top folder holds it, the shortest of its folders that has `root` as its root.
    pub(crate) fn inside<'a>(&self, root: &str, path: &'a str) -> Option<&'a str> {
        let direct = path.strip_prefix(root).and_then(|p| p.strip_prefix('/'));
        if direct.is_some() || root.is_empty() {
            return direct;
        }
        let mut folders: Vec<&std::path::Path> = std::path::Path::new(path).ancestors().collect();
        folders.reverse();
        let top = folders.into_iter().skip(1).find_map(|f| {
            let f = f.to_str()?;
            (self.of(f) == root).then_some(f)
        })?;
        path.strip_prefix(top)?.strip_prefix('/')
    }
}

impl Studio {
    /// A studio that finds a folder's worktree root with `find` (danielrispler/Falinks#434, decision 11), such as the
    /// shell's `git` lookup. `Studio::default` takes each folder as its own root.
    pub fn with_roots(find: impl Fn(&str) -> Option<String> + Send + 'static) -> Self {
        let roots = Roots {
            find: Some(Box::new(find)),
            ..Roots::default()
        };
        Studio {
            roots,
            ..Studio::default()
        }
    }

    /// Each folder's worktree root found so far, which the shell keeps across launches
    /// (danielrispler/plotkeeper#132).
    pub fn known_roots(&self) -> HashMap<String, String> {
        self.roots.known.borrow().clone()
    }

    /// Takes the roots a launch before found, so their folders need no lookup: on the real log the
    /// lookups were most of the replay's time (danielrispler/plotkeeper#132).
    pub fn know_roots(&mut self, roots: HashMap<String, String>) {
        self.roots.known.borrow_mut().extend(roots);
    }

    /// The worktree roots that `event` of a session's own agent touches (danielrispler/Falinks#434, decision 9): its
    /// folder's, and an `Edit` or `Write` path's, but those of its running workers with cards. A
    /// start, an end or a notice touches none.
    // shortcut: a Bash write through a path, without a `cd`, is not seen; parse commands for paths
    // if a live run shows cards missing such writes.
    pub(super) fn touches(&self, event: &Event) -> Vec<String> {
        if event.agent_id.is_some() || event.outside_turns() {
            return Vec::new();
        }
        let session = self.sessions.get(&event.session_id);
        let mut roots = self.named_roots(event);
        roots.retain(|root| !session.is_some_and(|s| s.workers_own(root)));
        roots
    }

    /// The worktree roots that the call of `event` names: its folder's, an `Edit` or `Write` path's,
    /// and those the hook found a command touched (danielrispler/plotkeeper#203).
    pub(super) fn named_roots(&self, event: &Event) -> Vec<String> {
        let path = event.tool().and_then(|t| t.path.as_deref());
        let path = path.filter(|p| p.starts_with('/'));
        let folder = path.and_then(|p| std::path::Path::new(p).parent()?.to_str());
        // With no lookup, a path under the event's folder is in its worktree.
        let below = |f: &&str| f.starts_with(&format!("{}/", event.cwd));
        let folder = folder.filter(|f| self.roots.find.is_some() || !below(f));
        let mut roots: Vec<String> = Vec::new();
        let listed = event.studio_touched_roots.iter().flatten();
        let folders = [Some(event.cwd.as_str()), folder].into_iter().flatten();
        // The hook's list holds roots already, so they need no lookup.
        let found = folders.map(|f| self.roots.of(f));
        for root in found.chain(listed.cloned()) {
            if !root.is_empty() && !roots.contains(&root) {
                roots.push(root);
            }
        }
        roots
    }

    /// The worktrees the shell fingerprints before it applies this live line (danielrispler/Falinks#328, module 5), each
    /// with its own key: none, one, or at a wake-up also its workers' ends (danielrispler/Falinks#369, decisions 6, 8, 10
    /// and 11).
    pub fn fingerprint_folders(&self, line: &str) -> Vec<Fingerprint> {
        let event = self.peek(line);
        let event = event.filter(|e| {
            !e.has(|h| {
                matches!(
                    h,
                    Happened::Renamed(_) | Happened::PromptNamed(_) | Happened::Deleted
                )
            })
        });
        event.map_or_else(Vec::new, |mut event| {
            event.session_id = self.row(&event);
            self.looks(&event)
        })
    }

    /// The row `event` belongs to: its session's, or for a start from `/clear`, the cleared session's
    /// in the same folder (danielrispler/Falinks#461, decision 18).
    // shortcut: a start that lands before its clear's `SessionEnd` makes a new row; match both ways
    // if the async hooks show it.
    pub(super) fn row(&self, event: &Event) -> String {
        if let Some(row) = self.rows.get(&event.session_id) {
            return row.clone();
        }
        let fresh = !self.sessions.contains_key(&event.session_id);
        if fresh && event.is_start_from(Start::Clear) {
            let cleared = (self.sessions.values()).filter(|s| s.clearing && s.cwd == event.cwd);
            if let Some(s) = cleared.max_by_key(|s| s.agent.last_event_at) {
                return s.agent.id.clone();
            }
        }
        event.session_id.clone()
    }

    /// The fingerprints before `event`: its own (`look`), and the end of each worker with cards that
    /// it ends, by any signal (danielrispler/Falinks#434, decision 19).
    pub(super) fn looks(&self, event: &Event) -> Vec<Fingerprint> {
        let mut looks = self.look(event);
        let Some(session) = self.sessions.get(&event.session_id) else {
            return looks;
        };
        for (id, ..) in session.ends(event) {
            let Some(agent) = session.subagent(&id) else {
                continue;
            };
            // One in its session's worktree, outside a turn, ends with the session's card (danielrispler/Falinks#434,
            // decision 16).
            let key = &session.agent.id;
            if agent.owner == *key && !session.in_turn() {
                let folder = agent.root.clone();
                let seen = |l: &Fingerprint| l.key == *key && l.folder == folder;
                if !looks.iter().any(seen) {
                    let purpose = Purpose::Card;
                    looks.push(Fingerprint {
                        key: key.clone(),
                        folder,
                        purpose,
                        began: None,
                    });
                }
            }
            let folder = agent.folder.clone();
            let Some(folder) = folder.filter(|_| session.has_cards(&id)) else {
                continue;
            };
            match looks.iter_mut().find(|look| look.key == id) {
                Some(look) => look.purpose = Purpose::Card,
                None => looks.push(Fingerprint {
                    key: id,
                    folder,
                    purpose: Purpose::Card,
                    began: Some(agent.agent.started_at),
                }),
            }
        }
        looks
    }

    /// The fingerprints before `event`. A session's own, one per worktree (danielrispler/Falinks#434, decisions 8 to
    /// 10): at a turn's first touch of a worktree, its base, so what Daniel changed himself before
    /// does not count; at a turn's end, its `Stop` or the next turn's first event (danielrispler/Falinks#434, decision 5),
    /// every worktree it touched, whose change since its base is the card's, and whose record is
    /// the next turn's base. A turn that starts after the last one ended first cards what its
    /// subagents changed between them (decision 17). A subagent's: its first event's, its base; a
    /// helper's before and after each Bash step, for a change across it that makes it a worker; a
    /// worker's after each Bash step, for a commit; and a worker's end. On a step 20 minutes after
    /// the last card of the worktree it is in, that worktree's, for the fallback card; a session's
    /// only in a turn. A helper's look after its Bash step comes beside such a fallback's.
    #[allow(
        clippy::too_many_lines,
        reason = "left whole when the lint came in (#555); spec #548 names no split of it"
    )]
    pub(super) fn look(&self, event: &Event) -> Vec<Fingerprint> {
        let step = event.tool_ends();
        let due = |since: &Since| step && event.studio_received_ms >= since.at + FALLBACK_MS;
        let session = self.sessions.get(&event.session_id);
        // A session's own fallback is on a prompt's steps only, not between its turns.
        let session_due = |s: &Session| s.in_turn() && due(&s.agent.since);
        let look = |key: &str, folder: &str, purpose| {
            let began = session.and_then(|s| match s.subagent(key) {
                Some(agent) => Some(agent.agent.started_at),
                // A turn that opens with this event begins at it.
                None if session.is_some_and(|s| s.opens_turn(event)) => {
                    Some(event.studio_received_ms)
                }
                None => s.turns.last().map(|t| t.started_at),
            });
            let look = Fingerprint {
                key: key.into(),
                folder: folder.into(),
                purpose,
                began,
            };
            (!folder.is_empty()).then_some(look)
        };
        let Some(id) = &event.agent_id else {
            let blank = Session::default();
            let s = session.unwrap_or(&blank);
            let key = &event.session_id;
            let ends = s.ends_turn_early(event) || event.turn_ends();
            let opens = s.opens_turn(event);
            // A turn's end, and a due fallback, look at every worktree it touched (danielrispler/Falinks#434, decision
            // 10).
            let mut closing = s.closing();
            // A turn seen only from its end, such as one the log starts in, ends where it ends.
            if closing.is_empty() && ends {
                closing = vec![self.roots.of(&event.cwd)];
            }
            let purpose = if ends {
                Some(Purpose::TurnEnd)
            } else {
                session
                    .is_some_and(session_due)
                    .then_some(Purpose::Fallback)
            };
            let mut looks: Vec<Fingerprint> = purpose
                .into_iter()
                .flat_map(|purpose| closing.iter().filter_map(move |r| look(key, r, purpose)))
                .collect();
            // Each worktree a turn touches first gets its base, which a turn's end just took. A
            // turn that starts after the last one ended first cards what subagents changed
            // between them (danielrispler/Falinks#434, decision 17).
            if opens || s.in_turn() && !event.turn_ends() {
                let between = if opens && !s.in_turn() {
                    s.between_turns(event.studio_received_ms)
                } else {
                    Vec::new()
                };
                let purpose = if between.is_empty() {
                    Purpose::Base
                } else {
                    Purpose::Card
                };
                let known = |root: &String| {
                    closing.contains(root) && ends || s.touched.contains(root) && !opens
                };
                let mut first: Vec<String> = self
                    .touches(event)
                    .into_iter()
                    .filter(|r| !known(r))
                    .collect();
                // The worktrees those subagents worked in, also those the turn touches only later.
                for id in &between {
                    let root = s.subagent(id).map(|a| a.root.clone()).unwrap_or_default();
                    if !root.is_empty() && !first.contains(&root) && !known(&root) {
                        first.push(root);
                    }
                }
                looks.extend(first.iter().filter_map(|root| look(key, root, purpose)));
            }
            return looks;
        };
        let Some(session) = session else {
            return Vec::new();
        };
        let agent = session.subagent(id);
        // The worktree this event makes its home, which its fingerprints are taken in from now on.
        let home = session.new_home(id, event, &self.roots);
        let Some(folder) = home
            .as_deref()
            .or_else(|| agent.and_then(|a| a.folder.as_deref()))
        else {
            return look(id, &event.cwd, Purpose::Base).into_iter().collect();
        };
        if home.is_some() {
            return look(id, folder, Purpose::Base).into_iter().collect();
        }
        let worker = |a: &Subagent| a.kind == Kind::Worker;
        let bash = event.tool().is_some_and(|t| t.kind == ToolKind::Command);
        // A helper's base right before its command, so only a change across it promotes it.
        // shortcut: the hooks are async, so a command that ends before the shell reads its
        // `PreToolUse` line is absorbed into this base and never promotes its helper; make the
        // hook synchronous if a live run shows missed promotions matter (danielrispler/Falinks#434, decision 23).
        if bash && event.tool_starts() && agent.is_some_and(|a| !worker(a)) {
            return look(id, folder, Purpose::Base).into_iter().collect();
        }
        let bash = step && bash;
        let owner = agent.map_or_default(|a| a.owner.as_str());
        let since = &session.record(owner).since;
        let fallback: Vec<Fingerprint> = match session.subagent(owner) {
            Some(own) if &own.agent.id == id => if worker(own) && due(since) {
                look(id, folder, Purpose::Fallback)
            } else {
                look(id, folder, Purpose::Commit).filter(|_| bash)
            }
            .into_iter()
            .collect(),
            // It works in the worktree of a worker above it, or of its session.
            Some(above) if worker(above) && due(since) => above
                .folder
                .as_deref()
                .and_then(|above_folder| look(&above.agent.id, above_folder, Purpose::Fallback))
                .into_iter()
                .collect(),
            None if session_due(session) => {
                let closing = session.closing().into_iter();
                let key = &session.agent.id;
                closing
                    .filter_map(|root| look(key, &root, Purpose::Fallback))
                    .collect()
            }
            _ => Vec::new(),
        };
        // A due fallback's look comes beside a helper's own (danielrispler/Falinks#434, decision 22).
        let helper =
            look(id, folder, Purpose::Commit).filter(|_| bash && agent.is_some_and(|a| !worker(a)));
        let helper = helper.filter(|h| fallback.iter().all(|f| f.key != h.key));
        fallback.into_iter().chain(helper).collect()
    }

    /// The sessions in a turn and the running subagents, with their folders, by their fingerprints'
    /// keys. After a replay the shell fingerprints them once, so a restart mid-turn leaves the
    /// turn's `Stop` and each subagent's next look a base to compare with.
    pub fn waiting_folders(&self) -> Vec<Fingerprint> {
        let waiting = self.sessions.values().filter(|s| s.in_turn());
        let agents = self.sessions.values().flat_map(|s| &s.agents);
        let running = agents.filter(|a| a.status == Status::Running);
        let running = running.filter_map(|a| Some((a.agent.id.as_str(), a.folder.as_deref()?)));
        // Every worktree a turn touched gets its base.
        let touched =
            waiting.flat_map(|s| s.touched.iter().map(|r| (s.agent.id.as_str(), r.as_str())));
        let folders = touched.chain(running);
        let print = |(key, folder): (&str, &str)| Fingerprint {
            key: key.into(),
            folder: folder.into(),
            purpose: Purpose::Base,
            began: None,
        };
        folders.map(print).collect()
    }
}
