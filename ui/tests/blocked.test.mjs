// A blocked command's card (danielrispler/plotkeeper#170, #171): its text and buttons by status. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { blockedView } from "../../dist/blocked.js";

const card = (status = "blocked", agent = null) => ({
  call: "toolu_1",
  command: 'claude -p "summarise" > /tmp/out.txt',
  cwd: "/work/repo",
  reason: "Nested agent runs are blocked.",
  agent,
  at: 1,
  status,
});

test("a blocked command shows in full with its folder and rule, and Allow and Decline, also after its hook has returned", () => {
  assert.deepEqual(blockedView(card(), null, true), {
    state: "Blocked",
    command: 'claude -p "summarise" > /tmp/out.txt',
    folder: "/work/repo",
    reason: "Nested agent runs are blocked.",
    agent: null,
    buttons: ["Allow", "Decline"],
    detail: null,
    tail: null,
  });
});

test("a subagent's block names the subagent", () => {
  assert.equal(blockedView(card("blocked", "a-7aaaa"), "Reviewer", true).agent, "Reviewer");
});

test("with no request file there is nothing to answer", () => {
  assert.deepEqual(blockedView(card(), null, false).buttons, []);
});

test("an allowed command waits for the agent's retry with no buttons, and a declined one is closed", () => {
  const allowed = blockedView(card("allowed"), null, true);
  assert.equal(allowed.state, "Allowed");
  assert.deepEqual(allowed.buttons, []);
  assert.equal(blockedView(card("declined"), null, true), null);
});

test("the card has no edit: it offers only Allow and Decline", () => {
  const all = ["blocked", "allowed"].flatMap((s) => blockedView(card(s), null, true).buttons);
  assert.deepEqual(all.filter((b) => !["Allow", "Decline"].includes(b)), []);
});

test("a running command shows the time since its retry started", () => {
  const view = blockedView({ ...card("running"), run: "toolu_2", run_at: 10_000 }, null, false, 75_000);
  assert.equal(view.state, "Running");
  assert.equal(view.detail, "1m");
  assert.deepEqual(view.buttons, []);
});

test("a finished command shows its exit code and the tail of its output, as a failed check does", () => {
  const done = { ...card("finished"), run: "toolu_2", exit_code: 2 };
  const view = blockedView(done, null, false, 0, { exit_code: 2, tail: "line one\nboom" });
  assert.equal(view.state, "Finished");
  assert.equal(view.detail, "exit code 2");
  assert.equal(view.tail, "line one\nboom");
  assert.equal(blockedView(done, null, false, 0, null).tail, null);
  assert.equal(blockedView({ ...done, exit_code: 0 }, null, false, 0, null).detail, "exit code 0");
  assert.equal(blockedView({ ...done, exit_code: null }, null, false, 0, null).detail, "No exit code");
});

test("a command in the background reads so until its wake-up", () => {
  const view = blockedView({ ...card("running_in_background"), task: "bash-7" }, null, false, 0);
  assert.equal(view.state, "Running in background");
  assert.deepEqual(view.buttons, []);
});

test("a command blocked again offers Allow and Decline once more", () => {
  const view = blockedView(card("blocked_again"), null, true);
  assert.equal(view.state, "Blocked again");
  assert.deepEqual(view.buttons, ["Allow", "Decline"]);
});
