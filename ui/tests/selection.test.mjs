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
  // A terminal that is not running shows the same message.
  assert.deepEqual(paneView(session("a", "claude-a"), false), { message: "Runs in another terminal. Open here resumes it." });
});

test("an ended session says it ended, not that it runs elsewhere", () => {
  assert.deepEqual(paneView(session("a", null, "ended"), false), { message: "Ended. Open here resumes it." });
});

test("Home shows while nothing is selected, not the login shell", () => {
  assert.equal(paneView(null, false), "home");
});

test("Open here shows the live terminal that already runs the conversation, never a second claude (#101)", async () => {
  const { runningTerminal } = await import("../../dist/panes.js");
  const term = (id, argv = [], exited = null) => ({ id, argv, cwd: "/tmp", exited });
  const session = { terminal: "claude-a", resume_id: "conv-1" };
  // The terminal the row links to.
  assert.equal(runningTerminal(session, [term("claude-b"), term("claude-a")]).id, "claude-a");
  // A terminal that resumed the conversation, while the row links to none.
  const resumed = term("claude-c", ["claude", "--resume", "conv-1"]);
  assert.equal(runningTerminal({ terminal: null, resume_id: "conv-1" }, [term("claude-b"), resumed]).id, "claude-c");
  // Nothing runs it: an exited terminal does not count.
  assert.equal(runningTerminal(session, [term("claude-a", [], "code 0"), term("claude-b", ["claude", "--resume", "other"])]), undefined);
});

test("the Home key flips between a session and Home (#239)", async () => {
  const { homeToggle } = await import("../../dist/selection.js");
  const listed = [{ id: "a" }, { id: "b" }];
  // From a session it remembers that session and shows Home.
  assert.deepEqual(homeToggle("a", null, listed), { show: "a" });
  // On Home it selects the remembered session.
  assert.deepEqual(homeToggle(null, "a", listed), { select: "a" });
});

test("on Home with the remembered session gone, the Home key does nothing (#239)", async () => {
  const { homeToggle } = await import("../../dist/selection.js");
  assert.equal(homeToggle(null, "gone", [{ id: "b" }]), null);
  assert.equal(homeToggle(null, null, [{ id: "b" }]), null);
});

const file = (path) => ({ root: "/r", path, from: null, added: 1, removed: 0, agent: null });
const turn = (id, started_at, files) => ({ id, started_at, ended_at: started_at + 5, prompt: "", message: null, checks: [], files: files && { files, merged: 0 } });

test("the link from a Retest names the turn of its first Quiz and that turn's first hunk (#243)", async () => {
  const { codeTarget } = await import("../../dist/selection.js");
  const turns = [turn("t0", 100, [file("a.rs")]), turn("t1", 200, [file("b.rs"), file("c.rs")])];
  assert.deepEqual(codeTarget(turns, "t1"), { turn: 1, file: file("b.rs") });
  // A card's Quiz names the turn that ran when the card was written.
  assert.deepEqual(codeTarget(turns, "card@150"), { turn: 0, file: file("a.rs") });
});

test("a turn with no hunk opens the turn alone, and an unknown turn opens nothing (#243)", async () => {
  const { codeTarget } = await import("../../dist/selection.js");
  const turns = [turn("t0", 100, null), turn("t1", 200, [])];
  assert.deepEqual(codeTarget(turns, "t0"), { turn: 0, file: null });
  assert.deepEqual(codeTarget(turns, "t1"), { turn: 1, file: null });
  assert.equal(codeTarget(turns, "gone"), null);
});

test("the link opens the hunk the summary names, else the turn's first file, and a worker card's turn when the core placed it (#243)", async () => {
  const { codeTarget } = await import("../../dist/selection.js");
  const turns = [turn("t0", 100, [file("a.rs")]), turn("t1", 200, [file("b.rs"), file("c.rs")])];
  assert.deepEqual(codeTarget(turns, "t1", "--- c.rs\n@@ -1 +1 @@\n+x\n"), { turn: 1, file: file("c.rs") });
  assert.deepEqual(codeTarget(turns, "t1", "--- gone.rs\n+x\n"), { turn: 1, file: file("b.rs") });
  assert.deepEqual(codeTarget(turns, "w@150", undefined, 1), { turn: 1, file: file("b.rs") });
});

test("the session left is remembered on every way to Home, and kept while a session is selected (#226)", async () => {
  const { leftSession } = await import("../../dist/selection.js");
  // Home key, a click on the list's empty space or a Delete all end the selection on none.
  assert.equal(leftSession("a", null, null), "a");
  // The last session left wins over a stale one.
  assert.equal(leftSession("b", null, "a"), "b");
  // Moving between sessions, or staying on Home, keeps what was remembered.
  assert.equal(leftSession("a", "b", "x"), "x");
  assert.equal(leftSession(null, null, "x"), "x");
  assert.equal(leftSession(null, "a", "x"), "x");
});
