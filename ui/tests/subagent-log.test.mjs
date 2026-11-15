// A subagent's log shows in the pane (danielrispler/plotkeeper#54). Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { paneView } from "../../dist/panes.js";
import { logNote, logRows, stateLabel } from "../../dist/log.js";

const session = { terminal: "claude-a", state: "working" };

test("a selected subagent's pane is its log, the session's row keeps its terminal", () => {
  assert.deepEqual(paneView(session, true, "agent-1"), { log: "agent-1" });
  assert.deepEqual(paneView(session, false, "agent-1"), { log: "agent-1" });
  assert.deepEqual(paneView(session, true, null), { terminal: "claude-a" });
  assert.deepEqual(paneView(session, true), { terminal: "claude-a" });
  assert.equal(paneView(null, false, "agent-1"), "home");
});

const call = (id, tool = "Read", detail = null) => ({ kind: "call", id, tool, detail });
const result = (id, text = "ok", failed = false, cut = false) => ({ kind: "result", id, text, failed, cut });

test("each result pairs with its call by id; a call without one is running; a result without a call is dropped", () => {
  const rows = logRows([
    { kind: "prompt", text: "Find it." },
    { kind: "reply", text: "Looking." },
    call("c1", "Read", "a.rs"),
    call("c2", "Bash", "ls"),
    result("c2", "boom", true),
    result("gone"),
    result("c1", "fn a() {}"),
    call("c3"),
  ]);
  assert.deepEqual(rows, [
    { kind: "prompt", text: "Find it." },
    { kind: "reply", text: "Looking." },
    { kind: "call", id: "c1", tool: "Read", detail: "a.rs", result: { text: "fn a() {}", failed: false, cut: false } },
    { kind: "call", id: "c2", tool: "Bash", detail: "ls", result: { text: "boom", failed: true, cut: false } },
    { kind: "call", id: "c3", tool: "Read", detail: null, result: null },
  ]);
});

test("with no log the pane says Starting while the subagent runs, else that the log is not on disk", () => {
  assert.equal(logNote(null, "running"), "Starting…");
  assert.equal(logNote(null, "completed"), "The log isn't on disk.");
  assert.equal(logNote(null, "killed"), "The log isn't on disk.");
  assert.equal(logNote({ entries: [], offset: 0 }, "running"), null);
});

test("the header's state is the core's status", () => {
  assert.equal(stateLabel("running"), "running");
  assert.equal(stateLabel("completed"), "done");
  assert.equal(stateLabel("killed"), "killed");
});
