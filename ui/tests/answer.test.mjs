// Answering from the card (#461, decision 16): the hook waits only while the waiting item's card
// shows. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { cardShown } from "../../dist/answer.js";
import { askKind } from "../../dist/types.js";

const ask = (tool_name, agent_id = null) => ({
  nonce: "n",
  hook_event_name: "PreToolUse",
  tool_name,
  tool_use_id: "t",
  tool_input: {},
  agent_id,
});

test("a question's card shows only in its asker's view; a plan's or a permission's in every view", () => {
  assert.equal(cardShown(ask("AskUserQuestion"), null), true);
  assert.equal(cardShown(ask("AskUserQuestion", "a-7"), null), false);
  assert.equal(cardShown(ask("AskUserQuestion", "a-7"), "a-7"), true);
  assert.equal(cardShown(ask("AskUserQuestion"), "a-7"), false);
  assert.equal(cardShown(ask("ExitPlanMode", "a-7"), null), true);
  assert.equal(cardShown(ask("Bash", "a-7"), null), true);
});

test("a call asks a question, a plan or a permission, by its tool", () => {
  assert.equal(askKind("AskUserQuestion"), "question");
  assert.equal(askKind("ExitPlanMode"), "plan");
  assert.equal(askKind("Bash"), "permission");
});
