// One selection everywhere (danielrispler/Falinks#567): what the terminal shows for the selected session, which the rail
// shows too. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { paneView } from "../../dist/panes.js";

const session = (id, terminal, state = "running") => ({ id, terminal, state });

test("selecting a session shows its own terminal, and the next one switches it", () => {
  assert.deepEqual(paneView(session("a", "claude-a"), true), { terminal: "claude-a" });
  assert.deepEqual(paneView(session("b", "claude-b"), true), { terminal: "claude-b" });
});

test("a session started outside the studio shows the message and Open here, not a shell", () => {
  assert.deepEqual(paneView(session("a", null), false), { message: "Runs in another terminal. Open here resumes it." });
});

test("a session whose terminal is gone shows the same message", () => {
  assert.deepEqual(paneView(session("a", "claude-a"), false), { message: "Runs in another terminal. Open here resumes it." });
});

test("an ended session says it ended, not that it runs elsewhere", () => {
  assert.deepEqual(paneView(session("a", null, "ended"), false), { message: "Ended. Open here resumes it." });
});

test("the login shell shows only while nothing is selected", () => {
  assert.equal(paneView(null, false), "shell");
});
