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
