# Studio

The shared language for the control room: the place Daniel does all agent work and keeps his understanding of it in step. The engine's language is in the root `CONTEXT.md`.

## Language

**Control room**:
The app this effort builds: the place Daniel does all agent work. It has three layers, each usable on its own: the understanding layer, the workbench and the Falinks engine.
_Avoid_: Dashboard (one view, not the thing itself)

**Understanding layer**:
The part of the control room that keeps Daniel's understanding in step with the agents' work: the turn summaries, the session Brief and the PR briefs. It works without the workbench, for any agent session.

**Wait**:
The time Daniel waits on an agent: from his prompt until the agent ends its turn, or until his next prompt if he interrupts it. Answering a permission, a question or a plan inside it does not start a new wait.

**Card**:
Retired as a turn's or a worker's explanation (danielrispler/plotkeeper#348): a **PR brief** is the explanation now. The cards stored before stay readable and none is shown. A blocked command's card is another thing, see **Blocked command**.
_Avoid_: Summary (read after the work, with no question first)

**Turn**:
An agent session's work on one prompt, Daniel's or a wake-up, from its first event to the agent's end of turn. It also ends when the next turn starts after Daniel interrupts it, or when the session ends. A prompt Daniel queues during a turn starts its own turn when the running one ends.
_Avoid_: Step (one tool call inside a turn)

**Turn files**:
The files a turn's agent and its workers changed, with their added and removed lines: every file that differs between the git trees the hook recorded at the turn's start and end, less the files another session's or turn's call names (unless a call of the turn itself names them too) and the files a merge brought in (those are counted as "+N merged" in the **Evidence line**). There is no external count: a hand edit of Daniel's during the turn shows as the turn's. A running turn's end tree is taken on demand. The ended turn's trees are kept under `refs/plotkeeper/turns/` so `git gc` does not prune its hunks. An agent that reports no trees (agy) has none, by its capability `turn_trees`.

**Intention**:
The latest thing the agent wrote in its own words during a turn, such as why it is about to read an issue. It stays shown, with its age, until the agent writes again, so it covers the steps that follow it. While a turn runs, its intentions are the last line of each of its responses. When the turn ends, its final response's intention becomes that response's first line, so a footer a plugin adds is never the intention. The rail shows it only while the session is Running (boxed) or Needs you (small, as "Why it asks"); at your turn the turn's Headline says what happened, and the summary's first line never replaces the intention. A side question ("ask about this") gets these intentions as context.
_Avoid_: Reason (the retired card writer's account of a step)

**Headline**:
One sentence, at most 16 words, on what a turn did, or what it waits on: an outcome, never a narration of the steps, and for a turn that ends by asking Daniel something, the ask. The turn writer is given the turn's recorded facts and writes it; code then holds it to them. A Headline that says fixed, passes or verified with no check of the turn that ended passed (or with one that ended failed) starts with "Agent says:". A Headline with a number or id the turn's material lacks is stripped and asked for once more; stripped twice, the rail shows the final response's first line. Until the summary arrives, the Headline is that line.
_Avoid_: Intention (the agent's own words while it works)

**Turn summary**:
A Headline and up to three typed Body lines, written by one cheap writer call when the turn ends, or when Daniel opens the turn if summaries are set to write on open. A turn gets one when its final response is about 150 words or more, or when it ends asking Daniel something; other turns show their response as it is. The writer is given the turn's recorded facts (its files with their line counts, hunks up to a cap, each check's command, result and retries) and told not to restate them. A Body line is `why`, `reported`, `open` or `ask`, at most 20 words; `open` and `ask` always show, and a `reported` line reads "Agent says". A line with a number or id the turn's material lacks is dropped; numbers in the turn's diff, line counts or check results, number words and digits in a known path always pass. A summary stored before the Headline is read with its first line as the Headline and the rest as `reported` lines; the writer is not run again on it.
_Avoid_: Card (retired), brief (the whole session's, kept for resuming)

**Transcript**:
Claude Code's own record of an agent session, which the control room reads for the intention, the turn's responses, the branch and the context size. When it cannot be read, the session's view falls back to its steps only.

**Phase**:
A recognisable stretch of a session's work on a ticket, such as orienting, designing, implementing, checking, or waiting on a PR's CI. A session moves through phases in a usual order.
_Avoid_: Stage, step

**Estimate**:
A range for how long the agent's remaining work will take, from the typical durations of the phases still ahead. It appears only when there is enough history for the current kind of work: 20 ended agent sessions of that kind. It is always marked as an estimate and leaves out the time that depends on Daniel; while the session needs him, it shows "depends on you" instead of a range.
_Avoid_: ETA (implies a promise)

**Kind of work**:
What an agent session's work is, named by its first prompt's slash command, else by its first skill, such as `implement` or `wayfinder`. An estimate compares a session only with sessions of the same kind.

**Base**:
The state of a worktree that the next change is measured from: its commit and its files when the previous change was taken, or when the prompt or worker began. A move of the commit that is not the agent's own work moves the base instead of adding to the change.
_Avoid_: Snapshot, checkpoint (the engine's term)

**Quiz**:
The multiple-choice question a card or a turn summary carried, with the writer's key. All of it is retired: a turn's own Quiz, the Retest a week after it, Home's learning slot and the hourly cap in danielrispler/plotkeeper#347, and the cards, their Quizzes, the check cards and the self-test in danielrispler/plotkeeper#348. The turn summary is a Headline and a Body, and the stored Quizzes, Retests and learning sections stay readable.
_Avoid_: Question (an agent's question to Daniel), Learning section (retired), Retest (retired)

**Turn writing**:
What the understanding layer writes when a turn ends or a worker returns: the turn summary, the session's name, and the Brief's Goal and Design so far, and no card. A session's pieces are written one at a time, in the order they were asked for.

**Check**:
A command of a project's check list, as run in one worktree. Its runs in a turn are one check, and its last run's result is the check's. The same command in two worktrees is two checks. [Checks](behaviour/checks.md) has the rules.

**Step ledger**:
The index of an agent session's tool calls, workers' included, kept as one row per call: its tool, file, command or pattern, turn, subagent, duration and result or exit code, and where in the event log its start and result lines are. It holds no output; a call's details are read from the log by those offsets.

**Evidence line**:
The line under a turn's Headline that states what the hooks and git recorded for the turn: the files it changed and each ended check's last result, such as `3 files · 2 checks passed`, with "+N merged" when a merge brought files in. It opens in place to each file with its line counts, the merged files, and each check with its status, naming its worktree when it is not the session's own; a failed check opens its exit code and the tail of its output. It has no model in it, and an agent that reports no trees has none.

**Brief**:
What Daniel reads to resume an agent session he lost track of, folded under its feed (inside **Where we were** while that shows): slots, not prose. **Goal** is one or two lines a model writes from Daniel's prompts, again when a new prompt of his came; it is the only part a model writes. **Decisions** lists each question the agent asked with Daniel's reply, in order. **Open** is computed from facts, never written: the pending question, each check whose last run failed (its worktree named when it is not the session's own), files changed with no check run since, the latest turn's `open` and `ask` lines, and unpushed work or an unmerged PR; each drops out once it is settled. **Design so far** is at most five one-line design decisions with their reasons, which a model writes from the turns' `why` lines and the summaries of the cards stored before they were retired; each cites the turn or card it came from, a click on the citation opens it in the rail, and code drops a line that cites nothing or something not in the session. A session with no `why` line and no card has none. A brief stored before the slots, as prose, shows as it is until the session's next Goal.

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

**Home** (retired):
The waiting room the page used to open on, in the centre column while no session was selected. It was retired with no setting to bring it back when the **Map view** became the page's base (spec danielrispler/plotkeeper#380, [ADR 0001](../adr/0001-retire-home.md)). What it held lives in the **Session list**: the second lines of needs-you, running and unseen rows, a header for every **Project**, the header's quiet dot and "Clear landed". Stored data that names Home, such as an old stop-rule line or a Quiz answer's `surface`, still reads.
_Avoid_: Home for the page as it is now (it is the Map view), start page, dashboard

**History**:
The view in the left panel over the **Map view** that holds the ended sessions, newest first by their last event, opened from the session list's "History · N" link or the palette's "Show history". One search box filters by session name, project name and branch, as a case-insensitive substring. Clicking a row selects the session, with its story as in the list; each row has a Delete, which opens the delete dialog. A session that starts again under its id is no longer ended and leaves History by itself. It adds no colour.
_Avoid_: Archive

**Workspaces** (retired):
Retired from the page (danielrispler/plotkeeper#335); its code is in `archive/workbench/` (#415). The rows under each project's sessions in its **Session list** group, the home of tasks: one row per workspace of that project, each on one line: its name, then its **upstream** and **main distance**, its uncommitted files and its number of sessions. A task's branch shows only when it differs from the task's name; the main checkout's row keeps its branch and, when its default branch is behind `origin`, carries **Pull main**'s button "Pull ↓N". Clicking a workspace offers a new session there and, for a task, Remove; tasks are created here too. The sessions themselves show only in their project's group.
_Formerly_: Places
_Avoid_: Board (an earlier plan for a view of its own, which Workspaces replaces)

**Landed**:
The state of a task whose work has shipped, so removing its worktree loses nothing: nothing uncommitted, nothing unpushed, its PR merged, and its issue closed when it has one, while none of its sessions is running or needs you. Once its PR is merged, its CI is not checked. Only tasks land, never the main checkout. The control room asks GitHub when a turn ends, when the window gains focus, or on a refresh, never on a timer. It shows as "Landed — safe to delete".
_Avoid_: Done (a turn's end), finished

**Upstream** (retired):
Retired with Workspaces (danielrispler/plotkeeper#335); its code is in `archive/workbench/` (#415). A workspace's branch as `origin` has it. Its distance says whether Daniel's work is pushed: commits not pushed yet and commits not pulled yet. A branch never pushed has none, and shows "not pushed".
_Avoid_: Sync, remote (alone)

**Main distance** (retired):
Retired with Workspaces (danielrispler/plotkeeper#335); its code is in `archive/workbench/` (#415). How far a workspace's branch is from `origin`'s default branch: the commits it adds, and the commits it is behind, which a rebase would take in. On the default branch itself, it is the upstream distance.
_Avoid_: Base (a retired card's starting point)

**Git line** (retired):
Retired from the page (danielrispler/plotkeeper#335); its code is in `archive/workbench/` (#415). The line in the top bar on the git state of the selected session's workspace, or the main checkout's with none: its workspace and branch, its upstream and main distance, its uncommitted files and any operation in progress. The branch shows once: for a task whose branch is its name, only the name does; the main checkout's branch always shows. The tokens that say nothing are hidden. Workspaces rows carry the same tokens.
_Avoid_: Git panel (a view of its own, which the studio does not have), status bar

**Plan usage** (retired):
Retired from the page (danielrispler/plotkeeper#335); its code is in `archive/workbench/` (#415). An agent's plan limits: its windows, such as the 5-hour and the weekly one, each with the share used and when it resets. A window of another length, such as the 30 days of Codex's free plan, shows as its own, named by its length. An agent with several limits, such as agy's model groups, has one group each, most used first, never merged into one and never mixed with another agent's. It is read without any credential, from what the agent itself reports. The page shows no plan usage yet: the top bar spec danielrispler/plotkeeper#380 describes (story 47) names it, but it is not built. When it is, the bar's glance shows the tightest window across every agent and group, with that agent's logo; one that cannot be read says why instead of showing an old number, and a window past its reset time is hidden.

**Pull main** (retired):
Retired with Workspaces (danielrispler/plotkeeper#335); its code is in `archive/workbench/` (#415). A fast-forward of a project's main checkout's default branch to `origin`'s, from the "Pull ↓N" button on its row in **Workspaces** or the palette's action for every project. It never merges, rebases or stashes, and never touches a task's branch. With the default branch checked out it is refused, the button disabled with the reason as its title, when a session in the main checkout is running or needs Daniel, when local main has commits `origin` lacks, or when the incoming commits would overwrite a changed file; a project with nothing to pull is skipped.
_Avoid_: Sync (see **Upstream**)

**Terminal daemon** (retired):
Retired with the Tauri app (#332); it is in `archive/terminal-daemon/` (#415). The background process that owns the workbench's terminals, so they outlive the app. The app connects to it over a socket.
_Avoid_: Server (the program that serves the page, below), multiplexer

**Server**:
The `server/` library and its `plotkeeper` program, which has no window: it holds the core state, the card store and the writer, follows the event log, writes the summaries and the PR briefs, and serves the learning layer as a page on 127.0.0.1 (one per data folder, behind a token) that Daniel opens in a browser tab. It owns no terminals: the **Terminal daemon** and the Tauri app's shell that also hosted the library are retired (#332, #415).
_Avoid_: Backend, daemon (the terminal daemon is another process)

**Wiring**:
plotkeeper putting its hooks into a harness, only with the user's consent. Each harness found is wired, declined or not yet asked: a start in a terminal asks only about the harnesses not yet asked, a start with no terminal never asks or wires, `plotkeeper install` wires every harness found, and `plotkeeper uninstall` unwires them all and forgets the answers.
_Avoid_: Setup, registering

**Pane** (retired):
Retired with the terminal (danielrispler/plotkeeper#335); its code is in `archive/` (#415). The window's view of one terminal of the terminal daemon. Up to two panes show at a time, when the terminal column is split; the others keep running hidden.
_Avoid_: Tab

**Terminal panel** (retired):
Retired with the terminal (danielrispler/plotkeeper#335); its code is in `archive/` (#415). A band under the terminal column that holds a plain login shell in the folder of the session shown, or the main checkout's with none, like the terminal panel of VS Code or Zed. One key shows and hides it. It sits under the pane, or under both panes of a split, at most 40 % of the column's height; it never spans the list or the rail. Each folder has one panel shell, kept by the **Terminal daemon**, so it survives restarts and keeps its scrollback when Daniel switches away and back.
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
The lines in the **Studio log**, marked `mismatch`, of the Claude sessions whose hook state and OSC 7501 state disagree for more than a few seconds: one line each, with the time, the session and both states; a new 7501 state starts a new line. A turn running by its hooks whose 7501 state stays `idle` was interrupted, and that line also ends it. They used to be a file of their own, `mismatch.log`, which is no longer written.

**Decision**:
A question, plan, permission or blocked command Daniel answered, with his answer and where he gave it: on the card or in the terminal. A session's decisions, newest first, are its decision history.
_Avoid_: Verdict (only a plan's or a permission's allow or deny)

**Blocked command**:
A Bash command an agent session tried in auto mode that auto mode blocked, held on a card until Daniel allows or declines it. Allowed, the agent runs it itself, once; declined, the block stands. While the agent waits on his click the session needs you; once it has moved on, its row still names the blocked command while its card has a button for him (Allow and Decline, or Ask again), and no longer once he has allowed it. The card is docked, in orange, only while it has a button; otherwise it sits in the rail's feed.
_Avoid_: Denied command (a command Daniel declined)

**Risk note**:
The card writer's one line on what a blocked command would do if allowed, led by its risk level (None, Low, Medium, High, Very high or Not sure); written from the command and its folder alone, never the agent's words; advice, the choice stays Daniel's. The classifier's label keeps its wording, "the rule that blocked it".
_Avoid_: Verdict, rating (a risk note judges nothing: it describes)

**Identity**:
The name an agent session gets from its work: its issue, its PR and its worktree, such as "danielrispler/Falinks#480 · PR danielrispler/Falinks#490 · agent-a1b2". A name Daniel gives it replaces it.

**Session name**:
What the list and the rail call an agent session: the name Daniel gave it, else its prompt name, else its identity. The prompt name is a few words a writer call makes from Daniel's first prompt, so wake-ups never name a session; when he cancels that prompt's turn before any call, his next prompt names it again; it keeps the identity's issue in front, such as "#37 · One event internal".
_Avoid_: Title

**Context size**:
How many tokens an agent session's context holds: the input tokens of its latest message, read from its transcript, shown in its row as "85k". The control room knows no window size, so it shows no share of it.

**Context share** (retired):
Retired with the status line snapshots that measured it (#416); its code is in `archive/`. How much of its context window an agent session used, as a percentage, from the session's status line snapshot: its tokens, its window size, and Claude Code's own percent used and left. Only sessions the desktop app started saved a snapshot.

**Context warning** (retired):
Retired with the context share (#416); its code is in `archive/ui/`. A neutral note when a session's context share passed a threshold Daniel set, 30% by default, once per crossing.

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
One worker runtime session running one agent on a task, such as a Claude Code session in a terminal. The unit the control room watches. Claude Code and agy sessions are followed fully, from their hooks and transcripts; Codex, Qwen Code, Cursor and Copilot sessions from their hooks only (sessions, turns, prompts and calls; no transcript is read, and turn trees are recorded for Codex and Cursor only); each as far as its **agent adapter**'s **capabilities** reach. A session of another agent has its status only.
_Avoid_: Run (alone)

**Agent adapter**:
The part of the core that turns one agent's hook lines and transcript into the studio's events. Claude Code, agy, Codex, Qwen Code, Copilot and Cursor each have one.

**Capability**:
What an agent adapter can report, such as a question that waits on Daniel, a permission mode, a subagent's last message or how full the session's context is. It decides what its sessions' views show: a part of the view hides when the adapter cannot report it, never by the agent's name.

**Subagent**:
An agent that an agent session, or another subagent, spawned to do part of its work, such as a Claude Code `Agent` call. It runs inside its session, not as a session of its own, and it is either a worker or a helper.
_Avoid_: Task (the workbench's unit of work)

**Worker**:
A subagent spawned with its own git worktree, or one that changes files. A helper becomes a worker at its first change. Its home is its own worktree: the one it was spawned isolated in, or the first worktree its commands name that is not its session's. Daniel follows a worker like a session, with its own brief. A finished worker folds into its parent's line as a helper does, and its session's rail keeps it in the subagent history. A worker is not a task.

**Helper**:
A subagent that is not a worker, such as a review or a search. It shows as one line under the agent that spawned it.

**Wake-up**:
A prompt that Claude Code sends, not Daniel, to wake an agent when a subagent or a background command it started returns. It names the returning task's id in markup. It never counts as Daniel's prompt.

**Session list**:
The list at the page's right, beside the **Map view**: every live agent session, one row each, in one group per **Project**. Every saved project keeps its header, even with no live session: those groups come after the ones with sessions, as their header only. A header is the project switcher: a click shows that project's map (on the project already shown it folds the group), and, for a project with no live session, its "⋯" holds Remove project. It shows the project's name, even folded its most urgent state (needs you over running), and the project's quiet dot, drawn as the map's mist, which lights for fog on a Core part, a broken law or a PR brief waiting, its hover naming which. Inside it the sessions are by state (needs you, running, your turn, then idle sessions collapsed last), then the project's **Workspaces** rows. A session belongs to the project whose workspace holds its folder, and its row carries a token for that workspace and branch (the main checkout's branch, or the task's name and branch), with no project badge. A session in a folder outside every project sits in a dimmed group after the projects, named after its repository's main checkout, else its folder, else `~`. Ended sessions are not in it: a "History · N" link at its end opens **History**. A row is one line, its name then its state's shape: a "Needs you" pill (the only colour), a spinner while running, a dot at your turn (filled, with a bold name, while **unseen**), a ring when idle or ended, a check mark instead when its task is **landed**. Only a needs-you, running or unseen row adds a second line, muted, one line cut with an ellipsis: its question's first line, its **Intention** (else its current step), or its last turn's **Headline**. Hovering a row lights its session's agent chips on the map. A quiet "Clear landed" at the list's end, shown only while a landed session shows, hides every landed row with an Undo for a few seconds, then deletes them. Running subagents nest under the agent that spawned them. When the window is narrow the list is a **strip** of the same shapes, each with a short label from the session's name, distinct between sessions; hovering it opens the list over the map. The top bar's needs-you chip counts the sessions that need Daniel; the top bar has no unseen chip, since the list's unseen rows say it. A finished subagent leaves the list and counts in its parent's "N done"; a failed one stays until Daniel opens it. Each row leads with its agent's logo, in one muted colour so "Needs you" stays the only colour, in every state and beside the running spinner, not instead of it; the same logo leads a row in History, and the session's header in its story. A plain terminal shows a terminal glyph. The strip, a project's header and a subagent's row carry no logo. Hovering the header's logo shows the model running the session with its agent ("Claude · Opus 5.5", "agy · Flash high"), and for an agent that reports no context size, such as agy, a note saying so, so a missing size reads as expected. A **status-only row** is a session of an agent plotkeeper does not record, such as Codex before `plotkeeper install` wired its hooks, in a studio terminal whose program reports its status (OSC 7501): it shows running, needs you or your turn from that status, its program's logo (Codex's, or a neutral glyph for a program the studio does not know), its name from the program and its folder, and a "status only" mark instead of a second line, as it has no intention, Headline, card, Brief, estimate or decisions. It notifies as a Claude session does, and goes when its status clears or its terminal ends. Only Daniel's own actions change the session he looks at: a click or key on a row, which opens its story in the left panel and shows its project's map, or a click on the list's empty space, a second click on the same row or Esc, which close the panel and leave the bare map.

**Story rail**:
The column on the window's left that tells the selected session's or subagent's story, drawn from recorded facts. Its header is one line: the state (needs you, running, your turn, idle), the phase, and a "⋯" menu for Rename, Close and Delete. Below it the newest turn is shown in full, now by state: while running, what Daniel asked and what has happened so far; at your turn or idle, its **Headline** and Body lines; when the session needs him, the open question first. A turn's block is its prompt in one line, its Headline and **Body**, the **Evidence line** under it, its cards, the subagents and checks it ran, and its full response, folded. Earlier turns are folded, newest first, each to one line that opens in place. When Daniel returns to a session the rail opens on the **Where we were** banner, with the **Brief** folded inside it; the decision history stays folded under the feed. The "Ask" mark sits on anchors (a file, a check, a subagent, a Headline) and opens the question about that fact. Below 1007 px the rail is an overlay: a spine at the window's left edge opens it, and it slides away when the pointer leaves.

**Focus mode**:
The window with the session list and the story rail folded, so the terminal takes the window. One key, rebindable in the keymap, enters it and the same key leaves it. Only two other things leave it: pressing the toast to jump to its session, and the next-needs-you key. Nothing else changes the layout, so the terminal's size never moves under Daniel's typing. While in it, a session that newly needs you shows a toast over the terminal's corner instead.
_Avoid_: Zen mode, full screen (the macOS window state)

**Map view**:
The page's base: one project's system map, filling the page under the top bar beside the **Session list**. The page opens on the project last viewed, else the first; the list's project headers switch it, and selecting a session shows its project's map. Everything else opens in one left panel that floats over it (a session's story, History, a turn's diff, widened, a worker's log, or Settings), as the part's sheet does on the right; neither darkens the map, and only a PR brief dims it. Dragging its background pans it and ⌘+wheel or a pinch zooms it, ⌘0 fits it, and each project's view is kept. Esc peels, in order: an open drawer, a scrubbed commit (back to now), the part's sheet, then the left panel; on the bare map it does nothing. `studio::ShowMap`, "back to the map", closes the panel (a keymap's old `studio::GoHome` is read as it). A fresh install shows one calm line in its place, a project with no map the prompt to draft one, and a project whose folder is gone says so with Remove project. Its **Ready for review** tray lists the PRs of the project that are open and not drafts.
_Avoid_: Architecture view, graph view

**Left panel**:
The one card that floats over the **Map view**'s left side (#385), in the part sheet's glass, never darkening the map: it holds a session's story, **History**, a turn's diff (widened to at most 780 px), a worker's log, or Settings, one at a time. Closing a panel that shows a session (its story, its diff or a worker's log) leaves the bare map with no session selected; ⌘2 closes and opens the selected session's story and keeps it selected. Below 1007 px only one of it and the part's sheet is open. Esc closes it last of the map's layers.
_Avoid_: Sidebar, centre column (retired with Home, #386)

**PR brief**:
The stepped plain-language explanation of one pull request on the system map, written when the PR becomes **Ready for review** and kept waiting. A PR that would fog at most two parts is told as a single story; a bigger one in two to four **chapters**. It ends with at most one prediction (the check) when the change alters a flow or adds a tagged test. Distinct from **Brief** (the session-resume brief in the story rail).
_Avoid_: Brief (alone — that is the session-resume brief), lesson

**Chapter**:
One behaviour of a chaptered **PR brief**, the kind Daniel would name ("Fog", "Broken laws"), told in three or four steps. It lights every part its behaviour crosses; finishing it clears their fog.
_Avoid_: Section, step (a step is one line of a chapter)

**Ready for review**:
A pull request that is open and not a draft. The shell learns it from `gh pr view` at a turn's end, and at window focus and the Map view's opening within one per-repository cooldown; never on a timer. The Map view's "Ready for review" tray lists these PRs.
_Avoid_: Open PR (a draft is open too), review requested

**Ratify**:
To accept or overturn a change the agent made to the system map or a law, or a decision it declared, offered in the **PR brief**. A map change nobody has decided on is **not yet ratified**; it is in effect all the same, since ratifying never blocks. Overturning copies a prompt that quotes the item to the clipboard, for Daniel to paste into his agent.
_Avoid_: Approve (that is the PR review), sign off

**Part catch-up**:
One story, in the **PR brief**'s reader and shape (a single story of three or four steps), of everything that landed in one fogged part since Daniel last caught up: every PR and direct push in its **Fog**, ordered by behaviour. A fogged part's sheet offers it as the main button, before "I know this"; reading it to the end clears that part's fog and completes no PR brief.
_Avoid_: Catch-up (alone — the setting that writes briefs at start is the brief backfill), summary

**Refresher**:
A part as the **System map** has it now, offered in the sheet of a part with no **Fog**: its flows, in order, and what it is linked with, drawn from the map itself and never written by a model. It is for a part Daniel once knew and has not looked at in a while.
_Avoid_: Recap, summary, catch-up (a **Part catch-up** tells what changed)

**Staleness band**:
A mild band along the foot of a Core part Daniel has not engaged with in a while: one band after 3 weeks, two after 6, three after 12. Staleness is time since Daniel last engaged with the part, not time since its code changed; engaging with it clears the band.
_Avoid_: Stale fog, age (**Fog** is what changed unseen; a band is what went unvisited)

**Gist**:
One plain sentence the writer adds to each chapter of a **PR brief** that lights a Core part, and to each **Part catch-up**: the story's whole point, as Daniel could say it back. It is checked by code and by the **Claim check** like every other item, and shown only after **Explain-back**.
_Avoid_: Summary, tl;dr, takeaway

**Explain-back**:
The optional one-line field in the end-of-story bar of a Core part's story: Daniel says the story back, then sees its **Gist** under his line and marks himself got it, partly or missed. It measures his understanding, as the rating measures the explanation; neither is ever shown as a number.
_Avoid_: Quiz, test, recall score

**Claim check**:
The second sandboxed call that checks each claim of a **PR brief** against the same facts the writer had. An item still flagged after one rewrite carries the mark **not confirmed from the facts**.
_Avoid_: Fact check, verification

**Background run**:
A headless agent run with scoped tools that plotkeeper starts on Daniel's click, inside Claude Code's OS sandbox, queued one at a time per project and cancellable from the map. It is not a session and has no terminal. [Background runs](behaviour/background-runs.md) has the rules.
_Avoid_: Session, job, a check sent to the background (a check an agent backgrounds inside its own session)

**Micro-world**:
A small interactive page, built by a **Background run** from the project's real code, that lets Daniel play with one mechanism: a flow of a part's **Refresher**, or the parts a step of a **PR brief** lights. plotkeeper checks the page before keeping it and shows it walled off from everything else it serves.
_Avoid_: Demo, simulation, explainer (an explainer is the dsh-visual page of a diff)

**Guard with a law**:
The offer at the end of the **PR brief** of a bug fix that neither adds nor changes a test tied to a **Law**: back a law on trust of a part the PR touched, or add a new law, so a check keeps the bug from coming back. Either starts a back a law **Background run**, and Daniel sees its test only once plotkeeper has proved it fails when the law is broken.
_Avoid_: Regression test (alone), incident drill (deferred)

**Pre-flight**:
Before work starts, the parts of the **System map** it will likely cross, framed on the map as *predicted*: from an open issue or one or two lines (named by the no-tools writer) or picked by hand. A **PR brief** of a PR that closes the pre-flighted issue, or a ticket under it, says what was expected against what the PR touched. [The system map](behaviour/map.md) has the rules.
_Avoid_: Plan, forecast, estimate (an **Estimate** is of a task's size)

**Seen**:
In the viewport (intersecting visibly, not merely built in the DOM), in a visible and focused window, with the view that holds it open. Used for PR brief rows and stop-rule counts.
_Avoid_: Shown (when it meant DOM-built), rendered

**System map**:
A project's own plain-language description of its system, kept in its repository as `system-map.json`: its parts, outside actors, links, ignored paths, laws and, optionally, flows. [The system map](behaviour/map.md) has the rules.
_Avoid_: Architecture diagram, model

**Part**:
One of 6 to 15 named pieces of a system on its system map, owning a set of file paths. Its id survives a rename.
_Avoid_: Module, component, crate (a part may hold several, or sit inside one)

**Outside actor**:
A named thing the system talks to that owns no files in its repository, such as the agents, the writer or GitHub.

**Link**:
A declared connection on the system map between two parts, or between a part and an outside actor.
_Avoid_: Edge (a computed edge)

**Computed edge**:
A dependency between two parts that an extractor read from the code. It is a hint, never drawn; the links are what the map shows.

**Flow**:
A named path of behaviour across the system, as ordered hops along links, each with one plain sentence.

**Law**:
Something that must stay true, in one plain sentence, attached to a part, a link or a flow. It is backed by a check when a tagged test stands behind it, and on trust when none does.
_Avoid_: Invariant, rule

**CI on the default branch**:
The latest finished run, on a project's default branch, of the CI job that runs its law tests. The studio asks GitHub for it, never on a timer. It is not a check: a check is run in a worktree.
_Avoid_: Check (a command run in one worktree), main's build

**Broken law**:
A law backed by a check whose tagged test failed in the latest CI on the default branch, read from the run's failing tests and their files. It marks the law's part, without naming the law, and is the only loud colour beside Needs you. A red run whose law tests failed in no tagged file, or never ran, is no broken law.

**Tier**:
How much of a part Daniel wants in his head: Core, Interface-only or Black box. The system map holds each part's default; his own choice is his state, outside the repository.

**Fog**:
The mark on a part when changes landed on `origin`'s default branch that Daniel has not caught up on: a cool mist, never a number. It is thin when only the part's files changed, and thick when a law on it, a link at its edge or a flow through it changed. A Black box part never fogs, and an Interface-only part fogs only thick.
_Avoid_: Stale, unread, debt

**plotkeeper**:
The project and its repository (`danielrispler/plotkeeper`). The app it builds is named Studio; the program that serves its page without the app is `plotkeeper` (see **Server**).

**Falinks run**:
Several agent sessions coordinated by the Falinks engine on one repository.

**Data source**:
A place the control room learns about agent sessions from, such as a worker runtime, Git, GitHub or the Falinks engine. The control room works fully without the Falinks engine as a data source.

**Project**:
A repository on plotkeeper's saved project list: added the first time a session runs in it (its **Main checkout**), kept after its sessions end or are deleted, and removed only by Remove project. A project whose folder was moved or deleted is gone: it stays listed, says so, and can still be removed.
_Avoid_: Workspace (a checkout inside a project), repository list (it is plotkeeper's own list)

**Engaged day**:
An active day with at least one entered action on that project's map: a part's sheet opened, a PR brief or part catch-up entered, an agent chip clicked, a law or proof opened, or a scrub. A day the map was only on screen is not one. The Map view's stop rule counts them: it passes at 4 or more in the review window (14 active days and 20 merged PRs).
_Avoid_: Map opened (opening is no longer engagement, since the map is the page)

**Settings**:
Daniel's choices for the studio, kept in `settings.json` in its data folder, in Zed's format: zoom, the terminal's font size, the summary writer with its model and effort (`summaries`), the OpenAI-compatible API's base URL (`summaries.base_url`), the brief backfill's window (`briefBackfill`) and the PR brief writer's model (`prBriefs.model`). The page sets only the writer's: its writer section (`ui/writers.ts`, in the Settings panel the top bar's gear opens) saves the writer, model and effort once its Test passes, and the base URL; it also takes the Anthropic and OpenAI-compatible API keys, which are kept in the macOS Keychain, never in `settings.json`. Daniel edits the rest in the files, in his editor. "Settings files" means `settings.json` and `keymap.json` together: the studio reloads either when saved, and a file that does not parse is reported while its last good version stays in effect.
_Avoid_: Preferences, config

**Sounds**:
The `sounds` setting, on by default: a sound when a session newly needs Daniel, and when a turn ends off screen. A needs-you sounds with its toast, at once; unfocused, the sound plays with the notification. A session on screen makes none.
_Avoid_: Alerts, chimes

**Keymap**:
Daniel's shortcuts for the studio, kept in `keymap.json` beside the settings, in Zed's format: keystrokes bound to actions, by context.
_Avoid_: Keybindings, shortcuts file

**Explanation language**:
The human language of briefs and questions: English for now, no longer Hebrew (danielrispler/Falinks#463), in plain words at Daniel's level. Code terms are in backticks.

**Prior art**:
How the benchmark tools already solve a standard shell or workspace problem, such as sidebar hierarchy, worktree navigation, branch switching or history. The benchmark tools are herdr, cmux, Workmux, AgentsRoom, T3 Code and Cursor's parallel background agents, with Zed second for editor chrome. The studio copies their proven pattern for these decisions and keeps its own design for the understanding layer. Prior-art notes live in `docs/studio/research/`.
_Avoid_: Competitor analysis (the point is proven patterns to adopt, not a market comparison)
