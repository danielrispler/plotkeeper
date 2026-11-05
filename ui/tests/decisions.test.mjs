// The decision history's words (#479): what Daniel answered and where. Run: `npm test` in apps/studio.
import { test } from "node:test";
import assert from "node:assert/strict";
import { answerText, kindText, sourceText } from "../../dist/decisions.js";

const at = 1793262013905;

test("a question's answer is its choices; a plan's and a permission's is their verdict", () => {
  const question = { tool: "AskUserQuestion", input: {}, answer: { "Which?": "Install", "Also?": "A, B" }, at, source: "terminal" };
  assert.equal(answerText(question), "Install · A, B");
  assert.equal(kindText(question), "Question");
  assert.equal(sourceText(question), "in the terminal");
  const plan = { tool: "ExitPlanMode", input: {}, answer: "deny", at, source: "card" };
  assert.equal(answerText(plan), "Rejected");
  assert.equal(sourceText(plan), "on the card");
  const permission = { tool: "Bash", input: {}, answer: "allow", at, source: "card" };
  assert.equal(answerText(permission), "Allowed");
  assert.equal(kindText(permission), "Permission · Bash");
});
