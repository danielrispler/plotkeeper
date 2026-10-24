# Studio

The shared language for the control room: one place to understand what coding agents are doing, during and after their work. The engine's language is in the root `CONTEXT.md`.

## Language

**Control room**:
The app this effort designs. Its form factor is not yet decided.
_Avoid_: Dashboard (one possible form, not the thing itself)

**Agent session**:
One worker runtime session running one agent on a task, such as a Claude Code session in a terminal. The unit the control room watches.
_Avoid_: Run (alone)

**Falinks run**:
Several agent sessions coordinated by the Falinks engine on one repository.

**Data source**:
A place the control room learns about agent sessions from, such as a worker runtime, Git, GitHub or the Falinks engine. The control room works fully without the Falinks engine as a data source.
