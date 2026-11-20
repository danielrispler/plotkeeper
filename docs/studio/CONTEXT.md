# Studio

The shared language for the control room: the place Daniel does all agent work and keeps his understanding of it in step. The engine's language is in the root `CONTEXT.md`.

## Language

**Control room**:
The app this effort builds: the place Daniel does all agent work. It has three layers, each usable on its own: the understanding layer, the workbench and the Falinks engine.
_Avoid_: Dashboard (one view, not the thing itself)

**Understanding layer**:
The part of the control room that keeps Daniel's understanding in step with the agents' work: guesses, cards and debrief. It works without the workbench, for any agent session.

**Wait**:
The time Daniel waits on an agent: from his prompt until the agent ends its turn, or until his next prompt if he interrupts it. Answering a permission, a question or a plan inside it does not start a new wait.

**Card**:
An explanation, in the explanation language, of what a prompt changed, or what a worker changed, since the previous card. It opens with a quiz, and offers "explain more" and a rating.
_Avoid_: Summary (read after the work, with no question first)

**Turn**:
An agent session's work on one prompt, Daniel's or a wake-up, from its first event to the agent's end of turn. It also ends when the next turn starts after Daniel interrupts it, or when the session ends. A prompt Daniel queues during a turn starts its own turn when the running one ends.
_Avoid_: Step (one tool call inside a turn)

**Turn files**:
The files a turn's agent and its workers changed, with their added and removed lines: every file that differs between the git trees the hook recorded at the turn's start and end, less the files another session's or turn's call names (unless a call of the turn itself names them too) and the files a merge brought in (those are counted as "+N merged" in the **Evidence line**). There is no external count: a hand edit of Daniel's during the turn shows as the turn's. A running turn's end tree is taken on demand. The ended turn's trees are kept under `refs/plotkeeper/turns/` so `git gc` does not prune its hunks. An agent that reports no trees (agy) has none, by its capability `turn_trees`.

**Intention**:
The latest thing the agent wrote in its own words during a turn, such as why it is about to read an issue. It stays shown, with its age, until the agent writes again, so it covers the steps that follow it. While a turn runs, its intentions are the last line of each of its responses. When the turn ends, its final response's intention becomes that response's first line, so a footer a plugin adds is never the intention. The rail shows it only while the session is Running (boxed) or Needs you (small, as "Why it asks"); at your turn the turn's Headline says what happened, and the summary's first line never replaces the intention. A side question ("ask about this") gets these intentions as context.
_Avoid_: Reason (the card writer's account of a step)

**Headline**:
One sentence, at most 16 words, on what a turn did, or what it waits on: an outcome, never a narration of the steps, and for a turn that ends by asking Daniel something, the ask. The turn writer is given the turn's recorded facts and writes it; code then holds it to them. A Headline that says fixed, passes or verified with no check of the turn that ended passed (or with one that ended failed) starts with "Agent says:". A Headline with a number or id the turn's material lacks is stripped and asked for once more; stripped twice, the rail shows the final response's first line. Until the summary arrives, the Headline is that line.
_Avoid_: Intention (the agent's own words while it works)

**Turn summary**:
A Headline and up to three typed Body lines, written by one cheap writer call when the turn ends, or when Daniel opens the turn if summaries are set to write on open. A turn gets one when its final response is about 150 words or more, or when it ends asking Daniel something; other turns show their response as it is. The writer is given the turn's recorded facts (its files with their line counts, hunks up to a cap, each check's command, result and retries) and told not to restate them. A Body line is `why`, `reported`, `open` or `ask`, at most 20 words; `open` and `ask` always show, and a `reported` line reads "Agent says". A line with a number or id the turn's material lacks is dropped; numbers in the turn's diff, line counts or check results, number words and digits in a known path always pass. A summary stored before the Headline is read with its first line as the Headline and the rest as `reported` lines; the writer is not run again on it.
_Avoid_: Card (on a change, with its quiz first), brief (the whole session's, kept for resuming)

**Transcript**:
Claude Code's own record of an agent session, which the control room reads for the intention, the turn's responses, the branch and the context share. When it cannot be read, the session's view falls back to its steps only.

**Phase**:
A recognisable stretch of a session's work on a ticket, such as orienting, designing, implementing, checking, or waiting on a PR's CI. A session moves through phases in a usual order.
_Avoid_: Stage, step

**Estimate**:
A range for how long the agent's remaining work will take, from the typical durations of the phases still ahead. It appears only when there is enough history for the current kind of work: 20 ended agent sessions of that kind. It is always marked as an estimate and leaves out the time that depends on Daniel; while the session needs him, it shows "depends on you" instead of a range.
_Avoid_: ETA (implies a promise)

**Kind of work**:
What an agent session's work is, named by its first prompt's slash command, else by its first skill, such as `implement` or `wayfinder`. An estimate compares a session only with sessions of the same kind.

**Base**:
The state of a worktree that the next card's change is measured from: its commit and its files when the previous card was written, or when the prompt or worker began. A move of the commit that is not the agent's own work moves the base instead of adding to the change.
_Avoid_: Snapshot, checkpoint (the engine's term)

**Quiz**:
The one multiple-choice question a turn can have, with the writer's key, which grades Daniel's guess. It is the card's when the turn has a card, and otherwise comes with the turn summary, open under the Headline. One set of rules holds it, and code enforces the guards: [`question-rules.md`](question-rules.md). A learning question stored before the Quiz reads as one. On a check card the quiz asks whether the check will pass, and the check's result is the key.
_Avoid_: Question (an agent's question to Daniel), Learning section (retired)

**Offer**:
What one decision gives a change: a card (with its Quiz), a lone Quiz, or nothing, by the table in [`question-rules.md`](question-rules.md). A turn, a worker's card and a **late change** each get one, decided once with the same facts for the card and for the summary. Each is recorded once in `offers.jsonl` ([`storage.md`](behaviour/storage.md)).

**Late change**:
What a subagent changed in its session's own worktree after the turn ended, which the session's card then covers. It is judged below the card row: it gets a card only when it passes the table's first row, and never a lone Quiz.

**Check card**:
The card a check of the session's own agent gets at its start when it usually runs over 2 minutes, on the change since the last card, so Daniel can guess while he waits whether the check will pass.

**Guess**:
Daniel's answer to a card's quiz. The card writer's key grades it, and the card shows right or wrong, with the reasons, as soon as he answers. On a check card the check's result settles it instead, and the card shows right or wrong, with the reasons, once the check ends. It is never shown as a score.

**Settle**:
To grade the guess on a check card by the check's result, once the check ends. A tap after the result is no guess.

**Not counted**:
A check card's guess whose check ended with an exit code that does not say whether it passed, such as `cargo test 2>&1 | tail`. No option is named the right one, and the weekly rates leave it out.

**Step ledger**:
The index of an agent session's tool calls, workers' included, kept as one row per call: its tool, file, command or pattern, turn, subagent, duration and result or exit code, and where in the event log its start and result lines are. It holds no output; a call's details are read from the log by those offsets.

**Evidence line**:
The line under a turn's Headline that states what the hooks and git recorded for the turn: the files it changed and each check's last result, such as `3 files · 2 checks passed`, with "+N merged" when a merge brought files in. It opens in place to each file with its line counts, the merged files, and each check with its status; a failed check opens its exit code and the tail of its output. It has no model in it, and an agent that reports no trees has none.

**Brief**:
What Daniel reads to resume an agent session he lost track of, folded under its feed (inside **Where we were** while that shows): slots, not prose. **Goal** is one or two lines a model writes from Daniel's prompts, again when a new prompt of his came; it is the only part a model writes. **Decisions** lists each question the agent asked with Daniel's reply, in order. **Open** is computed from facts, never written: the pending question, each check suite whose last run failed, files changed with no check run since, the latest turn's `open` and `ask` lines, and unpushed work or an unmerged PR; each drops out once it is settled. **Design so far** is at most five one-line design decisions with their reasons, which a model writes from the turns' `why` lines and the cards' summaries only; each cites the turn or card it came from, a click on the citation opens it in the rail, and code drops a line that cites nothing or something not in the session. A session with no `why` line and no card has none. A brief stored before the slots, as prose, shows as it is until the session's next Goal.

**Where we were**:
The banner the rail opens on when Daniel returns to a session, above the newest turn, which shows in full under it. It shows for an idle or ended session, and for one with unseen turns whose last turn ended 15 minutes ago or more. Three lines, all from facts: the last Headline with its age; what happened since he looked (the turns after the last one he saw, their files and check results, such as `2 turns · 4 files · checks passed`); and what is open, from the Brief's Open slot. The Brief folds inside it. It is decided when the session is selected and stays while he reads it, until he selects another session or sends a prompt.

**Workbench**:
The part of the control room where agent work happens: terminals, tasks and Workspaces. It works without the understanding layer.

**Task**:
One piece of agent work with its own git worktree and branch, and the terminals and agent sessions running in it.
_Avoid_: Ticket (an issue on the tracker; a ticket may become a task)

**Main checkout**:
A project's original working copy, outside any task. It appears in Workspaces and can host terminals and agent sessions, but it is not a task.

**Workspace**:
A folder where Daniel can start an agent session or a terminal: a project's main checkout or a task's worktree. The name tools like Conductor, herdr and Antigravity give the same unit. It is not the keymap's `Workspace` context, which is Zed's name for the window.
_Formerly_: Place
_Avoid_: Location

**Home**:
The view in the terminal column while no session is selected, such as after a delete: the sessions that need Daniel or are unseen, then the landed ones, then one row per project, in project order, to start a session there. Workspaces stays the home of tasks.
_Avoid_: Dashboard, board

**History**:
The view in the terminal column, like **Home**, that holds the ended sessions, newest first by their last event, opened from the session list's "History · N" link or the palette's "Show history". One search box filters by session name, project name and branch, as a case-insensitive substring. Clicking a row selects the session, with its rail as in the list; each row has a Delete, which opens the delete dialog. A session that starts again under its id is no longer ended and leaves History by itself. It adds no colour.
_Avoid_: Archive

**Workspaces**:
The rows under each project's sessions in its **Session list** group, the home of tasks: one row per workspace of that project, each on one line: its name, then its **upstream** and **main distance**, its uncommitted files and its number of sessions. A task's branch shows only when it differs from the task's name; the main checkout's row keeps its branch and, when its default branch is behind `origin`, carries **Pull main**'s button "Pull ↓N". Clicking a workspace offers a new session there and, for a task, Remove; tasks are created here too. The sessions themselves show only in their project's group.
_Formerly_: Places
_Avoid_: Board (an earlier plan for a view of its own, which Workspaces replaces)

**Landed**:
The state of a task whose work has shipped, so removing its worktree loses nothing: nothing uncommitted, nothing unpushed, its PR merged, and its issue closed when it has one, while none of its sessions is running or needs you. Once its PR is merged, its CI is not checked. Only tasks land, never the main checkout. The control room asks GitHub when a turn ends, when the window gains focus, or on a refresh, never on a timer. It shows as "Landed — safe to delete".
_Avoid_: Done (a turn's end), finished

**Upstream**:
A workspace's branch as `origin` has it. Its distance says whether Daniel's work is pushed: commits not pushed yet and commits not pulled yet. A branch never pushed has none, and shows "not pushed".
_Avoid_: Sync, remote (alone)

**Main distance**:
How far a workspace's branch is from `origin`'s default branch: the commits it adds, and the commits it is behind, which a rebase would take in. On the default branch itself, it is the upstream distance.
_Avoid_: Base (a card's starting point)

**Git line**:
The line in the top bar on the git state of the selected session's workspace, or the main checkout's with none: its workspace and branch, its upstream and main distance, its uncommitted files and any operation in progress. The branch shows once: for a task whose branch is its name, only the name does; the main checkout's branch always shows. The tokens that say nothing are hidden. Workspaces rows carry the same tokens.
_Avoid_: Git panel (a view of its own, which the studio does not have), status bar

**Plan usage**:
An agent's plan limits: its windows, such as the 5-hour and the weekly one, each with the share used and when it resets. A window of another length, such as the 30 days of Codex's free plan, shows as its own, named by its length. An agent with several limits, such as agy's model groups, has one group each, most used first, never merged into one and never mixed with another agent's. It is read without any credential, from what the agent itself reports. The top bar's glance shows the tightest window across every agent and group, with that agent's logo, or "Usage" when none has data; a click opens a popover with one tile per installed agent. Claude's is refreshed in the background; the other agents' are read only when the popover opens, and one that cannot be read says why instead of showing an old number. A window past its reset time is hidden.

**Pull main**:
A fast-forward of a project's main checkout's default branch to `origin`'s, from the "Pull ↓N" button on its row in **Workspaces** or the palette's action for every project. It never merges, rebases or stashes, and never touches a task's branch. With the default branch checked out it is refused, the button disabled with the reason as its title, when a session in the main checkout is running or needs Daniel, when local main has commits `origin` lacks, or when the incoming commits would overwrite a changed file; a project with nothing to pull is skipped.
_Avoid_: Sync (see **Upstream**)

**Terminal daemon**:
The background process that owns the workbench's terminals, so they outlive the app. The app connects to it over a socket.
_Avoid_: Server, multiplexer

**Pane**:
The window's view of one terminal of the terminal daemon. Up to two panes show at a time, when the terminal column is split; the others keep running hidden.
_Avoid_: Tab

**Terminal panel**:
A band under the terminal column that holds a plain login shell in the folder of the session shown, or the main checkout's with none, like the terminal panel of VS Code or Zed. One key shows and hides it. It sits under the pane, or under both panes of a split, at most 40 % of the column's height; it never spans the list or the rail. Each folder has one panel shell, kept by the **Terminal daemon**, so it survives restarts and keeps its scrollback when Daniel switches away and back.
_Avoid_: Bottom terminal, console

**Palette**:
The picker over every action of the window, each shown with its key, built from `keymap.json`, so it is also the list of keys. ⌘⇧P opens it. A new view or menu becomes an action in the palette, not a button in the top bar.
_Avoid_: Command menu, help

**Draft**:
What Daniel has typed into a session's prompt and not sent yet, as the studio follows his keys. The studio types into a terminal only once he has no draft there and has stopped typing for 3 seconds.

**Presence file**:
The empty file `present` in the studio's data folder, kept while the window is focused, so Claude Code's Remote Control skips a phone push Daniel would see on screen. Blur, quit and the app's start delete it, and the terminal daemon deletes it when the app's connection drops, unless it dropped the app for falling behind.

**Revival**:
Bringing back the terminals a terminal daemon ran after it died, from a crash, a reboot or an upgrade. A Claude terminal reopens its latest session with `claude --resume`; a shell that ran another agent with a resume command, such as `agy` or `codex`, reopens that agent's latest conversation in its folder; any other shell starts again in its folder. Each loses its screen, and a shell its state. Quitting a revived agent ends its terminal, as the agent, not a shell, is the terminal's program.
_Avoid_: Restore (nothing of the old process survives)

**Studio log**:
The studio's own diagnostics: its errors, warnings and panics, one line each with a local time and a level, in `~/Library/Logs/dev.falinks.studio/`. Not the event log, which holds the agents' hook events.

**Mismatch log**:
The lines in the **Studio log**, marked `mismatch`, of the Claude sessions whose hook state and OSC 7501 state disagree for more than a few seconds: one line each, with the time, the session and both states. They used to be a file of their own, `mismatch.log`, which is no longer written.

**Decision**:
A question, plan, permission or blocked command Daniel answered, with his answer and where he gave it: on the card or in the terminal. A session's decisions, newest first, are its decision history.
_Avoid_: Verdict (only a plan's or a permission's allow or deny)

**Blocked command**:
A Bash command an agent session tried in auto mode that auto mode blocked, held on a card until Daniel allows or declines it. Allowed, the agent runs it itself, once; declined, the block stands. While the agent waits on his click the session needs you; once it has moved on, its row still names the blocked command until he chooses.
_Avoid_: Denied command (a command Daniel declined)

**Identity**:
The name an agent session gets from its work: its issue, its PR and its worktree, such as "danielrispler/Falinks#480 · PR danielrispler/Falinks#490 · agent-a1b2". A name Daniel gives it replaces it.

**Session name**:
What the list and the rail call an agent session: the name Daniel gave it, else its prompt name, else its identity. The prompt name is a few words a writer call makes from Daniel's first prompt, so wake-ups never name a session; it keeps the identity's issue in front, such as "#37 · One event internal".
_Avoid_: Title

**Context share**:
How much of its context window an agent session uses, as a percentage. It is measured as Claude Code's status line measures it, from the session's status line snapshot: its tokens, its window size, and its own percent used and left. Without a snapshot, the control room shows the latest turn's tokens only.

**Context warning**:
A notice that an agent session's context share has passed the threshold Daniel set, 30% by default. It is raised once per crossing, and again only after the share has fallen back to the threshold or under, as after a compaction or a clear. It is a nudge, not a block: a neutral note with no sound, and the row's share shown at full strength while it stays past.
_Avoid_: Context alert, context limit

**On screen**:
An agent session whose terminal shows in a focused window, as the selected session or in a split's second pane. A finished turn on screen raises no note or sound. It is not the same as seen: a turn is seen only when the story rail shows it.

**Needs you**:
The state of an agent session that is blocked on Daniel: waiting for his permission, his click on a blocked command, his answer to a question tool, his approval of a plan, or an MCP request for input. A reply that ends with a question in its text blocks nothing, so it is your turn.
_Avoid_: Stuck (a session that is not progressing, which is a different state)

**Running**:
The state of an agent session in a turn: from Daniel's prompt until the agent ends its turn or the session ends.

**Your turn**:
The state of an agent session that finished its turn with nothing blocked on Daniel, even when its reply asks him something, or that started and has had no prompt yet. Unlike needs you, it does not interrupt him: reaching it off screen raises a note that fades and a soft sound, never colour.
_Avoid_: Done (the turn ended, not the task)

**Unseen**:
A your-turn agent session whose last ended turn Daniel has not seen; going idle keeps the mark. A turn is seen when the session is selected while the window is focused and the story rail shows it; a window left open in an empty room does not count, nor does focus mode, a closed overlay or the skills browser over the rail. The row shows a filled dot and a bold name, and the strip the filled dot; "Mark unseen" in the rail's menu brings it back until he selects the session again. It is the cue for "finished, not seen", and it survives a restart.
_Avoid_: Unread, new, done until seen

**Idle**:
The state of an agent session left at your turn with no events for 2 hours or more. Its next event brings it back.

**Ended**:
The state of an agent session from its own end event until it starts again under its id. The control room never guesses that a session is gone.

**Agent session**:
One worker runtime session running one agent on a task, such as a Claude Code session in a terminal. The unit the control room watches. Claude Code and agy sessions are followed fully, from their hooks and transcripts, each as far as its **agent adapter**'s **capabilities** reach; a session of another agent has its status only.
_Avoid_: Run (alone)

**Agent adapter**:
The part of the core that turns one agent's hook lines and transcript into the studio's events. Claude Code and agy each have one.

**Capability**:
What an agent adapter can report, such as a question that waits on Daniel, a permission mode, a subagent's last message or how full the session's context is. It decides what its sessions' views show: a part of the view hides when the adapter cannot report it, never by the agent's name.

**Subagent**:
An agent that an agent session, or another subagent, spawned to do part of its work, such as a Claude Code `Agent` call. It runs inside its session, not as a session of its own, and it is either a worker or a helper.
_Avoid_: Task (the workbench's unit of work)

**Worker**:
A subagent spawned with its own git worktree, or one that changes files. A helper becomes a worker at its first change. Its home is its own worktree: the one it was spawned isolated in, or the first worktree its commands name that is not its session's. Daniel follows a worker like a session, with its own cards and brief. A finished worker folds into its parent's line as a helper does, and its session's rail keeps it in the subagent history. A worker is not a task.

**Helper**:
A subagent that is not a worker, such as a review or a search. It shows as one line under the agent that spawned it.

**Wake-up**:
A prompt that Claude Code sends, not Daniel, to wake an agent when a subagent or a background command it started returns. It names the returning task's id in markup. It never counts as Daniel's prompt.

**Session list**:
The list of every live agent session, one row each, in one group per **Project**, in project order. A project's header shows its name and, even folded, its most urgent state (needs you over running), and on hover or focus a "+" that opens New session on that project; inside it the sessions are by state (needs you, running, your turn, then idle sessions collapsed last), then the project's **Workspaces** rows. A session belongs to the project whose workspace holds its folder, and its row carries a token for that workspace and branch (the main checkout's branch, or the task's name and branch), with no project badge. A session in a folder outside every project sits in a dimmed group after the projects, named after its repository's main checkout, else its folder, else `~`. Ended sessions are not in it: a "History · N" link at its end opens **History**. A row is one line, its name then its state's shape: a "Needs you" pill (the only colour), a spinner while running, a dot at your turn (filled, with a bold name, while **unseen**), a ring when idle or ended, a check mark instead when its task is **landed**. Only a running or needs-you row adds a second, muted line: its current step, or its question's first line. Running subagents nest under the agent that spawned them. When the window is narrow the list is a **strip** of the same shapes, each with a short label from the session's name, distinct between sessions; hovering it opens the list over the terminal. The top bar's needs-you chip counts the sessions that need Daniel, and an unseen chip beside it, not in colour, the unseen sessions. A finished subagent leaves the list and counts in its parent's "N done"; a failed one stays until Daniel opens it. Each row leads with its agent's logo, in one muted colour so "Needs you" stays the only colour, in every state and beside the running spinner, not instead of it; the same logo leads a row on Home and in History, and the session's header in the rail. A plain terminal shows a terminal glyph. The strip, a project's header and a subagent's row carry no logo. Hovering the header's logo shows the model running the session with its agent ("Claude · Opus 5.5", "agy · Flash high"), and for an agent that reports no context share, such as agy, a note saying so, so a missing share reads as expected. A **status-only row** is a session of another agent than Claude Code or agy, such as Codex, in a studio terminal whose program reports its status (OSC 7501): it shows running, needs you or your turn from that status, its program's logo (Codex's, or a neutral glyph for a program the studio does not know), its name from the program and its folder, and a "status only" mark instead of a second line, as it has no intention, Headline, card, Brief, estimate or decisions. It notifies as a Claude session does, and goes when its status clears or its terminal ends. Only Daniel's own actions change the session he looks at: a click or key on a row, a click on the list's empty space, which selects none and shows **Home**, or starting a session, which selects it once it reports its terminal.

**Story rail**:
The column on the window's left that tells the selected session's or subagent's story, drawn from recorded facts. Its header is one line: the state (needs you, running, your turn, idle), the phase, and a "⋯" menu for Rename, Close and Delete. Below it the newest turn is shown in full, now by state: while running, what Daniel asked and what has happened so far; at your turn or idle, its **Headline** and Body lines; when the session needs him, the open question first. A turn's block is its prompt in one line, its Headline and **Body**, the **Evidence line** under it, its **Quiz**, its cards, the subagents and checks it ran, and its full response, folded. Earlier turns are folded, newest first, each to one line that opens in place. When Daniel returns to a session the rail opens on the **Where we were** banner, with the **Brief** folded inside it; the decision history stays folded under the feed. The "Ask" mark sits on anchors (a file, a check, a subagent, a Headline) and opens the question about that fact. Below 1007 px the rail is an overlay: a spine at the window's left edge opens it, and it slides away when the pointer leaves.

**Focus mode**:
The window with the session list and the story rail folded, so the terminal takes the window. One key, rebindable in the keymap, enters it and the same key leaves it. Only two other things leave it: pressing the toast to jump to its session, and the next-needs-you key. Nothing else changes the layout, so the terminal's size never moves under Daniel's typing. While in it, a session that newly needs you shows a toast over the terminal's corner instead.
_Avoid_: Zen mode, full screen (the macOS window state)

**plotkeeper**:
The project and its repository (`danielrispler/plotkeeper`). The app it builds is named Studio.

**Falinks run**:
Several agent sessions coordinated by the Falinks engine on one repository.

**Data source**:
A place the control room learns about agent sessions from, such as a worker runtime, Git, GitHub or the Falinks engine. The control room works fully without the Falinks engine as a data source.

**Project**:
A repository added to the control room.

**Settings**:
Daniel's choices for the studio, kept in `settings.json` in its data folder, in Zed's format: zoom, the terminal's font size, the summary writer's model and effort, and the Doctor menu, whose entries are palette actions. ⌘, and a gear, the last item of the top bar, open the settings page, a dialog whose controls save each change by editing `settings.json` in place; it never folds into the bar's "⋯" menu. `studio::OpenSettingsFile` opens the raw file. "Settings files" means `settings.json` and `keymap.json` together: the studio reloads either when saved, and a file that does not parse is reported while its last good version stays in effect.
_Avoid_: Preferences, config

**Sounds**:
The `sounds` setting, on by default: a sound when a session newly needs Daniel, and when a turn ends off screen. A needs-you sounds with its toast, at once; unfocused, the sound plays with the notification. A session on screen makes none.
_Avoid_: Alerts, chimes

**Keymap**:
Daniel's shortcuts for the studio, kept in `keymap.json` beside the settings, in Zed's format: keystrokes bound to actions, by context.
_Avoid_: Keybindings, shortcuts file

**Explanation language**:
The human language of cards and questions: English for now, no longer Hebrew (danielrispler/Falinks#463), in plain words at Daniel's level. Code terms are in backticks.

**Prior art**:
How the benchmark tools already solve a standard shell or workspace problem, such as sidebar hierarchy, worktree navigation, branch switching or history. The benchmark tools are herdr, cmux, Workmux, AgentsRoom, T3 Code and Cursor's parallel background agents, with Zed second for editor chrome. The studio copies their proven pattern for these decisions and keeps its own design for the understanding layer. Prior-art notes live in `docs/studio/research/`.
_Avoid_: Competitor analysis (the point is proven patterns to adopt, not a market comparison)
