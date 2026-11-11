// Status-only rows (#560): another agent's session, from its terminal's OSC 7501 status, says so
// plainly and shows no step, intention or card. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { rowMark, secondLine, sessionName } from "../../dist/list.js";

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
