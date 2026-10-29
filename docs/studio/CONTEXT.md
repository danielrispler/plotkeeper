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
A short explanation of one step an agent took, in the explanation language. It opens with a question and shows the answer when Daniel asks. A wait has at most one card.
_Avoid_: Summary (read after the work, with no question first)

**Guess**:
Daniel's yes-or-no answer, before a check's result, to whether the check will pass. It is recorded but never shown as a score.

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

**Your turn**:
The state of an agent session that finished its turn without asking Daniel anything. Unlike needs you, it does not interrupt him.
_Avoid_: Done (the turn ended, not the task)

**Agent session**:
One worker runtime session running one agent on a task, such as a Claude Code session in a terminal. The unit the control room watches.
_Avoid_: Run (alone)

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
