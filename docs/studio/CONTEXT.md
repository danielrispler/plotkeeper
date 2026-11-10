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

**Intention**:
The latest thing the agent wrote in its own words during a turn, such as why it is about to read an issue. It stays shown, with its age, until the agent writes again, so it covers the steps that follow it. While a turn runs, its intentions are the last line of each of its responses. When the turn ends, its final response's intention becomes that response's first line, which says what the agent did, so a footer a plugin adds is never the intention. A side question ("ask about this") gets these intentions as context.
_Avoid_: Reason (the card writer's account of a step)

**Turn summary**:
A few lines on a turn's main ideas, written by one cheap writer call when the turn ends, or when Daniel opens the turn if summaries are set to write on open. A turn gets one when its final response is about 150 words or more, or when it ends asking Daniel something; other turns show their response as it is. When it arrives, the turn-end intention switches once to its first line, except with summaries written on open.
_Avoid_: Card (on a change, with its quiz first), brief (the whole session's, kept for resuming)

**Learning section**:
The part under a turn summary that teaches: the turn's main idea and one multiple-choice question on it, collapsed until Daniel opens it. The same writer call writes it. A turn with a card has none; it points to the card instead, so the turn is not quizzed twice.
_Avoid_: Quiz (a card's question on a change)

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
A card's multiple-choice question on the change's logic or design, with the card writer's key, which grades Daniel's guess. On a check card the quiz asks whether the check will pass, and the check's result is the key.
_Avoid_: Question (an agent's question to Daniel)

**Check card**:
The card a check of the session's own agent gets at its start when it usually runs over 2 minutes, on the change since the last card, so Daniel can guess while he waits whether the check will pass.

**Guess**:
Daniel's answer to a card's quiz. The card writer's key grades it, and the card shows right or wrong, with the reasons, as soon as he answers. On a check card the check's result settles it instead, and the card shows right or wrong, with the reasons, once the check ends. It is never shown as a score.

**Settle**:
To grade the guess on a check card by the check's result, once the check ends. A tap after the result is no guess.

**Not counted**:
A check card's guess whose check ended with an exit code that does not say whether it passed, such as `cargo test 2>&1 | tail`. No option is named the right one, and the weekly rates leave it out.

**Brief**:
A rolling account of an agent session's or a worker's work so far, at the top of its view, so that Daniel can resume a session he lost track of. Each new card updates it.

**Workbench**:
The part of the control room where agent work happens: terminals, tasks and the board. It works without the understanding layer.

**Task**:
One piece of agent work with its own git worktree and branch, and the terminals and agent sessions running in it.
_Avoid_: Ticket (an issue on the tracker; a ticket may become a task)

**Main checkout**:
A project's original working copy, outside any task. It appears on the board and can host terminals and agent sessions, but it is not a task.

**Board**:
The workbench view with one row per task and one per main checkout, showing the state of their agent sessions.

**Place**:
A folder where Daniel can start an agent session or a terminal: a project's main checkout or a task's worktree.
_Avoid_: Location, workspace

**Terminal daemon**:
The background process that owns the workbench's terminals, so they outlive the app. The app connects to it over a socket.
_Avoid_: Server, multiplexer

**Pane**:
The window's view of one terminal of the terminal daemon. One pane shows at a time; the others keep running hidden.
_Avoid_: Tab

**Draft**:
What Daniel has typed into a session's prompt and not sent yet, as the studio follows his keys. The studio types into a terminal only once he has no draft there and has stopped typing for 3 seconds.

**Presence file**:
The empty file `present` in the studio's data folder, kept while the window is focused, so Claude Code's Remote Control skips a phone push Daniel would see on screen. Blur, quit and the app's start delete it, and the terminal daemon deletes it when the app's connection drops, unless it dropped the app for falling behind.

**Revival**:
Bringing back the terminals a terminal daemon ran after it died, from a crash, a reboot or an upgrade. A Claude terminal reopens its latest session with `claude --resume`; a shell starts again in its folder. Each loses its screen, and a shell its state.
_Avoid_: Restore (nothing of the old process survives)

**Decision**:
A question, plan or permission Daniel answered, with his answer and where he gave it: on the card or in the terminal. A session's decisions, newest first, are its decision history.
_Avoid_: Verdict (only a plan's or a permission's allow or deny)

**Identity**:
The name an agent session gets from its work: its issue, its PR and its worktree, such as "#480 · PR #490 · agent-a1b2". A name Daniel gives it replaces it.

**Context share**:
How much of its context window an agent session uses, as a percentage. It is measured as Claude Code's status line measures it, from the session's status line snapshot: its tokens, its window size, and its own percent used and left. Without a snapshot, the control room shows the latest turn's tokens only.

**Needs you**:
The state of an agent session that is blocked on Daniel: waiting for his permission, his answer to a question tool, his approval of a plan, or an MCP request for input. A reply that ends with a question in its text blocks nothing, so it is your turn.
_Avoid_: Stuck (a session that is not progressing, which is a different state)

**Running**:
The state of an agent session in a turn: from Daniel's prompt until the agent ends its turn or the session ends.

**Your turn**:
The state of an agent session that finished its turn with nothing blocked on Daniel, even when its reply asks him something, or that started and has had no prompt yet. Unlike needs you, it does not interrupt him.
_Avoid_: Done (the turn ended, not the task)

**Idle**:
The state of an agent session left at your turn with no events for 2 hours or more. Its next event brings it back.

**Ended**:
The state of an agent session from its own end event until it starts again under its id. The control room never guesses that a session is gone.

**Agent session**:
One worker runtime session running one agent on a task, such as a Claude Code session in a terminal. The unit the control room watches.
_Avoid_: Run (alone)

**Subagent**:
An agent that an agent session, or another subagent, spawned to do part of its work, such as a Claude Code `Agent` call. It runs inside its session, not as a session of its own, and it is either a worker or a helper.
_Avoid_: Task (the workbench's unit of work)

**Worker**:
A subagent spawned with its own git worktree, or one that changes files. A helper becomes a worker at its first change. Daniel follows a worker like a session, with its own cards and brief. A finished worker folds into its parent's line as a helper does, and its session's rail keeps it in the subagent history. A worker is not a task.

**Helper**:
A subagent that is not a worker, such as a review or a search. It shows as one line under the agent that spawned it.

**Wake-up**:
A prompt that Claude Code sends, not Daniel, to wake an agent when a subagent or a background command it started returns. It names the returning task's id in markup. It never counts as Daniel's prompt.

**Session list**:
The list of every agent session, one row each, sorted by state: needs you, running, your turn, then idle and ended sessions collapsed last. A row is one line, its name then its state's shape: a "Needs you" pill (the only colour), a spinner while running, a dot at your turn (filled, with a bold name, while **unseen**), a ring when idle or ended. Only a running or needs-you row adds a second, muted line: its current step, or its question's first line. Running subagents nest under the agent that spawned them. When the window is narrow the list is a **strip** of the same shapes, each with a short label from the session's name, distinct between sessions; hovering it opens the list over the terminal. The top bar's needs-you chip counts the sessions that need Daniel. A finished subagent leaves the list and counts in its parent's "N done"; a failed one stays until Daniel opens it. Only Daniel's own actions change the session he looks at: a click or key on a row, or starting a session, which selects it once it reports its terminal.

**Story rail**:
The column on the window's left that tells the selected session's or subagent's story: its state and step, its brief, cards and checks, and its open question docked at the bottom.

**Focus mode**:
The window with the session list and the story rail folded, so the terminal takes the window. One key, rebindable in the keymap, enters it and the same key leaves it; nothing else changes the layout, so the terminal's size never moves under Daniel's typing. While in it, a session that newly needs you shows a toast over the terminal's corner instead.
_Avoid_: Zen mode, full screen (the macOS window state)

**Falinks run**:
Several agent sessions coordinated by the Falinks engine on one repository.

**Data source**:
A place the control room learns about agent sessions from, such as a worker runtime, Git, GitHub or the Falinks engine. The control room works fully without the Falinks engine as a data source.

**Project**:
A repository added to the control room. Each project has one project mode.

**Project mode**:
A per-project setting that says whether Daniel wants to understand the project or only its result. It has two values: understand mode and results-only mode.
_Avoid_: Growth mode

**Understand mode**:
The default project mode. Waits and cards serve Daniel's understanding of this project.

**Results-only mode**:
The project mode for a project where Daniel cares only about the result. The wait is his own time, for practice or enrichment unrelated to the project. Approval requests and agent questions still reach him.

**Settings**:
Daniel's choices for the studio, kept in `settings.json` in its data folder, in Zed's format: zoom, the terminal's font size, the summary writer's model and effort, and the Doctor menu. The summary writer's model and effort and the Doctor menu are read but not used yet; later tickets wire them. "Settings files" means `settings.json` and `keymap.json` together: the studio reloads either when saved, and a file that does not parse is reported while its last good version stays in effect.
_Avoid_: Preferences, config

**Keymap**:
Daniel's shortcuts for the studio, kept in `keymap.json` beside the settings, in Zed's format: keystrokes bound to actions, by context.
_Avoid_: Keybindings, shortcuts file

**Explanation language**:
The human language of cards and questions: English for now, no longer Hebrew (#463), in plain words at Daniel's level. Code terms are in backticks.
