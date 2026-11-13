// Status-only rows (danielrispler/Falinks#560): another agent's session, from its terminal's OSC 7501 status, says so
// plainly and shows no step, intention or card. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { rowMark, secondLine, sessionName } from "../../dist/list.js";
import { railOf, storyOf } from "../../dist/story.js";

const codex = (state) => ({
  id: "t-7",
  name: null,
  folder_name: "other-project",
  identity: { issue: null, name: "codex · other-project" },
  state,
  line: "No step yet",
  questions: [],
  transcript: { context: null, intention: null },
  status_only: true,
  turns: [],
  checks: [],
  agents: [],
});

test("a status-only row shows its name and \"status only\", and no step, intention or card", () => {
  for (const state of ["running", "needs_you", "your_turn"]) {
    const row = codex(state);
    assert.equal(sessionName(row), "codex · other-project");
    assert.equal(rowMark(row), "status only");
    assert.equal(secondLine(row), null);
  }
  // A Claude session's mark is its context share, and its running row keeps its step.
  const claude = { ...codex("running"), status_only: false, transcript: { context: { tokens: 9000, share: 9 } } };
  assert.equal(rowMark(claude), "context 9%");
  assert.equal(secondLine(claude), "No step yet");
});

test("the rail shows a status-only row no intention or card", () => {
  const row = { ...codex("your_turn"), transcript: { context: null, intention: "Left over" } };
  const cards = [{ request: { id: "t-7@1" } }];
  assert.equal(storyOf(row, null), null);
  assert.deepEqual(railOf({ session: row, agent: null }, false, cards), { top: null, latest: null, blocks: [] });
  // A Claude session's rail keeps its intention and newest card.
  const claude = { ...row, status_only: false };
  assert.equal(storyOf(claude, null).intention, "Left over");
  assert.equal(railOf({ session: claude, agent: null }, false, cards).top, cards[0]);
});
