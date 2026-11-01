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

**Quiz**:
A card's multiple-choice question on the change's logic or design, with the card writer's key, which grades Daniel's guess.
_Avoid_: Question (an agent's question to Daniel)

**Guess**:
Daniel's answer to a card's quiz. The card writer's key grades it, and the card shows right or wrong, with the reasons, as soon as he answers. It is never shown as a score.

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

**Needs you**:
The state of an agent session that is blocked on Daniel: waiting for his permission, his answer to a question, or his approval of a plan.
_Avoid_: Stuck (a session that is not progressing, which is a different state)

**Running**:
The state of an agent session in a turn: from Daniel's prompt until the agent ends its turn or the session ends.

**Your turn**:
The state of an agent session that finished its turn without asking Daniel anything, or that started and has had no prompt yet. Unlike needs you, it does not interrupt him.
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
A subagent spawned with its own git worktree, or one that changes files. A helper becomes a worker at its first change. Daniel follows a worker like a session, with its own cards and brief. A worker is not a task.

**Helper**:
A subagent that is not a worker, such as a review or a search. It shows as one line under the agent that spawned it.

**Wake-up**:
A prompt that Claude Code sends, not Daniel, to wake an agent when a subagent or a background command it started returns. It names the returning task's id in markup. It never counts as Daniel's prompt.

**Session list**:
The list of every agent session, one row each, grouped by state with needs you first and idle and ended sessions collapsed last. Each row says in one line what its session is doing, and subagents nest under the agent that spawned them. Only Daniel's click changes the session he looks at.

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

**Explanation language**:
The human language of cards and questions, such as Hebrew, in plain words at Daniel's level. Code terms stay in English.
