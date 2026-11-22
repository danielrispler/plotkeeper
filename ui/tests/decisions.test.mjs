// The decision history's words (danielrispler/Falinks#479): what Daniel answered and where. Run: `npm test`.
import { test } from "node:test";
import assert from "node:assert/strict";
import { answerText, kindText, riskText, sourceText } from "../../dist/decisions.js";

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

// The risk note beside a blocked command's answer (danielrispler/plotkeeper#299).
const bash = (answer) => ({ call: "toolu_1", agent: null, tool: "Bash", input: { command: "git push" }, answer, at, source: "card" });
const note = (arrived) => ({ call: "toolu_1", state: "arrived", level: "very_high", line: "Pushes to GitHub", ...(arrived === undefined ? {} : { at: arrived }) });

for (const answer of ["allow", "deny"]) {
  test(`a ${answer} shows the note that arrived before it, says so when it arrived after, and says when there is none`, () => {
    assert.equal(riskText(bash(answer), [note(at - 1)]), "Risk: Very high · Pushes to GitHub");
    assert.equal(riskText(bash(answer), [note(at + 1)]), "answered before the risk note arrived");
    assert.equal(riskText(bash(answer), []), "no risk note");
  });
}

test("a note that arrived in the answer's millisecond, or with no time, is not claimed seen; another call's note is not this one's", () => {
  assert.equal(riskText(bash("allow"), [note(at)]), "answered before the risk note arrived");
  assert.equal(riskText(bash("allow"), [note()]), "answered before the risk note arrived");
  assert.equal(riskText(bash("allow"), [{ ...note(at - 1), call: "toolu_2" }]), "no risk note");
});

test("a question, a plan and a permission of another tool show no note line", () => {
  const notes = [note(at - 1)];
  for (const tool of ["AskUserQuestion", "ExitPlanMode", "Edit"]) {
    assert.equal(riskText({ ...bash("allow"), tool }, notes), null);
  }
});
